# dubbo-rs

**Apache Dubbo 的 Rust 语言实现** — 与 dubbo-java 协议兼容的微服务框架。

> MSRV: 1.87 | 协议: Triple (Dubbo3), Dubbo TCP (Dubbo2)

---

## 目录结构

### Crates (`dubbo-*`)

| 目录                            | 优先级 | 功能描述                                                                                                                                                                                                                    | 完成进度                           |
|---------------------------------|--------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------------------------------|
| `dubbo-common/`                 | P0     | **核心基础** — URL、Node trait、常量、SPI 扩展机制、错误类型定义                                                                                                                                                            | ✅ 完成                             |
| `dubbo-macros/`                 | P0     | **proc-macro 代码生成** — `#[dubbo_rs::service]` / `#[dubbo_rs::client]` 属性宏，自动生成服务注册和客户端代理（5 tests）                                                                                                    | ✅ 完成                             |
| `dubbo-protocol/`               | P0     | **协议抽象** — Protocol / Invoker / Exporter / InvocationContext / RPCResult trait 定义                                                                                                                                     | ✅ 完成                             |
| `dubbo-serialization/`          | P0     | **序列化抽象** — Serialization trait + SPI 注册机制                                                                                                                                                                         | ✅ 完成                             |
| `dubbo-serialization-protobuf/` | P0     | **Protobuf 序列化** — 基于 prost，Triple 协议默认序列化（tonic 处理实际编解码）                                                                                                                                             | ✅ 完成                             |
| `dubbo-remoting/`               | P0     | **网络传输** — ExchangeClient / ExchangeServer trait、Codec、Request/Response、ConnectionPool                                                                                                                               | ✅ 完成                             |
| `dubbo-protocol-triple/`        | P0     | **Triple 协议** — HTTP/2 gRPC 兼容协议，基于 tonic，支持 Unary 调用                                                                                                                                                         | ✅ 完成 (跨语言验证通过)            |
| `dubbo-registry/`               | P0     | **注册中心抽象** — Registry trait、NotifyListener、ServiceEvent                                                                                                                                                             | ✅ 完成                             |
| `dubbo-registry-zookeeper/`     | P0     | **ZooKeeper 注册中心** — 支持接口级 (Dubbo2) 和应用级 (Dubbo3) 服务发现                                                                                                                                                     | ✅ 完成                             |
| `dubbo-registry-nacos/`         | P0     | **Nacos 注册中心** — 服务注册/注销/发现/监听，支持认证和命名空间                                                                                                                                                            | ✅ 完成                             |
| `dubbo-cluster/`                | P0     | **集群容错** — FailoverCluster (失败重试)、FailfastCluster (快速失败)、Directory、StaticDirectory、RegistryDirectory                                                                                                        | ✅ 完成                             |
| `dubbo-loadbalance/`            | P0     | **负载均衡** — Random (加权随机)、RoundRobin (加权轮询)、LeastActive (最少活跃)、ConsistentHash (一致性哈希)                                                                                                                | ✅ 完成                             |
| `dubbo-filter/`                 | P0     | **过滤器链** — Filter trait、FilterChain、EchoFilter (健康检查)、TokenFilter (令牌验证)、AccessLogFilter (访问日志)、TPSLimiter (限流)、GracefulShutdownFilter (优雅关闭)、CircuitBreaker (熔断)、GenericService (泛化调用) | ✅ 完成                             |
| `dubbo-proxy/`                  | P0     | **代理工厂** — ProxyFactory trait、DefaultProxyFactory（基于 cluster 的默认代理）                                                                                                                                           | ✅ 完成                             |
| `dubbo-client/`                 | P0     | **客户端高级 API** — Client Builder、connect/dial、URL 管理                                                                                                                                                                 | ✅ 完成                             |
| `dubbo-server/`                 | P0     | **服务端高级 API** — Server Builder、service 注册、serve 启动                                                                                                                                                               | ✅ 完成                             |
| `dubbo/`                        | P0     | **顶层 API** — Instance 统一入口（RootConfig + Server + Client）、start 启动                                                                                                                                                | ✅ 完成                             |
| `dubbo-serialization-hessian2/` | P1     | **Hessian2 序列化** — 完整 Hessian2 编解码器（基本类型/容器/POJO/异常），39 往返测试 + 54 跨语言验证测试                                                                                                                    | ✅ 完成 (跨语言验证通过)            |
| `dubbo-serialization-json/`     | P1     | **JSON 序列化** — 基于 serde_json，支持 JSON 验证和标准化（5 tests）                                                                                                                                                        | ✅ 完成                             |
| `dubbo-protocol-dubbo/`         | P1     | **Dubbo TCP 协议** — 16 字节协议头编解码、Hessian2 body 序列化、心跳、DubboClient/DubboServer、完整 invoke 链路                                                                                                             | ✅ 完成 (跨语言验证通过)            |
| `dubbo-config/`                 | P1     | **配置管理** — RootConfig / ProtocolConfig / RegistryConfig，YAML 加载 + Builder Pattern                                                                                                                                    | ✅ 完成                             |
| `dubbo-logger/`                 | P1     | **日志抽象** — 基于 tracing-subscriber，支持日志级别、格式 (json/pretty/compact)、文件输出（13 tests）                                                                                                                      | ✅ 完成                             |
| `dubbo-metrics/`                | P1     | **Prometheus 指标** — 请求计数/Counter、延迟/Histogram、错误率、MetricsCollector + 导出器（20 tests）                                                                                                                       | ✅ 完成                             |
| `dubbo-tracing/`                | P1     | **OpenTelemetry 链路追踪** — W3C Trace Context 传播、可配置采样率、OTLP exporter（14 tests）                                                                                                                                | ✅ 完成                             |
| `dubbo-registry-etcd/`          | P1     | **Etcd 注册中心** — 基于 etcd v3 HTTP API，支持 lease 管理和 base64 编码（7 tests）                                                                                                                                         | ✅ 完成                             |
| `dubbo-configcenter/`           | P2     | **配置中心抽象** — ConfigCenter trait、ConfigChangeEvent、DynamicConfiguration（14 tests）                                                                                                                                  | ✅ 完成                             |
| `dubbo-configcenter-zookeeper/` | P2     | **ZK 配置中心** — ZK watcher 事件 → 监听器异步通知、Builder 模式（15 tests）                                                                                                                                                | ✅ 完成                             |
| `dubbo-protocol-grpc/`          | P2     | **原生 gRPC 协议** — 标准 gRPC 协议支持（9 tests）                                                                                                                                                                          | ✅ 完成                             |
| `dubbo-protocol-jsonrpc/`       | P2     | **JSON-RPC 协议** — JSON-RPC 2.0 over HTTP（21 tests）                                                                                                                                                                      | ✅ 完成                             |
| `dubbo-protocol-rest/`          | P2     | **REST 协议** — RESTful HTTP + GET/POST 方法映射（15 tests）                                                                                                                                                                | ✅ 完成                             |
| `dubbo-codegen/`                | P2     | **protoc 代码生成插件** — proto 解析、tonic-prost-build 集成、服务注册/Channel Client/Invoker Client 代码生成，支持全部 4 种 RPC 类型（30+ tests）                                                                          | ✅ 完成                             |
| `dubbo-tls/`                    | P2     | **TLS/mTLS 支持** — ServerTlsConfig/ClientTlsConfig，PEM 证书加载 + rustls（16 tests）                                                                                                                                      | ✅ 完成                             |
| `dubbo-metadata/`               | P2     | **元数据中心** — MetadataInfo、ServiceDefinition、MetadataStorage trait + InMemoryStorage（23 tests）                                                                                                                       | ✅ 完成                             |

