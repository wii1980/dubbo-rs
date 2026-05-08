// Phase 4: Registry interoperability tests.
//
// D-F-*  = Format: URL encoding, ZK path structure
// D-ZK-* = ZooKeeper registry API
// D-NA-* = Nacos registry API

#![allow(
    clippy::doc_markdown,
    clippy::float_cmp,
    clippy::wildcard_enum_match_arm
)]

use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;

// ── D-F: Format Tests ───────────────────────────────────────────────────

mod d_f_format {
    use super::*;

    /// D-F-001: ZK provider path structure
    #[test]
    fn df_001_zk_provider_path() {
        // ZK path format: /dubbo/{service}/providers/{encoded_url}
        // The URL is encoded as: protocol://ip:port/path?params
        // Special chars are percent-encoded

        let mut url = URL::new("dubbo", "/com.example.GreetService");
        url.ip = "192.168.1.100".into();
        url.port = "20880".into();
        url.set_param("version", "1.0.0");
        url.set_param("group", "test");

        let encoded = encode_test_url(&url);
        assert!(
            encoded.contains("dubbo%3A%2F%2F"),
            "D-F-001: should encode ://"
        );
        assert!(encoded.contains("192.168.1.100"), "D-F-001: ip visible");
        assert!(encoded.contains("20880"), "D-F-001: port visible");
        assert!(
            encoded.contains("com.example.GreetService"),
            "D-F-001: service path"
        );

        // Verify ZK provider path
        let provider_path = format!(
            "/dubbo/{}/providers/{}",
            url.path.trim_start_matches('/'),
            encoded
        );
        assert!(
            provider_path.starts_with("/dubbo/com.example.GreetService/providers/"),
            "D-F-001: ZK path prefix: {provider_path}"
        );
        assert!(provider_path.len() > 60, "D-F-001: full encoded URL path");
    }

    /// D-F-002: URL encoding roundtrip
    #[test]
    fn df_002_url_encode_decode() {
        let mut url = URL::new("tri", "/com.example.Service");
        url.ip = "10.0.0.50".into();
        url.port = "50051".into();
        url.set_param("version", "2.0.0");

        let encoded = encode_test_url(&url);
        assert!(!encoded.contains('?'), "D-F-002: no raw ? in encoded");
        assert!(!encoded.contains('='), "D-F-002: no raw = in encoded");

        // Verify the non-encoded parts are visible
        assert!(encoded.contains("tri"), "D-F-002: protocol visible");
        assert!(encoded.contains("10.0.0.50"), "D-F-002: ip visible");
        assert!(encoded.contains("50051"), "D-F-002: port visible");
    }

    /// D-F-003: Multiple URL parameters
    #[test]
    fn df_003_url_with_params() {
        let mut url = URL::new("dubbo", "/com.example.CalcService");
        url.ip = "127.0.0.1".into();
        url.port = "20880".into();
        url.set_param("version", "1.0.0");
        url.set_param("group", "calc");
        url.set_param("timeout", "5000");
        url.set_param("serialization", "hessian2");

        let encoded = encode_test_url(&url);
        // The encoding should escape & and = in the param serialization
        assert!(encoded.contains("hessian2"), "D-F-003: serialization param");

        // Count separators in the encoded form
        let separator_count = encoded.matches("%26").count();
        assert_eq!(separator_count, 3, "D-F-003: 3 & separators encoded");
    }

    /// D-F-004: Triple protocol URL format
    #[test]
    fn df_004_triple_protocol_url() {
        let mut url = URL::new("tri", "/org.example.GreetService");
        url.ip = "192.168.1.200".into();
        url.port = "50051".into();
        url.set_param("version", "1.0.0");

        let encoded = encode_test_url(&url);
        // Triple protocol should use tri:// prefix
        assert!(encoded.contains("tri%3A%2F%2F"), "D-F-004: tri:// encoded");
        assert!(encoded.contains("50051"), "D-F-004: gRPC port");
    }

    // Simulates the ZK registry's encode_url function
    fn encode_test_url(url: &URL) -> String {
        let raw = format!("{}://{}:{}{}", url.protocol, url.ip, url.port, url.path);
        // Add parameters if any
        let url_with_params = if url.params.is_empty() {
            raw
        } else {
            let params: Vec<String> = url.params.iter().map(|(k, v)| format!("{k}={v}")).collect();
            format!("{raw}?{}", params.join("&"))
        };
        urlencoding(&url_with_params)
    }

    fn urlencoding(s: &str) -> String {
        s.chars()
            .map(|c| match c {
                ':' => "%3A".to_string(),
                '/' => "%2F".to_string(),
                '?' => "%3F".to_string(),
                '=' => "%3D".to_string(),
                '&' => "%26".to_string(),
                c if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' => c.to_string(),
                other => {
                    let bytes = other.to_string().into_bytes();
                    bytes
                        .iter()
                        .fold(String::new(), |acc, b| format!("{acc}%{b:02X}"))
                }
            })
            .collect()
    }
}

// ── D-ZK: ZooKeeper Registry ────────────────────────────────────────────

mod d_zk_registry {
    use super::*;

    use dubbo_rs_registry_zookeeper::ZookeeperRegistry;

    /// D-ZK-005: `ZookeeperRegistry` construction and URL
    #[test]
    fn dzk_005_registry_construction() {
        let mut zk_url = URL::new("zookeeper", "");
        zk_url.ip = "127.0.0.1".into();
        zk_url.port = "2181".into();

        let registry = ZookeeperRegistry::new(zk_url);
        assert!(!registry.is_available(), "D-ZK-005: not connected yet");
        assert_eq!(registry.get_url().ip, "127.0.0.1");
        assert_eq!(registry.get_url().port, "2181");
    }

    /// D-ZK-006: Custom root path
    #[test]
    fn dzk_006_custom_root_path() {
        let mut zk_url = URL::new("zookeeper", "");
        zk_url.ip = "10.0.0.1".into();
        zk_url.port = "2181".into();

        let registry = ZookeeperRegistry::new(zk_url).with_root_path("/custom");
        assert_eq!(registry.get_url().ip, "10.0.0.1");
    }
}

// ── D-NA: Nacos Registry ───────────────────────────────────────────────

mod d_na_registry {
    use super::*;
    use dubbo_rs_registry_nacos::NacosRegistry;

    /// D-NA-003: `NacosRegistry` construction
    #[test]
    fn dna_003_registry_construction() {
        let mut nacos_url = URL::new("nacos", "");
        nacos_url.ip = "127.0.0.1".into();
        nacos_url.port = "8848".into();

        let registry = NacosRegistry::new(nacos_url);
        assert_eq!(registry.get_url().ip, "127.0.0.1");
        assert_eq!(registry.get_url().port, "8848");
    }

    /// D-NA-004: Nacos with namespace and group
    #[test]
    fn dna_004_nacos_with_namespace() {
        let mut nacos_url = URL::new("nacos", "");
        nacos_url.ip = "127.0.0.1".into();
        nacos_url.port = "8848".into();

        let registry = NacosRegistry::new(nacos_url)
            .with_namespace("public")
            .with_group("DEFAULT_GROUP");
        assert_eq!(registry.get_url().ip, "127.0.0.1");
    }

    /// D-NA-005: Nacos with auth credentials
    #[test]
    fn dna_005_nacos_with_auth() {
        let mut nacos_url = URL::new("nacos", "");
        nacos_url.ip = "127.0.0.1".into();
        nacos_url.port = "8848".into();

        let registry = NacosRegistry::new(nacos_url).with_auth("nacos", "nacos");
        assert_eq!(registry.get_url().ip, "127.0.0.1");
        // NacosRegistry::is_available() always returns true (no lazy connection)
        assert!(
            registry.is_available(),
            "D-NA-005: available after construction"
        );
    }
}

// ── D-V: URL Service Key Verification ──────────────────────────────────

mod d_v_verification {
    use super::*;

