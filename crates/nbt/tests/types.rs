use litematic_nbt::{
    DecodeLimits, NbtCompound, NbtError, NbtErrorCode, NbtList, NbtTagType, NbtValue,
};

#[test]
fn default_limits_are_ordered() {
    let limits = DecodeLimits::default();
    assert!(limits.max_input_bytes > 0);
    assert!(limits.max_decompressed_bytes >= limits.max_input_bytes);
    assert!(limits.max_total_bytes >= limits.max_decompressed_bytes);
    assert!(limits.max_depth >= 16);
    assert!(limits.max_tags >= 1_000_000);
}

#[test]
fn tag_type_round_trips_known_tags() {
    let tags = [
        (0u8, NbtTagType::End),
        (1, NbtTagType::Byte),
        (2, NbtTagType::Short),
        (3, NbtTagType::Int),
        (4, NbtTagType::Long),
        (5, NbtTagType::Float),
        (6, NbtTagType::Double),
        (7, NbtTagType::ByteArray),
        (8, NbtTagType::String),
        (9, NbtTagType::List),
        (10, NbtTagType::Compound),
        (11, NbtTagType::IntArray),
        (12, NbtTagType::LongArray),
    ];

    for (byte, tag) in tags {
        assert_eq!(NbtTagType::from_u8(byte), Some(tag));
        assert_eq!(tag.as_u8(), byte);
    }
    assert_eq!(NbtTagType::from_u8(13), None);
}

#[test]
fn value_reports_its_tag_type() {
    assert_eq!(NbtValue::Byte(1).tag_type(), NbtTagType::Byte);
    assert_eq!(NbtValue::Short(1).tag_type(), NbtTagType::Short);
    assert_eq!(NbtValue::Int(1).tag_type(), NbtTagType::Int);
    assert_eq!(NbtValue::Long(1).tag_type(), NbtTagType::Long);
    assert_eq!(NbtValue::Float(1.0).tag_type(), NbtTagType::Float);
    assert_eq!(NbtValue::Double(1.0).tag_type(), NbtTagType::Double);
    assert_eq!(
        NbtValue::ByteArray(vec![1]).tag_type(),
        NbtTagType::ByteArray
    );
    assert_eq!(NbtValue::String("x".into()).tag_type(), NbtTagType::String);
    assert_eq!(
        NbtValue::List(NbtList::new(NbtTagType::End, vec![])).tag_type(),
        NbtTagType::List
    );
    assert_eq!(
        NbtValue::Compound(NbtCompound::default()).tag_type(),
        NbtTagType::Compound
    );
    assert_eq!(NbtValue::IntArray(vec![1]).tag_type(), NbtTagType::IntArray);
    assert_eq!(
        NbtValue::LongArray(vec![1]).tag_type(),
        NbtTagType::LongArray
    );
}

#[test]
fn compound_keys_are_sorted_and_last_value_wins() {
    let mut compound = NbtCompound::default();
    compound.insert("b".into(), NbtValue::Int(1));
    compound.insert("a".into(), NbtValue::Int(2));
    compound.insert("b".into(), NbtValue::Int(3));

    let keys: Vec<_> = compound.entries.keys().cloned().collect();
    assert_eq!(keys, vec!["a", "b"]);
    assert_eq!(compound.get("b"), Some(&NbtValue::Int(3)));
    assert_eq!(compound.len(), 2);
}

#[test]
fn error_display_contains_code_and_offset() {
    let err = NbtError::new(NbtErrorCode::TruncatedInput, 42, "unexpected end");
    let text = err.to_string();
    assert!(text.contains("TRUNCATED_INPUT"));
    assert!(text.contains("42"));
    assert!(text.contains("unexpected end"));
}
