//! Tests for agent registry.

use adp_delegation::registry::{AgentRegistry, RegistryEntry};
use adp_runtime::capabilities::{Capability, CapabilitySet};
use std::time::Duration;

fn make_entry(name: &str, caps: CapabilitySet) -> RegistryEntry {
    RegistryEntry {
        agent_id: adp_core::task::Id::new(),
        name: name.to_string(),
        capabilities: caps,
        metadata: serde_json::Value::Null,
        last_heartbeat: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn register_and_find() {
    let registry = AgentRegistry::new();
    let entry = make_entry(
        "coder",
        CapabilitySet::new().grant(Capability::FileRead { path: "/src".into() }),
    );
    let id = entry.agent_id;

    registry.register(entry).await;

    let found = registry.get(&id).await;
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "coder");
}

#[tokio::test]
async fn find_matching_requires_all_caps() {
    let registry = AgentRegistry::new();

    let coder = make_entry(
        "coder",
        CapabilitySet::new()
            .grant(Capability::FileRead { path: "/src".into() })
            .grant(Capability::FileWrite { path: "/src".into() }),
    );
    let reader = make_entry(
        "reader",
        CapabilitySet::new().grant(Capability::FileRead { path: "/src".into() }),
    );

    registry.register(coder).await;
    registry.register(reader).await;

    let required = CapabilitySet::new()
        .grant(Capability::FileRead { path: "/src".into() })
        .grant(Capability::FileWrite { path: "/src".into() });

    let matching = registry.find_matching(&required).await;
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].name, "coder");
}

#[tokio::test]
async fn prune_stale_removes_old_agents() {
    let registry = AgentRegistry::new();
    let mut entry = make_entry("old", CapabilitySet::new());
    entry.last_heartbeat = chrono::Utc::now() - chrono::Duration::seconds(3600);
    let id = entry.agent_id;

    registry.register(entry).await;
    registry.prune_stale(Duration::from_secs(300)).await;

    let found = registry.get(&id).await;
    assert!(found.is_none());
}