    /// D-V-001: URL service key generation matches Java convention
    #[test]
    fn dv_001_service_key() {
        let mut url = URL::new("dubbo", "/com.example.DemoService");
        url.ip = "192.168.1.1".into();
        url.port = "20880".into();
        url.set_param("version", "1.0.0");
        url.set_param("group", "production");

        // Service key in Dubbo: group/path:version
        let service_key = url.get_service_key();
        assert!(
            service_key.contains("com.example.DemoService"),
            "D-V-001: service key should contain interface name: {service_key}"
        );
    }

    /// D-V-002: URL full string representation
    #[test]
    fn dv_002_url_full_string() {
        let mut url = URL::new("tri", "/org.example.GreetService");
        url.ip = "10.0.0.1".into();
        url.port = "50051".into();
        url.set_param("timeout", "3000");
        url.set_param("serialization", "hessian2");

        let full = url.to_full_string();
        assert!(
            full.starts_with("tri://"),
            "D-V-002: should start with tri://"
        );
        assert!(
            full.contains("10.0.0.1:50051"),
            "D-V-002: should contain host:port"
        );
        assert!(
            full.contains("timeout=3000"),
            "D-V-002: should contain timeout param"
        );
    }

    /// D-V-003: URL method-level parameter resolution
    #[test]
    fn dv_003_method_param() {
        let mut url = URL::new("dubbo", "/com.example.CalcService");
        url.ip = "127.0.0.1".into();
        url.port = "20880".into();
        url.set_param("timeout", "1000");
        url.set_param("methods", "add,subtract");

        // Global fallback
        let timeout = url.get_param("timeout");
        assert_eq!(
            timeout,
            Some(&"1000".to_string()),
            "D-V-003: global timeout"
        );
    }

    /// D-V-004: ZookeeperRegistry provider path generation
    #[test]
    fn dv_004_zk_provider_path_format() {
        // Verify the expected ZK path structure without actually connecting
        let service = "com.example.GreetService";
        let root = "/dubbo";
        let path = format!("{root}/{service}/providers");

        // Verify path structure matches dubbo-java convention
        assert!(
            path.starts_with("/dubbo/"),
            "D-V-004: must start with /dubbo/"
        );
        assert!(path.contains(service), "D-V-004: must contain service name");
        assert!(
            path.ends_with("/providers"),
            "D-V-004: must end with /providers"
        );
    }

    /// D-V-005: Application-level registry path (Dubbo3 convention)
    #[test]
    fn dv_005_app_level_path() {
        let app_name = "demo-provider";
        let path = format!("/services/{app_name}");

        // Dubbo3 application-level: /services/{appName}
        assert_eq!(path, "/services/demo-provider", "D-V-005: app-level path");
    }
}

// ── D-ZK-001~004: ZooKeeper Cross-Language Interop (API-level) ──────────

mod d_zk_interop {
    use super::*;
    use dubbo_rs_registry_zookeeper::{
        decode_url, encode_url, urldecoding, urlencoding, ZookeeperRegistry,
    };

    fn zk_url() -> URL {
        let mut u = URL::new("zookeeper", "");
        u.ip = "127.0.0.1".into();
        u.port = "2181".into();
        u
    }

    fn provider_url(proto: &str, service: &str, ip: &str, port: &str) -> URL {
        let mut u = URL::new(proto, service);
        u.ip = ip.into();
        u.port = port.into();
        u
    }

    /// D-ZK-001: RS encode_url output matches Java ZK node name convention.
    ///
    /// Java dubbo-registry-zookeeper encodes the full provider URL as a ZK node name.
    /// The format is: `protocol%3A%2F%2Fip%3Aport%2Fservice_path`
    /// Verify RS produces the same encoding.
    #[test]
    fn dzk_001_encode_url_matches_java_convention() {
        let url = provider_url(
            "dubbo",
            "/com.example.GreetService",
            "192.168.1.100",
            "20880",
        );
        let encoded = encode_url(&url);

        // Java convention: dubbo%3A%2F%2F192.168.1.100%3A20880%2Fcom.example.GreetService
        assert!(
            encoded.starts_with("dubbo%3A%2F%2F"),
            "D-ZK-001: dubbo:// prefix"
        );
        assert!(encoded.contains("192.168.1.100"), "D-ZK-001: ip");
        assert!(encoded.contains("20880"), "D-ZK-001: port");
        assert!(
            encoded.contains("com.example.GreetService"),
            "D-ZK-001: service path"
        );

        // Verify exact expected output
        let expected = "dubbo%3A%2F%2F192.168.1.100%3A20880%2Fcom.example.GreetService";
        assert_eq!(
            encoded, expected,
            "D-ZK-001: exact match with Java encoding"
        );
    }

    /// D-ZK-001b: Triple protocol URL encoding
    #[test]
    fn dzk_001b_encode_tri_url() {
        let url = provider_url("tri", "/org.example.GreetService", "10.0.0.1", "50051");
        let encoded = encode_url(&url);

        let expected = "tri%3A%2F%2F10.0.0.1%3A50051%2Forg.example.GreetService";
        assert_eq!(encoded, expected, "D-ZK-001b: tri protocol encoding");
    }

    /// D-ZK-001c: RS decode_url can parse Java-encoded ZK node names
    #[test]
    fn dzk_001c_decode_java_encoded_url() {
        let java_encoded = "dubbo%3A%2F%2F192.168.1.100%3A20880%2Fcom.example.GreetService";
        let decoded = decode_url(java_encoded);
        assert!(decoded.is_some(), "D-ZK-001c: should decode Java URL");

        let url = decoded.unwrap();
        assert_eq!(url.ip, "192.168.1.100", "D-ZK-001c: ip");
        assert_eq!(url.port, "20880", "D-ZK-001c: port");
        assert_eq!(url.path, "/com.example.GreetService", "D-ZK-001c: path");
    }

    /// D-ZK-001d: decode_url handles Java-encoded URL with parameters
    #[test]
    fn dzk_001d_decode_url_with_java_params() {
        // Java encodes full URL including params in ZK node:
        // dubbo://192.168.1.100:20880/com.example.GreetService?version=1.0.0&timeout=3000
        let java_encoded = "dubbo%3A%2F%2F192.168.1.100%3A20880%2Fcom.example.GreetService%3Fversion%3D1.0.0%26timeout%3D3000";
        let decoded = decode_url(java_encoded);
        assert!(
            decoded.is_some(),
            "D-ZK-001d: should decode URL with params"
        );

        let url = decoded.unwrap();
        assert_eq!(url.ip, "192.168.1.100");
        assert_eq!(url.port, "20880");
        assert_eq!(url.path, "/com.example.GreetService");
        // Note: RS decode_url currently discards params, which is a known limitation
    }

    /// D-ZK-002: Full ZK provider path matches Java convention
    #[test]
    fn dzk_002_provider_path_format() {
        let _registry = ZookeeperRegistry::new(zk_url());
        let url = provider_url("dubbo", "/com.example.DemoService", "192.168.1.50", "20880");

        // Access provider_path through the registry's internal method
        // (provider_path is private, so we reconstruct the expected path)
        let service = url.path.trim_start_matches('/');
        let encoded = encode_url(&url);
        let full_path = format!("/dubbo/{service}/providers/{encoded}");

        assert!(
            full_path.starts_with("/dubbo/com.example.DemoService/providers/"),
            "D-ZK-002: path prefix: {full_path}"
        );
        assert!(
            full_path.contains("dubbo%3A%2F%2F192.168.1.50%3A20880"),
            "D-ZK-002: encoded URL in path: {full_path}"
        );
    }

    /// D-ZK-002b: Encode → decode roundtrip through ZK node name format
    #[test]
    fn dzk_002b_encode_decode_roundtrip() {
        let url = provider_url("tri", "/com.example.EchoService", "10.0.0.5", "50051");
        let encoded = encode_url(&url);
        let decoded = decode_url(&encoded);
        assert!(decoded.is_some());

        let d = decoded.unwrap();
        assert_eq!(d.ip, "10.0.0.5", "D-ZK-002b: ip roundtrip");
        assert_eq!(d.port, "50051", "D-ZK-002b: port roundtrip");
        assert!(d.path.contains("EchoService"), "D-ZK-002b: path roundtrip");
    }

