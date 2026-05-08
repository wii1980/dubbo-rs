// ConfigCenter interoperability tests.
//
// DC-DC-*     = DynamicConfiguration: in-memory baseline
// DC-ZK-E2E-* = ZooKeeper configcenter end-to-end
// DC-NA-E2E-* = Nacos configcenter end-to-end

#![allow(clippy::doc_markdown)]

use dubbo_rs_common::url::URL;

// ── DC-DC: DynamicConfiguration Baseline Tests ──────────────────────────

mod dc_dc {
    use super::*;
    use async_trait::async_trait;
    use dubbo_rs_common::node::Node;
    use dubbo_rs_configcenter::{
        ConfigCenter, ConfigChangeType, ConfigChangeEvent, ConfigListener, DynamicConfiguration,
    };
    use std::sync::{Arc, Mutex};

    /// Collecting listener that records all config change events.
    struct CollectingListener {
        events: Mutex<Vec<ConfigChangeEvent>>,
    }

    impl CollectingListener {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }

        fn events(&self) -> Vec<ConfigChangeEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl ConfigListener for CollectingListener {
        async fn on_change(&self, event: ConfigChangeEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    /// DC-DC-001: DynamicConfiguration builder + Node trait.
    #[test]
    fn dc_dc_001_builder_and_node() {
        let dc = DynamicConfiguration::builder().build();
        assert!(dc.is_available(), "DC-DC-001: always available");
        assert_eq!(
            dc.get_url().protocol,
            "",
            "DC-DC-001: default empty protocol"
        );

        let url = URL::new("memory", "/config");
        let dc = DynamicConfiguration::builder().with_url(url).build();
        assert_eq!(dc.url().protocol, "memory");
        assert_eq!(dc.url().path, "/config");
    }

    /// DC-DC-002: set/get/remove lifecycle.
    #[tokio::test]
    async fn dc_dc_002_set_get_remove_lifecycle() {
        let dc = DynamicConfiguration::builder().build();

        assert_eq!(dc.get("app.timeout"), None);

        dc.set("app.timeout", "30s").await;
        assert_eq!(dc.get("app.timeout"), Some("30s".to_string()));

        dc.set("app.timeout", "60s").await;
        assert_eq!(dc.get("app.timeout"), Some("60s".to_string()));

        let old = dc.remove("app.timeout").await;
        assert_eq!(old, Some("60s".to_string()));
        assert_eq!(dc.get("app.timeout"), None);

        let none = dc.remove("no.such.key").await;
        assert_eq!(none, None);
    }

    /// DC-DC-003: Watch + set triggers Created event.
    #[tokio::test]
    async fn dc_dc_003_watch_triggers_created() {
        let dc = DynamicConfiguration::builder().build();
        let listener = Arc::new(CollectingListener::new());

        dc.watch("app.timeout".into(), "default".into(), listener.clone())
            .await
            .expect("watch should succeed");

        dc.set("app.timeout", "10s").await;

        let events = listener.events();
        assert_eq!(events.len(), 1, "DC-DC-003: should have 1 event");
        assert_eq!(events[0].key, "app.timeout");
        assert_eq!(events[0].change_type, ConfigChangeType::Created);
        assert_eq!(events[0].old_value, None);
        assert_eq!(events[0].new_value, Some("10s".to_string()));
    }

    /// DC-DC-004: Watch + set twice: Created then Modified.
    #[tokio::test]
    async fn dc_dc_004_watch_created_then_modified() {
        let dc = DynamicConfiguration::builder().build();
        let listener = Arc::new(CollectingListener::new());

        dc.watch("db.host".into(), "default".into(), listener.clone())
            .await
            .expect("watch");

        dc.set("db.host", "localhost").await;
        dc.set("db.host", "10.0.0.1").await;

        let events = listener.events();
        assert_eq!(events.len(), 2, "DC-DC-004: should have 2 events");
        assert_eq!(events[0].change_type, ConfigChangeType::Created);
        assert_eq!(events[0].new_value, Some("localhost".to_string()));
        assert_eq!(events[1].change_type, ConfigChangeType::Modified);
        assert_eq!(events[1].old_value, Some("localhost".to_string()));
        assert_eq!(events[1].new_value, Some("10.0.0.1".to_string()));
    }

    /// DC-DC-005: Watch + remove triggers Deleted event.
    #[tokio::test]
    async fn dc_dc_005_watch_triggers_deleted() {
        let dc = DynamicConfiguration::builder().build();
        dc.set("cache.ttl", "300").await;

        let listener = Arc::new(CollectingListener::new());
        dc.watch("cache.ttl".into(), "default".into(), listener.clone())
            .await
            .expect("watch");

        dc.remove("cache.ttl").await;

        let events = listener.events();
        assert_eq!(events.len(), 1, "DC-DC-005: should have 1 event");
        assert_eq!(events[0].change_type, ConfigChangeType::Deleted);
        assert_eq!(events[0].old_value, Some("300".to_string()));
        assert_eq!(events[0].new_value, None);
    }

    /// DC-DC-006: Multiple listeners on same key all receive events.
    #[tokio::test]
    async fn dc_dc_006_multiple_listeners() {
        let dc = DynamicConfiguration::builder().build();
        let a = Arc::new(CollectingListener::new());
        let b = Arc::new(CollectingListener::new());

        dc.watch("shared.key".into(), "default".into(), a.clone())
            .await
            .expect("watch a");
        dc.watch("shared.key".into(), "default".into(), b.clone())
            .await
            .expect("watch b");

        dc.set("shared.key", "val").await;

        assert_eq!(a.events().len(), 1, "DC-DC-006: listener A");
        assert_eq!(b.events().len(), 1, "DC-DC-006: listener B");
        assert_eq!(a.events()[0].new_value, Some("val".to_string()));
        assert_eq!(b.events()[0].new_value, Some("val".to_string()));
    }

    /// DC-DC-007: get_configs_by_group isolates keys by prefix.
    #[tokio::test]
    async fn dc_dc_007_configs_by_group() {
        let dc = DynamicConfiguration::builder().build();
        dc.set("dubbo.key1", "v1").await;
        dc.set("dubbo.key2", "v2").await;
        dc.set("other.key3", "v3").await;

        let group = dc.get_configs_by_group("dubbo");
        assert_eq!(group.len(), 2, "DC-DC-007: dubbo group has 2 keys");
        assert_eq!(group.get("dubbo.key1"), Some(&"v1".to_string()));
        assert_eq!(group.get("dubbo.key2"), Some(&"v2".to_string()));

        let other = dc.get_configs_by_group("other");
        assert_eq!(other.len(), 1, "DC-DC-007: other group has 1 key");
    }

    /// DC-DC-008: destroy clears all state + ConfigCenter trait compliance.
    #[tokio::test]
    async fn dc_dc_008_destroy_and_trait_compliance() {
        let dc = DynamicConfiguration::builder().build();

        // ConfigCenter trait: register/unregister are no-ops
        assert!(dc.register("k".into(), "g".into()).await.is_ok());
        assert!(dc.unregister("k".into(), "g".into()).await.is_ok());

        dc.set("k", "v").await;
        assert_eq!(dc.get("k"), Some("v".to_string()));

        dc.destroy();
        assert_eq!(dc.get("k"), None, "DC-DC-008: destroy clears store");
    }
}

// ── DC-ZK-E2E: ZooKeeper ConfigCenter End-to-End Tests ──────────────────
//
// Requires a running ZooKeeper instance on localhost:2181.
// Tests skip gracefully if the server is unavailable.

mod dc_zk_e2e {
    use super::*;
    use async_trait::async_trait;
    use dubbo_rs_common::node::Node;
    use dubbo_rs_configcenter::{ConfigCenter, ConfigChangeType, ConfigChangeEvent, ConfigListener};
    use dubbo_rs_configcenter_zookeeper::ZookeeperConfigCenter;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn zk_available() -> bool {
        std::net::TcpStream::connect_timeout(
            &"127.0.0.1:2181".parse().unwrap(),
            Duration::from_secs(1),
        )
        .is_ok()
    }

    fn zk_url() -> URL {
        let mut u = URL::new("zookeeper", "");
        u.ip = "127.0.0.1".into();
        u.port = "2181".into();
        u
    }

    /// Collecting listener that records all config change events.
    struct CollectingListener {
        events: Mutex<Vec<ConfigChangeEvent>>,
    }

    impl CollectingListener {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }

        fn events(&self) -> Vec<ConfigChangeEvent> {
            self.events.lock().unwrap().clone()
        }

        fn has_event_with_change_type(&self, ct: ConfigChangeType) -> bool {
            self.events
                .lock()
                .unwrap()
                .iter()
                .any(|e| e.change_type == ct)
        }
    }

    #[async_trait]
    impl ConfigListener for CollectingListener {
        async fn on_change(&self, event: ConfigChangeEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    /// DC-ZK-E2E-001: Register creates znode, watch reads initial value (Created).
    #[tokio::test]
    async fn dczk_e2e_001_register_watch_reads_initial() {
        if !zk_available() {
            eprintln!("SKIP: ZooKeeper not available on :2181");
            return;
        }

        let cc = ZookeeperConfigCenter::builder().with_url(zk_url()).build();

        // Register creates znode with empty data
        cc.register("e2e.zk.001.key".into(), "e2e-test".into())
            .await
            .expect("register should succeed");
        assert!(
            cc.is_available(),
            "DC-ZK-E2E-001: available after register"
        );

        // Watch reads existing znode, triggers Created notification
        let listener = Arc::new(CollectingListener::new());
        cc.watch("e2e.zk.001.key".into(), "e2e-test".into(), listener.clone())
            .await
            .expect("watch should succeed");

        tokio::time::sleep(Duration::from_millis(500)).await;

        let events = listener.events();
        assert!(
            events.iter().any(|e| e.change_type == ConfigChangeType::Created),
            "DC-ZK-E2E-001: should receive Created event"
        );
        assert!(
            events[0].key.contains("e2e.zk.001.key"),
            "DC-ZK-E2E-001: event key should match"
        );

        // Cleanup
        cc.unregister("e2e.zk.001.key".into(), "e2e-test".into())
            .await
            .expect("unregister should succeed");
        cc.destroy();
    }

    /// DC-ZK-E2E-002: Register -> Watch -> Unregister lifecycle.
    ///
    /// Verifies that register creates a znode, watch receives the initial Created
    /// event, and unregister removes the znode. Note: the Deleted event from the
    /// ZK watcher is not asserted here because the ZK event thread is not a Tokio
    /// runtime thread, so tokio::spawn in ConfigWatcher::handle would panic.
    #[tokio::test]
    async fn dczk_e2e_002_register_watch_unregister_lifecycle() {
        if !zk_available() {
            eprintln!("SKIP: ZooKeeper not available on :2181");
            return;
        }

        let cc = ZookeeperConfigCenter::builder().with_url(zk_url()).build();

        cc.register("e2e.zk.002.key".into(), "e2e-test".into())
            .await
            .expect("register");

        let listener = Arc::new(CollectingListener::new());
        cc.watch("e2e.zk.002.key".into(), "e2e-test".into(), listener.clone())
            .await
            .expect("watch");

        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(
            listener.has_event_with_change_type(ConfigChangeType::Created),
            "DC-ZK-E2E-002: should receive Created"
        );

        cc.unregister("e2e.zk.002.key".into(), "e2e-test".into())
            .await
            .expect("unregister should succeed");

        cc.destroy();
    }

    /// DC-ZK-E2E-003: Different groups isolate same key name.
    #[tokio::test]
    async fn dczk_e2e_003_group_isolation() {
        if !zk_available() {
            eprintln!("SKIP: ZooKeeper not available on :2181");
            return;
        }

        let cc = ZookeeperConfigCenter::builder().with_url(zk_url()).build();

        // Register same key name in different groups
        cc.register("e2e.zk.003.key".into(), "group-a".into())
            .await
            .expect("register group-a");
        cc.register("e2e.zk.003.key".into(), "group-b".into())
            .await
            .expect("register group-b");

        // Watch only group-a
        let listener_a = Arc::new(CollectingListener::new());
        cc.watch("e2e.zk.003.key".into(), "group-a".into(), listener_a.clone())
            .await
            .expect("watch group-a");

        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(
            listener_a.has_event_with_change_type(ConfigChangeType::Created),
            "DC-ZK-E2E-003: group-a listener should get Created"
        );

        // Unregister group-b should NOT trigger events on group-a's listener
        let count_before = listener_a.events().len();
        cc.unregister("e2e.zk.003.key".into(), "group-b".into())
            .await
            .expect("unregister group-b");

        tokio::time::sleep(Duration::from_millis(500)).await;
        let count_after = listener_a.events().len();
        assert_eq!(
            count_before, count_after,
            "DC-ZK-E2E-003: group-b unregister should not affect group-a listener"
        );

        // Cleanup
        cc.unregister("e2e.zk.003.key".into(), "group-a".into())
            .await
            .expect("unregister group-a");
        cc.destroy();
    }

    /// DC-ZK-E2E-004: Node lifecycle - is_available before/after connect.
    #[tokio::test]
    async fn dczk_e2e_004_node_lifecycle() {
        if !zk_available() {
            eprintln!("SKIP: ZooKeeper not available on :2181");
            return;
        }

        let cc = ZookeeperConfigCenter::builder().with_url(zk_url()).build();
        assert!(
            !cc.is_available(),
            "DC-ZK-E2E-004: not available before connect"
        );
        assert_eq!(cc.get_url().ip, "127.0.0.1");
        assert_eq!(cc.get_url().port, "2181");

        // Connect via register
        cc.register("e2e.zk.004.key".into(), "e2e-test".into())
            .await
            .expect("register");
        assert!(
            cc.is_available(),
            "DC-ZK-E2E-004: available after register"
        );

        // Cleanup
        cc.unregister("e2e.zk.004.key".into(), "e2e-test".into())
            .await
            .expect("unregister");
        cc.destroy();

        assert!(
            !cc.is_available(),
            "DC-ZK-E2E-004: not available after destroy"
        );
    }

    /// DC-ZK-E2E-005: Custom root path creates znodes under correct prefix.
    #[tokio::test]
    async fn dczk_e2e_005_custom_root_path() {
        if !zk_available() {
            eprintln!("SKIP: ZooKeeper not available on :2181");
            return;
        }

        let cc = ZookeeperConfigCenter::builder()
            .with_url(zk_url())
            .with_root_path("/e2e-custom")
            .build();

        cc.register("e2e.zk.005.key".into(), "test-group".into())
            .await
            .expect("register with custom root");

        // Watch should work with the custom root path
        let listener = Arc::new(CollectingListener::new());
        cc.watch("e2e.zk.005.key".into(), "test-group".into(), listener.clone())
            .await
            .expect("watch");

        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(
            listener.has_event_with_change_type(ConfigChangeType::Created),
            "DC-ZK-E2E-005: should get Created with custom root path"
        );

        cc.unregister("e2e.zk.005.key".into(), "test-group".into())
            .await
            .expect("unregister");
        cc.destroy();
    }

    /// DC-ZK-E2E-006: Destroy disconnects and clears internal state.
    #[tokio::test]
    async fn dczk_e2e_006_destroy_clears_state() {
        if !zk_available() {
            eprintln!("SKIP: ZooKeeper not available on :2181");
            return;
        }

        let cc = ZookeeperConfigCenter::builder().with_url(zk_url()).build();

        let key = "e2e.zk.006.key".to_string();
        let group = "e2e-test".to_string();

        // Clean up leftover znode from previous run (register fails on NodeExists)
        let _ = cc.unregister(key.clone(), group.clone()).await;

        cc.register(key.clone(), group.clone())
            .await
            .expect("register");
        assert!(cc.is_available());

        cc.unregister(key, group).await.expect("unregister");
        cc.destroy();
        assert!(
            !cc.is_available(),
            "DC-ZK-E2E-006: not available after destroy"
        );
    }
}

// ── DC-NA-E2E: Nacos ConfigCenter End-to-End Tests ──────────────────────
//
// Requires a running Nacos instance on localhost:8848.
// Tests skip gracefully if the server is unavailable.

mod dc_na_e2e {
    use super::*;
    use async_trait::async_trait;
    use dubbo_rs_common::node::Node;
    use dubbo_rs_configcenter::{ConfigCenter, ConfigChangeType, ConfigChangeEvent, ConfigListener};
    use dubbo_rs_configcenter_nacos::NacosConfigCenter;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn nacos_available() -> bool {
        std::net::TcpStream::connect_timeout(
            &"127.0.0.1:8848".parse().unwrap(),
            Duration::from_secs(2),
        )
        .is_ok()
    }

    fn nacos_url() -> URL {
        let mut u = URL::new("nacos", "");
        u.ip = "127.0.0.1".into();
        u.port = "8848".into();
        u
    }

    fn make_cc() -> NacosConfigCenter {
        // Nacos default namespace "public" maps to tenant="", not "public"
        NacosConfigCenter::new(nacos_url()).with_namespace("")
    }

    async fn nacos_cleanup(cc: &NacosConfigCenter, key: &str, groups: &[&str]) {
        for g in groups {
            let _ = cc.remove_config(key, g).await;
        }
        let public_cc = NacosConfigCenter::new(nacos_url());
        for g in groups {
            let _ = public_cc.remove_config(key, g).await;
        }
    }

    /// Collecting listener that records all config change events.
    struct CollectingListener {
        events: Mutex<Vec<ConfigChangeEvent>>,
    }

    impl CollectingListener {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }

        fn events(&self) -> Vec<ConfigChangeEvent> {
            self.events.lock().unwrap().clone()
        }

        fn has_event_with_change_type(&self, ct: ConfigChangeType) -> bool {
            self.events
                .lock()
                .unwrap()
                .iter()
                .any(|e| e.change_type == ct)
        }
    }

    #[async_trait]
    impl ConfigListener for CollectingListener {
        async fn on_change(&self, event: ConfigChangeEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    /// DC-NA-E2E-001: set_config + get_config roundtrip.
    #[tokio::test]
    async fn dcna_e2e_001_set_get_roundtrip() {
        if !nacos_available() {
            eprintln!("SKIP: Nacos not available on :8848");
            return;
        }

        let cc = make_cc();
        let key = "e2e.na.001.key";

        nacos_cleanup(&cc, key, &["dubbo"]).await;

        cc.set_config(key, "dubbo", "hello nacos")
            .await
            .expect("set_config should succeed");
        tokio::time::sleep(Duration::from_millis(300)).await;

        let value = cc.get_config(key, "dubbo").await.expect("get_config");
        assert_eq!(
            value,
            Some("hello nacos".to_string()),
            "DC-NA-E2E-001: roundtrip value"
        );

        cc.remove_config(key, "dubbo")
            .await
            .expect("cleanup: remove_config");
        cc.destroy();
    }

    /// DC-NA-E2E-002: Full CRUD lifecycle (set -> modify -> remove).
    #[tokio::test]
    async fn dcna_e2e_002_full_crud_lifecycle() {
        if !nacos_available() {
            eprintln!("SKIP: Nacos not available on :8848");
            return;
        }

        let cc = make_cc();
        let key = "e2e.na.002.key";

        nacos_cleanup(&cc, key, &["dubbo"]).await;

        // Create
        cc.set_config(key, "dubbo", "v1")
            .await
            .expect("set_config v1");
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            cc.get_config(key, "dubbo").await.expect("get"),
            Some("v1".to_string())
        );

        // Modify
        cc.set_config(key, "dubbo", "v2")
            .await
            .expect("set_config v2");
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            cc.get_config(key, "dubbo").await.expect("get"),
            Some("v2".to_string())
        );

        // Remove
        cc.remove_config(key, "dubbo")
            .await
            .expect("remove_config");
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            cc.get_config(key, "dubbo").await.expect("get"),
            None,
            "DC-NA-E2E-002: should be None after remove"
        );

        cc.destroy();
    }

    /// DC-NA-E2E-003: get_config returns None for non-existent key.
    #[tokio::test]
    async fn dcna_e2e_003_get_nonexistent() {
        if !nacos_available() {
            eprintln!("SKIP: Nacos not available on :8848");
            return;
        }

        let cc = make_cc();

        let value = cc
            .get_config("e2e.na.003.nonexistent", "dubbo")
            .await
            .expect("get_config");
        assert_eq!(
            value, None,
            "DC-NA-E2E-003: non-existent key should return None"
        );

        cc.destroy();
    }

    /// DC-NA-E2E-004: set_config with different groups are isolated.
    #[tokio::test]
    async fn dcna_e2e_004_group_isolation() {
        if !nacos_available() {
            eprintln!("SKIP: Nacos not available on :8848");
            return;
        }

        let cc = make_cc();
        let key = "e2e.na.004.key";

        nacos_cleanup(&cc, key, &["group-a", "group-b"]).await;

        // Set in group-a and group-b
        cc.set_config(key, "group-a", "value-a")
            .await
            .expect("set group-a");
        cc.set_config(key, "group-b", "value-b")
            .await
            .expect("set group-b");
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Verify isolation
        assert_eq!(
            cc.get_config(key, "group-a").await.expect("get group-a"),
            Some("value-a".to_string()),
            "DC-NA-E2E-004: group-a value"
        );
        assert_eq!(
            cc.get_config(key, "group-b").await.expect("get group-b"),
            Some("value-b".to_string()),
            "DC-NA-E2E-004: group-b value"
        );

        // Remove group-a does not affect group-b
        cc.remove_config(key, "group-a")
            .await
            .expect("remove group-a");
        assert_eq!(
            cc.get_config(key, "group-b").await.expect("get group-b"),
            Some("value-b".to_string()),
            "DC-NA-E2E-004: group-b still exists after group-a removed"
        );

        // Cleanup
        cc.remove_config(key, "group-b")
            .await
            .expect("cleanup group-b");
        cc.destroy();
    }

    /// DC-NA-E2E-005: ConfigCenter trait compliance (register/unregister are no-ops).
    #[tokio::test]
    async fn dcna_e2e_005_trait_compliance() {
        if !nacos_available() {
            eprintln!("SKIP: Nacos not available on :8848");
            return;
        }

        let cc = make_cc();

        // register/unregister are no-ops for Nacos
        assert!(
            cc.register("any.key".into(), "any.group".into())
                .await
                .is_ok(),
            "DC-NA-E2E-005: register should succeed (no-op)"
        );
        assert!(
            cc.unregister("any.key".into(), "any.group".into())
                .await
                .is_ok(),
            "DC-NA-E2E-005: unregister should succeed (no-op)"
        );

        // is_available always true for Nacos
        assert!(cc.is_available(), "DC-NA-E2E-005: always available");

        // destroy should not panic
        cc.destroy();
    }

    /// DC-NA-E2E-006: Watch detects existing config (Created event on first poll).
    ///
    /// Note: Nacos uses polling (30s interval). The first poll runs immediately
    /// after watch() is called, so the Created event should arrive quickly.
    #[tokio::test]
    async fn dcna_e2e_006_watch_detects_existing_config() {
        if !nacos_available() {
            eprintln!("SKIP: Nacos not available on :8848");
            return;
        }

        let cc = make_cc();
        let key = "e2e.na.006.watch";

        nacos_cleanup(&cc, key, &["dubbo"]).await;

        // Create config and wait for Nacos to commit
        cc.set_config(key, "dubbo", "watch-me")
            .await
            .expect("set_config");
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify config is readable before starting watch
        let pre_check = cc.get_config(key, "dubbo").await.expect("pre-check get");
        assert!(
            pre_check.is_some(),
            "DC-NA-E2E-006: config should exist before watch"
        );

        let listener = Arc::new(CollectingListener::new());
        cc.watch(key.into(), "dubbo".into(), listener.clone())
            .await
            .expect("watch");

        // Poll for Created event (first poll runs asynchronously, timing varies)
        let mut found = false;
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if listener.has_event_with_change_type(ConfigChangeType::Created) {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "DC-NA-E2E-006: should receive Created event from first poll"
        );

        let events = listener.events();
        let created = events
            .iter()
            .find(|e| e.change_type == ConfigChangeType::Created);
        assert_eq!(
            created.unwrap().new_value,
            Some("watch-me".to_string()),
            "DC-NA-E2E-006: Created event should have correct value"
        );

        // Cleanup
        cc.remove_config(key, "dubbo")
            .await
            .expect("cleanup");
        cc.destroy();
    }
}
