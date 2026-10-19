//! Packfile, index and delta codecs.
//!
//! CVX reads version-2 pack indexes and version-2/3 packfiles, including
//! `ofs-delta` and `ref-delta` chains. The writer emits version-2 packfiles
//! with complete objects and a matching version-2 index.

#![forbid(unsafe_code)]

mod delta;
mod idx;
mod pack;
mod writer;

pub use delta::apply_delta;
pub use pack::{Pack, PackObject};
pub use writer::{PackInput, PackWriteOutcome, PackedObject, write_pack};
