use std::sync::Arc;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use crate::AppState;

#[derive(Deserialize)]
pub struct CreateThreadRequest {
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// "public" (default, added to board index) or "private" (invite-only).
    #[serde(default = "default_visibility")]
    pub visibility: String,
}

fn default_visibility() -> String {
    "public".to_string()
}

#[derive(Deserialize)]
pub struct CreatePostRequest {
    pub body: String,
    pub parent_id: Option<String>,
    /// JSON string with attachment metadata (hash, filename, mime, size).
    pub attachment_meta: Option<String>,
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub q: Option<String>,
    /// Filter: "public" (default), "private", or "all".
    pub visibility: Option<String>,
}

fn default_limit() -> i64 {
    50
}

pub async fn list_threads(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListQuery>,
) -> Json<serde_json::Value> {
    let repo = hvoc_store::ThreadRepo(&state.store);
    let vis = q.visibility.as_deref().unwrap_or("public");

    let result = if let Some(ref query) = q.q {
        repo.search(query, q.limit).await
    } else if vis == "private" {
        repo.list_private(q.limit, q.offset).await
    } else {
        repo.list(q.limit, q.offset).await
    };
    match result {
        Ok(threads) => Json(serde_json::json!({ "threads": threads })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

/// Get the invite link for a thread (its DHT key for sharing).
pub async fn get_thread_invite(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
) -> Json<serde_json::Value> {
    let ks = hvoc_store::Keystore(&state.store);
    let logical_key = format!("thread:{}", thread_id);
    match ks.get_dht_key(&logical_key).await {
        Ok(Some((record_key, _))) => {
            let invite = format!("hvoc-thread:{}", record_key);
            Json(serde_json::json!({
                "thread_id": thread_id,
                "dht_key": record_key,
                "invite": invite,
            }))
        }
        Ok(None) => Json(serde_json::json!({ "error": "thread not published to DHT" })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
pub struct JoinThreadRequest {
    /// The DHT record key or invite string (hvoc-thread:VLD0:...).
    pub invite: String,
}

/// Join a private thread by its DHT key / invite link.
pub async fn join_thread(
    State(state): State<Arc<AppState>>,
    Json(req): Json<JoinThreadRequest>,
) -> Json<serde_json::Value> {
    // Parse invite string.
    let dht_key_str = req.invite
        .strip_prefix("hvoc-thread:")
        .unwrap_or(&req.invite)
        .to_string();

    let record_key = match dht_key_str.parse::<veilid_core::RecordKey>() {
        Ok(k) => k,
        Err(e) => return Json(serde_json::json!({ "error": format!("invalid DHT key: {e}") })),
    };

    let rc = match state.node.routing_context() {
        Ok(rc) => rc,
        Err(e) => return Json(serde_json::json!({ "error": e.to_string() })),
    };

    // Open the thread's DHT record (read-only since we're not the author).
    if let Err(e) = hvoc_veilid::dht::open_record_readonly(&rc, record_key.clone()).await {
        return Json(serde_json::json!({ "error": format!("cannot open thread: {e}") }));
    }

    // Fetch the thread header from subkey 0.
    let thread = match hvoc_veilid::dht::get_value(&rc, record_key.clone(), 0, true).await {
        Ok(Some(data)) => match serde_json::from_slice::<hvoc_core::Thread>(&data) {
            Ok(t) => t,
            Err(e) => {
                let _ = hvoc_veilid::dht::close_record(&rc, record_key).await;
                return Json(serde_json::json!({ "error": format!("invalid thread data: {e}") }));
            }
        },
        Ok(None) => {
            let _ = hvoc_veilid::dht::close_record(&rc, record_key).await;
            return Json(serde_json::json!({ "error": "thread has no data" }));
        }
        Err(e) => {
            let _ = hvoc_veilid::dht::close_record(&rc, record_key).await;
            return Json(serde_json::json!({ "error": format!("DHT fetch failed: {e}") }));
        }
    };

    // Store locally as private.
    let thread_repo = hvoc_store::ThreadRepo(&state.store);
    if let Err(e) = thread_repo.insert_with_visibility(&thread, "private").await {
        let _ = hvoc_veilid::dht::close_record(&rc, record_key).await;
        return Json(serde_json::json!({ "error": e.to_string() }));
    }

    // Save the DHT key mapping.
    let ks = hvoc_store::Keystore(&state.store);
    let logical_key = format!("thread:{}", thread.object_id);
    let _ = ks.save_dht_key(&logical_key, &dht_key_str, None).await;

    // Fetch posts from subkey 1 (post index).
    if let Ok(Some(index_data)) = hvoc_veilid::dht::get_value(&rc, record_key.clone(), 1, true).await {
        if let Ok(_post_ids) = serde_json::from_slice::<Vec<String>>(&index_data) {
            let _ = thread_repo.increment_post_count(&thread.object_id, chrono::Utc::now().timestamp()).await;
        }
    }

    let _ = hvoc_veilid::dht::close_record(&rc, record_key).await;

    tracing::info!("Joined private thread: {} ({})", thread.title, thread.object_id);

    Json(serde_json::json!({
        "status": "joined",
        "thread_id": thread.object_id,
        "title": thread.title,
    }))
}

pub async fn get_thread(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let repo = hvoc_store::ThreadRepo(&state.store);
    match repo.get(&id).await {
        Ok(thread) => Json(serde_json::json!({ "thread": thread })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

pub async fn create_thread(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateThreadRequest>,
) -> Json<serde_json::Value> {
    let kp_guard = state.keypair.read().await;
    let kp = match kp_guard.as_ref() {
        Some(kp) => kp.clone(),
        None => return Json(serde_json::json!({ "error": "no active identity" })),
    };
    drop(kp_guard);

    // Create signed thread + opening post.
    let result = state.node.with_crypto(|cs| -> Result<(hvoc_core::Thread, hvoc_core::Post), hvoc_veilid::VeilidError> {
        let thread = hvoc_veilid::crypto::create_thread(cs, &kp, &req.title, req.tags.clone())?;
        let post = hvoc_veilid::crypto::create_post(cs, &kp, &thread.object_id, None, &req.body)?;
        Ok((thread, post))
    });

    let (thread, post) = match result {
        Ok(v) => v,
        Err(e) => return Json(serde_json::json!({ "error": e.to_string() })),
    };

    // Persist locally.
    let thread_repo = hvoc_store::ThreadRepo(&state.store);
    let post_repo = hvoc_store::PostRepo(&state.store);

    let visibility = if req.visibility == "private" { "private" } else { "public" };
    if let Err(e) = thread_repo.insert_with_visibility(&thread, visibility).await {
        return Json(serde_json::json!({ "error": e.to_string() }));
    }
    if let Err(e) = post_repo.insert(&post).await {
        return Json(serde_json::json!({ "error": e.to_string() }));
    }
    if let Err(e) = thread_repo
        .increment_post_count(&thread.object_id, post.created_at)
        .await
    {
        return Json(serde_json::json!({ "error": e.to_string() }));
    }

    // Publish thread + post to DHT (and register in board index if public).
    let thread_id = thread.object_id.clone();
    let post_id = post.object_id.clone();
    let state_bg = state.clone();
    let thread_bg = thread.clone();
    let post_bg = post.clone();
    let is_public = visibility == "public";
    tokio::spawn(async move {
        publish_to_dht(&state_bg, &thread_bg, &post_bg, is_public).await;
    });

    Json(serde_json::json!({
        "thread_id": thread_id,
        "post_id": post_id,
        "visibility": visibility,
    }))
}

pub async fn list_posts(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
) -> Json<serde_json::Value> {
    let repo = hvoc_store::PostRepo(&state.store);
    match repo.list_for_thread(&thread_id).await {
        Ok(posts) => Json(serde_json::json!({ "posts": posts })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

pub async fn create_post(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Json(req): Json<CreatePostRequest>,
) -> Json<serde_json::Value> {
    let kp_guard = state.keypair.read().await;
    let kp = match kp_guard.as_ref() {
        Some(kp) => kp.clone(),
        None => return Json(serde_json::json!({ "error": "no active identity" })),
    };
    drop(kp_guard);

    let result = state.node.with_crypto(|cs| -> Result<hvoc_core::Post, hvoc_veilid::VeilidError> {
        hvoc_veilid::crypto::create_post(cs, &kp, &thread_id, req.parent_id.as_deref(), &req.body)
    });

    let post = match result {
        Ok(p) => p,
        Err(e) => return Json(serde_json::json!({ "error": e.to_string() })),
    };

    let post_repo = hvoc_store::PostRepo(&state.store);
    if let Err(e) = post_repo.insert_with_attachment(&post, req.attachment_meta).await {
        return Json(serde_json::json!({ "error": e.to_string() }));
    }

    let thread_repo = hvoc_store::ThreadRepo(&state.store);
    let _ = thread_repo
        .increment_post_count(&thread_id, post.created_at)
        .await;

    // Publish post to DHT.
    let post_id = post.object_id.clone();
    publish_post_to_dht(&state, &thread_id, &post).await;

    Json(serde_json::json!({ "post_id": post_id }))
}

/// Delete (tombstone) a post. Only the author can delete their own posts.
pub async fn delete_post(
    State(state): State<Arc<AppState>>,
    Path(post_id): Path<String>,
) -> Json<serde_json::Value> {
    let kp_guard = state.keypair.read().await;
    let kp = match kp_guard.as_ref() {
        Some(kp) => kp.clone(),
        None => return Json(serde_json::json!({ "error": "no active identity" })),
    };
    drop(kp_guard);

    // Verify the post exists and belongs to this user.
    let post_repo = hvoc_store::PostRepo(&state.store);
    let post = match post_repo.get(&post_id).await {
        Ok(p) => p,
        Err(_) => return Json(serde_json::json!({ "error": "post not found" })),
    };

    let my_id = hvoc_veilid::crypto::author_id_from_key(&kp.key());
    if post.author_id != my_id {
        return Json(serde_json::json!({ "error": "can only delete your own posts" }));
    }

    let tombstone = match state.node.with_crypto(|cs| {
        hvoc_veilid::crypto::create_tombstone(cs, &kp, &post_id, Some("deleted by author"))
    }) {
        Ok(t) => t,
        Err(e) => return Json(serde_json::json!({ "error": e.to_string() })),
    };

    let tomb_repo = hvoc_store::TombstoneRepo(&state.store);
    if let Err(e) = tomb_repo.insert(&tombstone).await {
        return Json(serde_json::json!({ "error": e.to_string() }));
    }

    Json(serde_json::json!({ "status": "deleted", "tombstone_id": tombstone.object_id }))
}

/// Delete (tombstone) a thread. Only the author can delete their own threads.
pub async fn delete_thread(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
) -> Json<serde_json::Value> {
    let kp_guard = state.keypair.read().await;
    let kp = match kp_guard.as_ref() {
        Some(kp) => kp.clone(),
        None => return Json(serde_json::json!({ "error": "no active identity" })),
    };
    drop(kp_guard);

    let thread_repo = hvoc_store::ThreadRepo(&state.store);
    let thread = match thread_repo.get(&thread_id).await {
        Ok(t) => t,
        Err(_) => return Json(serde_json::json!({ "error": "thread not found" })),
    };

    let my_id = hvoc_veilid::crypto::author_id_from_key(&kp.key());
    if thread.author_id != my_id {
        return Json(serde_json::json!({ "error": "can only delete your own threads" }));
    }

    let tombstone = match state.node.with_crypto(|cs| {
        hvoc_veilid::crypto::create_tombstone(cs, &kp, &thread_id, Some("deleted by author"))
    }) {
        Ok(t) => t,
        Err(e) => return Json(serde_json::json!({ "error": e.to_string() })),
    };

    // Mark thread as tombstoned by removing from threads.
    let thread_repo2 = hvoc_store::ThreadRepo(&state.store);
    let _ = thread_repo2.delete(&thread_id).await;

    let tomb_repo = hvoc_store::TombstoneRepo(&state.store);
    let _ = tomb_repo.insert(&tombstone).await;

    Json(serde_json::json!({ "status": "deleted", "tombstone_id": tombstone.object_id }))
}

/// Publish a thread + opening post to DHT (best-effort, non-blocking on failure).
/// If `register_in_board` is true, the thread is added to the public board index.
async fn publish_to_dht(state: &AppState, thread: &hvoc_core::Thread, post: &hvoc_core::Post, register_in_board: bool) {
    let rc = match state.node.routing_context() {
        Ok(rc) => rc,
        Err(e) => {
            tracing::warn!("Failed to get routing context for DHT publish: {e}");
            return;
        }
    };

    // Create a DHT record for the thread (DFLT(2): header + post index).
    let schema = veilid_core::DHTSchema::dflt(2).unwrap();
    match hvoc_veilid::dht::create_record(&rc, schema, None).await {
        Ok((record_key, owner_kp)) => {
            let ks = hvoc_store::Keystore(&state.store);
            let logical_key = format!("thread:{}", thread.object_id);
            let owner_secret = owner_kp.map(|kp| kp.to_string());
            let _ = ks.save_dht_key(&logical_key, &record_key.to_string(), owner_secret.as_deref()).await;

            // Write thread header to subkey 0.
            if let Ok(json) = serde_json::to_vec(thread) {
                if let Err(e) = hvoc_veilid::dht::publish_thread_header(&rc, record_key.clone(), &json).await {
                    tracing::warn!("Failed to publish thread header: {e}");
                }
            }

            // Write initial post index to subkey 1.
            let index = serde_json::json!([post.object_id]);
            if let Ok(json) = serde_json::to_vec(&index) {
                if let Err(e) = hvoc_veilid::dht::update_thread_index(&rc, record_key.clone(), &json).await {
                    tracing::warn!("Failed to publish thread index: {e}");
                }
            }

            let rk_str = record_key.to_string();
            let _ = hvoc_veilid::dht::close_record(&rc, record_key).await;

            // Register in board index for discovery (public threads only).
            if register_in_board {
                super::sync::register_thread_in_board(state, &thread.object_id, &rk_str).await;
            }
        }
        Err(e) => tracing::warn!("Failed to create thread DHT record: {e}"),
    }

    // Create a separate DHT record for the post.
    let post_schema = veilid_core::DHTSchema::dflt(1).unwrap();
    match hvoc_veilid::dht::create_record(&rc, post_schema, None).await {
        Ok((record_key, owner_kp)) => {
            let ks = hvoc_store::Keystore(&state.store);
            let logical_key = format!("post:{}", post.object_id);
            let owner_secret = owner_kp.map(|kp| kp.to_string());
            let _ = ks.save_dht_key(&logical_key, &record_key.to_string(), owner_secret.as_deref()).await;

            if let Ok(json) = serde_json::to_vec(post) {
                if let Err(e) = hvoc_veilid::dht::publish_post(&rc, record_key.clone(), &json).await {
                    tracing::warn!("Failed to publish post: {e}");
                }
            }

            let _ = hvoc_veilid::dht::close_record(&rc, record_key).await;
        }
        Err(e) => tracing::warn!("Failed to create post DHT record: {e}"),
    }
}

/// Publish a reply post to DHT and update the thread's post index.
async fn publish_post_to_dht(state: &AppState, thread_id: &str, post: &hvoc_core::Post) {
    let rc = match state.node.routing_context() {
        Ok(rc) => rc,
        Err(e) => {
            tracing::warn!("Failed to get routing context for DHT publish: {e}");
            return;
        }
    };

    // Create DHT record for the post.
    let schema = veilid_core::DHTSchema::dflt(1).unwrap();
    match hvoc_veilid::dht::create_record(&rc, schema, None).await {
        Ok((record_key, owner_kp)) => {
            let ks = hvoc_store::Keystore(&state.store);
            let logical_key = format!("post:{}", post.object_id);
            let owner_secret = owner_kp.map(|kp| kp.to_string());
            let _ = ks.save_dht_key(&logical_key, &record_key.to_string(), owner_secret.as_deref()).await;

            if let Ok(json) = serde_json::to_vec(post) {
                if let Err(e) = hvoc_veilid::dht::publish_post(&rc, record_key.clone(), &json).await {
                    tracing::warn!("Failed to publish post: {e}");
                }
            }

            let _ = hvoc_veilid::dht::close_record(&rc, record_key).await;
        }
        Err(e) => tracing::warn!("Failed to create post DHT record: {e}"),
    }

    // Update the thread's post index.
    let ks = hvoc_store::Keystore(&state.store);
    let thread_logical = format!("thread:{}", thread_id);
    if let Ok(Some((record_key_str, owner_secret))) = ks.get_dht_key(&thread_logical).await {
        if let Ok(record_key) = record_key_str.parse::<veilid_core::RecordKey>() {
            // Open for writing if we have the owner secret.
            if let Some(ref secret_str) = owner_secret {
                if let Ok(writer) = secret_str.parse::<veilid_core::KeyPair>() {
                    let _ = hvoc_veilid::dht::open_record_writable(&rc, record_key.clone(), writer).await;
                }
            }

            // Read current index, append new post ID.
            if let Ok(Some(current)) = hvoc_veilid::dht::get_value(&rc, record_key.clone(), 1, false).await {
                if let Ok(mut ids) = serde_json::from_slice::<Vec<String>>(&current) {
                    ids.push(post.object_id.clone());
                    if let Ok(json) = serde_json::to_vec(&ids) {
                        let _ = hvoc_veilid::dht::update_thread_index(&rc, record_key.clone(), &json).await;
                    }
                }
            }

            let _ = hvoc_veilid::dht::close_record(&rc, record_key).await;
        }
    }
}
