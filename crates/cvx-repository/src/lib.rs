//! Repository discovery, initialization and high-level operations.
//!
//! CVX repositories use the `.cvx/` control directory by default, but its
//! internal layout is the Git repository format. Existing `.git/` repositories
//! are opened directly in compatibility mode. That keeps the engine
//! interoperable without forcing users to convert their repositories or run
//! the `git` executable at runtime.

#![forbid(unsafe_code)]

mod init;
mod repository;

pub use init::{InitOptions, InitOutcome, InitStorage};
pub use repository::{CVX_DIR, GIT_DIR, Repository, RepositoryKind};
