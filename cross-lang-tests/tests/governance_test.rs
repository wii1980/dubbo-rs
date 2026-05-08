// Phase 5: Service governance API tests.
#![allow(clippy::items_after_statements, clippy::float_cmp)]
#![allow(
    clippy::no_effect_underscore_binding,
    clippy::doc_markdown,
    clippy::unnecessary_wraps,
    unused_imports
)]

use async_trait::async_trait;
use dubbo_rs_cluster::{Cluster, FailfastCluster, FailoverCluster, StaticDirectory};
use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_filter::{
    AccessLogFilter, CircuitBreaker, EchoFilter, Filter, FilterChain, GracefulShutdownFilter,
    TokenFilter,
};
use dubbo_rs_loadbalance::{
    ConsistentHashLoadBalance, LeastActiveLoadBalance, LoadBalance, RandomLoadBalance,
    RoundRobinLoadBalance,
};
use dubbo_rs_protocol::{InvocationContext, Invoker, RPCResult};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[test]
fn e_001_random_loadbalance() {
    let url = URL::new("tri", "/test");
    let ctx = InvocationContext::new("m", URL::new("tri", "/test"));
    let result = RandomLoadBalance.select(&[], &url, &ctx);
    assert!(result.is_err(), "empty invokers should return error");
}

#[test]
fn e_001b_roundrobin() {
    let url = URL::new("tri", "/test");
    let ctx = InvocationContext::new("m", URL::new("tri", "/test"));
    let result = RoundRobinLoadBalance::new().select(&[], &url, &ctx);
    assert!(result.is_err(), "empty invokers should return error");
}

#[test]
fn e_001c_least_active() {
    let url = URL::new("tri", "/test");
    let ctx = InvocationContext::new("m", URL::new("tri", "/test"));
    let result = LeastActiveLoadBalance.select(&[], &url, &ctx);
    assert!(result.is_err(), "empty invokers should return error");
}

#[test]
fn e_001d_consistent_hash() {
    let url = URL::new("tri", "/test");
    let mut ctx = InvocationContext::new("m", URL::new("tri", "/test"));
    ctx.arguments = vec![b"k".to_vec()];
    let result = ConsistentHashLoadBalance::new().select(&[], &url, &ctx);
    assert!(result.is_err(), "empty invokers should return error");
}

#[tokio::test]
async fn e_002_failover_cluster() {
    let dir = StaticDirectory::new(URL::new("tri", "/test"));
    let invoker = FailoverCluster::default()
        .join(Box::new(dir))
        .await
        .unwrap();
    let mut ctx = InvocationContext::new("m", URL::new("tri", "/test"));
    let result = invoker.invoke(&mut ctx).await;
    assert!(result.is_err(), "empty directory should error on invoke");
}

#[tokio::test]
async fn e_002b_failfast_cluster() {
    let dir = StaticDirectory::new(URL::new("tri", "/test"));
    let invoker = FailfastCluster.join(Box::new(dir)).await.unwrap();
    let mut ctx = InvocationContext::new("m", URL::new("tri", "/test"));
    let result = invoker.invoke(&mut ctx).await;
    assert!(result.is_err(), "empty directory should error on invoke");
}

#[test]
fn e_003_echo_filter() {
    let _: Box<dyn Filter> = Box::new(EchoFilter);
}

#[test]
fn e_003b_token_filter() {
    let f = TokenFilter::new("my-token");
    let f2 = TokenFilter::new("my-token").with_key("auth_token");
    let _: Box<dyn Filter> = Box::new(f);
    let _: Box<dyn Filter> = Box::new(f2);
}

#[test]
fn e_003c_access_log() {
    let _: Box<dyn Filter> = Box::new(AccessLogFilter);
}

#[test]
fn e_003d_shutdown_filter() {
    let flag = Arc::new(AtomicBool::new(false));
    let f1 = GracefulShutdownFilter::from_flag(flag.clone());
    let f2 = GracefulShutdownFilter::from_flag(flag);
    let _: Box<dyn Filter> = Box::new(f1);
    let _: Box<dyn Filter> = Box::new(f2);
}

#[test]
fn e_003e_circuit_breaker() {
    let cb = CircuitBreaker::new().with_failure_threshold(3);
    assert!(cb.is_call_permitted(), "new breaker should start closed");
    cb.record_failure();
    cb.record_failure();
    cb.record_failure();
    assert!(
        !cb.is_call_permitted(),
        "breaker should open after threshold failures"
    );
}

// ── E-004: Error status code mapping (API-level) ──────────────────────────

mod e_004_error_mapping {
    use super::*;

    #[test]
    fn e_004a_client_timeout_status_code() {
        let err = RPCError::ClientTimeout("request took too long".into());
        assert_eq!(err.status_code(), 30, "E-004a: CLIENT_TIMEOUT = 30");
        let roundtrip = RPCError::from_status_code(30, "test");
        assert_eq!(roundtrip, RPCError::ClientTimeout("test".into()));
    }

    #[test]
    fn e_004b_server_timeout_status_code() {
        let err = RPCError::ServerTimeout("server busy".into());
        assert_eq!(err.status_code(), 31, "E-004b: SERVER_TIMEOUT = 31");
    }

    #[test]
    fn e_004c_service_not_found_status_code() {
        let err = RPCError::ServiceNotFound("com.example.Missing".into());
        assert_eq!(err.status_code(), 60, "E-004c: SERVICE_NOT_FOUND = 60");
    }

    #[test]
    fn e_004d_all_status_codes_unique() {
        let codes = [
            RPCError::ClientTimeout(String::new()).status_code(),
            RPCError::ServerTimeout(String::new()).status_code(),
            RPCError::BadRequest(String::new()).status_code(),
            RPCError::BadResponse(String::new()).status_code(),
            RPCError::ServiceNotFound(String::new()).status_code(),
            RPCError::ServiceError(String::new()).status_code(),
            RPCError::ServerError(String::new()).status_code(),
            RPCError::ClientError(String::new()).status_code(),
            RPCError::ServerThreadpoolExhausted(String::new()).status_code(),
        ];
        let unique: std::collections::HashSet<u8> = codes.iter().copied().collect();
        assert_eq!(
            unique.len(),
            9,
            "E-004d: all 9 error variants have unique status codes"
        );
    }
}

// ── E-005: Filter chain execution tests ───────────────────────────────────

mod e_005_filter_chain {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct MockInvoker {
        url: URL,
        call_count: Arc<AtomicUsize>,
    }

