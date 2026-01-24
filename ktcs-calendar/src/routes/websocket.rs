//! WebSocket endpoint for real-time proof confirmations
//!
//! Implements the /v1/stream endpoint per spec Section 5.2:
//!
//! ```javascript
//! const ws = new WebSocket('wss://calendar.example.com/v1/stream');
//! ws.send(JSON.stringify({ type: 'subscribe', proof_id: 'ktcs_abc123' }));
//! // Receive: { type: 'confirmed', proof_id: '...', block_hash: '...', proof: '...' }
//! ```

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// WebSocket message from client
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// Subscribe to proof confirmation events
    #[serde(rename = "subscribe")]
    Subscribe { proof_id: String },
    /// Unsubscribe from proof confirmation events
    #[serde(rename = "unsubscribe")]
    Unsubscribe { proof_id: String },
    /// Ping to keep connection alive
    #[serde(rename = "ping")]
    Ping,
}

/// WebSocket message to client
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
pub enum ServerMessage {
    /// Proof has been confirmed
    #[serde(rename = "confirmed")]
    Confirmed {
        proof_id: String,
        block_hash: String,
        daa_score: u64,
        blue_score: u64,
        timestamp: u64,
        proof: String, // base64-encoded .kts file
    },
    /// Proof is being batched
    #[serde(rename = "batched")]
    Batched { proof_id: String },
    /// Subscription acknowledged
    #[serde(rename = "subscribed")]
    Subscribed { proof_id: String },
    /// Unsubscription acknowledged
    #[serde(rename = "unsubscribed")]
    Unsubscribed { proof_id: String },
    /// Pong response
    #[serde(rename = "pong")]
    Pong,
    /// Error message
    #[serde(rename = "error")]
    Error { message: String },
}

/// Confirmation event broadcast to subscribed clients
#[derive(Debug, Clone)]
pub struct ConfirmationEvent {
    pub proof_id: String,
    pub block_hash: String,
    pub daa_score: u64,
    pub blue_score: u64,
    pub timestamp: u64,
    pub proof_base64: String,
}

/// Maximum number of subscriptions per session (prevent abuse)
const MAX_SUBSCRIPTIONS_PER_SESSION: usize = 100;

/// State for WebSocket connections
pub struct WsState {
    /// Broadcast channel for confirmation events
    pub confirmations_tx: broadcast::Sender<ConfirmationEvent>,
    /// Active subscriptions: proof_id -> set of session IDs
    subscriptions: RwLock<HashMap<String, HashSet<u64>>>,
    /// Per-session subscription count for rate limiting
    session_subscription_counts: RwLock<HashMap<u64, usize>>,
    /// Next session ID
    next_session_id: RwLock<u64>,
}

impl WsState {
    /// Create a new WebSocket state
    pub fn new() -> Self {
        let (confirmations_tx, _) = broadcast::channel(1024);
        Self {
            confirmations_tx,
            subscriptions: RwLock::new(HashMap::new()),
            session_subscription_counts: RwLock::new(HashMap::new()),
            next_session_id: RwLock::new(0),
        }
    }

    /// Allocate a new session ID
    async fn allocate_session_id(&self) -> u64 {
        let mut id = self.next_session_id.write().await;
        let session_id = *id;
        *id += 1;
        session_id
    }

    /// Subscribe a session to a proof ID
    /// Returns false if subscription limit exceeded
    async fn subscribe(&self, session_id: u64, proof_id: &str) -> bool {
        // Check subscription limit
        {
            let counts = self.session_subscription_counts.read().await;
            if let Some(&count) = counts.get(&session_id) {
                if count >= MAX_SUBSCRIPTIONS_PER_SESSION {
                    warn!(
                        "Session {} exceeded subscription limit ({} max)",
                        session_id, MAX_SUBSCRIPTIONS_PER_SESSION
                    );
                    return false;
                }
            }
        }

        // Validate proof_id format (basic validation)
        if proof_id.is_empty() || proof_id.len() > 64 {
            warn!("Session {} tried to subscribe to invalid proof_id", session_id);
            return false;
        }

        let mut subs = self.subscriptions.write().await;
        subs.entry(proof_id.to_string())
            .or_insert_with(HashSet::new)
            .insert(session_id);

        // Update subscription count
        let mut counts = self.session_subscription_counts.write().await;
        *counts.entry(session_id).or_insert(0) += 1;

        debug!("Session {} subscribed to {}", session_id, proof_id);
        true
    }

