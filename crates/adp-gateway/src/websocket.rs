//! WebSocket streaming for real-time events.
//!
//! Clients connect to `/ws/events` and receive all task state transitions
//! as they happen.

use crate::error::{GatewayError, Result};
use adp_core::task::Event;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
    routing::get,
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info, instrument};

/// Event broadcaster for WebSocket clients.
#[derive(Debug, Clone)]
pub struct EventBroadcaster {
    tx: broadcast::Sender<Event>,
}

impl EventBroadcaster {
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Broadcast an event to all connected clients.
    pub fn broadcast(&self, event: Event) {
        let _ = self.tx.send(event);
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

/// WebSocket handler.
#[instrument(skip(ws, broadcaster))]
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    axum::extract::State(broadcaster): axum::extract::State<Arc<EventBroadcaster>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, broadcaster))
}

async fn handle_socket(mut socket: WebSocket, broadcaster: Arc<EventBroadcaster>) {
    info!("WebSocket client connected");
    let mut rx = broadcaster.subscribe();

    loop {
        tokio::select! {
            Ok(event) = rx.recv() => {
                let json = match serde_json::to_string(&event) {
                    Ok(j) => j,
                    Err(e) => {
                        error!(error = %e, "failed to serialize event");
                        continue;
                    }
                };

                if socket.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }

            msg = socket.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Text(text))) => {
                        info!(msg = %text, "WebSocket message received");
                    }
                    _ => {}
                }
            }
        }
    }

    info!("WebSocket client disconnected");
}

/// Add WebSocket routes to a router.
pub fn routes(broadcaster: Arc<EventBroadcaster>) -> Router {
    Router::new()
        .route("/ws/events", get(ws_handler))
        .with_state(broadcaster)
}
