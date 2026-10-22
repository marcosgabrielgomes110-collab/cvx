use cvx_core::{Error, Result};
use cvx_index::{IndexLock, MODE_SYMLINK};
use cvx_object::{Commit, ObjectId, ObjectKind};

use crate::commit::IdentityRole;
use crate::repository::Repository;
use crate::reset::{ResetMode, ResetOptions};
use crate::status::StatusOptions;
use crate::worktree_ops::{WorktreeContent, read_content};

/// Options for `cvx stash push`.
#[derive(Debug, Clone, Default)]
pub struct StashPushOptions {
    /// Optional stash message.
    pub message: Option<String>,
}

/// One entry in the stash reflog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashEntry {
    /// Stash commit.
    pub oid: ObjectId,
    /// Reflog message.
    pub message: String,
}

/// Result of saving or restoring a stash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashOutcome {
    /// Stash commit.
    pub oid: ObjectId,
    /// Stash message.
    pub message: String,
}

impl Repository {
    /// Saves tracked worktree and index changes, then resets to `HEAD`.
    pub fn stash_push(&self, options: &StashPushOptions) -> Result<StashOutcome> {
        self.require_work_tree()?;

        let status = self.status(StatusOptions {
            untracked: true,
            collapse_untracked: false,
        })?;
        if !status.conflicts.is_empty() {
            return Err(Error::Message(
                "cannot stash with unresolved index entries".to_owned(),
            ));
        }
        if status.staged.is_empty() && status.unstaged.is_empty() {
            return Err(Error::nothing_to_commit("no local changes to save"));
        }

        let head_oid = self
            .head()?
            .oid()
            .ok_or_else(|| Error::Message("cannot stash before the first commit".to_owned()))?;
        let head = self.read_commit(head_oid)?;
        let filemode = self.filemode()?;
        let attributes = self.attributes()?;
        let autocrlf = self.autocrlf()?;

        let mut index = self.read_index()?;
        let index_tree = self.build_tree(&index)?;
        let tracked: Vec<Vec<u8>> = index
            .entries()
            .iter()
            .filter(|entry| entry.stage == 0)
            .map(|entry| entry.path.clone())
            .collect();
        let mut updates = Vec::new();
        let mut deletions = Vec::new();
        for path in tracked {
            match read_content(self.work_tree(), &path, filemode)? {
                WorktreeContent::Missing => deletions.push(path),
                WorktreeContent::Directory => {
                    return Err(Error::Message(format!(
                        "cannot stash {}: a directory is in the way",
                        String::from_utf8_lossy(&path)
                    )));
                }
                WorktreeContent::File { mode, content } => {
                    let content = if mode & 0o170_000 == MODE_SYMLINK {
                        content
                    } else {
                        let effective = cvx_attributes::effective_attributes(
                            &attributes.for_path(&path),
                            autocrlf,
                        );
                        effective.to_index_content(&content)
                    };
                    let oid = self.objects().write(ObjectKind::Blob, &content)?;
                    updates.push((path, mode, oid));
                }
            }
        }
        for path in deletions {
            index.remove_path(&path);
        }
        for (path, mode, oid) in updates {
            if let Some(entry) = index
                .entries_mut()
                .iter_mut()
                .find(|entry| entry.stage == 0 && entry.path == path)
            {
                entry.mode = mode;
                entry.oid = oid;
            }
        }

        let tree = self.build_tree(&index)?;
        if tree == head.tree {
            return Err(Error::nothing_to_commit("no local changes to save"));
        }

        let branch = self.branch_name()?.unwrap_or_else(|| "HEAD".to_owned());
        let default_message = format!("WIP on {branch}: {} {}", head_oid.short(7), head.subject());
        let message = options
            .message
            .clone()
            .filter(|message| !message.trim().is_empty())
            .unwrap_or(default_message);
        let mut message = message;
        if !message.ends_with('\n') {
            message.push('\n');
        }

        let author = self.resolve_signature(IdentityRole::Author)?;
        let committer = self.resolve_signature(IdentityRole::Committer)?;
        let mut index_message = format!(
            "index on {branch}: {} {}",
            head_oid.short(7),
            head.subject()
        );
        if !index_message.ends_with('\n') {
            index_message.push('\n');
        }
        let index_commit = Commit {
            tree: index_tree,
            parents: vec![head_oid],
            author: author.clone(),
            committer: committer.clone(),
            message: index_message,
            extra_headers: Vec::new(),
        };
        let index_oid = self
            .objects()
            .write(ObjectKind::Commit, &index_commit.serialize())?;
        let stash = Commit {
            tree,
            parents: vec![head_oid, index_oid],
            author,
            committer: committer.clone(),
            message: message.clone(),
            extra_headers: Vec::new(),
        };
        let oid = self
            .objects()
            .write(ObjectKind::Commit, &stash.serialize())?;

        let old = self.refs().read_ref("refs/stash")?;
        self.refs().update_ref("refs/stash", oid, old)?;
        if self.reflog_enabled()? {
            let reflog_message = format!("stash: {}", message.trim_end());
            self.append_reflog("refs/stash", old, oid, &committer, &reflog_message)?;
        }

        cvx_core::failpoint::hit("stash.before-reset")?;
        self.reset(&ResetOptions {
            target: "HEAD".to_owned(),
            mode: ResetMode::Hard,
            paths: Vec::new(),
            cwd: self.work_tree().to_path_buf(),
        })?;

        Ok(StashOutcome { oid, message })
    }

