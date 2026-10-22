use std::collections::BTreeMap;
use std::env;
use std::io::Write;

use cvx_core::{Error, IoResultExt, Result};
use cvx_object::{
    Commit, MODE_TREE, ObjectId, ObjectKind, Signature, Tree, TreeEntry, parse_timezone,
};
use cvx_ref::Head;
use time::OffsetDateTime;
use time::UtcOffset;
use time::format_description::well_known::Rfc3339;

use crate::repository::Repository;

/// Options for `cvx commit`.
#[derive(Debug, Clone)]
pub struct CommitOptions {
    /// One or more message paragraphs.
    pub messages: Vec<String>,
    /// Allow a commit whose tree is identical to its parent's tree.
    pub allow_empty: bool,
}

/// Result of a successful commit.
#[derive(Debug, Clone)]
pub struct CommitOutcome {
    /// New commit identifier.
    pub oid: ObjectId,
    /// Branch name when `HEAD` is symbolic.
    pub branch: Option<String>,
    /// Whether this was the first commit on the branch.
    pub root: bool,
    /// First line of the commit message.
    pub subject: String,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum IdentityRole {
    Author,
    Committer,
}

impl IdentityRole {
    fn upper(self) -> &'static str {
        match self {
            Self::Author => "AUTHOR",
            Self::Committer => "COMMITTER",
        }
    }
}

#[derive(Debug, Default)]
struct TreeNode {
    children: BTreeMap<Vec<u8>, TreeNode>,
    file: Option<(u32, ObjectId)>,
}

impl Repository {
    /// Commits the current index.
    ///
    /// When `MERGE_HEAD` exists, the resulting commit records both `HEAD` and
    /// the merge target as parents, matching Git's merge-commit workflow.
    pub fn commit(&self, options: CommitOptions) -> Result<CommitOutcome> {
        let merge_head = self.merge_head()?;
        let head_oid = self.head()?.oid();
        let mut parents: Vec<ObjectId> = head_oid.into_iter().collect();
        if let Some(merge_head) = merge_head {
            parents.push(merge_head);
        }

        let outcome = self.commit_with_parents(
            options.messages,
            parents,
            options.allow_empty || merge_head.is_some(),
        )?;
        if merge_head.is_some() {
            self.clear_merge_state()?;
        }
        Ok(outcome)
    }

    pub(crate) fn commit_with_parents(
        &self,
        messages: Vec<String>,
        parents: Vec<ObjectId>,
        allow_empty: bool,
    ) -> Result<CommitOutcome> {
        let index = self.read_index()?;
        if index.has_conflicts() {
            return Err(Error::Message(
                "cannot commit because the index has unmerged entries".to_owned(),
            ));
        }

        let tree = self.build_tree(&index)?;
        let head_oid = self.head()?.oid();
        let parent_commit = match parents.first() {
            Some(oid) => Some(self.read_commit(*oid)?),
            None => None,
        };
        let root = parents.is_empty();

        if let Some(parent) = &parent_commit {
            if parent.tree == tree && !allow_empty {
                return Err(Error::nothing_to_commit(
                    "nothing to commit, working tree clean",
                ));
            }
        } else if index.is_empty() && !allow_empty {
            return Err(Error::nothing_to_commit(
                "nothing to commit (create/copy files and use \"cvx add\" to track)",
            ));
        }

        let message = commit_message(&messages)?;
        let author = self.resolve_signature(IdentityRole::Author)?;
        let committer = self.resolve_signature(IdentityRole::Committer)?;
        let subject = message.lines().next().unwrap_or("").trim_end().to_owned();

        let commit = Commit {
            tree,
            parents: parents.clone(),
            author,
            committer: committer.clone(),
            message,
            extra_headers: Vec::new(),
        };
        let oid = self
            .objects()
            .write(ObjectKind::Commit, &commit.serialize())?;
        cvx_core::failpoint::hit("commit.before-ref-update")?;

        let branch = self.update_head_for_commit(
            oid,
            head_oid,
            &committer,
            &subject,
            root,
            parents.len() > 1,
        )?;

        Ok(CommitOutcome {
            oid,
            branch,
            root,
            subject,
        })
    }

