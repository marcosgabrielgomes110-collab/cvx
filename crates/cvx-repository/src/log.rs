use std::collections::BTreeMap;

use cvx_core::{Error, Result};
use cvx_object::{ObjectId, ObjectKind};
use cvx_revision::{LogEntry, walk_commits_with_boundary};

use crate::repository::Repository;

/// Options for `cvx log`.
#[derive(Debug, Clone, Default)]
pub struct LogOptions {
    /// Revision expression. Defaults to `HEAD`.
    pub revision: Option<String>,
    /// Maximum number of commits.
    pub limit: Option<usize>,
}

impl Repository {
    /// Walks commit history from a revision.
    pub fn log(&self, options: &LogOptions) -> Result<Vec<LogEntry>> {
        let start = self.resolve_revision(options.revision.as_deref().unwrap_or("HEAD"))?;
        let boundary: std::collections::HashSet<_> = self.shallow_commits()?.into_iter().collect();
        walk_commits_with_boundary(start, options.limit, &boundary, |oid| self.read_commit(oid))
    }

    /// Returns branch and tag names keyed by the commit they decorate.
    ///
    /// Annotated tags are peeled to their target commit. The current branch is
    /// labeled `HEAD -> <branch>`; unrelated branches and tags use their short
    /// names.
    pub fn decorations(&self) -> Result<BTreeMap<ObjectId, Vec<String>>> {
        let mut decorations: BTreeMap<ObjectId, Vec<String>> = BTreeMap::new();
        let current_branch = self.branch_name()?;

        for (reference, oid) in self.refs().list_refs("refs/heads")? {
            if let Some(name) = reference.strip_prefix("refs/heads/") {
                let label = if current_branch.as_deref() == Some(name) {
                    format!("HEAD -> {name}")
                } else {
                    name.to_owned()
                };
                decorations.entry(oid).or_default().push(label);
            }
        }

        for (reference, oid) in self.refs().list_refs("refs/tags")? {
            if let Some(name) = reference.strip_prefix("refs/tags/")
                && let Ok(commit) = self.peel_to_commit(oid)
            {
                decorations
                    .entry(commit)
                    .or_default()
                    .push(format!("tag: {name}"));
            }
        }

        if current_branch.is_none()
            && let Some(oid) = self.head()?.oid()
        {
            decorations.entry(oid).or_default().push("HEAD".to_owned());
        }

        for labels in decorations.values_mut() {
            labels.sort();
            labels.dedup();
        }
        Ok(decorations)
    }

    /// Resolves `HEAD`, a full or abbreviated object id, a branch or a tag.
    pub fn resolve_revision(&self, revision: &str) -> Result<ObjectId> {
        if revision == "HEAD" {
            let head = self.head()?;
            return head.oid().ok_or_else(|| {
                let branch = self
                    .branch_name()
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "HEAD".to_owned());
                Error::Message(format!(
                    "your current branch '{branch}' does not have any commits yet"
                ))
            });
        }

        if let Ok(oid) = ObjectId::from_hex(revision) {
            return self.peel_to_commit(oid);
        }

        let candidates = [
            format!("refs/heads/{revision}"),
            format!("refs/tags/{revision}"),
            format!("refs/remotes/{revision}"),
            revision.to_owned(),
        ];
        for candidate in candidates {
            if !cvx_core::refname::is_valid_ref_name(&candidate) {
                continue;
            }
            if let Some(oid) = self.refs().read_ref(&candidate)? {
                return self.peel_to_commit(oid);
            }
        }

        if let Some(oid) = self.resolve_abbreviated(revision)? {
            return self.peel_to_commit(oid);
        }

