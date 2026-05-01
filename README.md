# ADP — Agent Delegation Protocol

> Air traffic control for autonomous AI agents. Local-first. Protocol-native.

## Overview

ADP is a local-first orchestration layer for teams of autonomous AI agents. It provides deterministic task scheduling, capability-based security, WASM sandboxing, and local LLM inference — with zero cloud dependencies in the core path.

## Quick Start

```bash
# Clone
git clone [https://github.com/44tl/ADP.git](https://github.com/44tl/ADP.git)
cd adp

# Build
cargo build --release

# Run tests
cargo test --workspace

# Run desktop app
cargo tauri dev
```

## Architecture

ADP is built with a modular crate structure designed to separate core protocol logic from networking, user interface, and sandboxing:

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│                adp-gateway (gRPC / REST / WebSocket)            │
│                                                                 │
│                adp-desktop (Tauri Desktop Shell)                │
│                                                                 │
└──────────────┬───────────────────────────┬──────────────────────┘
               │                           │
 ┌─────────────▼─────────────┐ ┌───────────▼───────────┐
 │                           │ │                       │
 │      adp-delegation       │ │       adp-router      │
 │  (Strategies, Registry,   │ │    (LLM Inference,    │
 │        Consensus)         │ │   Token Management)   │
 │                           │ │                       │
 └─────────────┬─────────────┘ └───────────┬───────────┘
               │                           │
 ┌─────────────▼─────────────┐ ┌───────────▼───────────┐
 │                           │ │                       │
 │       adp-runtime         │ │       adp-memory      │
 │    (Agent Lifecycle,      │ │     (Vector Store,    │
 │      WASM Sandbox)        │ │     Conversations)    │
 │                           │ │                       │
 └─────────────┬─────────────┘ └───────────────────────┘
               │
 ┌─────────────▼─────────────┐
 │                           │
 │         adp-core          │
 │   (Protocol, State Machine│
 │        Scheduler)         │
 │                           │
 └───────────────────────────┘
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

## Example Agents

Reference implementations of ADP agents are compiled to WASM and run inside the `adp-runtime` sandbox.

| Agent | Input | Output | Capabilities |
|-------|-------|--------|-------------|
| `coder` | File paths + requirements | Diff patches | `file:read`, `file:write` |
| `researcher` | Query + sources | Summary + confidence | `http:request`, `tool:execute` |
| `tester` | Code context | Test cases + results | `file:read`, `tool:execute` |
| `architect` | Requirements + constraints | ADR + module structure | `eventlog:read`, `agent:spawn` |

### Building Agents

```bash
cargo build --target wasm32-wasi --release
```

## Configuration

Default configurations are loaded from `config/default.toml` and can be overridden using environment variables or a `~/.adp/config.toml` file.

```toml
[database]
path = "~/.adp/adp.db"

[server]
grpc_addr = "127.0.0.1:50051"
rest_addr = "127.0.0.1:8080"
websocket_addr = "127.0.0.1:8081"
```

## License

This project is licensed under the MIT License.
