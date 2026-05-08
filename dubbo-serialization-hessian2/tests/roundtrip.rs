#![allow(clippy::similar_names, clippy::float_cmp)]

use dubbo_rs_serialization_hessian2::codec::encoder::Encoder;
use dubbo_rs_serialization_hessian2::decoder::Decoder;

// ── Helpers ──

fn encode_decode_int(value: i32) -> i32 {
    let mut enc = Encoder::new();
    enc.write_int(value);
    let bytes = enc.into_bytes();
    let mut dec = Decoder::new(&bytes);
    dec.read_int().expect("decode int")
}

fn encode_decode_long(value: i64) -> i64 {
    let mut enc = Encoder::new();
    enc.write_long(value);
    let bytes = enc.into_bytes();
    let mut dec = Decoder::new(&bytes);
    dec.read_long().expect("decode long")
}

fn encode_decode_double(value: f64) -> f64 {
    let mut enc = Encoder::new();
    enc.write_double(value);
    let bytes = enc.into_bytes();
    let mut dec = Decoder::new(&bytes);
    dec.read_double().expect("decode double")
}

fn encode_decode_string(value: &str) -> String {
    let mut enc = Encoder::new();
    enc.write_string(value);
    let bytes = enc.into_bytes();
    let mut dec = Decoder::new(&bytes);
    dec.read_string().expect("decode string")
}

fn encode_decode_bool(value: bool) -> bool {
    let mut enc = Encoder::new();
    enc.write_bool(value);
    let bytes = enc.into_bytes();
    let mut dec = Decoder::new(&bytes);
    dec.read_bool().expect("decode bool")
}

fn encode_decode_binary(value: &[u8]) -> Vec<u8> {
    let mut enc = Encoder::new();
    enc.write_binary(value);
    let bytes = enc.into_bytes();
    let mut dec = Decoder::new(&bytes);
    dec.read_binary().expect("decode binary")
}

// ── Int roundtrip ──

#[test]
fn int_compact_single_byte() {
    for v in [-16, -1, 0, 1, 47] {
        assert_eq!(encode_decode_int(v), v, "roundtrip for {v}");
    }
}

#[test]
fn int_compact_two_byte() {
    for v in [100, -100, 2047, -2048] {
        assert_eq!(encode_decode_int(v), v, "roundtrip for {v}");
    }
}

#[test]
fn int_compact_three_byte() {
    for v in [100_000, -100_000] {
        assert_eq!(encode_decode_int(v), v, "roundtrip for {v}");
    }
}

#[test]
fn int_full() {
    for v in [i32::MAX, i32::MIN] {
        assert_eq!(encode_decode_int(v), v, "roundtrip for {v}");
    }
}

// ── Long roundtrip ──

#[test]
fn long_compact_single_byte() {
    for v in [-8i64, -1, 0, 1, 15] {
        assert_eq!(encode_decode_long(v), v, "roundtrip for {v}");
    }
}

#[test]
fn long_compact_two_byte() {
    for v in [100i64, -100, 2047, -2048] {
        assert_eq!(encode_decode_long(v), v, "roundtrip for {v}");
    }
}

#[test]
fn long_compact_three_byte() {
    for v in [100_000i64, -100_000] {
        assert_eq!(encode_decode_long(v), v, "roundtrip for {v}");
    }
}

#[test]
fn long_compact_32bit() {
    assert_eq!(encode_decode_long(1_000_000), 1_000_000);
    assert_eq!(encode_decode_long(-1_000_000), -1_000_000);
}

#[test]
fn long_full() {
    assert_eq!(encode_decode_long(i64::MAX), i64::MAX);
    assert_eq!(encode_decode_long(i64::MIN), i64::MIN);
}

// ── Double roundtrip ──

#[test]
fn double_roundtrip() {
    #[allow(clippy::approx_constant)]
    for v in [0.0, 1.0, -1.0, 42.0, 3.14] {
        let got = encode_decode_double(v);
        assert!((got - v).abs() < 1e-15, "roundtrip for {v}: got {got}");
    }
}

#[test]
fn double_extremes() {
    assert_eq!(encode_decode_double(f64::MAX), f64::MAX);
    assert_eq!(encode_decode_double(f64::MIN), f64::MIN);
}

// ── String roundtrip ──

#[test]
fn string_roundtrip() {
    for s in ["", "a", "hello"] {
        assert_eq!(encode_decode_string(s), s);
    }
}

#[test]
fn string_long() {
    let s = "a".repeat(100);
    assert_eq!(encode_decode_string(&s), s);
}

// ── Bool roundtrip ──

#[test]
fn bool_roundtrip() {
    assert!(encode_decode_bool(true));
    assert!(!encode_decode_bool(false));
}

// ── Binary roundtrip ──

