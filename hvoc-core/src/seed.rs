//! Bootstrap thread seeds for first-run population.
//!
//! A deterministic "HVOC System" keypair is derived from a fixed seed.
//! Every node generates the same keypair, so the same content produces
//! the same object IDs and valid signatures.

use crate::{Identity, Thread, Post};
use crate::crypto::SigningKey;
use sha2::{Sha256, Digest};

const SYSTEM_SEED_PHRASE: &[u8] = b"hvoc-bootstrap-system-identity-v1";

/// Derive the deterministic system identity.
pub fn system_identity() -> Identity {
    let mut hasher = Sha256::new();
    hasher.update(SYSTEM_SEED_PHRASE);
    let seed_bytes: [u8; 32] = hasher.finalize().into();
    let signing_key = SigningKey::from_bytes(&seed_bytes);
    Identity {
        handle: "HVOC".to_string(),
        bio: Some("HVOC System — bootstrap identity for seed content".to_string()),
        signing_key,
    }
}

pub struct SeedThread {
    pub title: String,
    pub tags: Vec<String>,
    pub body: String,
}

pub struct SeedReply {
    pub thread_index: usize,
    pub parent_post_index: Option<usize>,
    pub body: String,
}

pub fn seed_threads() -> Vec<SeedThread> {
    vec![
        SeedThread {
            title: "Welcome to HVOC".to_string(),
            tags: vec!["meta".into(), "welcome".into(), "pinned".into()],
            body: concat!(
                "You're running HVOC \u{2014} HVCK Veilid Overlay Chat.\n\n",
                "This is a peer-to-peer forum and encrypted messaging app built on the Veilid network. ",
                "There are no servers. No accounts. No company between you and your conversations.\n\n",
                "What you need to know:\n\n",
                "\u{2192} Your identity is a cryptographic keypair, not a username/password. ",
                "Back up your data directory (~/.hvoc) \u{2014} if you lose it, your identity is gone.\n\n",
                "\u{2192} Everything you post is signed with your Ed25519 key and content-addressed. ",
                "Other nodes verify your posts independently.\n\n",
                "\u{2192} Direct messages use ephemeral X25519 ECDH + ChaCha20Poly1305 on top of Veilid\u{2019}s ",
                "onion-routed transport. Double-layered encryption.\n\n",
                "\u{2192} Content on the DHT is ephemeral. If no node keeps it alive, it ages out. ",
                "This is a feature, not a bug.\n\n",
                "The network gets more useful the more people are on it. Welcome aboard.",
            ).to_string(),
        },
        SeedThread {
            title: "Introductions".to_string(),
            tags: vec!["meta".into(), "community".into(), "pinned".into()],
            body: concat!(
                "Drop a post. Say hello. Share as much or as little as you want.\n\n",
                "Identity here is optional by design \u{2014} you\u{2019}re a keypair and a handle. ",
                "If you want to be anonymous, be anonymous. If you want to build a reputation, ",
                "your signing key is your proof of continuity.\n\n",
                "No pressure. Lurking is valid.",
            ).to_string(),
        },
        SeedThread {
            title: "General".to_string(),
            tags: vec!["general".into(), "discussion".into()],
            body: concat!(
                "Catch-all thread. Talk about whatever.\n\n",
                "Security, privacy, tech, music, projects, questions, links, rants. ",
                "Posts appear in the order they were signed.\n\n",
                "Keep it human.",
            ).to_string(),
        },
        SeedThread {
            title: "Network Status & Testing".to_string(),
            tags: vec!["meta".into(), "network".into(), "testing".into()],
            body: concat!(
                "Use this thread to verify your node is working.\n\n",
                "Post here and check if other nodes can see it. ",
                "This thread doubles as a network health canary.",
            ).to_string(),
        },
        SeedThread {
            title: "Help & Troubleshooting".to_string(),
            tags: vec!["meta".into(), "help".into(), "support".into()],
            body: concat!(
                "Node not connecting? Posts not propagating? DMs not arriving?\n\n",
                "Post your issue here. Include HVOC version, OS, Veilid node state, ",
                "and relevant log lines (RUST_LOG=hvoc=debug).",
            ).to_string(),
        },
        SeedThread {
            title: "Development & Contributions".to_string(),
            tags: vec!["dev".into(), "rust".into(), "veilid".into(), "contributing".into()],
            body: concat!(
                "HVOC is open source. Coordination thread for contributors.\n\n",
                "Architecture: hvoc-core (types) \u{2192} hvoc-veilid (network) \u{2192} ",
                "hvoc-store (SQLite) \u{2192} hvoc-api (HTTP/WS) \u{2192} hvoc-cli (entrypoint)\n\n",
                "Start with hvoc-core/src/canon.rs and hvoc-core/src/object.rs. ",
                "Everything else builds on these two files.",
            ).to_string(),
        },
        SeedThread {
            title: "Privacy & Security".to_string(),
            tags: vec!["security".into(), "privacy".into(), "cryptography".into()],
            body: concat!(
                "Threat models, protocol analysis, cryptographic design decisions.\n\n",
                "HVOC security: Ed25519 identity, content-addressed integrity, ",
                "ephemeral ECDH forward-secret DMs, Veilid onion routing.\n\n",
                "If you find a vulnerability, post here or DM the author.",
            ).to_string(),
        },
        SeedThread {
            title: "Lounge".to_string(),
            tags: vec!["offtopic".into(), "lounge".into(), "social".into()],
            body: concat!(
                "Not everything has to be about security or protocol design.\n\n",
                "Music, books, projects, interesting links, bad jokes, good coffee. ",
                "Be decent to each other.",
            ).to_string(),
        },
    ]
}

