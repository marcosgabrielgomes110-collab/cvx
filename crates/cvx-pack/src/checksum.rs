use cvx_object::HashAlgorithm;

/// Computes a pack/index checksum using the repository hash algorithm.
pub(crate) fn checksum(algorithm: HashAlgorithm, bytes: &[u8]) -> Vec<u8> {
    match algorithm {
        HashAlgorithm::Sha1 => {
            use sha1::{Digest, Sha1};
            Sha1::digest(bytes).to_vec()
        }
        HashAlgorithm::Sha256 => {
            use sha2::{Digest, Sha256};
            Sha256::digest(bytes).to_vec()
        }
    }
}

/// Computes a checksum by streaming a reader, without buffering the input.
pub(crate) fn checksum_reader(
    algorithm: HashAlgorithm,
    mut reader: impl std::io::Read,
) -> std::io::Result<Vec<u8>> {
    let mut buffer = [0u8; 64 * 1024];
    match algorithm {
        HashAlgorithm::Sha1 => {
            use sha1::{Digest, Sha1};
            let mut hasher = Sha1::new();
            loop {
                let read = reader.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            Ok(hasher.finalize().to_vec())
        }
        HashAlgorithm::Sha256 => {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            loop {
                let read = reader.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            Ok(hasher.finalize().to_vec())
        }
    }
}
