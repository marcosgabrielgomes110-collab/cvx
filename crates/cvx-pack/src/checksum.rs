use cvx_object::HashAlgorithm;

pub(crate) enum Hasher {
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
}

impl Hasher {
    pub(crate) fn new(algorithm: HashAlgorithm) -> Self {
        match algorithm {
            HashAlgorithm::Sha1 => {
                use sha1::Digest;
                Self::Sha1(sha1::Sha1::new())
            }
            HashAlgorithm::Sha256 => {
                use sha2::Digest;
                Self::Sha256(sha2::Sha256::new())
            }
        }
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::Sha1(hasher) => {
                use sha1::Digest;
                hasher.update(bytes);
            }
            Self::Sha256(hasher) => {
                use sha2::Digest;
                hasher.update(bytes);
            }
        }
    }

    pub(crate) fn finalize(self) -> Vec<u8> {
        match self {
            Self::Sha1(hasher) => {
                use sha1::Digest;
                hasher.finalize().to_vec()
            }
            Self::Sha256(hasher) => {
                use sha2::Digest;
                hasher.finalize().to_vec()
            }
        }
    }
}

/// Computes a pack/index checksum using the repository hash algorithm.
pub(crate) fn checksum(algorithm: HashAlgorithm, bytes: &[u8]) -> Vec<u8> {
    let mut hasher = Hasher::new(algorithm);
    hasher.update(bytes);
    hasher.finalize()
}

/// Computes a checksum by streaming a reader, without buffering the input.
pub(crate) fn checksum_reader(
    algorithm: HashAlgorithm,
    mut reader: impl std::io::Read,
) -> std::io::Result<Vec<u8>> {
    let mut buffer = [0u8; 64 * 1024];
    let mut hasher = Hasher::new(algorithm);
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize())
}