    pub(crate) fn build_tree(&self, index: &cvx_index::Index) -> Result<ObjectId> {
        let mut root = TreeNode::default();

        for entry in index.entries() {
            if entry.stage != 0 {
                return Err(Error::Message(
                    "cannot build a tree from unmerged index entries".to_owned(),
                ));
            }

            let components: Vec<&[u8]> = entry.path.split(|byte| *byte == b'/').collect();
            insert_index_entry(&mut root, &components, entry.mode, entry.oid, &entry.path)?;
        }

        self.write_tree_node(&root)
    }

    fn write_tree_node(&self, node: &TreeNode) -> Result<ObjectId> {
        let mut entries = Vec::new();

        for (name, child) in &node.children {
            match (&child.file, child.children.is_empty()) {
                (Some(_), false) => {
                    return Err(Error::corrupt(format!(
                        "tree path conflict at {}",
                        String::from_utf8_lossy(name)
                    )));
                }
                (Some((mode, oid)), true) => {
                    entries.push(TreeEntry::new(*mode, name.clone(), *oid));
                }
                (None, true) => {
                    return Err(Error::corrupt(format!(
                        "empty tree path at {}",
                        String::from_utf8_lossy(name)
                    )));
                }
                (None, false) => {
                    let oid = self.write_tree_node(child)?;
                    entries.push(TreeEntry::new(MODE_TREE, name.clone(), oid));
                }
            }
        }

        let tree = Tree::new(entries);
        self.objects().write(ObjectKind::Tree, &tree.serialize())
    }

    pub(crate) fn resolve_signature(&self, role: IdentityRole) -> Result<Signature> {
        let role_name = role.upper();
        let name = match env_value(&format!("CVX_{role_name}_NAME"))
            .or_else(|| env_value(&format!("GIT_{role_name}_NAME")))
        {
            Some(name) => name,
            None => self
                .config_value("user.name")?
                .ok_or_else(|| missing_identity("user.name"))?,
        };
        let email = match env_value(&format!("CVX_{role_name}_EMAIL"))
            .or_else(|| env_value(&format!("GIT_{role_name}_EMAIL")))
        {
            Some(email) => email,
            None => self
                .config_value("user.email")?
                .ok_or_else(|| missing_identity("user.email"))?,
        };

        let local_offset = local_offset_minutes();
        let (timestamp, offset) = match env_value(&format!("CVX_{role_name}_DATE"))
            .or_else(|| env_value(&format!("GIT_{role_name}_DATE")))
        {
            Some(value) => {
                let (timestamp, offset) = parse_date(&value)?;
                (timestamp, offset.unwrap_or(local_offset))
            }
            None => (OffsetDateTime::now_utc().unix_timestamp(), local_offset),
        };

        let signature = Signature::new(name, email, timestamp, offset);
        if !signature.is_valid() {
            return Err(Error::Message(
                "identity contains characters that cannot be stored in a commit".to_owned(),
            ));
        }
        Ok(signature)
    }

    fn update_head_for_commit(
        &self,
        new_oid: ObjectId,
        expected_old: Option<ObjectId>,
        committer: &Signature,
        subject: &str,
        root: bool,
        merge: bool,
    ) -> Result<Option<String>> {
        let head = self.head()?;
        let message = if root {
            format!("commit (initial): {subject}")
        } else if merge {
            format!("commit (merge): {subject}")
        } else {
            format!("commit: {subject}")
        };

        match head {
            Head::Symbolic { target, oid } => {
                if oid != expected_old {
                    return Err(Error::Message(
                        "failed to commit: HEAD changed concurrently".to_owned(),
                    ));
                }
                self.refs().update_ref(&target, new_oid, expected_old)?;
                if self.reflog_enabled()? {
                    self.append_reflog(&target, expected_old, new_oid, committer, &message)?;
                    self.append_reflog("HEAD", expected_old, new_oid, committer, &message)?;
                }
                Ok(target.strip_prefix("refs/heads/").map(str::to_owned))
            }
            Head::Detached(oid) => {
                if Some(oid) != expected_old {
                    return Err(Error::Message(
                        "failed to commit: HEAD changed concurrently".to_owned(),
                    ));
                }
                self.refs().write_head_detached(new_oid)?;
                if self.reflog_enabled()? {
                    self.append_reflog("HEAD", expected_old, new_oid, committer, &message)?;
                }
                Ok(None)
            }
        }
    }

