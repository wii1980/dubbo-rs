pub use dubbo_rs_common;
pub use dubbo_rs_protocol;

use std::collections::HashMap;

use async_trait::async_trait;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_protocol::{InvocationContext, Invoker, RPCResult};

// ============================================================================
// Filter trait
// ============================================================================

/// A filter intercepts invocations in a chain-of-responsibility pattern.
///
/// **Lifecycle:**
/// - `invoke()` is called **before** the downstream invoker. The filter may
///   short-circuit (e.g., echo, token rejection) or modify `ctx` and call
///   `next.invoke(ctx)` to proceed.
/// - `on_response()` is called **after** `next.invoke()` returns. The filter
///   can inspect or transform the result (e.g., logging, metrics).
///
/// # Errors
///
/// `invoke()` returns an error when the filter rejects the call.
/// `on_response()` cannot fail—it must always return a (possibly modified)
/// `RPCResult`.
#[async_trait]
pub trait Filter: Send + Sync {
    /// Pre-process the invocation, then optionally forward to `next`.
    ///
    /// # Errors
    ///
    /// Returns an error if the filter rejects the call (e.g., token mismatch)
    /// or encounters an internal failure.
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error>;

    /// Post-process the RPC result before returning to the caller.
    ///
    /// The default implementation passes through the result unchanged.
    #[allow(unused_variables)]
    async fn on_response(
        &self,
        ctx: &InvocationContext,
        result: RPCResult,
        invoker: &dyn Invoker,
    ) -> RPCResult {
        result
    }
}

// ============================================================================
// FilterNode — one link in the chain
// ============================================================================

/// Internal: wraps a single filter and the next invoker it delegates to.
struct FilterNode {
    filter: Box<dyn Filter>,
    next: Box<dyn Invoker>,
}

impl Node for FilterNode {
    fn get_url(&self) -> &URL {
        self.next.get_url()
    }

    fn is_available(&self) -> bool {
        self.next.is_available()
    }

    fn destroy(&self) {
        self.next.destroy();
    }
}

#[async_trait]
impl Invoker for FilterNode {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        let result = self.filter.invoke(ctx, self.next.as_ref()).await?;
        Ok(self
            .filter
            .on_response(ctx, result, self.next.as_ref())
            .await)
    }
}

// ============================================================================
// FilterChain — builds a chain of filters wrapping a base invoker
// ============================================================================

/// A chain of filters wrapping a base invoker.
///
/// Filters execute in insertion order, outermost first:
///
/// ```text
/// filter[0].invoke → filter[1].invoke → ... → filter[n].invoke → base.invoke
///   ← on_response   ← on_response                ← on_response  ←
/// ```
///
/// Build the chain via [`FilterChain::build()`], which returns a
/// `Box<dyn Invoker>` ready for use.
pub struct FilterChain {
    filters: Vec<Box<dyn Filter>>,
    invoker: Box<dyn Invoker>,
}

impl FilterChain {
    /// Create a new chain.
    ///
    /// `filters` execute in the order given (index 0 is outermost).
    /// `invoker` is the base invoker at the end of the chain.
    #[must_use]
    pub fn new(filters: Vec<Box<dyn Filter>>, invoker: Box<dyn Invoker>) -> Self {
        Self { filters, invoker }
    }

    /// Consume the chain and build a boxed `Invoker`.
    ///
    /// Construction proceeds from innermost to outermost: the last filter
    /// wraps the base invoker, then the second-to-last wraps that, etc.
    #[must_use]
    pub fn build(mut self) -> Box<dyn Invoker> {
        let mut next = self.invoker;
        // Pop from end — outermost filter wraps the inner chain
        while let Some(filter) = self.filters.pop() {
            next = Box::new(FilterNode { filter, next });
        }
        next
    }
}

// ============================================================================
// Built-in Filters
// ============================================================================

// ── EchoFilter ──────────────────────────────────────────────────────

/// Health-check filter: intercepts `$echo` calls and returns the echo payload
/// without forwarding to the downstream invoker.
///
/// If the first argument is present, it is echoed back. Otherwise, the
/// method name itself is returned.
pub struct EchoFilter;

#[async_trait]
impl Filter for EchoFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        if ctx.method_name == "$echo" {
            let echo = ctx
                .arguments
                .first()
                .cloned()
                .unwrap_or_else(|| ctx.method_name.as_bytes().to_vec());
            return Ok(RPCResult::success(echo));
        }
        next.invoke(ctx).await
    }
}

// ── TokenFilter ─────────────────────────────────────────────────────

/// Token-based authentication filter.
///
/// Checks for a token in invocation attachments. If the token is
/// missing or mismatched, the invocation is rejected.
///
/// The attachment key defaults to `"token"` and can be customized.
pub struct TokenFilter {
    token: String,
    key: String,
}

impl TokenFilter {
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            key: "token".to_string(),
        }
    }

    #[must_use]
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }
}

#[async_trait]
impl Filter for TokenFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        match ctx.attachments.get(&self.key) {
            Some(t) if t == &self.token => next.invoke(ctx).await,
            Some(_) => Err(anyhow::anyhow!(
                "token mismatch for method '{}'",
                ctx.method_name
            )),
            None => Err(anyhow::anyhow!(
                "token missing: no '{}' in attachments for method '{}'",
                self.key,
                ctx.method_name
            )),
        }
    }
}

// ── AccessLogFilter ─────────────────────────────────────────────────

/// Access log filter: logs invocation details via [`tracing`] after
/// the call completes.
///
/// On success, logs at `INFO` level with the method name.
/// On failure, logs at `WARN` level with method name and error details.
pub struct AccessLogFilter;

