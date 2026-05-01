//! Tests for delegation strategies.

use adp_core::task::{BroadcastStrategy, MapReduceStrategy, PipelineStrategy, Strategy, VoteStrategy};
use adp_delegation::registry::{AgentRegistry, RegistryEntry};
use adp_delegation::strategy::{strategy_from_adp, StrategyContext};
use adp_runtime::capabilities::CapabilitySet;

fn make_registry() -> AgentRegistry {
    let registry = AgentRegistry::new();
    for i in 0..5 {
        let entry = RegistryEntry {
            agent_id: adp_core::task::Id::new(),
            name: format!("agent-{i}"),
            capabilities: CapabilitySet::new(),
            metadata: serde_json::Value::Null,
            last_heartbeat: chrono::Utc::now(),
        };
        registry.register(entry).await;
    }
    registry
}

#[tokio::test]
async fn single_strategy_selects_one() {
    let registry = make_registry();
    let strategy = strategy_from_adp(&Strategy::Single(adp_core::task::SingleStrategy));
    let ctx = StrategyContext {
        task_id: adp_core::task::Id::new(),
        required_capabilities: CapabilitySet::new(),
        task_context: serde_json::Value::Null,
    };

    let agents = strategy.select_agents(&registry, &ctx).await.unwrap();
    assert_eq!(agents.len(), 1);
}

#[tokio::test]
async fn broadcast_strategy_selects_up_to_max() {
    let registry = make_registry();
    let strategy = strategy_from_adp(&Strategy::Broadcast(BroadcastStrategy { max_agents: 3 }));
    let ctx = StrategyContext {
        task_id: adp_core::task::Id::new(),
        required_capabilities: CapabilitySet::new(),
        task_context: serde_json::Value::Null,
    };

    let agents = strategy.select_agents(&registry, &ctx).await.unwrap();
    assert_eq!(agents.len(), 3);
}

#[tokio::test]
async fn mapreduce_strategy_selects_up_to_map_count() {
    let registry = make_registry();
    let strategy = strategy_from_adp(&Strategy::MapReduce(MapReduceStrategy { map_count: 4 }));
    let ctx = StrategyContext {
        task_id: adp_core::task::Id::new(),
        required_capabilities: CapabilitySet::new(),
        task_context: serde_json::Value::Null,
    };

    let agents = strategy.select_agents(&registry, &ctx).await.unwrap();
    assert_eq!(agents.len(), 4);
}