### 其他目录

| 目录                | 类型       | 说明                                                                                          |
|---------------------|------------|-----------------------------------------------------------------------------------------------|
| `examples/`         | 示例       | 9 个示例程序，详见 [`examples/README.md`](./examples/README.md)                               |
| `scripts/`          | 工具脚本   | **QA 全量检查脚本** [`scripts/qa.sh`](./scripts/qa.sh) — 编译检查 / clippy / 测试 / 示例验证  |
| `docs/`             | 文档       | 开发计划、完善计划、审计报告                                                                  |
| `cross-lang-tests/` | 跨语言测试 | 跨语言兼容性验证套件 — Hessian2/Dubbo TCP/Triple(含Streaming)/注册中心/治理 (**1062+ tests**) | ✅ 全部通过 |
| `tests/`            | 集成测试   | 跨语言兼容性测试脚本 + Java fixture 项目 + Docker Compose                                     | ✅ 完成     |

---

## 核心架构

```
┌─────────────────────────────────────────────────────────┐
│                    dubbo (顶层 API)                      │
│              Instance, SetProviderService               │
├────────────────────┬────────────────────────────────────┤
│   dubbo-client     │         dubbo-server               │
│   Client, Dial     │    Server, Register, Serve         │
├────────────────────┴────────────────────────────────────┤
│                    dubbo-proxy                           │
│              ProxyFactory, ServiceProxy                 │
├─────────────────────────────────────────────────────────┤
│                    dubbo-cluster                         │
│          Directory, Cluster, LoadBalance, Router        │
├──────────┬──────────┬──────────┬────────────────────────┤
│  filter  │ registry │ protocol │     serialization      │
│  Filter  │ Registry │ Protocol │  Hessian2, Protobuf    │
│  Chain   │ SvcDisc  │ Invoker  │  JSON                  │
├──────────┴──────────┴──────────┴────────────────────────┤
│                    dubbo-remoting                         │
│            ExchangeClient, ExchangeServer               │
│            Request/Response, Codec                      │
├─────────────────────────────────────────────────────────┤
│                    dubbo-common                           │
│               URL, Node, Constants, SPI                 │
└─────────────────────────────────────────────────────────┘
```