#[test]
fn binary_empty() {
    assert_eq!(encode_decode_binary(&[]), Vec::<u8>::new());
}

#[test]
fn binary_small() {
    let data: Vec<u8> = (0..16).collect();
    assert_eq!(encode_decode_binary(&data), data);
}

#[test]
fn binary_large() {
    let data: Vec<u8> = (0..=255).collect();
    assert_eq!(encode_decode_binary(&data), data);
}

// ── Null roundtrip ──

#[test]
fn null_roundtrip() {
    let mut enc = Encoder::new();
    enc.write_null();
    let bytes = enc.into_bytes();
    let mut dec = Decoder::new(&bytes);
    dec.read_null().expect("decode null");
}

// ── Date roundtrip ──

#[test]
fn date_roundtrip() {
    let millis: i64 = 1_700_000_000_000; // ~ Nov 2023
    let mut enc = Encoder::new();
    enc.write_date(millis);
    let bytes = enc.into_bytes();
    let mut dec = Decoder::new(&bytes);
    let got = dec.read_date().expect("decode date");
    assert_eq!(got, millis);
}

// ── List roundtrip ──

#[test]
fn list_small_roundtrip() {
    let mut enc = Encoder::new();
    enc.write_list_begin(3);
    enc.write_int(1);
    enc.write_int(2);
    enc.write_int(3);
    let bytes = enc.into_bytes();

    let mut dec = Decoder::new(&bytes);
    let len = dec.read_list_begin().expect("list header");
    assert_eq!(len, 3);
    assert_eq!(dec.read_int().expect("read 1"), 1);
    assert_eq!(dec.read_int().expect("read 2"), 2);
    assert_eq!(dec.read_int().expect("read 3"), 3);
}

#[test]
fn list_large_count_roundtrip() {
    const COUNT: usize = 200;
    let mut enc = Encoder::new();
    enc.write_list_begin(COUNT);
    for _ in 0..COUNT {
        enc.write_int(42);
    }
    let bytes = enc.into_bytes();

    let mut dec = Decoder::new(&bytes);
    let len = dec.read_list_begin().expect("list header");
    assert_eq!(len, COUNT);
    for _ in 0..COUNT {
        assert_eq!(dec.read_int().expect("read int"), 42);
    }
}

#[test]
fn list_empty_roundtrip() {
    let mut enc = Encoder::new();
    enc.write_list_begin(0);
    let bytes = enc.into_bytes();

    let mut dec = Decoder::new(&bytes);
    let len = dec.read_list_begin().expect("list header");
    assert_eq!(len, 0);
}

// ── Map roundtrip ──

#[test]
fn map_untyped_roundtrip() {
    let mut enc = Encoder::new();
    enc.write_map_begin();
    enc.write_string("key1");
    enc.write_int(100);
    enc.write_string("key2");
    enc.write_string("value2");
    enc.write_map_end();
    let bytes = enc.into_bytes();

    let mut dec = Decoder::new(&bytes);
    let is_typed = dec.read_map_begin().expect("map header");
    assert!(!is_typed);
    assert_eq!(dec.read_string().expect("key1"), "key1");
    assert_eq!(dec.read_int().expect("val1"), 100);
    assert_eq!(dec.read_string().expect("key2"), "key2");
    assert_eq!(dec.read_string().expect("val2"), "value2");
}

#[test]
fn map_empty_roundtrip() {
    let mut enc = Encoder::new();
    enc.write_map_begin();
    enc.write_map_end();
    let bytes = enc.into_bytes();

    let mut dec = Decoder::new(&bytes);
    let is_typed = dec.read_map_begin().expect("map header");
    assert!(!is_typed);
    // Empty map — no entries to read
}

// ── Reference roundtrip ──

#[test]
fn ref_string_deduplication_roundtrip() {
    let mut enc = Encoder::new();
    enc.write_string_ref("hello");
    enc.write_string_ref("hello");
    let bytes = enc.into_bytes();
    // The second "hello" should be encoded as 'R' reference, not as full string
    assert!(
        bytes.len() < 12,
        "second string should be a ref, got {} bytes",
        bytes.len()
    );

    let mut dec = Decoder::new(&bytes);
    let s1 = dec.read_string().expect("string 1");
    assert_eq!(s1, "hello");

    let s2 = dec.read_ref().expect("ref");
    assert_eq!(s2, "hello");
}

