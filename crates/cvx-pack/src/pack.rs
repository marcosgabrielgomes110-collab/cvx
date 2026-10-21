use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

use cvx_core::{Error, Result};
use cvx_object::{HashAlgorithm, ObjectId, ObjectKind, hash_object_with};
use flate2::read::ZlibDecoder;

use crate::delta::apply_delta;
use crate::idx::PackIndex;

const PACK_HEADER_LEN: usize = 12;
const MAX_OBJECT_SIZE: u64 = 1024 * 1024 * 1024;
const MAX_DELTA_DEPTH: usize = 64;

/// A decoded object from a packfile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackObject {
    /// Object type.
    pub kind: ObjectKind,
    /// Uncompressed object payload.
    pub data: Vec<u8>,
}

/// A packfile and its index.
pub struct Pack {
    path: PathBuf,
    data: Vec<u8>,
    index: PackIndex,
    cache: RefCell<HashMap<u64, PackObject>>,
    object_count: usize,
    algorithm: HashAlgorithm,
}

impl fmt::Debug for Pack {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Pack")
            .field("path", &self.path)
            .field("object_count", &self.object_count)
            .finish_non_exhaustive()
    }
}

impl Pack {
    /// Opens a pack given its `.idx` file.
    pub fn open(idx_path: &Path) -> Result<Self> {
        Self::open_with_algorithm(idx_path, HashAlgorithm::Sha1)
    }

    /// Opens a pack using a specific repository object format.
    pub fn open_with_algorithm(idx_path: &Path, algorithm: HashAlgorithm) -> Result<Self> {
        let index_bytes = std::fs::read(idx_path)
            .map_err(|error| Error::io(idx_path.display().to_string(), error))?;
        let index = PackIndex::parse(index_bytes, algorithm)?;

        let pack_path = idx_path.with_extension("pack");
        let data = std::fs::read(&pack_path)
            .map_err(|error| Error::io(pack_path.display().to_string(), error))?;

        let checksum_len = algorithm.oid_len();
        if data.len() < PACK_HEADER_LEN + checksum_len {
            return Err(Error::corrupt(format!(
                "{} is too short to be a packfile",
                pack_path.display()
            )));
        }
        if &data[..4] != b"PACK" {
            return Err(Error::corrupt(format!(
                "{} has an invalid pack signature",
                pack_path.display()
            )));
        }

        let version = u32::from_be_bytes(data[4..8].try_into().expect("length checked"));
        if version != 2 && version != 3 {
            return Err(Error::unsupported(format!("packfile version {version}")));
        }

        let object_count =
            u32::from_be_bytes(data[8..12].try_into().expect("length checked")) as usize;
        if object_count != index.object_count() {
            return Err(Error::corrupt(format!(
                "{} has {object_count} objects but its index has {}",
                pack_path.display(),
                index.object_count()
            )));
        }

        let pack_checksum = &data[data.len() - checksum_len..];
        if index.pack_checksum()? != pack_checksum {
            return Err(Error::corrupt(format!(
                "{} does not match its index checksum",
                pack_path.display()
            )));
        }

        Ok(Self {
            path: pack_path,
            data,
            index,
            cache: RefCell::new(HashMap::new()),
            object_count,
            algorithm,
        })
    }

    /// Returns the pack path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the number of objects.
    pub fn object_count(&self) -> usize {
        self.object_count
    }

    /// Returns whether the pack contains an object.
    pub fn contains(&self, oid: ObjectId) -> Result<bool> {
        Ok(self.index.find(oid)?.is_some())
    }

    /// Returns every object identifier in the pack index.
    pub fn oids(&self) -> Result<Vec<ObjectId>> {
        self.index.oids()
    }

    /// Reads and verifies an object.
    pub fn read(&self, oid: ObjectId) -> Result<PackObject> {
        let Some(offset) = self.index.find(oid)? else {
            return Err(Error::object_not_found(oid.to_string()));
        };
        let object = self.read_at(offset, 0)?;
        if hash_object_with(self.algorithm, object.kind, &object.data) != oid {
            return Err(Error::corrupt(format!(
                "packed object {oid} does not match its identifier"
            )));
        }
        Ok(object)
    }