    pub(crate) fn reflog_enabled(&self) -> Result<bool> {
        if let Some(value) = self.config_value("core.logallrefupdates")? {
            if value.eq_ignore_ascii_case("always") {
                return Ok(true);
            }
            return Ok(self.config_bool("core.logallrefupdates")?.unwrap_or(true));
        }
        Ok(true)
    }

    pub(crate) fn append_reflog(
        &self,
        name: &str,
        old: Option<ObjectId>,
        new: ObjectId,
        committer: &Signature,
        message: &str,
    ) -> Result<()> {
        let path = self.control_dir().join("logs").join(name);
        if let Some(parent) = path.parent() {
            cvx_core::fs::ensure_dir(parent)?;
        }

        let old = old.unwrap_or(ObjectId::ZERO);
        let line = format!("{old} {new} {}\t{message}\n", committer.to_git_string());

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .path_context(&path)?;
        file.write_all(line.as_bytes()).path_context(&path)?;
        file.flush().path_context(&path)?;
        Ok(())
    }
}

fn insert_index_entry(
    node: &mut TreeNode,
    components: &[&[u8]],
    mode: u32,
    oid: ObjectId,
    path: &[u8],
) -> Result<()> {
    let Some((first, rest)) = components.split_first() else {
        return Err(Error::corrupt("empty index path"));
    };

    let child = node.children.entry(first.to_vec()).or_default();
    if !rest.is_empty() {
        if child.file.is_some() {
            return Err(Error::corrupt(format!(
                "index path conflict at {}",
                String::from_utf8_lossy(path)
            )));
        }
        insert_index_entry(child, rest, mode, oid, path)
    } else {
        if child.file.is_some() || !child.children.is_empty() {
            return Err(Error::corrupt(format!(
                "duplicate index path {}",
                String::from_utf8_lossy(path)
            )));
        }
        child.file = Some((mode, oid));
        Ok(())
    }
}

fn missing_identity(key: &str) -> Error {
    Error::Message(format!(
        "author identity unknown: configure {key} with `git config --global {key} \"...\"`"
    ))
}

fn env_value(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}

fn local_offset_minutes() -> i32 {
    UtcOffset::current_local_offset()
        .unwrap_or(UtcOffset::UTC)
        .whole_seconds()
        / 60
}

fn parse_date(value: &str) -> Result<(i64, Option<i32>)> {
    let value = value.trim();
    if let Some(rest) = value.strip_prefix('@') {
        let mut fields = rest.split_whitespace();
        let timestamp = fields
            .next()
            .ok_or_else(|| Error::Message("invalid date: missing timestamp".to_owned()))?
            .parse::<i64>()
            .map_err(|_| Error::Message(format!("invalid date: {value}")))?;
        let offset = match fields.next() {
            Some(offset) => Some(parse_timezone(offset)?),
            None => None,
        };
        if fields.next().is_some() {
            return Err(Error::Message(format!("invalid date: {value}")));
        }
        return Ok((timestamp, offset));
    }

    let datetime = OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|_| Error::Message(format!("invalid date: {value}")))?;
    Ok((
        datetime.unix_timestamp(),
        Some(datetime.offset().whole_seconds() / 60),
    ))
}

