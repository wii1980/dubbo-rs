use dubbo_rs_configcenter::ConfigCenter;
use dubbo_rs_common::node::Node;
use dubbo_rs_configcenter_nacos::NacosConfigCenter;

#[tokio::test]
async fn test_get_config_returns_value() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/nacos/v1/cs/configs")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("dataId".into(), "timeout".into()),
            mockito::Matcher::UrlEncoded("group".into(), "dubbo".into()),
            mockito::Matcher::UrlEncoded("tenant".into(), "public".into()),
        ]))
        .with_status(200)
        .with_body("30s")
        .create();

    let url = make_nacos_url_with_port(&server);
    let cc = NacosConfigCenter::new(url);

    let value = cc.get_config("timeout", "dubbo").await.expect("get_config should succeed");
    assert_eq!(value, Some("30s".to_string()));
    mock.assert();
}

#[tokio::test]
async fn test_get_config_returns_none_on_404() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/nacos/v1/cs/configs")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("dataId".into(), "missing-key".into()),
            mockito::Matcher::UrlEncoded("group".into(), "dubbo".into()),
            mockito::Matcher::UrlEncoded("tenant".into(), "public".into()),
        ]))
        .with_status(404)
        .create();

    let url = make_nacos_url_with_port(&server);
    let cc = NacosConfigCenter::new(url);

    let value = cc.get_config("missing-key", "dubbo").await.expect("get_config should succeed");
    assert_eq!(value, None);
    mock.assert();
}

#[tokio::test]
async fn test_get_config_returns_none_on_not_exist() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/nacos/v1/cs/configs")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("dataId".into(), "empty-key".into()),
            mockito::Matcher::UrlEncoded("group".into(), "dubbo".into()),
            mockito::Matcher::UrlEncoded("tenant".into(), "public".into()),
        ]))
        .with_status(200)
        .with_body("config data not exist")
        .create();

    let url = make_nacos_url_with_port(&server);
    let cc = NacosConfigCenter::new(url);

    let value = cc.get_config("empty-key", "dubbo").await.expect("get_config should succeed");
    assert_eq!(value, None);
    mock.assert();
}

#[tokio::test]
async fn test_get_config_returns_error_on_server_error() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/nacos/v1/cs/configs")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("dataId".into(), "error-key".into()),
            mockito::Matcher::UrlEncoded("group".into(), "dubbo".into()),
            mockito::Matcher::UrlEncoded("tenant".into(), "public".into()),
        ]))
        .with_status(500)
        .with_body("Internal Server Error")
        .create();

    let url = make_nacos_url_with_port(&server);
    let cc = NacosConfigCenter::new(url);

    let result = cc.get_config("error-key", "dubbo").await;
    assert!(result.is_err(), "should return error on 500");
    mock.assert();
}

#[tokio::test]
async fn test_get_config_with_auth() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/nacos/v1/cs/configs")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("dataId".into(), "secret-key".into()),
            mockito::Matcher::UrlEncoded("group".into(), "dubbo".into()),
            mockito::Matcher::UrlEncoded("tenant".into(), "public".into()),
            mockito::Matcher::UrlEncoded("accessKey".into(), "admin".into()),
            mockito::Matcher::UrlEncoded("secretKey".into(), "secret123".into()),
        ]))
        .with_status(200)
        .with_body("secret-value")
        .create();

    let url = make_nacos_url_with_port(&server);
    let cc = NacosConfigCenter::new(url)
        .with_auth("admin", "secret123");

    let value = cc.get_config("secret-key", "dubbo").await.expect("get_config should succeed");
    assert_eq!(value, Some("secret-value".to_string()));
    mock.assert();
}

#[tokio::test]
async fn test_set_config_succeeds() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/nacos/v1/cs/configs")
        .with_status(200)
        .with_body("ok")
        .expect_at_least(1)
        .create();

    let url = make_nacos_url_with_port(&server);
    let cc = NacosConfigCenter::new(url);

    let result = cc.set_config("timeout", "dubbo", "60s").await;
    assert!(result.is_ok(), "set_config should succeed");
    mock.assert();
}

