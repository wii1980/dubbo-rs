# 集成计划：将所有孤立 crate 接入 dubbo-rs-client / dubbo-rs-server / dubbo

## 目标

将以下 12 个完全孤立的 crate 及另外 5 个仅被 examples 使用的 crate 集成到主链路中，使 `dubbo-rs-client`、`dubbo-rs-server`、`dubbo` facade 真正具备 Dubbo 的核心能力。

## 优先级

| 优先级 | 任务                                                         | 涉及 crate                                           | 预估工时 |
|--------|--------------------------------------------------------------|------------------------------------------------------|----------|
| **P0** | dubbo-rs-macros 集成到 dubbo facade                             | `dubbo`, `dubbo-rs-macros`                              | 0.5h     |
| **P0** | FilterChain 集成到 dubbo-rs-client                              | `dubbo-rs-client`, `dubbo-rs-filter`                       | 1h       |
| **P0** | FilterChain 集成到 dubbo-rs-server                              | `dubbo-rs-server`, `dubbo-rs-filter`                       | 1h       |
| **P0** | Cluster+LoadBalance 集成到 dubbo-rs-client                      | `dubbo-rs-client`, `dubbo-rs-cluster`, `dubbo-rs-loadbalance` | 2h       |
| **P1** | Registry 自动注册到 dubbo-rs-server                             | `dubbo-rs-server`, `dubbo-rs-registry`, `dubbo-rs-registry-*` | 1.5h     |
| **P1** | Registry 订阅到 dubbo-rs-client                                 | `dubbo-rs-client`, `dubbo-rs-registry`, `dubbo-rs-registry-*` | 1.5h     |
| **P1** | MetricsFilter 创建并集成                                     | `dubbo-rs-metrics`, `dubbo-rs-filter`                      | 1h       |
| **P1** | Logger 初始化集成到 dubbo facade                             | `dubbo`, `dubbo-rs-logger`                              | 0.5h     |
| **P2** | TLS 集成到 dubbo-rs-server                                      | `dubbo-rs-server`, `dubbo-rs-tls`                          | 1h       |
| **P2** | TLS 集成到 dubbo-rs-client                                      | `dubbo-rs-client`, `dubbo-rs-tls`                          | 0.5h     |
| **P2** | GracefulShutdown 集成到 dubbo-rs-server                         | `dubbo-rs-server`, `dubbo-rs-filter`                       | 1h       |
| **P2** | dubbo-rs-config RootConfig 扩展 logger/metrics/tracing/tls 字段 | `dubbo-rs-config`                                       | 1h       |

---

## 任务明细

### Task 1: dubbo-rs-macros → dubbo facade

**修改文件**: `dubbo/Cargo.toml`, `dubbo/src/lib.rs`

**改动**:
- `dubbo/Cargo.toml` 添加 `dubbo-rs-macros = { workspace = true }` 依赖
- `dubbo/src/lib.rs` 添加 `pub use dubbo_rs_macros as macros;` 以及 `pub use dubbo_rs_macros::{service, client};`
- 这样用户可以通过 `dubbo_rs::service` 和 `dubbo_rs::client` 直接使用

**验证**: `cargo build -p dubbo` 通过

---

### Task 2: dubbo-rs-client + FilterChain

**修改文件**: `dubbo-client/Cargo.toml`, `dubbo-client/src/lib.rs`

**设计**:

```rust
pub struct Client {
    protocol_config: Option<ProtocolConfig>,
    url: Option<String>,
    channel: Option<Channel>,
    filters: Vec<Box<dyn Filter>>,      // 新增
}
```

**API 改动**:
- `Client::new()` — 初始化 `filters: vec![]`
- `Client::with_filter(filter: Box<dyn Filter>) -> Self` — 新增方法
- `Client::with_filters(filters: Vec<Box<dyn Filter>>) -> Self` — 新增方法
- `dial()` — 创建 channel 后，生成一个 ProtocolInvoker，然后用 FilterChain 包装

**关键实现**:

