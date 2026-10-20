//! Git object identifiers, kinds, hashing and the object types needed by the
//! local engine.
//!
//! The first milestone supports SHA-1 repositories. Object identifiers are
//! fixed-width and the hash algorithm is explicit so a future SHA-256
//! implementation cannot accidentally mix formats.

#![forbid(unsafe_code)]

mod commit;
mod hash;
mod id;
mod kind;
#[cfg(test)]
mod proptests;
mod signature;
mod tag;
mod tree;

pub use commit::Commit;
pub use hash::{HashAlgorithm, UnsupportedHashAlgorithm, hash_object};
pub use id::{ObjectId, ObjectIdError, SHA1_BYTES, SHA1_HEX_LEN};
pub use kind::ObjectKind;
pub use signature::{Signature, parse_timezone};
pub use tag::Tag;
pub use tree::{
    MODE_EXECUTABLE, MODE_FILE, MODE_GITLINK, MODE_SYMLINK, MODE_TREE, Tree, TreeEntry,
};