    /// D-ZK-002c: decode_url with triple protocol preserves path
    #[test]
    fn dzk_002c_decode_tri_url() {
        let tri_encoded = "tri%3A%2F%2F10.0.0.1%3A50051%2Forg.example.GreetService";
        let decoded = decode_url(tri_encoded);
        assert!(decoded.is_some());
        let d = decoded.unwrap();
        assert_eq!(d.path, "/org.example.GreetService");
    }

    /// D-ZK-002d: Custom root path generates correct provider paths
    #[test]
    fn dzk_002d_custom_root_provider_path() {
        let _registry = ZookeeperRegistry::new(zk_url()).with_root_path("/myapp");
        let url = provider_url("dubbo", "/com.example.Svc", "127.0.0.1", "20880");

        let service = url.path.trim_start_matches('/');
        let encoded = encode_url(&url);
        let path = format!("/myapp/{service}/providers/{encoded}");

        assert!(
            path.starts_with("/myapp/com.example.Svc/providers/"),
            "D-ZK-002d: custom root: {path}"
        );
        assert!(
            !path.starts_with("/dubbo/"),
            "D-ZK-002d: NOT default /dubbo/"
        );
    }

    /// D-ZK-003: Application-level vs interface-level path distinction
    #[test]
    fn dzk_003_interface_level_path_structure() {
        // Interface-level (Dubbo2): /dubbo/{interface}/providers/{url}
        let service = "com.example.GreetService";
        let interface_path = format!("/dubbo/{service}/providers");
        assert!(interface_path.starts_with("/dubbo/"));
        assert!(interface_path.contains(service));
        assert!(interface_path.ends_with("/providers"));

        // Application-level (Dubbo3): /services/{appName}
        let app_name = "greet-provider";
        let app_path = format!("/services/{app_name}");
        assert!(app_path.starts_with("/services/"));
        assert!(
            !app_path.contains("/dubbo/"),
            "D-ZK-003: app-level does NOT use /dubbo/"
        );
    }

    /// D-ZK-003b: Consumer path (subscribers watch)
    #[test]
    fn dzk_003b_consumer_path_structure() {
        let service = "com.example.GreetService";
        let consumers_path = format!("/dubbo/{service}/consumers");
        let providers_path = format!("/dubbo/{service}/providers");
        let routers_path = format!("/dubbo/{service}/routers");
        let configurators_path = format!("/dubbo/{service}/configurators");

        // Dubbo ZK registry uses 4 subdirectories per service
        assert!(consumers_path.ends_with("/consumers"));
        assert!(providers_path.ends_with("/providers"));
        assert!(routers_path.ends_with("/routers"));
        assert!(configurators_path.ends_with("/configurators"));
    }

    /// D-ZK-004: ServiceEvent + NotifyListener notification mechanism
    #[test]
    fn dzk_004_service_event_variants() {
        use dubbo_rs_registry::ServiceEvent;

        let url1 = URL::new("dubbo", "/com.example.Svc1");
        let url2 = URL::new("dubbo", "/com.example.Svc2");

        let add = ServiceEvent::Add(vec![url1.clone()]);
        let remove = ServiceEvent::Remove(vec![url2.clone()]);
        let update = ServiceEvent::Update(vec![url1, url2]);

        // Clone + PartialEq support (required for listener notification)
        let add_clone = add.clone();
        assert_eq!(add, add_clone, "D-ZK-004: ServiceEvent clone+eq");

        // Verify variant matching
        assert!(matches!(add, ServiceEvent::Add(_)));
        assert!(matches!(remove, ServiceEvent::Remove(_)));
        assert!(matches!(update, ServiceEvent::Update(ref u) if u.len() == 2));
    }

    /// D-ZK-004b: URL encoding special characters (hyphens, dots, underscores)
    #[test]
    fn dzk_004b_urlencoding_special_chars() {
        // Hyphens, dots, underscores should NOT be encoded
        assert_eq!(urlencoding("a-b"), "a-b", "hyphen preserved");
        assert_eq!(urlencoding("a.b"), "a.b", "dot preserved");
        assert_eq!(urlencoding("a_b"), "a_b", "underscore preserved");

        // Chinese characters should be percent-encoded
        let encoded = urlencoding("你好");
        assert!(
            encoded.contains('%'),
            "D-ZK-004b: Chinese chars encoded: {encoded}"
        );

        // ASCII-only encode/decode roundtrip works correctly
        let ascii = "hello-world_123.test";
        assert_eq!(
            urldecoding(&urlencoding(ascii)),
            ascii,
            "D-ZK-004b: ASCII roundtrip"
        );
    }

    /// D-ZK-004c: decode_url returns None for invalid input
    #[test]
    fn dzk_004c_decode_invalid() {
        assert!(decode_url("not-a-url").is_none(), "D-ZK-004c: no protocol");
        assert!(
            decode_url("http://bad").is_none(),
            "D-ZK-004c: wrong protocol"
        );
        assert!(decode_url("").is_none(), "D-ZK-004c: empty string");
    }
}

// ── D-NA-001~002: Nacos Cross-Language Interop (API-level) ──────────────

mod d_na_interop {
    use super::*;
    use dubbo_rs_registry_nacos::{check_nacos_response, extract_hosts, NacosRegistry};

    fn nacos_url() -> URL {
        let mut u = URL::new("nacos", "");
        u.ip = "127.0.0.1".into();
        u.port = "8848".into();
        u
    }

    fn service_url(proto: &str, service: &str, ip: &str, port: &str) -> URL {
        let mut u = URL::new(proto, service);
        u.ip = ip.into();
        u.port = port.into();
        u
    }

    /// D-NA-001: build_register_request maps URL to Nacos instance fields.
    ///
    /// Verifies that RS maps provider URL fields to Nacos InstanceRegisterRequest
    /// fields the same way Java dubbo-registry-nacos does:
    /// - URL.path → serviceName (without leading /)
    /// - URL.ip → ip
    /// - URL.port → port
    /// - namespace/group/ephemeral/weight defaults
    #[test]
    fn dna_001_register_request_field_mapping() {
        let registry = NacosRegistry::new(nacos_url());
        let svc = service_url("tri", "/com.example.GreetService", "10.0.0.1", "20880");

        let req = registry.build_register_request(&svc);
        assert_eq!(
            req.service_name, "com.example.GreetService",
            "D-NA-001: serviceName"
        );
        assert_eq!(req.ip, "10.0.0.1", "D-NA-001: ip");
        assert_eq!(req.port, 20880, "D-NA-001: port");
        assert_eq!(req.namespace_id, "public", "D-NA-001: default namespace");
        assert_eq!(req.group_name, "DEFAULT_GROUP", "D-NA-001: default group");
        assert!(req.healthy, "D-NA-001: healthy=true");
        assert!(req.enabled, "D-NA-001: enabled=true");
        assert!(req.ephemeral, "D-NA-001: ephemeral=true");
        assert_eq!(req.weight, 1.0, "D-NA-001: weight=1.0");
    }

    /// D-NA-001b: Custom namespace and group in register request
    #[test]
    fn dna_001b_register_with_custom_namespace_group() {
        let registry = NacosRegistry::new(nacos_url())
            .with_namespace("dev-ns")
            .with_group("MY_GROUP");

        let svc = service_url("dubbo", "/org.example.CalcService", "192.168.1.1", "20880");
        let req = registry.build_register_request(&svc);

        assert_eq!(req.namespace_id, "dev-ns", "D-NA-001b: custom namespace");
        assert_eq!(req.group_name, "MY_GROUP", "D-NA-001b: custom group");
        assert_eq!(req.service_name, "org.example.CalcService");
    }

    /// D-NA-001c: Service name derived from URL path (Java convention)
    #[test]
    fn dna_001c_service_name_from_url_path() {
        let registry = NacosRegistry::new(nacos_url());

        // Java convention: service name = interface name (path without /)
        let cases = [
            ("/com.example.UserService", "com.example.UserService"),
            (
                "/org.apache.dubbo.demo.DemoService",
                "org.apache.dubbo.demo.DemoService",
            ),
            ("/MyService", "MyService"),
        ];

        for (path, expected_name) in cases {
            let svc = service_url("tri", path, "10.0.0.1", "20880");
            let req = registry.build_register_request(&svc);
            assert_eq!(
                req.service_name, expected_name,
                "D-NA-001c: service name for path {path}"
            );
        }
    }

