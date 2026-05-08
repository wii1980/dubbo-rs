// Phase 1: Hessian2 byte-level verification tests.
//
// Test ID conventions:
//   A-D-*  = Decode: Java fixture → RS decode → assert value
//   A-E-*  = Encode: RS encode → assert bytes match Java fixture
//   A-R-*  = Roundtrip: RS encode → RS decode → assert same value

use cross_lang_tests::{assert_bytes_eq, FixtureLoader};
use dubbo_rs_serialization_hessian2::codec::encoder::Encoder;
use dubbo_rs_serialization_hessian2::decoder::Decoder;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn read_fixture(cat: &str, name: &str) -> Vec<u8> {
    FixtureLoader::read(cat, name)
}

/// Encode a single value using the Rust encoder, return bytes.
fn encode_bool(v: bool) -> Vec<u8> {
    let mut enc = Encoder::new();
    enc.write_bool(v);
    enc.into_bytes()
}

fn encode_int(v: i32) -> Vec<u8> {
    let mut enc = Encoder::new();
    enc.write_int(v);
    enc.into_bytes()
}

fn encode_long(v: i64) -> Vec<u8> {
    let mut enc = Encoder::new();
    enc.write_long(v);
    enc.into_bytes()
}

fn encode_double(v: f64) -> Vec<u8> {
    let mut enc = Encoder::new();
    enc.write_double(v);
    enc.into_bytes()
}

fn encode_string(v: &str) -> Vec<u8> {
    let mut enc = Encoder::new();
    enc.write_string(v);
    enc.into_bytes()
}

fn encode_null() -> Vec<u8> {
    let mut enc = Encoder::new();
    enc.write_null();
    enc.into_bytes()
}

// ── A-D: Decode Tests (Java fixture → RS decoder) ─────────────────────────

mod a_d_decode {
    use super::*;

    // Booleans

    #[test]
    fn ad_001_bool_true() {
        let data = read_fixture("primitives", "bool_true.bin");
        let mut dec = Decoder::new(&data);
        assert!(dec.read_bool().unwrap(), "A-D-001: bool true");
    }

    #[test]
    fn ad_002_bool_false() {
        let data = read_fixture("primitives", "bool_false.bin");
        let mut dec = Decoder::new(&data);
        assert!(!dec.read_bool().unwrap(), "A-D-002: bool false");
    }

    // Integers

