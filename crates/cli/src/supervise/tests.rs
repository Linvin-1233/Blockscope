use super::*;

fn capacity_with_memory(available: u64) -> Capacity {
    Capacity {
        platform: "test".to_string(),
        cores: 4,
        cpu_quota_milli: None,
        memory_limit_bytes: available,
        memory_available_bytes: available,
        suggested_workers: 2,
        suggested_threads: 2,
        suggested_max_quads: 20_000_000,
        default_width: 1024,
        default_height: 1024,
        default_supersample: 2,
    }
}

#[test]
fn memory_limit_keeps_headroom() {
    let capacity = capacity_with_memory(8 * 1024 * MIB);
    let limit = memory_limit(&capacity, 2, 0).unwrap();
    // 4 GiB per worker minus 15 percent headroom.
    assert!(limit > 3 * 1024 * MIB && limit < 4 * 1024 * MIB, "{limit}");
}

#[test]
fn memory_limit_override_wins() {
    let capacity = capacity_with_memory(8 * 1024 * MIB);
    assert_eq!(memory_limit(&capacity, 2, 512), Some(512 * MIB));
}

#[test]
fn unknown_memory_disables_the_limit() {
    let capacity = capacity_with_memory(0);
    assert_eq!(memory_limit(&capacity, 2, 0), None);
}

#[test]
fn route_key_hashes_regular_files_and_preserves_lod() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.litematic");
    std::fs::write(&path, b"data").unwrap();
    let line = json!({
        "type": "render",
        "input": { "path": path },
        "lod": 2,
    })
    .to_string();

    let key = route_key(&line);
    assert!(key.ends_with(":2"), "{key}");
    assert_eq!(key.len(), 66, "{key}");
}

#[test]
fn sticky_routing_hints_stay_bounded() {
    let mut sticky = HashMap::new();
    for index in 0..=MAX_STICKY_ENTRIES {
        remember_sticky(&mut sticky, &format!("hash-{index}"), index);
    }

    assert!(sticky.len() <= MAX_STICKY_ENTRIES);
    remember_sticky(&mut sticky, "hash-0", 0);
    assert_eq!(sticky.get("hash-0"), Some(&0));
}

#[test]
fn injects_hash_when_the_request_omits_it() {
    let line = json!({
        "type": "render",
        "input": { "path": "/tmp/model.litematic" },
    })
    .to_string();

    let injected = inject_input_hash(&line, "abc123");
    let value: Value = serde_json::from_str(&injected).unwrap();
    assert_eq!(
        value.pointer("/input/hash").and_then(Value::as_str),
        Some("abc123")
    );
}

#[test]
fn keeps_a_caller_supplied_hash() {
    let line = json!({
        "type": "render",
        "input": { "path": "/tmp/model.litematic", "hash": "caller" },
    })
    .to_string();

    let injected = inject_input_hash(&line, "supervisor");
    let value: Value = serde_json::from_str(&injected).unwrap();
    assert_eq!(
        value.pointer("/input/hash").and_then(Value::as_str),
        Some("caller")
    );
}

#[test]
fn route_key_returns_empty_hash_for_unreadable_paths() {
    let line = json!({
        "type": "render",
        "input": { "path": "definitely-not-a-file" },
    })
    .to_string();

    assert_eq!(route_key(&line), ":1");
}
