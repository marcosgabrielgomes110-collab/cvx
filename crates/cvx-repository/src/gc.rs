use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use cvx_core::{Error, IoResultExt, Result};
use cvx_object::ObjectId;
use cvx_pack::{Pack, PackInput, write_pack_with_algorithm};

use crate::Repository;

/// Result of a repository garbage-collection pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcReport {
    /// Number of loose objects written into the new pack.
    pub packed_objects: usize,
    /// Number of loose objects deleted by `--prune`.
    pub pruned_objects: usize,
    /// Number of old packfiles removed by a full repack.
    pub removed_packs: usize,
    /// Path to the pack written by this run, when one was needed.
    pub pack_path: Option<PathBuf>,
}

impl Repository {
    /// Consolidates reachable loose objects into a pack.
    ///
    /// By default only loose objects are packed and existing packs are left in
    /// place. With `prune`, all reachable objects are repacked, loose objects
    /// are removed and old packs without a `.keep` file are deleted.
    pub fn gc(&self, prune: bool) -> Result<GcReport> {
        let mut roots = Vec::new();
        for (_, oid) in self.refs().list_refs("refs")? {
            roots.push(oid);
        }
        if let Some(oid) = self.head()?.oid() {
            roots.push(oid);
        }
        for entry in self.read_index()?.entries() {
            roots.push(entry.oid);
        }
        roots.extend(reflog_roots(self)?);
        let reachable: BTreeSet<ObjectId> = self.reachable_objects(&roots)?.into_iter().collect();

        let mut inputs_data = Vec::new();
        for oid in &reachable {
            if prune || self.objects().loose().contains(*oid) {
                let object = self.objects().read(*oid)?;
                inputs_data.push((object.kind, object.data));
            }
        }

        let mut pack_path = None;
        let mut new_index_path = None;
        if !inputs_data.is_empty() {
            let inputs: Vec<PackInput<'_>> = inputs_data
                .iter()
                .map(|(kind, data)| PackInput { kind: *kind, data })
                .collect();
            let outcome = write_pack_with_algorithm(
                self.control_dir().join("objects/pack"),
                &inputs,
                self.hash_algorithm(),
            )?;
            let pack = Pack::open_with_algorithm(&outcome.index_path, self.hash_algorithm())?;
            for oid in &reachable {
                pack.read(*oid)?;
            }
            pack_path = Some(outcome.pack_path.clone());
            new_index_path = Some(outcome.index_path);
        }

        if !prune {
            for (kind, data) in &inputs_data {
                let oid = cvx_object::hash_object_with(self.hash_algorithm(), *kind, data);
                remove_loose(self, oid)?;
            }
        }

        let mut pruned_objects = 0;
        if prune {
            for oid in self.objects().iter_oids()? {
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

        let removed_packs = if prune {
            remove_old_packs(
                &self.control_dir().join("objects/pack"),
                pack_path.as_deref(),
                new_index_path.as_deref(),
            )?
        } else {
            0
        };

        Ok(GcReport {
            packed_objects: inputs_data.len(),
            pruned_objects,
            removed_packs,
            pack_path,
        })
    }
}

fn remove_loose(repository: &Repository, oid: ObjectId) -> Result<()> {
    let path = repository.objects().loose().path_for(oid);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(Error::io(path.display().to_string(), error)),
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_dir(parent);
    }
    Ok(())
}

fn remove_old_packs(
    pack_dir: &Path,
    keep_pack: Option<&Path>,
    keep_index: Option<&Path>,
) -> Result<usize> {
    let entries = match std::fs::read_dir(pack_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(Error::io(pack_dir.display().to_string(), error)),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| Error::io(pack_dir.display().to_string(), error))?
            .path();
        let extension = path.extension().and_then(|extension| extension.to_str());
        if extension != Some("pack") && extension != Some("idx") {
            continue;
        }
        if Some(path.as_path()) == keep_pack || Some(path.as_path()) == keep_index {
            continue;
        }
        let stem = path.with_extension("");
        if stem.with_extension("keep").is_file() {
            continue;
        }
        paths.push(path);
    }

    for path in paths
        .iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "idx"))
    {
        std::fs::remove_file(path).path_context(path)?;
    }
    let mut removed = 0;
    for path in paths
        .iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "pack"))
    {
        std::fs::remove_file(path).path_context(path)?;
        removed += 1;
    }
    Ok(removed)
}

fn reflog_roots(repository: &Repository) -> Result<Vec<ObjectId>> {
    let mut roots = Vec::new();
    let logs = repository.control_dir().join("logs");
    collect_reflog_dir(&logs, &mut roots)?;
    for name in [
        "ORIG_HEAD",
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "FETCH_HEAD",
    ] {
        let path = repository.control_dir().join(name);
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(Error::io(path.display().to_string(), error)),
        };
        for line in contents.lines() {
            let Some(token) = line.split_whitespace().next() else {
                continue;
            };
            if let Ok(oid) = ObjectId::from_hex(token)
                && !oid.is_zero()
            {
                roots.push(oid);
            }
        }
    }
    Ok(roots)
}

fn collect_reflog_dir(directory: &Path, roots: &mut Vec<ObjectId>) -> Result<()> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(Error::io(directory.display().to_string(), error)),
    };
    for entry in entries {
        let path = entry
            .map_err(|error| Error::io(directory.display().to_string(), error))?
            .path();
        if path.is_dir() {
            collect_reflog_dir(&path, roots)?;
            continue;
        }
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(_) => continue,
        };
        for line in contents.lines() {
            let mut fields = line.split_whitespace();
            for _ in 0..2 {
                if let Some(field) = fields.next()
                    && let Ok(oid) = ObjectId::from_hex(field)
                    && !oid.is_zero()
                {
                    roots.push(oid);
                }
            }
        }
    }
    Ok(())
}