---

## 协议兼容性

| 协议                 | dubbo-rs → dubbo-java   | dubbo-java → dubbo-rs   |
|----------------------|-------------------------|-------------------------|
| Triple + Protobuf    | ✅ 兼容                  | ✅ 兼容                  |
| Dubbo TCP + Hessian2 | ✅ 兼容 (跨语言验证通过) | ✅ 兼容 (跨语言验证通过) |
| gRPC Native          | ✅ 兼容                  | ✅ 兼容                  |
| JSON-RPC 2.0         | ✅ 标准协议              | ✅ 标准协议              |
| RESTful HTTP         | ✅ 标准协议              | ✅ 标准协议              |

---

## QA 检查

项目提供了全量 QA 脚本 `scripts/qa.sh`，覆盖编译检查、静态分析、测试和示例验证：

```bash
# 全量 QA（编译检查 + clippy 3 特性集 + fmt + 构建 + 测试 3 变体 + 示例运行）
./scripts/qa.sh

# 快速模式（跳过 clippy 和 no-default-features 测试）
./scripts/qa.sh --fast

# 指定阶段
./scripts/qa.sh --phase check    # 编译检查 + clippy + 构建
./scripts/qa.sh --phase test     # 测试（default / no-default / 特性变体）
./scripts/qa.sh --phase examples # 构建并运行所有示例
./scripts/qa.sh --phase fmt      # 格式检查（默认不运行，需显式指定）
```

> **注意**：`dubbo` crate 的 `configcenter-zookeeper` 和 `configcenter-nacos` 特性互斥，QA 脚本会自动分三次运行 clippy（default / zk / nacos）以覆盖全部特性组合。

---

## 技术栈

| 类别          | 技术                                                                 |
|---------------|----------------------------------------------------------------------|
| 异步运行时    | `tokio`                                                              |
| HTTP/2 + gRPC | `tonic` + `hyper`                                                    |
| Protobuf      | `prost` + `prost-build`                                              |
| 序列化        | `serde` (JSON/YAML), Hessian2 (自建)                                 |
| 注册中心      | ZooKeeper (`rust-zookeeper`), Nacos (自建 HTTP API), Etcd (HTTP API) |
| 可观测性      | `tracing`, `prometheus`, `opentelemetry`                             |
| 安全          | `rustls` (TLS/mTLS)                                                  |

---

## 许可证

Apache-2.0
