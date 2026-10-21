use std::collections::BTreeMap;

use cvx_core::{Error, Result};
use cvx_index::{Index, IndexEntry, IndexLock};
use cvx_object::{MODE_GITLINK, ObjectId};

use crate::commit::IdentityRole;
use crate::repository::Repository;
use crate::status::StatusOptions;
use crate::worktree_ops::{WorktreeContent, read_content, remove_file, write_file};

/// Options for `cvx switch`.
#[derive(Debug, Clone)]
pub struct SwitchOptions {
    /// Branch name or revision.
    pub target: String,
    /// Create the branch before switching.
    pub create: bool,
    /// Discard local changes instead of refusing.
    pub force: bool,
    /// Detach HEAD at the target revision instead of switching to a branch.
    pub detach: bool,
}

/// Result of a switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchOutcome {
    /// Branch name when HEAD ends up symbolic.
    pub branch: Option<String>,
    /// Commit at the target, if any.
    pub oid: Option<ObjectId>,
    /// Whether a branch was created.
    pub created: bool,
    /// Whether the repository was already at the target.
    pub already: bool,
    /// Whether HEAD ended up detached.
    pub detached: bool,
}

pub(crate) type TreeSnapshot = BTreeMap<Vec<u8>, (u32, ObjectId)>;

impl Repository {
    /// Switches branches or detaches HEAD.
    pub fn switch(&self, options: &SwitchOptions) -> Result<SwitchOutcome> {
        if self.merge_head()?.is_some() {
            return Err(Error::Message(
                "cannot switch branches while a merge is in progress".to_owned(),
            ));
        }
        let current_head = self.head()?;
        let current_oid = current_head.oid();
        let current_branch = current_head
            .target()
            .and_then(|target| target.strip_prefix("refs/heads/"))
            .map(str::to_owned);

        let (target_branch, target_oid, created) = if options.detach {
            if options.create {
                return Err(Error::Message(
                    "--detach cannot be combined with -c".to_owned(),
                ));
            }
            let oid = self.resolve_revision(&options.target)?;
            (None, Some(oid), false)
        } else if options.create {
            cvx_core::refname::validate_branch_name(&options.target)?;
            let reference = format!("refs/heads/{}", options.target);
            if self.refs().read_ref(&reference)?.is_some() {
                return Err(Error::Message(format!(
                    "branch '{}' already exists",
                    options.target
                )));
            }

            if let Some(oid) = current_oid {
                self.create_branch(&options.target, None)?;
                (Some(options.target.clone()), Some(oid), true)
            } else {
                (Some(options.target.clone()), None, true)
            }
        } else {
            let reference = format!("refs/heads/{}", options.target);
            let Some(oid) = self.refs().read_ref(&reference)? else {
                return Err(Error::Message(format!(
                    "invalid reference: {}",
                    options.target
                )));
            };
            (Some(options.target.clone()), Some(oid), false)
        };

        if !options.create && !options.detach {
            if let (Some(current), Some(target)) = (&current_branch, &target_branch) {
                if current == target && current_oid == target_oid {
                    return Ok(SwitchOutcome {
                        branch: target_branch,
                        oid: target_oid,
                        created: false,
                        already: true,
                        detached: false,
                    });
                }
            }
        }

        let target_tree = match target_oid {
            Some(oid) => {
                let commit = self.read_commit(oid)?;
                self.read_tree_recursive(commit.tree)?
                    .into_iter()
                    .map(|(path, leaf)| (path, (leaf.mode, leaf.oid)))
                    .collect()
            }
            None => TreeSnapshot::new(),
        };

        let lock = IndexLock::acquire(self.control_dir())?;
        let index = self.read_index()?;
        let filemode = self.filemode()?;

        let status = self.status(StatusOptions {
            untracked: true,
            collapse_untracked: false,
        })?;
        if !status.conflicts.is_empty() {
            return Err(Error::Message(
                "cannot switch branches with unmerged entries".to_owned(),
            ));
        }
        if !options.force && (!status.staged.is_empty() || !status.unstaged.is_empty()) {
            return Err(Error::Message(
                "local changes would be overwritten; commit them or use --force".to_owned(),
            ));
        }

        let index_snapshot: BTreeMap<Vec<u8>, (u32, ObjectId)> = index
            .entries()
            .iter()
            .filter(|entry| entry.stage == 0)
            .map(|entry| (entry.path.clone(), (entry.mode, entry.oid)))
            .collect();

        let result = self.checkout_target(&target_tree, &index_snapshot, filemode, options.force);
        if let Err(error) = result {
            if created {
                let _ = self.delete_branch(&options.target, true);
            }
            return Err(error);
        }

        let new_index = self.index_from_tree(&target_tree, filemode)?;
        lock.commit(&new_index)?;

        let head_label = current_branch.clone().unwrap_or_else(|| match current_oid {
            Some(oid) => oid.short(7),
            None => "HEAD".to_owned(),
        });
        let new_label = target_branch.clone().unwrap_or_else(|| match target_oid {
            Some(oid) => oid.short(7),
            None => "HEAD".to_owned(),
        });

        match &target_branch {
            Some(branch) => self
                .refs()
                .write_head_symbolic(&format!("refs/heads/{branch}"))?,
            None => {
                let oid = target_oid.expect("detached target has a commit");
                self.refs().write_head_detached(oid)?;
            }
        }

        if let Some(oid) = target_oid {
            if self.reflog_enabled()? {
                if let Ok(identity) = self.resolve_signature(IdentityRole::Committer) {
                    let message = format!("checkout: moving from {head_label} to {new_label}");
                    let _ = self.append_reflog("HEAD", current_oid, oid, &identity, &message);
                }
            }
        }

        Ok(SwitchOutcome {
            branch: target_branch,
            oid: target_oid,
            created,
            already: false,
            detached: options.detach,
        })
    }

