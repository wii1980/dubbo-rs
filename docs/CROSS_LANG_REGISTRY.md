# dubbo-rs ↔ dubbo-java 跨语言验证方案 — 注册发现与端到端

## 6 Domain D — 服务发现与注册中心互操作

### 6.1 方法论

**目标**: 验证 dubbo-rs 与 Java Dubbo 之间通过 ZooKeeper 和 Nacos 注册中心实现跨语言服务注册与发现。

**注册中心选型**:
- **ZooKeeper** (主选): Dubbo 生态中使用最广泛的注册中心，也是 dubbo-rs 首先需要兼容的目标。ZK 的临时节点（ephemeral znode）机制天然适合服务上下线感知。
- **Nacos** (备选): 阿里云生态首选注册中心，在 Dubbo 3.x 中地位提升。验证 dubbo-rs 对 Nacos 的支持覆盖了部分企业用户的实际需求。

**测试方法**: 按照严格的启动顺序控制时序: 启动注册中心 → 启动 Provider → 等待注册完成（3 秒）→ 启动 Consumer → 执行 RPC 调用 → 验证结果。每个步骤都有独立的健康检查点，确保测试的可重复性。

**注册模式覆盖**:
- **接口级注册** (Dubbo 2.x 风格): 以服务接口为粒度注册，路径格式为 `/dubbo/{interface}/providers/`。这是 dubbo-rs 首先需要支持的注册模式。
- **应用级注册** (Dubbo 3.x 风格): 以应用为粒度注册，路径格式为 `/services/{app-name}/`。Dubbo 3.x 推荐此模式，dubbo-rs 需要兼容。

### 6.2 ZooKeeper 测试用例（D-ZK 系列）

#### Test D-ZK-001: Java provider 注册，RS consumer 发现（接口级）
- **领域**: D (注册发现)
- **分类**: ZK
- **方向**: Java Provider → RS Consumer
- **前置条件**: ZooKeeper 运行于 `localhost:2181`，Java provider 配置 `org.apache.dubbo.DemoService` 接口，注册模式为 interface-level
- **操作步骤**:
  1. 启动 ZooKeeper（Docker/Podman 容器或本地实例）
  2. 启动 Java provider，连接 ZK，注册接口 `org.apache.dubbo.DemoService`
  3. 等待 3 秒，确保注册完成
  4. 启动 RS consumer，连接同一 ZK 实例，订阅 `org.apache.dubbo.DemoService`
  5. RS consumer 执行 `sayHello("world")` 调用
- **预期结果**: RS consumer 从 ZK 获取到 Java provider 的 URL，成功发起 RPC 调用并收到 `"Hello world"` 响应
- **通过标准**: ZK znode `/dubbo/org.apache.dubbo.DemoService/providers/` 下存在 provider URL，RS consumer 日志显示成功发现并调用

#### Test D-ZK-002: RS provider 注册，Java consumer 发现（接口级）
- **领域**: D (注册发现)
- **分类**: ZK
- **方向**: RS Provider → Java Consumer
- **前置条件**: ZooKeeper 运行于 `localhost:2181`，RS provider 配置 `org.apache.dubbo.DemoService` 接口，注册模式为 interface-level
- **操作步骤**:
  1. 启动 ZooKeeper
  2. 启动 RS provider，连接 ZK，注册接口 `org.apache.dubbo.DemoService`，监听端口 `20881`
  3. 等待 3 秒
  4. 启动 Java consumer，连接同一 ZK，订阅 `org.apache.dubbo.DemoService`
  5. Java consumer 执行 `sayHello("world")` 调用
- **预期结果**: Java consumer 从 ZK 获取到 RS provider 的 URL，成功调用并收到响应
- **通过标准**: ZK znode 下存在 RS provider 注册的 URL（格式需与 Java 兼容），Java consumer 成功调用且无反序列化错误

#### Test D-ZK-003: Java provider 注册，RS consumer 发现（应用级）
- **领域**: D (注册发现)
- **分类**: ZK
- **方向**: Java Provider → RS Consumer
- **前置条件**: ZooKeeper 运行于 `localhost:2181`，Java provider 以 application-level 模式注册，应用名 `demo-provider`
- **操作步骤**:
  1. 启动 ZooKeeper
  2. 启动 Java provider，配置 `dubbo.application.name=demo-provider`，注册模式为 application-level
  3. 等待 3 秒
  4. 启动 RS consumer，以 application-level 模式订阅 `demo-provider`
  5. RS consumer 执行 RPC 调用
- **预期结果**: RS consumer 通过应用级发现机制找到 provider 并成功调用
- **通过标准**: ZK 路径 `/services/demo-provider/` 下存在注册信息，RS consumer 成功解析应用级元数据并完成调用