```rust
// 新增：ProtocolInvoker — 包装 tonic channel 为 Invoker
struct ProtocolInvoker {
    channel: Channel,
    url: URL,
}

// dial() 末尾增加：
let invoker = Box::new(ProtocolInvoker { channel, url });
let invoker = FilterChain::new(self.filters, invoker).build();
```

**依赖改动**: `dubbo-client/Cargo.toml` 添加 `dubbo-rs-filter = { workspace = true }`

**验证**: `cargo build -p dubbo-rs-client` 通过

---

### Task 3: dubbo-rs-server + FilterChain

**修改文件**: `dubbo-server/Cargo.toml`, `dubbo-server/src/lib.rs`

**设计**:

```rust
pub struct Server {
    application: String,
    version: String,
    protocol_config: Option<ProtocolConfig>,
    service_fn: Option<Box<dyn FnOnce(...)>>,
    filters: Vec<Box<dyn Filter>>,      // 新增
}
```

**API 改动**:
- `Server::new()` — 初始化 `filters: vec![]`
- `Server::with_filter(filter: Box<dyn Filter>) -> Self` — 新增
- `Server::with_filters(filters: Vec<Box<dyn Filter>>) -> Self` — 新增
- 当前 `serve()` 是裸启 tonic server，无法直接插入 filter 层
- **方案**: 新增 `serve_with_layer()` 或通过 tonic 的 `Layer` 机制实现 interceptor

但由于 tonic 的拦截器机制与 dubbo 的 Filter trait 不直接兼容，前期简化方案：
- 保留当前 tonic server 直接启动方式
- 将 FilterChain 信息存储在 Server 上，为后续通过 tonic interceptor 或 ServiceBuilder 接入做准备
- 提供 `filter_invoke()` 方法供应用层手动使用

**依赖改动**: `dubbo-server/Cargo.toml` 添加 `dubbo-rs-filter = { workspace = true }`

**验证**: `cargo build -p dubbo-rs-server` 通过

---

### Task 4: dubbo-rs-client + Cluster + LoadBalance

**修改文件**: `dubbo-client/Cargo.toml`, `dubbo-client/src/lib.rs`

**设计**: 新增 `ClusterClient` 包装器，将 Cluster 能力附加到 Client 上

```rust
// Client 新增字段
pub struct Client {
    // ... 现有字段
    cluster: Option<Box<dyn Cluster>>,
    loadbalance: Option<Box<dyn LoadBalance>>,
    registry: Option<Box<dyn Registry>>,
    directory: Option<Box<dyn Directory>>,
}
```

**API 改动**:
- `Client::with_cluster(cluster: Box<dyn Cluster>) -> Self`
- `Client::with_loadbalance(lb: Box<dyn LoadBalance>) -> Self`
- `Client::with_registry(registry: Box<dyn Registry>) -> Self`
- `dial()` 增加逻辑：
  1. 如果有 registry → subscribe 服务，创建 RegistryDirectory
  2. 如果只有 URL → 创建 StaticDirectory（当前行为）
  3. 如果有 cluster → 用 cluster.join(directory)
  4. 返回 cluster_invoker 给调用方

**需要新增类型**:

```rust
// TonicInvoker — 将 tonic Channel 包装为 dubbo Invoker
pub struct TonicInvoker {
    channel: Channel,
    url: URL,
}
```

**注意**: dubbo-rs-loadbalance 的 LoadBalance trait 当前在 dubbo-rs-loadbalance crate 中，但在 dubbo-rs-cluster 中未直接使用。需要在 dubbo-rs-client 中集成。

dubbo-rs-loadbalance crate 的 LoadBalance trait:
```rust
pub trait LoadBalance: Send + Sync {
    fn select(&self, invokers: &[Arc<dyn Invoker>], url: &URL, invocation: &InvocationContext) -> Option<usize>;
}
```

**依赖改动**: `dubbo-client/Cargo.toml` 添加 `dubbo-rs-cluster`, `dubbo-rs-loadbalance`, `dubbo-rs-registry`

**验证**: `cargo build -p dubbo-rs-client` 通过

---

### Task 5: dubbo-rs-server + Registry 自动注册

