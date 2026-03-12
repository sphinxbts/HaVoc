//! First-run seed population.
//!
//! Inserts deterministic seed threads and posts so the UI has content
//! to display immediately on first boot.

use crate::{Store, StoreError, ThreadRepo, PostRepo};
use hvoc_core::seed::{materialize_seeds, is_system_author};
use tracing::{info, warn};

const BOOTSTRAP_VERSION_KEY: &str = "bootstrap_version";
const CURRENT_BOOTSTRAP_VERSION: i32 = 1;

/// Check whether the store needs bootstrapping and populate if so.
pub async fn bootstrap_if_needed(store: &Store) -> Result<(), StoreError> {
    let current = get_bootstrap_version(store).await?;
    if current >= CURRENT_BOOTSTRAP_VERSION {
        return Ok(());
    }

    info!(from = current, to = CURRENT_BOOTSTRAP_VERSION, "bootstrapping seed content");

    let seeds = materialize_seeds(0);
    let thread_repo = ThreadRepo(store);
    let post_repo = PostRepo(store);

    let mut thread_count = 0;
    let mut post_count = 0;

    for seed in &seeds {
        match thread_repo.insert(&seed.thread).await {
            Ok(_) => thread_count += 1,
            Err(e) => {
                warn!(thread_id = %seed.thread.object_id, error = %e, "seed thread insert failed");
            }
        }

        for post in &seed.posts {
            match post_repo.insert(post).await {
                Ok(_) => post_count += 1,
                Err(e) => {
                    warn!(post_id = %post.object_id, error = %e, "seed post insert failed");
                }
            }
        }
    }

    set_bootstrap_version(store, CURRENT_BOOTSTRAP_VERSION).await?;
    info!(threads = thread_count, posts = post_count, "bootstrap complete");
    Ok(())
}

async fn get_bootstrap_version(store: &Store) -> Result<i32, StoreError> {
    let conn = store.conn().clone();
    let key = BOOTSTRAP_VERSION_KEY.to_string();
    tokio::task::spawn_blocking(move || {
        let conn = conn.blocking_lock();
        let mut stmt = conn.prepare("SELECT value FROM metadata WHERE key = ?1")?;
        let result: Result<String, _> = stmt.query_row(rusqlite::params![key], |row| row.get(0));
        match result {
            Ok(v) => Ok(v.parse::<i32>().unwrap_or(0)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
            Err(e) => Err(StoreError::Db(e)),
        }
    })
    .await
    .map_err(|e| StoreError::Task(e.to_string()))?
}

async fn set_bootstrap_version(store: &Store, version: i32) -> Result<(), StoreError> {
    let conn = store.conn().clone();
    let key = BOOTSTRAP_VERSION_KEY.to_string();
    let val = version.to_string();
    tokio::task::spawn_blocking(move || {
        let conn = conn.blocking_lock();
        conn.execute(
            "INSERT OR REPLACE INTO metadata (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, val],
        )?;
        Ok::<_, StoreError>(())
    })
    .await
    .map_err(|e| StoreError::Task(e.to_string()))?
}

/// Returns true if the given author_id is the HVOC system bootstrap identity.
pub fn is_system_post(author_id: &str) -> bool {
    is_system_author(author_id)
}
