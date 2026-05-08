// Cross-language verification tests for dubbo-rs ↔ dubbo-java.

use cross_lang_tests::*;

// ── Domain A: Hessian2 Serialization ────────────────────────────────────────

mod hessian2 {
    use super::*;

    #[test]
    fn test_fixtures_exist() {
        let categories = [
            "primitives",
            "strings",
            "collections",
            "maps",
            "dates",
            "objects",
        ];
        let mut total = 0;
        for cat in &categories {
            let files = FixtureLoader::list(cat);
            eprintln!("  {cat}: {} files", files.len());
            total += files.len();
        }
        eprintln!("Total fixture files: {total}");
        assert!(
            total >= 25,
            "expected at least 25 fixture files, got {total}"
        );
    }

    #[test]
    fn test_a_d_001_decode_bool_true() {
        let data = FixtureLoader::read("primitives", "bool_true.bin");
        assert_eq!(data, b"\x54");
    }

    #[test]
    fn test_a_d_002_decode_bool_false() {
        let data = FixtureLoader::read("primitives", "bool_false.bin");
        assert_eq!(data, b"\x46");
    }

    #[test]
    fn test_a_d_003_decode_int_zero() {
        let data = FixtureLoader::read("primitives", "int_zero.bin");
        assert_eq!(data, b"\x90");
    }

    #[test]
    fn test_a_d_015_decode_null() {
        let data = FixtureLoader::read("primitives", "null.bin");
        assert_eq!(data, b"\x4e");
    }

    #[test]
    fn test_a_d_017_decode_string_hello() {
        let data = FixtureLoader::read("strings", "string_ascii.bin");
        let tag = data[0];
        assert!(
            tag == b'S' || tag == b's' || tag <= 0x1f,
            "expected Hessian2 string tag byte, got 0x{tag:02x}"
        );
        assert!(!data.is_empty());
    }

    #[test]
    fn test_a_d_024_decode_pojo() {
        let data = FixtureLoader::read("objects", "pojo_person.bin");
        assert!(!data.is_empty());
        let tag = data[0];
        // Hessian2 serialized object: 'C' (class def) + 'M'/'H' (map)
        assert!(
            tag == b'M' || tag == b'H' || tag == b'C',
            "expected Hessian2 class/map tag (C/M/H), got 0x{tag:02x}"
        );
    }
}

// ── Harness components test ─────────────────────────────────────────────────

mod harness_tests {
    use super::*;

    #[test]
    fn test_port_allocator_returns_available_port() {
        let port = PortAllocator::allocate();
        // OS assigns an ephemeral port from the dynamic range
        assert!(port > 0);
        assert!(PortAllocator::is_port_available(port));
    }

    #[test]
    fn test_port_allocator_sequential() {
        let p1 = PortAllocator::allocate();
        let p2 = PortAllocator::allocate();
        assert_ne!(p1, p2, "consecutive allocations must return different ports");
    }

    #[test]
    fn test_fixture_loader_path_resolution() {
        let path = FixtureLoader::path("primitives", "null.bin");
        let s = path.to_string_lossy();
        assert!(s.contains("fixtures/hessian2"), "path: {s}");
        assert!(s.ends_with("null.bin"), "path: {s}");
    }

    #[test]
    fn test_assert_bytes_eq_identical() {
        let a = b"\x90\x91\x92";
        assert!(assert_bytes_eq(a, a, "test").is_ok());
    }

    #[test]
    fn test_assert_bytes_eq_different() {
        let a = b"\x90\x91\x92";
        let b = b"\x90\x00\x92";
        let err = assert_bytes_eq(a, b, "test").unwrap_err();
        assert!(err.contains("diff at offset 1"));
    }

    #[test]
    fn test_assert_bytes_eq_length_mismatch() {
        let a = b"\x90\x91";
        let b = b"\x90\x91\x92";
        let err = assert_bytes_eq(a, b, "test").unwrap_err();
        assert!(err.contains("length mismatch"));
    }

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex_encode(b""), "");
        assert_eq!(hex_encode(b"\x00\xff"), "00ff");
        assert_eq!(hex_encode(b"hello"), "68656c6c6f");
    }
}
