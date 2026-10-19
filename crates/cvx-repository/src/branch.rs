use cvx_core::{Error, Result};
use cvx_object::ObjectId;

use crate::repository::Repository;

/// A local branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInfo {
    /// Short branch name.
    pub name: String,
    /// Commit at the branch tip.
    pub oid: ObjectId,
    /// Whether this is the current branch.
    pub current: bool,
}

impl Repository {
    /// Lists local branches in name order.
    pub fn branches(&self) -> Result<Vec<BranchInfo>> {
        let current = self.head()?.target().map(str::to_owned);
        let mut branches = Vec::new();

        for (reference, oid) in self.refs().list_refs("refs/heads")? {
            let Some(name) = reference.strip_prefix("refs/heads/") else {
                continue;
            };
            branches.push(BranchInfo {
                name: name.to_owned(),
                oid,
                current: current.as_deref() == Some(reference.as_str()),
            });
        }

        branches.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(branches)
    }

    /// Creates a local branch.
    pub fn create_branch(&self, name: &str, start_point: Option<&str>) -> Result<ObjectId> {
        cvx_core::refname::validate_branch_name(name)?;
        let reference = format!("refs/heads/{name}");

        if self.refs().read_ref(&reference)?.is_some() {
            return Err(Error::Message(format!("branch '{name}' already exists")));
        }

        let oid = match start_point {
            Some(revision) => self.resolve_revision(revision)?,
            None => self.head()?.oid().ok_or_else(|| {
                Error::Message("cannot create a branch before the first commit".to_owned())
            })?,
        };
        self.read_commit(oid)?;
        self.refs().update_ref(&reference, oid, None)?;
        Ok(oid)
    }

    /// Deletes a local branch.
    pub fn delete_branch(&self, name: &str, force: bool) -> Result<()> {
        cvx_core::refname::validate_branch_name(name)?;
        let reference = format!("refs/heads/{name}");

        if self.head()?.target() == Some(reference.as_str()) {
            return Err(Error::Message(format!(
                "cannot delete branch '{name}': it is currently checked out"
            )));
        }

        let Some(oid) = self.refs().read_ref(&reference)? else {
            return Err(Error::Message(format!("branch '{name}' not found")));
        };

        if !force {
            let Some(head) = self.head()?.oid() else {
                return Err(Error::Message(format!(
                    "cannot delete branch '{name}': HEAD has no commits"
                )));
            };
            if !cvx_revision::is_ancestor(oid, head, |oid| self.read_commit(oid))? {
                return Err(Error::Message(format!(
                    "branch '{name}' is not fully merged; use -D to force deletion"
                )));
            }
        }

        self.refs().delete_ref(&reference, Some(oid))
    }
}
