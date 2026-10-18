//! Git object identifiers, kinds and hashing.
//!
//! The first milestone supports SHA-1 repositories. Object identifiers are
//! fixed-width and the hash algorithm is explicit so a future SHA-256
//! implementation cannot accidentally mix formats.

#![forbid(unsafe_code)]

mod hash;
mod id;
mod kind;

pub use hash::{HashAlgorithm, UnsupportedHashAlgorithm, hash_object};
pub use id::{ObjectId, ObjectIdError, SHA1_BYTES, SHA1_HEX_LEN};
pub use kind::ObjectKind;
