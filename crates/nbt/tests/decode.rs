use flate2::Compression;
use flate2::write::{GzEncoder, ZlibEncoder};
use litematic_nbt::{
    DecodeLimits, NbtCompound, NbtErrorCode, NbtTagType, NbtValue, decode, detect_compression,
};
use std::io::Write;

fn named(tag: u8, name: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend_from_slice(&(name.len() as u16).to_be_bytes());
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(payload);
    out
}

fn root(children: &[u8]) -> Vec<u8> {
    let mut out = vec![10, 0, 0];
    out.extend_from_slice(children);
    out.push(0);
    out
}

fn byte_tag(name: &str, value: i8) -> Vec<u8> {
    named(1, name, &[value as u8])
}

fn short_tag(name: &str, value: i16) -> Vec<u8> {
    named(2, name, &value.to_be_bytes())
}

fn int_tag(name: &str, value: i32) -> Vec<u8> {
    named(3, name, &value.to_be_bytes())
}

fn long_tag(name: &str, value: i64) -> Vec<u8> {
    named(4, name, &value.to_be_bytes())
}

fn float_tag(name: &str, value: f32) -> Vec<u8> {
    named(5, name, &value.to_be_bytes())
}

fn double_tag(name: &str, value: f64) -> Vec<u8> {
    named(6, name, &value.to_be_bytes())
}

fn byte_array_tag(name: &str, values: &[i8]) -> Vec<u8> {
    let mut payload = (values.len() as i32).to_be_bytes().to_vec();
    payload.extend(values.iter().map(|v| *v as u8));
    named(7, name, &payload)
}

fn string_payload(raw: &[u8]) -> Vec<u8> {
    let mut payload = (raw.len() as u16).to_be_bytes().to_vec();
    payload.extend_from_slice(raw);
    payload
}

fn string_tag(name: &str, value: &str) -> Vec<u8> {
    named(8, name, &string_payload(value.as_bytes()))
}

fn raw_string_tag(name: &str, raw: &[u8]) -> Vec<u8> {
    named(8, name, &string_payload(raw))
}

fn list_tag(name: &str, element_type: u8, values: &[Vec<u8>]) -> Vec<u8> {
    let mut payload = vec![element_type];
    payload.extend_from_slice(&(values.len() as i32).to_be_bytes());
    for value in values {
        payload.extend_from_slice(value);
    }
    named(9, name, &payload)
}

fn compound_tag(name: &str, children: &[u8]) -> Vec<u8> {
    let mut payload = children.to_vec();
    payload.push(0);
    named(10, name, &payload)
}

