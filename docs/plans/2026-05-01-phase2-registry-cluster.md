# Phase 2: 注册中心与集群实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 实现 ZooKeeper/Nacos 服务注册发现、集群容错（Failover/Failfast）和四种负载均衡策略（Random/RoundRobin/LeastActive/ConsistentHash）。

**Architecture:** 基于 Phase 1 的 Invoker/Protocol/URL 基础设施，实现 Registry trait 抽象层，ZK/Nacos 具体注册中心，Cluster+Directory 集群抽象，以及 LoadBalance 策略插件。

**Tech Stack:** Rust 1.87 (edition 2024), tokio, async-trait, zookeeper 0.8, dashmap, serde_json, reqwest

**Current State:** Phase 0/1 完成 (54 tests)。所有 Phase 2 crate 均为骨架状态，需要从零实现。

---

## 差距总览

| # | 差距项 | 影响 crate | 优先级 |
|---|--------|-----------|--------|
| 1 | `Registry` trait 未定义 | dubbo-rs-registry | P0 |
| 2 | `NotifyListener` / `ServiceEvent` 未定义 | dubbo-rs-registry | P0 |
| 3 | `RegistryDirectory` 未实现 | dubbo-rs-registry | P0 |
| 4 | ZK 注册中心未实现 | dubbo-rs-registry-zookeeper | P0 |
| 5 | Nacos 注册中心未实现 | dubbo-rs-registry-nacos | P0 |
| 6 | `Cluster` / `Directory` trait 未定义 | dubbo-rs-cluster | P0 |
| 7 | `FailoverCluster` 未实现 | dubbo-rs-cluster | P0 |
| 8 | `FailfastCluster` 未实现 | dubbo-rs-cluster | P0 |
| 9 | `LoadBalance` trait 未定义 | dubbo-rs-loadbalance | P0 |
| 10 | 四种负载均衡策略未实现 | dubbo-rs-loadbalance | P0 |
| 11 | 直连模式 (DirectConnector) 未实现 | dubbo-rs-cluster | P0 |
| 12 | CI 集成测试无 ZK/Nacos 服务 | .github/workflows/ | P1 |

---

### Task 1: 实现 dubbo-rs-registry 核心 trait

**Files:**
- Modify: `dubbo-registry/src/lib.rs`
- Modify: `dubbo-registry/Cargo.toml`

**Step 1: 定义 ServiceEvent 和 NotifyListener**

```rust
// dubbo-registry/src/lib.rs

use async_trait::async_trait;
use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use std::collections::HashMap;
use std::sync::Arc;

/// Service discovery event type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceEvent {
    /// Provider nodes added
    Add(Vec<URL>),
    /// Provider nodes removed
    Remove(Vec<URL>),
    /// Provider nodes updated
    Update(Vec<URL>),
}

/// Listener for service discovery changes
#[async_trait]
pub trait NotifyListener: Send + Sync {
    /// Called when service providers change
    async fn notify(&self, event: ServiceEvent);

    /// Returns the service URL this listener is subscribed to
    fn get_url(&self) -> URL;
}

/// Registry trait — service registration and discovery
#[async_trait]
pub trait Registry: Node {
    /// Register a service URL to the registry
    async fn register(&self, url: URL) -> Result<(), RPCError>;

    /// Unregister a service URL from the registry
    async fn unregister(&self, url: URL) -> Result<(), RPCError>;

    /// Subscribe to service changes
    async fn subscribe(&self, url: URL, listener: Arc<dyn NotifyListener>) -> Result<(), RPCError>;

    /// Unsubscribe from service changes
    async fn unsubscribe(&self, url: URL, listener: Arc<dyn NotifyListener>) -> Result<(), RPCError>;
}
```

**Step 2: 编写测试**

针对 ServiceEvent 和 Registry trait 的 smoke test 编写 stub 实现测试 trait 是否可编译和实例化。

**Step 3: 运行测试**

Run: `cargo test -p dubbo-rs-registry`
Expected: 测试通过

**Step 4: Commit**

```bash
git add dubbo-registry/
git commit -m "feat(registry): define Registry, NotifyListener, ServiceEvent traits"
```

---

### Task 2: 实现 RegistryDirectory — 注册中心到 Invoker 的桥接

**Files:**
- Modify: `dubbo-registry/src/lib.rs` (添加 RegistryDirectory)

**Step 1: 实现 RegistryDirectory**

RegistryDirectory 持有订阅的 Invoker 列表，响应 NotifyListener 通知来更新列表。

