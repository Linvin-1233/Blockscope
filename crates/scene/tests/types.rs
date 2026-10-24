use litematic_scene::{BlockStateKey, PackedIndices};
use std::collections::BTreeMap;

#[test]
fn palette_bit_width_matches_litematica() {
    let cases = [
        (1usize, 2u8),
        (2, 2),
        (3, 2),
        (4, 2),
        (5, 3),
        (16, 4),
        (17, 5),
        (32, 5),
        (33, 6),
        (64, 6),
        (65, 7),
    ];

    for (palette_len, expected) in cases {
        assert_eq!(
            PackedIndices::bits_for_palette(palette_len),
            expected,
            "palette length {palette_len}"
        );
    }
}

#[test]
fn packed_indices_read_lsb_first_values() {
    // bits = 2, values [0, 1, 2] -> 0b10_01_00 = 36
    let packed = PackedIndices::new(2, vec![36], 3).unwrap();

    assert_eq!(packed.bits(), 2);
    assert_eq!(packed.volume(), 3);
    assert_eq!(packed.get(0), 0);
    assert_eq!(packed.get(1), 1);
    assert_eq!(packed.get(2), 2);
    assert_eq!(packed.iter().collect::<Vec<_>>(), vec![0, 1, 2]);
}

#[test]
fn packed_indices_cross_long_boundary() {
    // bits = 3, volume = 22 -> 66 bits. Entry 21 occupies bit 63 and bits 0..1.
    let first = (1u64 << 63) as i64;
    let packed = PackedIndices::new(3, vec![first, 2], 22).unwrap();

    assert_eq!(packed.get(21), 5);
    assert_eq!(packed.get(0), 0);
}

#[test]
fn packed_indices_reject_wrong_long_count() {
    assert!(PackedIndices::new(2, vec![], 1).is_none());
    assert!(PackedIndices::new(2, vec![0, 0], 1).is_none());
    assert!(PackedIndices::new(0, vec![0], 1).is_none());
    assert!(PackedIndices::new(33, vec![0], 1).is_none());
}

#[test]
fn expected_long_count_rounds_up() {
    assert_eq!(PackedIndices::expected_long_count(1, 2), Some(1));
    assert_eq!(PackedIndices::expected_long_count(31, 2), Some(1));
    assert_eq!(PackedIndices::expected_long_count(32, 2), Some(1));
    assert_eq!(PackedIndices::expected_long_count(33, 2), Some(2));
}

#[test]
fn block_state_key_orders_and_canonicalizes_properties() {
    let mut properties = BTreeMap::new();
    properties.insert("half".to_string(), "bottom".to_string());
    properties.insert("facing".to_string(), "east".to_string());
    let key = BlockStateKey::new("minecraft:oak_stairs", properties);

    assert_eq!(
        key.canonical(),
        "minecraft:oak_stairs[facing=east,half=bottom]"
    );
    assert!(!key.is_air());
    assert!(BlockStateKey::simple("minecraft:air").is_air());
    assert!(BlockStateKey::simple("minecraft:cave_air").is_air());
    assert!(BlockStateKey::simple("minecraft:void_air").is_air());
    assert!(!BlockStateKey::simple("minecraft:structure_void").is_air());
}