**修改文件**: `dubbo-server/Cargo.toml`, `dubbo-server/src/lib.rs`

**设计**:

```rust
pub struct Server {
    // ... 现有字段
    registry: Option<Box<dyn Registry>>,
    service_url: Option<URL>,
}
```

**API 改动**:
- `Server::with_registry(registry: Box<dyn Registry>) -> Self`
- `Server::with_service_url(url: URL) -> Self`
- `serve()` 增加逻辑：
  1. 如果有 registry + service_url → 注册到 registry
  2. 启动 tonic server
  3. 监听 shutdown → unregister

**依赖改动**: `dubbo-server/Cargo.toml` 添加 `dubbo-rs-registry`

**验证**: `cargo build -p dubbo-rs-server` 通过

---

### Task 6: dubbo-rs-client + Registry 订阅

**修改文件**: `dubbo-client/src/lib.rs`（Cargo.toml 已在 Task 4 中添加）

**设计**: 在 `dial()` 中使用 RegistryDirectory 替代 StaticDirectory

```rust
pub async fn dial(&mut self) -> Result<()> {
    // 1. 解析 URL 获取 host:port
    let url_str = self.url.as_ref().ok_or(...)?;
    let service_url = parse_url_to_url_obj(url_str)?;
    
    // 2. 如果有 registry → 使用 RegistryDirectory
    if let Some(registry) = &self.registry {
        let dir = RegistryDirectory::new(service_url.clone());
        // 设置 invoker 工厂 — 根据 URL 创建 TonicInvoker
        dir = dir.with_invoker_factory(|url| {
            let addr = format!("http://{}:{}", url.ip, url.port);
            // 这里需要异步，但 factory 是同步的 — 简化：预先连接
            // 实际应该异步连接
            Err(RPCError::ServerError("not yet implemented".into()))
        });
        registry.subscribe(service_url, Arc::new(dir)).await?;
        // ... cluster.join(dir)
    }
}
```

注意：这里有一个 async 问题 — `with_invoker_factory` 是同步闭包，但建连是异步的。需要重构 `RegistryDirectory` 或将连接推迟到首次 invoke。

**简化方案**: 在 `dial()` 中预先解析 registry 得到的 URL，为每个 URL 建连后放入 RegistryDirectory。

**验证**: `cargo build -p dubbo-rs-client` 通过

---

### Task 7: MetricsFilter 创建

**修改文件**: `dubbo-metrics/src/lib.rs` 新增 `MetricsFilter`

**设计**:

```rust
pub struct MetricsFilter {
    collector: MetricsCollector,
}

impl MetricsFilter {
    pub fn new(collector: MetricsCollector) -> Self { ... }
}

#[async_trait]
impl Filter for MetricsFilter {
    async fn invoke(&self, ctx: &mut InvocationContext, next: &dyn Invoker) -> Result<RPCResult> {
        let start = Instant::now();
        let result = next.invoke(ctx).await;
        let duration = start.elapsed().as_secs_f64();
        
        match &result {
            Ok(r) => self.collector.record_invocation(ctx, r, duration),
            Err(e) => {
                self.collector.record_request(ctx.url.path.as_str(), &ctx.method_name, "error");
                self.collector.record_duration(ctx.url.path.as_str(), &ctx.method_name, duration);
            }
        }
        
        result
    }
}
```

**依赖改动**: dubbo-rs-metrics 当前不依赖 dubbo-rs-filter，需要添加 `dubbo-rs-filter` 依赖（或 `dubbo-rs-common` + `dubbo-rs-protocol` — 已存在）

**验证**: `cargo build -p dubbo-rs-metrics` 通过

---

### Task 8: Logger 初始化集成到 dubbo facade

**修改文件**: `dubbo/Cargo.toml`, `dubbo/src/lib.rs`

**设计**:

- `dubbo/Cargo.toml` 添加 `dubbo-rs-logger = { workspace = true }`
- `dubbo/src/lib.rs` 添加 `pub use dubbo_rs_logger as logger;`
- `Instance::new()` 或新增 `Instance::init_logger()` 方法自动初始化 logger

**验证**: `cargo build -p dubbo` 通过

