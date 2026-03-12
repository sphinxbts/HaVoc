//! Local identity for signing objects.

use crate::crypto::{self, SigningKey};
use crate::AuthorId;

pub struct Identity {
    pub handle: String,
    pub bio: Option<String>,
    pub signing_key: SigningKey,
}

impl Identity {
    /// The hex-encoded public key used as author_id.
    pub fn author_id(&self) -> AuthorId {
        crypto::author_id(&self.signing_key)
    }
}
