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
mod init;
mod log;
mod repository;
mod restore;
mod status;
mod switch;
mod worktree_ops;

pub use add::{AddOptions, AddOutcome};
pub use branch::BranchInfo;
pub use commit::{CommitOptions, CommitOutcome};
pub use cvx_object::{ObjectId, Signature};
pub use cvx_ref::Head;
pub use cvx_revision::LogEntry;
pub use diff::DiffOptions;
pub use init::{InitOptions, InitOutcome, InitStorage};
pub use log::LogOptions;
pub use repository::{CVX_DIR, GIT_DIR, Repository, RepositoryKind, TreeLeaf};
pub use restore::{RestoreOptions, RestoreOutcome};
pub use status::{Status, StatusEntry, StatusKind, StatusOptions, UntrackedEntry};
pub use switch::{SwitchOptions, SwitchOutcome};
