# ADP — Agent Delegation Protocol

> Air traffic control for autonomous AI agents. Local-first. Protocol-native.

## Overview

ADP is a local-first orchestration layer for teams of autonomous AI agents.
It provides deterministic task scheduling, capability-based security,
WASM sandboxing, and local LLM inference — with zero cloud dependencies
in the core path.

## Quick Start

```bash
# Clone
git clone https://github.com/44tl/ADP.git
cd adp

# Build
cargo build --release

# Run tests
cargo test --workspace

# Run desktop app
cargo tauri dev
```

## Crates

| Crate | Purpose | Dependencies |
|-------|---------|-------------|
| `adp-core` | Protocol, state machine, scheduler | — |
| `adp-runtime` | Agent lifecycle, WASM sandbox | `adp-core` |
| `adp-delegation` | Strategies, registry, consensus | `adp-core`, `adp-runtime` |
| `adp-memory` | Vector store, conversations | `adp-core` |
| `adp-mcp` | MCP protocol, adapters | `adp-runtime` |
| `adp-router` | LLM inference, token management | `adp-core` |
| `adp-gateway` | gRPC/REST/WebSocket | `adp-core`, `adp-delegation` |
| `adp-desktop` | Tauri desktop shell | all above |

## License

MIT