#[test]
fn ref_mixed_strings_roundtrip() {
    let mut enc = Encoder::new();
    enc.write_string_ref("alpha");
    enc.write_string_ref("beta");
    enc.write_string_ref("alpha"); // deduplicated
    enc.write_string_ref("gamma");
    enc.write_string_ref("beta"); // deduplicated
    let bytes = enc.into_bytes();

    let mut dec = Decoder::new(&bytes);
    assert_eq!(dec.read_string().expect("alpha"), "alpha");
    assert_eq!(dec.read_string().expect("beta"), "beta");
    assert_eq!(dec.read_ref().expect("ref alpha"), "alpha");
    assert_eq!(dec.read_string().expect("gamma"), "gamma");
    assert_eq!(dec.read_ref().expect("ref beta"), "beta");
}

#[test]
fn ref_not_found_error() {
    let mut enc = Encoder::new();
    enc.write_string("hello");
    enc.write_ref(99);
    let bytes = enc.into_bytes();

    let mut dec = Decoder::new(&bytes);
    dec.read_string().expect("string");
    let err = dec.read_ref().unwrap_err();
    let err_msg = format!("{err}");
    assert!(
        err_msg.contains("99"),
        "error should mention index 99: {err_msg}"
    );
}

// ── Object / ClassDef ──

#[test]
fn class_def_roundtrip() {
    let class_name = "com.example.User";
    let fields = vec!["name".to_string(), "age".to_string()];

    let mut enc = Encoder::new();
    enc.write_class_def(class_name, &fields);
    let bytes = enc.into_bytes();

    let mut dec = Decoder::new(&bytes);
    let idx = dec.read_class_def().expect("class def");
    let def = dec.get_class_def(idx).expect("get class def");

    assert_eq!(def.class_name, class_name);
    assert_eq!(def.field_names, fields);
}

#[test]
fn class_def_duplicate_returns_same_index() {
    let class_name = "com.example.User";
    let fields = vec!["name".to_string()];

    let mut enc = Encoder::new();
    let idx1 = enc.write_class_def(class_name, &fields);
    let idx2 = enc.write_class_def(class_name, &fields);
    assert_eq!(idx1, idx2);
}

#[test]
fn object_instance_roundtrip() {
    let class_name = "com.example.Greeting";
    let fields = vec!["message".to_string(), "priority".to_string()];

    let mut enc = Encoder::new();
    let class_idx = enc.write_class_def(class_name, &fields);
    enc.write_object_begin(class_idx);
    enc.write_string("Hello, world!");
    enc.write_int(42);
    let bytes = enc.into_bytes();

    let mut dec = Decoder::new(&bytes);
    let cd_idx = dec.read_class_def().expect("class def");
    assert_eq!(cd_idx, 0);
    let obj_idx = dec.read_object_begin().expect("object");
    assert_eq!(obj_idx, 0);
    let message = dec.read_string().expect("field message");
    let priority = dec.read_int().expect("field priority");

    assert_eq!(message, "Hello, world!");
    assert_eq!(priority, 42);
}

#[test]
fn multiple_class_defs_and_objects_roundtrip() {
    // Two different class definitions, used in sequence
    let class_a = "com.example.Point";
    let fields_a = vec!["x".to_string(), "y".to_string()];
    let class_b = "com.example.Rect";
    let fields_b = vec!["top_left".to_string(), "bottom_right".to_string()];

    let mut enc = Encoder::new();
    let idx_a = enc.write_class_def(class_a, &fields_a);
    let idx_b = enc.write_class_def(class_b, &fields_b);
    assert_ne!(idx_a, idx_b);

    // Object of class A
    enc.write_object_begin(idx_a);
    enc.write_int(10);
    enc.write_int(20);

    // Object of class B
    enc.write_object_begin(idx_b);
    enc.write_int(0);
    enc.write_int(100);

    let bytes = enc.into_bytes();
    let mut dec = Decoder::new(&bytes);

    // Class def A
    let cdi_a = dec.read_class_def().unwrap();
    assert_eq!(dec.get_class_def(cdi_a).unwrap().class_name, class_a);

    // Class def B
    let cdi_b = dec.read_class_def().unwrap();
    assert_eq!(dec.get_class_def(cdi_b).unwrap().class_name, class_b);

    // Object A
    let oi_a = dec.read_object_begin().unwrap();
    assert_eq!(oi_a, cdi_a);
    assert_eq!(dec.read_int().unwrap(), 10);
    assert_eq!(dec.read_int().unwrap(), 20);

    // Object B
    let oi_b = dec.read_object_begin().unwrap();
    assert_eq!(oi_b, cdi_b);
    assert_eq!(dec.read_int().unwrap(), 0);
    assert_eq!(dec.read_int().unwrap(), 100);
}

#[test]
fn object_read_missing_class_def_error() {
    // Object `O` referencing non-existent class def index
    let mut enc = Encoder::new();
    enc.write_object_begin(99);
    let bytes = enc.into_bytes();

    let mut dec = Decoder::new(&bytes);
    let err = dec.read_object_begin().unwrap_err();
    assert!(format!("{err}").contains("99"));
}