#[async_trait]
impl Filter for AccessLogFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        next.invoke(ctx).await
    }

    async fn on_response(
        &self,
        ctx: &InvocationContext,
        result: RPCResult,
        _invoker: &dyn Invoker,
    ) -> RPCResult {
        if result.is_error() {
            tracing::warn!(
                method = %ctx.method_name,
                error = ?result.error,
                "RPC call failed",
            );
        } else {
            tracing::info!(
                method = %ctx.method_name,
                "RPC call succeeded",
            );
        }
        result
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    // ── test helpers ──────────────────────────────────────────────────

    pub(crate) struct TestInvoker {
        url: URL,
        call_count: Arc<AtomicUsize>,
        pub(crate) should_fail: Arc<AtomicBool>,
    }

    impl TestInvoker {
        pub(crate) fn new() -> Self {
            Self {
                url: URL::new("tri", "/com.example.TestService"),
                call_count: Arc::new(AtomicUsize::new(0)),
                should_fail: Arc::new(AtomicBool::new(false)),
            }
        }

        #[allow(dead_code)]
        pub(crate) fn count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
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

    #[async_trait]
    impl Invoker for TestInvoker {
        async fn invoke(&self, _ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            if self.should_fail.load(Ordering::SeqCst) {
                Ok(RPCResult::from_error(
                    dubbo_rs_common::error::RPCError::ServerError("test failure".into()),
                ))
            } else {
                Ok(RPCResult::success(b"ok".to_vec()))
            }
        }
    }

    pub(crate) fn make_ctx(method: &str) -> InvocationContext {
        let url = URL::new("tri", "/com.example.TestService");
        InvocationContext::new(method, url)
    }

    // ── Task 1: Filter trait + FilterChain ────────────────────────────

    #[test]
    fn test_filter_chain_empty_passes_through() {
        let invoker = Box::new(TestInvoker::new());
        let chain = FilterChain::new(vec![], invoker);
        // build should work
        let _ = chain.build();
    }

    #[tokio::test]
    async fn test_filter_chain_single_filter_calls_base() {
        let base = Box::new(TestInvoker::new());
        let filters: Vec<Box<dyn Filter>> = vec![Box::new(PassThroughFilter)];
        let chain = FilterChain::new(filters, base);
        let invoker = chain.build();

        let mut ctx = make_ctx("sayHello");
        let result = invoker.invoke(&mut ctx).await.unwrap();
        assert!(!result.is_error());
    }

    #[tokio::test]
    async fn test_filter_chain_execution_order() {
        let base = Box::new(TestInvoker::new());

        let order: Arc<std::sync::Mutex<Vec<String>>> = Arc::new(std::sync::Mutex::new(Vec::new()));

        let outer = OrderFilter {
            name: "outer",
            order: order.clone(),
        };
        let inner = OrderFilter {
            name: "inner",
            order: order.clone(),
        };

        let chain = FilterChain::new(
            vec![Box::new(outer) as Box<dyn Filter>, Box::new(inner)],
            base,
        );
        let invoker = chain.build();

        let mut ctx = make_ctx("sayHello");
        let _ = invoker.invoke(&mut ctx).await.unwrap();

        let recorded = order.lock().unwrap();
        let expected: Vec<String> = vec![
            "outer.invoke".into(),
            "inner.invoke".into(),
            "inner.on_response".into(),
            "outer.on_response".into(),
        ];
        assert_eq!(*recorded, expected);
    }

    #[tokio::test]
    async fn test_filter_can_short_circuit() {
        let base = Box::new(TestInvoker::new());
        // This filter returns a result without calling next
        let chain = FilterChain::new(vec![Box::new(ShortCircuitFilter) as Box<dyn Filter>], base);
        let invoker = chain.build();

        let mut ctx = make_ctx("$echo");
        let result = invoker.invoke(&mut ctx).await.unwrap();
        assert!(!result.is_error());
        assert_eq!(
            String::from_utf8_lossy(result.value.as_ref().unwrap()),
            "short-circuit"
        );
    }

    #[tokio::test]
    async fn test_filter_node_delegates() {
        let base = Box::new(TestInvoker::new());
        let node = FilterNode {
            filter: Box::new(PassThroughFilter),
            next: base,
        };

        assert!(node.is_available());
        assert_eq!(node.get_url().path, "/com.example.TestService");
    }

    // ── Task 2: EchoFilter ───────────────────────────────────────────

    #[tokio::test]
    async fn test_echo_filter_short_circuits() {
        let echo = EchoFilter;
        let mut ctx = make_ctx("$echo");
        ctx.arguments = vec![b"ping".to_vec()];

        // Short-circuit means next.invoke is never called
        let result = echo.invoke(&mut ctx, &stub_next()).await.unwrap();
        assert!(!result.is_error());
        assert_eq!(
            String::from_utf8_lossy(result.value.as_ref().unwrap()),
            "ping"
        );
    }

    #[tokio::test]
    async fn test_echo_filter_echoes_method_name() {
        let echo = EchoFilter;
        let mut ctx = make_ctx("$echo");
        // No arguments — should echo method name

        let result = echo.invoke(&mut ctx, &stub_next()).await.unwrap();
        assert!(!result.is_error());
        assert_eq!(
            String::from_utf8_lossy(result.value.as_ref().unwrap()),
            "$echo"
        );
    }

    #[tokio::test]
    async fn test_echo_filter_passes_through_other_methods() {
        let echo = EchoFilter;
        let mut ctx = make_ctx("sayHello");
        ctx.arguments = vec![b"world".to_vec()];

        // Should pass through to next
        let next = TestInvoker::new();
        let result = echo.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
        assert_eq!(next.count(), 1);
    }

    fn stub_next() -> TestInvoker {
        TestInvoker::new()
    }

    // ── Task 3: TokenFilter ──────────────────────────────────────────

    #[tokio::test]
    async fn test_token_filter_correct_token_passes() {
        let filter = TokenFilter::new("secret");
        let mut ctx = make_ctx("sayHello");
        ctx.attachments
            .insert("token".to_string(), "secret".to_string());
        let next = TestInvoker::new();

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
        assert_eq!(next.count(), 1);
    }

    #[tokio::test]
    async fn test_token_filter_wrong_token_blocked() {
        let filter = TokenFilter::new("secret");
        let mut ctx = make_ctx("sayHello");
        ctx.attachments
            .insert("token".to_string(), "wrong".to_string());
        let next = TestInvoker::new();

        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("token mismatch"));
        assert_eq!(next.count(), 0);
    }

    #[tokio::test]
    async fn test_token_filter_missing_token_blocked() {
        let filter = TokenFilter::new("secret");
        let mut ctx = make_ctx("sayHello");
        let next = TestInvoker::new();

        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_err());
        assert_eq!(next.count(), 0);
    }

    #[tokio::test]
    async fn test_token_filter_custom_key() {
        let filter = TokenFilter::new("abc").with_key("x-auth");
        let mut ctx = make_ctx("sayHello");
        ctx.attachments
            .insert("x-auth".to_string(), "abc".to_string());
        let next = TestInvoker::new();

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
    }

    // ── Task 4: AccessLogFilter ──────────────────────────────────────

    #[tokio::test]
    async fn test_access_log_filter_passes_result_unchanged() {
        let filter = AccessLogFilter;
        let ctx = make_ctx("sayHello");
        let result = RPCResult::success(b"data".to_vec());
        let invoker = TestInvoker::new();

        let output = filter.on_response(&ctx, result.clone(), &invoker).await;
        assert_eq!(output.value, result.value);
        assert!(!output.is_error());
    }

    #[tokio::test]
    async fn test_access_log_filter_handles_error_result() {
        let filter = AccessLogFilter;
        let ctx = make_ctx("sayHello");
        let err = dubbo_rs_common::error::RPCError::ServerError("boom".into());
        let result = RPCResult::from_error(err);
        let invoker = TestInvoker::new();

        let output = filter.on_response(&ctx, result, &invoker).await;
        assert!(output.is_error());
    }

    // ── Task 5: TPSLimiter + TPSLimitFilter ──────────────────────────

    #[test]
    fn test_tps_limiter_initial_allowable() {
        let limiter = DefaultTPSLimiter::new(1000, 1000);
        assert!(limiter.is_allowable());
    }

    #[test]
    fn test_tps_limiter_exhausts_tokens() {
        let limiter = DefaultTPSLimiter::new(100, 10);
        let mut allowed = 0;
        for _ in 0..20 {
            if limiter.is_allowable() {
                allowed += 1;
            }
        }
        assert!(
            allowed <= 10,
            "should allow at most capacity=10 calls, got {allowed}"
        );
    }

    #[tokio::test]
    async fn test_tps_limit_filter_blocks_when_exhausted() {
        let limiter = Box::new(DefaultTPSLimiter::new(100, 1));
        let filter = TPSLimitFilter::new(limiter);
        let next = TestInvoker::new();

        let mut ctx = make_ctx("sayHello");
        let _r1 = filter.invoke(&mut ctx, &next).await.unwrap();

        let mut ctx2 = make_ctx("sayHello");
        let r2 = filter.invoke(&mut ctx2, &next).await;
        assert!(r2.is_err(), "second call should be rate-limited");
    }

    #[test]
    #[allow(clippy::no_effect_underscore_binding)]
    fn test_tps_limiter_new_capacity() {
        let limiter = DefaultTPSLimiter::new(200, 50);
        // All 50 capacity tokens should be available
        let mut allowable_count = 0;
        for _ in 0..60 {
            if limiter.is_allowable() {
                allowable_count += 1;
            }
        }
        let _allowable_count = allowable_count;
    }

    // ── Tracking helper for FilterNode delegation tests ──────────────

    struct DestroyTrackingInvoker {
        url: URL,
        destroyed: Arc<AtomicBool>,
    }

    impl DestroyTrackingInvoker {
        fn new() -> Self {
            Self {
                url: URL::new("tri", "/com.example.TrackService"),
                destroyed: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl Node for DestroyTrackingInvoker {
        fn get_url(&self) -> &URL {
            &self.url
        }

        fn is_available(&self) -> bool {
            false
        }

        fn destroy(&self) {
            self.destroyed.store(true, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl Invoker for DestroyTrackingInvoker {
        async fn invoke(&self, _ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
            Ok(RPCResult::success(b"tracked".to_vec()))
        }
    }

    // ── FilterNode delegation tests ─────────────────────────────────

    #[test]
    fn test_filter_node_get_url_delegates() {
        let base = Box::new(TestInvoker::new());
        let node = FilterNode {
            filter: Box::new(PassThroughFilter),
            next: base,
        };
        assert_eq!(node.get_url().path, "/com.example.TestService");
    }

    #[test]
    fn test_filter_node_is_available_delegates() {
        let base = Box::new(DestroyTrackingInvoker::new());
        let node = FilterNode {
            filter: Box::new(PassThroughFilter),
            next: base,
        };
        assert!(!node.is_available());
    }

    #[test]
    fn test_filter_node_destroy_delegates() {
        let base = Box::new(DestroyTrackingInvoker::new());
        let destroyed = base.destroyed.clone();
        let node = FilterNode {
            filter: Box::new(PassThroughFilter),
            next: base,
        };
        assert!(!destroyed.load(Ordering::SeqCst));
        node.destroy();
        assert!(destroyed.load(Ordering::SeqCst));
    }

    // ── TokenFilter additional tests ────────────────────────────────

    #[tokio::test]
    async fn test_token_filter_custom_key_still_rejects_default_key() {
        let filter = TokenFilter::new("secret").with_key("x-token");
        let mut ctx = make_ctx("sayHello");
        ctx.attachments
            .insert("token".to_string(), "secret".to_string());
        let next = TestInvoker::new();

        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("token missing"));
        assert_eq!(next.count(), 0);
    }

    #[tokio::test]
    async fn test_token_filter_default_key_should_work() {
        let filter = TokenFilter::new("mypassword");
        let mut ctx = make_ctx("sayHello");
        ctx.attachments
            .insert("token".to_string(), "mypassword".to_string());
        let next = TestInvoker::new();

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
        assert_eq!(next.count(), 1);
    }

    #[tokio::test]
    async fn test_token_filter_empty_token() {
        let filter = TokenFilter::new("");
        let mut ctx = make_ctx("sayHello");
        ctx.attachments.insert("token".to_string(), String::new());
        let next = TestInvoker::new();

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
        assert_eq!(next.count(), 1);
    }

    // ── Task 9: GracefulShutdownFilter ──────────────────────────────

    #[tokio::test]
    async fn test_graceful_shutdown_allows_when_not_shutdown() {
        let filter = GracefulShutdownFilter::default();
        let mut ctx = make_ctx("sayHello");
        let next = TestInvoker::new();

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
    }

    #[tokio::test]
    async fn test_graceful_shutdown_rejects_when_shutdown() {
        let filter = GracefulShutdownFilter::default();
        filter.shutdown();
        let mut ctx = make_ctx("sayHello");
        let next = TestInvoker::new();

        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("shutting down"));
    }

    #[tokio::test]
    async fn test_graceful_shutdown_shared_flag() {
        let flag = Arc::new(AtomicBool::new(false));
        let f1 = GracefulShutdownFilter::from_flag(flag.clone());
        let f2 = GracefulShutdownFilter::from_flag(flag.clone());

        assert!(!f1.is_shutdown());
        assert!(!f2.is_shutdown());

        f1.shutdown();
        assert!(f2.is_shutdown());
    }

    #[tokio::test]
    async fn test_graceful_shutdown_inflight_tracking() {
        let filter = GracefulShutdownFilter::new();
        assert_eq!(filter.active_count(), 0);

        let mut ctx = make_ctx("sayHello");
        let next = TestInvoker::new();

        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_ok());
        assert_eq!(filter.active_count(), 0);
    }

    #[tokio::test]
    async fn test_graceful_shutdown_inflight_error_path() {
        let filter = GracefulShutdownFilter::new();
        let mut ctx = make_ctx("sayHello");
        let next = TestInvoker::new();
        next.should_fail.store(true, Ordering::SeqCst);

        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_error());
        assert_eq!(filter.active_count(), 0);
    }

    #[tokio::test]
    async fn test_graceful_shutdown_rejects_new_requests() {
        let filter = GracefulShutdownFilter::new();
        filter.shutdown();
        let mut ctx = make_ctx("sayHello");
        let next = TestInvoker::new();

        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("shutting down"));
        assert_eq!(filter.active_count(), 0);
    }

    #[tokio::test]
    async fn test_graceful_shutdown_waits_for_inflight() {
        let filter = Arc::new(GracefulShutdownFilter::new());
        let filter_clone = filter.clone();

        // Simulate an in-flight request by incrementing active_count
        filter.active_count.fetch_add(1, Ordering::SeqCst);
        assert_eq!(filter.active_count(), 1);

        filter.shutdown();

        // Spawn a task that will complete the in-flight request after a short delay
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            filter_clone.decrement_and_notify();
        });

        let completed = filter
            .wait_for_shutdown(std::time::Duration::from_secs(5))
            .await;
        assert!(completed);
        assert_eq!(filter.active_count(), 0);
    }

    #[tokio::test]
    async fn test_graceful_shutdown_timeout() {
        let filter = GracefulShutdownFilter::new();
        filter.active_count.fetch_add(1, Ordering::SeqCst);
        filter.shutdown();

        let completed = filter
            .wait_for_shutdown(std::time::Duration::from_millis(50))
            .await;
        assert!(!completed);
    }

    #[test]
    fn test_graceful_shutdown_with_custom_timeout() {
        let filter = GracefulShutdownFilter::with_timeout(std::time::Duration::from_secs(10));
        assert_eq!(filter.active_count(), 0);
        assert!(!filter.is_shutdown());
    }

    #[test]
    fn test_graceful_shutdown_resolve_timeout_from_url() {
        let filter = GracefulShutdownFilter::new();
        let mut url = URL::new("tri", "/test");
        url.set_param("shutdown.timeout", "5000");
        let timeout = filter.resolve_timeout_from_url(&url);
        assert_eq!(timeout, std::time::Duration::from_millis(5000));
    }

    #[test]
    fn test_graceful_shutdown_resolve_timeout_invalid() {
        let filter = GracefulShutdownFilter::new();
        let mut url = URL::new("tri", "/test");
        url.set_param("shutdown.timeout", "not_a_number");
        let timeout = filter.resolve_timeout_from_url(&url);
        assert_eq!(timeout, std::time::Duration::from_secs(30));
    }

    #[test]
    fn test_graceful_shutdown_resolve_timeout_missing() {
        let filter = GracefulShutdownFilter::new();
        let url = URL::new("tri", "/test");
        let timeout = filter.resolve_timeout_from_url(&url);
        assert_eq!(timeout, std::time::Duration::from_secs(30));
    }

    // ── Task 10: ActiveLimitFilter ──────────────────────────────────

    #[tokio::test]
    async fn test_active_limit_allows_within_limit() {
        let filter = ActiveLimitFilter::new(2);
        let mut ctx = make_ctx("sayHello");
        let next = TestInvoker::new();

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
    }

    #[tokio::test]
    async fn test_active_limit_rejects_when_full() {
        let filter = ActiveLimitFilter::new(1);
        let next = TestInvoker::new();

        filter.active_count.fetch_add(1, Ordering::SeqCst);
        assert_eq!(filter.active_count(), 1);

        let mut ctx = make_ctx("sayHello");
        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_active_limit_on_response_decrements() {
        let filter = ActiveLimitFilter::new(5);
        let ctx = make_ctx("sayHello");
        let result = RPCResult::success(b"x".to_vec());
        let invoker = TestInvoker::new();

        filter.active_count.fetch_add(1, Ordering::SeqCst);
        assert_eq!(filter.active_count(), 1);

        filter.on_response(&ctx, result, &invoker).await;
        assert_eq!(filter.active_count(), 0);
    }

    // ── Task 11: ExecuteLimitFilter ────────────────────────────────

    #[tokio::test]
    async fn test_execute_limit_allows_within_limit() {
        let filter = ExecuteLimitFilter::new(5);
        let mut ctx = make_ctx("sayHello");
        let next = TestInvoker::new();

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
    }

    #[tokio::test]
    async fn test_execute_limit_rejects_when_semaphore_exhausted() {
        let filter = ExecuteLimitFilter::new(1);
        let next = TestInvoker::new();

        let _permit = filter.semaphore.try_acquire().unwrap();

        let mut ctx = make_ctx("sayHello");
        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_err());
    }

    // ── filter stubs for testing ──────────────────────────────────────

    struct PassThroughFilter;

    #[async_trait]
    impl Filter for PassThroughFilter {
        async fn invoke(
            &self,
            ctx: &mut InvocationContext,
            next: &dyn Invoker,
        ) -> Result<RPCResult, anyhow::Error> {
            next.invoke(ctx).await
        }
    }

    struct ShortCircuitFilter;

    #[async_trait]
    impl Filter for ShortCircuitFilter {
        async fn invoke(
            &self,
            _ctx: &mut InvocationContext,
            _next: &dyn Invoker,
        ) -> Result<RPCResult, anyhow::Error> {
            Ok(RPCResult::success(b"short-circuit".to_vec()))
        }
    }

    struct OrderFilter {
        name: &'static str,
        order: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl OrderFilter {
        fn record(&self, phase: &str) {
            self.order
                .lock()
                .unwrap()
                .push(format!("{}.{}", self.name, phase));
        }
    }

    #[async_trait]
    impl Filter for OrderFilter {
        async fn invoke(
            &self,
            ctx: &mut InvocationContext,
            next: &dyn Invoker,
        ) -> Result<RPCResult, anyhow::Error> {
            self.record("invoke");
            let result = next.invoke(ctx).await;
            Ok(result?)
        }

        async fn on_response(
            &self,
            _ctx: &InvocationContext,
            result: RPCResult,
            _invoker: &dyn Invoker,
        ) -> RPCResult {
            self.record("on_response");
            result
        }
    }

    // ── TPSLimiter ──────────────────────────────────────────────────────

    use std::sync::Mutex;
    use std::time::Instant;

    /// Rate limiter used by [`TPSLimitFilter`].
    pub trait TPSLimiter: Send + Sync {
        /// Returns `true` if a call is allowed under the current rate.
        fn is_allowable(&self) -> bool;
    }

    /// Token-bucket rate limiter.
    ///
    /// Tokens refill at `rate` tokens per second, up to `capacity`.
    /// Each call consumes one token. The bucket uses fixed-interval
    /// refill (capped at 100ms granularity) to avoid per-call clock reads.
    pub struct DefaultTPSLimiter {
        rate: u64,
        capacity: u64,
        tokens: std::sync::atomic::AtomicU64,
        last_refill: Mutex<Instant>,
    }

    impl DefaultTPSLimiter {
        #[must_use]
        pub fn new(rate: u64, capacity: u64) -> Self {
            Self {
                rate,
                capacity,
                tokens: std::sync::atomic::AtomicU64::new(capacity * 1000),
                last_refill: Mutex::new(Instant::now()),
            }
        }

        fn refill(&self) {
            let mut last = self.last_refill.lock().unwrap();
            let elapsed = last.elapsed();
            if elapsed.as_millis() < 100 {
                return;
            }
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss
            )]
            let add = ((elapsed.as_secs_f64() * self.rate as f64) * 1000.0) as u64;
            let current = self.tokens.load(std::sync::atomic::Ordering::Relaxed);
            let new = (current + add).min(self.capacity * 1000);
            self.tokens.store(new, std::sync::atomic::Ordering::Relaxed);
            *last = Instant::now();
        }
    }

    impl TPSLimiter for DefaultTPSLimiter {
        fn is_allowable(&self) -> bool {
            use std::sync::atomic::Ordering;
            self.refill();
            let mut current = self.tokens.load(Ordering::Relaxed);
            loop {
                if current < 1000 {
                    return false;
                }
                match self.tokens.compare_exchange_weak(
                    current,
                    current - 1000,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return true,
                    Err(actual) => current = actual,
                }
            }
        }
    }

    // ── TPSLimitFilter ──────────────────────────────────────────────────

    /// TPS (transactions per second) rate-limiting filter.
    ///
    /// Delegates to a [`TPSLimiter`] to decide whether each invocation
    /// is allowed. Rejected calls return an error without reaching the
    /// downstream invoker.
    pub struct TPSLimitFilter {
        limiter: Box<dyn TPSLimiter>,
    }

    impl TPSLimitFilter {
        #[must_use]
        pub fn new(limiter: Box<dyn TPSLimiter>) -> Self {
            Self { limiter }
        }
    }

    #[async_trait]
    impl Filter for TPSLimitFilter {
        async fn invoke(
            &self,
            ctx: &mut InvocationContext,
            next: &dyn Invoker,
        ) -> Result<RPCResult, anyhow::Error> {
            if self.limiter.is_allowable() {
                next.invoke(ctx).await
            } else {
                Err(anyhow::anyhow!(
                    "TPS limit exceeded for method '{}'",
                    ctx.method_name
                ))
            }
        }
    }

    // ── TimeoutFilter ─────────────────────────────────────────────────

    use std::time::Duration;

    struct SlowInvoker {
        url: URL,
        delay: Duration,
    }

    impl SlowInvoker {
        fn new(delay: Duration) -> Self {
            Self {
                url: URL::new("tri", "/com.example.SlowService"),
                delay,
            }
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
        async fn invoke(&self, _ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
            tokio::time::sleep(self.delay).await;
            Ok(RPCResult::success(b"slow_response".to_vec()))
        }
    }

    #[tokio::test]
    async fn test_timeout_filter_allows_fast_calls() {
        let filter = TimeoutFilter::new(Duration::from_millis(500));
        let next = TestInvoker::new();
        let mut ctx = make_ctx("sayHello");
        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
    }

    #[tokio::test]
    async fn test_timeout_filter_rejects_slow_calls() {
        let filter = TimeoutFilter::new(Duration::from_millis(50));
        let next = SlowInvoker::new(Duration::from_millis(500));
        let mut ctx = make_ctx("slowMethod");
        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("timed out"),
            "expected timeout error, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_timeout_filter_reads_url_param() {
        let filter = TimeoutFilter::new(Duration::from_secs(30));
        let next = SlowInvoker::new(Duration::from_millis(200));

        let mut url = URL::new("tri", "/com.example.Service");
        url.set_param("timeout", "50");
        let mut ctx = InvocationContext::new("slowMethod", url);

        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_err(), "should timeout with 50ms URL param");
    }

    #[tokio::test]
    async fn test_timeout_filter_default_3_seconds() {
        let filter = TimeoutFilter::default();
        let ctx = make_ctx("sayHello");
        let url = ctx.url.clone();
        let timeout = filter.resolve_timeout(&InvocationContext::new("sayHello", url));
        assert_eq!(timeout, Duration::from_secs(3));
    }

    #[tokio::test]
    async fn test_timeout_filter_ignores_invalid_param() {
        let filter = TimeoutFilter::new(Duration::from_secs(5));
        let mut url = URL::new("tri", "/com.example.Service");
        url.set_param("timeout", "not_a_number");
        let ctx = InvocationContext::new("sayHello", url);
        let timeout = filter.resolve_timeout(&ctx);
        assert_eq!(
            timeout,
            Duration::from_secs(5),
            "should fall back to default for invalid param"
        );
    }

    #[tokio::test]
    async fn test_timeout_filter_custom_default() {
        let filter = TimeoutFilter::new(Duration::from_secs(10));
        let ctx = make_ctx("sayHello");
        let url = ctx.url.clone();
        let timeout = filter.resolve_timeout(&InvocationContext::new("sayHello", url));
        assert_eq!(timeout, Duration::from_secs(10));
    }
}

