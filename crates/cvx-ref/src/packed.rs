use std::collections::BTreeMap;
use std::path::Path;

use cvx_core::{Error, Result};
use cvx_object::ObjectId;

/// Reads `.git/packed-refs` (or `.cvx/packed-refs`).
///
/// Peeled tag lines are ignored for now; annotated tag peeling belongs to the
/// revision layer. Loose references always take precedence over packed ones.
pub(crate) fn read_packed_refs(git_dir: &Path) -> Result<BTreeMap<String, ObjectId>> {
    let path = git_dir.join("packed-refs");
    if !path.exists() {
        return Ok(BTreeMap::new());
    }

    let contents = cvx_core::fs::read_to_string(&path)?;
    let mut refs = BTreeMap::new();

    for (index, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('^') {
            continue;
        }

        let mut fields = line.split_whitespace();
        let Some(oid) = fields.next() else {
            continue;
        };
        let Some(name) = fields.next() else {
            return Err(Error::corrupt(format!(
                "{}:{}: packed reference is missing a name",
                path.display(),
                index + 1
            )));
        };
        if fields.next().is_some() {
            return Err(Error::corrupt(format!(
                "{}:{}: unexpected fields in packed reference",
                path.display(),
                index + 1
            )));
        }

        let oid = ObjectId::from_hex(oid).map_err(|error| {
            Error::corrupt(format!(
                "{}:{}: invalid object id: {error}",
                path.display(),
                index + 1
            ))
        })?;
        cvx_core::refname::validate_ref_name(name).map_err(|_| {
            Error::corrupt(format!(
                "{}:{}: invalid reference name: {name}",
                path.display(),
                index + 1
            ))
        })?;

        refs.insert(name.to_owned(), oid);
    }

    Ok(refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_packed_refs() {
        let temp = tempfile::tempdir().expect("tempdir");
        let head = "1111111111111111111111111111111111111111";
        let tag = "2222222222222222222222222222222222222222";
        std::fs::write(
            temp.path().join("packed-refs"),
            format!(
                "# pack-refs with: peeled fully-peeled sorted\n{head} refs/heads/main\n{tag} refs/tags/v1\n^3333333333333333333333333333333333333333\n"
            ),
        )
        .expect("write");

        let refs = read_packed_refs(temp.path()).expect("read");
        assert_eq!(
            refs.get("refs/heads/main"),
            Some(&ObjectId::from_hex(head).unwrap())
        );
        assert_eq!(
            refs.get("refs/tags/v1"),
            Some(&ObjectId::from_hex(tag).unwrap())
        );
    }

    #[test]
    fn rejects_malformed_packed_refs() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("packed-refs"),
            "not-an-oid refs/heads/main\n",
        )
        .expect("write");

        assert!(read_packed_refs(temp.path()).is_err());
    }
}