#### Test D-ZK-004: RS provider 注册，Java consumer 发现（应用级）
- **领域**: D (注册发现)
- **分类**: ZK
- **方向**: RS Provider → Java Consumer
- **前置条件**: ZooKeeper 运行于 `localhost:2181`，RS provider 以 application-level 模式注册，应用名 `rs-demo-provider`
- **操作步骤**:
  1. 启动 ZooKeeper
  2. 启动 RS provider，配置应用名 `rs-demo-provider`，注册模式为 application-level，监听端口 `20881`
  3. 等待 3 秒
  4. 启动 Java consumer，以 application-level 模式订阅 `rs-demo-provider`
  5. Java consumer 执行 RPC 调用
- **预期结果**: Java consumer 通过应用级发现机制找到 RS provider 并成功调用
- **通过标准**: ZK 路径 `/services/rs-demo-provider/` 下存在 RS 注册的元数据，Java consumer 成功解析并完成调用

#### Test D-ZK-005: RS provider → RS consumer 基线（接口级）
- **领域**: D (注册发现)
- **分类**: ZK
- **方向**: RS Provider → RS Consumer
- **前置条件**: ZooKeeper 运行于 `localhost:2181`，RS provider 和 consumer 均为 dubbo-rs 实现
- **操作步骤**:
  1. 启动 ZooKeeper
  2. 启动 RS provider，接口级注册 `org.apache.dubbo.DemoService`
  3. 等待 3 秒
  4. 启动 RS consumer，订阅同一接口
  5. RS consumer 执行 `sayHello("zk-baseline")` 调用
- **预期结果**: 纯 RS 环境下注册发现全链路正常工作
- **通过标准**: 注册、发现、调用三个阶段均成功，作为跨语言测试的基线对照

### 6.3 Nacos 测试用例（D-NA 系列）

#### Test D-NA-001: Java provider 注册，RS consumer 发现
- **领域**: D (注册发现)
- **分类**: Nacos
- **方向**: Java Provider → RS Consumer
- **前置条件**: Nacos 运行于 `localhost:8848`（含 gRPC 端口 `9848`），Java provider 配置 Nacos 注册中心
- **操作步骤**:
  1. 启动 Nacos Server（单机模式，Docker/Podman 容器）
  2. 启动 Java provider，注册到 Nacos，服务名 `providers:org.apache.dubbo.DemoService:1.0.0`
  3. 等待 3 秒
  4. 启动 RS consumer，连接 Nacos，订阅同一服务
  5. RS consumer 执行 RPC 调用
- **预期结果**: RS consumer 从 Nacos 获取 Java provider 实例列表，成功调用
- **通过标准**: Nacos 控制台或 API 可查到 provider 注册信息，RS consumer 调用成功返回 `"Hello world"`

#### Test D-NA-002: RS provider 注册，Java consumer 发现
- **领域**: D (注册发现)
- **分类**: Nacos
- **方向**: RS Provider → Java Consumer
- **前置条件**: Nacos 运行于 `localhost:8848`，RS provider 配置 Nacos 注册中心，监听端口 `20881`
- **操作步骤**:
  1. 启动 Nacos Server
  2. 启动 RS provider，注册到 Nacos，服务名 `providers:org.apache.dubbo.DemoService:1.0.0`
  3. 等待 3 秒
  4. 启动 Java consumer，连接 Nacos，订阅同一服务
  5. Java consumer 执行 RPC 调用
- **预期结果**: Java consumer 从 Nacos 获取 RS provider 实例，成功调用
- **通过标准**: Nacos 中存在 RS 注册的实例（含 IP、端口、元数据），Java consumer 成功调用

#### Test D-NA-003: RS provider → RS consumer 基线
- **领域**: D (注册发现)
- **分类**: Nacos
- **方向**: RS Provider → RS Consumer
- **前置条件**: Nacos 运行于 `localhost:8848`，RS provider 和 consumer 均为 dubbo-rs 实现
- **操作步骤**:
  1. 启动 Nacos Server
  2. 启动 RS provider，注册到 Nacos
  3. 等待 3 秒
  4. 启动 RS consumer，订阅服务
  5. RS consumer 执行 `sayHello("nacos-baseline")` 调用
- **预期结果**: 纯 RS 环境下 Nacos 注册发现全链路正常
- **通过标准**: 注册、发现、调用均成功，作为 Nacos 跨语言测试的基线

### 6.4 注册格式验证（D-F 系列）