```rust
use dubbo_rs_protocol::Invoker;
use dashmap::DashMap;
use std::sync::RwLock;

pub struct RegistryDirectory {
    url: URL,
    invokers: RwLock<Vec<Box<dyn Invoker>>>,
    /// Map of subscriber URL → listener for notification routing
    listeners: DashMap<String, Vec<Arc<dyn NotifyListener>>>,
}

impl RegistryDirectory {
    pub fn new(url: URL) -> Self { ... }
    
    /// Get all available invokers for the given invocation
    pub fn list(&self) -> Vec<&dyn Invoker> { ... }
    
    /// Refresh invoker list from registry notification
    pub fn refresh_invokers(&self, urls: &[URL]) { ... }
    
    /// Add a listener for this directory
    pub fn add_listener(&self, listener: Arc<dyn NotifyListener>) { ... }
}
```

**Step 2: 编写测试**

测试 RegistryDirectory 的 invoker 刷新和列表功能。

**Step 3: 运行测试**

Run: `cargo test -p dubbo-rs-registry`
Expected: 所有测试通过

**Step 4: Commit**

```bash
git add dubbo-registry/src/lib.rs
git commit -m "feat(registry): implement RegistryDirectory bridge between registry and invokers"
```

---

### Task 3: 实现 ZooKeeper 注册中心

**Files:**
- Modify: `dubbo-rs-registry-zookeeper/Cargo.toml`
- Modify: `dubbo-rs-registry-zookeeper/src/lib.rs`

**Step 1: 添加依赖**

```toml
[dependencies]
dubbo-rs-common = { workspace = true }
dubbo-rs-registry = { workspace = true }
zookeeper = { workspace = true }
async-trait = { workspace = true }
tokio = { workspace = true }
anyhow = { workspace = true }
dashmap = { workspace = true }
```

**Step 2: 实现 ZookeeperRegistry**

核心逻辑：
- `register()`: 在 `/dubbo/{service}/providers/{url}` 创建临时节点
- `unregister()`: 删除临时节点
- `subscribe()`: Watch 服务 providers 目录，变化时通知 listener
- Dubbo2 接口级路径: `/dubbo/{interface}/providers/{encoded_url}`
- Dubbo3 应用级路径: `/services/{app}/`

```rust
pub struct ZookeeperRegistry {
    url: URL,
    zk: ZooKeeper,
    /// Registered URLs for cleanup on destroy
    registered: RwLock<Vec<String>>,
    /// Active watchers — keyed by service path
    watchers: DashMap<String, tokio::sync::watch::Sender<()>>,
}
```

**Step 3: 实现路径生成**

```
Provider URL encoding for ZK:
dubbo://192.168.1.100:20880/com.example.DemoService?application=demo&version=1.0.0

ZK Path:
/dubbo/com.example.DemoService/providers/dubbo%3A%2F%2F192.168.1.100%3A20880%2F...
```

**Step 4: 编写测试**

使用 mock ZK 或跳过网络测试，主要测试 URL 编码/解码和路径生成逻辑。

**Step 5: 运行测试**

Run: `cargo test -p dubbo-rs-registry-zookeeper`
Expected: 测试通过

**Step 6: Commit**

```bash
git add dubbo-rs-registry-zookeeper/
git commit -m "feat(registry-zk): implement ZookeeperRegistry with register/subscribe/unregister"
```

---

### Task 4: 实现 Nacos 注册中心

**Files:**
- Modify: `dubbo-rs-registry-nacos/Cargo.toml`
- Modify: `dubbo-rs-registry-nacos/src/lib.rs`

**Step 1: 添加依赖**

```toml
[dependencies]
dubbo-rs-common = { workspace = true }
dubbo-rs-registry = { workspace = true }
async-trait = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
reqwest = { workspace = true }
anyhow = { workspace = true }
dashmap = { workspace = true }
```

**Step 2: 实现 Nacos 客户端 (基于 HTTP API)**

Nacos Open API:
- 服务注册: `POST /nacos/v1/ns/instance`
- 服务发现: `GET /nacos/v1/ns/instance/list`
- 心跳: `PUT /nacos/v1/ns/instance/beat`
- 订阅: `GET /nacos/v1/ns/instance/list` + 定时轮询

```rust
pub struct NacosRegistry {
    url: URL,
    server_addr: String,   // e.g. "http://127.0.0.1:8848"
    namespace: String,
    username: Option<String>,
    password: Option<String>,
    client: reqwest::Client,
    /// Active heartbeat tasks
    heartbeats: DashMap<String, tokio::task::JoinHandle<()>>,
    /// Service watchers  
    watchers: DashMap<String, Vec<Arc<dyn NotifyListener>>>,
}
```