pub fn seed_replies() -> Vec<SeedReply> {
    vec![
        SeedReply {
            thread_index: 0,
            parent_post_index: Some(0),
            body: concat!(
                "Quick start commands:\n\n",
                "# Start the API server + web UI\n",
                "hvoc serve\n\n",
                "The API server binds to 127.0.0.1:7734 by default. ",
                "The web UI connects over HTTP and WebSocket for live updates.",
            ).to_string(),
        },
        SeedReply {
            thread_index: 0,
            parent_post_index: Some(0),
            body: concat!(
                "Important: back up your identity.\n\n",
                "Your identity lives in the encrypted local store. ",
                "There is no account recovery. There is no password reset. ",
                "You are the only custodian of your keys.",
            ).to_string(),
        },
    ]
}

pub struct MaterializedSeed {
    pub thread: Thread,
    pub posts: Vec<Post>,
}

/// Generate all seed content, signed by the deterministic system identity.
pub fn materialize_seeds(base_timestamp: i64) -> Vec<MaterializedSeed> {
    let system = system_identity();
    let threads = seed_threads();
    let replies = seed_replies();

    let mut materialized = Vec::with_capacity(threads.len());

    for (thread_idx, seed) in threads.into_iter().enumerate() {
        let thread_ts = base_timestamp + thread_idx as i64;
        let thread = Thread::create_with_timestamp(
            seed.title, seed.tags, &system, thread_ts,
        ).expect("seed thread creation must not fail");

        let mut posts = Vec::new();

        let op_ts = thread_ts + 1;
        let op = Post::create_with_timestamp(
            thread.object_id.clone(), None, seed.body, &system, op_ts,
        ).expect("seed post creation must not fail");
        posts.push(op);

        for (reply_offset, reply) in replies.iter()
            .filter(|r| r.thread_index == thread_idx)
            .enumerate()
        {
            let parent_id = reply.parent_post_index.map(|idx| {
                posts[idx].object_id.clone()
            });
            let reply_ts = op_ts + 1 + reply_offset as i64;
            let post = Post::create_with_timestamp(
                thread.object_id.clone(), parent_id, reply.body.clone(), &system, reply_ts,
            ).expect("seed reply creation must not fail");
            posts.push(post);
        }

        materialized.push(MaterializedSeed { thread, posts });
    }

    materialized
}

pub fn seed_thread_ids() -> Vec<String> {
    materialize_seeds(0).iter().map(|s| s.thread.object_id.clone()).collect()
}

pub fn is_system_author(author_id: &str) -> bool {
    system_identity().author_id() == author_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_identity_is_deterministic() {
        let a = system_identity();
        let b = system_identity();
        assert_eq!(a.author_id(), b.author_id());
    }

    #[test]
    fn seed_content_is_deterministic() {
        let a = materialize_seeds(0);
        let b = materialize_seeds(0);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.thread.object_id, y.thread.object_id);
            assert_eq!(x.posts.len(), y.posts.len());
            for (px, py) in x.posts.iter().zip(y.posts.iter()) {
                assert_eq!(px.object_id, py.object_id);
            }
        }
    }

    #[test]
    fn seed_threads_have_expected_count() {
        assert_eq!(materialize_seeds(0).len(), 8);
    }

    #[test]
    fn all_seed_object_ids_are_unique() {
        let seeds = materialize_seeds(0);
        let mut ids: Vec<&str> = Vec::new();
        for s in &seeds {
            ids.push(&s.thread.object_id);
            for p in &s.posts { ids.push(&p.object_id); }
        }
        let unique: std::collections::HashSet<&&str> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len());
    }
}