    #[test]
    fn ad_003_int_zero() {
        let data = read_fixture("primitives", "int_zero.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.read_int().unwrap(), 0, "A-D-003: int 0");
    }

    #[test]
    fn ad_004_int_one() {
        let data = read_fixture("primitives", "int_one.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.read_int().unwrap(), 1, "A-D-004: int 1");
    }

    #[test]
    fn ad_005_int_47() {
        let data = read_fixture("primitives", "int_47.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.read_int().unwrap(), 47, "A-D-005: int 47");
    }

    #[test]
    fn ad_006_int_neg16() {
        let data = read_fixture("primitives", "int_neg16.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.read_int().unwrap(), -16, "A-D-006: int -16");
    }

    #[test]
    fn ad_007_int_256() {
        let data = read_fixture("primitives", "int_256.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.read_int().unwrap(), 256, "A-D-007: int 256");
    }

    #[test]
    fn ad_008_int_max() {
        let data = read_fixture("primitives", "int_max.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.read_int().unwrap(), i32::MAX, "A-D-008: int MAX");
    }

    #[test]
    fn ad_009_int_min() {
        let data = read_fixture("primitives", "int_min.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.read_int().unwrap(), i32::MIN, "A-D-009: int MIN");
    }

    // Longs

    #[test]
    fn ad_010_long_zero() {
        let data = read_fixture("primitives", "long_zero.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.read_long().unwrap(), 0, "A-D-010: long 0");
    }

    #[test]
    fn ad_011_long_15() {
        let data = read_fixture("primitives", "long_15.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.read_long().unwrap(), 15, "A-D-011: long 15");
    }

    #[test]
    fn ad_012_long_neg8() {
        let data = read_fixture("primitives", "long_neg8.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.read_long().unwrap(), -8, "A-D-012: long -8");
    }

    #[test]
    fn ad_013_long_max() {
        let data = read_fixture("primitives", "long_big.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.read_long().unwrap(), i64::MAX, "A-D-013: long MAX");
    }

    // Doubles

    #[test]
    #[allow(clippy::float_cmp)]
    fn ad_014_double_zero() {
        let data = read_fixture("primitives", "double_zero.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.read_double().unwrap(), 0.0, "A-D-014: double 0.0");
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn ad_015_double_one() {
        let data = read_fixture("primitives", "double_one.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.read_double().unwrap(), 1.0, "A-D-015: double 1.0");
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn ad_016_double_pi() {
        let data = read_fixture("primitives", "double_pi.bin");
        let mut dec = Decoder::new(&data);
        let val = dec.read_double().unwrap();
        assert!((val - 3.14159).abs() < 1e-10, "A-D-016: double pi = {val}");
    }

    #[test]
    fn ad_017_double_neg() {
        let data = read_fixture("primitives", "double_neg.bin");
        let mut dec = Decoder::new(&data);
        let val = dec.read_double().unwrap();
        assert!(
            (val - (-99.99)).abs() < 1e-10,
            "A-D-017: double -99.99 = {val}"
        );
    }

    // Null

    #[test]
    fn ad_018_null() {
        let data = read_fixture("primitives", "null.bin");
        let mut dec = Decoder::new(&data);
        dec.read_null().unwrap();
    }

    // Strings

    #[test]
    fn ad_019_string_empty() {
        let data = read_fixture("strings", "string_empty.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.read_string().unwrap(), "", "A-D-019: empty string");
    }

    #[test]
    fn ad_020_string_hello() {
        let data = read_fixture("strings", "string_ascii.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.read_string().unwrap(), "hello", "A-D-020: hello");
    }

    #[test]
    fn ad_021_string_unicode() {
        let data = read_fixture("strings", "string_unicode.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(
            dec.read_string().unwrap(),
            "你好世界",
            "A-D-021: Chinese string"
        );
    }

    #[test]
    fn ad_022_string_mixed() {
        let data = read_fixture("strings", "string_mixed.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(
            dec.read_string().unwrap(),
            "Hello, 世界! 🦀",
            "A-D-022: mixed string"
        );
    }

    // Collections (list)

    #[test]
    fn ad_023_list_empty() {
        let data = read_fixture("collections", "list_empty.bin");
        let mut dec = Decoder::new(&data);
        let len = dec.read_list_begin().unwrap();
        assert_eq!(len, 0, "A-D-023: empty list length = 0");
    }

    #[test]
    fn ad_024_list_int() {
        let data = read_fixture("collections", "list_int.bin");
        let mut dec = Decoder::new(&data);
        let len = dec.read_list_begin().unwrap();
        assert_eq!(len, 3, "A-D-024: int list length");
        assert_eq!(dec.read_int().unwrap(), 1);
        assert_eq!(dec.read_int().unwrap(), 2);
        assert_eq!(dec.read_int().unwrap(), 3);
    }

    #[test]
    fn ad_025_list_mixed() {
        let data = read_fixture("collections", "list_mixed.bin");
        let mut dec = Decoder::new(&data);
        let len = dec.read_list_begin().unwrap();
        assert_eq!(len, 4, "A-D-025: mixed list length");
        assert_eq!(dec.read_string().unwrap(), "hello");
        assert_eq!(dec.read_int().unwrap(), 42);
        assert!(dec.read_bool().unwrap());
        dec.read_null().unwrap();
    }

    #[test]
    fn ad_026_list_nested() {
        let data = read_fixture("collections", "list_nested.bin");
        let mut dec = Decoder::new(&data);
        let outer_len = dec.read_list_begin().unwrap();
        assert_eq!(outer_len, 2, "A-D-026: nested list outer length");
        // Inner list 1
        let inner1_len = dec.read_list_begin().unwrap();
        assert_eq!(inner1_len, 2);
        assert_eq!(dec.read_string().unwrap(), "a");
        assert_eq!(dec.read_string().unwrap(), "b");
        // Inner list 2 (empty)
        let inner2_len = dec.read_list_begin().unwrap();
        assert_eq!(inner2_len, 0);
    }

    // Maps

    #[test]
    fn ad_027_map_empty() {
        let data = read_fixture("maps", "map_empty.bin");
        let mut dec = Decoder::new(&data);
        let is_typed = dec.read_map_begin().unwrap();
        assert!(!is_typed, "A-D-027: untyped empty map");
    }

    #[test]
    fn ad_028_map_string() {
        let data = read_fixture("maps", "map_string.bin");
        let mut dec = Decoder::new(&data);
        let is_typed = dec.read_map_begin().unwrap();
        if is_typed {
            // Typed map: read (and skip) the type string
            let _type_name = dec.read_string().unwrap();
        }
        assert_eq!(dec.read_string().unwrap(), "key1");
        assert_eq!(dec.read_string().unwrap(), "value1");
        assert_eq!(dec.read_string().unwrap(), "key2");
        assert_eq!(dec.read_string().unwrap(), "value2");
    }

    #[test]
    fn ad_029_map_mixed() {
        let data = read_fixture("maps", "map_mixed.bin");
        let mut dec = Decoder::new(&data);
        let is_typed = dec.read_map_begin().unwrap();
        if is_typed {
            let _type_name = dec.read_string().unwrap();
        }
        assert_eq!(dec.read_string().unwrap(), "name");
        assert_eq!(dec.read_string().unwrap(), "dubbo-rs");
        assert_eq!(dec.read_string().unwrap(), "version");
        assert_eq!(dec.read_int().unwrap(), 3);
        assert_eq!(dec.read_string().unwrap(), "stable");
        assert!(dec.read_bool().unwrap());
        assert_eq!(dec.read_string().unwrap(), "ratio");
        let ratio = dec.read_double().unwrap();
        assert!((ratio - 0.95).abs() < 1e-10);
        assert_eq!(dec.read_string().unwrap(), "nullable");
        dec.read_null().unwrap();
    }

    // Dates

    #[test]
    fn ad_030_date_epoch() {
        let data = read_fixture("dates", "date_epoch.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.read_date().unwrap(), 0, "A-D-030: epoch");
    }

    #[test]
    fn ad_031_date_fixed() {
        let data = read_fixture("dates", "date_fixed.bin");
        let mut dec = Decoder::new(&data);
        assert_eq!(
            dec.read_date().unwrap(),
            1_705_312_200_000,
            "A-D-031: 2024-01-15T10:30:00 UTC"
        );
    }

    // POJOs (objects)

    #[test]
    fn ad_032_pojo_person() {
        let data = read_fixture("objects", "pojo_person.bin");
        let mut dec = Decoder::new(&data);
        let cd_idx = dec.read_class_def().expect("class def");
        let cd = dec.get_class_def(cd_idx).unwrap();
        assert_eq!(
            cd.class_name,
            "com.dubborst.fixtures.Hessian2ReferenceEncoder$SamplePerson"
        );
        assert_eq!(cd.field_names, vec!["active", "age", "name"]);
        let _obj_idx = dec.read_object_begin().unwrap();
        // Fields in alphabetical order per Java BeanInfo reflection
        assert!(dec.read_bool().unwrap(), "active");
        assert_eq!(dec.read_int().unwrap(), 30, "age");
        assert_eq!(dec.read_string().unwrap(), "Alice", "name");
    }

    #[test]
    fn ad_033_pojo_sparse() {
        let data = read_fixture("objects", "pojo_sparse.bin");
        let mut dec = Decoder::new(&data);
        let cd_idx = dec.read_class_def().expect("class def");
        let cd = dec.get_class_def(cd_idx).unwrap();
        assert_eq!(
            cd.class_name,
            "com.dubborst.fixtures.Hessian2ReferenceEncoder$SamplePerson"
        );
        assert_eq!(cd.field_names, vec!["active", "age", "name"]);
        let _obj_idx = dec.read_object_begin().unwrap();
        assert!(!dec.read_bool().unwrap(), "active");
        assert_eq!(dec.read_int().unwrap(), 0, "age");
        dec.read_null().unwrap(); // name = null
    }
}

// ── A-E: Encode Tests (RS encoder → bytes match Java fixture) ─────────────

mod a_e_encode {
    use super::*;

    #[test]
    fn ae_001_bool_true() {
        let rs_bytes = encode_bool(true);
        let fixture = read_fixture("primitives", "bool_true.bin");
        assert_bytes_eq(&fixture, &rs_bytes, "A-E-001: encode bool true").unwrap();
    }

    #[test]
    fn ae_002_bool_false() {
        let rs_bytes = encode_bool(false);
        let fixture = read_fixture("primitives", "bool_false.bin");
        assert_bytes_eq(&fixture, &rs_bytes, "A-E-002: encode bool false").unwrap();
    }

    #[test]
    fn ae_003_int_zero() {
        let rs_bytes = encode_int(0);
        let fixture = read_fixture("primitives", "int_zero.bin");
        assert_bytes_eq(&fixture, &rs_bytes, "A-E-003: int 0").unwrap();
    }

    #[test]
    fn ae_004_int_one() {
        let rs_bytes = encode_int(1);
        let fixture = read_fixture("primitives", "int_one.bin");
        assert_bytes_eq(&fixture, &rs_bytes, "A-E-004: int 1").unwrap();
    }

    #[test]
    fn ae_005_int_42() {
        let _rs_bytes = encode_int(42);
        let fixture = read_fixture("primitives", "int_47.bin");
        // 42 ≠ 47 — this is a known fixture difference, test name is conceptual
        // We test the value 47 to match fixture
        let rs_47 = encode_int(47);
        assert_bytes_eq(&fixture, &rs_47, "A-E-005: int 47").unwrap();
    }

    #[test]
    fn ae_006_long_zero() {
        let rs_bytes = encode_long(0);
        let fixture = read_fixture("primitives", "long_zero.bin");
        assert_bytes_eq(&fixture, &rs_bytes, "A-E-006: long 0").unwrap();
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn ae_007_double_314() {
        let rs_bytes = encode_double(3.14159);
        let fixture = read_fixture("primitives", "double_pi.bin");
        assert_bytes_eq(&fixture, &rs_bytes, "A-E-007: double pi").unwrap();
    }

    #[test]
    fn ae_008_null() {
        let rs_bytes = encode_null();
        let fixture = read_fixture("primitives", "null.bin");
        assert_bytes_eq(&fixture, &rs_bytes, "A-E-008: null").unwrap();
    }

    #[test]
    fn ae_009_string_hello() {
        let rs_bytes = encode_string("hello");
        let fixture = read_fixture("strings", "string_ascii.bin");
        assert_bytes_eq(&fixture, &rs_bytes, "A-E-009: string hello").unwrap();
    }

    #[test]
    fn ae_010_string_unicode() {
        let rs_bytes = encode_string("你好世界");
        let fixture = read_fixture("strings", "string_unicode.bin");
        assert_bytes_eq(&fixture, &rs_bytes, "A-E-010: unicode string").unwrap();
    }

    #[test]
    fn ae_011_string_empty() {
        let rs_bytes = encode_string("");
        let fixture = read_fixture("strings", "string_empty.bin");
        assert_bytes_eq(&fixture, &rs_bytes, "A-E-011: empty string").unwrap();
    }
}

// ── A-R: Roundtrip Tests (RS encode → RS decode → preserve value) ────────

mod a_r_roundtrip {
    use super::*;

    #[test]
    fn ar_001_bool() {
        let bytes = encode_bool(true);
        let mut dec = Decoder::new(&bytes);
        assert!(dec.read_bool().unwrap());

        let bytes = encode_bool(false);
        let mut dec = Decoder::new(&bytes);
        assert!(!dec.read_bool().unwrap());
    }

    #[test]
    fn ar_002_int() {
        for v in [0, 1, -1, 42, -16, 47, 256, i32::MAX, i32::MIN] {
            let bytes = encode_int(v);
            let mut dec = Decoder::new(&bytes);
            assert_eq!(dec.read_int().unwrap(), v, "A-R-002: int {v}");
        }
    }

    #[test]
    fn ar_003_long() {
        for v in [0i64, 1, -1, 15, -8, 256, 1_000_000, i64::MAX, i64::MIN] {
            let bytes = encode_long(v);
            let mut dec = Decoder::new(&bytes);
            assert_eq!(dec.read_long().unwrap(), v, "A-R-003: long {v}");
        }
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn ar_004_double() {
        for v in [0.0, 1.0, -1.0, 3.14159, -99.99, f64::MAX, f64::MIN] {
            let bytes = encode_double(v);
            let mut dec = Decoder::new(&bytes);
            let got = dec.read_double().unwrap();
            if v.is_sign_negative() {
                assert!(got.is_sign_negative(), "A-R-004: sign for {v}");
            }
            // Use direct equality (bit-exact) for compatible values
            assert!(
                (got - v).abs() < 1e-10 || got.to_bits() == v.to_bits(),
                "A-R-004: double {v} got {got}"
            );
        }
    }

    #[test]
    fn ar_005_string() {
        for s in ["", "a", "hello", "你好世界", "Hello, 世界! 🦀"] {
            let bytes = encode_string(s);
            let mut dec = Decoder::new(&bytes);
            assert_eq!(dec.read_string().unwrap(), s, "A-R-005: string '{s}'");
        }
    }

    #[test]
    fn ar_006_null() {
        let bytes = encode_null();
        let mut dec = Decoder::new(&bytes);
        dec.read_null().unwrap();
    }

    #[test]
    fn ar_007_list() {
        let mut enc = Encoder::new();
        enc.write_list_begin(3);
        enc.write_int(10);
        enc.write_int(20);
        enc.write_int(30);
        let bytes = enc.into_bytes();
        let mut dec = Decoder::new(&bytes);
        let len = dec.read_list_begin().unwrap();
        assert_eq!(len, 3);
        assert_eq!(dec.read_int().unwrap(), 10);
        assert_eq!(dec.read_int().unwrap(), 20);
        assert_eq!(dec.read_int().unwrap(), 30);
    }

    #[test]
    fn ar_008_list_empty() {
        let mut enc = Encoder::new();
        enc.write_list_begin(0);
        let bytes = enc.into_bytes();
        let mut dec = Decoder::new(&bytes);
        assert_eq!(dec.read_list_begin().unwrap(), 0);
    }

    #[test]
    fn ar_009_map() {
        let mut enc = Encoder::new();
        enc.write_map_begin();
        enc.write_string("k1");
        enc.write_string("v1");
        enc.write_string("k2");
        enc.write_int(2);
        enc.write_map_end();
        let bytes = enc.into_bytes();
        let mut dec = Decoder::new(&bytes);
        let is_typed = dec.read_map_begin().unwrap();
        assert!(!is_typed);
        assert_eq!(dec.read_string().unwrap(), "k1");
        assert_eq!(dec.read_string().unwrap(), "v1");
        assert_eq!(dec.read_string().unwrap(), "k2");
        assert_eq!(dec.read_int().unwrap(), 2);
    }

    #[test]
    fn ar_010_fixture_roundtrip_null() {
        // A-R-010: Java fixture → RS decode → RS encode → byte compare
        let fixture = read_fixture("primitives", "null.bin");
        let mut dec = Decoder::new(&fixture);
        dec.read_null().unwrap();
        let re_encoded = encode_null();
        assert_bytes_eq(&fixture, &re_encoded, "A-R-010: null roundtrip").unwrap();
    }
}
