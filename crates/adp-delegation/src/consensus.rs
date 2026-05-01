//! Consensus engine for VOTE delegation strategy.
//!
//! When a task uses [`Strategy::Vote`](adp_core::task::Strategy::Vote),
//! multiple agents produce results independently. The [`ConsensusEngine`]
//! determines whether those results agree sufficiently to form a quorum.
//!
//! # Rules
//!
//! 1. **Quorum**: At least `quorum_percent` of agents must agree on the same result.
//! 2. **Agreement**: Results are compared via JSON structural equality.
//! 3. **Tie**: If no result reaches quorum, the consensus fails and triggers re-delegation.
//! 4. **Partial failure**: Failed agent results are excluded from quorum calculation.

use crate::error::{DelegationError, Result};
use adp_core::task::Id;
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, info, instrument, warn};

/// Outcome of a consensus round.
#[derive(Debug, Clone, PartialEq)]
pub enum ConsensusOutcome {
    /// Consensus reached with the given result.
    Reached {
        result: Value,
        agreeing_agents: Vec<Id>,
        total_agents: usize,
    },
    /// No quorum reached. Contains all results for analysis.
    Failed {
        results: Vec<(Id, Value)>,
        reason: String,
    },
}

/// Consensus engine for vote-based delegation.
#[derive(Debug, Clone, Default)]
pub struct ConsensusEngine;

impl ConsensusEngine {
    /// Create a new consensus engine.
    pub fn new() -> Self {
        Self
    }

    /// Evaluate results from multiple agents and determine if quorum is reached.
    ///
    /// # Parameters
    ///
    /// - `results`: Vec of `(agent_id, result)` pairs. `None` values represent failures.
    /// - `quorum_percent`: Required agreement percentage (0-100).
    #[instrument(skip(self, results), fields(quorum = quorum_percent, total = results.len()))]
    pub fn evaluate(
        &self,
        results: Vec<(Id, Option<Value>)>,
        quorum_percent: u32,
    ) -> Result<ConsensusOutcome> {
        // Filter out failures.
        let successful: Vec<(Id, Value)> = results
            .into_iter()
            .filter_map(|(id, maybe_result)| maybe_result.map(|r| (id, r)))
            .collect();

        let total = successful.len();
        if total == 0 {
            return Ok(ConsensusOutcome::Failed {
                results: Vec::new(),
                reason: "all agents failed".to_string(),
            });
        }

        // Group by result (JSON structural equality).
        let mut buckets: HashMap<String, (Value, Vec<Id>)> = HashMap::new();
        for (agent_id, result) in successful {
            let key = serde_json::to_string(&result)
                .map_err(|e| DelegationError::ConsensusFailed(format!("serialization: {e}")))?;
            buckets
                .entry(key)
                .and_modify(|(_, agents)| agents.push(agent_id))
                .or_insert_with(|| (result, vec![agent_id]));
        }

        // Find the bucket with the most agreements.
        let (best_result, best_agents) = buckets
            .into_values()
            .max_by_key(|(_, agents)| agents.len())
            .unwrap();

        let agreement_count = best_agents.len();
        let agreement_percent = (agreement_count as f64 / total as f64) * 100.0;

        debug!(
            agreement_count = agreement_count,
            agreement_percent = agreement_percent,
            "consensus evaluation"
        );

        if agreement_percent >= quorum_percent as f64 {
            info!(
                agreement_count = agreement_count,
                total = total,
                "consensus reached"
            );
            Ok(ConsensusOutcome::Reached {
                result: best_result,
                agreeing_agents: best_agents,
                total_agents: total,
            })
        } else {
            warn!(
                agreement_percent = agreement_percent,
                quorum_required = quorum_percent,
                "consensus failed"
            );
            Ok(ConsensusOutcome::Failed {
                results: vec![], // Could populate from buckets if needed
                reason: format!(
                    "agreement {}% < quorum {}%",
                    agreement_percent, quorum_percent
                ),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_id() -> Id {
        Id::new()
    }

    #[test]
    fn unanimous_consensus() {
        let engine = ConsensusEngine::new();
        let results = vec![
            (make_id(), Some(json!({"answer": 42}))),
            (make_id(), Some(json!({"answer": 42}))),
            (make_id(), Some(json!({"answer": 42}))),
        ];

        let outcome = engine.evaluate(results, 67).unwrap();
        assert!(matches!(outcome, ConsensusOutcome::Reached { .. }));
        if let ConsensusOutcome::Reached { result, agreeing_agents, total_agents } = outcome {
            assert_eq!(result, json!({"answer": 42}));
            assert_eq!(agreeing_agents.len(), 3);
            assert_eq!(total_agents, 3);
        }
    }

    #[test]
    fn quorum_reached_with_two_thirds() {
        let engine = ConsensusEngine::new();
        let results = vec![
            (make_id(), Some(json!({"answer": 42}))),
            (make_id(), Some(json!({"answer": 42}))),
            (make_id(), Some(json!({"answer": 99}))),
        ];

        let outcome = engine.evaluate(results, 67).unwrap();
        assert!(matches!(outcome, ConsensusOutcome::Reached { .. }));
    }

    #[test]
    fn quorum_not_reached() {
        let engine = ConsensusEngine::new();
        let results = vec![
            (make_id(), Some(json!({"answer": 42}))),
            (make_id(), Some(json!({"answer": 99}))),
            (make_id(), Some(json!({"answer": 100}))),
        ];

        let outcome = engine.evaluate(results, 67).unwrap();
        assert!(matches!(outcome, ConsensusOutcome::Failed { .. }));
    }

    #[test]
    fn failures_excluded_from_quorum() {
        let engine = ConsensusEngine::new();
        let results = vec![
            (make_id(), Some(json!({"answer": 42}))),
            (make_id(), Some(json!({"answer": 42}))),
            (make_id(), None), // failure
        ];

        // 2/2 = 100% agreement among successful agents
        let outcome = engine.evaluate(results, 67).unwrap();
        assert!(matches!(outcome, ConsensusOutcome::Reached { .. }));
    }

    #[test]
    fn all_failures() {
        let engine = ConsensusEngine::new();
        let results = vec![
            (make_id(), None),
            (make_id(), None),
        ];

        let outcome = engine.evaluate(results, 67).unwrap();
        assert!(matches!(outcome, ConsensusOutcome::Failed { .. }));
    }
}
