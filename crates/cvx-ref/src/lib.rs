//! References, `HEAD`, packed refs and reflogs.
//!
//! The first milestone implements the read paths needed by discovery and
//! initialization plus safe writes of loose references. Reference
//! transactions, compare-and-swap updates and reflog append are added together
//! with `cvx commit`.

#![forbid(unsafe_code)]

mod head;
mod packed;
mod store;

pub use head::Head;
pub use store::RefStore;