    /// Unsubscribe a session from a proof ID
    async fn unsubscribe(&self, session_id: u64, proof_id: &str) {
        let mut subs = self.subscriptions.write().await;
        if let Some(sessions) = subs.get_mut(proof_id) {
            if sessions.remove(&session_id) {
                // Decrement subscription count
                let mut counts = self.session_subscription_counts.write().await;
                if let Some(count) = counts.get_mut(&session_id) {
                    *count = count.saturating_sub(1);
                }
            }
            if sessions.is_empty() {
                subs.remove(proof_id);
            }
        }
        debug!("Session {} unsubscribed from {}", session_id, proof_id);
    }

    /// Unsubscribe a session from all proof IDs
    async fn unsubscribe_all(&self, session_id: u64) {
        let mut subs = self.subscriptions.write().await;
        for sessions in subs.values_mut() {
            sessions.remove(&session_id);
        }
        // Clean up empty entries
        subs.retain(|_, sessions| !sessions.is_empty());

        // Clean up session subscription count
        let mut counts = self.session_subscription_counts.write().await;
        counts.remove(&session_id);

        debug!("Session {} unsubscribed from all", session_id);
    }

    /// Broadcast a confirmation event
    pub fn broadcast_confirmation(&self, event: ConfirmationEvent) {
        if let Err(e) = self.confirmations_tx.send(event) {
            debug!("No subscribers for confirmation: {}", e);
        }
    }
}

impl Default for WsState {
    fn default() -> Self {
        Self::new()
    }
}

/// Application state needed by WebSocket handler
pub trait WsAppState: Clone + Send + Sync + 'static {
    fn ws_state(&self) -> &Arc<WsState>;
}