    fn read_at(&self, offset: u64, depth: usize) -> Result<PackObject> {
        if depth > MAX_DELTA_DEPTH {
            return Err(Error::corrupt("pack delta chain is too deep"));
        }
        if let Some(object) = self.cache.borrow().get(&offset).cloned() {
            return Ok(object);
        }

        let (type_code, size, data_start) = self.read_header(offset)?;
        let object = match type_code {
            1..=4 => {
                let kind = match type_code {
                    1 => ObjectKind::Commit,
                    2 => ObjectKind::Tree,
                    3 => ObjectKind::Blob,
                    4 => ObjectKind::Tag,
                    _ => unreachable!("range is 1..=4"),
                };
                PackObject {
                    kind,
                    data: self.decompress(data_start, size)?,
                }
            }
            6 => {
                let (distance, position) = self.read_ofs_delta(data_start)?;
                let base_offset = offset.checked_sub(distance).ok_or_else(|| {
                    Error::corrupt("pack offset delta points before the pack start")
                })?;
                let delta = self.decompress(position, size)?;
                let base = self.read_at(base_offset, depth + 1)?;
                PackObject {
                    kind: base.kind,
                    data: apply_delta(&base.data, &delta)?,
                }
            }
            7 => {
                let base_end = data_start + self.algorithm.oid_len();
                if base_end > self.object_data_end() {
                    return Err(Error::corrupt("pack ref delta is truncated"));
                }
                let base_oid =
                    ObjectId::from_slice(&self.data[data_start..base_end]).map_err(|error| {
                        Error::corrupt(format!("pack ref delta has an invalid base: {error}"))
                    })?;
                let Some(base_offset) = self.index.find(base_oid)? else {
                    return Err(Error::corrupt(format!(
                        "pack ref delta base {base_oid} is not present in the pack"
                    )));
                };
                let delta = self.decompress(base_end, size)?;
                let base = self.read_at(base_offset, depth + 1)?;
                PackObject {
                    kind: base.kind,
                    data: apply_delta(&base.data, &delta)?,
                }
            }
            other => {
                return Err(Error::corrupt(format!(
                    "pack contains unknown object type {other}"
                )));
            }
        };

        self.cache.borrow_mut().insert(offset, object.clone());
        Ok(object)
    }

    fn read_header(&self, offset: u64) -> Result<(u8, usize, usize)> {
        let offset =
            usize::try_from(offset).map_err(|_| Error::corrupt("pack offset is too large"))?;
        let mut position = offset;
        let first = self.byte(position)?;
        position += 1;

        let type_code = (first >> 4) & 0x07;
        let mut size = u64::from(first & 0x0f);
        let mut shift = 4u32;
        let mut byte = first;

        while byte & 0x80 != 0 {
            byte = self.byte(position)?;
            position += 1;
            size |= u64::from(byte & 0x7f) << shift;
            shift += 7;
            if shift >= 64 {
                return Err(Error::corrupt("pack object size overflows"));
            }
        }

        let size =
            usize::try_from(size).map_err(|_| Error::corrupt("pack object size is too large"))?;
        Ok((type_code, size, position))
    }

    fn read_ofs_delta(&self, start: usize) -> Result<(u64, usize)> {
        let mut position = start;
        let mut byte = self.byte(position)?;
        position += 1;
        let mut distance = u64::from(byte & 0x7f);

        while byte & 0x80 != 0 {
            byte = self.byte(position)?;
            position += 1;
            distance = distance
                .checked_add(1)
                .and_then(|value| value.checked_shl(7))
                .and_then(|value| value.checked_add(u64::from(byte & 0x7f)))
                .ok_or_else(|| Error::corrupt("pack offset delta overflows"))?;
        }

        Ok((distance, position))
    }

    fn decompress(&self, start: usize, size: usize) -> Result<Vec<u8>> {
        if size as u64 > MAX_OBJECT_SIZE {
            return Err(Error::corrupt(format!(
                "packed object is too large ({size} bytes)"
            )));
        }
        if start >= self.object_data_end() {
            return Err(Error::corrupt("pack object data is truncated"));
        }

        let compressed = &self.data[start..self.object_data_end()];
        let mut decoder = ZlibDecoder::new(compressed).take(size as u64 + 1);
        let mut output = Vec::with_capacity(size);
        decoder
            .read_to_end(&mut output)
            .map_err(|error| Error::io("failed to decompress packed object", error))?;

        if output.len() != size {
            return Err(Error::corrupt(format!(
                "packed object declares {size} bytes but produced {}",
                output.len()
            )));
        }
        Ok(output)
    }

    fn byte(&self, position: usize) -> Result<u8> {
        if position >= self.object_data_end() {
            return Err(Error::corrupt("pack object is truncated"));
        }
        Ok(self.data[position])
    }