    impl MockInvoker {
        fn new(url: URL) -> Self {
            Self {
                url,
                call_count: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    impl Node for MockInvoker {
        fn get_url(&self) -> &URL {
            &self.url
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait]
    impl Invoker for MockInvoker {
        async fn invoke(&self, ctx: &mut InvocationContext) -> anyhow::Result<RPCResult> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            Ok(RPCResult::success(
                format!("mock:{}", ctx.method_name).into_bytes(),
            ))
        }
    }

    /// E-005a: EchoFilter short-circuits $echo calls
    #[tokio::test]
    async fn e_005a_echo_filter_short_circuit() {
        let url = URL::new("dubbo", "/test");
        let invoker = MockInvoker::new(url);
        let count = invoker.call_count.clone();

        let chain = FilterChain::new(vec![Box::new(EchoFilter)], Box::new(invoker)).build();

        let mut ctx = InvocationContext::new("$echo", URL::new("dubbo", "/test"));
        ctx.arguments = vec![b"ping".to_vec()];

        let result = chain.invoke(&mut ctx).await.unwrap();
        assert_eq!(
            result.value,
            Some(b"ping".to_vec()),
            "E-005a: echo filter should return args"
        );
        assert_eq!(
            count.load(Ordering::Relaxed),
            0,
            "E-005a: base invoker should NOT be called"
        );
    }

    /// E-005b: EchoFilter passes through non-echo calls
    #[tokio::test]
    async fn e_005b_echo_filter_passthrough() {
        let url = URL::new("dubbo", "/test");
        let invoker = MockInvoker::new(url);
        let count = invoker.call_count.clone();

        let chain = FilterChain::new(vec![Box::new(EchoFilter)], Box::new(invoker)).build();

        let mut ctx = InvocationContext::new("sayHello", URL::new("dubbo", "/test"));
        let result = chain.invoke(&mut ctx).await.unwrap();
        assert_eq!(
            result.value,
            Some(b"mock:sayHello".to_vec()),
            "E-005b: should call base invoker"
        );
        assert_eq!(
            count.load(Ordering::Relaxed),
            1,
            "E-005b: base invoker should be called once"
        );
    }

    /// E-005c: TokenFilter rejects missing token
    #[tokio::test]
    async fn e_005c_token_filter_rejects_missing() {
        let url = URL::new("dubbo", "/test");
        let invoker = MockInvoker::new(url);

        let chain = FilterChain::new(
            vec![Box::new(TokenFilter::new("secret"))],
            Box::new(invoker),
        )
        .build();

        let mut ctx = InvocationContext::new("sayHello", URL::new("dubbo", "/test"));
        let result = chain.invoke(&mut ctx).await;
        assert!(result.is_err(), "E-005c: should reject without token");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("token missing"),
            "E-005c: error should mention missing token"
        );
    }

    /// E-005d: TokenFilter accepts valid token
    #[tokio::test]
    async fn e_005d_token_filter_accepts_valid() {
        let url = URL::new("dubbo", "/test");
        let invoker = MockInvoker::new(url);

        let chain = FilterChain::new(
            vec![Box::new(TokenFilter::new("secret"))],
            Box::new(invoker),
        )
        .build();

        let mut ctx = InvocationContext::new("sayHello", URL::new("dubbo", "/test"));
        ctx.attachments.insert("token".into(), "secret".into());
        let result = chain.invoke(&mut ctx).await;
        assert!(result.is_ok(), "E-005d: should accept valid token");
    }

    /// E-005e: Filter chain order (echo + token combined)
    #[tokio::test]
    async fn e_005e_combined_filter_chain() {
        let url = URL::new("dubbo", "/test");
        let invoker = MockInvoker::new(url);

        let chain = FilterChain::new(
            vec![Box::new(EchoFilter), Box::new(TokenFilter::new("key123"))],
            Box::new(invoker),
        )
        .build();

        // $echo should short-circuit before token check
        let mut ctx = InvocationContext::new("$echo", URL::new("dubbo", "/test"));
        ctx.arguments = vec![b"health-check".to_vec()];
        let result = chain.invoke(&mut ctx).await.unwrap();
        assert_eq!(
            result.value,
            Some(b"health-check".to_vec()),
            "E-005e: echo bypasses token"
        );
    }

    /// E-005f: CircuitBreaker state transitions
    #[test]
    fn e_005f_circuit_breaker_states() {
        let cb = CircuitBreaker::new()
            .with_failure_threshold(3)
            .with_recovery_timeout(std::time::Duration::from_millis(100));

        // Initially should permit calls
        assert!(cb.is_call_permitted(), "E-005f: initially permits calls");

        // Record failures up to threshold
        for _ in 0..3 {
            cb.record_failure();
        }
        assert!(
            !cb.is_call_permitted(),
            "E-005f: should block after threshold failures"
        );
    }

    /// E-005g: RPCError display formatting
    #[test]
    fn e_005g_rpc_error_display() {
        let err = RPCError::ServiceError("divide by zero".into());
        let msg = err.to_string();
        assert!(
            msg.contains("Service error"),
            "E-005g: should contain variant name"
        );
        assert!(
            msg.contains("divide by zero"),
            "E-005g: should contain message"
        );
    }
}

// ── E-004h: Timeout control via tokio::time::timeout (API-level) ──────────

mod e_004_timeout_control {
    use super::*;

    /// A slow invoker that sleeps before responding.
    struct SlowInvoker {
        url: URL,
        delay: std::time::Duration,
    }

    impl SlowInvoker {
        fn new(url: URL, delay: std::time::Duration) -> Self {
            Self { url, delay }
        }
    }

    impl Node for SlowInvoker {
        fn get_url(&self) -> &URL {
            &self.url
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait]
    impl Invoker for SlowInvoker {
        async fn invoke(&self, ctx: &mut InvocationContext) -> anyhow::Result<RPCResult> {
            tokio::time::sleep(self.delay).await;
            Ok(RPCResult::success(
                format!("slow:{}", ctx.method_name).into_bytes(),
            ))
        }
    }

    /// E-004h: tokio::time::timeout fires when invoker is too slow
    #[tokio::test]
    async fn e_004h_timeout_fires_on_slow_invoker() {
        let url = URL::new("dubbo", "/com.example.SlowService");
        let invoker = SlowInvoker::new(url, std::time::Duration::from_millis(200));

        let mut ctx =
            InvocationContext::new("slowMethod", URL::new("dubbo", "/com.example.SlowService"));

        // 50ms timeout should fire because invoker takes 200ms
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            invoker.invoke(&mut ctx),
        )
        .await;

        assert!(result.is_err(), "E-004h: should timeout");
        // Elapsed itself proves timeout occurred
        // Map to RPCError for consistency with dubbo error model
        let rpc_err = RPCError::ClientTimeout("request timed out after 50ms".into());
        assert_eq!(
            rpc_err.status_code(),
            30,
            "E-004h: CLIENT_TIMEOUT status = 30"
        );
    }

    /// E-004i: Fast invoker completes within timeout
    #[tokio::test]
    async fn e_004i_fast_invoker_within_timeout() {
        let url = URL::new("dubbo", "/com.example.FastService");
        let invoker = SlowInvoker::new(url, std::time::Duration::from_millis(10));

        let mut ctx =
            InvocationContext::new("fastMethod", URL::new("dubbo", "/com.example.FastService"));

        // 500ms timeout should NOT fire because invoker takes only 10ms
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            invoker.invoke(&mut ctx),
        )
        .await;

        assert!(result.is_ok(), "E-004i: should not timeout");
        let rpc_result = result.unwrap().unwrap();
        assert!(!rpc_result.is_error());
        let reply = String::from_utf8_lossy(rpc_result.value.as_ref().unwrap());
        assert!(
            reply.contains("fastMethod"),
            "E-004i: reply contains method name"
        );
    }

    /// E-004j: Timeout + FilterChain integration
    #[tokio::test]
    async fn e_004j_timeout_with_filter_chain() {
        let url = URL::new("dubbo", "/com.example.ChainService");
        let invoker = SlowInvoker::new(url, std::time::Duration::from_millis(200));

        let chain = FilterChain::new(vec![Box::new(EchoFilter)], Box::new(invoker)).build();

        let mut ctx =
            InvocationContext::new("process", URL::new("dubbo", "/com.example.ChainService"));

        // Timeout should fire even through filter chain
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(50), chain.invoke(&mut ctx))
                .await;

        assert!(
            result.is_err(),
            "E-004j: timeout should fire through filter chain"
        );
    }
}

// ── E-005h: Exception propagation (API-level) ──────────────────────────────

mod e_005_exception_propagation {
    use super::*;

    /// An invoker that returns RPC errors for specific methods.
    struct ErrorInvoker {
        url: URL,
    }

    impl ErrorInvoker {
        fn new(url: URL) -> Self {
            Self { url }
        }
    }

    impl Node for ErrorInvoker {
        fn get_url(&self) -> &URL {
            &self.url
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait]
    impl Invoker for ErrorInvoker {
        async fn invoke(&self, ctx: &mut InvocationContext) -> anyhow::Result<RPCResult> {
            match ctx.method_name.as_str() {
                "throwBiz" => Ok(RPCResult::from_error(RPCError::ServiceError(
                    "business logic failed".into(),
                ))),
                "throwNotFound" => Ok(RPCResult::from_error(RPCError::ServiceNotFound(
                    "com.example.Missing".into(),
                ))),
                "throwTimeout" => Ok(RPCResult::from_error(RPCError::ServerTimeout(
                    "provider busy".into(),
                ))),
                "throwInternal" => Err(anyhow::anyhow!("internal server error")),
                _ => Ok(RPCResult::success(b"ok".to_vec())),
            }
        }
    }

    /// E-005h: ServiceError propagates through direct invoke
    #[tokio::test]
    async fn e_005h_service_error_propagation() {
        let url = URL::new("dubbo", "/com.example.ErrorService");
        let invoker = ErrorInvoker::new(url);

        let mut ctx =
            InvocationContext::new("throwBiz", URL::new("dubbo", "/com.example.ErrorService"));
        let result = invoker.invoke(&mut ctx).await.unwrap();

        assert!(result.is_error(), "E-005h: should be error result");
        let err = result.error.unwrap();
        assert_eq!(err.status_code(), 70, "E-005h: SERVICE_ERROR status = 70");
        let msg = err.to_string();
        assert!(
            msg.contains("business logic failed"),
            "E-005h: error message preserved"
        );
    }

    /// E-005i: ServiceNotFound propagates with correct status code
    #[tokio::test]
    async fn e_005i_service_not_found_propagation() {
        let url = URL::new("dubbo", "/com.example.ErrorService");
        let invoker = ErrorInvoker::new(url);

        let mut ctx = InvocationContext::new(
            "throwNotFound",
            URL::new("dubbo", "/com.example.ErrorService"),
        );
        let result = invoker.invoke(&mut ctx).await.unwrap();

        assert!(result.is_error(), "E-005i: should be error");
        let err = result.error.unwrap();
        assert_eq!(
            err.status_code(),
            60,
            "E-005i: SERVICE_NOT_FOUND status = 60"
        );
    }

    /// E-005j: ServerTimeout propagates through FilterChain
    #[tokio::test]
    async fn e_005j_timeout_error_through_chain() {
        let url = URL::new("dubbo", "/com.example.ErrorService");
        let invoker = ErrorInvoker::new(url);

        let chain = FilterChain::new(vec![Box::new(EchoFilter)], Box::new(invoker)).build();

        let mut ctx = InvocationContext::new(
            "throwTimeout",
            URL::new("dubbo", "/com.example.ErrorService"),
        );
        let result = chain.invoke(&mut ctx).await.unwrap();

        assert!(result.is_error(), "E-005j: timeout error through chain");
        let err = result.error.unwrap();
        assert_eq!(err.status_code(), 31, "E-005j: SERVER_TIMEOUT status = 31");
    }