/// WebSocket upgrade handler
pub async fn ws_handler<S: WsAppState>(
    ws: WebSocketUpgrade,
    State(state): State<S>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handle a WebSocket connection
async fn handle_socket<S: WsAppState>(socket: WebSocket, state: S) {
    let ws_state = state.ws_state();
    let session_id = ws_state.allocate_session_id().await;

    info!("WebSocket connection {} established", session_id);

    let (mut sender, mut receiver) = socket.split();

    // Create channel for sending messages to client
    let (tx, mut rx) = mpsc::channel::<ServerMessage>(32);

    // Subscribe to confirmation broadcast
    let mut confirmations_rx = ws_state.confirmations_tx.subscribe();

    // Track this session's subscriptions
    let session_subscriptions: Arc<RwLock<HashSet<String>>> = Arc::new(RwLock::new(HashSet::new()));

    // Task to send messages to client
    let tx_clone = tx.clone();
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match serde_json::to_string(&msg) {
                Ok(json) => {
                    if sender.send(Message::Text(json)).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to serialize message: {}", e);
                }
            }
        }
    });

    // Task to forward confirmations to subscribed clients
    let subs_clone = session_subscriptions.clone();
    let confirm_task = tokio::spawn(async move {
        loop {
            match confirmations_rx.recv().await {
                Ok(event) => {
                    // Check if this session is subscribed to this proof
                    let subs = subs_clone.read().await;
                    if subs.contains(&event.proof_id) {
                        let msg = ServerMessage::Confirmed {
                            proof_id: event.proof_id,
                            block_hash: event.block_hash,
                            daa_score: event.daa_score,
                            blue_score: event.blue_score,
                            timestamp: event.timestamp,
                            proof: event.proof_base64,
                        };
                        if tx_clone.send(msg).await.is_err() {
                            break;
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Session lagged behind by {} messages", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });

    // Main receive loop
    while let Some(result) = receiver.next().await {
        match result {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(msg) => match msg {
                        ClientMessage::Subscribe { proof_id } => {
                            if ws_state.subscribe(session_id, &proof_id).await {
                                session_subscriptions.write().await.insert(proof_id.clone());
                                let _ = tx.send(ServerMessage::Subscribed { proof_id }).await;
                            } else {
                                let _ = tx
                                    .send(ServerMessage::Error {
                                        message: format!(
                                            "Subscription failed: limit exceeded ({} max) or invalid proof_id",
                                            MAX_SUBSCRIPTIONS_PER_SESSION
                                        ),
                                    })
                                    .await;
                            }
                        }
                        ClientMessage::Unsubscribe { proof_id } => {
                            ws_state.unsubscribe(session_id, &proof_id).await;
                            session_subscriptions.write().await.remove(&proof_id);
                            let _ = tx.send(ServerMessage::Unsubscribed { proof_id }).await;
                        }
                        ClientMessage::Ping => {
                            let _ = tx.send(ServerMessage::Pong).await;
                        }
                    },
                    Err(e) => {
                        let _ = tx
                            .send(ServerMessage::Error {
                                message: format!("Invalid message: {}", e),
                            })
                            .await;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                break;
            }
            Ok(_) => {
                // Ignore binary, ping, pong
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
        }
    }

    // Cleanup
    info!("WebSocket connection {} closed", session_id);
    ws_state.unsubscribe_all(session_id).await;

    // Cancel tasks
    send_task.abort();
    confirm_task.abort();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ws_state() {
        let state = WsState::new();

        let session1 = state.allocate_session_id().await;
        let session2 = state.allocate_session_id().await;

        assert_eq!(session1, 0);
        assert_eq!(session2, 1);

        state.subscribe(session1, "proof_1").await;
        state.subscribe(session2, "proof_1").await;
        state.subscribe(session1, "proof_2").await;

        let subs = state.subscriptions.read().await;
        assert_eq!(subs.get("proof_1").unwrap().len(), 2);
        assert_eq!(subs.get("proof_2").unwrap().len(), 1);
        drop(subs);

        state.unsubscribe(session1, "proof_1").await;
        let subs = state.subscriptions.read().await;
        assert_eq!(subs.get("proof_1").unwrap().len(), 1);
        drop(subs);

        state.unsubscribe_all(session2).await;
        let subs = state.subscriptions.read().await;
        assert!(subs.get("proof_1").is_none() || subs.get("proof_1").unwrap().is_empty());
    }

    #[test]
    fn test_client_message_parse() {
        let msg: ClientMessage =
            serde_json::from_str(r#"{"type":"subscribe","proof_id":"ktcs_123"}"#).unwrap();
        match msg {
            ClientMessage::Subscribe { proof_id } => assert_eq!(proof_id, "ktcs_123"),
            _ => panic!("Wrong message type"),
        }

        let msg: ClientMessage =
            serde_json::from_str(r#"{"type":"unsubscribe","proof_id":"ktcs_456"}"#).unwrap();
        match msg {
            ClientMessage::Unsubscribe { proof_id } => assert_eq!(proof_id, "ktcs_456"),
            _ => panic!("Wrong message type"),
        }

        let msg: ClientMessage = serde_json::from_str(r#"{"type":"ping"}"#).unwrap();
        assert!(matches!(msg, ClientMessage::Ping));
    }

    #[test]
    fn test_server_message_serialize() {
        let msg = ServerMessage::Confirmed {
            proof_id: "ktcs_123".to_string(),
            block_hash: "abc123".to_string(),
            daa_score: 42000000,
            blue_score: 41500000,
            timestamp: 1706000000000,
            proof: "base64proof".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("confirmed"));
        assert!(json.contains("ktcs_123"));

        let msg = ServerMessage::Pong;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("pong"));
    }
}
