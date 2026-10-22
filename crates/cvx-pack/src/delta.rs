use cvx_core::{Error, Result};

const MAX_RESULT_SIZE: u64 = 1024 * 1024 * 1024;

/// Applies a Git pack delta to a base object.
pub fn apply_delta(base: &[u8], delta: &[u8]) -> Result<Vec<u8>> {
    let (base_size, mut position) = read_varint(delta, 0)
        .ok_or_else(|| Error::corrupt("pack delta is missing the base size"))?;
    let (result_size, next) = read_varint(delta, position)
        .ok_or_else(|| Error::corrupt("pack delta is missing the result size"))?;
    position = next;

    if base_size != base.len() as u64 {
        return Err(Error::corrupt(format!(
            "pack delta expects a {base_size}-byte base, got {}",
            base.len()
        )));
    }
    if result_size > MAX_RESULT_SIZE {
        return Err(Error::corrupt("pack delta result is too large"));
    }

    let mut result = Vec::with_capacity(result_size as usize);

    while position < delta.len() {
        let opcode = delta[position];
        position += 1;

        if opcode & 0x80 != 0 {
            let mut offset = 0u64;
            for byte_index in 0..4 {
                if opcode & (1 << byte_index) != 0 {
                    let byte = *delta
                        .get(position)
                        .ok_or_else(|| Error::corrupt("pack delta copy offset is truncated"))?;
                    position += 1;
                    offset |= u64::from(byte) << (byte_index * 8);
                }
            }

            let mut size = 0u64;
            for byte_index in 0..3 {
                if opcode & (1 << (byte_index + 4)) != 0 {
                    let byte = *delta
                        .get(position)
                        .ok_or_else(|| Error::corrupt("pack delta copy size is truncated"))?;
                    position += 1;
                    size |= u64::from(byte) << (byte_index * 8);
                }
            }
            if size == 0 {
                size = 0x10000;
            }

            let start = usize::try_from(offset)
                .map_err(|_| Error::corrupt("pack delta copy offset is too large"))?;
            let length = usize::try_from(size)
                .map_err(|_| Error::corrupt("pack delta copy size is too large"))?;
            let end = start
                .checked_add(length)
                .ok_or_else(|| Error::corrupt("pack delta copy range overflow"))?;
            if end > base.len() {
                return Err(Error::corrupt("pack delta copies outside the base object"));
            }
            result.extend_from_slice(&base[start..end]);
        } else if opcode != 0 {
            let end = position
                .checked_add(usize::from(opcode))
                .ok_or_else(|| Error::corrupt("pack delta insert length overflow"))?;
            if end > delta.len() {
                return Err(Error::corrupt("pack delta insert is truncated"));
            }
            result.extend_from_slice(&delta[position..end]);
            position = end;
        } else {
            return Err(Error::corrupt("pack delta contains a reserved opcode"));
        }
    }

    if result.len() as u64 != result_size {
        return Err(Error::corrupt(format!(
            "pack delta produced {} bytes, expected {result_size}",
            result.len()
        )));
    }

    Ok(result)
}

fn read_varint(data: &[u8], mut position: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0u32;

    loop {
        let byte = *data.get(position)?;
        position += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }

    Some((value, position))
}

/// Builds a Git pack delta that transforms `base` into `target`.
///
/// The encoder uses a small rolling block index and is intentionally
/// conservative: it falls back to inserts when no sufficiently long match is
/// found. The result is always accepted by [`apply_delta`].
pub(crate) fn create_delta(base: &[u8], target: &[u8]) -> Vec<u8> {
    const BLOCK: usize = 16;
    const MAX_CANDIDATES: usize = 8;

    let mut output = Vec::new();
    write_varint(base.len() as u64, &mut output);
    write_varint(target.len() as u64, &mut output);

    if target.is_empty() {
        return output;
    }
    if base.is_empty() {
        write_insert(target, &mut output);
        return output;
    }

    let mut blocks: std::collections::HashMap<u64, Vec<usize>> = std::collections::HashMap::new();
    if base.len() >= BLOCK {
        for position in 0..=base.len() - BLOCK {
            let hash = hash_block(&base[position..position + BLOCK]);
            let positions = blocks.entry(hash).or_default();
            if positions.len() < MAX_CANDIDATES {
                positions.push(position);
            }
        }
    }

    let mut position = 0;
    let mut insert_start = 0;
    while position < target.len() {
        let mut matched = None;
        if position + BLOCK <= target.len() {
            let hash = hash_block(&target[position..position + BLOCK]);
            if let Some(candidates) = blocks.get(&hash) {
                for candidate in candidates {
                    if base[*candidate..*candidate + BLOCK] == target[position..position + BLOCK] {
                        let mut length = BLOCK;
                        while *candidate + length < base.len()
                            && position + length < target.len()
                            && base[*candidate + length] == target[position + length]
                        {
                            length += 1;
                        }
                        matched = Some((*candidate, length));
                        break;
                    }
                }
            }
        }

        match matched {
            Some((offset, length)) => {
                if insert_start < position {
                    write_insert(&target[insert_start..position], &mut output);
                }
                write_copy(offset, length, &mut output);
                position += length;
                insert_start = position;
            }
            None => position += 1,
        }
    }
    if insert_start < target.len() {
        write_insert(&target[insert_start..], &mut output);
    }
    output
}