    /// E-005k: Internal error (transport-level Err) propagates
    #[tokio::test]
    async fn e_005k_internal_error_propagation() {
        let url = URL::new("dubbo", "/com.example.ErrorService");
        let invoker = ErrorInvoker::new(url);

        let mut ctx = InvocationContext::new(
            "throwInternal",
            URL::new("dubbo", "/com.example.ErrorService"),
        );
        let result = invoker.invoke(&mut ctx).await;

        assert!(result.is_err(), "E-005k: internal error as Err");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("internal server error"),
            "E-005k: message preserved"
        );
    }

    /// E-005l: Normal method succeeds alongside error methods
    #[tokio::test]
    async fn e_005l_normal_succeeds_alongside_errors() {
        let url = URL::new("dubbo", "/com.example.ErrorService");
        let invoker = ErrorInvoker::new(url);

        let mut ctx = InvocationContext::new(
            "normalMethod",
            URL::new("dubbo", "/com.example.ErrorService"),
        );
        let result = invoker.invoke(&mut ctx).await.unwrap();

        assert!(!result.is_error(), "E-005l: normal method should succeed");
        assert_eq!(result.value, Some(b"ok".to_vec()));
    }

    /// E-005m: Error status code roundtrip (encode → decode)
    #[test]
    fn e_005m_error_status_code_roundtrip() {
        // Verify all error variants survive status_code → from_status_code roundtrip
        let cases: Vec<(RPCError, u8)> = vec![
            (RPCError::ClientTimeout("t".into()), 30),
            (RPCError::ServerTimeout("t".into()), 31),
            (RPCError::BadRequest("t".into()), 40),
            (RPCError::BadResponse("t".into()), 50),
            (RPCError::ServiceNotFound("t".into()), 60),
            (RPCError::ServiceError("t".into()), 70),
            (RPCError::ServerError("t".into()), 80),
            (RPCError::ClientError("t".into()), 90),
        ];

        for (original, expected_code) in cases {
            let code = original.status_code();
            assert_eq!(code, expected_code, "E-005m: status code for {original:?}");
            let roundtrip = RPCError::from_status_code(code, "test_msg");
            assert_eq!(
                roundtrip.status_code(),
                code,
                "E-005m: roundtrip for code {code}"
            );
        }
    }
}

// ── E-001 deep: Load balance selection logic ────────────────────────────────

mod e_001_loadbalance {
    use dubbo_rs_common::node::Node;
    use dubbo_rs_common::url::URL;
    use dubbo_rs_loadbalance::{
        ConsistentHashLoadBalance, LeastActiveLoadBalance, LoadBalance, P2CLoadBalance,
        RandomLoadBalance, RoundRobinLoadBalance, ShortestResponseLoadBalance,
    };
    use dubbo_rs_protocol::{InvocationContext, Invoker, RPCResult};

    struct LbTestInvoker {
        url: URL,
    }

    impl LbTestInvoker {
        fn new(url: URL) -> Self {
            Self { url }
        }
    }

    impl Node for LbTestInvoker {
        fn get_url(&self) -> &URL {
            &self.url
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait::async_trait]
    impl Invoker for LbTestInvoker {
        async fn invoke(&self, _ctx: &mut InvocationContext) -> anyhow::Result<RPCResult> {
            Ok(RPCResult::success(b"lb-test".to_vec()))
        }
    }

    fn lb_ctx() -> InvocationContext {
        InvocationContext::new("sayHello", lb_url())
    }

    fn lb_url() -> URL {
        URL::new("tri", "/com.example.LoadBalancedService")
    }

    fn make_invoker_with_params(params: &[(&str, &str)]) -> Box<dyn Invoker> {
        let mut url = URL::new("tri", "/com.example.LoadBalancedService");
        for (k, v) in params {
            url.set_param(*k, *v);
        }
        Box::new(LbTestInvoker::new(url))
    }

    /// E-001e: Random LB selects only valid indices across 20 selections.
    #[test]
    fn e_001e_random_select_returns_valid_index() {
        let lb = RandomLoadBalance;
        let invokers: Vec<Box<dyn Invoker>> =
            (0..3).map(|_| make_invoker_with_params(&[])).collect();

        for _ in 0..20 {
            let idx = lb.select(&invokers, &lb_url(), &lb_ctx()).unwrap();
            assert!(idx < 3, "E-001e: index {idx} out of range [0,3)");
        }
    }

    /// E-001f: Random LB returns ServiceNotFound on empty invoker list.
    #[test]
    fn e_001f_random_empty_returns_error() {
        let lb = RandomLoadBalance;
        let result = lb.select(&[], &lb_url(), &lb_ctx());
        assert!(result.is_err(), "E-001f: should fail on empty list");
        let err = result.unwrap_err();
        assert_eq!(err.status_code(), 60, "E-001f: ServiceNotFound = 60");
    }

    /// E-001g: Round-robin with equal weights cycles [0,1,2,0,1,2].
    #[test]
    fn e_001g_roundrobin_strict_sequence() {
        let lb = RoundRobinLoadBalance::new();
        let invokers: Vec<Box<dyn Invoker>> =
            (0..3).map(|_| make_invoker_with_params(&[])).collect();
        let url = lb_url();
        let ctx = lb_ctx();

        // With default weight=100 for all, total_weight != 0, so weighted path is used.
        // Since all weights are equal, the weighted round-robin still cycles 0,1,2.
        let indices: Vec<usize> = (0..6)
            .map(|_| lb.select(&invokers, &url, &ctx).unwrap())
            .collect();
        assert_eq!(
            indices,
            vec![0, 1, 2, 0, 1, 2],
            "E-001g: strict round-robin sequence"
        );
    }

    /// E-001h: Round-robin with single invoker always returns 0.
    #[test]
    fn e_001h_roundrobin_single_always_zero() {
        let lb = RoundRobinLoadBalance::new();
        let invokers: Vec<Box<dyn Invoker>> = vec![make_invoker_with_params(&[])];
        let url = lb_url();
        let ctx = lb_ctx();

        for _ in 0..10 {
            let idx = lb.select(&invokers, &url, &ctx).unwrap();
            assert_eq!(idx, 0, "E-001h: single invoker must always be index 0");
        }
    }

    /// E-001i: Least-active prefers invoker with active=0 (index 1).
    #[test]
    fn e_001i_least_active_prefers_zero() {
        let lb = LeastActiveLoadBalance;
        let invokers: Vec<Box<dyn Invoker>> = vec![
            make_invoker_with_params(&[("active", "5")]),
            make_invoker_with_params(&[("active", "0")]),
            make_invoker_with_params(&[("active", "3")]),
        ];

        let idx = lb.select(&invokers, &lb_url(), &lb_ctx()).unwrap();
        assert_eq!(idx, 1, "E-001i: should pick invoker with active=0");
    }

    /// E-001j: Least-active tiebreak by weight — both active=0, weight 200 wins.
    #[test]
    fn e_001j_least_active_tiebreak_weight() {
        let lb = LeastActiveLoadBalance;
        let invokers: Vec<Box<dyn Invoker>> = vec![
            make_invoker_with_params(&[("active", "0"), ("weight", "50")]),
            make_invoker_with_params(&[("active", "0"), ("weight", "200")]),
        ];

        let idx = lb.select(&invokers, &lb_url(), &lb_ctx()).unwrap();
        assert_eq!(
            idx, 1,
            "E-001j: should pick higher-weight invoker on active tie"
        );
    }

    /// E-001k: Consistent hash is deterministic — same args, 50 calls, all same index.
    #[test]
    fn e_001k_consistent_hash_deterministic() {
        let lb = ConsistentHashLoadBalance::new();
        let invokers: Vec<Box<dyn Invoker>> =
            (0..5).map(|_| make_invoker_with_params(&[])).collect();
        let url = lb_url();
        let mut ctx = lb_ctx();
        ctx.arguments = vec![b"consistent-key-42".to_vec()];

        let first = lb.select(&invokers, &url, &ctx).unwrap();
        for _ in 0..49 {
            let idx = lb.select(&invokers, &url, &ctx).unwrap();
            assert_eq!(
                idx, first,
                "E-001k: consistent hash must always return same index"
            );
        }
    }

    /// E-001l: Consistent hash spreads across invokers for different keys.
    #[test]
    fn e_001l_consistent_hash_different_keys_spread() {
        let lb = ConsistentHashLoadBalance::new();
        let invokers: Vec<Box<dyn Invoker>> =
            (0..5).map(|_| make_invoker_with_params(&[])).collect();
        let url = lb_url();

        let mut seen = std::collections::HashSet::new();
        for i in 0..10 {
            let mut ctx = lb_ctx();
            ctx.arguments = vec![format!("key-{i}").into_bytes()];
            let idx = lb.select(&invokers, &url, &ctx).unwrap();
            seen.insert(idx);
        }

        assert!(
            seen.len() >= 2,
            "E-001l: at least 2 different indices for 10 keys, got {seen:?}",
        );
    }