#### Test D-F-001: ZK URL 路径格式对比
- **领域**: D (注册发现)
- **分类**: Format
- **方向**: N/A（格式比对）
- **前置条件**: ZooKeeper 运行于 `localhost:2181`，分别准备 Java provider 和 RS provider 注册到同一 ZK
- **操作步骤**:
  1. 启动 ZooKeeper
  2. 启动 Java provider，注册接口 `org.apache.dubbo.DemoService`
  3. 等待 3 秒
  4. 读取 ZK znode `/dubbo/org.apache.dubbo.DemoService/providers/` 下 Java 注册的 URL
  5. 停止 Java provider
  6. 启动 RS provider，注册同一接口
  7. 读取同一 znode 下 RS 注册的 URL
  8. 对比两条 URL 的路径格式、编码方案
- **预期结果**: RS 与 Java 注册的 znode 路径格式完全一致: `/dubbo/{interface}/providers/{url_encoded_provider_url}`。URL 编码方案（百分号编码）一致，特殊字符转义规则相同
- **通过标准**: 路径前缀 `/dubbo/org.apache.dubbo.DemoService/providers/` 完全匹配。URL 中的 `dubbo://` scheme、host:port、接口名、参数键值对均可被 Java 和 RS 互相正确解析

#### Test D-F-002: 注册 URL 参数一致性
- **领域**: D (注册发现)
- **分类**: Format
- **方向**: N/A（参数比对）
- **前置条件**: ZK 中同时存在 Java provider 和 RS provider 的注册 URL
- **操作步骤**:
  1. 从 D-F-001 的 ZK 中提取 Java provider URL 和 RS provider URL
  2. 解析两段 URL 的 query parameters
  3. 逐一比对必需参数的存在性和格式
- **预期结果**: 两段 URL 均包含以下必需参数且格式一致:
  - `application`: 应用名（String）
  - `interface`: 接口全限定名（String，如 `org.apache.dubbo.DemoService`）
  - `methods`: 方法列表（逗号分隔，如 `sayHello`）
  - `side`: 固定为 `provider`
  - `timestamp`: 注册时间戳（毫秒级 Unix timestamp）
  - `dubbo`: Dubbo 协议版本号（如 `2.0.2`）
  - `serialization`: 序列化方式（如 `hessian2`）
- **通过标准**: 上述 7 个必需参数在 RS 和 Java 注册 URL 中均存在，参数值语义一致（timestamp 不要求相等，但格式必须为合法数字）。Java consumer 能够无警告地解析 RS provider 的全部参数

---

## 7 Domain E — 端到端服务治理

### 7.1 方法论

**目标**: 在真实的多节点拓扑下验证 dubbo-rs 的服务治理能力，涵盖负载均衡、故障转移、过滤器链、超时控制和异常传播。

**范围**: 本域仅覆盖基础场景。高级特性（如条件路由、标签路由、熔断降级）作为后续迭代的扩展目标。当前阶段专注于验证 dubbo-rs 在多 provider 环境下的核心治理行为是否与 Java Dubbo 一致。

**依赖关系**: Domain E 是最高级别的验证，依赖于 Level 1 到 Level 4（Domain A 到 Domain D）全部通过。任何一个底层域的问题都会导致 E 系列测试结果不可靠。

**测试拓扑**: 所有 E 系列测试需要至少 2 个 provider 实例和 1 个 consumer 实例，通过注册中心（ZK）进行服务发现。Provider 分布在不同端口，模拟多节点部署。

### 7.2 测试用例（E 系列）

#### Test E-001: 多 provider 轮询负载均衡
- **领域**: E (服务治理)
- **分类**: 负载均衡
- **方向**: RS Consumer → 2 × Java Provider
- **前置条件**: ZooKeeper 运行于 `localhost:2181`，两个 Java provider 实例分别监听 `20880` 和 `20881`，均注册 `org.apache.dubbo.DemoService`，RS consumer 配置 `roundrobin` 负载策略
- **操作步骤**:
  1. 启动 ZooKeeper
  2. 启动 Java provider-1（端口 `20880`），注册到 ZK
  3. 启动 Java provider-2（端口 `20881`），注册到 ZK
  4. 等待 5 秒，确保两个 provider 均已注册
  5. 启动 RS consumer，订阅服务，配置 `roundrobin` 策略
  6. 连续发起 10 次 `sayHello` 调用
  7. 统计每个 provider 收到的调用次数
- **预期结果**: 两个 provider 各收到约 5 次调用（允许 ±20% 偏差，即 4:6 或 6:4 均可接受）
- **通过标准**: 调用总数 = 10，两个 provider 的调用次数均在 4~6 次范围内。所有 10 次调用均成功返回，无错误

