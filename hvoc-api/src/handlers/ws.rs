//! WebSocket handler for live sync events and real-time call relay.
//!
//! The frontend connects here to receive real-time updates (new posts,
//! DHT value changes, attachment state, etc.) and to send/receive call
//! packets (video frames, audio chunks, signaling) for ASCII video chat.

use std::sync::Arc;
use axum::{
    extract::{State, WebSocketUpgrade, ws::{Message, WebSocket}},
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};

use crate::AppState;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to sync events from the Veilid node.
    let mut sync_rx = state.node.subscribe_sync();

    // Forward sync events to the WebSocket client.
    let send_task = tokio::spawn(async move {
        while let Ok(event) = sync_rx.recv().await {
            let json = match serde_json::to_string(&event) {
                Ok(j) => j,
                Err(_) => continue,
            };
            if sender.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Read messages from the client — handle call packets.
    let state_recv = state.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    handle_client_message(&state_recv, &text).await;
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either task to finish.
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }
}

/// Handle a JSON message from the WebSocket client.
///
/// Call packets have a "t" field (call_offer, call_answer, call_reject,
/// call_end, vf, af) and a "peer" field identifying the recipient.
async fn handle_client_message(state: &AppState, text: &str) {
    let msg: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return,
    };

    let msg_type = match msg.get("t").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return,
    };

    let peer_id = match msg.get("peer").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => return,
    };

    // Update call state for signaling messages.
    match msg_type {
        "call_offer" | "call_answer" => {
            let mut cs = state.call_state.write().await;
            cs.active_peer = Some(peer_id.clone());
            cs.started_at = Some(chrono::Utc::now().timestamp());
        }
        "call_end" | "call_reject" => {
            let mut cs = state.call_state.write().await;
            cs.active_peer = None;
            cs.started_at = None;
        }
        _ => {} // vf, af — no state change
    }

    // Wrap the call packet in a DmPayload and deliver via Veilid app_message.
    deliver_call_packet(state, &peer_id, msg).await;
}

/// Encrypt and deliver a call packet to a peer via Veilid app_message.
/// Uses the same ECDH encryption as text DMs but with the `call_packet` field.
async fn deliver_call_packet(state: &AppState, recipient_id: &str, packet: serde_json::Value) {
    let kp_guard = state.keypair.read().await;
    let kp = match kp_guard.as_ref().cloned() {
        Some(kp) => kp,
        None => return,
    };
    drop(kp_guard);

    let recipient_pub = match recipient_id.parse::<veilid_core::PublicKey>() {
        Ok(pk) => pk,
        Err(_) => return,
    };

    // Build a DmPayload with call_packet instead of body text.
    let now = chrono::Utc::now().timestamp();
    let payload = hvoc_core::DmPayload {
        body: String::new(),
        sent_at: now,
        call_packet: Some(packet),
    };
    let plaintext = match serde_json::to_vec(&payload) {
        Ok(p) => p,
        Err(_) => return,
    };

    // ECDH shared secret.
    let encrypted = match state.node.with_crypto(|cs| {
        let shared = cs
            .generate_shared_secret(&recipient_pub, &kp.secret(), b"hvoc-dm-v1")
            .map_err(|e| hvoc_veilid::VeilidError::Crypto(e.to_string()))?;
        let nonce = cs.random_nonce();
        let ciphertext = cs
            .encrypt_aead(&plaintext, &nonce, &shared, None)
            .map_err(|e| hvoc_veilid::VeilidError::Crypto(e.to_string()))?;

        let sender_id = hvoc_veilid::crypto::author_id_from_key(&kp.key());
        let recipient_id_str = hvoc_veilid::crypto::author_id_from_key(&recipient_pub);

        Ok(hvoc_veilid::crypto::EncryptedDm {
            sender_id,
            recipient_id: recipient_id_str,
            nonce: hex::encode(nonce.as_ref()),
            ciphertext: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &ciphertext,
            ),
            sent_at: now,
        })
    }) {
        Ok(e) => e,
        Err(_) => return,
    };

    // Look up recipient's route and send.
    let ks = hvoc_store::Keystore(&state.store);
    let inbox_key = format!("inbox:{}", recipient_id);
    let (route_key_str, _) = match ks.get_dht_key(&inbox_key).await {
        Ok(Some(v)) => v,
        _ => return,
    };

    let rc = match state.node.routing_context() {
        Ok(rc) => rc,
        Err(_) => return,
    };

    let route_key = match route_key_str.parse::<veilid_core::RecordKey>() {
        Ok(k) => k,
        Err(_) => return,
    };

    let _ = hvoc_veilid::dht::open_record_readonly(&rc, route_key.clone()).await;
    if let Ok(Some(route_blob)) = hvoc_veilid::dht::get_value(&rc, route_key.clone(), 0, true).await {
        if let Ok(route_id) = state.node.api.import_remote_private_route(route_blob) {
            let msg_bytes = serde_json::to_vec(&encrypted).unwrap_or_default();
            let _ = rc.app_message(veilid_core::Target::RouteId(route_id), msg_bytes).await;
        }
    }
    let _ = hvoc_veilid::dht::close_record(&rc, route_key).await;
}