---

### Task 9: dubbo-rs-server + TLS

**修改文件**: `dubbo-server/Cargo.toml`, `dubbo-server/src/lib.rs`

**设计**:

```rust
pub struct Server {
    // ... 现有字段
    tls_config: Option<ServerTlsConfig>,
}
```

**API 改动**:
- `Server::with_tls(config: ServerTlsConfig) -> Self`
- `serve()` 中如果 tls_config 有值，构建 `tonic::transport::Server::builder().tls_config(...)`

**依赖改动**: `dubbo-server/Cargo.toml` 添加 `dubbo-rs-tls`

**注意**: tonic 的 `Server::tls_config()` 接受 `TlsServerConfig`（来自 `tonic::transport::server::TlsServerConfig`），而不是 `rustls::ServerConfig`。需要适配。

**验证**: `cargo build -p dubbo-rs-server` 通过

---

### Task 10: dubbo-rs-client + TLS

**修改文件**: `dubbo-client/src/lib.rs`

**设计**:

```rust
pub struct Client {
    // ... 现有字段
    tls_config: Option<ClientTlsConfig>,
}
```

**API 改动**:
- `Client::with_tls(config: ClientTlsConfig) -> Self`
- `dial()` 中使用 `Endpoint::from_shared(addr)?.tls_config(...)?.connect()`

**依赖改动**: `dubbo-client/Cargo.toml` 添加 `dubbo-rs-tls`

**验证**: `cargo build -p dubbo-rs-client` 通过

---

### Task 11: dubbo-rs-server + GracefulShutdown

**修改文件**: `dubbo-server/src/lib.rs`

**设计**:

```rust
pub struct Server {
    // ... 现有字段
    graceful_shutdown: bool,
}
```

**API 改动**:
- `Server::with_graceful_shutdown() -> Self`
- `serve()` 中注册信号处理器（SIGTERM/SIGINT）→ 触发 shutdown

**验证**: `cargo build -p dubbo-rs-server` 通过

---

### Task 12: dubbo-rs-config RootConfig 扩展

**修改文件**: `dubbo-config/src/lib.rs`, `dubbo-config/Cargo.toml`

**设计**:

```rust
pub struct RootConfig {
    pub application: String,
    pub version: String,
    pub protocols: Vec<ProtocolConfig>,
    pub registries: Vec<RegistryConfig>,
    pub tls: Option<TlsConfig>,
    // 新增字段
    pub logger: Option<LoggerConfig>,   // serde(flatten) 或嵌套
}
```

**注意**: 这需要 `dubbo-rs-config` 依赖 `dubbo-rs-logger`。为避免循环依赖，可以用 `serde_yaml::Value` 保存通用配置，让上层解析。

**简化方案**: 暂不修改 RootConfig，而是让 `Instance` 提供 `configure_logger()` 等方法。

**验证**: `cargo build -p dubbo-rs-config` 通过

---

## 执行顺序

```
Phase 1（P0）:
  Task 1 (dubbo-rs-macros re-export) → 30min
  Task 2 (client + FilterChain)  → 1h
  Task 3 (server + FilterChain)  → 1h
  Task 4 (client + Cluster)      → 2h

Phase 2（P1）:
  Task 5 (server + Registry)     → 1.5h
  Task 6 (client + Registry)     → 1.5h
  Task 7 (MetricsFilter)         → 1h
  Task 8 (Logger facade)         → 0.5h

Phase 3（P2）:
  Task 9 (server + TLS)          → 1h
  Task 10 (client + TLS)         → 0.5h
  Task 11 (server + shutdown)    → 1h
  Task 12 (config fields)        → 1h
```

总预估工时: ~12h

---

## 验证方式

每个 Task 完成后:
1. `cargo build` 编译通过
2. 对应 crate 的测试通过: `cargo test -p <crate>`
3. 不影响现有示例: `cargo build --examples`
4. LSP diagnostics 零错误

全部完成后:
1. `cargo build --workspace` 全量编译
2. `cargo test --workspace` 全量测试
3. 检查 `phone-dialing` 示例能否正常运行
