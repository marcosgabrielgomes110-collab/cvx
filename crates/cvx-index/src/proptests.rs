use proptest::prelude::*;

use crate::Index;

proptest! {
    #[test]
    fn index_parser_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..8192)) {
        let _ = Index::parse(&bytes);
    }
}