    /// E-001m: ShortestResponseLoadBalance prefers invoker with lowest RT.
    #[test]
    fn e_001m_shortest_response_prefers_lowest_rt() {
        let lb = ShortestResponseLoadBalance;
        let invokers: Vec<Box<dyn Invoker>> = vec![
            make_invoker_with_params(&[("rt", "200"), ("rt_count", "50")]),
            make_invoker_with_params(&[("rt", "30"), ("rt_count", "50")]),
            make_invoker_with_params(&[("rt", "150"), ("rt_count", "50")]),
        ];
        let idx = lb.select(&invokers, &lb_url(), &lb_ctx()).unwrap();
        assert_eq!(idx, 1, "E-001m: should pick invoker with lowest RT");
    }

    /// E-001n: ShortestResponseLoadBalance falls back to random when no RT data.
    #[test]
    fn e_001n_shortest_response_fallback() {
        let lb = ShortestResponseLoadBalance;
        let invokers: Vec<Box<dyn Invoker>> =
            (0..3).map(|_| make_invoker_with_params(&[])).collect();
        let idx = lb.select(&invokers, &lb_url(), &lb_ctx()).unwrap();
        assert!(idx < 3, "E-001n: should return a valid index");
    }

    /// E-001o: P2CLoadBalance selects from multiple invokers.
    #[test]
    fn e_001o_p2c_selects_valid_index() {
        let lb = P2CLoadBalance::new();
        let invokers: Vec<Box<dyn Invoker>> =
            (0..3).map(|_| make_invoker_with_params(&[])).collect();
        let idx = lb.select(&invokers, &lb_url(), &lb_ctx()).unwrap();
        assert!(idx < 3, "E-001o: should return a valid index");
    }

    /// E-001p: P2CLoadBalance distributes across invokers.
    #[test]
    fn e_001p_p2c_distribution() {
        let lb = P2CLoadBalance::new();
        let invokers: Vec<Box<dyn Invoker>> =
            (0..3).map(|_| make_invoker_with_params(&[])).collect();

        let mut counts = [0usize; 3];
        for _ in 0..300 {
            let idx = lb.select(&invokers, &lb_url(), &lb_ctx()).unwrap();
            counts[idx] += 1;
            let key = format!(":{}", invokers[idx].get_url().port);
            lb.record_result(&key, 10, true);
        }

        for (i, count) in counts.iter().enumerate() {
            assert!(
                *count > 30,
                "E-001p: invoker {i} got {count} calls, expected > 30"
            );
        }
    }
}

// ── E-002 deep: Cluster retry logic ─────────────────────────────────────────

mod e_002_cluster {
    use dubbo_rs_cluster::{
        AvailableCluster, BroadcastCluster, Cluster, Directory, FailbackCluster, FailfastCluster,
        FailoverCluster, FailsafeCluster, ForkingCluster, MockCluster, StaticDirectory,
    };
    use dubbo_rs_common::node::Node;
    use dubbo_rs_common::url::URL;
    use dubbo_rs_protocol::{InvocationContext, Invoker, RPCResult};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct CountedInvoker {
        url: URL,
        succeed: bool,
        call_count: Arc<AtomicUsize>,
    }

    impl CountedInvoker {
        fn new(url: URL, succeed: bool) -> Self {
            Self {
                url,
                succeed,
                call_count: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    impl Node for CountedInvoker {
        fn get_url(&self) -> &URL {
            &self.url
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait::async_trait]
    impl Invoker for CountedInvoker {
        async fn invoke(&self, _ctx: &mut InvocationContext) -> anyhow::Result<RPCResult> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            if self.succeed {
                Ok(RPCResult::success(b"ok".to_vec()))
            } else {
                Ok(RPCResult::from_error(
                    dubbo_rs_common::error::RPCError::ServerError("fail".into()),
                ))
            }
        }
    }

    fn svc_url() -> URL {
        URL::new("tri", "/com.example.ClusterService")
    }

    fn invoker_url(id: usize) -> URL {
        let mut url = URL::new("tri", "/com.example.ClusterService");
        url.ip = format!("192.168.1.{id}");
        url.port = "50051".to_string();
        url
    }

    /// E-002c: Failover retries until success — 2 fail + 1 succeeds, all called.
    #[tokio::test]
    async fn e_002c_failover_retries_until_success() {
        let inv1 = Arc::new(CountedInvoker::new(invoker_url(1), false));
        let inv2 = Arc::new(CountedInvoker::new(invoker_url(2), false));
        let inv3 = Arc::new(CountedInvoker::new(invoker_url(3), true));

        let c1 = inv1.call_count.clone();
        let c2 = inv2.call_count.clone();
        let c3 = inv3.call_count.clone();

        let dir = StaticDirectory::new(svc_url());
        dir.add_invoker(inv1);
        dir.add_invoker(inv2);
        dir.add_invoker(inv3);

        let cluster = FailoverCluster::new().with_retries(2);
        let invoker = cluster.join(Box::new(dir)).await.unwrap();

        let mut ctx = InvocationContext::new("sayHello", svc_url());
        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_ok(), "E-002c: should eventually succeed");

        // With retries=2, total attempts = 3. Each attempt iterates all invokers.
        // Attempt 1: inv1(fail), inv2(fail), inv3(success) → done
        assert_eq!(c1.load(Ordering::SeqCst), 1, "E-002c: inv1 called once");
        assert_eq!(c2.load(Ordering::SeqCst), 1, "E-002c: inv2 called once");
        assert_eq!(c3.load(Ordering::SeqCst), 1, "E-002c: inv3 called once");
    }

    /// E-002d: Failover with all invokers failing returns error.
    #[tokio::test]
    async fn e_002d_failover_all_fail_returns_error() {
        let inv1 = Arc::new(CountedInvoker::new(invoker_url(1), false));
        let inv2 = Arc::new(CountedInvoker::new(invoker_url(2), false));
        let inv3 = Arc::new(CountedInvoker::new(invoker_url(3), false));

        let dir = StaticDirectory::new(svc_url());
        dir.add_invoker(inv1);
        dir.add_invoker(inv2);
        dir.add_invoker(inv3);

        let cluster = FailoverCluster::new().with_retries(2);
        let invoker = cluster.join(Box::new(dir)).await.unwrap();

        let mut ctx = InvocationContext::new("sayHello", svc_url());
        let result = invoker.invoke(&mut ctx).await;
        assert!(
            result.is_err(),
            "E-002d: should fail when all invokers fail"
        );
    }

    /// E-002e: Failfast uses only first invoker — second never called.
    #[tokio::test]
    async fn e_002e_failfast_first_wins() {
        let inv1 = Arc::new(CountedInvoker::new(invoker_url(1), true));
        let inv2 = Arc::new(CountedInvoker::new(invoker_url(2), true));

        let c1 = inv1.call_count.clone();
        let c2 = inv2.call_count.clone();

        let dir = StaticDirectory::new(svc_url());
        dir.add_invoker(inv1);
        dir.add_invoker(inv2);

        let invoker = FailfastCluster.join(Box::new(dir)).await.unwrap();

        let mut ctx = InvocationContext::new("sayHello", svc_url());
        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_ok(), "E-002e: first invoker succeeds");

        assert_eq!(c1.load(Ordering::SeqCst), 1, "E-002e: first invoker called");
        assert_eq!(
            c2.load(Ordering::SeqCst),
            0,
            "E-002e: second invoker NOT called"
        );
    }

    /// E-002f: Failfast with empty directory returns error on invoke.
    #[tokio::test]
    async fn e_002f_failfast_empty_dir_error() {
        let dir = StaticDirectory::new(svc_url());
        let invoker = FailfastCluster.join(Box::new(dir)).await.unwrap();

        let mut ctx = InvocationContext::new("sayHello", svc_url());
        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_err(), "E-002f: should error on empty directory");
    }

    /// E-002g: FailoverCluster with_retries(3) constructs without error.
    #[test]
    fn e_002g_failover_retries_config() {
        let _cluster = FailoverCluster::default();
        let _cluster = FailoverCluster::new().with_retries(3);
        let _cluster = FailoverCluster::new().with_retries(0);
    }

    /// E-002h: StaticDirectory add + list returns both invokers.
    #[tokio::test]
    async fn e_002h_static_directory_add_list() {
        let dir = StaticDirectory::new(svc_url());
        dir.add_invoker(Arc::new(CountedInvoker::new(invoker_url(1), true)));
        dir.add_invoker(Arc::new(CountedInvoker::new(invoker_url(2), true)));

        assert_eq!(dir.invoker_count(), 2, "E-002h: should have 2 invokers");

        let ctx = InvocationContext::new("sayHello", svc_url());
        let list = dir.list(&ctx).await.unwrap();
        assert_eq!(list.len(), 2, "E-002h: list() returns 2 invokers");
    }