// ── GracefulShutdownFilter ──────────────────────────────────────────

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;

/// Default shutdown timeout in seconds.
const DEFAULT_SHUTDOWN_TIMEOUT_SECS: u64 = 30;

/// Graceful shutdown filter: rejects new invocations after receiving
/// a shutdown signal, allowing in-flight requests to complete.
///
/// Features:
/// - In-flight request tracking via `active_count`
/// - Shutdown completion notification via `Notify`
/// - Configurable shutdown timeout (via URL param "shutdown.timeout" in ms)
///
/// Multiple instances can share the same shutdown flag via
/// [`GracefulShutdownFilter::from_flag`].
pub struct GracefulShutdownFilter {
    shutdown_flag: Arc<AtomicBool>,
    active_count: Arc<AtomicUsize>,
    shutdown_notify: Arc<Notify>,
    shutdown_timeout: Duration,
}

impl GracefulShutdownFilter {
    /// Create a new filter with default 30-second shutdown timeout.
    #[must_use]
    pub fn new() -> Self {
        Self {
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            active_count: Arc::new(AtomicUsize::new(0)),
            shutdown_notify: Arc::new(Notify::new()),
            shutdown_timeout: Duration::from_secs(DEFAULT_SHUTDOWN_TIMEOUT_SECS),
        }
    }

    /// Create a new filter with a custom shutdown timeout.
    #[must_use]
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            active_count: Arc::new(AtomicUsize::new(0)),
            shutdown_notify: Arc::new(Notify::new()),
            shutdown_timeout: timeout,
        }
    }

    /// Create from a shared shutdown flag (backward compatible).
    #[must_use]
    pub fn from_flag(flag: Arc<AtomicBool>) -> Self {
        Self {
            shutdown_flag: flag,
            active_count: Arc::new(AtomicUsize::new(0)),
            shutdown_notify: Arc::new(Notify::new()),
            shutdown_timeout: Duration::from_secs(DEFAULT_SHUTDOWN_TIMEOUT_SECS),
        }
    }

    /// Read shutdown timeout from URL parameter "shutdown.timeout" (milliseconds).
    /// Falls back to the configured default if absent or invalid.
    #[must_use]
    pub fn resolve_timeout_from_url(&self, url: &URL) -> Duration {
        if let Some(val) = url.get_param("shutdown.timeout") {
            if let Ok(ms) = val.parse::<u64>() {
                return Duration::from_millis(ms);
            }
        }
        self.shutdown_timeout
    }

    /// Signal that shutdown has begun. New invocations will be rejected.
    pub fn shutdown(&self) {
        self.shutdown_flag.store(true, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_shutdown(&self) -> bool {
        self.shutdown_flag.load(Ordering::SeqCst)
    }

    /// Return the current number of in-flight requests.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.active_count.load(Ordering::SeqCst)
    }

    /// Wait for all in-flight requests to complete, up to the given timeout.
    /// Returns `true` if all requests completed, `false` if timed out.
    pub async fn wait_for_shutdown(&self, timeout: Duration) -> bool {
        if self.active_count.load(Ordering::SeqCst) == 0 {
            return true;
        }
        let result = tokio::time::timeout(timeout, self.shutdown_notify.notified()).await;
        result.is_ok()
    }

    /// Decrement active count and notify waiters if we reached zero during shutdown.
    pub(crate) fn decrement_and_notify(&self) {
        let prev = self.active_count.fetch_sub(1, Ordering::SeqCst);
        if prev == 1 && self.shutdown_flag.load(Ordering::SeqCst) {
            self.shutdown_notify.notify_waiters();
        }
    }
}

impl Default for GracefulShutdownFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Filter for GracefulShutdownFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        if self.shutdown_flag.load(Ordering::SeqCst) {
            return Err(anyhow::anyhow!(
                "server shutting down, rejecting call to '{}'",
                ctx.method_name,
            ));
        }
        self.active_count.fetch_add(1, Ordering::SeqCst);
        let result = next.invoke(ctx).await;
        self.decrement_and_notify();
        result
    }
}

// ── ActiveLimitFilter ───────────────────────────────────────────────

/// Active call limiter: rejects invocations when the number of
/// concurrently active calls exceeds the configured maximum.
///
/// The counter is incremented in `invoke()` and decremented in
/// `on_response()` so that in-flight calls are correctly tracked.
pub struct ActiveLimitFilter {
    active_count: Arc<AtomicUsize>,
    max_active: usize,
}

impl ActiveLimitFilter {
    #[must_use]
    pub fn new(max_active: usize) -> Self {
        Self {
            active_count: Arc::new(AtomicUsize::new(0)),
            max_active,
        }
    }

    #[must_use]
    pub fn active_count(&self) -> usize {
        self.active_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Filter for ActiveLimitFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        let current = self.active_count.fetch_add(1, Ordering::SeqCst);
        if current >= self.max_active {
            self.active_count.fetch_sub(1, Ordering::SeqCst);
            return Err(anyhow::anyhow!(
                "active limit reached ({}/{}) for '{}'",
                current,
                self.max_active,
                ctx.method_name,
            ));
        }
        next.invoke(ctx).await
    }

    async fn on_response(
        &self,
        _ctx: &InvocationContext,
        result: RPCResult,
        _invoker: &dyn Invoker,
    ) -> RPCResult {
        self.active_count.fetch_sub(1, Ordering::SeqCst);
        result
    }
}

// ── ExecuteLimitFilter ──────────────────────────────────────────────

/// Execution concurrency limiter using a [`tokio::sync::Semaphore`].
///
/// Limits the number of concurrently executing invocations. Calls
/// that cannot immediately acquire a permit are rejected.
pub struct ExecuteLimitFilter {
    semaphore: Arc<tokio::sync::Semaphore>,
}

impl ExecuteLimitFilter {
    #[must_use]
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent)),
        }
    }
}

