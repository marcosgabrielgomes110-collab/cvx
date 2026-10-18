use cvx_core::{Error, Result};
use cvx_object::{ObjectId, ObjectKind};
use cvx_revision::{LogEntry, walk_commits};

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
        walk_commits(start, options.limit, |oid| self.read_commit(oid))
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

    fn peel_to_commit(&self, oid: ObjectId) -> Result<ObjectId> {
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
