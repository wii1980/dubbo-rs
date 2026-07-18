# Phase 3: 过滤器与治理实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 实现 Filter trait + FilterChain 过滤器框架，4 个核心过滤器（Echo/Token/AccessLog/TPS），路由规则（ConditionRouter/TagRouter/RouterChain），服务治理过滤器（GracefulShutdown/ActiveLimit/ExecuteLimit），以及集成示例。

**Architecture:** 基于 Phase 2 的 Invoker/Protocol/Cluster 基础设施，在 dubbo-rs-filter crate 中实现 Filter trait 和 FilterChain（责任链模式），每个 Filter 包裹下一个 Invoker，invoke() 做前置处理，on_response() 做后置处理。路由规则实现在 dubbo-rs-cluster 中。

**Tech Stack:** Rust 1.87 (edition 2024), tokio, async-trait, tracing

**Current State:** Phase 0/1/2 完成 (~90 tests pass, clippy 零警告)。dubbo-rs-filter crate 为骨架状态（2 行 re-export），dubbo-rs-cluster 已有 Cluster/Directory/StaticDirectory/RegistryDirectory/FailoverCluster/FailfastCluster。

---

## 差距总览

| # | 差距项 | crate | 优先级 |
|---|--------|------|--------|
| 1 | `Filter` trait 未定义 | dubbo-rs-filter | P0 |
| 2 | `FilterChain` 未实现 | dubbo-rs-filter | P0 |
| 3 | `EchoFilter` 未实现 | dubbo-rs-filter | P0 |
| 4 | `TokenFilter` 未实现 | dubbo-rs-filter | P0 |
| 5 | `AccessLogFilter` 未实现 | dubbo-rs-filter | P0 |
| 6 | `TPSLimiter` + `TPSLimitFilter` 未实现 | dubbo-rs-filter | P0 |
| 7 | `ConditionRouter` 未实现 | dubbo-rs-cluster | P0 |
| 8 | `TagRouter` 未实现 | dubbo-rs-cluster | P0 |
| 9 | `RouterChain` 未实现 | dubbo-rs-cluster | P0 |
| 10 | `GracefulShutdownFilter` 未实现 | dubbo-rs-filter | P0 |
| 11 | `ActiveLimitFilter` 未实现 | dubbo-rs-filter | P0 |
| 12 | `ExecuteLimitFilter` 未实现 | dubbo-rs-filter | P0 |
| 13 | 集成示例未创建 | examples/ | P1 |

---

### Task 1: 实现 Filter trait 和 FilterChain

**Files:**
- Modify: `dubbo-filter/src/lib.rs`

**Step 1: 定义 Filter trait**

```rust
use async_trait::async_trait;
use dubbo_rs_protocol::{InvocationContext, Invoker, RPCResult};

/// A filter intercepts invocations in a chain-of-responsibility pattern.
///
/// `invoke()` is called before the downstream invoker. The filter may:
/// - Short-circuit the call and return early (e.g., token rejection)
/// - Modify the invocation context before passing it downstream
/// - Call `next.invoke(ctx)` to proceed down the chain
///
/// After the downstream returns, `on_response()` is called so the filter
/// can inspect or modify the result (e.g., log response, inject headers).
#[async_trait]
pub trait Filter: Send + Sync {
    /// Pre-process the invocation, then optionally call `next.invoke(ctx)`.
    ///
    /// # Errors
    ///
    /// Returns an error if the filter rejects the call or encounters
    /// an internal failure.
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error>;

    /// Post-process the RPC result before it is returned to the caller.
    ///
    /// Default implementation passes through the result unchanged.
    async fn on_response(
        &self,
        _ctx: &InvocationContext,
        result: RPCResult,
        _invoker: &dyn Invoker,
    ) -> RPCResult {
        result
    }
}
```

**Step 2: 实现 FilterChain**

FilterChain 是 Invoker 实现，按反向顺序构建链：最内层是 base invoker，每层 filter 包裹下一层。

