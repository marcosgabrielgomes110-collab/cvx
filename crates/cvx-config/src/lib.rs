//! Git configuration parsing and lookup.
//!
//! CVX reads the same `gitconfig` files as Git so that identity, branch
//! conventions and remote settings remain shared during the compatibility
//! phase. The parser implements the common Git syntax, including sections,
//! subsections, quoting, comments, booleans and relative includes.

#![forbid(unsafe_code)]

mod loader;
mod parser;

pub use loader::{load_global, load_system};
pub use parser::{Config, Entry};
