//! Tests for capability-based security.

use adp_runtime::capabilities::{Capability, CapabilitySet};

#[test]
fn empty_capability_set_has_nothing() {
    let caps = CapabilitySet::new();
    assert!(caps.is_empty());
    assert!(!caps.has(&Capability::EventLogRead));
}

#[test]
fn grant_and_check() {
    let caps = CapabilitySet::new()
        .grant(Capability::FileRead { path: "/data".into() })
        .grant(Capability::HttpRequest {
            allowlist: vec!["api.local".into()],
        });

    assert!(caps.has(&Capability::FileRead { path: "/data".into() }));
    assert!(!caps.has(&Capability::FileRead { path: "/etc".into() }));
    assert!(caps.has_category("file"));
    assert!(caps.has_category("http"));
    assert!(!caps.has_category("tool"));
}

#[test]
fn revoke_removes_capability() {
    let mut caps = CapabilitySet::new().grant(Capability::SpawnAgent);
    assert!(caps.has(&Capability::SpawnAgent));
    caps.revoke(&Capability::SpawnAgent);
    assert!(!caps.has(&Capability::SpawnAgent));
}

#[test]
fn serde_roundtrip() {
    let caps = CapabilitySet::new()
        .grant(Capability::FileRead { path: "/tmp".into() })
        .grant(Capability::ToolExecution {
            tools: vec!["browser".into()],
        });

    let json = serde_json::to_string(&caps).unwrap();
    let decoded: CapabilitySet = serde_json::from_str(&json).unwrap();
    assert_eq!(caps, decoded);
}