```rust
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;

/// A chain of filters wrapping a base invoker.
///
/// Built from innermost (invoker) to outermost (first filter):
/// filter[0] → filter[1] → ... → filter[n] → base invoker
pub struct FilterChain {
    filters: Vec<Box<dyn Filter>>,
    invoker: Box<dyn Invoker>,
    url: URL,
}

impl FilterChain {
    #[must_use]
    pub fn new(filters: Vec<Box<dyn Filter>>, invoker: Box<dyn Invoker>, url: URL) -> Self {
        Self { filters, invoker, url }
    }
}

impl Node for FilterChain {
    fn get_url(&self) -> &URL { &self.url }
    fn is_available(&self) -> bool { true }
    fn destroy(&self) {}
}

/// Internal node: one link in the filter chain.
struct FilterNode {
    filter: Box<dyn Filter>,
    next: Box<dyn Invoker>,
}

impl Node for FilterNode {
    fn get_url(&self) -> &URL { self.next.get_url() }
    fn is_available(&self) -> bool { self.next.is_available() }
    fn destroy(&self) { self.next.destroy(); }
}

#[async_trait]
impl Invoker for FilterNode {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        let result = self.filter.invoke(ctx, self.next.as_ref()).await?;
        Ok(self.filter.on_response(ctx, result, self.next.as_ref()).await)
    }
}

// FilterChain as Invoker: build chain and delegate
#[async_trait]
impl Invoker for FilterChain {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        // Build chain from innermost to outermost
        let mut current: Box<dyn Invoker> = Box::new(FilterChainRef {
            url: self.url.clone(),
            invoker: &*self.invoker,
        });
        // Actually, since we need ownership, we build the chain differently...
        self.build_chain().invoke(ctx).await
    }
}
```

Wait — the chain construction needs ownership. Let me redesign:

```rust
impl FilterChain {
    /// Consume self and build a boxed Invoker chain.
    /// The chain runs filters in order wrapping the base invoker.
    pub fn build(mut self) -> Box<dyn Invoker> {
        #[allow(clippy::borrowed_box)]
        fn build_recursive(
            mut filters: Vec<Box<dyn Filter>>,
            next: Box<dyn Invoker>,
        ) -> Box<dyn Invoker> {
            if filters.is_empty() {
                next
            } else {
                // Take from the END — outermost filter runs first
                let filter = filters.pop().unwrap();
                let inner = build_recursive(filters, next);
                Box::new(FilterNode { filter, next: inner })
            }
        }
        build_recursive(self.filters, self.invoker)
    }
}
```

Actually, the chaining logic needs to be: filter[0].invoke → filter[1].invoke → ... → base invoker. So we pop from the end and wrap inward:

```
base_invoker = invoker
filter_node_n = FilterNode { filter: filters[n], next: base_invoker }
filter_node_n-1 = FilterNode { filter: filters[n-1], next: filter_node_n }
...
outermost = FilterNode { filter: filters[0], next: inner_chain }
```

Pop from end, wrap inward. That's correct.

Let me also make FilterChain itself implement Invoker by delegating to build().

**Step 3: 编写测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use dubbo_rs_common::node::Node;
    use dubbo_rs_common::url::URL;
    use dubbo_rs_protocol::{InvocationContext, Invoker, RPCResult};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct DummyInvoker {
        url: URL,
        call_count: Arc<AtomicUsize>,
    }

    impl DummyInvoker {
        fn new() -> Self {
            let url = URL::new("tri", "/test.Service");
            Self { url, call_count: Arc::new(AtomicUsize::new(0)) }
        }
    }

    impl Node for DummyInvoker {
        fn get_url(&self) -> &URL { &self.url }
        fn is_available(&self) -> bool { true }
        fn destroy(&self) {}
    }

    #[async_trait]
    impl Invoker for DummyInvoker {
        async fn invoke(&self, _ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(RPCResult::success(b"dummy".to_vec()))
        }
    }

    #[test]
    fn test_filter_chain_empty() {
        let invoker = Box::new(DummyInvoker::new());
        let chain = FilterChain::new(vec![], invoker, URL::new("tri", "/test"));
        // Empty chain should still work
    }

    #[test]
    fn test_filter_chain_build_preserves_order() {
        // Test that filters execute in order and base invoker is called
    }
}
```

**Step 4: 运行测试**

Run: `cargo test -p dubbo-rs-filter`
Expected: tests pass

**Step 5: Commit**

```bash
git add dubbo-filter/
git commit -m "feat(filter): define Filter trait and FilterChain implementation"
```

---

### Task 2: 实现 EchoFilter (健康检查)

**Files:**
- Modify: `dubbo-filter/src/lib.rs` (添加 EchoFilter)
- Modify: `dubbo-filter/Cargo.toml` (添加 tracing 依赖)

**Step 1: 实现 EchoFilter**

EchoFilter 检测 `$echo` 方法调用，直接返回 echo 响应，不调用下游。

```rust
use dubbo_rs_common::constants;

