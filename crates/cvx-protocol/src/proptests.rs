use proptest::prelude::*;

use crate::{parse_all, read_from};

proptest! {
    #[test]
    fn pkt_parser_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        let _ = parse_all(&bytes);
        let mut cursor = std::io::Cursor::new(bytes);
        while let Ok(Some(_)) = read_from(&mut cursor) {}
    }
}