    /// Lists stash entries, newest first.
    pub fn stash_list(&self) -> Result<Vec<StashEntry>> {
        let mut entries = self
            .stash_reflog()?
            .into_iter()
            .map(|line| StashEntry {
                oid: line.new,
                message: line.message,
            })
            .collect::<Vec<_>>();
        entries.reverse();
        Ok(entries)
    }

    /// Applies the newest stash without removing it.
    pub fn stash_apply(&self) -> Result<StashOutcome> {
        self.require_work_tree()?;

        let oid = self
            .refs()
            .read_ref("refs/stash")?
            .ok_or_else(|| Error::Message("no stash entries".to_owned()))?;
        let stash = self.read_commit(oid)?;
        let parent = *stash
            .parents
            .first()
            .ok_or_else(|| Error::corrupt("stash commit has no parent"))?;
        if self.head()?.oid() != Some(parent) {
            return Err(Error::Message(
                "the stash was created on a different commit; cannot apply it here".to_owned(),
            ));
        }

        let status = self.status(StatusOptions {
            untracked: true,
            collapse_untracked: false,
        })?;
        if !status.conflicts.is_empty() || !status.staged.is_empty() || !status.unstaged.is_empty()
        {
            return Err(Error::Message(
                "local changes would be overwritten; commit them first".to_owned(),
            ));
        }

        let stash_tree = self.tree_snapshot_for_commit(oid)?;
        let parent_tree = self.tree_snapshot_for_commit(parent)?;
        let filemode = self.filemode()?;
        let lock = IndexLock::acquire(self.control_dir())?;
        self.checkout_target(&stash_tree, &parent_tree, filemode, false)?;

        let index = self.index_from_snapshot(&parent_tree)?;
        lock.commit(&index)?;

        Ok(StashOutcome {
            oid,
            message: stash.subject().to_owned(),
        })
    }

    /// Applies and removes the newest stash.
    pub fn stash_pop(&self) -> Result<StashOutcome> {
        let outcome = self.stash_apply()?;
        self.stash_drop()?;
        Ok(outcome)
    }

    /// Removes the newest stash entry.
    pub fn stash_drop(&self) -> Result<StashOutcome> {
        let oid = self
            .refs()
            .read_ref("refs/stash")?
            .ok_or_else(|| Error::Message("no stash entries".to_owned()))?;
        let message = self
            .read_commit(oid)
            .map(|commit| commit.subject().to_owned())
            .unwrap_or_default();

        let path = self.control_dir().join("logs/refs/stash");
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.refs().delete_ref("refs/stash", Some(oid))?;
                return Ok(StashOutcome { oid, message });
            }
            Err(error) => return Err(Error::io(path.display().to_string(), error)),
        };
        let lines = parse_reflog(&contents);
        let previous = lines
            .last()
            .filter(|line| line.new == oid)
            .and_then(|line| line.old);

        let trimmed = contents.trim_end_matches('\n');
        let remaining = match trimmed.rfind('\n') {
            Some(position) => &trimmed[..position + 1],
            None => "",
        };
        if remaining.is_empty() {
            let _ = std::fs::remove_file(&path);
        } else {
            cvx_core::fs::write_atomic(&path, remaining.as_bytes(), Some(0o644))?;
        }

        match previous {
            Some(previous) => self.refs().update_ref("refs/stash", previous, Some(oid))?,
            None => self.refs().delete_ref("refs/stash", Some(oid))?,
        }
        Ok(StashOutcome { oid, message })
    }

    fn stash_reflog(&self) -> Result<Vec<ReflogLine>> {
        let path = self.control_dir().join("logs/refs/stash");
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(Error::io(path.display().to_string(), error)),
        };
        Ok(parse_reflog(&contents))
    }
}

#[derive(Debug, Clone)]
struct ReflogLine {
    old: Option<ObjectId>,
    new: ObjectId,
    message: String,
}

fn parse_reflog(contents: &str) -> Vec<ReflogLine> {
    let mut lines = Vec::new();
    for line in contents.lines() {
        let (metadata, message) = line.split_once('\t').unwrap_or((line, ""));
        let mut fields = metadata.split_whitespace();
        let (Some(old), Some(new)) = (fields.next(), fields.next()) else {
            continue;
        };
        let Ok(new) = ObjectId::from_hex(new) else {
            continue;
        };
        let old = ObjectId::from_hex(old).ok().filter(|oid| !oid.is_zero());
        lines.push(ReflogLine {
            old,
            new,
            message: message.to_owned(),
        });
    }
    lines
}