#[tokio::test]
async fn test_set_config_rejected_by_nacos() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/nacos/v1/cs/configs")
        .with_status(200)
        .with_body("{\"code\":500,\"message\":\"internal error\"}")
        .expect_at_least(1)
        .create();

    let url = make_nacos_url_with_port(&server);
    let cc = NacosConfigCenter::new(url);

    let result = cc.set_config("timeout", "dubbo", "bad-value").await;
    assert!(result.is_err(), "set_config should fail when Nacos rejects");
    mock.assert();
}

#[tokio::test]
async fn test_set_config_http_error() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/nacos/v1/cs/configs")
        .with_status(400)
        .with_body("Bad Request")
        .create();

    let url = make_nacos_url_with_port(&server);
    let cc = NacosConfigCenter::new(url);

    let result = cc.set_config("timeout", "dubbo", "bad-value").await;
    assert!(result.is_err(), "set_config should fail on 400");
    mock.assert();
}

#[tokio::test]
async fn test_remove_config_succeeds() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("DELETE", "/nacos/v1/cs/configs")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("dataId".into(), "old-key".into()),
            mockito::Matcher::UrlEncoded("group".into(), "dubbo".into()),
            mockito::Matcher::UrlEncoded("tenant".into(), "public".into()),
        ]))
        .with_status(200)
        .with_body("ok")
        .create();

    let url = make_nacos_url_with_port(&server);
    let cc = NacosConfigCenter::new(url);

    let result = cc.remove_config("old-key", "dubbo").await;
    assert!(result.is_ok(), "remove_config should succeed");
    mock.assert();
}

#[tokio::test]
async fn test_remove_config_rejected() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("DELETE", "/nacos/v1/cs/configs")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("dataId".into(), "missing-key".into()),
            mockito::Matcher::UrlEncoded("group".into(), "dubbo".into()),
            mockito::Matcher::UrlEncoded("tenant".into(), "public".into()),
        ]))
        .with_status(200)
        .with_body("{\"code\":500,\"message\":\"config not found\"}")
        .create();

    let url = make_nacos_url_with_port(&server);
    let cc = NacosConfigCenter::new(url);

    let result = cc.remove_config("missing-key", "dubbo").await;
    assert!(result.is_err(), "remove_config should fail when Nacos rejects");
    mock.assert();
}

#[tokio::test]
async fn test_watch_and_destroy() {
    let mut server = mockito::Server::new_async().await;
    let get_mock = server
        .mock("GET", "/nacos/v1/cs/configs")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("dataId".into(), "poll-key".into()),
            mockito::Matcher::UrlEncoded("group".into(), "dubbo".into()),
            mockito::Matcher::UrlEncoded("tenant".into(), "public".into()),
        ]))
        .with_status(200)
        .with_body("v1")
        .expect_at_least(0)
        .create();

    let url = make_nacos_url_with_port(&server);
    let cc = NacosConfigCenter::new(url);

    let listener = std::sync::Arc::new(TestListener::new());
    cc.watch(
        "poll-key".to_string(),
        "dubbo".to_string(),
        listener.clone(),
    )
    .await
    .expect("watch should succeed");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    cc.destroy();
    get_mock.assert();
}

fn make_nacos_url_with_port(server: &mockito::Server) -> dubbo_rs_common::url::URL {
    let addr = server.host_with_port();
    let parts: Vec<&str> = addr.split(':').collect();
    let mut url = dubbo_rs_common::url::URL::new("nacos", "");
    url.ip = parts[0].to_string();
    url.port = (*parts.get(1).unwrap_or(&"8848")).to_string();
    url
}

struct TestListener {
    events: std::sync::Mutex<Vec<dubbo_rs_configcenter::ConfigChangeEvent>>,
}

impl TestListener {
    fn new() -> Self {
        Self {
            events: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl dubbo_rs_configcenter::ConfigListener for TestListener {
    async fn on_change(&self, event: dubbo_rs_configcenter::ConfigChangeEvent) {
        self.events.lock().unwrap().push(event);
    }
}