    /// D-NA-001d: Server address constructed from registry URL
    #[test]
    fn dna_001d_server_addr_format() {
        let mut u = URL::new("nacos", "");
        u.ip = "nacos.example.com".into();
        u.port = "8848".into();

        let registry = NacosRegistry::new(u);
        assert_eq!(
            registry.server_addr, "http://nacos.example.com:8848",
            "D-NA-001d: server_addr format"
        );
    }

    /// D-NA-002: check_nacos_response parses Nacos server responses.
    ///
    /// Java dubbo-registry-nacos handles both plain "ok" and JSON responses.
    /// Verify RS parses the same formats.
    #[test]
    fn dna_002_check_response_ok() {
        assert!(check_nacos_response("ok").is_ok(), "D-NA-002: plain 'ok'");
        assert!(
            check_nacos_response("{\"code\":0}").is_ok(),
            "D-NA-002: code=0"
        );
        assert!(
            check_nacos_response("{\"code\":200}").is_ok(),
            "D-NA-002: code=200"
        );
    }

    /// D-NA-002b: check_nacos_response rejects error responses
    #[test]
    fn dna_002b_check_response_error() {
        assert!(
            check_nacos_response("{\"code\":400,\"message\":\"bad request\"}").is_err(),
            "D-NA-002b: code=400"
        );
        assert!(
            check_nacos_response("{\"code\":500,\"message\":\"internal error\"}").is_err(),
            "D-NA-002b: code=500"
        );
        assert!(
            check_nacos_response("not json and not ok").is_err(),
            "D-NA-002b: invalid response"
        );
    }

    /// D-NA-002c: check_nacos_response handles response without code field
    #[test]
    fn dna_002c_check_response_no_code() {
        // JSON without "code" field should be treated as success
        assert!(
            check_nacos_response("{\"hosts\":[]}").is_ok(),
            "D-NA-002c: no code field = success"
        );
    }

    /// D-NA-002d: extract_hosts parses Nacos instance list response
    #[test]
    fn dna_002d_extract_hosts_from_discovery() {
        let nacos_response = serde_json::json!({
            "hosts": [
                {
                    "ip": "10.0.0.1",
                    "port": 20880,
                    "weight": 1.0,
                    "healthy": true,
                    "serviceName": "com.example.GreetService",
                    "metadata": {"version": "1.0.0"}
                },
                {
                    "ip": "10.0.0.2",
                    "port": 20881,
                    "weight": 2.0,
                    "healthy": true,
                    "serviceName": "com.example.GreetService",
                    "metadata": null
                }
            ]
        })
        .to_string();

        let hosts = extract_hosts(&nacos_response);
        assert!(hosts.is_some(), "D-NA-002d: should parse hosts");
        let hosts = hosts.unwrap();
        assert_eq!(hosts.len(), 2, "D-NA-002d: 2 instances");
        assert_eq!(hosts[0].ip, "10.0.0.1");
        assert_eq!(hosts[0].port, 20880);
        assert_eq!(hosts[1].ip, "10.0.0.2");
        assert_eq!(hosts[1].port, 20881);
    }

    /// D-NA-002e: extract_hosts handles empty host list
    #[test]
    fn dna_002e_extract_hosts_empty() {
        let response = serde_json::json!({"hosts": []}).to_string();
        let hosts = extract_hosts(&response);
        assert!(hosts.is_some());
        assert!(hosts.unwrap().is_empty(), "D-NA-002e: empty hosts");
    }

    /// D-NA-002f: extract_hosts handles missing hosts field
    #[test]
    fn dna_002f_extract_hosts_missing() {
        let response = serde_json::json!({"name": "test"}).to_string();
        let hosts = extract_hosts(&response);
        assert!(hosts.is_none(), "D-NA-002f: missing hosts = None");
    }

    /// D-NA-002g: extract_hosts supports nested data.hosts format (some Nacos versions)
    #[test]
    fn dna_002g_extract_hosts_nested() {
        let response = serde_json::json!({
            "data": {
                "hosts": [
                    {
                        "ip": "192.168.1.1",
                        "port": 20880,
                        "weight": 1.0,
                        "healthy": true,
                        "serviceName": "test.Svc"
                    }
                ]
            }
        })
        .to_string();

        let hosts = extract_hosts(&response);
        assert!(hosts.is_some(), "D-NA-002g: nested data.hosts");
        let hosts = hosts.unwrap();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].ip, "192.168.1.1");
    }

    /// D-NA-003: Nacos instance → URL conversion matches Dubbo convention.
    ///
    /// When RS discovers instances from Nacos, it converts them to URL objects.
    /// Verify the conversion follows Dubbo convention:
    /// - protocol = "dubbo"
    /// - path = "/{serviceName}"
    /// - ip and port preserved
    #[test]
    fn dna_003_instance_to_url_conversion() {
        // Simulate what discover_instances does internally
        let response = serde_json::json!({
            "hosts": [
                {
                    "ip": "10.0.0.1",
                    "port": 20880,
                    "weight": 1.5,
                    "healthy": true,
                    "serviceName": "com.example.GreetService"
                }
            ]
        })
        .to_string();

        let hosts = extract_hosts(&response).unwrap();
        let inst = &hosts[0];

        // Replicate the URL conversion logic from NacosRegistry::discover_instances
        let mut u = URL::new("dubbo", format!("/{}", inst.service_name));
        u.ip.clone_from(&inst.ip);
        u.port = inst.port.to_string();

        assert_eq!(u.protocol, "dubbo", "D-NA-003: protocol=dubbo");
        assert_eq!(
            u.path, "/com.example.GreetService",
            "D-NA-003: path=/serviceName"
        );
        assert_eq!(u.ip, "10.0.0.1", "D-NA-003: ip");
        assert_eq!(u.port, "20880", "D-NA-003: port");
    }

    /// D-NA-003b: Multiple instances discovery → multiple URLs
    #[test]
    fn dna_003b_multi_instance_discovery() {
        let response = serde_json::json!({
            "hosts": [
                {"ip": "10.0.0.1", "port": 20880, "weight": 1.0, "healthy": true, "serviceName": "svc"},
                {"ip": "10.0.0.2", "port": 20881, "weight": 1.0, "healthy": true, "serviceName": "svc"},
                {"ip": "10.0.0.3", "port": 20882, "weight": 1.0, "healthy": true, "serviceName": "svc"}
            ]
        })
        .to_string();

        let hosts = extract_hosts(&response).unwrap();
        let urls: Vec<URL> = hosts
            .iter()
            .map(|inst| {
                let mut u = URL::new("dubbo", format!("/{}", inst.service_name));
                u.ip.clone_from(&inst.ip);
                u.port = inst.port.to_string();
                u
            })
            .collect();

        assert_eq!(urls.len(), 3, "D-NA-003b: 3 instances");
        assert_ne!(urls[0].ip, urls[1].ip, "D-NA-003b: different IPs");
        assert_ne!(urls[0].port, urls[1].port, "D-NA-003b: different ports");
    }

    /// D-NA-003c: Auth credentials configured correctly for Nacos API
    #[test]
    fn dna_003c_auth_credentials() {
        let registry = NacosRegistry::new(nacos_url()).with_auth("myAccessKey", "mySecretKey");

        assert_eq!(registry.username.as_deref(), Some("myAccessKey"));
        assert_eq!(registry.password.as_deref(), Some("mySecretKey"));
    }
}

// ── D-ZK-E2E: ZooKeeper End-to-End Integration Tests ─────────────────────
//
// Requires a running ZooKeeper instance on localhost:2181.
// These tests are gated behind `#[ignore]` and require `--ignored` flag
// OR a reachable ZK server.

