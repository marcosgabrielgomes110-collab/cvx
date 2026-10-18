//! Shared primitives for the CVX engine.
//!
//! This crate deliberately has no knowledge of the object database, the index,
//! references, or GitHub. It contains the pieces every other layer needs:
//! errors, filesystem helpers, path and reference validation, and process-wide
//! conventions.

#![forbid(unsafe_code)]

pub mod error;
pub mod fs;
pub mod refname;

pub use error::{Error, IoResultExt, Result};