**Step 3: 实现核心 API 方法**

- `register_instance()`: POST instance 信息
- `deregister_instance()`: DELETE instance
- `get_instances()`: GET instance list (服务发现)
- `send_heartbeat()`: PUT beat (保活)
- `start_heartbeat_loop()`: 定时心跳协程
- `subscribe()`: 定时轮询 + NotifyListener 通知
- 支持 namespace/group/accessKey/secretKey 参数

**Step 4: 编写测试**

测试 Nacos URL 参数解析、实例注册/注销请求格式、心跳间隔配置。

**Step 5: 运行测试**

Run: `cargo test -p dubbo-rs-registry-nacos`
Expected: 测试通过

**Step 6: Commit**

```bash
git add dubbo-rs-registry-nacos/
git commit -m "feat(registry-nacos): implement NacosRegistry with HTTP API integration"
```

---

### Task 5: 实现 dubbo-rs-cluster — Cluster/Directory trait

**Files:**
- Modify: `dubbo-cluster/src/lib.rs`
- Modify: `dubbo-cluster/Cargo.toml`

**Step 1: 定义 Cluster 和 Directory trait**

```rust
use async_trait::async_trait;
use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::url::URL;
use dubbo_rs_protocol::{InvocationContext, Invoker, RPCResult};

/// Directory provides a list of invokers for a service
#[async_trait]
pub trait Directory: Send + Sync {
    /// List available invokers for the given invocation
    async fn list(&self, ctx: &InvocationContext) -> Result<Vec<Box<dyn Invoker>>, RPCError>;
}

/// Cluster strategy — joins a directory into a single invoker
#[async_trait]
pub trait Cluster: Send + Sync {
    /// Join a directory into a fault-tolerant invoker
    async fn join(&self, directory: Box<dyn Directory>) -> Result<Box<dyn Invoker>, RPCError>;
}
```

**Step 2: 实现 StaticDirectory**

StaticDirectory 持有固定的 invoker 列表（用于直连模式）。

```rust
pub struct StaticDirectory {
    url: URL,
    invokers: Vec<Box<dyn Invoker>>,
}
```

**Step 3: 编写测试**

测试 StaticDirectory 的 list 方法和 trait 编译验证。

**Step 4: 运行测试**

Run: `cargo test -p dubbo-rs-cluster`
Expected: 测试通过

**Step 5: Commit**

```bash
git add dubbo-cluster/
git commit -m "feat(cluster): define Cluster and Directory traits with StaticDirectory"
```

---

### Task 6: 实现 FailoverCluster 和 FailfastCluster

**Files:**
- Modify: `dubbo-cluster/src/lib.rs`

**Step 1: 实现 FailoverCluster**

失败自动切换：调用失败后重试下一个 invoker（默认2次重试）。

```rust
pub struct FailoverCluster {
    retries: u32,  // default: 2
}

#[async_trait]
impl Cluster for FailoverCluster {
    async fn join(&self, directory: Box<dyn Directory>) -> Result<Box<dyn Invoker>, RPCError> {
        Ok(Box::new(FailoverClusterInvoker {
            directory,
            retries: self.retries,
        }))
    }
}
```

FailoverClusterInvoker 的 invoke() 逻辑：
1. 从 directory.list() 获取 invoker 列表
2. 按顺序调用（可选 LoadBalance 选择），失败后重试下一个
3. 所有重试耗尽后返回错误

**Step 2: 实现 FailfastCluster**

快速失败：只调用一次，失败即返回错误。

```rust
pub struct FailfastCluster;

#[async_trait]
impl Cluster for FailfastCluster {
    async fn join(&self, directory: Box<dyn Directory>) -> Result<Box<dyn Invoker>, RPCError> {
        Ok(Box::new(FailfastClusterInvoker { directory }))
    }
}
```

**Step 3: 编写测试**

使用 mock Invoker 测试 failover 的重试逻辑和 failfast 的快速失败行为。

**Step 4: 运行测试**

Run: `cargo test -p dubbo-rs-cluster`
Expected: 测试通过

**Step 5: Commit**

```bash
git add dubbo-cluster/src/lib.rs
git commit -m "feat(cluster): implement FailoverCluster and FailfastCluster strategies"
```

---

### Task 7: 实现 dubbo-rs-loadbalance — LoadBalance trait

**Files:**
- Modify: `dubbo-loadbalance/src/lib.rs`
- Modify: `dubbo-loadbalance/Cargo.toml`

**Step 1: 定义 LoadBalance trait**