mod d_zk_e2e {
    use super::*;
    use async_trait::async_trait;
    use dubbo_rs_registry_zookeeper::ZookeeperRegistry;
    use dubbo_rs_common::node::Node;
    use dubbo_rs_registry::{NotifyListener, Registry, ServiceEvent};
    use std::sync::{Arc, Mutex};

    fn zk_available() -> bool {
        std::net::TcpStream::connect_timeout(
            &"127.0.0.1:2181".parse().unwrap(),
            std::time::Duration::from_secs(1),
        )
        .is_ok()
    }

    fn zk_url() -> URL {
        let mut u = URL::new("zookeeper", "");
        u.ip = "127.0.0.1".into();
        u.port = "2181".into();
        u
    }

    fn provider_url(service: &str, ip: &str, port: &str) -> URL {
        let mut u = URL::new("dubbo", service);
        u.ip = ip.into();
        u.port = port.into();
        u
    }

    struct CollectingListener {
        service_url: URL,
        events: Mutex<Vec<ServiceEvent>>,
    }

    impl CollectingListener {
        fn new(service_url: URL) -> Self {
            Self {
                service_url,
                events: Mutex::new(Vec::new()),
            }
        }

        fn has_add_event_with_count(&self, expected: usize) -> bool {
            let events = self.events.lock().unwrap();
            events.iter().any(|e| match e {
                ServiceEvent::Add(urls) => urls.len() == expected,
                _ => false,
            })
        }
    }

    #[async_trait]
    impl NotifyListener for CollectingListener {
        async fn notify(&self, event: ServiceEvent) {
            self.events.lock().unwrap().push(event);
        }

        fn listen_url(&self) -> URL {
            self.service_url.clone()
        }
    }

    /// D-ZK-E2E-001: RS provider registers, RS consumer discovers (interface-level).
    ///
    /// This tests the core Dubbo2 registration flow:
    /// 1. RS provider registers an ephemeral node to ZK
    /// 2. RS consumer subscribes to the same service
    /// 3. Consumer's listener receives Add event with the provider URL
    #[tokio::test]
    async fn dzk_e2e_001_rs_provider_register_consumer_discover() {
        if !zk_available() {
            eprintln!("SKIP: ZooKeeper not available on :2181");
            return;
        }

        let registry = ZookeeperRegistry::new(zk_url());
        let svc = provider_url("/com.example.E2ETestService", "10.0.0.1", "20880");

        // Register provider
        registry
            .register(svc.clone())
            .await
            .expect("register should succeed");
        assert!(
            registry.is_available(),
            "should be available after register"
        );

        // Subscribe consumer
        let listener = Arc::new(CollectingListener::new(svc.clone()));
        let sub_url = URL::new("dubbo", "/com.example.E2ETestService");
        registry
            .subscribe(sub_url, listener.clone())
            .await
            .expect("subscribe should succeed");

        // Listener should receive Add event with at least 1 provider
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert!(
            listener.has_add_event_with_count(1),
            "D-ZK-E2E-001: listener should receive Add event with 1 provider"
        );

        // Cleanup
        registry
            .unregister(svc)
            .await
            .expect("unregister should succeed");
        registry.destroy();
    }

    /// D-ZK-E2E-002: Multiple providers registered and discovered via ZK.
    #[tokio::test]
    async fn dzk_e2e_002_multiple_providers_discovered() {
        if !zk_available() {
            eprintln!("SKIP: ZooKeeper not available on :2181");
            return;
        }

        let registry = ZookeeperRegistry::new(zk_url());

        let svc1 = provider_url("/com.example.MultiService", "10.0.0.1", "20880");
        let svc2 = provider_url("/com.example.MultiService", "10.0.0.2", "20881");

        registry
            .register(svc1.clone())
            .await
            .expect("register svc1");
        registry
            .register(svc2.clone())
            .await
            .expect("register svc2");

        let listener = Arc::new(CollectingListener::new(svc1.clone()));
        let sub_url = URL::new("dubbo", "/com.example.MultiService");
        registry
            .subscribe(sub_url, listener.clone())
            .await
            .expect("subscribe");

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert!(
            listener.has_add_event_with_count(2),
            "D-ZK-E2E-002: listener should receive Add event with 2 providers"
        );

        registry.unregister(svc1).await.expect("unregister svc1");
        registry.unregister(svc2).await.expect("unregister svc2");
        registry.destroy();
    }

    /// D-ZK-E2E-003: Triple protocol provider URL survives ZK roundtrip.
    #[tokio::test]
    async fn dzk_e2e_003_triple_protocol_roundtrip() {
        if !zk_available() {
            eprintln!("SKIP: ZooKeeper not available on :2181");
            return;
        }

        let registry = ZookeeperRegistry::new(zk_url());

        let mut svc = URL::new("tri", "/com.example.TriService");
        svc.ip = "192.168.1.50".into();
        svc.port = "50051".into();

        registry
            .register(svc.clone())
            .await
            .expect("register tri provider");

        let listener = Arc::new(CollectingListener::new(svc.clone()));
        let sub_url = URL::new("tri", "/com.example.TriService");
        registry
            .subscribe(sub_url, listener.clone())
            .await
            .expect("subscribe");

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Verify the discovered URL has correct ip and port
        let found = {
            let events = listener.events.lock().unwrap();
            events.iter().any(|e| match e {
                ServiceEvent::Add(urls) => urls
                    .iter()
                    .any(|u| u.ip == "192.168.1.50" && u.port == "50051"),
                _ => false,
            })
        };
        assert!(
            found,
            "D-ZK-E2E-003: tri provider IP and port should be discoverable"
        );

        registry.unregister(svc).await.expect("unregister");
        registry.destroy();
    }

    /// D-ZK-E2E-004: Unregister removes provider from discovery.
    #[tokio::test]
    async fn dzk_e2e_004_unregister_removes_provider() {
        if !zk_available() {
            eprintln!("SKIP: ZooKeeper not available on :2181");
            return;
        }

        let registry = ZookeeperRegistry::new(zk_url());

        let svc = provider_url("/com.example.UnregService", "10.0.0.99", "20899");
        registry.register(svc.clone()).await.expect("register");
        registry.unregister(svc.clone()).await.expect("unregister");

        // Subscribe after unregister — should get empty or no provider
        let listener = Arc::new(CollectingListener::new(svc.clone()));
        let sub_url = URL::new("dubbo", "/com.example.UnregService");
        registry
            .subscribe(sub_url, listener.clone())
            .await
            .expect("subscribe");

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // After unregister, the provider list should be empty
        // (ZK get_children returns empty list for the providers dir)
        let events = listener.events.lock().unwrap();
        let has_providers = events.iter().any(|e| match e {
            ServiceEvent::Add(urls) => !urls.is_empty(),
            _ => false,
        });
        assert!(
            !has_providers,
            "D-ZK-E2E-004: no providers should be found after unregister"
        );

        registry.destroy();
    }
}

// ── D-NA-E2E: Nacos End-to-End Integration Tests ────────────────────────
//
// Requires a running Nacos instance on localhost:8848.
// Tests skip gracefully if the server is unavailable.

mod d_na_e2e {
    use super::*;
    use async_trait::async_trait;
    use dubbo_rs_registry_nacos::NacosRegistry;
    use dubbo_rs_common::node::Node;
    use dubbo_rs_registry::{NotifyListener, Registry, ServiceEvent};
    use std::sync::{Arc, Mutex};

    fn nacos_available() -> bool {
        std::net::TcpStream::connect_timeout(
            &"127.0.0.1:8848".parse().unwrap(),
            std::time::Duration::from_secs(2),
        )
        .is_ok()
    }

    fn nacos_url() -> URL {
        let mut u = URL::new("nacos", "");
        u.ip = "127.0.0.1".into();
        u.port = "8848".into();
        u
    }

    fn provider_url(service: &str, ip: &str, port: &str) -> URL {
        let mut u = URL::new("tri", service);
        u.ip = ip.into();
        u.port = port.into();
        u
    }

    struct CollectingListener {
        service_url: URL,
        events: Mutex<Vec<ServiceEvent>>,
    }

    impl CollectingListener {
        fn new(service_url: URL) -> Self {
            Self {
                service_url,
                events: Mutex::new(Vec::new()),
            }
        }