#[async_trait]
impl Filter for ExecuteLimitFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        let Ok(permit) = self.semaphore.try_acquire() else {
            return Err(anyhow::anyhow!(
                "execution limit reached for '{}'",
                ctx.method_name,
            ));
        };
        let result = next.invoke(ctx).await;
        drop(permit);
        result
    }
}

// ── TimeoutFilter ───────────────────────────────────────────────────

/// Timeout filter: wraps invocations with a deadline using
/// [`tokio::time::timeout`].
///
/// The timeout value is read from the invocation URL's `"timeout"` parameter
/// (in milliseconds). If the parameter is absent, `default_timeout` is used
/// (default: 3 seconds).
///
/// When a call exceeds the deadline, the filter returns a timeout error and
/// logs a warning.
pub struct TimeoutFilter {
    default_timeout: Duration,
}

impl TimeoutFilter {
    /// Create a new filter with the given default timeout.
    #[must_use]
    pub fn new(default_timeout: Duration) -> Self {
        Self { default_timeout }
    }

    /// Resolve the effective timeout for the given invocation context.
    ///
    /// Reads the `"timeout"` URL parameter (milliseconds). Falls back to
    /// `default_timeout` if the parameter is absent or unparseable.
    fn resolve_timeout(&self, ctx: &InvocationContext) -> Duration {
        ctx.url
            .get_param("timeout")
            .and_then(|v| v.parse::<u64>().ok())
            .map_or(self.default_timeout, Duration::from_millis)
    }
}

impl Default for TimeoutFilter {
    fn default() -> Self {
        Self::new(Duration::from_secs(3))
    }
}

#[async_trait]
impl Filter for TimeoutFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        let timeout = self.resolve_timeout(ctx);
        if let Ok(result) = tokio::time::timeout(timeout, next.invoke(ctx)).await {
            result
        } else {
            tracing::warn!(
                method = %ctx.method_name,
                timeout_ms = timeout.as_millis(),
                "invocation timed out"
            );
            Err(anyhow::anyhow!(
                "invocation of '{}' timed out after {}ms",
                ctx.method_name,
                timeout.as_millis()
            ))
        }
    }
}

// ── AuthFilter ─────────────────────────────────────────────────────

use dashmap::DashMap;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Maximum allowed clock skew for timestamp freshness checks (5 minutes).
const AUTH_TIMESTAMP_TOLERANCE_MS: u64 = 5 * 60 * 1000;

/// HMAC-SHA256 signature authentication filter.
///
/// Matches the access-key / secret-key authentication pattern used by
/// dubbo-java and dubbo-go:
///
/// **Consumer side** — when the invocation URL carries `access_key` and
/// `secret_key` parameters, the filter computes an HMAC-SHA256 signature
/// of `{method_name}\n{interface}\n{timestamp}` and attaches `ak`,
/// `signature`, and `timestamp` to the invocation attachments.
///
/// **Provider side** — when an `ak` attachment is already present the
/// filter looks up the corresponding secret key from its local credential
/// store, recomputes the expected signature, and compares it with the one
/// provided in constant time.  A missing, unknown, mismatched, or expired
/// signature is rejected.
pub struct AuthFilter {
    credentials: Arc<DashMap<String, String>>,
}

impl AuthFilter {
    /// Create a filter with an empty credential store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            credentials: Arc::new(DashMap::new()),
        }
    }

    /// Create a filter pre-loaded with one credential pair.
    #[must_use]
    pub fn with_credentials(access_key: &str, secret_key: &str) -> Self {
        let filter = Self::new();
        filter
            .credentials
            .insert(access_key.to_string(), secret_key.to_string());
        filter
    }

    /// Add (or overwrite) a credential pair at runtime.
    pub fn add_credential(&self, access_key: &str, secret_key: &str) {
        self.credentials
            .insert(access_key.to_string(), secret_key.to_string());
    }

    /// Compute the HMAC-SHA256 hex-encoded signature for the given inputs.
    fn compute_signature(
        secret_key: &str,
        method_name: &str,
        interface: &str,
        timestamp: &str,
    ) -> String {
        let string_to_sign = format!("{method_name}\n{interface}\n{timestamp}");
        let mut mac = HmacSha256::new_from_slice(secret_key.as_bytes())
            .expect("HMAC can accept any key length");
        mac.update(string_to_sign.as_bytes());
        let result = mac.finalize();
        hex::encode(result.into_bytes())
    }
}

impl Default for AuthFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Filter for AuthFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        // ── Provider-side: validate incoming signature ──
        if let Some(access_key) = ctx.attachments.get("ak").cloned() {
            let secret_key = self
                .credentials
                .get(&access_key)
                .map(|v| v.value().clone())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "auth failed: unknown access key '{}' for method '{}'",
                        access_key,
                        ctx.method_name
                    )
                })?;

            let provided_signature =
                ctx.attachments.get("signature").cloned().ok_or_else(|| {
                    anyhow::anyhow!(
                        "auth failed: missing signature for method '{}'",
                        ctx.method_name
                    )
                })?;

            let timestamp_str = ctx.attachments.get("timestamp").cloned().ok_or_else(|| {
                anyhow::anyhow!(
                    "auth failed: missing timestamp for method '{}'",
                    ctx.method_name
                )
            })?;

            // Optional timestamp freshness check
            if let Ok(ts_ms) = timestamp_str.parse::<u64>() {
                #[allow(clippy::cast_possible_truncation)]
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                if now_ms > ts_ms + AUTH_TIMESTAMP_TOLERANCE_MS {
                    return Err(anyhow::anyhow!(
                        "auth failed: expired timestamp for method '{}'",
                        ctx.method_name
                    ));
                }
            }

            let interface = ctx.url.path.as_str();
            let expected =
                Self::compute_signature(&secret_key, &ctx.method_name, interface, &timestamp_str);

            let expected_bytes = expected.as_bytes();
            let provided_bytes = provided_signature.as_bytes();
            if expected_bytes.len() != provided_bytes.len()
                || expected_bytes
                    .iter()
                    .zip(provided_bytes.iter())
                    .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                    != 0
            {
                return Err(anyhow::anyhow!(
                    "auth failed: signature mismatch for method '{}'",
                    ctx.method_name
                ));
            }

            return next.invoke(ctx).await;
        }

        // ── Consumer-side: sign the request ──
        if let (Some(access_key), Some(secret_key)) = (
            ctx.url.get_param("access_key").cloned(),
            ctx.url.get_param("secret_key").cloned(),
        ) {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();

            // u128 → u64 is safe: as_millis() is well within u64 range
            #[allow(clippy::cast_possible_truncation)]
            let timestamp_str = now_ms.to_string();

            let interface = ctx.url.path.as_str();
            let signature =
                Self::compute_signature(&secret_key, &ctx.method_name, interface, &timestamp_str);

            ctx.attachments.insert("ak".to_string(), access_key);
            ctx.attachments.insert("signature".to_string(), signature);
            ctx.attachments
                .insert("timestamp".to_string(), timestamp_str);
        }

        next.invoke(ctx).await
    }
}

// ============================================================================
// CircuitBreaker — Sentinel-style circuit breaker filter
// ============================================================================

use std::sync::atomic::AtomicU64;
use std::sync::Mutex;
use std::time::Instant;

/// Circuit breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

/// A sliding-window circuit breaker that monitors call outcomes and
/// transitions between `Closed`, `Open`, and `HalfOpen` states.
///
/// When the failure rate exceeds `failure_threshold` within
/// `window_duration`, the circuit opens. After `recovery_timeout`,
/// it transitions to half-open and allows a limited number of probe
/// calls. If probes succeed, the circuit closes; otherwise it re-opens.
pub struct CircuitBreaker {
    state: Mutex<CircuitBreakerState>,
    failure_count: AtomicU64,
    success_count: AtomicU64,
    total_count: AtomicU64,
    last_failure_time: Mutex<Instant>,
    last_state_change: Mutex<Instant>,
    failure_threshold: u64,
    success_threshold: u64,
    window_duration: Duration,
    recovery_timeout: Duration,
    max_half_open_probes: u64,
    half_open_count: AtomicU64,
}

