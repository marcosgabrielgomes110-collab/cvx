use std::collections::BTreeSet;
use std::path::PathBuf;

use cvx_core::{Error, IoResultExt, Result};
use cvx_object::ObjectId;
use cvx_pack::{Pack, PackInput, write_pack_with_algorithm};

use crate::Repository;

/// Result of a repository garbage-collection pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcReport {
    /// Number of loose objects written into the new pack.
    pub packed_objects: usize,
    /// Number of unreachable loose objects deleted by `--prune`.
    pub pruned_objects: usize,
    /// Path to the pack written by this run, when one was needed.
    pub pack_path: Option<PathBuf>,
}

impl Repository {
    /// Consolidates reachable loose objects into a pack.
    ///
    /// Packed objects are not rewritten and existing packs are left in place.
    /// When `prune` is set, unreachable loose objects are deleted as well.
    pub fn gc(&self, prune: bool) -> Result<GcReport> {
        let mut roots = Vec::new();
        for (_, oid) in self.refs().list_refs("refs")? {
            roots.push(oid);
        }
        if let Some(oid) = self.head()?.oid() {
            roots.push(oid);
        }
        let reachable: BTreeSet<ObjectId> = self.reachable_objects(&roots)?.into_iter().collect();

        let mut inputs_data = Vec::new();
        for oid in &reachable {
            if self.objects().loose().contains(*oid) {
                let object = self.objects().read(*oid)?;
                inputs_data.push((object.kind, object.data));
            }
        }

        let pack_path = if inputs_data.is_empty() {
            None
        } else {
            let inputs: Vec<PackInput<'_>> = inputs_data
                .iter()
                .map(|(kind, data)| PackInput { kind: *kind, data })
                .collect();
            let outcome = write_pack_with_algorithm(
                self.control_dir().join("objects/pack"),
                &inputs,
                self.hash_algorithm(),
            )?;
            Pack::open_with_algorithm(&outcome.index_path, self.hash_algorithm())?;
            Some(outcome.pack_path)
        };

        if pack_path.is_some() {
            for (kind, data) in &inputs_data {
                let oid = cvx_object::hash_object_with(self.hash_algorithm(), *kind, data);
                let path = self.objects().loose().path_for(oid);
                match std::fs::remove_file(&path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(Error::io(path.display().to_string(), error));
                    }
                }
                if let Some(parent) = path.parent() {
                    let _ = std::fs::remove_dir(parent);
                }
            }
        }

        let mut pruned_objects = 0;
        if prune {
            for oid in self.objects().iter_oids()? {
                if reachable.contains(&oid) {
                    continue;
                }
                let path = self.objects().loose().path_for(oid);
                if !path.is_file() {
                    continue;
                }
                std::fs::remove_file(&path).path_context(&path)?;
                if let Some(parent) = path.parent() {
                    let _ = std::fs::remove_dir(parent);
                }
                pruned_objects += 1;
            }
        }

        Ok(GcReport {
            packed_objects: inputs_data.len(),
            pruned_objects,
            pack_path,
        })
    }
}
