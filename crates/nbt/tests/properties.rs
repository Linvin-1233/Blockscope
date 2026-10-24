use litematic_nbt::{DecodeLimits, decode};
use proptest::prelude::*;

proptest! {
    #[test]
    fn arbitrary_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..512)) {
        let _ = decode(&data, &DecodeLimits::small());
    }
}