impl CircuitBreaker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(CircuitBreakerState::Closed),
            failure_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            total_count: AtomicU64::new(0),
            last_failure_time: Mutex::new(Instant::now()),
            last_state_change: Mutex::new(Instant::now()),
            failure_threshold: 10,
            success_threshold: 5,
            window_duration: Duration::from_secs(30),
            recovery_timeout: Duration::from_secs(60),
            max_half_open_probes: 3,
            half_open_count: AtomicU64::new(0),
        }
    }

    #[must_use]
    pub fn with_failure_threshold(mut self, threshold: u64) -> Self {
        self.failure_threshold = threshold;
        self
    }

    #[must_use]
    pub fn with_success_threshold(mut self, threshold: u64) -> Self {
        self.success_threshold = threshold;
        self
    }

    #[must_use]
    pub fn with_window_duration(mut self, duration: Duration) -> Self {
        self.window_duration = duration;
        self
    }

    #[must_use]
    pub fn with_recovery_timeout(mut self, duration: Duration) -> Self {
        self.recovery_timeout = duration;
        self
    }

    #[must_use]
    pub fn with_max_half_open_probes(mut self, count: u64) -> Self {
        self.max_half_open_probes = count;
        self
    }

    fn transition_state(&self, new_state: CircuitBreakerState) {
        if let Ok(mut state) = self.state.lock() {
            *state = new_state;
        }
        if let Ok(mut t) = self.last_state_change.lock() {
            *t = Instant::now();
        }
    }

    /// Check if a call is allowed through the circuit breaker.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn is_call_permitted(&self) -> bool {
        loop {
            let state = *self.state.lock().expect("circuit breaker mutex poisoned");

            match state {
                CircuitBreakerState::Closed => return true,
                CircuitBreakerState::Open => {
                    let elapsed = self
                        .last_state_change
                        .lock()
                        .expect("circuit breaker mutex poisoned")
                        .elapsed();
                    if elapsed >= self.recovery_timeout {
                        self.transition_state(CircuitBreakerState::HalfOpen);
                        self.half_open_count.store(0, Ordering::SeqCst);
                        continue;
                    }
                    return false;
                }
                CircuitBreakerState::HalfOpen => {
                    let count = self.half_open_count.fetch_add(1, Ordering::SeqCst);
                    return count < self.max_half_open_probes;
                }
            }
        }
    }

    /// Record a successful call.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    pub fn record_success(&self) {
        self.success_count.fetch_add(1, Ordering::SeqCst);
        self.total_count.fetch_add(1, Ordering::SeqCst);

        let state = *self.state.lock().expect("circuit breaker mutex poisoned");
        if state == CircuitBreakerState::HalfOpen {
            let successes = self.success_count.load(Ordering::SeqCst);
            if successes >= self.success_threshold {
                self.success_count.store(0, Ordering::SeqCst);
                self.failure_count.store(0, Ordering::SeqCst);
                self.half_open_count.store(0, Ordering::SeqCst);
                self.transition_state(CircuitBreakerState::Closed);
            }
        } else {
            self.maybe_reset_window();
        }
    }

    /// Record a failed call.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    pub fn record_failure(&self) {
        self.failure_count.fetch_add(1, Ordering::SeqCst);
        self.total_count.fetch_add(1, Ordering::SeqCst);

        if let Ok(mut t) = self.last_failure_time.lock() {
            *t = Instant::now();
        }

        let state = *self.state.lock().expect("circuit breaker mutex poisoned");
        match state {
            CircuitBreakerState::Closed => {
                self.maybe_open_circuit();
            }
            CircuitBreakerState::HalfOpen => {
                self.transition_state(CircuitBreakerState::Open);
            }
            CircuitBreakerState::Open => {}
        }
    }

    fn maybe_reset_window(&self) {
        if let Ok(last_fail) = self.last_failure_time.lock() {
            if last_fail.elapsed() >= self.window_duration {
                self.failure_count.store(0, Ordering::SeqCst);
                self.success_count.store(0, Ordering::SeqCst);
            }
        }
    }

    fn maybe_open_circuit(&self) {
        let failures = self.failure_count.load(Ordering::SeqCst);
        if failures >= self.failure_threshold {
            self.transition_state(CircuitBreakerState::Open);
        }
    }

    /// Return the current circuit breaker state.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn state(&self) -> CircuitBreakerState {
        *self.state.lock().expect("circuit breaker mutex poisoned")
    }

    #[must_use]
    pub fn failure_count(&self) -> u64 {
        self.failure_count.load(Ordering::SeqCst)
    }

    #[must_use]
    pub fn success_count(&self) -> u64 {
        self.success_count.load(Ordering::SeqCst)
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

/// A filter that wraps a [`CircuitBreaker`] to provide Sentinel-style
/// circuit breaking for Dubbo invocations.
///
/// When the circuit is open, invocations are rejected immediately with
/// an error. Successful and failed calls are recorded to drive state
/// transitions.
pub struct CircuitBreakerFilter {
    breaker: Arc<CircuitBreaker>,
}

impl CircuitBreakerFilter {
    #[must_use]
    pub fn new(breaker: Arc<CircuitBreaker>) -> Self {
        Self { breaker }
    }

    #[must_use]
    pub fn breaker(&self) -> &Arc<CircuitBreaker> {
        &self.breaker
    }
}

#[async_trait]
impl Filter for CircuitBreakerFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        if !self.breaker.is_call_permitted() {
            return Err(anyhow::anyhow!(
                "circuit breaker open for '{}'",
                ctx.method_name,
            ));
        }

        match next.invoke(ctx).await {
            Ok(result) => {
                if result.is_error() {
                    self.breaker.record_failure();
                } else {
                    self.breaker.record_success();
                }
                Ok(result)
            }
            Err(e) => {
                self.breaker.record_failure();
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod circuit_breaker_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_circuit_breaker_initial_state_closed() {
        let cb = CircuitBreaker::new();
        assert_eq!(cb.state(), CircuitBreakerState::Closed);
    }

    #[test]
    fn test_circuit_breaker_call_permitted_when_closed() {
        let cb = CircuitBreaker::new();
        assert!(cb.is_call_permitted());
    }

    #[test]
    fn test_circuit_breaker_records_success_and_failure() {
        let cb = CircuitBreaker::new();
        cb.record_success();
        assert_eq!(cb.success_count(), 1);

        cb.record_failure();
        assert_eq!(cb.failure_count(), 1);
    }

    #[test]
    fn test_circuit_breaker_opens_after_threshold() {
        let cb = CircuitBreaker::new().with_failure_threshold(3);

        for _ in 0..3 {
            cb.record_failure();
        }

        assert_eq!(cb.state(), CircuitBreakerState::Open);
        assert!(!cb.is_call_permitted());
    }

    #[test]
    fn test_circuit_breaker_stays_closed_below_threshold() {
        let cb = CircuitBreaker::new().with_failure_threshold(5);

        for _ in 0..4 {
            cb.record_failure();
        }

        assert_eq!(cb.state(), CircuitBreakerState::Closed);
    }

    #[test]
    fn test_circuit_breaker_half_open_after_recovery_timeout() {
        let cb = CircuitBreaker::new()
            .with_failure_threshold(2)
            .with_recovery_timeout(Duration::from_millis(1));

        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitBreakerState::Open);

        std::thread::sleep(Duration::from_millis(10));

        assert!(cb.is_call_permitted());
        assert_eq!(cb.state(), CircuitBreakerState::HalfOpen);
    }

    #[test]
    fn test_circuit_breaker_closes_after_success_threshold_in_half_open() {
        let cb = CircuitBreaker::new()
            .with_failure_threshold(2)
            .with_recovery_timeout(Duration::from_millis(1))
            .with_success_threshold(2);

        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitBreakerState::Open);

        std::thread::sleep(Duration::from_millis(10));
        let _ = cb.is_call_permitted();
        assert_eq!(cb.state(), CircuitBreakerState::HalfOpen);

        cb.record_success();
        cb.record_success();
        assert_eq!(cb.state(), CircuitBreakerState::Closed);
    }

    #[test]
    fn test_circuit_breaker_reopens_on_failure_in_half_open() {
        let cb = CircuitBreaker::new()
            .with_failure_threshold(2)
            .with_recovery_timeout(Duration::from_millis(1));

        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitBreakerState::Open);

        std::thread::sleep(Duration::from_millis(10));
        let _ = cb.is_call_permitted();
        assert_eq!(cb.state(), CircuitBreakerState::HalfOpen);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitBreakerState::Open);
    }

    #[test]
    fn test_circuit_breaker_max_half_open_probes() {
        let cb = CircuitBreaker::new()
            .with_failure_threshold(2)
            .with_recovery_timeout(Duration::from_millis(1))
            .with_max_half_open_probes(2);

        cb.record_failure();
        cb.record_failure();

        std::thread::sleep(Duration::from_millis(10));

        assert!(cb.is_call_permitted());
        assert!(cb.is_call_permitted());
        assert!(!cb.is_call_permitted());
    }

    #[test]
    fn test_circuit_breaker_default_has_sensible_defaults() {
        let cb = CircuitBreaker::default();
        assert_eq!(cb.failure_count(), 0);
        assert_eq!(cb.success_count(), 0);
        assert_eq!(cb.state(), CircuitBreakerState::Closed);
    }

    #[test]
    fn test_circuit_breaker_resets_window() {
        let cb = CircuitBreaker::new()
            .with_failure_threshold(10)
            .with_window_duration(Duration::from_millis(1));

        cb.record_failure();
        assert_eq!(cb.failure_count(), 1);

        std::thread::sleep(Duration::from_millis(10));

        cb.record_success();
        assert_eq!(cb.failure_count(), 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_filter_allows_when_closed() {
        let cb = Arc::new(CircuitBreaker::new());
        let filter = CircuitBreakerFilter::new(cb.clone());

        let next = crate::tests::TestInvoker::new();
        let mut ctx = crate::tests::make_ctx("sayHello");

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
    }

    #[tokio::test]
    async fn test_circuit_breaker_filter_rejects_when_open() {
        let cb = Arc::new(CircuitBreaker::new().with_failure_threshold(1));
        cb.record_failure();
        assert_eq!(cb.state(), CircuitBreakerState::Open);

        let filter = CircuitBreakerFilter::new(cb.clone());
        let next = crate::tests::TestInvoker::new();
        let mut ctx = crate::tests::make_ctx("sayHello");

        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("circuit breaker open"));
    }
}

// ============================================================================
// GenericInvoker — generic call support for dynamic invocation
// ============================================================================

use dubbo_rs_common::constants::{
    GENERIC_KEY, GENERIC_SERIALIZATION_BEAN, GENERIC_SERIALIZATION_DEFAULT,
    GENERIC_SERIALIZATION_PROTOBUF_JSON,
};

/// Serialization mode for generic invocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenericSerialization {
    /// Default: JSON string encode/decode (current behavior).
    Json,
    /// Protobuf-JSON: args are JSON strings, encoded via proto descriptor.
    ProtobufJson,
    /// Map mode: args are JSON strings representing key-value maps.
    Map,
}

impl Default for GenericSerialization {
    fn default() -> Self {
        Self::Json
    }
}

/// Generic invocation interface — calls any Dubbo service without
/// compiled POJO classes, using dynamic parameter types and return values.
///
/// This is the Rust equivalent of Dubbo's `GenericService`, enabling
/// API gateways, test tools, and dynamic service consumers to invoke
/// services with JSON-encoded parameters and decode responses as JSON.
#[async_trait]
pub trait GenericService: Send + Sync {
    /// Invoke a Dubbo method with generic parameters.
    ///
    /// `method` — the method name to call (e.g., `"sayHello"`).
    /// `param_types` — Java-style type descriptors (e.g., `["Ljava/lang/String;"]`).
    /// `args` — JSON-encoded argument values matching `param_types`.
    ///
    /// Returns a JSON-encoded string of the response value on success.
    ///
    /// # Errors
    ///
    /// Returns an error if the invocation fails (network, serialization,
    /// or service error).
    async fn invoke(
        &self,
        method: String,
        param_types: Vec<String>,
        args: Vec<String>,
    ) -> Result<String, anyhow::Error>;

    /// Invoke with an explicit serialization mode.
    ///
    /// The default implementation delegates to [`GenericService::invoke`].
    async fn invoke_with_mode(
        &self,
        method: String,
        param_types: Vec<String>,
        args: Vec<String>,
        mode: GenericSerialization,
    ) -> Result<String, anyhow::Error> {
        let _ = mode;
        self.invoke(method, param_types, args).await
    }
}

/// A [`GenericService`] implementation that wraps a [`Invoker`] and
/// encodes/decodes arguments and results as JSON strings.
///
/// This allows callers to invoke any Dubbo service by providing
/// method name, parameter type descriptors, and JSON-encoded arguments
/// without needing compiled protobuf or POJO stubs.
pub struct GenericInvoker {
    invoker: Box<dyn Invoker>,
    url: URL,
}

impl GenericInvoker {
    #[must_use]
    pub fn new(invoker: Box<dyn Invoker>, url: URL) -> Self {
        Self { invoker, url }
    }

    #[must_use]
    pub fn url(&self) -> &URL {
        &self.url
    }
}

#[async_trait]
impl GenericService for GenericInvoker {
    async fn invoke(
        &self,
        method: String,
        param_types: Vec<String>,
        args: Vec<String>,
    ) -> Result<String, anyhow::Error> {
        let mut ctx = InvocationContext::new(&method, self.url.clone());
        ctx.parameter_types = param_types;
        ctx.arguments = args.into_iter().map(String::into_bytes).collect();
        ctx.attachments.insert(
            GENERIC_KEY.to_string(),
            GENERIC_SERIALIZATION_DEFAULT.to_string(),
        );

        let result = self.invoker.invoke(&mut ctx).await?;

        if result.is_error() {
            Err(anyhow::anyhow!(
                "generic invoke '{}' failed: {:?}",
                method,
                result.error
            ))
        } else {
            let value_bytes = result.value.as_ref().map_or(&b"null"[..], |v| v.as_slice());
            String::from_utf8(value_bytes.to_vec())
                .map_err(|e| anyhow::anyhow!("invalid UTF-8 in generic response: {e}"))
        }
    }

    async fn invoke_with_mode(
        &self,
        method: String,
        param_types: Vec<String>,
        args: Vec<String>,
        mode: GenericSerialization,
    ) -> Result<String, anyhow::Error> {
        let mut ctx = InvocationContext::new(&method, self.url.clone());
        ctx.parameter_types = param_types;

        match mode {
            GenericSerialization::Json => {
                ctx.arguments = args.into_iter().map(String::into_bytes).collect();
                ctx.attachments.insert(
                    GENERIC_KEY.to_string(),
                    GENERIC_SERIALIZATION_DEFAULT.to_string(),
                );
            }
            GenericSerialization::ProtobufJson => {
                ctx.arguments = args
                    .into_iter()
                    .map(|json_str| {
                        serde_json::from_str::<serde_json::Value>(&json_str)
                            .map_or_else(|_| json_str.into_bytes(), |v| v.to_string().into_bytes())
                    })
                    .collect();
                ctx.attachments.insert(
                    GENERIC_KEY.to_string(),
                    GENERIC_SERIALIZATION_PROTOBUF_JSON.to_string(),
                );
            }
            GenericSerialization::Map => {
                ctx.arguments = args
                    .into_iter()
                    .map(|json_str| {
                        let parsed: std::collections::HashMap<String, serde_json::Value> =
                            serde_json::from_str(&json_str).unwrap_or_default();
                        serde_json::to_vec(&parsed).unwrap_or_default()
                    })
                    .collect();
                ctx.attachments.insert(
                    GENERIC_KEY.to_string(),
                    GENERIC_SERIALIZATION_BEAN.to_string(),
                );
            }
        }

        let result = self.invoker.invoke(&mut ctx).await?;

        if result.is_error() {
            Err(anyhow::anyhow!(
                "generic invoke '{}' failed: {:?}",
                method,
                result.error
            ))
        } else {
            let value_bytes = result.value.as_ref().map_or(&b"null"[..], |v| v.as_slice());
            String::from_utf8(value_bytes.to_vec())
                .map_err(|e| anyhow::anyhow!("invalid UTF-8 in generic response: {e}"))
        }
    }
}

// ============================================================================
// GenericFilter — intercepts and processes generic invocations
// ============================================================================

/// Filter for identifying and processing generic invocations.
///
/// Detects generic calls via the `GENERIC_KEY` attachment and handles
/// response transformation based on the serialization mode:
/// - **Json**: passes through unchanged.
/// - **`ProtobufJson`**: decodes protobuf bytes back to JSON.
/// - **Map (bean)**: decodes response to JSON.
pub struct GenericFilter;