/// Health-check filter: intercepts `$echo` calls and returns the
/// method name as a response without forwarding to the downstream invoker.
pub struct EchoFilter;

#[async_trait]
impl Filter for EchoFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        if ctx.method_name == "$echo" {
            let echo = ctx.arguments.first()
                .cloned()
                .unwrap_or_else(|| ctx.method_name.as_bytes().to_vec());
            return Ok(RPCResult::success(echo));
        }
        next.invoke(ctx).await
    }
}
```

**Step 2: 编写测试**

测试：
- `$echo` 方法被短路返回
- 非 `$echo` 方法正常传递给下一个 invoker

**Step 3: 运行测试**

Run: `cargo test -p dubbo-rs-filter`
Expected: tests pass

**Step 4: Commit**

```bash
git add dubbo-filter/
git commit -m "feat(filter): implement EchoFilter for health check detection"
```

---

### Task 3: 实现 TokenFilter (令牌验证)

**Files:**
- Modify: `dubbo-filter/src/lib.rs`

**Step 1: 实现 TokenFilter**

TokenFilter 检查 invocation attachments 中的 token，如果与配置不匹配则拒绝调用。

```rust
/// Token-based authentication filter.
///
/// Checks the `token` attachment (or configured key) against the
/// expected value. Rejects the invocation with `RPCError::BadRequest`
/// if the token is missing or mismatched.
pub struct TokenFilter {
    token: String,
    /// Attachment key to check (default: "token")
    key: String,
}

impl TokenFilter {
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        Self { token: token.into(), key: "token".to_string() }
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
        let provided = ctx.attachments.get(&self.key);
        match provided {
            Some(t) if t == &self.token => next.invoke(ctx).await,
            Some(_) => Err(anyhow::anyhow!(
                "token mismatch: invalid token for {}", ctx.method_name
            )),
            None => Err(anyhow::anyhow!(
                "token missing: no '{}' in attachments for {}", self.key, ctx.method_name
            )),
        }
    }
}
```

**Step 2: 编写测试**

测试：
- 正确的 token 通过
- 错误的 token 被拒绝
- 缺失 token 被拒绝

**Step 3: 运行测试**

Run: `cargo test -p dubbo-rs-filter`
Expected: tests pass

**Step 4: Commit**

```bash
git add dubbo-filter/
git commit -m "feat(filter): implement TokenFilter for authentication"
```

---

### Task 4: 实现 AccessLogFilter (访问日志)

**Files:**
- Modify: `dubbo-filter/src/lib.rs`
- Modify: `dubbo-filter/Cargo.toml` (tracing 已在 Task 2 添加)

**Step 1: 实现 AccessLogFilter**

AccessLogFilter 在 `on_response()` 中记录调用日志（方法名、参数、结果、耗时）。

```rust
use std::time::Instant;

/// Access log filter: logs invocation details after the call completes.
///
/// Uses `tracing` for structured logging. Records method name,
/// elapsed time, and result status (success/error).
pub struct AccessLogFilter;