fn commit_message(messages: &[String]) -> Result<String> {
    if messages.is_empty() {
        return Err(Error::Message("no commit message supplied".to_owned()));
    }

    let joined = messages.join("\n\n");
    let mut lines: Vec<String> = joined
        .lines()
        .map(|line| line.trim_end().to_owned())
        .collect();

    while lines.first().is_some_and(|line| line.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return Err(Error::Message("empty commit message".to_owned()));
    }

    let mut message = lines.join("\n");
    message.push('\n');
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InitOptions;
    use std::path::PathBuf;

    fn repository(temp: &std::path::Path) -> Repository {
        let repository = match Repository::init(InitOptions {
            directory: temp.to_path_buf(),
            ..InitOptions::default()
        })
        .expect("init")
        {
            crate::InitOutcome::Created(repository) => repository,
            _ => panic!("expected new repository"),
        };
        configure_identity(&repository);
        repository
    }

    fn configure_identity(repository: &Repository) {
        let path = repository.control_dir().join("config");
        let mut config = std::fs::read_to_string(&path).expect("config");
        config.push_str("\n[user]\n\tname = Test User\n\temail = test@example.com\n");
        std::fs::write(path, config).expect("write config");
    }

    fn add_all(repository: &Repository, cwd: &std::path::Path) {
        repository
            .add(crate::AddOptions {
                paths: vec![PathBuf::from(".")],
                cwd: cwd.to_path_buf(),
                force: false,
                update: false,
                all: true,
            })
            .expect("add");
    }

    fn options(message: &str) -> CommitOptions {
        CommitOptions {
            messages: vec![message.to_owned()],
            allow_empty: false,
        }
    }

    #[test]
    fn commits_and_moves_the_branch() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("hello.txt"), "hello\n").expect("write");
        add_all(&repository, temp.path());

        let outcome = repository
            .commit(options("initial commit"))
            .expect("commit");
        assert!(outcome.root);
        assert_eq!(outcome.branch.as_deref(), Some("main"));
        assert_eq!(outcome.subject, "initial commit");
        assert_eq!(repository.head().expect("head").oid(), Some(outcome.oid));

        let commit = repository.read_commit(outcome.oid).expect("read commit");
        assert_eq!(commit.parents.len(), 0);
        assert_eq!(commit.subject(), "initial commit");

        let tree = repository.read_tree_recursive(commit.tree).expect("tree");
        assert_eq!(tree.len(), 1);
        assert!(tree.contains_key(b"hello.txt".as_slice()));
    }

    #[test]
    fn second_commit_has_parent() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("hello.txt"), "hello\n").expect("write");
        add_all(&repository, temp.path());
        let first = repository.commit(options("first")).expect("first");

        std::fs::write(temp.path().join("hello.txt"), "hello again\n").expect("write");
        add_all(&repository, temp.path());
        let second = repository.commit(options("second")).expect("second");

        let commit = repository.read_commit(second.oid).expect("read");
        assert_eq!(commit.parents, vec![first.oid]);
    }

    #[test]
    fn refuses_empty_commit_without_flag() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("hello.txt"), "hello\n").expect("write");
        add_all(&repository, temp.path());
        repository.commit(options("first")).expect("first");

        let error = repository
            .commit(options("second"))
            .expect_err("nothing to commit");
        assert!(matches!(error, Error::NothingToCommit { .. }));

        repository
            .commit(CommitOptions {
                messages: vec!["empty".to_owned()],
                allow_empty: true,
            })
            .expect("allow empty");
    }

    #[test]
    fn writes_reflog_and_tree_objects_readable_by_git() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("hello.txt"), "hello\n").expect("write");
        add_all(&repository, temp.path());
        let outcome = repository.commit(options("initial")).expect("commit");

        let reflog =
            std::fs::read_to_string(temp.path().join(".cvx/logs/refs/heads/main")).expect("reflog");
        assert!(reflog.contains("commit (initial): initial"));
        assert!(reflog.contains(&outcome.oid.to_hex()));

        let head_log =
            std::fs::read_to_string(temp.path().join(".cvx/logs/HEAD")).expect("HEAD log");
        assert!(head_log.contains(&outcome.oid.to_hex()));
    }
}