fn int_array_tag(name: &str, values: &[i32]) -> Vec<u8> {
    let mut payload = (values.len() as i32).to_be_bytes().to_vec();
    for value in values {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    named(11, name, &payload)
}

fn long_array_tag(name: &str, values: &[i64]) -> Vec<u8> {
    let mut payload = (values.len() as i32).to_be_bytes().to_vec();
    for value in values {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    named(12, name, &payload)
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn compound(value: &NbtValue) -> &NbtCompound {
    match value {
        NbtValue::Compound(compound) => compound,
        other => panic!("expected compound, got {:?}", other.tag_type()),
    }
}

#[test]
fn decodes_all_scalar_tags() {
    let mut children = Vec::new();
    children.extend(byte_tag("byte", -7));
    children.extend(short_tag("short", -300));
    children.extend(int_tag("int", -70_000));
    children.extend(long_tag("long", -5_000_000_000));
    children.extend(float_tag("float", 1.5));
    children.extend(double_tag("double", 2.5));
    children.extend(string_tag("string", "hello"));

    let decoded = decode(&root(&children), &DecodeLimits::default()).unwrap();
    let root_tag = compound(&decoded.value);

    assert_eq!(root_tag.get("byte"), Some(&NbtValue::Byte(-7)));
    assert_eq!(root_tag.get("short"), Some(&NbtValue::Short(-300)));
    assert_eq!(root_tag.get("int"), Some(&NbtValue::Int(-70_000)));
    assert_eq!(root_tag.get("long"), Some(&NbtValue::Long(-5_000_000_000)));
    assert_eq!(root_tag.get("float"), Some(&NbtValue::Float(1.5)));
    assert_eq!(root_tag.get("double"), Some(&NbtValue::Double(2.5)));
    assert_eq!(
        root_tag.get("string"),
        Some(&NbtValue::String("hello".into()))
    );
}

#[test]
fn decodes_arrays_lists_and_nested_compounds() {
    let mut nested = int_tag("value", 9);
    nested.push(0);
    let mut children = Vec::new();
    children.extend(byte_array_tag("bytes", &[1, 2, 3]));
    children.extend(int_array_tag("ints", &[4, 5]));
    children.extend(long_array_tag("longs", &[6, 7]));
    children.extend(list_tag(
        "int_list",
        3,
        &[1i32.to_be_bytes().to_vec(), 2i32.to_be_bytes().to_vec()],
    ));
    children.extend(list_tag("compound_list", 10, &[nested]));
    children.extend(list_tag("empty_list", 0, &[]));

    let decoded = decode(&root(&children), &DecodeLimits::default()).unwrap();
    let root_tag = compound(&decoded.value);

    assert_eq!(
        root_tag.get("bytes"),
        Some(&NbtValue::ByteArray(vec![1, 2, 3]))
    );
    assert_eq!(root_tag.get("ints"), Some(&NbtValue::IntArray(vec![4, 5])));
    assert_eq!(
        root_tag.get("longs"),
        Some(&NbtValue::LongArray(vec![6, 7]))
    );

    match root_tag.get("int_list") {
        Some(NbtValue::List(list)) => {
            assert_eq!(list.element_type, NbtTagType::Int);
            assert_eq!(list.values, vec![NbtValue::Int(1), NbtValue::Int(2)]);
        }
        other => panic!("expected int list, got {:?}", other),
    }

    match root_tag.get("compound_list") {
        Some(NbtValue::List(list)) => {
            assert_eq!(list.element_type, NbtTagType::Compound);
            assert_eq!(list.values.len(), 1);
        }
        other => panic!("expected compound list, got {:?}", other),
    }

    match root_tag.get("empty_list") {
        Some(NbtValue::List(list)) => {
            assert_eq!(list.element_type, NbtTagType::End);
            assert!(list.values.is_empty());
        }
        other => panic!("expected empty list, got {:?}", other),
    }
}

#[test]
fn detects_compression_from_magic_bytes() {
    assert_eq!(
        detect_compression(&[0x1f, 0x8b, 0x08, 0x00]),
        litematic_nbt::Compression::Gzip
    );
    assert_eq!(
        detect_compression(&zlib(b"hello")),
        litematic_nbt::Compression::Zlib
    );
    assert_eq!(
        detect_compression(&[10, 0, 0]),
        litematic_nbt::Compression::Uncompressed
    );
}

#[test]
fn decodes_gzip_and_zlib_inputs() {
    let data = root(&int_tag("value", 42));

    let from_gzip = decode(&gzip(&data), &DecodeLimits::default()).unwrap();
    let from_zlib = decode(&zlib(&data), &DecodeLimits::default()).unwrap();

    assert_eq!(
        compound(&from_gzip.value).get("value"),
        Some(&NbtValue::Int(42))
    );
    assert_eq!(
        compound(&from_zlib.value).get("value"),
        Some(&NbtValue::Int(42))
    );
}

#[test]
fn decodes_modified_utf8_nul_chinese_and_emoji() {
    let nul = raw_string_tag("nul", &[0xc0, 0x80, b'a']);
    let chinese = raw_string_tag("chinese", &[0xe4, 0xbd, 0xa0, 0xe5, 0xa5, 0xbd]);
    let modified_emoji = raw_string_tag("emoji", &[0xed, 0xa0, 0xbd, 0xed, 0xb8, 0x80]);
    let standard_emoji = raw_string_tag("standard", &[0xf0, 0x9f, 0x98, 0x80]);

    let mut children = Vec::new();
    children.extend(nul);
    children.extend(chinese);
    children.extend(modified_emoji);
    children.extend(standard_emoji);

    let decoded = decode(&root(&children), &DecodeLimits::default()).unwrap();
    let root_tag = compound(&decoded.value);

    assert_eq!(
        root_tag.get("nul"),
        Some(&NbtValue::String("\u{0}a".into()))
    );
    assert_eq!(
        root_tag.get("chinese"),
        Some(&NbtValue::String("你好".into()))
    );
    assert_eq!(root_tag.get("emoji"), Some(&NbtValue::String("😀".into())));
    assert_eq!(
        root_tag.get("standard"),
        Some(&NbtValue::String("😀".into()))
    );
}

#[test]
fn rejects_truncated_input() {
    let data = root(&int_tag("value", 1));
    let truncated = &data[..data.len() - 1];

    let err = decode(truncated, &DecodeLimits::default()).unwrap_err();
    assert_eq!(err.code, NbtErrorCode::TruncatedInput);
}

#[test]
fn rejects_invalid_tag_and_end_root() {
    let err = decode(&[13, 0, 0], &DecodeLimits::default()).unwrap_err();
    assert_eq!(err.code, NbtErrorCode::InvalidTag);

    let err = decode(&[0], &DecodeLimits::default()).unwrap_err();
    assert_eq!(err.code, NbtErrorCode::InvalidTag);
}

#[test]
fn rejects_non_empty_list_with_end_element_type() {
    let mut payload = vec![0];
    payload.extend_from_slice(&1i32.to_be_bytes());
    let data = root(&named(9, "bad", &payload));

    let err = decode(&data, &DecodeLimits::default()).unwrap_err();
    assert_eq!(err.code, NbtErrorCode::InvalidTag);
}

#[test]
fn rejects_trailing_bytes() {
    let mut data = root(&int_tag("value", 1));
    data.push(0);

    let err = decode(&data, &DecodeLimits::default()).unwrap_err();
    assert_eq!(err.code, NbtErrorCode::TrailingBytes);
}

#[test]
fn rejects_invalid_compression() {
    let err = decode(&[0x1f, 0x8b, 0x00, 0x00], &DecodeLimits::default()).unwrap_err();
    assert_eq!(err.code, NbtErrorCode::InvalidCompression);
}

#[test]
fn rejects_invalid_utf8() {
    let data = root(&raw_string_tag("bad", &[0xc3, 0x28]));

    let err = decode(&data, &DecodeLimits::default()).unwrap_err();
    assert_eq!(err.code, NbtErrorCode::InvalidUtf8);
}

#[test]
fn rejects_deep_nesting() {
    let mut nested = Vec::new();
    for index in 0..12 {
        nested = compound_tag(&format!("level{index}"), &nested);
    }

    let err = decode(&root(&nested), &DecodeLimits::small()).unwrap_err();
    assert_eq!(err.code, NbtErrorCode::LimitExceeded);
}

#[test]
fn rejects_oversized_values_and_budgets() {
    let mut limits = DecodeLimits::small();

    let err = decode(&root(&string_tag("long", &"x".repeat(65))), &limits).unwrap_err();
    assert_eq!(err.code, NbtErrorCode::LimitExceeded);

    let long_list = vec![1i32.to_be_bytes().to_vec(); 33];
    let err = decode(&root(&list_tag("long_list", 3, &long_list)), &limits).unwrap_err();
    assert_eq!(err.code, NbtErrorCode::LimitExceeded);

    let err = decode(&root(&byte_array_tag("long_bytes", &[1; 65])), &limits).unwrap_err();
    assert_eq!(err.code, NbtErrorCode::LimitExceeded);

    limits.max_array_len = 1_000_000;
    limits.max_total_bytes = 100;
    let err = decode(&root(&byte_array_tag("budget", &[1; 200])), &limits).unwrap_err();
    assert_eq!(err.code, NbtErrorCode::LimitExceeded);

    let err = decode(&[0u8; 5000], &DecodeLimits::small()).unwrap_err();
    assert_eq!(err.code, NbtErrorCode::LimitExceeded);
}

#[test]
fn rejects_negative_lengths() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&(-1i32).to_be_bytes());
    let data = root(&named(7, "bad", &payload));

    let err = decode(&data, &DecodeLimits::default()).unwrap_err();
    assert_eq!(err.code, NbtErrorCode::InvalidTag);
}

#[test]
fn truncation_at_every_byte_never_panics() {
    let mut children = Vec::new();
    children.extend(int_tag("int", 7));
    children.extend(string_tag("string", "hello"));
    children.extend(byte_array_tag("bytes", &[1, 2, 3]));
    children.extend(list_tag(
        "ints",
        3,
        &[1i32.to_be_bytes().to_vec(), 2i32.to_be_bytes().to_vec()],
    ));
    let data = root(&children);

    for end in 0..data.len() {
        let _ = decode(&data[..end], &DecodeLimits::default());
    }
}

#[test]
fn rejects_decompressed_size_limit() {
    let compressed = gzip(&[0u8; 9000]);

    let err = decode(&compressed, &DecodeLimits::small()).unwrap_err();
    assert_eq!(err.code, NbtErrorCode::LimitExceeded);
}

#[test]
fn rejects_tag_count_limit() {
    let mut children = Vec::new();
    for index in 0..200 {
        children.extend(int_tag(&format!("k{index}"), index));
    }

    let err = decode(&root(&children), &DecodeLimits::small()).unwrap_err();
    assert_eq!(err.code, NbtErrorCode::LimitExceeded);
}