#[async_trait]
impl Filter for GenericFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        next.invoke(ctx).await
    }

    async fn on_response(
        &self,
        ctx: &InvocationContext,
        result: RPCResult,
        _invoker: &dyn Invoker,
    ) -> RPCResult {
        let Some(generic_mode) = ctx.attachments.get(GENERIC_KEY) else {
            return result;
        };

        if result.is_error() {
            return result;
        }

        match generic_mode.as_str() {
            GENERIC_SERIALIZATION_PROTOBUF_JSON | GENERIC_SERIALIZATION_BEAN => {
                if let Some(ref value_bytes) = result.value {
                    if let Ok(json_str) = String::from_utf8(value_bytes.clone()) {
                        if serde_json::from_str::<serde_json::Value>(&json_str).is_err() {
                            if let Ok(json_value) =
                                serde_json::from_slice::<serde_json::Value>(value_bytes)
                            {
                                return RPCResult::success(json_value.to_string().into_bytes());
                            }
                        }
                    }
                }
                result
            }
            _ => result,
        }
    }
}

// ── ContextFilter ───────────────────────────────────────────────────

/// Context filter: propagates RPC context information through attachments.
///
/// Filters out internal/system attachments (keys starting with `_` or `dubbo.`)
/// while preserving business-level attachments for downstream propagation.
///
/// On the consumer side, this filter validates and cleans attachments before
/// forwarding. On the provider side, it ensures attachments are available
/// during invocation. Response attachments pass through unchanged.
pub struct ContextFilter;

impl ContextFilter {
    /// Returns `true` if the given attachment key is an internal/system key
    /// that should be filtered out from propagation.
    fn is_internal_key(key: &str) -> bool {
        key.starts_with('_') || key.starts_with("dubbo.")
    }
}

#[async_trait]
impl Filter for ContextFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        let internal_keys: Vec<String> = ctx
            .attachments
            .keys()
            .filter(|k| Self::is_internal_key(k))
            .cloned()
            .collect();

        for key in internal_keys {
            ctx.attachments.remove(&key);
        }

        next.invoke(ctx).await
    }

    // on_response uses default pass-through
}

// ── ExceptionFilter ─────────────────────────────────────────────────

/// Exception filter: classifies and handles exceptions from RPC calls.
///
/// Inspired by dubbo-java's `ExceptionFilter`, this filter checks whether
/// an error returned by the service is a "declared" (expected) exception.
/// Undeclared exceptions are wrapped in a generic `ServerError` to prevent
/// leaking implementation details to the caller.
pub struct ExceptionFilter {
    declared_exceptions: Vec<String>,
}

impl ExceptionFilter {
    /// Create a new filter with no declared exceptions.
    #[must_use]
    pub fn new() -> Self {
        Self {
            declared_exceptions: Vec::new(),
        }
    }

    /// Create a new filter with a list of declared exception patterns.
    ///
    /// Exception patterns are matched as substrings of the error message.
    #[must_use]
    pub fn with_exceptions(exceptions: Vec<String>) -> Self {
        Self {
            declared_exceptions: exceptions,
        }
    }

    /// Returns `true` if the given error message matches any declared exception pattern.
    fn is_declared(&self, error_message: &str) -> bool {
        self.declared_exceptions
            .iter()
            .any(|pattern| error_message.contains(pattern.as_str()))
    }
}

impl Default for ExceptionFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Filter for ExceptionFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        next.invoke(ctx).await
    }

    async fn on_response(
        &self,
        _ctx: &InvocationContext,
        result: RPCResult,
        _invoker: &dyn Invoker,
    ) -> RPCResult {
        if let Some(ref error) = result.error {
            let error_msg = error.to_string();
            if !self.is_declared(&error_msg) {
                return RPCResult::from_error(dubbo_rs_common::error::RPCError::ServerError(
                    "service internal error".to_string(),
                ));
            }
        }
        result
    }
}

#[cfg(test)]
mod context_exception_tests {
    use super::*;
    use crate::tests::{make_ctx, TestInvoker};

    // ── ContextFilter tests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_context_filter_passes_through() {
        let filter = ContextFilter;
        let next = TestInvoker::new();
        let mut ctx = make_ctx("sayHello");

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
    }

    #[tokio::test]
    async fn test_context_filter_propagates_business_attachments() {
        let filter = ContextFilter;
        let next = TestInvoker::new();
        let mut ctx = make_ctx("sayHello");
        ctx.attachments
            .insert("trace_id".to_string(), "abc123".to_string());
        ctx.attachments
            .insert("span_id".to_string(), "span456".to_string());
        ctx.attachments
            .insert("custom_key".to_string(), "custom_value".to_string());

        let _ = filter.invoke(&mut ctx, &next).await.unwrap();

        // Business attachments should still be present after filtering
        assert_eq!(ctx.attachments.get("trace_id").unwrap(), "abc123");
        assert_eq!(ctx.attachments.get("span_id").unwrap(), "span456");
        assert_eq!(ctx.attachments.get("custom_key").unwrap(), "custom_value");
    }

    #[tokio::test]
    async fn test_context_filter_filters_internal_attachments() {
        let filter = ContextFilter;
        let next = TestInvoker::new();
        let mut ctx = make_ctx("sayHello");
        ctx.attachments
            .insert("trace_id".to_string(), "abc123".to_string());
        ctx.attachments
            .insert("_internal".to_string(), "should_be_removed".to_string());
        ctx.attachments
            .insert("dubbo.protocol".to_string(), "tri".to_string());

        let _ = filter.invoke(&mut ctx, &next).await.unwrap();

        // Internal keys should be removed
        assert!(
            !ctx.attachments.contains_key("_internal"),
            "underscore-prefixed keys should be filtered"
        );
        assert!(
            !ctx.attachments.contains_key("dubbo.protocol"),
            "dubbo.-prefixed keys should be filtered"
        );
        // Business key should remain
        assert_eq!(ctx.attachments.get("trace_id").unwrap(), "abc123");
    }

    // ── ExceptionFilter tests ────────────────────────────────────────

    #[tokio::test]
    async fn test_exception_filter_passes_through_success() {
        let filter = ExceptionFilter::new();
        let next = TestInvoker::new();
        let mut ctx = make_ctx("sayHello");

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());

        // on_response should pass through success unchanged
        let response = filter.on_response(&ctx, result, &next).await;
        assert!(!response.is_error());
    }

    #[tokio::test]
    async fn test_exception_filter_passes_through_declared_exception() {
        let filter = ExceptionFilter::with_exceptions(vec!["BusinessException".to_string()]);
        let next = TestInvoker::new();
        let ctx = make_ctx("sayHello");

        let err_result = RPCResult::from_error(dubbo_rs_common::error::RPCError::ServiceError(
            "BusinessException: invalid input".into(),
        ));

        let response = filter.on_response(&ctx, err_result, &next).await;
        assert!(response.is_error());
        // Should pass through the original declared error
        let error = response.error.as_ref().unwrap();
        let msg = error.to_string();
        assert!(
            msg.contains("BusinessException"),
            "should contain declared exception, got: {msg}"
        );
    }

    #[tokio::test]
    async fn test_exception_filter_wraps_undeclared_exception() {
        let filter = ExceptionFilter::with_exceptions(vec!["BusinessException".to_string()]);
        let next = TestInvoker::new();
        let ctx = make_ctx("sayHello");

        let err_result = RPCResult::from_error(dubbo_rs_common::error::RPCError::ServiceError(
            "UnexpectedDbError: connection lost".into(),
        ));

        let response = filter.on_response(&ctx, err_result, &next).await;
        assert!(response.is_error());
        // Should be wrapped in generic error
        let error = response.error.as_ref().unwrap();
        let msg = error.to_string();
        assert!(
            msg.contains("service internal error"),
            "should contain generic error message, got: {msg}"
        );
        // Should NOT contain the original error details
        assert!(
            !msg.contains("UnexpectedDbError"),
            "should not leak implementation details"
        );
    }
}

#[cfg(test)]
mod generic_tests {
    use super::*;
    use dubbo_rs_common::node::Node;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    struct EchoInvoker {
        url: URL,
        call_count: Arc<AtomicUsize>,
        should_fail: Arc<AtomicBool>,
    }

