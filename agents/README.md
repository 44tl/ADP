# ADP Example Agents

Reference implementations of ADP agents. Each agent is a Rust crate that
compiles to a WASM module and runs inside the `adp-runtime` sandbox.

## Agents

| Agent | Input | Output | Capabilities |
|-------|-------|--------|-------------|
| `coder` | File paths + requirements | Diff patches | `file:read`, `file:write` |
| `researcher` | Query + sources | Summary + confidence | `http:request`, `tool:execute` |
| `tester` | Code context | Test cases + results | `file:read`, `tool:execute` |
| `architect` | Requirements + constraints | ADR + module structure | `eventlog:read`, `agent:spawn` |

## Building

```bash
cargo build --target wasm32-wasi --release
```

## Security

Agents have **zero** capabilities by default. The host runtime must explicitly
grant capabilities via the agent manifest before spawning.