fn hash_block(block: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in block {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn write_varint(mut value: u64, output: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn write_insert(bytes: &[u8], output: &mut Vec<u8>) {
    for chunk in bytes.chunks(127) {
        output.push(chunk.len() as u8);
        output.extend_from_slice(chunk);
    }
}

fn write_copy(offset: usize, length: usize, output: &mut Vec<u8>) {
    let mut offset = offset as u64;
    let mut length = length as u64;
    while length > 0 {
        let chunk = length.min(0x00ff_ffff) as usize;
        let mut opcode = 0x80u8;
        let mut bytes = Vec::new();
        for index in 0..4 {
            let byte = ((offset >> (index * 8)) & 0xff) as u8;
            if byte != 0 {
                opcode |= 1 << index;
                bytes.push(byte);
            }
        }
        for index in 0..3 {
            let byte = ((chunk as u64 >> (index * 8)) & 0xff) as u8;
            if byte != 0 {
                opcode |= 1 << (index + 4);
                bytes.push(byte);
            }
        }
        output.push(opcode);
        output.extend_from_slice(&bytes);
        offset += chunk as u64;
        length -= chunk as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_copy_delta() {
        // base size 5, result size 5, copy offset 0 size 5
        let delta = [5, 5, 0x90, 5];
        assert_eq!(apply_delta(b"hello", &delta).unwrap(), b"hello");
    }

    #[test]
    fn applies_insert_delta() {
        // base size 0, result size 5, insert "world"
        let delta = [0, 5, 5, b'w', b'o', b'r', b'l', b'd'];
        assert_eq!(apply_delta(b"", &delta).unwrap(), b"world");
    }

    #[test]
    fn applies_mixed_delta() {
        // result: "hello world"
        let delta = [5, 11, 0x90, 5, 6, b' ', b'w', b'o', b'r', b'l', b'd'];
        assert_eq!(apply_delta(b"hello", &delta).unwrap(), b"hello world");
    }

    #[test]
    fn rejects_bad_deltas() {
        assert!(apply_delta(b"hello", &[6, 5, 0x90, 5]).is_err());
        assert!(apply_delta(b"hello", &[5, 10, 0x90, 5]).is_err());
        assert!(apply_delta(b"hello", &[5, 5, 0x00]).is_err());
        assert!(apply_delta(b"hello", &[5, 5, 0x90]).is_err());
    }

    #[test]
    fn creates_roundtrippable_deltas() {
        let base = b"line one\nline two\nline three\nline four\n";
        let target = b"line one\nline two changed\nline three\nline four\nline five\n";
        let delta = create_delta(base, target);
        assert_eq!(apply_delta(base, &delta).unwrap(), target);
        assert!(delta.len() < target.len() + 16);
    }

    #[test]
    fn creates_empty_and_insert_deltas() {
        assert_eq!(
            apply_delta(b"base", &create_delta(b"base", b"")).unwrap(),
            b""
        );
        assert_eq!(
            apply_delta(b"", &create_delta(b"", b"new content")).unwrap(),
            b"new content"
        );
    }

    #[test]
    fn creates_large_deltas_with_multiple_copies() {
        let base = vec![b'x'; 200_000];
        let target = base.clone();
        let delta = create_delta(&base, &target);
        assert_eq!(apply_delta(&base, &delta).unwrap(), target);
        assert!(delta.len() < 100);
    }
}