#[async_trait]
impl Filter for AccessLogFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        // Store start time in context for on_response
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
                "RPC call failed"
            );
        } else {
            tracing::info!(
                method = %ctx.method_name,
                "RPC call succeeded"
            );
        }
        result
    }
}
```

**Step 2: 编写测试**

测试：
- on_response 在成功时被调用
- on_response 在失败时被调用
- result 不受修改

**Step 3: 运行测试**

Run: `cargo test -p dubbo-rs-filter`
Expected: tests pass

**Step 4: Commit**

```bash
git add dubbo-filter/
git commit -m "feat(filter): implement AccessLogFilter with tracing integration"
```

---

### Task 5: 实现 TPSLimiter + TPSLimitFilter (TPS 限流)

**Files:**
- Modify: `dubbo-filter/src/lib.rs`

**Step 1: 实现 TPSLimiter trait 和 DefaultTPSLimiter**

使用令牌桶算法实现 TPS 限流。

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// TPS rate limiter interface.
pub trait TPSLimiter: Send + Sync {
    /// Check whether a call is allowed. Returns `true` if allowed.
    fn is_allowable(&self) -> bool;
}

/// Token-bucket based TPS limiter.
///
/// Refills `rate` tokens per second, up to `capacity` tokens.
pub struct DefaultTPSLimiter {
    rate: u64,           // tokens per second
    capacity: u64,       // max tokens
    tokens: AtomicU64,   // current tokens (scaled by 1000 for precision)
    last_refill: Mutex<Instant>,
}

impl DefaultTPSLimiter {
    #[must_use]
    pub fn new(rate: u64, capacity: u64) -> Self {
        Self {
            rate,
            capacity,
            tokens: AtomicU64::new(capacity * 1000),
            last_refill: Mutex::new(Instant::now()),
        }
    }

    fn refill(&self) {
        let mut last = self.last_refill.lock().unwrap();
        let elapsed = last.elapsed();
        if elapsed < Duration::from_millis(100) {
            return; // refill at most every 100ms
        }
        let add = (elapsed.as_secs_f64() * self.rate as f64 * 1000.0) as u64;
        let current = self.tokens.load(Ordering::Relaxed);
        let new = (current + add).min(self.capacity * 1000);
        self.tokens.store(new, Ordering::Relaxed);
        *last = Instant::now();
    }
}

impl TPSLimiter for DefaultTPSLimiter {
    fn is_allowable(&self) -> bool {
        self.refill();
        let mut current = self.tokens.load(Ordering::Relaxed);
        loop {
            if current < 1000 {
                return false;
            }
            match self.tokens.compare_exchange_weak(
                current, current - 1000,
                Ordering::SeqCst, Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
    }
}
```

**Step 2: 实现 TPSLimitFilter**

```rust
/// TPS rate-limiting filter.
///
/// Rejects invocations when the rate exceeds the configured limit.
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
            Err(anyhow::anyhow!("TPS limit exceeded for {}", ctx.method_name))
        }
    }
}
```

**Step 3: 编写测试**

测试：
- 在限制内允许调用
- 超出限制后拒绝调用
- 令牌桶正确 refill

**Step 4: 运行测试**

Run: `cargo test -p dubbo-rs-filter`
Expected: tests pass

**Step 5: Commit**

```bash
git add dubbo-filter/
git commit -m "feat(filter): implement TPSLimiter with token-bucket algorithm and TPSLimitFilter"
```

---

### Task 6: 实现 ConditionRouter (条件路由)

**Files:**
- Modify: `dubbo-cluster/src/lib.rs`

**Step 1: 实现 ConditionRouter**

条件路由根据 invocation context 中的参数匹配规则，过滤 invoker 列表。

