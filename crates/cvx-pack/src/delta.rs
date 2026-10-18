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
}
