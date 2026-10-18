//! Object database access for loose objects and packfiles.
//!
//! Reads check loose objects first and then version-2 pack indexes. Writes
//! create verified loose objects; pack writing belongs to the transport
//! milestone.

#![forbid(unsafe_code)]

mod loose;
mod store;

pub use loose::{LooseStore, StoredObject};
pub use store::ObjectStore;
