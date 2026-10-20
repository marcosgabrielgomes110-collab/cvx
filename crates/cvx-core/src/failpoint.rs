//! Test-only failure injection.
//!
//! Set `CVX_FAILPOINT` to a comma-separated list of failpoint names to make the
//! corresponding operation fail with an injected error. Production runs leave
//! the variable unset, so this module has no effect.
//!
//! Failpoints are intentionally cheap: one environment lookup per hit. They are
//! used by crash-recovery tests to verify that interrupted writes leave a
//! consistent repository.

use std::env;

use crate::{Error, Result};

/// Returns whether a failpoint is enabled.
pub fn enabled(name: &str) -> bool {
    let Some(value) = env::var_os("CVX_FAILPOINT") else {
        return false;
    };
    let value = value.to_string_lossy();
    value
        .split(',')
        .map(str::trim)
        .any(|candidate| candidate == name)
}

/// Fails with an injected error when the named failpoint is enabled.
pub fn hit(name: &str) -> Result<()> {
    if enabled(name) {
        Err(Error::Message(format!(
            "injected failure at failpoint '{name}'"
        )))
    } else {
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    // Environment mutation is process-global and unsafe to do in parallel
    // tests, so the environment path is exercised by CLI integration tests.
    #[test]
    fn disabled_by_default() {
        assert!(!enabled("nonexistent.failpoint"));
        assert!(hit("nonexistent.failpoint").is_ok());
    }
}