#### Test E-002: Provider 崩溃故障转移
- **领域**: E (服务治理)
- **分类**: 故障转移
- **方向**: RS Consumer → 2 × Java Provider（其中一个中途崩溃）
- **前置条件**: ZooKeeper 运行，两个 Java provider 已注册（端口 `20880`、`20881`），RS consumer 配置 `failover` 集群策略， retries = 1
- **操作步骤**:
  1. 启动 ZooKeeper，启动两个 Java provider，等待注册完成
  2. 启动 RS consumer，验证可正常调用
  3. 发起第 1 次调用，记录被选中的 provider（假设为 provider-1）
  4. 杀掉 provider-1 进程（`kill -9`）
  5. 等待 2 秒（允许 ZK session timeout 触发）
  6. 连续发起 5 次调用
  7. 检查调用结果和目标 provider
- **预期结果**: 杀掉 provider-1 后，RS consumer 自动故障转移到 provider-2。后续 5 次调用全部成功，无超时或连接错误
- **通过标准**: 故障转移完成后（步骤 6），全部 5 次调用均成功返回，响应来自 provider-2。允许前 1~2 次调用因 provider-1 下线尚未感知而重试

#### Test E-003: EchoFilter 过滤器链
- **领域**: E (服务治理)
- **分类**: 过滤器
- **方向**: RS Consumer（含 EchoFilter）→ Java Provider
- **前置条件**: ZooKeeper 运行，Java provider 正常注册并实现 `org.apache.dubbo.DemoService`，RS consumer 配置 `echo` 过滤器
- **操作步骤**:
  1. 启动 ZooKeeper 和 Java provider
  2. 启动 RS consumer，配置 Filter 链中包含 `EchoFilter`
  3. RS consumer 发送 `$echo("test-value")` 回声请求
  4. 记录响应内容
- **预期结果**: Echo 请求经过 RS consumer 的 EchoFilter 处理，直接在客户端返回传入的值 `"test-value"`，不触发远程调用。若 Dubbo 协议层要求 echo 请求到达服务端，则服务端原样返回 `"test-value"`
- **通过标准**: 响应值等于 `"test-value"`，无序列化异常。通过日志确认请求经过了 EchoFilter 处理

#### Test E-004: 客户端超时控制
- **领域**: E (服务治理)
- **分类**: 超时
- **方向**: RS Consumer → Java Provider（延迟响应）
- **前置条件**: ZooKeeper 运行，Java provider 的 `sayHello` 方法实现中包含 `Thread.sleep(5000)`（5 秒延迟），RS consumer 配置超时时间为 1000ms
- **操作步骤**:
  1. 启动 ZooKeeper 和 Java provider（含 5s 延迟的方法实现）
  2. 启动 RS consumer，配置 `timeout = 1000`（1 秒超时）
  3. RS consumer 发起 `sayHello("timeout-test")` 调用
  4. 记录调用耗时和返回结果
- **预期结果**: RS consumer 在 1 秒内收到超时错误，不会挂起等待 5 秒。返回的错误类型为 timeout 相关的异常
- **通过标准**: 调用在 1000ms ~ 1500ms 之间返回（允许少量误差），返回 `Err` 且错误信息包含 `"timeout"` 或 `"timed out"`。进程不挂起

#### Test E-005: 异常传播
- **领域**: E (服务治理)
- **分类**: 异常处理
- **方向**: RS Consumer → Java Provider（抛出异常）
- **前置条件**: ZooKeeper 运行，Java provider 的 `sayHello` 方法在收到参数 `"throw"` 时抛出 `new RuntimeException("intentional error for testing")`，RS consumer 正常连接
- **操作步骤**:
  1. 启动 ZooKeeper 和 Java provider
  2. 启动 RS consumer
  3. RS consumer 发起 `sayHello("throw")` 调用
  4. 检查返回的错误信息
- **预期结果**: Java provider 抛出 `RuntimeException`，dubbo-rs consumer 收到包含异常信息的错误响应。异常类型和 message 字段可被正确解析
- **通过标准**: RS consumer 的返回值为 `Err`，错误中包含 `"intentional error for testing"` 字符串（或可辨识的等价信息）。异常的 classname 字段包含 `"RuntimeException"`

---

## 相关文档

- [CROSS_LANG_OVERVIEW.md](./CROSS_LANG_OVERVIEW.md) — 跨语言验证总体概述
- [CROSS_LANG_HESSIAN2.md](./CROSS_LANG_HESSIAN2.md) — Hessian2 序列化级验证（Domain A）
- [CROSS_LANG_PROTOCOLS.md](./CROSS_LANG_PROTOCOLS.md) — 协议级验证（Domain B / C）
- [CROSS_LANG_VERIFICATION.md](./CROSS_LANG_VERIFICATION.md) — 完整跨语言验证方案（源文档）
- [DEVELOPMENT_PLAN.md](./DEVELOPMENT_PLAN.md) — 开发计划
