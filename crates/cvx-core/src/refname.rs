use crate::error::{Error, Result};

/// Validates a full reference name such as `refs/heads/main`.
///
/// The rules follow `git check-ref-format` for full references. `HEAD` is not
/// a normal reference and is rejected by this function.
pub fn validate_ref_name(name: &str) -> Result<()> {
    let invalid = || Err(Error::invalid_ref(name));

    if name.is_empty() || name == "@" || !name.contains('/') {
        return invalid();
    }
    if name.starts_with('/') || name.ends_with('/') || name.contains("//") {
        return invalid();
    }
    if name.ends_with('.') || name.contains("..") || name.contains("@{") {
        return invalid();
    }

    for byte in name.bytes() {
        if byte < 0x20
            || byte == 0x7f
            || matches!(byte, b' ' | b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\')
        {
            return invalid();
        }
    }

    for component in name.split('/') {
        if component.is_empty() || component.starts_with('.') || component.ends_with(".lock") {
            return invalid();
        }
    }

    Ok(())
}

/// Validates a branch short name, building the full `refs/heads/<name>` form.
pub fn validate_branch_name(name: &str) -> Result<()> {
    if name.is_empty() || name.starts_with('-') {
        return Err(Error::invalid_ref(format!("refs/heads/{name}")));
    }
    validate_ref_name(&format!("refs/heads/{name}"))
}

/// Returns whether a full reference name is valid.
pub fn is_valid_ref_name(name: &str) -> bool {
    validate_ref_name(name).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_references() {
        for name in [
            "refs/heads/main",
            "refs/heads/feature/auth",
            "refs/tags/v0.4.0",
            "refs/remotes/origin/main",
            "refs/heads/-dash",
            "refs/heads/foo@bar",
        ] {
            assert!(is_valid_ref_name(name), "{name} should be valid");
        }
    }

    #[test]
    fn rejects_invalid_references() {
        for name in [
            "",
            "HEAD",
            "@",
            "refs/heads/",
            "/refs/heads/main",
            "refs//heads/main",
            "refs/heads/main/",
            "refs/heads/.hidden",
            "refs/heads/main.",
            "refs/heads/foo..bar",
            "refs/heads/foo.lock",
            "refs/heads/foo.lock/bar",
            "refs/heads/foo bar",
            "refs/heads/foo~bar",
            "refs/heads/foo^bar",
            "refs/heads/foo:bar",
            "refs/heads/foo?bar",
            "refs/heads/foo*bar",
            "refs/heads/foo[bar",
            "refs/heads/foo\\bar",
            "refs/heads/foo@{bar",
            "refs/heads/foo\nbar",
        ] {
            assert!(!is_valid_ref_name(name), "{name:?} should be invalid");
        }
    }

    #[test]
    fn validates_branch_short_names() {
        assert!(validate_branch_name("main").is_ok());
        assert!(validate_branch_name("feature/auth").is_ok());
        assert!(validate_branch_name("").is_err());
        assert!(validate_branch_name("-dash").is_err());
        assert!(validate_branch_name("bad..name").is_err());
        assert!(validate_branch_name("bad name").is_err());
    }
}
