//! MCP (Model Context Protocol) client and adapter.
//!
//! MCP servers run as separate OS processes and communicate via stdio (JSON-RPC 2.0).
//! This module provides:
//! - [`McpServer`] — manages the lifecycle of an MCP server process.
//! - [`McpClient`] — sends requests and receives responses via JSON-RPC.
//! - [`McpTool`] — represents a tool exposed by an MCP server.
//!
//! # Security
//!
//! MCP servers run **outside** the WASM sandbox in separate OS processes.
//! They are spawned with the same UID as the host, so they inherit the host's
//! filesystem and network access. This is by design: MCP servers are trusted
//! infrastructure, not untrusted agent code.

use crate::error::{RuntimeError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, instrument, warn};

/// A tool exposed by an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// A running MCP server process.
pub struct McpServer {
    /// Server name (for logging).
    pub name: String,
    /// The OS process handle.
    child: Child,
    /// Channel to send JSON-RPC requests.
    request_tx: mpsc::Sender<JsonRpcRequest>,
    /// Shutdown signal.
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl McpServer {
    /// Spawn a new MCP server process.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # async fn example() -> adp_runtime::Result<()> {
    /// use adp_runtime::mcp::McpServer;
    ///
    /// let server = McpServer::spawn("browser", "mcp-browser-server").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument]
    pub async fn spawn(name: impl Into<String>, command: impl Into<String>) -> Result<Self> {
        let name = name.into();
        let cmd = command.into();
        info!(server_name = %name, command = %cmd, "spawning MCP server");

        let mut child = Command::new(&cmd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RuntimeError::Mcp(format!("failed to spawn {cmd}: {e}")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RuntimeError::Mcp("failed to capture stdout".to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| RuntimeError::Mcp("failed to capture stdin".to_string()))?;

        let (request_tx, mut request_rx) = mpsc::channel::<JsonRpcRequest>(32);
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        // Spawn the I/O loop.
        tokio::spawn(async move {
            let mut stdin = stdin;
            let mut stdout_reader = BufReader::new(stdout).lines();
            let mut pending_requests: std::collections::HashMap<u64, oneshot::Sender<JsonRpcResponse>> =
                std::collections::HashMap::new();
            let mut next_id: u64 = 1;

            loop {
                tokio::select! {
                    biased;

                    // Shutdown signal
                    _ = &mut shutdown_rx => {
                        debug!("MCP server I/O loop shutting down");
                        break;
                    }

                    // Incoming request from client
                    Some(req) = request_rx.recv() => {
                        let id = next_id;
                        next_id += 1;
                        pending_requests.insert(id, req.response_tx);

                        let json = match serde_json::to_string(&JsonRpcMessage::Request {
                            jsonrpc: "2.0".to_string(),
                            id,
                            method: req.method,
                            params: req.params,
                        }) {
                            Ok(j) => j,
                            Err(e) => {
                                error!(error = %e, "failed to serialize JSON-RPC request");
                                continue;
                            }
                        };

                        if let Err(e) = stdin.write_all(json.as_bytes()).await {
                            error!(error = %e, "failed to write to MCP server stdin");
                        }
                        if let Err(e) = stdin.write_all(b"
").await {
                            error!(error = %e, "failed to write newline to MCP server stdin");
                        }
                        if let Err(e) = stdin.flush().await {
                            error!(error = %e, "failed to flush MCP server stdin");
                        }
                    }

                    // Response from server stdout
                    Ok(Some(line)) = stdout_reader.next_line() => {
                        if line.trim().is_empty() {
                            continue;
                        }

                        debug!(line = %line, "MCP server stdout");

                        match serde_json::from_str::<JsonRpcMessage>(&line) {
                            Ok(JsonRpcMessage::Response { id, result, error }) => {
                                if let Some(tx) = pending_requests.remove(&id) {
                                    let resp = JsonRpcResponse { result, error };
                                    let _ = tx.send(resp);
                                }
                            }
                            Ok(JsonRpcMessage::Notification { method, params }) => {
                                debug!(method = %method, params = ?params, "MCP notification");
                            }
                            Err(e) => {
                                warn!(error = %e, line = %line, "failed to parse JSON-RPC message");
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            name,
            child,
            request_tx,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    /// Send a JSON-RPC request and await the response.
    pub async fn request(&self, method: impl Into<String>, params: Value) -> Result<Value> {
        let (tx, rx) = oneshot::channel();
        let req = JsonRpcRequest {
            method: method.into(),
            params,
            response_tx: tx,
        };

        self.request_tx
            .send(req)
            .await
            .map_err(|_| RuntimeError::Mcp("request channel closed".to_string()))?;

        let resp = rx
            .await
            .map_err(|_| RuntimeError::Mcp("response channel closed".to_string()))?;

        if let Some(err) = resp.error {
            return Err(RuntimeError::Mcp(format!(
                "JSON-RPC error: {} (code: {})",
                err.message, err.code
            )));
        }

        resp.result.ok_or_else(|| RuntimeError::Mcp("empty response".to_string()))
    }

    /// Gracefully shut down the MCP server.
    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        match tokio::time::timeout(tokio::time::Duration::from_secs(5), self.child.wait()).await {
            Ok(Ok(status)) => {
                info!(server_name = %self.name, status = ?status, "MCP server exited");
                Ok(())
            }
            Ok(Err(e)) => Err(RuntimeError::Mcp(format!("wait failed: {e}"))),
            Err(_) => {
                warn!(server_name = %self.name, "MCP server did not exit gracefully, killing");
                let _ = self.child.kill().await;
                Ok(())
            }
        }
    }
}

// ===================================================================
// Internal JSON-RPC types
// ===================================================================

#[derive(Debug)]
struct JsonRpcRequest {
    method: String,
    params: Value,
    response_tx: oneshot::Sender<JsonRpcResponse>,
}

#[derive(Debug)]
struct JsonRpcResponse {
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum JsonRpcMessage {
    Request {
        jsonrpc: String,
        id: u64,
        method: String,
        #[serde(default)]
        params: Value,
    },
    Response {
        jsonrpc: String,
        id: u64,
        #[serde(default)]
        result: Option<Value>,
        #[serde(default)]
        error: Option<JsonRpcError>,
    },
    Notification {
        jsonrpc: String,
        method: String,
        #[serde(default)]
        params: Value,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}
