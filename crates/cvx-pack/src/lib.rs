//! Packfile, index and delta codecs.
//!
//! CVX reads version-2 pack indexes and version-2/3 packfiles, including
//! `ofs-delta` and `ref-delta` chains. The writer emits version-2 packfiles
//! with complete objects and a matching version-2 index.

#![forbid(unsafe_code)]

mod checksum;
mod delta;
mod idx;
mod ingest;
mod pack;
#[cfg(test)]
mod proptests;
mod writer;

pub use delta::apply_delta;
pub use ingest::{IngestedObject, ingest_pack, ingest_pack_with_algorithm, ingest_pack_with_bases};
pub use pack::{Pack, PackObject};
pub use writer::{
    PackBase, PackInput, PackWriteOutcome, PackedObject, encode_pack,
    encode_pack_thin_with_algorithm, encode_pack_with_algorithm, write_pack,
    write_pack_thin_with_algorithm, write_pack_with_algorithm,
};
