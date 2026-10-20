//! Reading and writing the Git index.
//!
//! The index is the staging area between the worktree and the next commit. This
//! crate implements version 2 reads and writes, plus version 3 and 4 reads,
//! including merge stages and the extended flags used by skip-worktree and
//! intent-to-add entries.

#![forbid(unsafe_code)]

mod entry;
mod index;
mod lock;
#[cfg(test)]
mod proptests;

pub use entry::{
    IndexEntry, MODE_EXECUTABLE, MODE_FILE, MODE_GITLINK, MODE_SYMLINK, MODE_TREE, path_from_bytes,
    path_to_bytes, validate_path,
};
pub use index::Index;
pub use lock::IndexLock;
