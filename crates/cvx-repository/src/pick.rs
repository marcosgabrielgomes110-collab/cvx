use std::collections::BTreeSet;

use cvx_core::{Error, Result};
use cvx_index::IndexLock;
use cvx_merge::{MergeLabels, merge_file, merge_mode};
use cvx_object::{ObjectId, ObjectKind, hash_object_with};

use crate::repository::Repository;
use crate::status::StatusOptions;
use crate::switch::TreeSnapshot;
use crate::worktree_ops::remove_file;

/// Options for `cvx cherry-pick` and `cvx revert`.
#[derive(Debug, Clone)]
pub struct PickOptions {
    /// Commit or revision to pick or revert.
    pub revision: String,
}

/// Result of a cherry-pick or revert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickOutcome {
    /// New commit.
    pub oid: ObjectId,
    /// First line of the new commit message.
    pub subject: String,
    /// Branch that received the commit, when HEAD is symbolic.
    pub branch: Option<String>,
}

enum Change {
    Delete,
    Write { mode: u32, content: Vec<u8> },
}

impl Repository {
    /// Applies a commit on top of the current branch.
    pub fn cherry_pick(&self, options: &PickOptions) -> Result<PickOutcome> {
        self.apply_pick(options, false)
    }

    /// Reverts a commit on top of the current branch.
    pub fn revert(&self, options: &PickOptions) -> Result<PickOutcome> {
        self.apply_pick(options, true)
    }

    fn apply_pick(&self, options: &PickOptions, reverse: bool) -> Result<PickOutcome> {
        self.require_work_tree()?;

        let status = self.status(StatusOptions {
            untracked: true,
            collapse_untracked: false,
        })?;
        if !status.conflicts.is_empty() {
            return Err(Error::Message(
                "cannot pick with unresolved index entries".to_owned(),
            ));
        }
        if !status.staged.is_empty() || !status.unstaged.is_empty() {
            return Err(Error::Message(
                "local changes would be overwritten; commit them first".to_owned(),
            ));
        }

        let head_oid = self
            .head()?
            .oid()
            .ok_or_else(|| Error::Message("cannot pick before the first commit".to_owned()))?;
        let target_oid = self.resolve_revision(&options.revision)?;
        let target = self.read_commit(target_oid)?;
        if target.parents.len() > 1 {
            return Err(Error::unsupported(
                "cherry-picking or reverting merge commits",
            ));
        }

        let ours_tree = self.tree_snapshot_for_commit(head_oid)?;
        let target_tree = self.tree_snapshot_for_commit(target_oid)?;
        let parent_tree = match target.parents.first() {
            Some(parent) => self.tree_snapshot_for_commit(*parent)?,
            None => TreeSnapshot::new(),
        };
        let (base_tree, theirs_tree) = if reverse {
            (target_tree, parent_tree)
        } else {
            (parent_tree, target_tree)
        };

        let paths = base_tree
            .keys()
            .chain(ours_tree.keys())
            .chain(theirs_tree.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut changes: Vec<(Vec<u8>, Change)> = Vec::new();
        let mut conflicts = Vec::new();

        for path in paths {
            let base = base_tree.get(&path).copied();
            let ours = ours_tree.get(&path).copied();
            let theirs = theirs_tree.get(&path).copied();

            if base == theirs || ours == theirs {
                continue;
            }

            if ours == base {
                match theirs {
                    Some((mode, oid)) => {
                        if mode & 0o170_000 == 0o160_000 {
                            return Err(Error::unsupported("picking submodule entries"));
                        }
                        changes.push((
                            path,
                            Change::Write {
                                mode,
                                content: self.blob_content(oid)?,
                            },
                        ));
                    }
                    None => changes.push((path, Change::Delete)),
                }
                continue;
            }

            match (base, ours, theirs) {
                (Some(base), Some(ours), Some(theirs)) => {
                    if ours.0 & 0o170_000 == 0o160_000
                        || theirs.0 & 0o170_000 == 0o160_000
                        || base.0 & 0o170_000 == 0o160_000
                    {
                        return Err(Error::unsupported("picking submodule entries"));
                    }
                    let labels = MergeLabels {
                        ours: "HEAD",
                        theirs: if reverse { "revert" } else { "cherry-pick" },
                    };
                    let base_content = self.blob_content(base.1)?;
                    let ours_content = self.blob_content(ours.1)?;
                    let theirs_content = self.blob_content(theirs.1)?;
                    let merged = merge_file(&base_content, &ours_content, &theirs_content, labels);
                    if merged.clean
                        && let Some(mode) = merge_mode(base.0, ours.0, theirs.0)
                    {
                        if merged.content == ours_content && mode == ours.0 {
                            continue;
                        }
                        changes.push((
                            path,
                            Change::Write {
                                mode,
                                content: merged.content,
                            },
                        ));
                        continue;
                    }
                    conflicts.push(path);
                }
                _ => conflicts.push(path),
            }
        }

        if !conflicts.is_empty() {
            let paths = conflicts
                .iter()
                .map(|path| String::from_utf8_lossy(path).into_owned())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(Error::Message(format!(
                "cannot {} cleanly; no changes were applied: {paths}",
                if reverse { "revert" } else { "cherry-pick" }
            )));
        }

        if changes.is_empty() {
            return Err(Error::nothing_to_commit(if reverse {
                "nothing to revert; the change is already absent"
            } else {
                "nothing to cherry-pick; the change is already present"
            }));
        }

        // Resolve identity before touching the index or worktree.
        self.resolve_signature(crate::commit::IdentityRole::Author)?;
        self.resolve_signature(crate::commit::IdentityRole::Committer)?;

        let filemode = self.filemode()?;
        let lock = IndexLock::acquire(self.control_dir())?;
        let mut index = self.read_index()?;
        for (path, change) in changes {
            match change {
                Change::Delete => {
                    index.remove_path(&path);
                    remove_file(self.work_tree(), &path)?;
                }
                Change::Write { mode, content } => {
                    let oid = hash_object_with(self.hash_algorithm(), ObjectKind::Blob, &content);
                    self.write_clean(&mut index, &path, mode, oid, &content, filemode)?;
                }
            }
        }
        lock.commit(&index)?;

        let subject = target.subject().to_owned();
        let message = if reverse {
            format!("Revert \"{subject}\"\n\nThis reverts commit {target_oid}.")
        } else {
            target.message.clone()
        };
        let outcome = self.commit_with_parents(vec![message], vec![head_oid], false)?;
        Ok(PickOutcome {
            oid: outcome.oid,
            subject: outcome.subject,
            branch: outcome.branch,
        })
    }
}