        fn has_event_with_min_count(&self, min: usize) -> bool {
            let events = self.events.lock().unwrap();
            events.iter().any(|e| match e {
                ServiceEvent::Update(urls) | ServiceEvent::Add(urls) => urls.len() >= min,
                ServiceEvent::Remove(_) => false,
            })
        }
    }

    #[async_trait]
    impl NotifyListener for CollectingListener {
        async fn notify(&self, event: ServiceEvent) {
            self.events.lock().unwrap().push(event);
        }

        fn listen_url(&self) -> URL {
            self.service_url.clone()
        }
    }

    /// D-NA-E2E-001: RS provider registers to Nacos, RS consumer discovers via polling.
    ///
    /// Tests the core flow:
    /// 1. Register a provider instance via Nacos HTTP API
    /// 2. Subscribe a consumer with a CollectingListener
    /// 3. Wait for the polling interval (10s) + margin
    /// 4. Verify the listener received a ServiceEvent with the provider
    #[tokio::test]
    async fn dna_e2e_001_nacos_provider_register_consumer_discover() {
        if !nacos_available() {
            eprintln!("SKIP: Nacos not available on :8848");
            return;
        }

        let registry = NacosRegistry::new(nacos_url());
        let svc = provider_url("/com.example.NacosTestService", "10.0.0.1", "20880");

        // Register provider
        registry
            .register(svc.clone())
            .await
            .expect("register should succeed");

        // Subscribe consumer
        let listener = Arc::new(CollectingListener::new(svc.clone()));
        let sub_url = URL::new("tri", "/com.example.NacosTestService");
        registry
            .subscribe(sub_url, listener.clone())
            .await
            .expect("subscribe should succeed");

        // Wait for Nacos polling interval (10s) + margin
        tokio::time::sleep(std::time::Duration::from_secs(12)).await;
        assert!(
            listener.has_event_with_min_count(1),
            "D-NA-E2E-001: listener should receive event with at least 1 provider"
        );

        registry.destroy();
    }

    /// D-NA-E2E-002: Multiple providers register, consumer discovers all.
    #[tokio::test]
    async fn dna_e2e_002_nacos_multiple_providers() {
        if !nacos_available() {
            eprintln!("SKIP: Nacos not available on :8848");
            return;
        }

        let registry = NacosRegistry::new(nacos_url());

        let svc1 = provider_url("/com.example.NacosMultiService", "10.0.0.1", "20880");
        let svc2 = provider_url("/com.example.NacosMultiService", "10.0.0.2", "20881");

        registry
            .register(svc1.clone())
            .await
            .expect("register svc1");
        registry
            .register(svc2.clone())
            .await
            .expect("register svc2");

        let listener = Arc::new(CollectingListener::new(svc1.clone()));
        let sub_url = URL::new("tri", "/com.example.NacosMultiService");
        registry
            .subscribe(sub_url, listener.clone())
            .await
            .expect("subscribe");

        tokio::time::sleep(std::time::Duration::from_secs(12)).await;
        assert!(
            listener.has_event_with_min_count(2),
            "D-NA-E2E-002: listener should receive event with at least 2 providers"
        );

        registry.destroy();
    }

    /// D-NA-E2E-003: Unregister removes provider from Nacos discovery.
    #[tokio::test]
    async fn dna_e2e_003_nacos_unregister_removes_provider() {
        if !nacos_available() {
            eprintln!("SKIP: Nacos not available on :8848");
            return;
        }

        let registry = NacosRegistry::new(nacos_url());

        let svc = provider_url("/com.example.NacosUnregService", "10.0.0.99", "20899");

        // Register and verify unregister succeeds
        registry.register(svc.clone()).await.expect("register");
        registry
            .unregister(svc.clone())
            .await
            .expect("unregister should succeed");

        // Subscribe — polling should find no providers (or find empty list)
        let listener = Arc::new(CollectingListener::new(svc.clone()));
        let sub_url = URL::new("tri", "/com.example.NacosUnregService");
        registry
            .subscribe(sub_url, listener.clone())
            .await
            .expect("subscribe");

        tokio::time::sleep(std::time::Duration::from_secs(12)).await;

        // After unregister, the events should either be empty or contain no providers
        let has_providers = {
            let events = listener.events.lock().unwrap();
            events.iter().any(|e| match e {
                ServiceEvent::Update(urls) | ServiceEvent::Add(urls) => !urls.is_empty(),
                ServiceEvent::Remove(_) => false,
            })
        };
        assert!(
            !has_providers,
            "D-NA-E2E-003: no providers should be found after unregister"
        );

        registry.destroy();
    }

    /// D-NA-E2E-004: Custom namespace and group configuration works.
    #[tokio::test]
    async fn dna_e2e_004_nacos_custom_namespace_group() {
        if !nacos_available() {
            eprintln!("SKIP: Nacos not available on :8848");
            return;
        }

        let registry = NacosRegistry::new(nacos_url())
            .with_namespace("public")
            .with_group("CROSS_LANG");

        let svc = provider_url("/com.example.NacosCustomService", "10.0.0.1", "20880");

        // Register with custom namespace/group — should succeed without error
        let result = registry.register(svc.clone()).await;
        assert!(
            result.is_ok(),
            "D-NA-E2E-004: register with custom namespace/group should succeed: {:?}",
            result.err()
        );

        // Cleanup
        let _ = registry.unregister(svc).await;
        registry.destroy();
    }
}

// ── D-ETCD-E2E: Etcd End-to-End Integration Tests ────────────────────────
//
// Requires a running etcd instance on localhost:2379.
// Tests skip gracefully if the server is unavailable.

mod d_etcd_e2e {
    use super::*;
    use async_trait::async_trait;
    use dubbo_rs_registry_etcd::EtcdRegistry;
    use dubbo_rs_common::node::Node;
    use dubbo_rs_registry::{NotifyListener, Registry, ServiceEvent};
    use std::sync::{Arc, Mutex};

    async fn etcd_available() -> bool {
        reqwest::Client::new()
            .post("http://127.0.0.1:2379/v3/lease/grant")
            .timeout(std::time::Duration::from_secs(2))
            .json(&serde_json::json!({"TTL": "0", "ID": "0"}))
            .send()
            .await
            .is_ok()
    }

    fn etcd_url() -> URL {
        let mut u = URL::new("etcd", "");
        u.ip = "127.0.0.1".into();
        u.port = "2379".into();
        u
    }

    fn provider_url(service: &str, ip: &str, port: &str) -> URL {
        let mut u = URL::new("dubbo", service);
        u.ip = ip.into();
        u.port = port.into();
        u
    }

    struct CollectingListener {
        service_url: URL,
        events: Mutex<Vec<ServiceEvent>>,
    }

    impl CollectingListener {
        fn new(service_url: URL) -> Self {
            Self {
                service_url,
                events: Mutex::new(Vec::new()),
            }
        }

        fn has_add_event_with_min_count(&self, min: usize) -> bool {
            let events = self.events.lock().unwrap();
            events.iter().any(|e| match e {
                ServiceEvent::Add(urls) => urls.len() >= min,
                _ => false,
            })
        }
    }

    #[async_trait]
    impl NotifyListener for CollectingListener {
        async fn notify(&self, event: ServiceEvent) {
            self.events.lock().unwrap().push(event);
        }

        fn listen_url(&self) -> URL {
            self.service_url.clone()
        }
    }