    pub(crate) fn index_from_tree(&self, tree: &TreeSnapshot, filemode: bool) -> Result<Index> {
        let mut new_index = Index::new();
        for (path, (mode, oid)) in tree {
            let absolute = self.work_tree().join(cvx_index::path_from_bytes(path)?);
            let metadata = std::fs::symlink_metadata(&absolute)
                .map_err(|error| Error::io(absolute.display().to_string(), error))?;
            let mut entry = IndexEntry::from_metadata(path.clone(), *oid, &metadata, filemode)?;
            entry.mode = *mode;
            new_index.insert(entry)?;
        }
        Ok(new_index)
    }

    pub(crate) fn checkout_target(
        &self,
        target: &TreeSnapshot,
        index: &BTreeMap<Vec<u8>, (u32, ObjectId)>,
        filemode: bool,
        force: bool,
    ) -> Result<()> {
        let attributes = self.attributes()?;
        let autocrlf = self.autocrlf()?;
        for (path, (mode, oid)) in target {
            if mode & 0o170_000 == MODE_GITLINK {
                return Err(Error::unsupported("checking out submodule entries"));
            }

            if !force && !index.contains_key(path) {
                match read_content(self.work_tree(), path, filemode)? {
                    WorktreeContent::File { .. } => {
                        return Err(Error::Message(format!(
                            "untracked file {} would be overwritten; use --force",
                            String::from_utf8_lossy(path)
                        )));
                    }
                    WorktreeContent::Directory => {
                        return Err(Error::Message(format!(
                            "a directory is in the way of {}",
                            String::from_utf8_lossy(path)
                        )));
                    }
                    WorktreeContent::Missing => {}
                }
            }

            let object = self.read_object(*oid)?;
            if object.kind != cvx_object::ObjectKind::Blob {
                return Err(Error::corrupt(format!(
                    "{} is a {}, not a blob",
                    oid, object.kind
                )));
            }
            if *mode & 0o170_000 == cvx_object::MODE_SYMLINK {
                write_file(self.work_tree(), path, *mode, &object.data)?;
            } else {
                let attributes =
                    cvx_attributes::effective_attributes(&attributes.for_path(path), autocrlf);
                crate::worktree_ops::write_file_with_attributes(
                    self.work_tree(),
                    path,
                    *mode,
                    &object.data,
                    &attributes,
                )?;
            }
        }

        for path in index.keys() {
            if target.contains_key(path) {
                continue;
            }
            remove_file(self.work_tree(), path)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AddOptions, CommitOptions, InitOptions};
    use std::path::{Path, PathBuf};

    fn repository(temp: &Path) -> Repository {
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
        std::fs::write(config, contents).expect("write config");
        repository
    }

    fn add_all(repository: &Repository, cwd: &Path) {
        repository
            .add(AddOptions {
                paths: vec![PathBuf::from(".")],
                cwd: cwd.to_path_buf(),
                force: false,
                update: false,
                all: true,
            })
            .expect("add");
    }

    fn commit(repository: &Repository, message: &str) {
        repository
            .commit(CommitOptions {
                messages: vec![message.to_owned()],
                allow_empty: false,
            })
            .expect("commit");
    }

    #[test]
    fn creates_and_switches_branches() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("file.txt"), "main\n").expect("write");
        add_all(&repository, temp.path());
        commit(&repository, "main");

        let outcome = repository
            .switch(&SwitchOptions {
                target: "feature".to_owned(),
                create: true,
                force: false,
                detach: false,
            })
            .expect("switch -c");
        assert!(outcome.created);
        assert_eq!(outcome.branch.as_deref(), Some("feature"));

        std::fs::write(temp.path().join("file.txt"), "feature\n").expect("write");
        add_all(&repository, temp.path());
        commit(&repository, "feature");

        repository
            .switch(&SwitchOptions {
                target: "main".to_owned(),
                create: false,
                force: false,
                detach: false,
            })
            .expect("switch main");
        assert_eq!(
            std::fs::read_to_string(temp.path().join("file.txt")).unwrap(),
            "main\n"
        );
    }

    #[test]
    fn refuses_to_overwrite_local_changes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("file.txt"), "main\n").expect("write");
        add_all(&repository, temp.path());
        commit(&repository, "main");

        repository
            .switch(&SwitchOptions {
                target: "feature".to_owned(),
                create: true,
                force: false,
                detach: false,
            })
            .expect("switch -c");

        std::fs::write(temp.path().join("file.txt"), "local\n").expect("write");
        let result = repository.switch(&SwitchOptions {
            target: "main".to_owned(),
            create: false,
            force: false,
            detach: false,
        });
        assert!(result.is_err());
    }

    #[test]
    fn detaches_head() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("file.txt"), "content\n").expect("write");
        add_all(&repository, temp.path());
        commit(&repository, "first");

        let outcome = repository
            .switch(&SwitchOptions {
                target: "HEAD".to_owned(),
                create: false,
                force: false,
                detach: true,
            })
            .expect("detach");
        assert!(outcome.detached);
        assert!(repository.head().expect("head").target().is_none());
    }
}
