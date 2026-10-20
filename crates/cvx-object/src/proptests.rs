use proptest::prelude::*;

use crate::{Commit, Tag, Tree};

proptest! {
    #[test]
    fn commit_parser_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        let _ = Commit::parse(&bytes);
    }

    #[test]
    fn tree_parser_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        let _ = Tree::parse(&bytes);
    }

    #[test]
    fn tag_parser_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        let _ = Tag::parse(&bytes);
    }
}
