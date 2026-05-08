#![allow(
    clippy::unused_self,
    clippy::unused_async,
    clippy::needless_pass_by_value,
    dead_code
)]

use dubbo_rs_macros::service;

struct GreeterService;

#[service]
impl GreeterService {
    async fn say_hello(&self, name: String) -> String {
        format!("Hello, {name}!")
    }

    async fn say_goodbye(&self, name: String) -> String {
        format!("Goodbye, {name}!")
    }
}

#[test]
fn test_service_macro_exposes_methods() {
    let methods = GreeterService::__service_methods();
    assert_eq!(methods.len(), 2);
    assert!(methods.contains(&"say_hello"));
    assert!(methods.contains(&"say_goodbye"));
}

#[test]
fn test_service_macro_single_method() {
    struct SingleService;

    #[service]
    impl SingleService {
        fn do_stuff(&self) {}
    }

    let methods = SingleService::__service_methods();
    assert_eq!(methods, vec!["do_stuff"]);
}
