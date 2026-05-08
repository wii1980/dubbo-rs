#![allow(
    clippy::unused_self,
    clippy::unused_async,
    clippy::needless_pass_by_value,
    clippy::items_after_statements,
    async_fn_in_trait,
    dead_code
)]

use dubbo_rs_macros::client;

#[client]
pub trait Greeter {
    async fn say_hello(&self, name: String) -> Result<String, anyhow::Error>;
}

#[test]
fn test_client_struct_exists() {
    let url = dubbo_rs_common::url::URL::new("tri", "/com.example.Greeter");

    struct MockInvoker {
        url: dubbo_rs_common::url::URL,
    }

    impl dubbo_rs_common::node::Node for MockInvoker {
        fn get_url(&self) -> &dubbo_rs_common::url::URL {
            &self.url
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait::async_trait]
    impl dubbo_rs_protocol::Invoker for MockInvoker {
        async fn invoke(
            &self,
            _ctx: &mut dubbo_rs_protocol::InvocationContext,
        ) -> Result<dubbo_rs_protocol::RPCResult, anyhow::Error> {
            Ok(dubbo_rs_protocol::RPCResult::success(
                serde_json::to_vec("Hello, World!").unwrap(),
            ))
        }
    }

    let invoker = Box::new(MockInvoker { url: url.clone() });
    let _client = GreeterClient::new(invoker);
}

#[tokio::test]
async fn test_client_invoke() {
    let url = dubbo_rs_common::url::URL::new("tri", "/com.example.Greeter");

    struct EchoInvoker {
        url: dubbo_rs_common::url::URL,
    }

    impl dubbo_rs_common::node::Node for EchoInvoker {
        fn get_url(&self) -> &dubbo_rs_common::url::URL {
            &self.url
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait::async_trait]
    impl dubbo_rs_protocol::Invoker for EchoInvoker {
        async fn invoke(
            &self,
            ctx: &mut dubbo_rs_protocol::InvocationContext,
        ) -> Result<dubbo_rs_protocol::RPCResult, anyhow::Error> {
            let arg = ctx.arguments.first().cloned().unwrap_or_default();
            let name: String = serde_json::from_slice(&arg)?;
            let response = format!("Hello, {name}!");
            Ok(dubbo_rs_protocol::RPCResult::success(
                serde_json::to_vec(&response).unwrap(),
            ))
        }
    }

    let invoker = Box::new(EchoInvoker { url });
    let client = GreeterClient::new(invoker);

    let result = client.say_hello("Alice".to_string()).await.unwrap();
    assert_eq!(result, "Hello, Alice!");
}

#[test]
fn test_client_no_arg_trait_generates() {
    #[client]
    pub trait Pinger {
        async fn ping(&self) -> Result<String, anyhow::Error>;
    }

    let url = dubbo_rs_common::url::URL::new("tri", "/com.example.Pinger");
    struct PongInvoker {
        url: dubbo_rs_common::url::URL,
    }

    impl dubbo_rs_common::node::Node for PongInvoker {
        fn get_url(&self) -> &dubbo_rs_common::url::URL {
            &self.url
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait::async_trait]
    impl dubbo_rs_protocol::Invoker for PongInvoker {
        async fn invoke(
            &self,
            _ctx: &mut dubbo_rs_protocol::InvocationContext,
        ) -> Result<dubbo_rs_protocol::RPCResult, anyhow::Error> {
            Ok(dubbo_rs_protocol::RPCResult::success(
                serde_json::to_vec("pong").unwrap(),
            ))
        }
    }

    let _client = PingerClient::new(Box::new(PongInvoker { url }));
}
