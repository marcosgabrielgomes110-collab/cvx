//! Packfile, index and delta codecs.
//!
//! CVX reads version-2 pack indexes and version-2/3 packfiles, including
//! `ofs-delta` and `ref-delta` chains. Writes are introduced with the transport
//! milestone; the local engine creates loose objects.

#![forbid(unsafe_code)]

mod delta;
mod idx;
mod pack;

pub use delta::apply_delta;
pub use pack::{Pack, PackObject};