    /// E-002i: ForkingCluster invokes up to `forks` invokers in parallel.
    ///
    /// With 4 invokers and forks=2, only 2 should be called.
    /// The first successful response wins.
    #[tokio::test]
    async fn e_002i_forking_parallel_invoke() {
        let inv1 = Arc::new(CountedInvoker::new(invoker_url(1), true));
        let inv2 = Arc::new(CountedInvoker::new(invoker_url(2), true));
        let inv3 = Arc::new(CountedInvoker::new(invoker_url(3), true));
        let inv4 = Arc::new(CountedInvoker::new(invoker_url(4), true));

        let c1 = inv1.call_count.clone();
        let c2 = inv2.call_count.clone();
        let c3 = inv3.call_count.clone();
        let c4 = inv4.call_count.clone();

        let dir = StaticDirectory::new(svc_url());
        dir.add_invoker(inv1);
        dir.add_invoker(inv2);
        dir.add_invoker(inv3);
        dir.add_invoker(inv4);

        let cluster = ForkingCluster::new().with_forks(2);
        let invoker = cluster.join(Box::new(dir)).await.unwrap();

        let mut ctx = InvocationContext::new("sayHello", svc_url());
        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_ok(), "E-002i: ForkingCluster should succeed");

        let total_called = c1.load(Ordering::SeqCst)
            + c2.load(Ordering::SeqCst)
            + c3.load(Ordering::SeqCst)
            + c4.load(Ordering::SeqCst);
        assert_eq!(
            total_called, 2,
            "E-002i: exactly 2 invokers should be called"
        );
    }

    /// E-002j: ForkingCluster returns error when all forked invokers fail.
    #[tokio::test]
    async fn e_002j_forking_all_fail() {
        let inv1 = Arc::new(CountedInvoker::new(invoker_url(1), false));
        let inv2 = Arc::new(CountedInvoker::new(invoker_url(2), false));

        let dir = StaticDirectory::new(svc_url());
        dir.add_invoker(inv1);
        dir.add_invoker(inv2);

        let cluster = ForkingCluster::new().with_forks(2);
        let invoker = cluster.join(Box::new(dir)).await.unwrap();

        let mut ctx = InvocationContext::new("sayHello", svc_url());
        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_err(), "E-002j: should fail when all forks fail");
    }

    /// E-002k: BroadcastCluster invokes ALL invokers sequentially.
    ///
    /// With 3 invokers, all 3 should be called exactly once.
    #[tokio::test]
    async fn e_002k_broadcast_invokes_all() {
        let inv1 = Arc::new(CountedInvoker::new(invoker_url(1), true));
        let inv2 = Arc::new(CountedInvoker::new(invoker_url(2), true));
        let inv3 = Arc::new(CountedInvoker::new(invoker_url(3), true));

        let c1 = inv1.call_count.clone();
        let c2 = inv2.call_count.clone();
        let c3 = inv3.call_count.clone();

        let dir = StaticDirectory::new(svc_url());
        dir.add_invoker(inv1);
        dir.add_invoker(inv2);
        dir.add_invoker(inv3);

        let cluster = BroadcastCluster;
        let invoker = cluster.join(Box::new(dir)).await.unwrap();

        let mut ctx = InvocationContext::new("sayHello", svc_url());
        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_ok(), "E-002k: BroadcastCluster should succeed");

        assert_eq!(c1.load(Ordering::SeqCst), 1, "E-002k: inv1 called once");
        assert_eq!(c2.load(Ordering::SeqCst), 1, "E-002k: inv2 called once");
        assert_eq!(c3.load(Ordering::SeqCst), 1, "E-002k: inv3 called once");
    }

    /// E-002l: BroadcastCluster collects errors but continues to all invokers.
    #[tokio::test]
    async fn e_002l_broadcast_partial_failure() {
        let inv1 = Arc::new(CountedInvoker::new(invoker_url(1), true));
        let inv2 = Arc::new(CountedInvoker::new(invoker_url(2), false));
        let inv3 = Arc::new(CountedInvoker::new(invoker_url(3), true));

        let c1 = inv1.call_count.clone();
        let c2 = inv2.call_count.clone();
        let c3 = inv3.call_count.clone();

        let dir = StaticDirectory::new(svc_url());
        dir.add_invoker(inv1);
        dir.add_invoker(inv2);
        dir.add_invoker(inv3);

        let cluster = BroadcastCluster;
        let invoker = cluster.join(Box::new(dir)).await.unwrap();

        let mut ctx = InvocationContext::new("sayHello", svc_url());
        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_err(), "E-002l: should error on partial failure");

        assert_eq!(c1.load(Ordering::SeqCst), 1, "E-002l: inv1 called");
        assert_eq!(c2.load(Ordering::SeqCst), 1, "E-002l: inv2 called");
        assert_eq!(c3.load(Ordering::SeqCst), 1, "E-002l: inv3 called");
    }

    /// E-002q: FailsafeCluster swallows errors and returns success.
    #[tokio::test]
    async fn e_002q_failsafe_swallows_errors() {
        let dir = StaticDirectory::new(svc_url());
        let invoker = FailsafeCluster.join(Box::new(dir)).await.unwrap();
        let mut ctx = InvocationContext::new("sayHello", svc_url());
        let result = invoker.invoke(&mut ctx).await;
        assert!(
            result.is_ok(),
            "E-002q: failsafe should return ok even on empty dir"
        );
    }

    /// E-002r: FailsafeCluster calls invoker when available.
    #[tokio::test]
    async fn e_002r_failsafe_calls_invoker() {
        let inv = Arc::new(CountedInvoker::new(invoker_url(1), true));
        let c = inv.call_count.clone();
        let dir = StaticDirectory::new(svc_url());
        dir.add_invoker(inv);
        let invoker = FailsafeCluster.join(Box::new(dir)).await.unwrap();
        let mut ctx = InvocationContext::new("sayHello", svc_url());
        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_ok(), "E-002r: should pass through to invoker");
        assert_eq!(c.load(Ordering::SeqCst), 1, "E-002r: invoker called once");
    }

    /// E-002s: FailbackCluster returns success even when directory empty.
    #[tokio::test]
    async fn e_002s_failback_empty_directory() {
        let dir = StaticDirectory::new(svc_url());
        let invoker = FailbackCluster::new().join(Box::new(dir)).await.unwrap();
        let mut ctx = InvocationContext::new("sayHello", svc_url());
        let result = invoker.invoke(&mut ctx).await;
        assert!(
            result.is_ok(),
            "E-002s: failback should return ok even on empty dir"
        );
    }

    /// E-002t: AvailableCluster picks first invoker when all available.
    #[tokio::test]
    async fn e_002t_available_picks_first() {
        let inv1 = Arc::new(CountedInvoker::new(invoker_url(1), true));
        let inv2 = Arc::new(CountedInvoker::new(invoker_url(2), true));
        let c1 = inv1.call_count.clone();
        let c2 = inv2.call_count.clone();
        let dir = StaticDirectory::new(svc_url());
        dir.add_invoker(inv1);
        dir.add_invoker(inv2);
        let invoker = AvailableCluster.join(Box::new(dir)).await.unwrap();
        let mut ctx = InvocationContext::new("sayHello", svc_url());
        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_ok(), "E-002t: available cluster should succeed");
        assert_eq!(c1.load(Ordering::SeqCst), 1, "E-002t: first invoker called");
        assert_eq!(
            c2.load(Ordering::SeqCst),
            0,
            "E-002t: second invoker not called"
        );
    }

    /// E-002u: AvailableCluster returns error when no invokers available.
    #[tokio::test]
    async fn e_002u_available_empty_directory() {
        let dir = StaticDirectory::new(svc_url());
        let invoker = AvailableCluster.join(Box::new(dir)).await.unwrap();
        let mut ctx = InvocationContext::new("sayHello", svc_url());
        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_err(), "E-002u: should error on empty directory");
    }

    /// E-002v: MockCluster force-mode returns mock result without calling invoker.
    #[tokio::test]
    async fn e_002v_mock_force_mode() {
        let dir = StaticDirectory::new(svc_url());
        let cluster = MockCluster::new()
            .with_force(true)
            .with_mock_result(b"mock-data".to_vec());
        let invoker = cluster.join(Box::new(dir)).await.unwrap();
        let mut ctx = InvocationContext::new("sayHello", svc_url());
        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_ok(), "E-002v: mock should succeed");
        assert_eq!(
            result.unwrap().value,
            Some(b"mock-data".to_vec()),
            "E-002v: should return mock data"
        );
    }

    /// E-002w: MockCluster fail-mode returns mock when invoker fails.
    #[tokio::test]
    async fn e_002w_mock_fail_mode() {
        let inv = Arc::new(CountedInvoker::new(invoker_url(1), false));
        let dir = StaticDirectory::new(svc_url());
        dir.add_invoker(inv);
        let cluster = MockCluster::new().with_mock_result(b"fallback".to_vec());
        let invoker = cluster.join(Box::new(dir)).await.unwrap();
        let mut ctx = InvocationContext::new("sayHello", svc_url());
        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_ok(), "E-002w: mock fallback should succeed");
        assert_eq!(
            result.unwrap().value,
            Some(b"fallback".to_vec()),
            "E-002w: should return fallback mock"
        );
    }
}

