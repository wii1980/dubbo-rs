use dubbo_rs_configcenter::ConfigCenter;
use dubbo_rs_common::node::Node;
use dubbo_rs_configcenter_apollo::ApolloConfigCenterBuilder;

#[tokio::test]
async fn test_get_config_returns_value() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/configs/my-app/default/application/timeout")
        .with_status(200)
        .with_body(r#"{"value":"30s","key":"timeout"}"#)
        .create();

    let cc = ApolloConfigCenterBuilder::new()
        .meta_server_url(server.url())
        .app_id("my-app")
        .build()
        .expect("build should succeed");

    let value = cc.get_config("timeout").await.expect("get_config should succeed");
    assert_eq!(value, Some("30s".to_string()));
    mock.assert();
}

#[tokio::test]
async fn test_get_config_returns_none_on_404() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/configs/my-app/default/application/missing-key")
        .with_status(404)
        .create();

    let cc = ApolloConfigCenterBuilder::new()
        .meta_server_url(server.url())
        .app_id("my-app")
        .build()
        .expect("build should succeed");

    let value = cc.get_config("missing-key").await.expect("get_config should succeed");
    assert_eq!(value, None);
    mock.assert();
}

#[tokio::test]
async fn test_get_config_returns_none_on_empty_body() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/configs/my-app/default/application/empty-key")
        .with_status(200)
        .with_body("")
        .create();

    let cc = ApolloConfigCenterBuilder::new()
        .meta_server_url(server.url())
        .app_id("my-app")
        .build()
        .expect("build should succeed");

    let value = cc.get_config("empty-key").await.expect("get_config should succeed");
    assert_eq!(value, None);
    mock.assert();
}

#[tokio::test]
async fn test_get_config_returns_error_on_server_error() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/configs/my-app/default/application/error-key")
        .with_status(500)
        .with_body("Internal Server Error")
        .create();

    let cc = ApolloConfigCenterBuilder::new()
        .meta_server_url(server.url())
        .app_id("my-app")
        .build()
        .expect("build should succeed");

    let result = cc.get_config("error-key").await;
    assert!(result.is_err(), "should return error on 500");
    mock.assert();
}

#[tokio::test]
async fn test_get_config_with_auth_token() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/configs/secured-app/default/application/secret-key")
        .match_header("Authorization", "my-token")
        .with_status(200)
        .with_body(r#"{"value":"top-secret"}"#)
        .create();

    let cc = ApolloConfigCenterBuilder::new()
        .meta_server_url(server.url())
        .app_id("secured-app")
        .token("my-token")
        .build()
        .expect("build should succeed");

    let value = cc.get_config("secret-key").await.expect("get_config should succeed");
    assert_eq!(value, Some("top-secret".to_string()));
    mock.assert();
}



#[tokio::test]
async fn test_set_config_succeeds() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/configs")
        .with_status(200)
        .with_body("ok")
        .create();

    let cc = ApolloConfigCenterBuilder::new()
        .meta_server_url(server.url())
        .app_id("my-app")
        .build()
        .expect("build should succeed");

    let result = cc.set_config("timeout", "60s").await;
    assert!(result.is_ok(), "set_config should succeed");
    mock.assert();
}

#[tokio::test]
async fn test_set_config_returns_error_on_failure() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/configs")
        .with_status(400)
        .with_body("Bad Request")
        .create();

    let cc = ApolloConfigCenterBuilder::new()
        .meta_server_url(server.url())
        .app_id("my-app")
        .build()
        .expect("build should succeed");

    let result = cc.set_config("timeout", "bad-value").await;
    assert!(result.is_err(), "set_config should fail on 400");
    mock.assert();
}



#[tokio::test]
async fn test_remove_config_succeeds() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("DELETE", "/configs/my-app/default/application/old-key")
        .with_status(200)
        .with_body("ok")
        .create();

    let cc = ApolloConfigCenterBuilder::new()
        .meta_server_url(server.url())
        .app_id("my-app")
        .build()
        .expect("build should succeed");

    let result = cc.remove_config("old-key").await;
    assert!(result.is_ok(), "remove_config should succeed");
    mock.assert();
}

#[tokio::test]
async fn test_remove_config_returns_error_on_failure() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("DELETE", "/configs/my-app/default/application/missing-key")
        .with_status(404)
        .with_body("Not Found")
        .create();

    let cc = ApolloConfigCenterBuilder::new()
        .meta_server_url(server.url())
        .app_id("my-app")
        .build()
        .expect("build should succeed");

    let result = cc.remove_config("missing-key").await;
    assert!(result.is_err(), "remove_config should fail on 404");
    mock.assert();
}



#[tokio::test]
async fn test_watch_and_destroy() {
    let mut server = mockito::Server::new_async().await;
    let get_mock = server
        .mock("GET", "/configs/my-app/default/application/poll-key")
        .with_status(200)
        .with_body(r#"{"value":"v1"}"#)
        .expect_at_least(0)
        .create();

    let cc = ApolloConfigCenterBuilder::new()
        .meta_server_url(server.url())
        .app_id("my-app")
        .build()
        .expect("build should succeed");

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



#[test]
fn test_build_config_url_with_mock_server() {
    let cc = ApolloConfigCenterBuilder::new()
        .meta_server_url("http://127.0.0.1:8080")
        .app_id("my-app")
        .cluster("prod")
        .namespace("dev-ns")
        .build()
        .expect("build should succeed");

    let url = cc.build_config_url("app.timeout");
    assert_eq!(
        url,
        "http://127.0.0.1:8080/configs/my-app/prod/dev-ns/app.timeout"
    );

    let notification_url = cc.build_notification_url();
    assert_eq!(
        notification_url,
        "http://127.0.0.1:8080/notifications/v2"
    );
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