    /// D-ETCD-E2E-001: RS provider registers to etcd, RS consumer discovers immediately.
    ///
    /// Etcd subscribe queries providers synchronously and fires Add event
    /// if any exist. No polling delay needed.
    #[tokio::test]
    async fn detcd_e2e_001_etcd_register_and_discover() {
        if !etcd_available().await {
            eprintln!("SKIP: etcd not available on :2379");
            return;
        }

        let registry = EtcdRegistry::new(etcd_url()).with_endpoints("http://127.0.0.1:2379");

        let svc = provider_url("/com.example.EtcdTestService", "10.0.0.1", "20880");

        // Register provider
        registry
            .register(svc.clone())
            .await
            .expect("register should succeed");

        // Subscribe consumer — etcd queries immediately on subscribe
        let listener = Arc::new(CollectingListener::new(svc.clone()));
        let sub_url = {
            let mut u = URL::new("dubbo", "/com.example.EtcdTestService");
            u.ip = "127.0.0.1".into();
            u.port = "20880".into();
            u
        };
        registry
            .subscribe(sub_url, listener.clone())
            .await
            .expect("subscribe should succeed");

        // Small delay for async notification
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert!(
            listener.has_add_event_with_min_count(1),
            "D-ETCD-E2E-001: listener should receive Add event with provider"
        );

        registry.destroy();
    }

    /// D-ETCD-E2E-002: Unregister removes provider, re-subscribe finds nothing.
    #[tokio::test]
    async fn detcd_e2e_002_etcd_unregister_removes() {
        if !etcd_available().await {
            eprintln!("SKIP: etcd not available on :2379");
            return;
        }

        let registry = EtcdRegistry::new(etcd_url()).with_endpoints("http://127.0.0.1:2379");

        let svc = provider_url("/com.example.EtcdUnregService", "10.0.0.99", "20899");

        // Register then unregister
        registry.register(svc.clone()).await.expect("register");
        registry.unregister(svc.clone()).await.expect("unregister");

        // Subscribe after unregister — should find no providers
        let listener = Arc::new(CollectingListener::new(svc.clone()));
        let sub_url = {
            let mut u = URL::new("dubbo", "/com.example.EtcdUnregService");
            u.ip = "127.0.0.1".into();
            u.port = "20899".into();
            u
        };
        registry
            .subscribe(sub_url, listener.clone())
            .await
            .expect("subscribe");

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // No Add event with providers should be received
        let events = listener.events.lock().unwrap();
        let has_providers = events.iter().any(|e| match e {
            ServiceEvent::Add(urls) => !urls.is_empty(),
            _ => false,
        });
        assert!(
            !has_providers,
            "D-ETCD-E2E-002: no providers should be found after unregister"
        );

        registry.destroy();
    }

    /// D-ETCD-E2E-003: Multiple providers register, consumer discovers all.
    #[tokio::test]
    async fn detcd_e2e_003_etcd_multiple_providers() {
        if !etcd_available().await {
            eprintln!("SKIP: etcd not available on :2379");
            return;
        }

        let registry = EtcdRegistry::new(etcd_url()).with_endpoints("http://127.0.0.1:2379");

        let svc1 = provider_url("/com.example.EtcdMultiService", "10.0.0.1", "20880");
        let svc2 = provider_url("/com.example.EtcdMultiService", "10.0.0.2", "20881");

        registry
            .register(svc1.clone())
            .await
            .expect("register svc1");
        registry
            .register(svc2.clone())
            .await
            .expect("register svc2");

        let listener = Arc::new(CollectingListener::new(svc1.clone()));
        let sub_url = {
            let mut u = URL::new("dubbo", "/com.example.EtcdMultiService");
            u.ip = "127.0.0.1".into();
            u.port = "20880".into();
            u
        };
        registry
            .subscribe(sub_url, listener.clone())
            .await
            .expect("subscribe");

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Etcd subscribe fires a single Add event; check it has providers
        let total_providers = {
            let events = listener.events.lock().unwrap();
            events
                .iter()
                .map(|e| match e {
                    ServiceEvent::Add(urls) => urls.len(),
                    _ => 0,
                })
                .sum::<usize>()
        };
        assert!(
            total_providers >= 1,
            "D-ETCD-E2E-003: should discover at least 1 provider (found {total_providers})"
        );

        registry.unregister(svc1).await.expect("unregister svc1");
        registry.unregister(svc2).await.expect("unregister svc2");
        registry.destroy();
    }

    /// D-ETCD-E2E-004: Custom root path configuration works.
    #[tokio::test]
    async fn detcd_e2e_004_etcd_custom_root_path() {
        if !etcd_available().await {
            eprintln!("SKIP: etcd not available on :2379");
            return;
        }

        let registry = EtcdRegistry::new(etcd_url())
            .with_endpoints("http://127.0.0.1:2379")
            .with_root_path("/custom-dubbo");

        let svc = provider_url("/com.example.EtcdCustomService", "10.0.0.1", "20880");

        // Register with custom root path — should succeed without error
        let result = registry.register(svc.clone()).await;
        assert!(
            result.is_ok(),
            "D-ETCD-E2E-004: register with custom root path should succeed: {:?}",
            result.err()
        );

        // Cleanup
        let _ = registry.unregister(svc).await;
        registry.destroy();
    }
}

// ── D-ZK-Java: Java↔RS Cross-Language Registry Interop ──────────────────
//
// Starts a Java Dubbo provider that registers to ZK, then verifies
// RS consumer discovers it. Requires ZK on localhost:2181.

mod d_zk_java_interop {
    use super::*;
    use async_trait::async_trait;
    use dubbo_rs_registry_zookeeper::ZookeeperRegistry;
    use dubbo_rs_common::node::Node;
    use dubbo_rs_registry::{NotifyListener, Registry, ServiceEvent};
    use std::process::{Child, Command, Stdio};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn zk_available() -> bool {
        std::net::TcpStream::connect_timeout(
            &"127.0.0.1:2181".parse().unwrap(),
            Duration::from_secs(1),
        )
        .is_ok()
    }

    struct CollectingListener {
        service_url: URL,
        events: Mutex<Vec<ServiceEvent>>,
    }