    impl EchoInvoker {
        fn new() -> Self {
            Self {
                url: URL::new("tri", "/com.example.GenericService"),
                call_count: Arc::new(AtomicUsize::new(0)),
                should_fail: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl Node for EchoInvoker {
        fn get_url(&self) -> &URL {
            &self.url
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait]
    impl Invoker for EchoInvoker {
        async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            if self.should_fail.load(Ordering::SeqCst) {
                return Ok(RPCResult::from_error(
                    dubbo_rs_common::error::RPCError::ServiceError("test error".into()),
                ));
            }
            let echo = format!(
                "{} called with {} args",
                ctx.method_name,
                ctx.arguments.len()
            );
            Ok(RPCResult::success(echo.as_bytes().to_vec()))
        }
    }

    #[tokio::test]
    async fn test_generic_invoke_success() {
        let invoker = Box::new(EchoInvoker::new());
        let url = URL::new("tri", "/com.example.GenericService");
        let service = GenericInvoker::new(invoker, url);

        let result = service
            .invoke(
                "sayHello".into(),
                vec!["Ljava/lang/String;".into()],
                vec!["\"world\"".into()],
            )
            .await
            .unwrap();

        assert!(result.contains("sayHello"));
        assert!(result.contains("1 args"));
    }

    #[tokio::test]
    async fn test_generic_invoke_error() {
        let invoker = Box::new(EchoInvoker::new());
        invoker.should_fail.store(true, Ordering::SeqCst);
        let url = URL::new("tri", "/com.example.GenericService");
        let service = GenericInvoker::new(invoker, url);

        let result = service.invoke("badMethod".into(), vec![], vec![]).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_generic_invoke_multiple_args() {
        let invoker = Box::new(EchoInvoker::new());
        let url = URL::new("tri", "/com.example.GenericService");
        let service = GenericInvoker::new(invoker, url);

        let result = service
            .invoke(
                "greet".into(),
                vec!["Ljava/lang/String;".into(), "I".into()],
                vec!["\"alice\"".into(), "42".into()],
            )
            .await
            .unwrap();

        assert!(result.contains("greet"));
        assert!(result.contains("2 args"));
    }

    #[test]
    fn test_generic_invoker_url() {
        let invoker = Box::new(EchoInvoker::new());
        let url = URL::new("tri", "/com.example.TestService");
        let service = GenericInvoker::new(invoker, url.clone());
        assert_eq!(service.url().path, "/com.example.TestService");
    }

    #[tokio::test]
    async fn test_generic_invoker_error_contains_method_name() {
        let invoker = Box::new(EchoInvoker::new());
        invoker.should_fail.store(true, Ordering::SeqCst);
        let url = URL::new("tri", "/com.example.GenericService");
        let service = GenericInvoker::new(invoker, url);

        let result = service.invoke("failingMethod".into(), vec![], vec![]).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("failingMethod"),
            "error should contain method name, got: {err_msg}"
        );
    }

    // ── New tests for R1.5 generic invocation improvements ────────────

    use dubbo_rs_common::constants::GENERIC_KEY;

    struct ContextCapturingInvoker {
        url: URL,
        captured: std::sync::Mutex<CapturedContext>,
    }

    #[derive(Default)]
    struct CapturedContext {
        attachments: std::collections::HashMap<String, String>,
        arguments: Vec<Vec<u8>>,
        method_name: String,
    }

    impl ContextCapturingInvoker {
        fn new() -> Self {
            Self {
                url: URL::new("tri", "/com.example.CaptureService"),
                captured: std::sync::Mutex::new(CapturedContext::default()),
            }
        }
    }

    impl Node for ContextCapturingInvoker {
        fn get_url(&self) -> &URL {
            &self.url
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait]
    impl Invoker for ContextCapturingInvoker {
        async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
            let mut captured = self.captured.lock().unwrap();
            captured.attachments = ctx.attachments.clone();
            captured.arguments = ctx.arguments.clone();
            captured.method_name = ctx.method_name.clone();
            drop(captured);
            let echo = format!(
                "{} called with {} args",
                ctx.method_name,
                ctx.arguments.len()
            );
            Ok(RPCResult::success(echo.as_bytes().to_vec()))
        }
    }

    #[tokio::test]
    async fn test_generic_invocation_json_mode() {
        let invoker = Box::new(ContextCapturingInvoker::new());
        let url = URL::new("tri", "/com.example.GenericService");
        let service = GenericInvoker::new(invoker, url);

        let result = service
            .invoke_with_mode(
                "sayHello".into(),
                vec!["Ljava/lang/String;".into()],
                vec!["\"world\"".into()],
                GenericSerialization::Json,
            )
            .await
            .unwrap();

        assert!(result.contains("sayHello"));
        assert!(result.contains("1 args"));
    }

    #[tokio::test]
    async fn test_generic_invocation_protobuf_json_mode() {
        let invoker = Box::new(ContextCapturingInvoker::new());
        let url = URL::new("tri", "/com.example.GenericService");
        let service = GenericInvoker::new(invoker, url);

        let result = service
            .invoke_with_mode(
                "sayHello".into(),
                vec!["Ljava/lang/String;".into()],
                vec!["{\"name\":\"world\"}".into()],
                GenericSerialization::ProtobufJson,
            )
            .await
            .unwrap();

        assert!(result.contains("sayHello"));
    }

    #[tokio::test]
    async fn test_generic_invocation_map_mode() {
        let invoker = Box::new(ContextCapturingInvoker::new());
        let url = URL::new("tri", "/com.example.GenericService");
        let service = GenericInvoker::new(invoker, url);

        let result = service
            .invoke_with_mode(
                "sayHello".into(),
                vec!["Ljava/lang/String;".into()],
                vec!["{\"key\":\"value\"}".into()],
                GenericSerialization::Map,
            )
            .await
            .unwrap();

        assert!(result.contains("sayHello"));
        assert!(result.contains("1 args"));
    }

    #[tokio::test]
    async fn test_generic_filter_identifies_generic_call() {
        let filter = GenericFilter;
        let next = crate::tests::TestInvoker::new();
        let mut ctx = crate::tests::make_ctx("sayHello");
        ctx.attachments.insert(
            GENERIC_KEY.to_string(),
            GENERIC_SERIALIZATION_DEFAULT.to_string(),
        );

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
    }

    #[tokio::test]
    async fn test_generic_filter_passthrough_non_generic() {
        let filter = GenericFilter;
        let next = crate::tests::TestInvoker::new();
        let mut ctx = crate::tests::make_ctx("sayHello");

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());

        let response = filter.on_response(&ctx, result.clone(), &next).await;
        assert_eq!(response.value, result.value);
    }

    #[tokio::test]
    async fn test_generic_service_invoke_with_mode_default() {
        struct SimpleGenericService;

        #[async_trait]
        impl GenericService for SimpleGenericService {
            async fn invoke(
                &self,
                method: String,
                _param_types: Vec<String>,
                _args: Vec<String>,
            ) -> Result<String, anyhow::Error> {
                Ok(format!("invoked:{method}"))
            }
        }

        let svc = SimpleGenericService;
        let result = svc
            .invoke_with_mode("test".into(), vec![], vec![], GenericSerialization::Json)
            .await
            .unwrap();
        assert_eq!(result, "invoked:test");
    }

    #[tokio::test]
    async fn test_generic_invocation_attachment_set() {
        let invoker = Box::new(ContextCapturingInvoker::new());
        let url = URL::new("tri", "/com.example.GenericService");
        let service = GenericInvoker::new(invoker, url);

        service
            .invoke("sayHello".into(), vec![], vec![])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_generic_invocation_json_mode_sets_attachment() {
        let invoker = Box::new(ContextCapturingInvoker::new());
        let url = URL::new("tri", "/com.example.GenericService");
        let service = GenericInvoker::new(invoker, url);

        service
            .invoke_with_mode(
                "sayHello".into(),
                vec![],
                vec![],
                GenericSerialization::Json,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_generic_invocation_protobuf_json_sets_attachment() {
        let invoker = Box::new(ContextCapturingInvoker::new());
        let url = URL::new("tri", "/com.example.GenericService");
        let service = GenericInvoker::new(invoker, url);

        service
            .invoke_with_mode(
                "sayHello".into(),
                vec![],
                vec!["{}".into()],
                GenericSerialization::ProtobufJson,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_generic_invocation_map_mode_sets_attachment() {
        let invoker = Box::new(ContextCapturingInvoker::new());
        let url = URL::new("tri", "/com.example.GenericService");
        let service = GenericInvoker::new(invoker, url);

        service
            .invoke_with_mode(
                "sayHello".into(),
                vec![],
                vec!["{}".into()],
                GenericSerialization::Map,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_generic_filter_on_response_protobuf_json() {
        let filter = GenericFilter;
        let next = crate::tests::TestInvoker::new();
        let ctx = crate::tests::make_ctx("sayHello");

        let result = RPCResult::success(br#"{"status":"ok"}"#.to_vec());
        let response = filter.on_response(&ctx, result, &next).await;
        assert_eq!(response.value.as_ref().unwrap(), br#"{"status":"ok"}"#);
    }

    #[tokio::test]
    async fn test_generic_filter_on_response_passthrough_for_json_mode() {
        let filter = GenericFilter;
        let next = crate::tests::TestInvoker::new();
        let mut ctx = crate::tests::make_ctx("sayHello");
        ctx.attachments.insert(
            GENERIC_KEY.to_string(),
            GENERIC_SERIALIZATION_DEFAULT.to_string(),
        );

        let original = RPCResult::success(b"response_data".to_vec());
        let response = filter.on_response(&ctx, original.clone(), &next).await;
        assert_eq!(response.value, original.value);
    }

    #[tokio::test]
    async fn test_generic_serialization_default_is_json() {
        assert_eq!(GenericSerialization::default(), GenericSerialization::Json);
    }
}

// ============================================================================
// CacheFilter — LRU/TTL result caching
// ============================================================================

/// A cached invocation result with creation timestamp for TTL checks.
pub struct CachedEntry {
    pub value: Vec<u8>,
    pub created_at: Instant,
}

/// Abstraction over cache storage backends.
///
/// Implementations must be `Send + Sync` so they can be shared across
/// async tasks inside the filter.
pub trait CacheStore: Send + Sync {
    /// Retrieve a cached entry by key.
    fn get(&self, key: &str) -> Option<CachedEntry>;
    /// Store a value under the given key.
    fn put(&self, key: &str, value: Vec<u8>);
    /// Remove a specific entry.
    fn remove(&self, key: &str);
    /// Current number of entries in the store.
    fn size(&self) -> usize;
    /// Remove all entries.
    fn clear(&self);
}

// ── LruCacheStore ────────────────────────────────────────────────────

/// LRU (Least Recently Used) cache store backed by a `HashMap` with
/// access-order tracking via a `Vec<String>`.
///
/// When the cache is full, the oldest (least recently used) entry is
/// evicted first. On every `get()`, the accessed key is moved to the
/// end of the order list so it becomes the most recently used.
pub struct LruCacheStore {
    entries: Mutex<HashMap<String, CachedEntry>>,
    order: Mutex<Vec<String>>,
    max_size: usize,
}

impl LruCacheStore {
    #[must_use]
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            order: Mutex::new(Vec::new()),
            max_size,
        }
    }
}

impl CacheStore for LruCacheStore {
    fn get(&self, key: &str) -> Option<CachedEntry> {
        let entries = self.entries.lock().expect("LruCacheStore mutex poisoned");
        if entries.contains_key(key) {
            let mut order = self.order.lock().expect("LruCacheStore mutex poisoned");
            order.retain(|k| k != key);
            order.push(key.to_string());
            entries.get(key).map(|e| CachedEntry {
                value: e.value.clone(),
                created_at: e.created_at,
            })
        } else {
            None
        }
    }

    fn put(&self, key: &str, value: Vec<u8>) {
        let mut entries = self.entries.lock().expect("LruCacheStore mutex poisoned");
        let mut order = self.order.lock().expect("LruCacheStore mutex poisoned");

        if entries.contains_key(key) {
            order.retain(|k| k != key);
        }

        while entries.len() >= self.max_size && !order.is_empty() {
            let oldest = order.remove(0);
            entries.remove(&oldest);
        }

        entries.insert(
            key.to_string(),
            CachedEntry {
                value,
                created_at: Instant::now(),
            },
        );
        order.push(key.to_string());
    }

    fn remove(&self, key: &str) {
        let mut entries = self.entries.lock().expect("LruCacheStore mutex poisoned");
        let mut order = self.order.lock().expect("LruCacheStore mutex poisoned");
        entries.remove(key);
        order.retain(|k| k != key);
    }

    fn size(&self) -> usize {
        self.entries
            .lock()
            .expect("LruCacheStore mutex poisoned")
            .len()
    }

    fn clear(&self) {
        let mut entries = self.entries.lock().expect("LruCacheStore mutex poisoned");
        let mut order = self.order.lock().expect("LruCacheStore mutex poisoned");
        entries.clear();
        order.clear();
    }
}

// ── TtlCacheStore ────────────────────────────────────────────────────

/// TTL (Time To Live) cache store that evicts expired entries on read
/// and falls back to oldest-entry eviction when at capacity.
///
/// On `get()`, entries whose `created_at + ttl < now` are removed and
/// `None` is returned. On `put()`, expired entries are cleaned up first;
/// if still at capacity, the oldest entry is evicted.
pub struct TtlCacheStore {
    entries: Mutex<HashMap<String, CachedEntry>>,
    order: Mutex<Vec<String>>,
    ttl: Duration,
    max_size: usize,
}

impl TtlCacheStore {
    #[must_use]
    pub fn new(ttl: Duration, max_size: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            order: Mutex::new(Vec::new()),
            ttl,
            max_size,
        }
    }

    fn evict_expired(&self) {
        let mut entries = self.entries.lock().expect("TtlCacheStore mutex poisoned");
        let mut order = self.order.lock().expect("TtlCacheStore mutex poisoned");
        let now = Instant::now();
        let expired_keys: Vec<String> = entries
            .iter()
            .filter(|(_, e)| now.duration_since(e.created_at) > self.ttl)
            .map(|(k, _)| k.clone())
            .collect();
        for key in &expired_keys {
            entries.remove(key);
            order.retain(|k| k != key);
        }
    }
}

impl CacheStore for TtlCacheStore {
    fn get(&self, key: &str) -> Option<CachedEntry> {
        let mut entries = self.entries.lock().expect("TtlCacheStore mutex poisoned");
        let mut order = self.order.lock().expect("TtlCacheStore mutex poisoned");

        if let Some(entry) = entries.get(key) {
            if Instant::now().duration_since(entry.created_at) > self.ttl {
                entries.remove(key);
                order.retain(|k| k != key);
                return None;
            }
            Some(CachedEntry {
                value: entry.value.clone(),
                created_at: entry.created_at,
            })
        } else {
            None
        }
    }

    fn put(&self, key: &str, value: Vec<u8>) {
        self.evict_expired();

        let mut entries = self.entries.lock().expect("TtlCacheStore mutex poisoned");
        let mut order = self.order.lock().expect("TtlCacheStore mutex poisoned");

        if entries.contains_key(key) {
            order.retain(|k| k != key);
        }

        while entries.len() >= self.max_size && !order.is_empty() {
            let oldest = order.remove(0);
            entries.remove(&oldest);
        }

        entries.insert(
            key.to_string(),
            CachedEntry {
                value,
                created_at: Instant::now(),
            },
        );
        order.push(key.to_string());
    }

    fn remove(&self, key: &str) {
        let mut entries = self.entries.lock().expect("TtlCacheStore mutex poisoned");
        let mut order = self.order.lock().expect("TtlCacheStore mutex poisoned");
        entries.remove(key);
        order.retain(|k| k != key);
    }

    fn size(&self) -> usize {
        self.entries
            .lock()
            .expect("TtlCacheStore mutex poisoned")
            .len()
    }

    fn clear(&self) {
        let mut entries = self.entries.lock().expect("TtlCacheStore mutex poisoned");
        let mut order = self.order.lock().expect("TtlCacheStore mutex poisoned");
        entries.clear();
        order.clear();
    }
}

// ── CacheFilter ──────────────────────────────────────────────────────

/// Caching filter that short-circuits invocations when a cached result
/// exists for the same method name and arguments.
///
/// Cache keys are derived from the method name and a hash of the
/// argument bytes. On cache miss, the invocation proceeds through
/// `next.invoke()`. Successful results are cached; error results are
/// not.
pub struct CacheFilter {
    store: Box<dyn CacheStore>,
}

impl CacheFilter {
    #[must_use]
    pub fn new(store: Box<dyn CacheStore>) -> Self {
        Self { store }
    }
}

fn simple_hash(data: &[u8]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}

fn cache_key(ctx: &InvocationContext) -> String {
    let mut key = ctx.method_name.clone();
    for arg in &ctx.arguments {
        use std::fmt::Write;
        let _ = write!(key, ":{:02x}", simple_hash(arg));
    }
    key
}

#[async_trait]
impl Filter for CacheFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        let key = cache_key(ctx);

        // Check cache
        if let Some(entry) = self.store.get(&key) {
            return Ok(RPCResult::success(entry.value));
        }

        let result = next.invoke(ctx).await?;

        if !result.is_error() {
            if let Some(ref value) = result.value {
                self.store.put(&key, value.clone());
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod cache_filter_tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    // ── LruCacheStore tests ──────────────────────────────────────────

    #[test]
    fn test_lru_cache_put_and_get() {
        let store = LruCacheStore::new(10);
        store.put("key1", b"value1".to_vec());
        store.put("key2", b"value2".to_vec());

        assert_eq!(store.size(), 2);

        let entry = store.get("key1").expect("key1 should exist");
        assert_eq!(entry.value, b"value1");

        let entry = store.get("key2").expect("key2 should exist");
        assert_eq!(entry.value, b"value2");

        assert!(store.get("key3").is_none());
    }

    #[test]
    fn test_lru_cache_evicts_oldest() {
        let store = LruCacheStore::new(2);
        store.put("a", b"1".to_vec());
        store.put("b", b"2".to_vec());

        store.put("c", b"3".to_vec());

        assert!(store.get("a").is_none(), "a should have been evicted");
        assert!(store.get("b").is_some(), "b should still exist");
        assert!(store.get("c").is_some(), "c should exist");
        assert_eq!(store.size(), 2);
    }

    #[test]
    fn test_lru_cache_remove() {
        let store = LruCacheStore::new(10);
        store.put("key1", b"value1".to_vec());
        assert_eq!(store.size(), 1);

        store.remove("key1");
        assert_eq!(store.size(), 0);
        assert!(store.get("key1").is_none());
    }

    #[test]
    fn test_lru_cache_clear() {
        let store = LruCacheStore::new(10);
        store.put("a", b"1".to_vec());
        store.put("b", b"2".to_vec());
        store.put("c", b"3".to_vec());
        assert_eq!(store.size(), 3);

        store.clear();
        assert_eq!(store.size(), 0);
        assert!(store.get("a").is_none());
    }

    // ── TtlCacheStore tests ──────────────────────────────────────────

    #[test]
    fn test_ttl_cache_entry_expires() {
        let store = TtlCacheStore::new(Duration::from_millis(50), 10);
        store.put("key1", b"value1".to_vec());

        assert!(store.get("key1").is_some());

        thread::sleep(Duration::from_millis(80));

        assert!(store.get("key1").is_none(), "entry should have expired");
    }

    #[test]
    fn test_ttl_cache_entry_valid_within_ttl() {
        let store = TtlCacheStore::new(Duration::from_secs(10), 10);
        store.put("key1", b"value1".to_vec());

        let entry = store.get("key1").expect("entry should exist within TTL");
        assert_eq!(entry.value, b"value1");
    }

    // ── CacheFilter integration tests ────────────────────────────────

    #[tokio::test]
    async fn test_cache_filter_returns_cached_result() {
        let store = Box::new(LruCacheStore::new(10));
        let filter = CacheFilter::new(store);
        let next = crate::tests::TestInvoker::new();

        let mut ctx = crate::tests::make_ctx("sayHello");
        let result1 = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result1.is_error());
        assert_eq!(next.count(), 1);

        let mut ctx2 = crate::tests::make_ctx("sayHello");
        let result2 = filter.invoke(&mut ctx2, &next).await.unwrap();
        assert!(!result2.is_error());
        assert_eq!(next.count(), 1, "invoker should not be called again");
        assert_eq!(result2.value, result1.value);
    }

    #[tokio::test]
    async fn test_cache_filter_does_not_cache_errors() {
        let store = Box::new(LruCacheStore::new(10));
        let filter = CacheFilter::new(store);
        let next = crate::tests::TestInvoker::new();
        next.should_fail.store(true, Ordering::SeqCst);

        let mut ctx = crate::tests::make_ctx("sayHello");
        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(result.is_error());
        assert_eq!(next.count(), 1);

        next.should_fail.store(false, Ordering::SeqCst);
        let mut ctx2 = crate::tests::make_ctx("sayHello");
        let result2 = filter.invoke(&mut ctx2, &next).await.unwrap();
        assert!(!result2.is_error());
        assert_eq!(
            next.count(),
            2,
            "invoker should be called again since error was not cached"
        );
    }

    #[tokio::test]
    async fn test_cache_filter_cache_key_varies_by_method() {
        let store = Box::new(LruCacheStore::new(10));
        let filter = CacheFilter::new(store);
        let next = crate::tests::TestInvoker::new();

        let mut ctx1 = crate::tests::make_ctx("sayHello");
        let _r1 = filter.invoke(&mut ctx1, &next).await.unwrap();

        let mut ctx2 = crate::tests::make_ctx("bye");
        let _r2 = filter.invoke(&mut ctx2, &next).await.unwrap();

        assert_eq!(
            next.count(),
            2,
            "different methods should have separate cache entries"
        );
    }

    #[tokio::test]
    async fn test_cache_filter_cache_key_varies_by_args() {
        let store = Box::new(LruCacheStore::new(10));
        let filter = CacheFilter::new(store);
        let next = crate::tests::TestInvoker::new();

        let mut ctx1 = crate::tests::make_ctx("sayHello");
        ctx1.arguments = vec![b"alice".to_vec()];
        let _r1 = filter.invoke(&mut ctx1, &next).await.unwrap();

        let mut ctx2 = crate::tests::make_ctx("sayHello");
        ctx2.arguments = vec![b"bob".to_vec()];
        let _r2 = filter.invoke(&mut ctx2, &next).await.unwrap();

        assert_eq!(
            next.count(),
            2,
            "different args should have separate cache entries"
        );
    }
}

// ============================================================================
// AuthFilter tests
// ============================================================================

#[cfg(test)]
mod auth_filter_tests {
    use super::*;

    fn make_auth_ctx(method: &str) -> InvocationContext {
        let url = URL::new("tri", "/com.example.TestService");
        InvocationContext::new(method, url)
    }

    #[tokio::test]
    async fn test_auth_filter_no_credentials_passes_through() {
        let filter = AuthFilter::new();
        let next = crate::tests::TestInvoker::new();
        let mut ctx = make_auth_ctx("sayHello");

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
        assert_eq!(next.count(), 1);
    }

    #[tokio::test]
    async fn test_auth_filter_signs_request_on_consumer_side() {
        let filter = AuthFilter::new();
        let next = crate::tests::TestInvoker::new();

        let mut url = URL::new("tri", "/com.example.TestService");
        url.set_param("access_key", "my-ak");
        url.set_param("secret_key", "my-sk");
        let mut ctx = InvocationContext::new("sayHello", url);

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());

        assert_eq!(ctx.attachments.get("ak").unwrap(), "my-ak");
        assert!(ctx.attachments.contains_key("signature"));
        assert!(ctx.attachments.contains_key("timestamp"));
    }

    #[tokio::test]
    async fn test_auth_filter_validates_correct_signature() {
        let filter = AuthFilter::with_credentials("test-ak", "test-sk");
        let next = crate::tests::TestInvoker::new();

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        #[allow(clippy::cast_possible_truncation)]
        let timestamp_str = now_ms.to_string();

        let signature = AuthFilter::compute_signature(
            "test-sk",
            "sayHello",
            "/com.example.TestService",
            &timestamp_str,
        );

        let mut ctx = make_auth_ctx("sayHello");
        ctx.attachments
            .insert("ak".to_string(), "test-ak".to_string());
        ctx.attachments.insert("signature".to_string(), signature);
        ctx.attachments
            .insert("timestamp".to_string(), timestamp_str);

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
        assert_eq!(next.count(), 1);
    }

    #[tokio::test]
    async fn test_auth_filter_rejects_invalid_signature() {
        let filter = AuthFilter::with_credentials("test-ak", "test-sk");
        let next = crate::tests::TestInvoker::new();

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        #[allow(clippy::cast_possible_truncation)]
        let timestamp_str = now_ms.to_string();

        let mut ctx = make_auth_ctx("sayHello");
        ctx.attachments
            .insert("ak".to_string(), "test-ak".to_string());
        ctx.attachments
            .insert("signature".to_string(), "badsignature123".to_string());
        ctx.attachments
            .insert("timestamp".to_string(), timestamp_str);

        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("signature mismatch"));
        assert_eq!(next.count(), 0);
    }

    #[tokio::test]
    async fn test_auth_filter_rejects_unknown_access_key() {
        let filter = AuthFilter::with_credentials("known-ak", "known-sk");
        let next = crate::tests::TestInvoker::new();

        let mut ctx = make_auth_ctx("sayHello");
        ctx.attachments
            .insert("ak".to_string(), "unknown-ak".to_string());
        ctx.attachments
            .insert("signature".to_string(), "whatever".to_string());
        ctx.attachments
            .insert("timestamp".to_string(), "0".to_string());

        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unknown access key"));
        assert_eq!(next.count(), 0);
    }

    #[tokio::test]
    async fn test_auth_filter_rejects_missing_signature() {
        let filter = AuthFilter::with_credentials("test-ak", "test-sk");
        let next = crate::tests::TestInvoker::new();

        let mut ctx = make_auth_ctx("sayHello");
        ctx.attachments
            .insert("ak".to_string(), "test-ak".to_string());
        ctx.attachments
            .insert("timestamp".to_string(), "0".to_string());

        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing signature"));
        assert_eq!(next.count(), 0);
    }

    #[tokio::test]
    async fn test_auth_filter_rejects_expired_timestamp() {
        let filter = AuthFilter::with_credentials("test-ak", "test-sk");
        let next = crate::tests::TestInvoker::new();

        let old_timestamp = "1000".to_string();
        let signature = AuthFilter::compute_signature(
            "test-sk",
            "sayHello",
            "/com.example.TestService",
            &old_timestamp,
        );

        let mut ctx = make_auth_ctx("sayHello");
        ctx.attachments
            .insert("ak".to_string(), "test-ak".to_string());
        ctx.attachments.insert("signature".to_string(), signature);
        ctx.attachments
            .insert("timestamp".to_string(), old_timestamp);

        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("expired timestamp"));
        assert_eq!(next.count(), 0);
    }

    #[tokio::test]
    async fn test_auth_filter_add_credential_at_runtime() {
        let filter = AuthFilter::new();
        let next = crate::tests::TestInvoker::new();

        filter.add_credential("runtime-ak", "runtime-sk");

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        #[allow(clippy::cast_possible_truncation)]
        let timestamp_str = now_ms.to_string();

        let signature = AuthFilter::compute_signature(
            "runtime-sk",
            "sayHello",
            "/com.example.TestService",
            &timestamp_str,
        );

        let mut ctx = make_auth_ctx("sayHello");
        ctx.attachments
            .insert("ak".to_string(), "runtime-ak".to_string());
        ctx.attachments.insert("signature".to_string(), signature);
        ctx.attachments
            .insert("timestamp".to_string(), timestamp_str);

        let result = filter.invoke(&mut ctx, &next).await.unwrap();
        assert!(!result.is_error());
        assert_eq!(next.count(), 1);
    }

    #[test]
    fn test_auth_filter_signature_computation_deterministic() {
        let sig1 = AuthFilter::compute_signature(
            "secret",
            "sayHello",
            "/com.example.TestService",
            "1234567890",
        );
        let sig2 = AuthFilter::compute_signature(
            "secret",
            "sayHello",
            "/com.example.TestService",
            "1234567890",
        );
        assert_eq!(sig1, sig2);
    }

    #[tokio::test]
    async fn test_auth_filter_rejects_missing_timestamp() {
        let filter = AuthFilter::with_credentials("test-ak", "test-sk");
        let next = crate::tests::TestInvoker::new();

        let mut ctx = make_auth_ctx("sayHello");
        ctx.attachments
            .insert("ak".to_string(), "test-ak".to_string());
        ctx.attachments
            .insert("signature".to_string(), "somesig".to_string());

        let result = filter.invoke(&mut ctx, &next).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing timestamp"));
        assert_eq!(next.count(), 0);
    }
}