        Err(Error::Message(format!("unknown revision: {revision}")))
    }

    pub(crate) fn peel_to_commit(&self, oid: ObjectId) -> Result<ObjectId> {
        let mut current = oid;
        for _ in 0..8 {
            let object = self.read_object(current)?;
            match object.kind {
                ObjectKind::Commit => return Ok(current),
                ObjectKind::Tag => {
                    let text = std::str::from_utf8(&object.data)
                        .map_err(|_| Error::corrupt(format!("tag {current} is not valid UTF-8")))?;
                    let line = text
                        .lines()
                        .next()
                        .ok_or_else(|| Error::corrupt("tag object is empty"))?;
                    let value = line
                        .strip_prefix("object ")
                        .ok_or_else(|| Error::corrupt("tag object is missing an object header"))?;
                    current = ObjectId::from_hex(value)
                        .map_err(|error| Error::corrupt(format!("invalid tag target: {error}")))?;
                }
                other => {
                    return Err(Error::Message(format!(
                        "revision {oid} points to a {other}, not a commit"
                    )));
                }
            }
        }
        Err(Error::corrupt("tag chain is too deep"))
    }

    fn resolve_abbreviated(&self, revision: &str) -> Result<Option<ObjectId>> {
        if revision.len() < 4
            || revision.len() >= 40
            || !revision.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Ok(None);
        }

        let prefix = revision.to_ascii_lowercase();
        let objects = self.control_dir().join("objects");
        let mut matches = Vec::new();

        let Ok(entries) = std::fs::read_dir(&objects) else {
            return Ok(None);
        };
        for entry in entries {
            let entry = entry.map_err(|error| Error::io(objects.display().to_string(), error))?;
            let Some(directory) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if directory.len() != 2 || !directory.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                continue;
            }
            let Ok(files) = std::fs::read_dir(entry.path()) else {
                continue;
            };
            for file in files {
                let file =
                    file.map_err(|error| Error::io(entry.path().display().to_string(), error))?;
                let Some(name) = file.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                let candidate = format!("{directory}{name}");
                if candidate.starts_with(&prefix) {
                    if let Ok(oid) = ObjectId::from_hex(&candidate) {
                        matches.push(oid);
                    }
                }
            }
        }

        match matches.as_slice() {
            [] => Ok(None),
            [oid] => Ok(Some(*oid)),
            _ => Err(Error::Message(format!(
                "short object id '{revision}' is ambiguous"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AddOptions, CommitOptions, InitOptions};
    use std::path::PathBuf;

    fn shallow_test_repository(temp: &std::path::Path) -> (Repository, Vec<ObjectId>) {
        let repository = match Repository::init(InitOptions {
            directory: temp.to_path_buf(),
            ..InitOptions::default()
        })
        .expect("init")
        {
            crate::InitOutcome::Created(repository) => repository,
            _ => panic!("expected new repository"),
        };
        let config = repository.control_dir().join("config");
        let mut contents = std::fs::read_to_string(&config).expect("config");
        contents.push_str("\n[user]\n\tname = Test User\n\temail = test@example.com\n");
        std::fs::write(config, contents).expect("config");

        let mut commits = Vec::new();
        for index in 0..3 {
            std::fs::write(temp.join("file.txt"), format!("{index}\n")).expect("write");
            repository
                .add(AddOptions {
                    paths: vec![PathBuf::from(".")],
                    cwd: temp.to_path_buf(),
                    force: false,
                    update: false,
                    all: true,
                })
                .expect("add");
            commits.push(
                repository
                    .commit(CommitOptions {
                        messages: vec![format!("commit {index}")],
                        allow_empty: false,
                    })
                    .expect("commit")
                    .oid,
            );
        }
        (repository, commits)
    }

    #[test]
    fn log_stops_at_shallow_boundaries() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (repository, commits) = shallow_test_repository(temp.path());
        repository
            .write_shallow(&[commits[1]])
            .expect("write shallow");

        let entries = repository.log(&LogOptions::default()).expect("shallow log");
        let oids: Vec<ObjectId> = entries.iter().map(|entry| entry.oid).collect();
        assert_eq!(oids, vec![commits[2], commits[1]]);
    }
}