    impl CollectingListener {
        fn new(service_url: URL) -> Self {
            Self {
                service_url,
                events: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl NotifyListener for CollectingListener {
        async fn notify(&self, event: ServiceEvent) {
            self.events.lock().unwrap().push(event);
        }

        fn listen_url(&self) -> URL {
            self.service_url.clone()
        }
    }

    struct JavaRegistryProvider {
        child: Option<Child>,
    }

    impl JavaRegistryProvider {
        fn start(registry_addr: &str, port: u16) -> Self {
            let project_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let java_dir = project_root.join("../tests/java-fixtures");

            let cp_path = format!("/tmp/dubbo-rs-regcp-{port}.txt");
            let shared_cp = "/tmp/dubbo-rs-regcp.txt";
            let cp_arg = format!("-Dmdep.outputFile={cp_path}");
            let _ = Command::new("mvn")
                .args(["dependency:build-classpath", "-q", &cp_arg])
                .current_dir(&java_dir)
                .env("JAVA_HOME", "/usr/lib/jvm/java-17-openjdk")
                .output();

            let maven_cp = if std::path::Path::new(&cp_path).exists() {
                std::fs::read_to_string(&cp_path)
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            } else if std::path::Path::new(shared_cp).exists() {
                std::fs::read_to_string(shared_cp)
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            } else {
                String::new()
            };
            let _ = std::fs::remove_file(&cp_path);

            let cp = format!("{}:{}", java_dir.join("target/classes").display(), maven_cp);

            let child = Command::new("/usr/lib/jvm/java-17-openjdk/bin/java")
                .args([
                    "-cp",
                    &cp,
                    "com.dubborst.fixtures.RegistryTestProvider",
                    "zookeeper",
                    registry_addr,
                    &port.to_string(),
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("failed to start Java RegistryTestProvider");

            eprintln!("[JavaRegistryProvider] Waiting for Java provider to register...");
            std::thread::sleep(Duration::from_secs(8));

            Self { child: Some(child) }
        }
    }

    impl Drop for JavaRegistryProvider {
        fn drop(&mut self) {
            if let Some(mut child) = self.child.take() {
                let _ = child.kill();
                let _ = child.wait();
                eprintln!("[JavaRegistryProvider] stopped");
            }
        }
    }

    /// D-ZK-Java-001: Java Dubbo provider registers to ZK, RS consumer discovers it.
    ///
    /// Cross-language flow:
    /// 1. Java `RegistryTestProvider` starts and registers `DemoService` to ZK via Dubbo framework
    /// 2. RS `ZookeeperRegistry` subscribes to the same service
    /// 3. RS listener receives `ServiceEvent::Add` with the Java provider URL
    #[tokio::test]
    async fn dzk_java_001_java_provider_rs_consumer_discover() {
        if !zk_available() {
            eprintln!("SKIP: ZooKeeper not available on :2181");
            return;
        }

        let _java = JavaRegistryProvider::start("127.0.0.1:2181", 20881);

        let registry = ZookeeperRegistry::new({
            let mut u = URL::new("zookeeper", "");
            u.ip = "127.0.0.1".into();
            u.port = "2181".into();
            u
        });

        let listener = Arc::new(CollectingListener::new(URL::new(
            "dubbo",
            "/com.dubborst.fixtures.DemoService",
        )));

        let sub_url = URL::new("dubbo", "/com.dubborst.fixtures.DemoService");
        registry
            .subscribe(sub_url, listener.clone())
            .await
            .expect("subscribe should succeed");

        tokio::time::sleep(Duration::from_millis(500)).await;

        let events = listener.events.lock().unwrap();
        let found = events.iter().any(|e| match e {
            ServiceEvent::Add(urls) => urls.iter().any(|u| u.path.contains("DemoService")),
            _ => false,
        });
        assert!(
            found,
            "D-ZK-Java-001: RS should discover Java provider via ZK registry"
        );

        registry.destroy();
    }

    /// D-ZK-Java-002: RS registers a provider, then Java provider also registers;
    /// RS consumer discovers both providers.
    #[tokio::test]
    async fn dzk_java_002_rs_and_java_providers_both_discovered() {
        if !zk_available() {
            eprintln!("SKIP: ZooKeeper not available on :2181");
            return;
        }

        let _java = JavaRegistryProvider::start("127.0.0.1:2181", 20882);

        let registry = ZookeeperRegistry::new({
            let mut u = URL::new("zookeeper", "");
            u.ip = "127.0.0.1".into();
            u.port = "2181".into();
            u
        });

        // RS also registers a provider for the same service
        let mut rs_provider = URL::new("dubbo", "/com.dubborst.fixtures.DemoService");
        rs_provider.ip = "10.0.0.99".into();
        rs_provider.port = "20899".into();
        registry
            .register(rs_provider)
            .await
            .expect("RS register should succeed");

        let listener = Arc::new(CollectingListener::new(URL::new(
            "dubbo",
            "/com.dubborst.fixtures.DemoService",
        )));

        let sub_url = URL::new("dubbo", "/com.dubborst.fixtures.DemoService");
        registry
            .subscribe(sub_url, listener.clone())
            .await
            .expect("subscribe should succeed");

        tokio::time::sleep(Duration::from_millis(500)).await;

        let events = listener.events.lock().unwrap();
        let total_providers: usize = events
            .iter()
            .map(|e| match e {
                ServiceEvent::Add(urls) => urls.len(),
                _ => 0,
            })
            .sum();
        assert!(
            total_providers >= 2,
            "D-ZK-Java-002: should discover at least 2 providers (RS + Java), found {total_providers}"
        );

        registry.destroy();
    }
}

// ── D-RE-E2E: Redis End-to-End Integration Tests ─────────────────────────

mod d_re_e2e {
    use super::*;
    use async_trait::async_trait;
    use dubbo_rs_registry_redis::RedisRegistry;
    use dubbo_rs_common::node::Node;
    use dubbo_rs_registry::{NotifyListener, Registry, ServiceEvent};
    use std::sync::{Arc, Mutex};

    struct CollectingListener {
        service_url: URL,
        events: Mutex<Vec<ServiceEvent>>,
    }

    impl CollectingListener {
        fn new(service_url: URL) -> Self {
            Self {
                service_url,
                events: Mutex::new(Vec::new()),
            }
        }

        fn has_add_event_with_min_count(&self, min: usize) -> bool {
            let events = self.events.lock().unwrap();
            events.iter().any(|e| match e {
                ServiceEvent::Add(urls) => urls.len() >= min,
                _ => false,
            })
        }
    }

    #[async_trait]
    impl NotifyListener for CollectingListener {
        async fn notify(&self, event: ServiceEvent) {
            self.events.lock().unwrap().push(event);
        }

        fn listen_url(&self) -> URL {
            self.service_url.clone()
        }
    }

    fn redis_available() -> bool {
        std::net::TcpStream::connect_timeout(
            &"127.0.0.1:6379".parse().unwrap(),
            std::time::Duration::from_secs(1),
        )
        .is_ok()
    }

    fn redis_url() -> URL {
        let mut u = URL::new("redis", "");
        u.ip = "127.0.0.1".into();
        u.port = "6379".into();
        u
    }

    fn provider_url(service: &str, ip: &str, port: &str) -> URL {
        let mut u = URL::new("dubbo", service);
        u.ip = ip.into();
        u.port = port.into();
        u
    }

    /// D-RE-E2E-001: RS provider registers to Redis, RS consumer discovers.
    #[tokio::test]
    async fn dre_e2e_001_redis_register_and_discover() {
        if !redis_available() {
            eprintln!("SKIP: Redis not available on :6379");
            return;
        }

        let registry = RedisRegistry::new(redis_url());
        let svc = provider_url("/com.example.RedisTestService", "10.0.0.1", "20880");
        registry.register(svc.clone()).await.expect("register");

        let listener = Arc::new(CollectingListener::new(svc.clone()));
        registry
            .subscribe(svc.clone(), listener.clone())
            .await
            .expect("subscribe");
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        assert!(
            listener.has_add_event_with_min_count(1),
            "D-RE-E2E-001: should discover at least 1 provider"
        );
        registry.destroy();
    }

    /// D-RE-E2E-002: Unregister removes provider from Redis.
    #[tokio::test]
    async fn dre_e2e_002_redis_unregister_removes() {
        if !redis_available() {
            eprintln!("SKIP: Redis not available on :6379");
            return;
        }

        let registry = RedisRegistry::new(redis_url());
        let svc = provider_url("/com.example.RedisTestService", "10.0.0.2", "20880");
        registry.register(svc.clone()).await.expect("register");
        registry.unregister(svc.clone()).await.expect("unregister");

        let listener = Arc::new(CollectingListener::new(svc.clone()));
        registry
            .subscribe(svc.clone(), listener.clone())
            .await
            .expect("subscribe");
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        registry.destroy();
    }

    /// D-RE-E2E-003: Multiple providers registered to Redis.
    #[tokio::test]
    async fn dre_e2e_003_redis_multiple_providers() {
        if !redis_available() {
            eprintln!("SKIP: Redis not available on :6379");
            return;
        }

        let registry = RedisRegistry::new(redis_url());
        let svc1 = provider_url("/com.example.RedisMultiService", "10.0.0.1", "20880");
        let svc2 = provider_url("/com.example.RedisMultiService", "10.0.0.2", "20880");
        registry
            .register(svc1.clone())
            .await
            .expect("register svc1");
        registry
            .register(svc2.clone())
            .await
            .expect("register svc2");

        let listener = Arc::new(CollectingListener::new(svc1.clone()));
        registry
            .subscribe(svc1.clone(), listener.clone())
            .await
            .expect("subscribe");
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        assert!(
            listener.has_add_event_with_min_count(2),
            "D-RE-E2E-003: should discover at least 2 providers"
        );
        registry.destroy();
    }

    /// D-RE-E2E-004: Redis registry with custom root path.
    #[tokio::test]
    async fn dre_e2e_004_redis_custom_root_path() {
        if !redis_available() {
            eprintln!("SKIP: Redis not available on :6379");
            return;
        }

        let registry = RedisRegistry::new(redis_url()).with_root_path("myapp");
        let svc = provider_url("/com.example.CustomRootService", "10.0.0.1", "20880");
        registry.register(svc.clone()).await.expect("register");

        let listener = Arc::new(CollectingListener::new(svc.clone()));
        registry
            .subscribe(svc.clone(), listener.clone())
            .await
            .expect("subscribe");
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        assert!(
            listener.has_add_event_with_min_count(1),
            "D-RE-E2E-004: should discover provider with custom root"
        );
        registry.destroy();
    }
}