```rust
/// Condition-based router — filters invokers by matching rules against
/// invocation context parameters.
///
/// Rules follow Dubbo condition routing syntax:
/// `key=value => key=value`
/// - Left side: match conditions (AND logic)
/// - Right side: filter conditions (AND logic)
pub struct ConditionRouter {
    /// Match rule: `[(key, value)]` — all must match for the rule to apply
    match_rules: Vec<(String, String)>,
    /// Filter rule: `[(key, value)]` — apply to invoker URL params
    filter_rules: Vec<(String, String)>,
}

impl ConditionRouter {
    /// Parse a condition rule string like `"region=beijing => env=gray"`.
    ///
    /// Returns `None` if no match rules (router always matches).
    pub fn parse(rule: &str) -> Option<Self> {
        let parts: Vec<&str> = rule.splitn(2, "=>").collect();
        let match_rules = Self::parse_kv_pairs(parts[0].trim());
        let filter_rules = if parts.len() > 1 {
            Self::parse_kv_pairs(parts[1].trim())
        } else {
            Vec::new()
        };
        Some(Self { match_rules, filter_rules })
    }

    fn parse_kv_pairs(s: &str) -> Vec<(String, String)> {
        s.split(',')
            .filter_map(|kv| {
                let mut parts = kv.splitn(2, '=');
                let k = parts.next()?.trim();
                let v = parts.next()?.trim();
                if k.is_empty() { None } else { Some((k.to_string(), v.to_string())) }
            })
            .collect()
    }

    /// Check if this router applies to the given invocation.
    fn matches_invocation(&self, ctx: &InvocationContext) -> bool {
        self.match_rules.iter().all(|(k, v)| {
            ctx.attachments.get(k).map_or(false, |val| val == v)
        })
    }

    /// Filter invokers by the filter rules against invoker URL params.
    fn filter_invokers(&self, invokers: &[Box<dyn Invoker>]) -> Vec<usize> {
        invokers.iter().enumerate().filter(|(_, invoker)| {
            self.filter_rules.iter().all(|(k, v)| {
                invoker.get_url().get_param(k).map_or(false, |val| val == v)
            })
        }).map(|(i, _)| i).collect()
    }
}
```

**Step 2: 编写测试**

测试：
- 解析形如 `key=val => key=val` 的规则
- 不匹配时不过滤
- 匹配时正确过滤

**Step 3: 运行测试**

Run: `cargo test -p dubbo-rs-cluster`
Expected: tests pass

**Step 4: Commit**

```bash
git add dubbo-cluster/
git commit -m "feat(cluster): implement ConditionRouter for conditional routing"
```

---

### Task 7: 实现 TagRouter (标签路由)

**Files:**
- Modify: `dubbo-cluster/src/lib.rs`

**Step 1: 实现 TagRouter**

TagRouter 基于标签进行流量染色路由。从 invocation attachments 中获取 `dubbo.tag`，匹配 invoker URL 中的 tag 参数。

```rust
/// Tag-based router for traffic coloring.
///
/// Reads the `dubbo.tag` attachment from the invocation context and routes
/// to invokers whose `tag` URL parameter matches. Falls back to invokers
/// without any tag if no tagged invoker matches.
pub struct TagRouter {
    tag_key: String,
}

impl TagRouter {
    #[must_use]
    pub fn new() -> Self {
        Self { tag_key: "dubbo.tag".to_string() }
    }

    #[must_use]
    pub fn with_tag_key(mut self, key: impl Into<String>) -> Self {
        self.tag_key = key.into();
        self
    }
}

impl TagRouter {
    /// Filter invokers to those matching the requested tag.
    /// If no tagged invokers match, falls back to non-tagged invokers.
    pub fn route(&self, invokers: &[Box<dyn Invoker>], ctx: &InvocationContext) -> Vec<usize> {
        let requested_tag = ctx.attachments.get(&self.tag_key);

        let Some(tag) = requested_tag else {
            // No tag requested — return all invokers
            return (0..invokers.len()).collect();
        };

        // First try: invokers with matching tag
        let tagged: Vec<usize> = invokers.iter().enumerate()
            .filter(|(_, inv)| {
                inv.get_url().get_param("tag").map_or(false, |t| t == tag)
            })
            .map(|(i, _)| i)
            .collect();

        if !tagged.is_empty() {
            return tagged;
        }

        // Fallback: invokers without any tag
        invokers.iter().enumerate()
            .filter(|(_, inv)| inv.get_url().get_param("tag").is_none())
            .map(|(i, _)| i)
            .collect()
    }
}

impl Default for TagRouter {
    fn default() -> Self { Self::new() }
}
```

**Step 2: 编写测试**

测试：
- 无 tag 请求时返回所有 invoker
- 匹配 tag 时只返回匹配的
- fallback 到无 tag 的 invoker

**Step 3: 运行测试**

Run: `cargo test -p dubbo-rs-cluster`
Expected: tests pass

**Step 4: Commit**

