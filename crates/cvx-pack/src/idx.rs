use cvx_core::{Error, Result};
use cvx_object::{HashAlgorithm, ObjectId};

const IDX_MAGIC: &[u8; 4] = b"\xfftOc";
const VERSION: u32 = 2;
const FANOUT_BYTES: usize = 256 * 4;

/// Parsed version-2 pack index.
#[derive(Debug, Clone)]
pub(crate) struct PackIndex {
    bytes: Vec<u8>,
    count: usize,
    names_start: usize,
    offsets_start: usize,
    large_start: usize,
    large_count: usize,
    algorithm: HashAlgorithm,
}

impl PackIndex {
    pub(crate) fn parse(bytes: Vec<u8>, algorithm: HashAlgorithm) -> Result<Self> {
        let checksum_len = algorithm.oid_len();
        if bytes.len() < 8 + FANOUT_BYTES + 2 * checksum_len {
            return Err(Error::corrupt("pack index is too short"));
        }
        if &bytes[..4] != IDX_MAGIC {
            return Err(Error::unsupported(
                "version-1 pack indexes (only version 2 is supported)",
            ));
        }

        let version = read_u32(&bytes, 4)?;
        if version != VERSION {
            return Err(Error::unsupported(format!("pack index version {version}")));
        }

        let payload_len = bytes.len() - checksum_len;
        let checksum = crate::checksum::checksum(algorithm, &bytes[..payload_len]);
        if checksum.as_slice() != &bytes[payload_len..] {
            return Err(Error::corrupt("pack index checksum does not match"));
        }

        let count = read_u32(&bytes, 8 + 255 * 4)? as usize;
        let names_start = 8 + FANOUT_BYTES;
        let offsets_start = names_start
            .checked_add(
                count
                    .checked_mul(algorithm.oid_len())
                    .ok_or_else(|| Error::corrupt("pack index object count overflow"))?,
            )
            .and_then(|value| value.checked_add(count.checked_mul(4)?))
            .ok_or_else(|| Error::corrupt("pack index offset overflow"))?;
        let large_start = offsets_start
            .checked_add(
                count
                    .checked_mul(4)
                    .ok_or_else(|| Error::corrupt("pack index offset table overflow"))?,
            )
            .ok_or_else(|| Error::corrupt("pack index offset table overflow"))?;

        let pack_checksum_start = bytes.len() - 2 * checksum_len;
        if large_start > pack_checksum_start {
            return Err(Error::corrupt("pack index tables are truncated"));
        }
        let large_bytes = pack_checksum_start - large_start;
        if large_bytes % 8 != 0 {
            return Err(Error::corrupt(
                "pack index has a misaligned large-offset table",
            ));
        }

        Ok(Self {
            bytes,
            count,
            names_start,
            offsets_start,
            large_start,
            large_count: large_bytes / 8,
            algorithm,
        })
    }

    pub(crate) fn object_count(&self) -> usize {
        self.count
    }

    pub(crate) fn oids(&self) -> Result<Vec<ObjectId>> {
        let mut oids = Vec::with_capacity(self.count);
        for index in 0..self.count {
            let oid_len = self.algorithm.oid_len();
            let start = self.names_start + index * oid_len;
            let oid = ObjectId::from_slice(
                self.bytes
                    .get(start..start + oid_len)
                    .ok_or_else(|| Error::corrupt("pack index is truncated"))?,
            )
            .map_err(|error| Error::corrupt(format!("pack index has an invalid id: {error}")))?;
            oids.push(oid);
        }
        Ok(oids)
    }

    pub(crate) fn pack_checksum(&self) -> Result<&[u8]> {
        let checksum_len = self.algorithm.oid_len();
        let start = self.bytes.len() - 2 * checksum_len;
        Ok(&self.bytes[start..start + checksum_len])
    }

    /// Finds an object and returns its offset inside the pack.
    pub(crate) fn find(&self, oid: ObjectId) -> Result<Option<u64>> {
        if oid.len() != self.algorithm.oid_len() {
            return Ok(None);
        }
        let target = oid.as_bytes();
        let first = usize::from(target[0]);
        let start = if first == 0 {
            0
        } else {
            read_u32(&self.bytes, 8 + (first - 1) * 4)? as usize
        };
        let end = read_u32(&self.bytes, 8 + first * 4)? as usize;

        if start > end || end > self.count {
            return Err(Error::corrupt("pack index fanout table is invalid"));
        }

        let mut low = start;
        let mut high = end;
        while low < high {
            let middle = low + (high - low) / 2;
            let name_start = self.names_start + middle * self.algorithm.oid_len();
            let candidate = &self.bytes[name_start..name_start + self.algorithm.oid_len()];
            match candidate.cmp(target) {
                std::cmp::Ordering::Less => low = middle + 1,
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => return self.offset(middle).map(Some),
            }
        }

        Ok(None)
    }

    fn offset(&self, index: usize) -> Result<u64> {
        let raw = read_u32(&self.bytes, self.offsets_start + index * 4)?;
        if raw & 0x8000_0000 == 0 {
            return Ok(u64::from(raw));
        }

        let large_index = (raw & 0x7fff_ffff) as usize;
        if large_index >= self.large_count {
            return Err(Error::corrupt(
                "pack index large-offset reference is out of range",
            ));
        }
        let start = self.large_start + large_index * 8;
        let value = read_u64(&self.bytes, start)?;
        Ok(value)
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| Error::corrupt("pack index is truncated"))?;
    let array: [u8; 4] = slice.try_into().expect("length checked");
    Ok(u32::from_be_bytes(array))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64> {
    let slice = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| Error::corrupt("pack index is truncated"))?;
    let array: [u8; 8] = slice.try_into().expect("length checked");
    Ok(u64::from_be_bytes(array))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_garbage() {
        assert!(PackIndex::parse(b"not an index".to_vec(), HashAlgorithm::Sha1).is_err());
    }

    #[test]
    fn rejects_checksum_mismatch() {
        let mut bytes = vec![0u8; 8 + FANOUT_BYTES + 40];
        bytes[..4].copy_from_slice(IDX_MAGIC);
        bytes[4..8].copy_from_slice(&VERSION.to_be_bytes());
        assert!(PackIndex::parse(bytes, HashAlgorithm::Sha1).is_err());
    }
}