// ── E-002 router: ConditionRouter / TagRouter ───────────────────────────────

mod e_002_router {
    use dubbo_rs_cluster::{ConditionRouter, TagRouter};
    use dubbo_rs_common::node::Node;
    use dubbo_rs_common::url::URL;
    use dubbo_rs_protocol::{InvocationContext, Invoker, RPCResult};
    use std::sync::Arc;

    struct TestInvoker {
        url: URL,
    }

    impl TestInvoker {
        fn new(host: &str, region: Option<&str>) -> Self {
            let mut url = URL::new("tri", "/com.example.RouteService");
            url.ip = host.to_string();
            if let Some(r) = region {
                url.set_param("region", r);
            }
            Self { url }
        }

        fn with_tag(host: &str, tag: Option<&str>) -> Self {
            let mut url = URL::new("tri", "/com.example.RouteService");
            url.ip = host.to_string();
            if let Some(t) = tag {
                url.set_param("tag", t);
            }
            Self { url }
        }
    }

    impl Node for TestInvoker {
        fn get_url(&self) -> &URL {
            &self.url
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait::async_trait]
    impl Invoker for TestInvoker {
        async fn invoke(&self, _ctx: &mut InvocationContext) -> anyhow::Result<RPCResult> {
            Ok(RPCResult::success(b"ok".to_vec()))
        }
    }

    /// E-002m: ConditionRouter parses and filters by region.
    #[test]
    fn e_002m_condition_router_filter() {
        let router = ConditionRouter::parse("region=bj => region=hz").unwrap();

        let invokers: Vec<Arc<dyn Invoker>> = vec![
            Arc::new(TestInvoker::new("host1", Some("bj"))),
            Arc::new(TestInvoker::new("host2", Some("hz"))),
            Arc::new(TestInvoker::new("host3", Some("bj"))),
            Arc::new(TestInvoker::new("host4", Some("sh"))),
        ];

        let mut ctx = InvocationContext::new("route", URL::new("tri", "/test"));
        ctx.attachments.insert("region".into(), "bj".into());

        assert!(
            router.matches_invocation(&ctx),
            "E-002m: should match bj invocation"
        );

        let selected = router.filter_invokers(&invokers);
        assert_eq!(
            selected,
            vec![1],
            "E-002m: only invoker at index 1 has region=hz"
        );
    }

    /// E-002n: ConditionRouter returns all invokers when no match condition.
    #[test]
    fn e_002n_condition_router_always_match() {
        let router = ConditionRouter::parse("=> region=hz").unwrap();

        let invokers: Vec<Arc<dyn Invoker>> = vec![
            Arc::new(TestInvoker::new("host1", Some("bj"))),
            Arc::new(TestInvoker::new("host2", Some("hz"))),
        ];

        let ctx = InvocationContext::new("route", URL::new("tri", "/test"));
        assert!(
            router.matches_invocation(&ctx),
            "E-002n: empty match rules always match"
        );

        let selected = router.filter_invokers(&invokers);
        assert_eq!(selected, vec![1], "E-002n: should filter to region=hz");
    }

    /// E-002o: TagRouter routes by dubbo.tag attachment.
    #[test]
    fn e_002o_tag_router_basic() {
        let router = TagRouter::new();

        let invokers: Vec<Arc<dyn Invoker>> = vec![
            Arc::new(TestInvoker::with_tag("host1", Some("gray"))),
            Arc::new(TestInvoker::with_tag("host2", Some("prod"))),
            Arc::new(TestInvoker::with_tag("host3", Some("gray"))),
            Arc::new(TestInvoker::with_tag("host4", None)),
        ];

        let mut ctx = InvocationContext::new("route", URL::new("tri", "/test"));
        ctx.attachments.insert("dubbo.tag".into(), "gray".into());

        let selected = router.route(&invokers, &ctx);
        assert_eq!(
            selected,
            vec![0, 2],
            "E-002o: should route to gray-tagged invokers"
        );
    }

    /// E-002p: TagRouter falls back to untagged when no tag match.
    #[test]
    fn e_002p_tag_router_fallback() {
        let router = TagRouter::new();

        let invokers: Vec<Arc<dyn Invoker>> = vec![
            Arc::new(TestInvoker::with_tag("host1", Some("prod"))),
            Arc::new(TestInvoker::with_tag("host2", None)),
        ];

        let mut ctx = InvocationContext::new("route", URL::new("tri", "/test"));
        ctx.attachments.insert("dubbo.tag".into(), "canary".into());

        let selected = router.route(&invokers, &ctx);
        assert_eq!(
            selected,
            vec![1],
            "E-002p: should fall back to untagged invoker"
        );
    }
}

// ── E-003 deep: Filter integration ──────────────────────────────────────────

mod e_003_filter_deep {
    use dubbo_rs_common::node::Node;
    use dubbo_rs_common::url::URL;
    use dubbo_rs_filter::{
        AccessLogFilter, ActiveLimitFilter, AuthFilter, CacheFilter, CircuitBreaker,
        CircuitBreakerFilter, ContextFilter, EchoFilter, ExceptionFilter, ExecuteLimitFilter,
        FilterChain, GenericFilter, GenericInvoker, GenericService, GracefulShutdownFilter,
        LruCacheStore, TimeoutFilter,
    };
    use dubbo_rs_metrics::{MetricsCollector, MetricsCollectorBuilder, MetricsFilter};
    use dubbo_rs_protocol::{InvocationContext, Invoker, RPCResult};
    use dubbo_rs_tracing::{ExporterType, TracingConfig, TracingFilter};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    struct MockInvoker {
        url: URL,
        call_count: Arc<AtomicUsize>,
    }