```bash
git add dubbo-cluster/
git commit -m "feat(cluster): implement TagRouter for traffic coloring"
```

---

### Task 8: 实现 RouterChain (路由链)

**Files:**
- Modify: `dubbo-cluster/src/lib.rs`

**Step 1: 实现 RouterChain**

RouterChain 组合多个 Router，按顺序过滤 invoker 列表。

```rust
/// A chain of routers that filters invokers sequentially.
///
/// Each router in the chain receives the filtered output of the previous
/// router. If any router filters out all invokers, the chain stops and
/// returns an empty list.
pub struct RouterChain {
    condition_router: Option<ConditionRouter>,
    tag_router: Option<TagRouter>,
}

impl RouterChain {
    #[must_use]
    pub fn new() -> Self {
        Self { condition_router: None, tag_router: None }
    }

    pub fn with_condition_router(mut self, router: ConditionRouter) -> Self {
        self.condition_router = Some(router);
        self
    }

    pub fn with_tag_router(mut self, router: TagRouter) -> Self {
        self.tag_router = Some(router);
        self
    }

    /// Route invokers through all configured routers.
    ///
    /// Returns the indices of invokers that pass all routing rules.
    pub fn route(
        &self,
        invokers: &[Box<dyn Invoker>],
        ctx: &InvocationContext,
    ) -> Vec<usize> {
        let all_indices: Vec<usize> = (0..invokers.len()).collect();

        let mut current = all_indices;

        if let Some(ref cr) = self.condition_router {
            if cr.matches_invocation(ctx) {
                let filtered: Vec<usize> = cr.filter_invokers(invokers);
                // Intersect with current selection
                current.retain(|i| filtered.contains(i));
                if current.is_empty() {
                    return current;
                }
            }
        }

        if let Some(ref tr) = self.tag_router {
            let filtered = tr.route(invokers, ctx);
            current.retain(|i| filtered.contains(i));
        }

        current
    }
}

impl Default for RouterChain {
    fn default() -> Self { Self::new() }
}
```

**Step 2: 编写测试**

测试：
- 空链返回所有 invoker
- 条件路由正确过滤
- 标签路由正确过滤
- 组合链正确执行

**Step 3: 运行测试**

Run: `cargo test -p dubbo-rs-cluster`
Expected: tests pass

**Step 4: Commit**

```bash
git add dubbo-cluster/
git commit -m "feat(cluster): implement RouterChain for sequential router filtering"
```

---

### Task 9: 实现 GracefulShutdownFilter (优雅关闭)

**Files:**
- Modify: `dubbo-filter/src/lib.rs`

**Step 1: 实现 GracefulShutdownFilter**

优雅关闭过滤器：检测关闭信号，拒绝新请求，等待进行中的请求完成。

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Graceful shutdown filter: rejects new invocations when the system
/// is shutting down, allowing in-flight requests to complete.
pub struct GracefulShutdownFilter {
    shutdown_flag: Arc<AtomicBool>,
}

impl GracefulShutdownFilter {
    #[must_use]
    pub fn new() -> Self {
        Self { shutdown_flag: Arc::new(AtomicBool::new(false)) }
    }

    #[must_use]
    pub fn from_flag(flag: Arc<AtomicBool>) -> Self {
        Self { shutdown_flag: flag }
    }

    /// Signal that shutdown has started.
    pub fn shutdown(&self) {
        self.shutdown_flag.store(true, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_shutdown(&self) -> bool {
        self.shutdown_flag.load(Ordering::SeqCst)
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
                "server is shutting down, rejecting call to {}", ctx.method_name
            ));
        }
        next.invoke(ctx).await
    }
}