    fn object_data_end(&self) -> usize {
        self.data.len().saturating_sub(self.algorithm.oid_len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    struct GitFixture {
        _temp: tempfile::TempDir,
        repo: PathBuf,
        global: PathBuf,
        home: PathBuf,
    }

    impl GitFixture {
        fn new() -> Option<Self> {
            if Command::new("git").arg("--version").output().is_err() {
                return None;
            }

            let temp = tempfile::tempdir().expect("tempdir");
            let repo = temp.path().join("repo");
            let home = temp.path().join("home");
            std::fs::create_dir(&repo).expect("repo");
            std::fs::create_dir(&home).expect("home");
            let global = temp.path().join("global.gitconfig");
            std::fs::write(&global, "").expect("config");

            let fixture = Self {
                _temp: temp,
                repo,
                global,
                home,
            };
            fixture.git(&["init", "-q", "-b", "main"]);
            Some(fixture)
        }

        fn git(&self, args: &[&str]) {
            let output = Command::new("git")
                .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
                .args(args)
                .current_dir(&self.repo)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", &self.global)
                .env("HOME", &self.home)
                .output()
                .expect("run git");
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        fn output(&self, args: &[&str]) -> String {
            let output = Command::new("git")
                .args(args)
                .current_dir(&self.repo)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", &self.global)
                .env("HOME", &self.home)
                .output()
                .expect("run git");
            assert!(output.status.success());
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }

        fn pack_index(&self) -> PathBuf {
            let pack_dir = self.repo.join(".git/objects/pack");
            let entry = std::fs::read_dir(&pack_dir)
                .expect("pack dir")
                .filter_map(|entry| entry.ok())
                .find(|entry| entry.path().extension().is_some_and(|ext| ext == "idx"))
                .expect("pack index");
            entry.path()
        }
    }

    #[test]
    fn reads_objects_from_a_real_pack() {
        let Some(fixture) = GitFixture::new() else {
            return;
        };

        for index in 0..20 {
            let content =
                format!("line one\nline two {index}\nshared text shared text shared text\n");
            std::fs::write(fixture.repo.join(format!("file-{index:02}.txt")), content)
                .expect("write");
        }
        fixture.git(&["add", "."]);
        fixture.git(&["commit", "-q", "-m", "pack me"]);
        fixture.git(&["gc", "-q", "--aggressive"]);

        let pack = Pack::open(&fixture.pack_index()).expect("open pack");
        assert!(pack.object_count() > 0);

        let commit_oid =
            ObjectId::from_hex(&fixture.output(&["rev-parse", "HEAD"])).expect("commit id");
        let commit = pack.read(commit_oid).expect("read commit");
        assert_eq!(commit.kind, ObjectKind::Commit);
        assert!(commit.data.starts_with(b"tree "));

        for index in 0..20 {
            let path = format!("HEAD:file-{index:02}.txt");
            let blob_oid = ObjectId::from_hex(&fixture.output(&["rev-parse", path.as_str()]))
                .expect("blob id");
            let blob = pack.read(blob_oid).expect("read blob");
            assert_eq!(blob.kind, ObjectKind::Blob);
            assert!(blob.data.starts_with(b"line one\n"));
        }
    }

    #[test]
    fn reads_a_delta_chain_when_present() {
        let Some(fixture) = GitFixture::new() else {
            return;
        };

        let mut content = String::new();
        for line in 0..2000 {
            content.push_str(&format!("common line {line}\n"));
        }
        for revision in 0..10 {
            let mut version = content.clone();
            version.push_str(&format!("revision {revision}\n"));
            std::fs::write(fixture.repo.join("large.txt"), version).expect("write");
            fixture.git(&["add", "large.txt"]);
            let message = format!("revision {revision}");
            fixture.git(&["commit", "-q", "-m", &message]);
        }
        fixture.git(&["gc", "-q", "--aggressive"]);

        let Some(entry) = std::fs::read_dir(fixture.repo.join(".git/objects/pack"))
            .expect("pack dir")
            .filter_map(|entry| entry.ok())
            .find(|entry| entry.path().extension().is_some_and(|ext| ext == "idx"))
        else {
            return;
        };
        let pack = Pack::open(&entry.path()).expect("open pack");

        for revision in 0..10 {
            let path = format!("HEAD~{revision}:large.txt");
            let oid = ObjectId::from_hex(&fixture.output(&["rev-parse", path.as_str()]))
                .expect("blob id");
            let blob = pack.read(oid).expect("read delta blob");
            assert!(
                blob.data
                    .ends_with(format!("revision {}\n", 9 - revision).as_bytes())
            );
        }
    }

    #[test]
    #[allow(clippy::permissions_set_readonly_false)]
    fn rejects_a_truncated_pack() {
        let Some(fixture) = GitFixture::new() else {
            return;
        };
        std::fs::write(fixture.repo.join("file"), "content").expect("write");
        fixture.git(&["add", "."]);
        fixture.git(&["commit", "-q", "-m", "one"]);
        fixture.git(&["gc", "-q"]);
        let idx = fixture.pack_index();
        let pack_path = idx.with_extension("pack");
        let mut bytes = std::fs::read(&pack_path).expect("read pack");
        bytes.truncate(bytes.len() / 2);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&pack_path, std::fs::Permissions::from_mode(0o644))
                .expect("permissions");
        }
        #[cfg(not(unix))]
        {
            let mut permissions = std::fs::metadata(&pack_path)
                .expect("metadata")
                .permissions();
            permissions.set_readonly(false);
            std::fs::set_permissions(&pack_path, permissions).expect("permissions");
        }
        std::fs::remove_file(&pack_path).expect("remove pack");
        std::fs::write(&pack_path, bytes).expect("truncate");

        assert!(Pack::open(&idx).is_err());
    }
}
