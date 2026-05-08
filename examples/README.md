# dubbo-rs 示例

本目录包含 9 个示例程序，展示 dubbo-rs 的核心功能。按照依赖复杂度排列：前面的示例无需外部基础设施即可运行，后面的示例需要外部服务（ZooKeeper、Nacos）。

---

## 示例总览

| 示例                                  | 外部依赖        | 演示内容                                                                       |
|---------------------------------------|-----------------|--------------------------------------------------------------------------------|
| [`hello-world`](#hello-world)         | 无              | Triple 协议端到端 gRPC 通信                                                    |
| [`instance-demo`](#instance-demo)     | 无              | **dubbo_rs::Instance** 统一入口 — Config + Server + Client + Graceful Shutdown |
| [`filter-chain`](#filter-chain)       | 无              | 过滤器链（Echo / Token / AccessLog）                                           |
| [`failover`](#failover)               | 无              | 集群容错（Failover v.s. Failfast）                                             |
| [`canary-release`](#canary-release)   | 无              | 路由规则（条件路由 / 标签路由 / 路由链 / 灰度发布）                            |
| [`dubbo2-interop`](#dubbo2-interop)   | 无              | Dubbo TCP 协议（Dubbo2）端到端通信                                             |
| [`zk-discovery`](#zk-discovery)       | ZooKeeper       | ZK 服务注册发现 + gRPC 调用                                                    |
| [`nacos-discovery`](#nacos-discovery) | Nacos           | Nacos 服务注册发现 + gRPC 调用                                                 |
| [`phone-dialing`](#phone-dialing)     | 无 (Nacos 可选) | 全功能展示：Config / Filter / LoadBalance / Cluster / Streaming / Nacos        |

---

## hello-world

Triple 协议端到端 gRPC 通信示例 — dubbo-rs 的 "Hello World"。

**功能**：在同一进程中启动 gRPC 服务端和客户端，客户端调用服务端 `say_hello` 方法并输出响应。

**服务定义**：Protobuf (`.proto`) 文件定义 `Greeter` 服务，通过 `tonic-build` 生成 Rust 代码。

### 调用方式

```bash
cargo run -p hello-world
```

**无外部依赖，开箱即用**。

**输出示例**：
```
=== dubbo-rs Hello World (Phase 1) ===

Starting gRPC server on [::1]:50051
Connecting to gRPC server...
Response: Hello, dubbo-rs! (from dubbo-rs)

=== Hello World completed successfully! ===
```

---

## instance-demo

dubbo_rs::Instance 统一入口示例 — 展示 `dubbo_rs::Instance` 高层 API 的完整使用流程。

**功能**：在同一进程中展示 dubbo-rs 框架的 6 个核心步骤：

1. **配置管理** — `RootConfig` Builder 模式（application / version / protocol）
2. **服务端构建** — `Server` Builder + `register_service` 注册 gRPC 服务
3. **客户端构建** — `Client` Builder + URL 配置
4. **Instance 统一入口** — 组装 Config / Server / Client / GracefulShutdownFilter，调用 `start()` 启动
5. **RPC 调用** — 通过 tonic channel 发起 3 次 gRPC 请求并输出响应
6. **优雅停机** — 触发 GracefulShutdownFilter + `instance.shutdown()` 等待完成

### 调用方式

```bash
cargo run -p instance-demo
```

**无外部依赖，开箱即用**。

**输出示例**：
```
=== dubbo-rs Instance API Demo ===

[Config] Application: instance-demo
[Config] Protocol: tri://127.0.0.1:50051

[Instance] Server started in background

[Server] Received request from: dubbo-rs
[Client] Response: Hello, dubbo-rs! (from dubbo-rs Instance API)
[Server] Received request from: Instance API
[Client] Response: Hello, Instance API! (from dubbo-rs Instance API)
[Server] Received request from: World
[Client] Response: Hello, World! (from dubbo-rs Instance API)

[Instance] Triggering graceful shutdown...
[Instance] Shutdown flag set: is_shutdown=true
[Instance] Shutdown complete.

=== Instance API Demo completed! ===
```

---

## filter-chain

过滤器链演示 — 展示 `EchoFilter`（健康检查）、`TokenFilter`（令牌验证）和 `AccessLogFilter`（访问日志）的组合使用。

**功能**：
- Demo 1：携带正确 token 的正常调用
- Demo 2：`$echo` 健康检查调用
- Demo 3：缺失 token → 返回错误
- Demo 4：错误 token → 返回错误

### 调用方式

```bash
cargo run -p filter-chain
```

**无外部依赖，开箱即用**。

---

## failover

集群容错策略对比 — 演示 `FailoverCluster`（失败自动重试）和 `FailfastCluster`（快速失败）的不同行为。

**功能**：
- Demo 1：FailoverCluster — 所有 Invoker 健康（正常返回）
- Demo 2：FailoverCluster — 所有 Invoker 失败（自动重试全部节点，retries=2，共 3 次尝试）
- Demo 3：FailfastCluster — 不重试，失败即返回错误
- Demo 4：两种策略的行为对比说明

### 调用方式

```bash
cargo run -p failover
```

**无外部依赖，开箱即用**。

---

## canary-release

路由规则与灰度发布演示 — 展示 `ConditionRouter`（条件路由）、`TagRouter`（标签路由）和 `RouterChain`（组合路由链）。

**功能**：
- Demo 1：ConditionRouter — 路由到 `env=gray` 的灰度实例
- Demo 2：ConditionRouter — 基于客户端 region 的条件触发路由
- Demo 3：ConditionRouter — 多条件匹配（AND 逻辑）
- Demo 4：TagRouter — 基于 `dubbo.tag` 的流量染色，tag 不匹配时回退到无标签节点
- Demo 5：RouterChain — ConditionRouter → TagRouter 链式过滤
- Demo 6：灰度发布场景 — 同时展示灰度池和稳定池的实例分布

### 调用方式

```bash
cargo run -p canary-release
```

**无外部依赖，开箱即用**。

---

## dubbo2-interop

Dubbo TCP 协议（Dubbo2）端到端通信示例 — 演示 Dubbo TCP 协议的请求/响应完整链路。

**功能**：
- 服务端：启动 Dubbo TCP Server，监听指定端口
- 客户端：连接服务端，发送 3 次 Hessian2 编码的 RPC 请求，接收响应

### 调用方式

```bash
# 同时启动 server + client（默认模式）
cargo run -p dubbo2-interop

# 仅启动 server
cargo run -p dubbo2-interop -- server

# 仅启动 client
cargo run -p dubbo2-interop -- client
```

**环境变量**：

| 变量   | 说明           | 默认值  |
|--------|----------------|---------|
| `PORT` | 服务端监听端口 | `20880` |

**无外部依赖，开箱即用**。

---

## zk-discovery

ZooKeeper 服务注册发现示例 — 演示服务端注册到 ZK、客户端通过 ZK 发现服务并发起 gRPC 调用的完整流程。

**功能**：
- 服务端：连接 ZK → 注册服务 URL → 启动 gRPC 服务
- 客户端：连接 ZK → 订阅服务变更 → 发现 Provider → 发起 3 次 gRPC 调用

### 调用方式

```bash
# 同时启动 server + client（默认模式）
cargo run -p zk-discovery

# 仅启动 server
cargo run -p zk-discovery -- server

# 仅启动 client
cargo run -p zk-discovery -- client
```

**前置条件**：需要本地运行 ZooKeeper（默认 `127.0.0.1:2181`）。

**环境变量**：

| 变量          | 说明            | 默认值           |
|---------------|-----------------|------------------|
| `ZK_ADDR`     | ZK 地址         | `127.0.0.1:2181` |
| `SERVER_PORT` | gRPC 服务端端口 | `50051`          |

---

## nacos-discovery

Nacos 服务注册发现示例 — 演示服务端注册到 Nacos、客户端通过 Nacos 发现服务并发起 gRPC 调用的完整流程。

**功能**：
- 服务端：连接 Nacos → 注册服务 URL → 启动 gRPC 服务
- 客户端：连接 Nacos → 订阅服务变更 → 发现 Provider → 发起 3 次 gRPC 调用

### 调用方式

```bash
# 同时启动 server + client（默认模式）
cargo run -p nacos-discovery

# 仅启动 server
cargo run -p nacos-discovery -- server

# 仅启动 client
cargo run -p nacos-discovery -- client
```

**前置条件**：需要本地运行 Nacos（默认 `127.0.0.1:8848`）。

**环境变量**：

| 变量              | 说明             | 默认值            |
|-------------------|------------------|-------------------|
| `NACOS_ADDR`      | Nacos 地址       | `127.0.0.1:8848`  |
| `NACOS_NAMESPACE` | Nacos 命名空间   | `public` (不使用) |
| `NACOS_GROUP`     | Nacos 分组       | `DEFAULT_GROUP`   |
| `NACOS_USERNAME`  | Nacos 认证用户名 | (可选)            |
| `NACOS_PASSWORD`  | Nacos 认证密码   | (可选)            |
| `SERVER_PORT`     | gRPC 服务端端口  | `50051`           |

---

## phone-dialing

dubbo-rs 全功能展示示例 — 通过电话拨号场景演示框架的 6 大核心能力。

**Phase 1 — 配置管理**（`dubbo-rs-config`）
- 从 YAML 字符串加载 `RootConfig`（`serde_yaml`）
- Builder 模式构建配置（`with_application` / `with_protocol` / `with_registry`）
- 展示默认值（`RootConfig::default()`）

**Phase 2 — 过滤器链**（`dubbo-rs-filter`）
- 组合 `EchoFilter`（`$echo` 健康检查，绕过鉴权）+ `TokenFilter`（令牌验证）+ `AccessLogFilter`（访问日志）
- 4 个演示：正确 token / `$echo` 健康检查 / 缺失 token / 错误 token

**Phase 3 — 负载均衡**（`dubbo-rs-loadbalance`）
- `RandomLoadBalance` — 加权随机选择（100 / 200 / 300 权重分布）
- `RoundRobinLoadBalance` — 加权轮询序列
- `LeastActiveLoadBalance` — 最少活跃调用优先
- `ConsistentHashLoadBalance` — 一致性哈希（相同输入 → 相同节点）

**Phase 4 — 集群容错**（`dubbo-rs-cluster`）
- `FailoverCluster`（retries=2）— 混合节点场景下自动重试直到成功
- `FailfastCluster` — 失败节点立即返回错误，不重试

**Phase 5 — Server Streaming RPC**（`dubbo-rs-server` + `dubbo-rs-client`）
- 使用 `dubbo_rs_server::Server` Builder 启动 gRPC 服务
- 使用 `dubbo_rs_client::Client` Builder 建立连接
- 4 种拨号场景的实时进度流：正常通话 / 快速应答 / 线路忙 / 无效号码

**Phase 6-7 — Nacos 服务注册发现**（`dubbo-rs-registry-nacos`，可选）
- 服务端注册到 Nacos + 启动 gRPC 服务
- 客户端通过 Nacos 发现服务 + 发起流式调用

### 调用方式

```bash
# 所有 demo（无外部依赖，开箱即用）
cargo run -p phone-dialing

# 仅 server + Nacos 注册（需要 Nacos）
cargo run -p phone-dialing -- server

# 仅 client + Nacos 发现（需要 Nacos）
cargo run -p phone-dialing -- client

# 所有 demo + Nacos 流式调用（需要 Nacos）
cargo run -p phone-dialing -- nacos
```

**无前置条件**（默认模式）。Nacos 模式需要本地运行 Nacos（默认 `127.0.0.1:8848`）。

**环境变量**：

| 变量              | 说明             | 默认值            |
|-------------------|------------------|-------------------|
| `SERVER_PORT`     | gRPC 服务端端口  | `50051`           |
| `NACOS_ADDR`      | Nacos 地址       | `127.0.0.1:8848`  |
| `NACOS_NAMESPACE` | Nacos 命名空间   | `public` (不使用) |
| `NACOS_GROUP`     | Nacos 分组       | `DEFAULT_GROUP`   |
| `NACOS_USERNAME`  | Nacos 认证用户名 | (可选)            |
| `NACOS_PASSWORD`  | Nacos 认证密码   | (可选)            |