impl Default for GracefulShutdownFilter {
    fn default() -> Self { Self::new() }
}
```

**Step 2: 编写测试**

测试：
- 正常时可调用
- 关闭信号后拒绝新调用
- 共享 flag 的多个实例同步

**Step 3: 运行测试**

Run: `cargo test -p dubbo-rs-filter`
Expected: tests pass

**Step 4: Commit**

```bash
git add dubbo-filter/
git commit -m "feat(filter): implement GracefulShutdownFilter"
```

---

### Task 10: 实现 ActiveLimitFilter (活跃调用数限制)

**Files:**
- Modify: `dubbo-filter/src/lib.rs`

**Step 1: 实现 ActiveLimitFilter**

限制同时进行的活跃调用数量，超过限制的请求被拒绝或排队。

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

/// Limits the number of concurrently active invocations.
///
/// Increments a counter before calling the downstream invoker,
/// decrements after the call completes (on_response).
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
                "active limit reached ({}/{}) for {}",
                current, self.max_active, ctx.method_name
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
```

**Step 2: 编写测试**

测试：
- 限制内允许并发调用
- 超出限制拒绝
- 完成后计数减少

**Step 3: 运行测试**

Run: `cargo test -p dubbo-rs-filter`
Expected: tests pass

**Step 4: Commit**

```bash
git add dubbo-filter/
git commit -m "feat(filter): implement ActiveLimitFilter for concurrency control"
```

---

### Task 11: 实现 ExecuteLimitFilter (并发执行限制)

**Files:**
- Modify: `dubbo-filter/src/lib.rs`

**Step 1: 实现 ExecuteLimitFilter**

ExecuteLimitFilter 使用信号量模式限制并发执行数量。

```rust
use tokio::sync::Semaphore;
use std::sync::Arc;

/// Execution concurrency limiter using a semaphore.
///
/// Limits the number of concurrent executions through permit acquisition.
/// Calls that cannot acquire a permit immediately are rejected (non-blocking).
pub struct ExecuteLimitFilter {
    semaphore: Arc<Semaphore>,
}

impl ExecuteLimitFilter {
    #[must_use]
    pub fn new(max_concurrent: usize) -> Self {
        Self { semaphore: Arc::new(Semaphore::new(max_concurrent)) }
    }
}

#[async_trait]
impl Filter for ExecuteLimitFilter {
    async fn invoke(
        &self,
        ctx: &mut InvocationContext,
        next: &dyn Invoker,
    ) -> Result<RPCResult, anyhow::Error> {
        // Try to acquire a permit without waiting
        let permit = match self.semaphore.try_acquire() {
            Ok(p) => p,
            Err(_) => {
                return Err(anyhow::anyhow!(
                    "execution limit reached for {}", ctx.method_name
                ));
            }
        };

        let result = next.invoke(ctx).await;

        // Permit is dropped here, releasing the slot
        drop(permit);

        result
    }
}
```

**Step 2: 编写测试**

测试：
- 许可内可执行
- 超出许可拒绝
- 完成释放许可

**Step 3: 运行测试**

Run: `cargo test -p dubbo-rs-filter`
Expected: tests pass

**Step 4: Commit**

```bash
git add dubbo-filter/
git commit -m "feat(filter): implement ExecuteLimitFilter with semaphore-based concurrency limit"
```

---

### Task 12: 构建集成示例

**Files:**
- Create: `examples/filter-chain/Cargo.toml`
- Create: `examples/filter-chain/src/main.rs`
- Modify: `Cargo.toml` (添加 workspace member)

**Step 1: 创建 filter-chain 示例**

演示过滤器链的构建和调用：

