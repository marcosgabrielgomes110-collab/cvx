//! Repository discovery, initialization and high-level operations.
//!
//! CVX repositories use the `.cvx/` control directory by default, but its
//! internal layout is the Git repository format. Existing `.git/` repositories
//! are opened directly in compatibility mode. That keeps the engine
//! interoperable without forcing users to convert their repositories or run
//! the `git` executable at runtime.

#![forbid(unsafe_code)]

mod add;
mod branch;
mod commit;
mod diff;
mod fsck;
mod gc;
mod hooks;
mod init;
mod log;
mod merge;
mod pick;
mod reachable;
mod remote;
mod repository;
mod reset;
mod restore;
mod stash;
mod status;
mod switch;
mod tag;
mod worktree_ops;

pub use add::{AddOptions, AddOutcome};
pub use branch::BranchInfo;
pub use commit::{CommitOptions, CommitOutcome};
pub use cvx_object::{HashAlgorithm, ObjectId, Signature};
pub use cvx_ref::Head;
pub use cvx_revision::LogEntry;
pub use diff::DiffOptions;
pub use fsck::{FsckOptions, FsckReport};
pub use gc::GcReport;
pub use init::{InitOptions, InitOutcome, InitStorage};
pub use log::LogOptions;
pub use merge::{MergeOptions, MergeOutcome};
pub use pick::{PickOptions, PickOutcome};
pub use remote::{CloneOptions, CloneOutcome, FetchOutcome, PushOptions, PushOutcome, RemoteInfo};
pub use repository::{CVX_DIR, GIT_DIR, Repository, RepositoryKind, TreeLeaf};
pub use reset::{ResetMode, ResetOptions, ResetOutcome};
pub use restore::{RestoreOptions, RestoreOutcome};
pub use stash::{StashEntry, StashOutcome, StashPushOptions};
pub use status::{
    ConflictEntry, ConflictKind, Status, StatusEntry, StatusKind, StatusOptions, UntrackedEntry,
};
pub use switch::{SwitchOptions, SwitchOutcome};
pub use tag::{TagOptions, TagOutcome};