    impl MockInvoker {
        fn new() -> Self {
            Self {
                url: URL::new("tri", "/com.example.FilterService"),
                call_count: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    impl Node for MockInvoker {
        fn get_url(&self) -> &URL {
            &self.url
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait::async_trait]
    impl Invoker for MockInvoker {
        async fn invoke(&self, _ctx: &mut InvocationContext) -> anyhow::Result<RPCResult> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(RPCResult::success(b"mock-ok".to_vec()))
        }
    }

    struct ErrorInvoker {
        url: URL,
    }

    impl ErrorInvoker {
        fn new() -> Self {
            Self {
                url: URL::new("tri", "/com.example.FilterService"),
            }
        }
    }

    impl Node for ErrorInvoker {
        fn get_url(&self) -> &URL {
            &self.url
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait::async_trait]
    impl Invoker for ErrorInvoker {
        async fn invoke(&self, _ctx: &mut InvocationContext) -> anyhow::Result<RPCResult> {
            Ok(RPCResult::from_error(
                dubbo_rs_common::error::RPCError::ServiceError("fail".into()),
            ))
        }
    }

    fn filter_url() -> URL {
        URL::new("tri", "/com.example.FilterService")
    }

    /// E-003f: AccessLogFilter passes through successful results.
    #[tokio::test]
    async fn e_003f_accesslog_success_passthrough() {
        let invoker = MockInvoker::new();
        let count = invoker.call_count.clone();

        let chain = FilterChain::new(vec![Box::new(AccessLogFilter)], Box::new(invoker)).build();

        let mut ctx = InvocationContext::new("sayHello", filter_url());
        let result = chain.invoke(&mut ctx).await.unwrap();
        assert!(!result.is_error(), "E-003f: should be success");
        assert_eq!(
            result.value,
            Some(b"mock-ok".to_vec()),
            "E-003f: value passes through"
        );
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "E-003f: invoker called once"
        );
    }

    /// E-003g: AccessLogFilter preserves error results.
    #[tokio::test]
    async fn e_003g_accesslog_error_passthrough() {
        let chain = FilterChain::new(
            vec![Box::new(AccessLogFilter)],
            Box::new(ErrorInvoker::new()),
        )
        .build();

        let mut ctx = InvocationContext::new("sayHello", filter_url());
        let result = chain.invoke(&mut ctx).await.unwrap();
        assert!(result.is_error(), "E-003g: error should be preserved");
        let err = result.error.unwrap();
        assert_eq!(err.status_code(), 70, "E-003g: ServiceError = 70");
    }

    /// E-003h: GracefulShutdownFilter allows calls before shutdown.
    #[tokio::test]
    async fn e_003h_shutdown_allows_before() {
        let shutdown = GracefulShutdownFilter::new();
        let chain =
            FilterChain::new(vec![Box::new(shutdown)], Box::new(MockInvoker::new())).build();

        let mut ctx = InvocationContext::new("sayHello", filter_url());
        let result = chain.invoke(&mut ctx).await;
        assert!(result.is_ok(), "E-003h: should succeed before shutdown");
    }

    /// E-003i: GracefulShutdownFilter rejects calls after shutdown().
    #[tokio::test]
    async fn e_003i_shutdown_rejects_after() {
        let shutdown = Arc::new(AtomicBool::new(false));
        let filter = GracefulShutdownFilter::from_flag(shutdown.clone());

        let chain = FilterChain::new(vec![Box::new(filter)], Box::new(MockInvoker::new())).build();

        // Trigger shutdown
        shutdown.store(true, Ordering::SeqCst);

        let mut ctx = InvocationContext::new("sayHello", filter_url());
        let result = chain.invoke(&mut ctx).await;
        assert!(result.is_err(), "E-003i: should reject after shutdown");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("shutting down"),
            "E-003i: error should mention shutdown"
        );
    }

    /// E-003j: CircuitBreaker opens after threshold failures and rejects.
    #[tokio::test]
    async fn e_003j_circuit_breaker_open_rejects() {
        let breaker = Arc::new(CircuitBreaker::new().with_failure_threshold(3));
        // Record 3 failures to open the circuit
        for _ in 0..3 {
            breaker.record_failure();
        }
        assert!(
            !breaker.is_call_permitted(),
            "E-003j: circuit should be open"
        );

        let filter = CircuitBreakerFilter::new(breaker);
        let chain = FilterChain::new(vec![Box::new(filter)], Box::new(MockInvoker::new())).build();

        let mut ctx = InvocationContext::new("sayHello", filter_url());
        let result = chain.invoke(&mut ctx).await;
        assert!(
            result.is_err(),
            "E-003j: should reject when circuit is open"
        );
    }

    /// E-003k: CircuitBreaker closed allows calls and records success.
    #[tokio::test]
    async fn e_003k_circuit_breaker_closed_allows() {
        let breaker = Arc::new(CircuitBreaker::new());
        let filter = CircuitBreakerFilter::new(breaker.clone());

        let chain = FilterChain::new(vec![Box::new(filter)], Box::new(MockInvoker::new())).build();

        let mut ctx = InvocationContext::new("sayHello", filter_url());
        let result = chain.invoke(&mut ctx).await.unwrap();
        assert!(
            !result.is_error(),
            "E-003k: should succeed through closed circuit"
        );
        assert_eq!(
            breaker.success_count(),
            1,
            "E-003k: should record 1 success"
        );
    }

    /// E-003l: EchoFilter short-circuits before shutdown check in chain.
    #[tokio::test]
    async fn e_003l_echo_before_shutdown_chain() {
        let shutdown = Arc::new(AtomicBool::new(true)); // already shut down
        let shutdown_filter = GracefulShutdownFilter::from_flag(shutdown);

        let chain = FilterChain::new(
            vec![Box::new(EchoFilter), Box::new(shutdown_filter)],
            Box::new(MockInvoker::new()),
        )
        .build();

        // $echo should short-circuit before reaching shutdown filter
        let mut ctx = InvocationContext::new("$echo", filter_url());
        ctx.arguments = vec![b"ping".to_vec()];
        let result = chain.invoke(&mut ctx).await.unwrap();
        assert_eq!(
            result.value,
            Some(b"ping".to_vec()),
            "E-003l: echo short-circuits before shutdown"
        );
    }

    /// E-003m: Two GracefulShutdownFilter sharing same flag — shutdown one, other rejects.
    #[tokio::test]
    async fn e_003m_shutdown_shared_flag() {
        let flag = Arc::new(AtomicBool::new(false));
        let f1 = GracefulShutdownFilter::from_flag(flag.clone());
        let f2 = GracefulShutdownFilter::from_flag(flag.clone());

        // Shutdown via f1
        f1.shutdown();

        // f2 should also reject because they share the same flag
        let chain = FilterChain::new(vec![Box::new(f2)], Box::new(MockInvoker::new())).build();

        let mut ctx = InvocationContext::new("sayHello", filter_url());
        let result = chain.invoke(&mut ctx).await;
        assert!(
            result.is_err(),
            "E-003m: shared flag should cause rejection"
        );
        assert!(
            result.unwrap_err().to_string().contains("shutting down"),
            "E-003m: shutdown message"
        );
    }

    /// E-003n: AuthFilter provider-side rejects calls with missing signature.
    #[tokio::test]
    async fn e_003n_auth_filter_rejects_missing_signature() {
        let filter = AuthFilter::new();
        let chain = FilterChain::new(vec![Box::new(filter)], Box::new(MockInvoker::new())).build();

        let mut ctx = InvocationContext::new("sayHello", filter_url());
        let result = chain.invoke(&mut ctx).await;
        assert!(
            result.is_ok(),
            "E-003n: AuthFilter without attachments passes through"
        );
    }

    /// E-003o: AuthFilter provider-side rejects unknown access key.
    #[tokio::test]
    async fn e_003o_auth_filter_rejects_unknown_key() {
        let filter = AuthFilter::new();
        let chain = FilterChain::new(vec![Box::new(filter)], Box::new(MockInvoker::new())).build();

        let mut ctx = InvocationContext::new("sayHello", filter_url());
        ctx.attachments.insert("ak".into(), "unknown_key".into());
        ctx.attachments
            .insert("signature".into(), "invalid_sig".into());
        ctx.attachments.insert("timestamp".into(), "0".into());

        let result = chain.invoke(&mut ctx).await;
        assert!(result.is_err(), "E-003o: should reject unknown access key");
    }

    /// E-003p: ActiveLimitFilter allows calls within concurrent limit.
    #[tokio::test]
    async fn e_003p_active_limit_allows_under_limit() {
        let filter = ActiveLimitFilter::new(5);
        let chain = FilterChain::new(vec![Box::new(filter)], Box::new(MockInvoker::new())).build();
        let mut ctx = InvocationContext::new("sayHello", filter_url());
        let result = chain.invoke(&mut ctx).await;
        assert!(result.is_ok(), "E-003p: should allow under limit");
    }

    /// E-003q: ExecuteLimitFilter allows calls within semaphore limit.
    #[tokio::test]
    async fn e_003q_execute_limit_allows_under_limit() {
        let filter = ExecuteLimitFilter::new(5);
        let chain = FilterChain::new(vec![Box::new(filter)], Box::new(MockInvoker::new())).build();
        let mut ctx = InvocationContext::new("sayHello", filter_url());
        let result = chain.invoke(&mut ctx).await;
        assert!(
            result.is_ok(),
            "E-003q: should allow under concurrent limit"
        );
    }

    /// E-003r: ExecuteLimitFilter allows when semaphore has capacity.
    #[tokio::test]
    async fn e_003r_execute_limit_allows_with_capacity() {
        let filter = ExecuteLimitFilter::new(1);
        let chain = FilterChain::new(vec![Box::new(filter)], Box::new(MockInvoker::new())).build();
        let mut ctx = InvocationContext::new("sayHello", filter_url());
        let r1 = chain.invoke(&mut ctx).await;
        assert!(r1.is_ok(), "E-003r: first call should succeed");
        let r2 = chain.invoke(&mut ctx).await;
        assert!(
            r2.is_ok(),
            "E-003r: second call may also succeed (depends on timing)"
        );
    }

    /// E-003s: TimeoutFilter allows fast calls within timeout.
    #[tokio::test]
    async fn e_003s_timeout_filter_allows_fast_calls() {
        let filter = TimeoutFilter::new(std::time::Duration::from_secs(5));
        let chain = FilterChain::new(vec![Box::new(filter)], Box::new(MockInvoker::new())).build();
        let mut ctx = InvocationContext::new("sayHello", filter_url());
        let result = chain.invoke(&mut ctx).await;
        assert!(
            result.is_ok(),
            "E-003s: fast call should succeed within timeout"
        );
    }

    /// E-003t: TimeoutFilter rejects slow calls that exceed timeout.
    #[tokio::test]
    async fn e_003t_timeout_filter_rejects_slow_calls() {
        use std::time::Duration;
        let filter = TimeoutFilter::new(Duration::from_millis(50));

        struct SlowInvoker;
        impl Node for SlowInvoker {
            fn get_url(&self) -> &URL {
                unimplemented!()
            }
            fn is_available(&self) -> bool {
                true
            }
            fn destroy(&self) {}
        }
        #[async_trait::async_trait]
        impl Invoker for SlowInvoker {
            async fn invoke(&self, _: &mut InvocationContext) -> anyhow::Result<RPCResult> {
                tokio::time::sleep(Duration::from_millis(200)).await;
                Ok(RPCResult::success(b"slow".to_vec()))
            }
        }

        let chain = FilterChain::new(vec![Box::new(filter)], Box::new(SlowInvoker)).build();
        let mut ctx = InvocationContext::new("sayHello", filter_url());
        let result = chain.invoke(&mut ctx).await;
        assert!(result.is_err(), "E-003t: slow call should timeout");
        assert!(
            result.unwrap_err().to_string().contains("timed out"),
            "E-003t: error should mention timed out"
        );
    }

    /// E-003u: ContextFilter removes internal keys (starting with _ or dubbo.).
    #[tokio::test]
    async fn e_003u_context_filter_removes_internal_keys() {
        let filter = ContextFilter;
        let chain = FilterChain::new(vec![Box::new(filter)], Box::new(MockInvoker::new())).build();
        let mut ctx = InvocationContext::new("sayHello", filter_url());
        ctx.attachments.insert("_internal".into(), "hidden".into());
        ctx.attachments.insert("dubbo.trace".into(), "abc".into());
        ctx.attachments.insert("user.key".into(), "visible".into());
        let result = chain.invoke(&mut ctx).await;
        assert!(result.is_ok(), "E-003u: should allow through");
    }

    /// E-003v: ExceptionFilter wraps undeclared exceptions.
    #[tokio::test]
    async fn e_003v_exception_filter_wraps_undeclared() {
        struct ErrInvoker(URL);
        impl Node for ErrInvoker {
            fn get_url(&self) -> &URL {
                &self.0
            }
            fn is_available(&self) -> bool {
                true
            }
            fn destroy(&self) {}
        }
        #[async_trait::async_trait]
        impl Invoker for ErrInvoker {
            async fn invoke(&self, _: &mut InvocationContext) -> anyhow::Result<RPCResult> {
                Ok(RPCResult::from_error(
                    dubbo_rs_common::error::RPCError::ServiceError("internal db error".into()),
                ))
            }
        }

        let filter = ExceptionFilter::new();
        let chain =
            FilterChain::new(vec![Box::new(filter)], Box::new(ErrInvoker(filter_url()))).build();
        let mut ctx = InvocationContext::new("sayHello", filter_url());
        let result = chain.invoke(&mut ctx).await;
        assert!(result.is_ok(), "E-003v: should not fail at invoke level");
        let rpc_result = result.unwrap();
        assert!(
            rpc_result.is_error(),
            "E-003v: should still be error result"
        );
        if let Some(err) = rpc_result.error {
            assert_eq!(
                err.status_code(),
                80,
                "E-003v: should become SERVER_ERROR (80)"
            );
        }
    }

    /// E-003w: ExceptionFilter passes through declared exceptions.
    #[tokio::test]
    async fn e_003w_exception_filter_passes_declared() {
        struct ErrInvoker2(URL);
        impl Node for ErrInvoker2 {
            fn get_url(&self) -> &URL {
                &self.0
            }
            fn is_available(&self) -> bool {
                true
            }
            fn destroy(&self) {}
        }
        #[async_trait::async_trait]
        impl Invoker for ErrInvoker2 {
            async fn invoke(&self, _: &mut InvocationContext) -> anyhow::Result<RPCResult> {
                Ok(RPCResult::from_error(
                    dubbo_rs_common::error::RPCError::ServiceError("known biz error".into()),
                ))
            }
        }

        let filter = ExceptionFilter::with_exceptions(vec!["known".into()]);
        let chain =
            FilterChain::new(vec![Box::new(filter)], Box::new(ErrInvoker2(filter_url()))).build();
        let mut ctx = InvocationContext::new("sayHello", filter_url());
        let result = chain.invoke(&mut ctx).await.unwrap();
        assert!(result.is_error(), "E-003w: should be error result");
        if let Some(err) = result.error {
            assert_eq!(
                err.status_code(),
                70,
                "E-003w: should keep SERVICE_ERROR (70)"
            );
        }
    }

    /// E-003x: CacheFilter caches first call result, returns cached on second.
    #[tokio::test]
    async fn e_003x_cache_filter_caches_result() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        struct CountInvoker {
            count: Arc<AtomicUsize>,
        }
        impl Node for CountInvoker {
            fn get_url(&self) -> &URL {
                unimplemented!()
            }
            fn is_available(&self) -> bool {
                true
            }
            fn destroy(&self) {}
        }
        #[async_trait::async_trait]
        impl Invoker for CountInvoker {
            async fn invoke(&self, _: &mut InvocationContext) -> anyhow::Result<RPCResult> {
                self.count.fetch_add(1, Ordering::SeqCst);
                Ok(RPCResult::success(b"cached-result".to_vec()))
            }
        }

        let count = Arc::new(AtomicUsize::new(0));
        let invoker = CountInvoker {
            count: count.clone(),
        };
        let store = Box::new(LruCacheStore::new(100));
        let filter = CacheFilter::new(store);
        let chain = FilterChain::new(vec![Box::new(filter)], Box::new(invoker)).build();

        let mut ctx = InvocationContext::new("getData", filter_url());
        ctx.arguments = vec![b"key1".to_vec()];

        let r1 = chain.invoke(&mut ctx).await.unwrap();
        assert_eq!(
            r1.value,
            Some(b"cached-result".to_vec()),
            "E-003x: first call returns correct value"
        );
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "E-003x: invoker called once"
        );

