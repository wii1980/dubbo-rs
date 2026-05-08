use dubbo_rs_common::url::URL;
use dubbo_rs_configcenter_zookeeper::ZookeeperConfigCenter;
use dubbo_rs_common::node::Node;

fn make_zk_url() -> URL {
    let mut url = URL::new("zookeeper", "/dubbo/config");
    url.ip = "127.0.0.1".into();
    url.port = "2181".into();
    url
}

#[test]
fn test_builder_defaults_from_external() {
    let cc = ZookeeperConfigCenter::builder().build();
    assert!(!cc.is_available());
    assert!(cc.get_url().protocol.is_empty());
}

#[test]
fn test_builder_with_url_reuses_url() {
    let url = make_zk_url();
    let cc = ZookeeperConfigCenter::builder()
        .with_url(url.clone())
        .build();
    assert_eq!(cc.get_url(), &url);
}

#[test]
fn test_builder_custom_root_path() {
    let cc = ZookeeperConfigCenter::builder()
        .with_url(make_zk_url())
        .with_root_path("/myapp")
        .build();
    assert_eq!(cc.get_url().ip, "127.0.0.1");
    assert_eq!(cc.get_url().port, "2181");
}

#[test]
fn test_destroy_on_unconnected_instance() {
    let cc = ZookeeperConfigCenter::builder()
        .with_url(make_zk_url())
        .build();
    assert!(!cc.is_available());
    cc.destroy();
    assert!(!cc.is_available());
}

#[test]
fn test_get_url_identity() {
    let mut url = URL::new("zookeeper", "/myapp");
    url.ip = "10.0.0.1".into();
    url.port = "2189".into();
    let cc = ZookeeperConfigCenter::builder()
        .with_url(url.clone())
        .with_root_path("/custom")
        .with_session_timeout(std::time::Duration::from_secs(30))
        .build();
    assert_eq!(cc.get_url(), &url);
    assert_eq!(cc.get_url().protocol, "zookeeper");
    assert_eq!(cc.get_url().ip, "10.0.0.1");
    assert_eq!(cc.get_url().port, "2189");
}

#[test]
fn test_node_trait_implementation() {
    let cc = ZookeeperConfigCenter::builder()
        .with_url(make_zk_url())
        .build();
    assert!(!cc.is_available());
    assert_eq!(cc.get_url().ip, "127.0.0.1");
    assert_eq!(cc.get_url().port, "2181");
}
