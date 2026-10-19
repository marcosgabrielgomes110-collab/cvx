use std::collections::BTreeSet;

use cvx_core::Result;
use cvx_object::{Commit, ObjectId, ObjectKind, Tag, Tree};

use crate::repository::Repository;

impl Repository {
    /// Collects every object reachable from the given roots.
    ///
    /// Commits pull in their trees and parents, trees pull in their entries,
    /// annotated tags pull in their targets, and blobs are leaves. Submodule
    /// gitlinks are skipped because their commits live in another repository.
    pub fn reachable_objects(&self, roots: &[ObjectId]) -> Result<Vec<ObjectId>> {
        let mut visited = BTreeSet::new();
        let mut stack = roots.to_vec();

        while let Some(oid) = stack.pop() {
            if !visited.insert(oid) {
                continue;
            }

            let object = self.read_object(oid)?;
            match object.kind {
                ObjectKind::Commit => {
                    let commit = Commit::parse(&object.data)?;
                    stack.push(commit.tree);
                    stack.extend(commit.parents);
                }
                ObjectKind::Tree => {
                    let tree = Tree::parse(&object.data)?;
                    for entry in tree.entries() {
                        if !entry.is_gitlink() {
                            stack.push(entry.oid);
                        }
                    }
                }
                ObjectKind::Tag => {
                    let tag = Tag::parse(&object.data)?;
                    stack.push(tag.object);
                }
                ObjectKind::Blob => {}
            }
        }

        Ok(visited.into_iter().collect())
    }

    /// Collects every object reachable from local branches, tags and `HEAD`.
    pub fn reachable_from_local_refs(&self) -> Result<Vec<ObjectId>> {
        let mut roots = Vec::new();
        for (_, oid) in self.refs().list_refs("refs/heads")? {
            roots.push(oid);
        }
        for (_, oid) in self.refs().list_refs("refs/tags")? {
            roots.push(oid);
        }
        if let Some(oid) = self.head()?.oid() {
            roots.push(oid);
        }
        self.reachable_objects(&roots)
    }
}

#[cfg(test)]
mod tests {
    use crate::{AddOptions, CommitOptions, InitOptions};
    use std::path::{Path, PathBuf};

    use super::*;

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
        let path = repository.control_dir().join("config");
        let mut config = std::fs::read_to_string(&path).expect("config");
        config.push_str("\n[user]\n\tname = Test User\n\temail = test@example.com\n");
        std::fs::write(path, config).expect("config");
        repository
    }

    #[test]
    fn collects_reachable_commits_trees_blobs_and_tags() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repository = repository(temp.path());
        std::fs::write(temp.path().join("file.txt"), "content\n").expect("write");
        repository
            .add(AddOptions {
                paths: vec![PathBuf::from(".")],
                cwd: temp.path().to_path_buf(),
                force: false,
                update: false,
                all: true,
            })
            .expect("add");
        let commit = repository
            .commit(CommitOptions {
                messages: vec!["first".to_owned()],
                allow_empty: false,
            })
            .expect("commit");
        repository
            .create_tag(&crate::TagOptions {
                name: "v1".to_owned(),
                target: None,
                message: Some("release".to_owned()),
                annotated: true,
                force: false,
            })
            .expect("tag");

        let objects = repository
            .reachable_from_local_refs()
            .expect("reachable objects");

        assert!(objects.contains(&commit.oid));
        let commit_object = repository.read_commit(commit.oid).expect("commit object");
        assert!(objects.contains(&commit_object.tree));
        let tree = repository
            .read_tree_recursive(commit_object.tree)
            .expect("tree");
        assert_eq!(tree.len(), 1);
        let blob = tree.values().next().expect("blob").oid;
        assert!(objects.contains(&blob));

        let tags = repository.tags().expect("tags");
        assert_eq!(tags.len(), 1);
        assert!(objects.contains(&tags[0].1));
        assert_eq!(
            repository.resolve_revision("v1").expect("peel tag"),
            commit.oid
        );
    }
}