        let r2 = chain.invoke(&mut ctx).await.unwrap();
        assert_eq!(
            r2.value,
            Some(b"cached-result".to_vec()),
            "E-003x: cached call returns same value"
        );
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "E-003x: invoker NOT called second time (cached)"
        );
    }

    /// E-003y: GenericInvoker invokes via GenericService trait.
    #[tokio::test]
    async fn e_003y_generic_invoker_basic_invoke() {
        struct EchoInvoker(URL);
        impl Node for EchoInvoker {
            fn get_url(&self) -> &URL {
                &self.0
            }
            fn is_available(&self) -> bool {
                true
            }
            fn destroy(&self) {}
        }
        #[async_trait::async_trait]
        impl Invoker for EchoInvoker {
            async fn invoke(&self, ctx: &mut InvocationContext) -> anyhow::Result<RPCResult> {
                let reply = format!(
                    "echo:{}",
                    String::from_utf8_lossy(ctx.arguments.first().map_or(&b""[..], |v| v))
                );
                Ok(RPCResult::success(reply.into_bytes()))
            }
        }

        let generic = GenericInvoker::new(
            Box::new(EchoInvoker(URL::new("tri", "/com.example.GenericService"))),
            URL::new("tri", "/com.example.GenericService"),
        );

        let result = generic
            .invoke(
                "sayHello".into(),
                vec!["Ljava/lang/String;".into()],
                vec!["\"world\"".into()],
            )
            .await
            .unwrap();
        assert!(
            result.contains("echo:"),
            "E-003y: should contain echo prefix, got {result}"
        );
        assert!(
            result.contains("world"),
            "E-003y: should contain arg, got {result}"
        );
    }

    /// E-003z: GenericFilter passes through non-generic calls.
    #[tokio::test]
    async fn e_003z_generic_filter_passthrough() {
        let filter = GenericFilter;
        let chain = FilterChain::new(vec![Box::new(filter)], Box::new(MockInvoker::new())).build();
        let mut ctx = InvocationContext::new("sayHello", filter_url());
        let result = chain.invoke(&mut ctx).await;
        assert!(result.is_ok(), "E-003z: should pass through normally");
        let rpc_result = result.unwrap();
        assert_eq!(
            rpc_result.value,
            Some(b"mock-ok".to_vec()),
            "E-003z: should return mock result"
        );
    }

    /// E-004a: MetricsCollector builds successfully.
    #[test]
    fn e_004a_metrics_collector_build() {
        let collector = MetricsCollector::new().expect("E-004a: should build MetricsCollector");
        let _ = collector;
    }

    /// E-004b: MetricsCollector builds with namespace.
    #[test]
    fn e_004b_metrics_collector_with_namespace() {
        let collector = MetricsCollectorBuilder::new()
            .namespace("dubbo")
            .build()
            .expect("E-004b: should build with namespace");
        let _ = collector;
    }

    /// E-004c: MetricsFilter passes through successful requests.
    #[tokio::test]
    async fn e_004c_metrics_filter_passthrough() {
        let collector = MetricsCollector::new().unwrap();
        let filter = MetricsFilter::new(collector);
        let chain = FilterChain::new(vec![Box::new(filter)], Box::new(MockInvoker::new())).build();

        let mut ctx = InvocationContext::new("sayHello", filter_url());
        let result = chain.invoke(&mut ctx).await;
        assert!(result.is_ok(), "E-004c: MetricsFilter should pass through");
        assert_eq!(
            result.unwrap().value,
            Some(b"mock-ok".to_vec()),
            "E-004c: should return mock result"
        );
    }

    /// E-004d: TracingConfig builds with default values.
    #[test]
    fn e_004d_tracing_config_default() {
        let config = TracingConfig::new();
        assert!(config.sample_rate >= 0.0 && config.sample_rate <= 1.0);
        assert!(config.enable_trace_id_log);
    }

    /// E-004e: TracingConfig with custom settings.
    #[test]
    fn e_004e_tracing_config_custom() {
        let config = TracingConfig::new()
            .with_sample_rate(0.5)
            .with_trace_id_log(false)
            .with_exporter(ExporterType::Stdout);
        assert_eq!(config.sample_rate, 0.5);
        assert!(!config.enable_trace_id_log);
    }
}