#[test]
fn class_def_read_wrong_tag_error() {
    // Try to read class def from an int tag
    let mut enc = Encoder::new();
    enc.write_int(42);
    let bytes = enc.into_bytes();

    let mut dec = Decoder::new(&bytes);
    assert!(dec.read_class_def().is_err());
}

#[test]
fn object_read_wrong_tag_error() {
    // Try to read object from a string tag
    let mut enc = Encoder::new();
    enc.write_string("not-an-object");
    let bytes = enc.into_bytes();

    let mut dec = Decoder::new(&bytes);
    assert!(dec.read_object_begin().is_err());
}

// ── List variant tests ───────────────────────────────────────────────────

/// 0x58: untyped fixed-length list (for >7 elements after encoder change)
#[test]
fn list_fixed_untyped_variant() {
    // Manually construct: 0x58 + compact_int(10) + 10x null
    let bytes = {
        let mut enc = Encoder::new();
        enc.write_list_begin(10); // should write 0x58 + int(10)
        for _ in 0..10 {
            enc.write_null();
        }
        let b = enc.into_bytes();
        assert_eq!(b[0], 0x58, "should use 0x58 tag for large list");
        b
    };

    let mut dec = Decoder::new(&bytes);
    let count = dec.read_list_begin().expect("list begin");
    assert_eq!(count, 10, "should read 10 elements");
    for _ in 0..10 {
        dec.read_null().expect("null");
    }
}

/// 0x55: typed variable-length list (U + type + values + Z)
#[test]
fn list_typed_varlength_variant() {
    // Manual construct: 0x55 + "int" + compact_int(1) + compact_int(2) + Z
    let bytes = {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0x55]); // typed var-length
                                        // type string "int"
        buf.extend_from_slice(&[0x03]); // compact string length 3
        buf.extend_from_slice(b"int");
        // values
        buf.extend_from_slice(&[0x91]); // compact int 1
        buf.extend_from_slice(&[0x92]); // compact int 2
        buf.extend_from_slice(b"Z"); // end marker
        buf
    };

    let mut dec = Decoder::new(&bytes);
    let count = dec.read_list_begin().expect("list begin");
    assert_eq!(count, 0, "var-length returns 0");
    assert!(!dec.peek_is_list_end(), "should have elements");
    assert_eq!(dec.read_int().unwrap(), 1);
    assert_eq!(dec.read_int().unwrap(), 2);
    assert!(dec.peek_is_list_end(), "next should be Z");
    dec.read_list_end().expect("list end");
}

/// 0x57: untyped variable-length list (W + values + Z)
#[test]
fn list_untyped_varlength_variant() {
    let bytes = {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0x57]); // untyped var-length
        buf.extend_from_slice(b"T"); // true
        buf.extend_from_slice(b"F"); // false
        buf.extend_from_slice(b"Z"); // end
        buf
    };

    let mut dec = Decoder::new(&bytes);
    let count = dec.read_list_begin().expect("list begin");
    assert_eq!(count, 0, "var-length returns 0");
    assert!(dec.read_bool().unwrap());
    assert!(!dec.read_bool().unwrap());
    assert!(dec.peek_is_list_end());
    dec.read_list_end().expect("list end");
}

/// 0x70-0x77: typed compact list (tag = 0x70 + length, type + values)
#[test]
fn list_typed_compact_variant() {
    // 0x71 = typed compact, 1 element, type "str"
    let bytes = {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0x71]); // 0x70 + 1 = typed compact, count=1
        buf.extend_from_slice(&[0x03]); // compact string length 3
        buf.extend_from_slice(b"str");
        buf.extend_from_slice(&[0x05]); // compact string length 5
        buf.extend_from_slice(b"hello");
        buf
    };

    let mut dec = Decoder::new(&bytes);
    let count = dec.read_list_begin().expect("list begin");
    assert_eq!(count, 1, "typed compact count=1");
    assert_eq!(dec.read_string().unwrap(), "hello");
}

/// Verify encoder now uses compact encoding for small lists
#[test]
fn list_encoder_uses_compact_for_small() {
    let mut enc = Encoder::new();
    enc.write_list_begin(3);
    enc.write_int(1);
    enc.write_int(2);
    enc.write_int(3);
    let bytes = enc.into_bytes();

    // Tag should be 0x78 + 3 = 0x7b (compact untyped)
    assert_eq!(bytes[0], 0x7b, "small list should use compact tag");

    // Decode should still work
    let mut dec = Decoder::new(&bytes);
    assert_eq!(dec.read_list_begin().unwrap(), 3);
    assert_eq!(dec.read_int().unwrap(), 1);
    assert_eq!(dec.read_int().unwrap(), 2);
    assert_eq!(dec.read_int().unwrap(), 3);
}
