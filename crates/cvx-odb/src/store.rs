use std::path::{Path, PathBuf};

use cvx_core::{Error, Result};
use cvx_object::{ObjectId, ObjectKind};
use cvx_pack::Pack;

use crate::loose::{LooseStore, StoredObject};

/// Combined loose and packed object database.
#[derive(Debug)]
pub struct ObjectStore {
    loose: LooseStore,
    packs: Vec<Pack>,
}

impl ObjectStore {
    /// Opens the object database under an `objects/` directory.
    pub fn open(objects_dir: impl Into<PathBuf>) -> Result<Self> {
        let objects_dir = objects_dir.into();
        let loose = LooseStore::new(&objects_dir);
        let packs = open_packs(&objects_dir.join("pack"))?;
        Ok(Self { loose, packs })
    }

    /// Writes a loose object.
    pub fn write(&self, kind: ObjectKind, data: &[u8]) -> Result<ObjectId> {
        self.loose.write(kind, data)
    }

    /// Reads an object, checking loose storage first and then packfiles.
    pub fn read(&self, oid: ObjectId) -> Result<StoredObject> {
        if self.loose.contains(oid) {
            return self.loose.read(oid);
        }

        for pack in &self.packs {
            if pack.contains(oid)? {
                let object = pack.read(oid)?;
                return Ok(StoredObject {
                    kind: object.kind,
                    data: object.data,
                });
            }
        }

        Err(Error::object_not_found(oid.to_string()))
    }

    /// Returns whether an object is present.
    pub fn contains(&self, oid: ObjectId) -> Result<bool> {
        if self.loose.contains(oid) {
            return Ok(true);
        }
        for pack in &self.packs {
            if pack.contains(oid)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns the loose object store.
    pub fn loose(&self) -> &LooseStore {
        &self.loose
    }

    /// Returns the number of opened packs.
    pub fn pack_count(&self) -> usize {
        self.packs.len()
    }
}

fn open_packs(pack_dir: &Path) -> Result<Vec<Pack>> {
    let entries = match std::fs::read_dir(pack_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(Error::io(pack_dir.display().to_string(), error)),
    };

    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| Error::io(pack_dir.display().to_string(), error))?;
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "idx") {
            paths.push(path);
        }
    }
    paths.sort();

    let mut packs = Vec::with_capacity(paths.len());
    for path in paths {
        packs.push(Pack::open(&path)?);
    }
    Ok(packs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_an_empty_object_store() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = ObjectStore::open(temp.path().join("objects")).expect("open");
        assert_eq!(store.pack_count(), 0);
    }

    #[test]
    fn writes_and_reads_loose_objects() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = ObjectStore::open(temp.path().join("objects")).expect("open");
        let oid = store.write(ObjectKind::Blob, b"hello\n").expect("write");
        assert_eq!(store.read(oid).expect("read").data, b"hello\n");
        assert!(store.contains(oid).expect("contains"));
    }
}
