//! Lightweight Ed25519 signing for deterministic seed content.
//!
//! This module provides Ed25519 signing without requiring a running Veilid node.
//! Used by the seed/bootstrap system to generate identical content on every node.

pub use ed25519_dalek::SigningKey;
pub use ed25519_dalek::VerifyingKey;

use crate::AuthorId;

/// Sign bytes with an Ed25519 signing key, returning raw 64-byte signature.
pub fn sign(key: &SigningKey, data: &[u8]) -> Vec<u8> {
    use ed25519_dalek::Signer;
    key.sign(data).to_bytes().to_vec()
}

/// Derive the author_id (hex-encoded public key) from a signing key.
pub fn author_id(key: &SigningKey) -> AuthorId {
    hex::encode(key.verifying_key().as_bytes())
}