```rust
use dubbo_rs_filter::{FilterChain, EchoFilter, TokenFilter, AccessLogFilter};
use dubbo_rs_protocol::{InvocationContext, Invoker, RPCResult};
use dubbo_rs_common::url::URL;
use dubbo_rs_common::node::Node;

struct HelloInvoker { url: URL }

impl Node for HelloInvoker {
    fn get_url(&self) -> &URL { &self.url }
    fn is_available(&self) -> bool { true }
    fn destroy(&self) {}
}

#[async_trait::async_trait]
impl Invoker for HelloInvoker {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        println!("  -> HelloInvoker processing: {}", ctx.method_name);
        Ok(RPCResult::success(format!("Hello, {}!", ctx.method_name).as_bytes().to_vec()))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let filters: Vec<Box<dyn Filter>> = vec![
        Box::new(EchoFilter),
        Box::new(TokenFilter::new("secret123")),
        Box::new(AccessLogFilter),
    ];

    let invoker = Box::new(HelloInvoker {
        url: URL::new("tri", "/com.example.Greeter"),
    });
    let chain = FilterChain::new(filters, invoker, URL::new("tri", "/com.example.Greeter"));
    let chain_invoker = chain.build();

    // Test 1: Normal call with correct token
    println!("=== Test 1: Normal call ===");
    let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/com.example.Greeter"));
    ctx.attachments.insert("token".to_string(), "secret123".to_string());
    let result = chain_invoker.invoke(&mut ctx).await?;
    println!("Result: {:?}\n", String::from_utf8_lossy(result.value.as_ref().unwrap()));

    // Test 2: Echo health check (bypasses token)
    println!("=== Test 2: Echo ===");
    let mut ctx = InvocationContext::new("$echo", URL::new("tri", "/com.example.Greeter"));
    ctx.arguments = vec![b"ping".to_vec()];
    let result = chain_invoker.invoke(&mut ctx).await?;
    println!("Result: {:?}\n", String::from_utf8_lossy(result.value.as_ref().unwrap()));

    // Test 3: Missing token (should fail)
    println!("=== Test 3: Missing token ===");
    let mut ctx = InvocationContext::new("sayHi", URL::new("tri", "/com.example.Greeter"));
    match chain_invoker.invoke(&mut ctx).await {
        Ok(r) => println!("Unexpected success: {:?}", r),
        Err(e) => println!("Expected error: {}\n", e),
    }

    Ok(())
}
```

**Step 2: 注册 workspace member**

**Step 3: 运行示例**

Run: `cargo run -p filter-chain`
Expected: 3 个测试场景输出正确

**Step 4: Commit**

```bash
git add examples/filter-chain/ Cargo.toml
git commit -m "feat(examples): add filter-chain demonstration"
```

---

### Task 13: 运行完整测试套件和 lint

**Step 1: 运行所有测试**

Run: `cargo test --workspace`
Expected: all tests pass (预计 120+ 个)

**Step 2: 运行 clippy (pedantic)**

Run: `cargo clippy --workspace -- -D warnings`
Expected: 零警告

**Step 3: 运行 fmt 检查**

Run: `cargo fmt --all -- --check`
Expected: 无格式问题

**Step 4: Commit** (如有修复)

```bash
git add -A
git commit -m "chore: fix clippy warnings and formatting after Phase 3"
```

---

### Task 14: 更新 DEVELOPMENT_PLAN.md

**Files:**
- Modify: `DEVELOPMENT_PLAN.md`

**Step 1: 标记 Phase 3 所有任务为完成**

更新 §6 Phase 3 部分的所有 `[ ]` → `[x]`。

**Step 2: Commit**

```bash
git add DEVELOPMENT_PLAN.md
git commit -m "docs: mark Phase 3 filter and governance tasks as complete"
```

---

## 验收标准

Phase 3 完成标志：
- [ ] `Filter` trait 定义完整（invoke + on_response 默认实现）
- [ ] `FilterChain` 实现责任链模式，通过 Invoker trait 暴露
- [ ] `EchoFilter` 拦截 `$echo` 方法返回 echo 响应
- [ ] `TokenFilter` 验证 attachments 中的 token
- [ ] `AccessLogFilter` 使用 tracing 记录调用日志
- [ ] `TPSLimiter` 定义 + `DefaultTPSLimiter` 令牌桶实现
- [ ] `TPSLimitFilter` 集成 TPSLimiter 进行限流
- [ ] `ConditionRouter` 解析条件路由规则并过滤 invoker
- [ ] `TagRouter` 基于标签染色路由，支持 fallback
- [ ] `RouterChain` 组合多个 Router 顺序过滤
- [ ] `GracefulShutdownFilter` 检测关闭信号拒绝新请求
- [ ] `ActiveLimitFilter` 限制并发活跃调用数
- [ ] `ExecuteLimitFilter` 基于 Semaphore 限制并发执行
- [ ] `examples/filter-chain` 演示过滤器链完整流程
- [ ] 所有测试通过 (120+ tests, clippy 零警告)
- [ ] `DEVELOPMENT_PLAN.md` Phase 3 标记完成
