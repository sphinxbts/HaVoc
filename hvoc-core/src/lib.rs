pub mod canon;
pub mod crypto;
pub mod error;
pub mod identity;
pub mod object;
pub mod seed;

pub use error::CoreError;
pub use identity::Identity;
pub use object::*;

/// Opaque author identifier — the string encoding of a Veilid public key.
pub type AuthorId = String;

/// Content-addressed object identifier — hex-encoded BLAKE3 hash.
pub type ObjectId = String;