```rust
use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::url::URL;
use dubbo_rs_protocol::{InvocationContext, Invoker};

pub trait LoadBalance: Send + Sync {
    /// Select an invoker from the list
    fn select(
        &self,
        invokers: &[Box<dyn Invoker>],
        url: &URL,
        invocation: &InvocationContext,
    ) -> Result<Box<dyn Invoker>, RPCError>;
}
```

**Step 2: 实现 RandomLoadBalance (加权随机)**

```rust
pub struct RandomLoadBalance;

impl LoadBalance for RandomLoadBalance {
    fn select(&self, invokers: &[Box<dyn Invoker>], ...) -> Result<...> {
        // 按权重加权随机选择
        // 如果所有权重相同，则纯随机
    }
}
```

算法：
- 获取每个 invoker 的 weight 参数
- 计算总权重
- 生成 [0, total_weight) 随机数
- 根据累积权重定位 invoker

**Step 3: 实现 RoundRobinLoadBalance (加权轮询)**

使用原子计数器 + DashMap 跟踪每个服务每台机器的当前轮询位置。

**Step 4: 实现 LeastActiveLoadBalance**

选择当前活跃调用数最少的 invoker（需要维护活跃调用计数器）。

**Step 5: 实现 ConsistentHashLoadBalance**

基于方法参数哈希的一致性哈希环，用于有状态服务。

**Step 6: 编写测试**

测试：
- Random: 统计分布均匀性
- RoundRobin: 轮询顺序和权重正确性
- LeastActive: 活跃度影响选择
- ConsistentHash: 节点增删时的缓存命中率

**Step 7: 运行测试**

Run: `cargo test -p dubbo-rs-loadbalance`
Expected: 测试通过

**Step 8: Commit**

```bash
git add dubbo-loadbalance/
git commit -m "feat(loadbalance): implement Random, RoundRobin, LeastActive, ConsistentHash strategies"
```

---

### Task 8: 构建示例和集成测试

**Files:**
- Create: `examples/zk-discovery/`
- Create: `examples/failover/`

**Step 1: zk-discovery 示例**

创建独立的 ZK 服务发现demo，包含 provider 注册和 consumer 发现。

**Step 2: failover 示例**

模拟故障注入的 failover demo。

**Step 3: 运行示例**

Run: `cargo run -p zk-discovery` (需要 ZK)
Expected: 服务注册和发现成功

**Step 4: Commit**

```bash
git add examples/
git commit -m "feat(examples): add ZK discovery and failover demonstration projects"
```

---

### Task 9: 运行完整测试套件

**Step 1: 运行所有测试**

Run: `cargo test --workspace`
Expected: 所有测试通过（预计 70+ 个）

**Step 2: 运行 clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: 零警告

**Step 3: 运行 fmt 检查**

Run: `cargo fmt --all -- --check`
Expected: 无格式问题

**Step 4: Commit** (如有修复)

```bash
git add -A
git commit -m "test: verify full test suite passes after Phase 2 implementation"
```

---

### Task 10: 更新 DEVELOPMENT_PLAN.md

**Files:**
- Modify: `DEVELOPMENT_PLAN.md`

**Step 1: 标记 Phase 2 任务为完成**

更新检查项。

**Step 2: Commit**

```bash
git add DEVELOPMENT_PLAN.md
git commit -m "docs: mark Phase 2 registry and cluster tasks as complete"
```

---

## 验收标准

Phase 2 完成标志：
- [ ] `Registry` trait 定义完整（register/unregister/subscribe/unsubscribe）
- [ ] `ServiceEvent` 和 `NotifyListener` 定义完整
- [ ] `RegistryDirectory` 实现注册中心到 Invoker 的桥接
- [ ] `ZookeeperRegistry` 实现接口级和应用级服务注册/发现
- [ ] `NacosRegistry` 实现 HTTP API 集成、心跳、认证
- [ ] `Cluster` 和 `Directory` trait 定义
- [ ] `FailoverCluster` 实现自动重试
- [ ] `FailfastCluster` 实现快速失败
- [ ] `StaticDirectory` 实现直连模式
- [ ] `LoadBalance` trait 定义
- [ ] `RandomLoadBalance` 加权随机
- [ ] `RoundRobinLoadBalance` 加权轮询
- [ ] `LeastActiveLoadBalance` 最少活跃
- [ ] `ConsistentHashLoadBalance` 一致性哈希
- [ ] 集成测试和示例
- [ ] 所有测试通过 (70+ tests, clippy 零警告)
- [ ] `DEVELOPMENT_PLAN.md` 更新为完成状态
