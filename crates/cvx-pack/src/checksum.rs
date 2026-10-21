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
