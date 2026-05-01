//! Tests for resource limits.

use adp_runtime::resources::ResourceLimits;
use std::time::Duration;

#[test]
fn default_limits_are_conservative() {
    let limits = ResourceLimits::default();
    assert_eq!(limits.max_fuel, 10_000_000_000);
    assert_eq!(limits.max_memory_pages, 512);
    assert_eq!(limits.max_execution_time, Duration::from_secs(300));
    assert_eq!(limits.max_output_size, 1_048_576);
}

#[test]
fn builder_overrides() {
    let limits = ResourceLimits::builder()
        .max_fuel(1_000)
        .max_memory_pages(64)
        .max_execution_time(Duration::from_secs(10))
        .max_output_size(1024)
        .build();

    assert_eq!(limits.max_fuel, 1_000);
    assert_eq!(limits.max_memory_pages, 64);
    assert_eq!(limits.max_execution_time, Duration::from_secs(10));
    assert_eq!(limits.max_output_size, 1024);
}
