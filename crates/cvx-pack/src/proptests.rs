use proptest::prelude::*;

use crate::delta::apply_delta;
use crate::idx::PackIndex;
use crate::ingest::ingest_pack;

proptest! {
    #[test]
    fn delta_parser_never_panics(
        base in prop::collection::vec(any::<u8>(), 0..2048),
        delta in prop::collection::vec(any::<u8>(), 0..2048),
    ) {
        let _ = apply_delta(&base, &delta);
    }

    #[test]
    fn index_parser_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        let _ = PackIndex::parse(bytes.clone(), cvx_object::HashAlgorithm::Sha1);
        let _ = PackIndex::parse(bytes, cvx_object::HashAlgorithm::Sha256);
    }

    #[test]
    fn pack_ingest_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        let _ = ingest_pack(&bytes);
        let _ = crate::ingest_pack_with_algorithm(&bytes, cvx_object::HashAlgorithm::Sha256);
    }
}
