//! Application state for the Tauri desktop app.
//!
//! [`AppState`] holds the initialized ADP services and is shared
//! across all Tauri command handlers via Tauri's managed state.

use adp_core::db::open_database;
use adp_core::scheduler::DagScheduler;
use adp_core::store_libsql::{LibSqlEventStore, LibSqlTaskStore};
use adp_delegation::registry::AgentRegistry;
use adp_delegation::engine::DelegationEngine;
use adp_router::router::InferenceRouter;
use adp_router::token_manager::TokenBudget;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, instrument};

/// Shared application state.
#[derive(Debug)]
pub struct AppState {
    /// The task scheduler.
    pub scheduler: Arc<DagScheduler<LibSqlTaskStore, LibSqlEventStore>>,
    /// The delegation engine.
    pub delegation: Arc<DelegationEngine>,
    /// The agent registry.
    pub registry: AgentRegistry,
    /// The inference router.
    pub router: Arc<RwLock<InferenceRouter>>,
    /// Database path.
    pub db_path: PathBuf,
}

impl AppState {
    /// Initialize all ADP services.
    #[instrument]
    pub async fn new(db_path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        info!(path = %db_path.display(), "initializing ADP desktop state");

        let conn = open_database(&db_path).await?;
        let task_store = LibSqlTaskStore::new(conn.clone());
        let event_store = LibSqlEventStore::new(conn);
        let scheduler = Arc::new(DagScheduler::new(task_store, event_store));

        let registry = AgentRegistry::new();
        let delegation = Arc::new(DelegationEngine::new(registry.clone()));

        let router = Arc::new(RwLock::new(InferenceRouter::new(
            TokenBudget::default(),
            "default",
        )));

        Ok(Self {
            scheduler,
            delegation,
            registry,
            router,
            db_path,
        })
    }
}
