use proptest::prelude::*;

use crate::Config;

proptest! {
    #[test]
    fn config_parser_never_panics(
        characters in prop::collection::vec(any::<char>(), 0..512),
    ) {
        let input: String = characters.into_iter().collect();
        let _ = Config::parse_str(&input, "proptest");
    }
}
