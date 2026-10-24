use litematic_scene::PackedIndices;
use proptest::prelude::*;

proptest! {
    #[test]
    fn packed_values_round_trip(
        bits in 2u8..=16,
        raw_values in prop::collection::vec(any::<u16>(), 1..=64),
    ) {
        let mask = (1u32 << bits) - 1;
        let values: Vec<u32> = raw_values
            .into_iter()
            .map(|value| u32::from(value) & mask)
            .collect();

        let packed = PackedIndices::from_values(bits, &values).unwrap();
        prop_assert_eq!(packed.iter().collect::<Vec<_>>(), values);
    }
}

#[test]
fn rejects_values_above_the_bit_mask() {
    assert!(PackedIndices::from_values(2, &[4]).is_none());
}
