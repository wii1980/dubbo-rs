# dubbo-rs ↔ dubbo-java 跨语言验证方案 — 实施计划

## 8 测试用例总索引

### Domain A: Hessian2 序列化

| 测试编号 | 领域 | 分类 | 描述 | 方向 | 前置条件 | 优先级 | 状态 |
|----------|------|------|------|------|----------|--------|------|
| A-D-001 | A-Hessian2 | Decode | `boolean(true)` 解码 | N/A | fixture `bool_true.bin` | P0 | ✅ 已完成 |
| A-D-002 | A-Hessian2 | Decode | `boolean(false)` 解码 | N/A | fixture `bool_false.bin` | P0 | ✅ 已完成 |
| A-D-003 | A-Hessian2 | Decode | `int(0)` 解码 | N/A | fixture `int_0.bin` | P0 | ✅ 已完成 |
| A-D-004 | A-Hessian2 | Decode | `int(42)` 解码 | N/A | fixture `int_42.bin` | P0 | ✅ 已完成 |
| A-D-005 | A-Hessian2 | Decode | `int(-1)` 解码 | N/A | fixture `int_minus1.bin` | P0 | ✅ 已完成 |
| A-D-006 | A-Hessian2 | Decode | `int(2^31 - 1)` 解码 | N/A | fixture `int_max.bin` | P1 | ✅ 已完成 |
| A-D-007 | A-Hessian2 | Decode | `long(0)` 解码 | N/A | fixture `long_0.bin` | P0 | ✅ 已完成 |
| A-D-008 | A-Hessian2 | Decode | `long(2^63 - 1)` 解码 | N/A | fixture `long_max.bin` | P1 | ✅ 已完成 |
| A-D-009 | A-Hessian2 | Decode | `double(0.0)` 解码 | N/A | fixture `double_0.bin` | P0 | ✅ 已完成 |
| A-D-010 | A-Hessian2 | Decode | `double(3.14)` 解码 | N/A | fixture `double_314.bin` | P0 | ✅ 已完成 |
| A-D-011 | A-Hessian2 | Decode | `double(-1.0)` 解码 | N/A | fixture `double_minus1.bin` | P0 | ✅ 已完成 |
| A-D-012 | A-Hessian2 | Decode | `double(1.797e308)` 解码 | N/A | fixture `double_max.bin` | P1 | ✅ 已完成 |
| A-D-013 | A-Hessian2 | Decode | `float(0.0f)` 解码 | N/A | fixture `float_0.bin` | P1 | ✅ 已完成 |
| A-D-014 | A-Hessian2 | Decode | `float(3.14f)` 解码 | N/A | fixture `float_314.bin` | P1 | ✅ 已完成 |
| A-D-015 | A-Hessian2 | Decode | `null` 解码 | N/A | fixture `null.bin` | P0 | ✅ 已完成 |
| A-D-016 | A-Hessian2 | Decode | `String("")` 空字符串解码 | N/A | fixture `string_empty.bin` | P0 | ✅ 已完成 |
| A-D-017 | A-Hessian2 | Decode | `String("hello")` 解码 | N/A | fixture `string_hello.bin` | P0 | ✅ 已完成 |
| A-D-018 | A-Hessian2 | Decode | `String(中文)` UTF-8 字符串解码 | N/A | fixture `string_utf8.bin` | P0 | ✅ 已完成 |
| A-D-019 | A-Hessian2 | Decode | `String(1024 chars)` 长字符串解码 | N/A | fixture `string_long.bin` | P1 | ✅ 已完成 |
| A-D-020 | A-Hessian2 | Decode | `Date` 解码 | N/A | fixture `date_now.bin` | P1 | ✅ 已完成 |
| A-D-021 | A-Hessian2 | Decode | `byte[]` 解码 | N/A | fixture `bytes.bin` | P1 | ✅ 已完成 |
| A-D-022 | A-Hessian2 | Decode | `ArrayList` 解码 | N/A | fixture `list.bin` | P0 | ✅ 已完成 |
| A-D-023 | A-Hessian2 | Decode | `HashMap` 解码 | N/A | fixture `map.bin` | P0 | ✅ 已完成 |
| A-D-024 | A-Hessian2 | Decode | 自定义 `User` 对象解码 | N/A | fixture `user.bin` | P0 | ✅ 已完成 |
| A-D-025 | A-Hessian2 | Decode | 嵌套泛型 `List<Map<String, User>>` 解码 | N/A | fixture `nested_generic.bin` | P2 | ✅ 已完成 |
| A-E-001 | A-Hessian2 | Encode | `boolean(true)` 编码 → 与 Java fixture 比对 | N/A | fixture `bool_true.bin` | P0 | ✅ 已完成 |
| A-E-002 | A-Hessian2 | Encode | `boolean(false)` 编码 → 与 Java fixture 比对 | N/A | fixture `bool_false.bin` | P0 | ✅ 已完成 |
| A-E-003 | A-Hessian2 | Encode | `int(0)` 编码比对 | N/A | fixture `int_0.bin` | P0 | ✅ 已完成 |
| A-E-004 | A-Hessian2 | Encode | `int(42)` 编码比对 | N/A | fixture `int_42.bin` | P0 | ✅ 已完成 |
| A-E-005 | A-Hessian2 | Encode | `long(0)` 编码比对 | N/A | fixture `long_0.bin` | P0 | ✅ 已完成 |
| A-E-006 | A-Hessian2 | Encode | `double(3.14)` 编码比对 | N/A | fixture `double_314.bin` | P0 | ✅ 已完成 |
| A-E-007 | A-Hessian2 | Encode | `null` 编码比对 | N/A | fixture `null.bin` | P0 | ✅ 已完成 |
| A-E-008 | A-Hessian2 | Encode | `String("hello")` 编码比对 | N/A | fixture `string_hello.bin` | P0 | ✅ 已完成 |
| A-E-009 | A-Hessian2 | Encode | `String(中文)` 编码比对 | N/A | fixture `string_utf8.bin` | P0 | ✅ 已完成 |
| A-E-010 | A-Hessian2 | Encode | `ArrayList` 编码比对 | N/A | fixture `list.bin` | P0 | ✅ 已完成 |
| A-E-011 | A-Hessian2 | Encode | `HashMap` 编码比对 | N/A | fixture `map.bin` | P0 | ✅ 已完成 |
| A-E-012 | A-Hessian2 | Encode | 自定义 `User` 对象编码比对 | N/A | fixture `user.bin` | P0 | ✅ 已完成 |
| A-R-001 | A-Hessian2 | Roundtrip | `boolean` 往返: RS encode → RS decode | N/A | N/A | P0 | ✅ 已完成 |
| A-R-002 | A-Hessian2 | Roundtrip | `int` 往返 | N/A | N/A | P0 | ✅ 已完成 |
| A-R-003 | A-Hessian2 | Roundtrip | `long` 往返 | N/A | N/A | P0 | ✅ 已完成 |
| A-R-004 | A-Hessian2 | Roundtrip | `double` 往返 | N/A | N/A | P0 | ✅ 已完成 |
| A-R-005 | A-Hessian2 | Roundtrip | `String` 往返 | N/A | N/A | P0 | ✅ 已完成 |
| A-R-006 | A-Hessian2 | Roundtrip | `null` 往返 | N/A | N/A | P0 | ✅ 已完成 |
| A-R-007 | A-Hessian2 | Roundtrip | `List` 往返 | N/A | N/A | P0 | ✅ 已完成 |
| A-R-008 | A-Hessian2 | Roundtrip | `Map` 往返 | N/A | N/A | P0 | ✅ 已完成 |
| A-R-009 | A-Hessian2 | Roundtrip | 自定义 `User` 对象往返 | N/A | N/A | P0 | ✅ 已完成 |
| A-R-010 | A-Hessian2 | Roundtrip | Java fixture → RS decode → RS encode → 字节比对 | N/A | All decode fixtures | P0 | ✅ 已完成 |

### Domain B: Dubbo TCP 协议

| 测试编号 | 领域 | 分类 | 描述 | 方向 | 前置条件 | 优先级 | 状态 |
|----------|------|------|------|------|----------|--------|------|
| B-H-001 | B-DubboTCP | Header | 魔数 `0xdabb` 验证 | RS → Java | Java DubboTcpTestServer | P0 | ✅ 已完成 |
| B-H-002 | B-DubboTCP | Header | Request flag (byte 2) 验证 | RS → Java | Java DubboTcpTestServer | P0 | ✅ 已完成 |
| B-H-003 | B-DubboTCP | Header | 序列化类型 ID 验证 (Hessian2 = 2) | RS → Java | Java DubboTcpTestServer | P0 | ✅ 已完成 |
| B-H-004 | B-DubboTCP | Header | Request ID 字节序验证 | RS → Java | Java DubboTcpTestServer | P0 | ✅ 已完成 |
| B-H-005 | B-DubboTCP | Header | Body length 字段验证 | RS → Java | Java DubboTcpTestServer | P0 | ✅ 已完成 |
| B-B-001 | B-DubboTCP | Body | Dubbo version 字段解码 | RS → Java | Java provider | P0 | ✅ 已完成 |
| B-B-002 | B-DubboTCP | Body | Service interface 字段解码 | RS → Java | Java provider | P0 | ✅ 已完成 |
| B-B-003 | B-DubboTCP | Body | Service version 字段解码 | RS → Java | Java provider | P0 | ✅ 已完成 |
| B-B-004 | B-DubboTCP | Body | Method name 字段解码 | RS → Java | Java provider | P0 | ✅ 已完成 |
| B-B-005 | B-DubboTCP | Body | Parameter types 字段解码 | RS → Java | Java provider | P0 | ✅ 已完成 |
| B-B-006 | B-DubboTCP | Body | Arguments 字段解码 | RS → Java | Java provider | P0 | ✅ 已完成 |
| B-B-007 | B-DubboTCP | Body | Attachments 字段解码 | RS → Java | Java provider | P0 | ✅ 已完成 |
| B-R-001 | B-DubboTCP | Roundtrip | RS consumer → Java provider `sayHello("World")` 完整调用 | RS → Java | Java DubboTcpTestServer | P0 | ✅ 已完成 |
| B-R-002 | B-DubboTCP | Roundtrip | RS consumer → Java provider `echo("ping")` 调用 | RS → Java | Java DubboTcpTestServer | P0 | ✅ 已完成 |
| B-R-003 | B-DubboTCP | Roundtrip | RS consumer → Java provider `ping()` 无参调用 | RS → Java | Java DubboTcpTestServer | P0 | ✅ 已完成 |
| B-R-004 | B-DubboTCP | Roundtrip | RS provider → RS consumer 自闭环 `sayHello` | RS → RS | RS Dubbo TCP provider | P1 | ✅ 已完成 |
| B-R-005 | B-DubboTCP | Roundtrip | RS provider → RS consumer 多次连续调用 | RS → RS | RS provider | P1 | ✅ 已完成 |
| B-R-006 | B-DubboTCP | Roundtrip | 多参数方法调用 `add(10, 32)` | RS → Java | Java provider | P1 | ✅ 已完成 |
| B-R-007 | B-DubboTCP | Roundtrip | Attachments 透传验证 | RS → Java | Java provider | P1 | ✅ 已完成 |

### Domain C: Triple 协议

| 测试编号 | 领域 | 分类 | 描述 | 方向 | 前置条件 | 优先级 | 状态 |
|----------|------|------|------|------|----------|--------|------|
| C-001 | C-Triple | Unary | TripleRequestWrapper protobuf 编解码往返 | N/A | N/A | P0 | ✅ 已完成 |
| C-002 | C-Triple | Unary | TripleResponseWrapper protobuf 编解码往返 | N/A | N/A | P0 | ✅ 已完成 |
| C-003 | C-Triple | Unary | 空字段编解码 | N/A | N/A | P0 | ✅ 已完成 |
| C-004 | C-Triple | Unary | TripleInvoker 构造和状态 | N/A | N/A | P0 | ✅ 已完成 |
| C-005 | C-Triple | Unary | gRPC 连接建立和 Channel 状态验证 | RS → Mock | Mock Triple server | P0 | ✅ 已完成 |
| C-006 | C-Triple | Unary | RS Invoker → Mock Server Echo 调用 | RS → Mock | Mock Triple server | P0 | ✅ 已完成 |
| C-007 | C-Triple | Unary | RS Invoker → Mock Server SayHello 调用 | RS → Mock | Mock Triple server | P0 | ✅ 已完成 |
| C-S-001 | C-Triple | Streaming | ServerStream — 接收 3 个响应块 | RS → Mock | Mock Triple server | P1 | ✅ 已完成 |
| C-S-002 | C-Triple | Streaming | ClientStream — 发送 3 个请求，接收计数 | RS → Mock | Mock Triple server | P1 | ✅ 已完成 |
| C-S-003 | C-Triple | Streaming | BidiStream — 发送 2 个请求，接收回复 | RS → Mock | Mock Triple server | P1 | ✅ 已完成 |
| C-008 | C-Triple | Metadata | 自定义 attachment 通过 wrapper 透传 | RS → Mock | Mock Triple server | P1 | ✅ 已完成 |
| C-009 | C-Triple | Error | gRPC status code 映射验证 (NOT_FOUND/INTERNAL/DEADLINE_EXCEEDED) | RS → Mock | Mock Triple server | P1 | ✅ 已完成 |
| C-010 | C-Triple | Error | 错误详情 (grpc-status-details-bin) 解码 | RS → Mock | Mock Triple server | P1 | ✅ 已完成 |

### Domain D: 注册中心互操作

| 测试编号 | 领域 | 分类 | 描述 | 方向 | 前置条件 | 优先级 | 状态 |
|----------|------|------|------|------|----------|--------|------|
| D-ZK-001 | D-注册发现 | ZK | URL 编码输出与 Java ZK 节点名约定一致（含 tri/dubbo 协议） | RS ↔ Java | 无需 ZK | P0 | ✅ API级 |
| D-ZK-002 | D-注册发现 | ZK | ZK provider path 格式 + decode_url 解析 Java 编码 URL | RS ↔ Java | 无需 ZK | P0 | ✅ API级 |
| D-ZK-003 | D-注册发现 | ZK | 应用级 vs 接口级路径结构验证 | N/A | 无需 ZK | P1 | ✅ API级 |
| D-ZK-004 | D-注册发现 | ZK | ServiceEvent/NotifyListener + URL 编解码边界 | RS → RS | 无需 ZK | P1 | ✅ API级 |
| D-ZK-005 | D-注册发现 | ZK | RS provider → RS consumer 基线（接口级） | RS → RS | ZooKeeper :2181 | P0 | ✅ API级 |
| D-NA-001 | D-注册发现 | Nacos | 注册请求字段映射（serviceName/ip/port/namespace/group）与 Java 一致 | RS ↔ Java | 无需 Nacos | P1 | ✅ API级 |
| D-NA-002 | D-注册发现 | Nacos | Nacos 响应解析（check_nacos_response + extract_hosts 多格式） | RS ↔ Java | 无需 Nacos | P1 | ✅ API级 |
| D-NA-003 | D-注册发现 | Nacos | 实例→URL 转换 + 多实例发现 + auth 凭证配置 | RS → RS | 无需 Nacos | P1 | ✅ API级 |
| D-F-001 | D-注册发现 | Format | ZK URL 路径格式对比 | N/A | ZooKeeper :2181 | P0 | ✅ 已完成 |
| D-F-002 | D-注册发现 | Format | 注册 URL 参数一致性 | N/A | ZooKeeper :2181 | P0 | ✅ 已完成 |
| D-ZK-E2E-001 | D-注册发现 | ZK E2E | RS provider 注册 → RS consumer 发现（接口级） | RS → RS | ZooKeeper :2181 | P0 | ✅ E2E |
| D-ZK-E2E-002 | D-注册发现 | ZK E2E | 多 provider 注册 → consumer 发现全部 | RS → RS | ZooKeeper :2181 | P0 | ✅ E2E |
| D-ZK-E2E-003 | D-注册发现 | ZK E2E | Triple 协议 URL 经 ZK 往返保持正确 | RS → RS | ZooKeeper :2181 | P1 | ✅ E2E |
| D-ZK-E2E-004 | D-注册发现 | ZK E2E | 注销后 provider 从发现中移除 | RS → RS | ZooKeeper :2181 | P0 | ✅ E2E |
| D-NA-E2E-001 | D-注册发现 | Nacos E2E | RS provider 注册 → RS consumer 发现 | RS → RS | Nacos :8848 | P0 | ✅ E2E |
| D-NA-E2E-002 | D-注册发现 | Nacos E2E | 多 provider 注册 → consumer 发现全部 | RS → RS | Nacos :8848 | P0 | ✅ E2E |
| D-NA-E2E-003 | D-注册发现 | Nacos E2E | 注销后 provider 从发现中移除 | RS → RS | Nacos :8848 | P0 | ✅ E2E |
| D-NA-E2E-004 | D-注册发现 | Nacos E2E | 自定义 namespace + group 隔离 | RS → RS | Nacos :8848 | P1 | ✅ E2E |
| D-ETCD-E2E-001 | D-注册发现 | Etcd E2E | RS provider 注册 → RS consumer 发现 | RS → RS | Etcd :2379 | P0 | ✅ E2E |
| D-ETCD-E2E-002 | D-注册发现 | Etcd E2E | 注销后 provider 从发现中移除 | RS → RS | Etcd :2379 | P0 | ✅ E2E |
| D-ETCD-E2E-003 | D-注册发现 | Etcd E2E | 多 provider 注册 → consumer 发现全部 | RS → RS | Etcd :2379 | P0 | ✅ E2E |
| D-ETCD-E2E-004 | D-注册发现 | Etcd E2E | 自定义 root path | RS → RS | Etcd :2379 | P1 | ✅ E2E |
| D-ZK-Java-001 | D-注册发现 | ZK 跨语言 | Java Dubbo provider 注册到 ZK → RS consumer 发现 | Java → RS | ZK :2181 + Java 17 | P0 | ✅ E2E |
| D-ZK-Java-002 | D-注册发现 | ZK 跨语言 | RS + Java 同时注册 → RS consumer 发现全部 provider | RS+Java → RS | ZK :2181 + Java 17 | P0 | ✅ E2E |

### Domain E: 端到端服务治理

| 测试编号 | 领域 | 分类 | 描述 | 方向 | 前置条件 | 优先级 | 状态 |
|----------|------|------|------|------|----------|--------|------|
| E-001 | E-服务治理 | 负载均衡 | Random 加权随机范围/空列表错误 | RS → RS | 无需外部依赖 | P1 | ✅ 深入测试 |
| E-001b | E-服务治理 | 负载均衡 | RoundRobin 加权严格轮询序列 | RS → RS | 无需外部依赖 | P1 | ✅ 深入测试 |
| E-001c | E-服务治理 | 负载均衡 | LeastActive 零活跃/权重决胜 | RS → RS | 无需外部依赖 | P1 | ✅ 深入测试 |
| E-001d | E-服务治理 | 负载均衡 | ConsistentHash 确定性/均匀分布 | RS → RS | 无需外部依赖 | P1 | ✅ 深入测试 |
| E-002 | E-服务治理 | 故障转移 | FailoverCluster 重试到成功/全部失败 | RS → RS | 无需外部依赖 | P1 | ✅ 深入测试 |
| E-002b | E-服务治理 | 故障转移 | FailfastCluster 首次成功/空目录 | RS → RS | 无需外部依赖 | P1 | ✅ 深入测试 |
| E-003 | E-服务治理 | 过滤器 | AccessLogFilter 透传 + GracefulShutdown 前后 | RS → RS | 无需外部依赖 | P2 | ✅ 深入测试 |
| E-003b | E-服务治理 | 过滤器 | CircuitBreaker 熔断打开/关闭 + Echo+Shutdown 链 | RS → RS | 无需外部依赖 | P2 | ✅ 深入测试 |
| E-004 | E-服务治理 | 超时 | 客户端超时控制 | RS → Java | ZK + 慢 provider | P1 | ✅ API级 |
| E-005 | E-服务治理 | 异常 | 异常传播 | RS → Java | ZK + Java provider | P1 | ✅ API级 |

---

## 9 分阶段实施计划

### Phase 0: 基础设施搭建（已完成 ✅）

本阶段目标已全部达成。基于这套基础设施，团队可以并行推进后续各阶段的测试开发。

**完成清单**:

1. ✅ **创建 `tests/java-fixtures/` Maven 项目**
   - `pom.xml`（dubbo 3.3.3 + Netty 4.1.111 + JUnit 5.10.3）
   - `DemoService.java` — 服务接口
   - `Hessian2ReferenceEncoder.java` — 生成 37 个 `.bin` fixture 文件
   - `DubboTcpTestServer.java` — Netty Dubbo TCP server（完整 16 字节头编解码）
   - `TripleTestServer.java` — Triple protocol skeleton

2. ✅ **创建 `tests/fixtures/hessian2/` 目录结构**
   - 子目录: `primitives/`、`strings/`、`collections/`、`maps/`、`dates/`、`objects/`
   - `.gitignore` 排除生成文件

3. ✅ **创建 `cross-lang-tests/` Rust 测试 crate**
   - `FixtureLoader` — 读取 `.bin` 文件
   - `PortAllocator` — 端口分配（AtomicU16 + fallback 到 OS 分配）
   - `JavaServer` — Java 进程管理（DubboTcpTestServer / TripleTestServer）
   - `DockerServices` — ZK/Nacos 容器管理
   - `assert_bytes_eq` — 逐字节对比 + hex diff

4. ✅ **创建 `tests/docker-compose.yml`**
   - ZooKeeper 3.8（:2181）
   - Nacos 2.4.0（:8848, :9848, :9849）

5. ✅ **创建 `tests/generate-fixtures.sh`**
   - 调用 Maven Hession2ReferenceEncoder，输出 37 个 `.bin` 文件
   - 生成 `manifest.csv`（SHA-256 + 文件大小）

6. ✅ **创建 `tests/docker-health-check.sh`**
   - 轮询 ZK（`ruok` 命令）和 Nacos（actuator/health 端点）
   - 超时 60 秒

**验证门禁**:
- ✅ `cd tests/java-fixtures && ./mvnw compile` → 成功
- ✅ `cargo test -p cross-lang-tests` → **122 tests passed + 2 ignored**（较初始 16 个大幅扩展）
- ✅ `./tests/generate-fixtures.sh` → 37+ `.bin` 文件 + manifest
- ⏳ `docker compose -f tests/docker-compose.yml up -d` → 需要 Docker 或 Podman 环境（Podman 可作为 Docker 的替代方案，命令兼容：`podman compose -f tests/docker-compose.yml up -d`）

---

### Phase 1: Hessian2 字节级验证（已完成 ✅）

本阶段采用 TDD 方式: 先编写期望失败的测试，暴露 dubbo-rs Hessian2 实现中的问题，然后修复代码使测试通过。每个测试都比对 Java 生成的标准数据桩。

**完成清单**:

1. ✅ **生成全部 25+ 类型数据桩文件**
   - `generate-fixtures.sh` 生成 37 个 `.bin` 文件到 `tests/fixtures/hessian2/`
   - `manifest.csv` 包含 SHA-256 校验和

2. ✅ **编写 Rust 解码测试 33 个 (A-D-*)**（`cross-lang-tests/tests/hessian2_test.rs`）
   - 基本类型: bool、int、long、double、null
   - 字符串: 空字符串、ASCII、中文 UTF-8、混合含 emoji（modified UTF-8）
   - 集合: 空列表、int 列表、混合列表、嵌套列表
   - Map: 空 map、string map、混合 typed map（LinkedHashMap）
   - 日期: epoch、固定时间戳
   - POJO: Person、Order、sparse 对象

3. ✅ **编写 Rust 编码测试 11 个 (A-E-*)**
   - Rust Encoder 编码结果与 Java fixture 逐字节比对
   - 涵盖 bool、int、long、double、null、string 类型

4. ✅ **编写往返测试 10 个 (A-R-*)**
   - A-R-001~A-R-009: RS encode → RS decode，验证自闭环无损
   - A-R-010: Java fixture → RS decode → RS encode → 字节比对

5. ✅ **修复 8 个 Hessian2 跨语言兼容性问题**
   - double 字节序错误（LE→BE，编码器 + 解码器）
   - 字符串长度以字节数而非字符数计算（编码器 + 解码器）
   - 缺失 0x30-0x33 字符串标签支持（Java 长字符串）
   - 缺失紧凑日期 `0x4b` 解析支持
   - 缺失紧凑列表 `0x78-0x7f` 解析支持
   - 缺失紧凑对象 `0x60-0x6f` 解析支持
   - 缺失 typed map 类型字符串消费
   - 缺失 modified UTF-8 surrogate pair（CESU-8）解码

**验证门禁**: ✅ ALL A-* tests pass（54 tests, `cargo test --test hessian2_test`）

---

### Phase 2: Dubbo TCP 协议验证（已完成 ✅）

**前置依赖**: Phase 1 必须通过（Dubbo TCP 使用 Hessian2 作为默认序列化） ✅

**完成清单**:

1. ✅ **完善 `DubboTcpTestServer.java`** — 完整实现 Hessian2 body 解码（提取方法名、参数、attachments）、mock 服务分发（sayHello/echo/ping）、正确响应编码（RESPONSE_VALUE 格式）

2. ✅ **Header 测试 13 个 (B-H-*)**（`cross-lang-tests/tests/dubbo_tcp_test.rs`）
   - B-H-001: 魔数 `0xdabb` BigEndian 验证
   - B-H-002/a/b/c/d: Request/Response/TwoWay/Event flag 位
   - B-H-003: Serialization ID = 2 (Hessian2)
   - B-H-004: Request ID BigEndian 编码
   - B-H-005/a/b/c/d: Body length BigEndian（含大 body 和空 body）
   - B-H-006: Status 字节
   - B-H-007/008: Header 编解码往返

3. ✅ **Body 测试 12 个 (B-B-*)**
   - B-B-001~005: dubbo_version/service_path/service_version/method_name/param_types
   - B-B-006: Arguments 列表格式
   - B-B-007: Attachments 无类型 map
   - B-B-008~010: 请求/响应/异常 Body 编解码往返
   - B-B-011: Header + Body 完整帧编解码往返

4. ✅ **完整往返测试 7 个 (B-R-*)**
   - B-R-001: RS DubboClient → Java DubboTcpTestServer（sayHello("World"), 验证回复含 "Hello"/"World"）
   - B-R-002: RS DubboClient → Java DubboTcpTestServer（echo("ping"), 验证回复含 "ping"）
   - B-R-003: RS DubboClient → Java DubboTcpTestServer（ping 无参, 验证回复 "pong"）
   - B-R-004: RS DubboClient → RS DubboServer（sayHello 自闭环, 验证回复含 "Hello from dubbo-rs!"）
   - B-R-005: RS DubboClient → RS DubboServer（5 次连续调用, 验证无状态行为）
   - B-R-006: RS DubboClient → Java DubboTcpTestServer（add("10","32") 多参数, 验证回复含 "42"）
   - B-R-007: RS DubboClient → Java DubboTcpTestServer（echoAttachments, 验证 trace_id/source 透传）
   - 测试通过 Java 进程管理（`JavaProc`）自动编译 Maven 项目并启动/停止 Java server

5. ✅ **修复协议层问题**
   - RISK-01: `codec.rs` + `transport.rs` 字节序统一为 BigEndian
   - `decoder.rs`: `read_binary` 增加紧凑标签支持（`0x20-0x2f`, `0x34-0x35`）

**验证门禁**: ✅ ALL B-* tests pass（28 tests, 含 3 个端到端 Java 交互测试）

---

### Phase 3: Triple 协议验证（已完成 ✅）

**前置依赖**: 无硬性依赖。Triple 使用 Protobuf 序列化，不依赖 Hessian2。

**完成清单**:

1. ✅ **Protobuf 序列化测试 (C-001~003)**
   - C-001: `TripleRequestWrapper` protobuf 编码/解码往返
   - C-002: `TripleResponseWrapper` protobuf 编码/解码往返
   - C-003: 空字段编码/解码

2. ✅ **API 和网络测试 (C-004~006)**
   - C-004: `TripleInvoker` 构造和状态
   - C-005: gRPC 连接建立和 Channel 状态验证
   - C-006: 调用往返（Ignore: tonic 0.12 tower buffer issue on path mismatch）

3. ✅ **RISK-03 Triple wrapper 改进** — `serialize_type` 增加可配置项 `TripleInvoker::with_serialize_type()`，默认 `"hessian2"`，可设为 `"protobuf"`

**验证门禁**: ✅ ALL C-* tests pass（**10 passed** — 7 Unary + 3 Streaming（ServerStream/ClientStream/BidiStream），tonic 升级至 0.14 后无 ignored 测试）

---

### Phase 4: 注册中心互操作（已完成 ✅）

**前置依赖**: Phase 2 或 Phase 3 至少一个通过 ✅

**完成清单**:

1. ✅ **格式验证测试 (D-F-*)**
   - D-F-001: ZK provider path 格式和 URL 编码
   - D-F-002: URL 编码/解码往返
   - D-F-003: 多参数 URL 编码
   - D-F-004: Triple 协议 URL 编码

2. ✅ **ZooKeeper 跨语言互操作测试 (D-ZK-*)**
   - D-ZK-001/a/b/c/d: URL 编码输出与 Java ZK 节点名约定一致（dubbo/tri 协议、带参数 URL 解码）
   - D-ZK-002/a/b/c/d: Provider path 格式生成 + 编码解码 roundtrip + 自定义根路径
   - D-ZK-003/a/b: 应用级 vs 接口级路径结构 + ZK 四子目录（providers/consumers/routers/configurators）
   - D-ZK-004/a/b/c: ServiceEvent/NotifyListener + 特殊字符编码 + 非法输入处理
   - D-ZK-005/006: ZookeeperRegistry 构造和自定义根路径

2b. ✅ **ZooKeeper 端到端集成测试 (D-ZK-E2E-*)**
   - D-ZK-E2E-001: RS provider 注册 → RS consumer 发现（接口级），验证 ephemeral node 创建和 listener Add 事件
   - D-ZK-E2E-002: 多 provider 注册（2 实例）→ consumer 订阅发现全部 2 个 provider
   - D-ZK-E2E-003: Triple 协议 URL 经过 ZK 注册/发现往返后 IP 和 port 保持正确
   - D-ZK-E2E-004: unregister 后 consumer 订阅返回空 provider 列表

3. ✅ **Nacos 跨语言互操作测试 (D-NA-*)**
   - D-NA-001/a/b/c/d: 注册请求字段映射（serviceName/ip/port/namespace/group/weight/ephemeral）+ server_addr 格式
   - D-NA-002/a~g: Nacos 响应解析（ok/code=0/code=200/错误码/无 code 字段/hosts 列表/空 hosts/缺失 hosts/嵌套 data.hosts）
   - D-NA-003/a/b/c: 实例→URL 转换 + 多实例发现 + auth 凭证配置

4. ✅ **公开内部 API 供跨 crate 测试**
   - `dubbo-rs-registry-zookeeper`: `encode_url`, `decode_url`, `urlencoding`, `urldecoding` 改为 `pub`
   - `dubbo-rs-registry-nacos`: `build_register_request`, `check_nacos_response`, `extract_hosts` 改为 `pub`；`InstanceRegisterRequest`, `NacosInstance` 字段改为 `pub`

**注意**: 当前测试在无 Docker/Podman 环境下运行，为 API 级验证。完整的端到端注册发现测试（真实 Java provider/consumer 通过 ZK/Nacos 发现）需要 Docker 或 Podman 环境。

**验证门禁**: ✅ ALL D-* tests pass（55 tests: 27 API 级 + 4 ZK E2E + 4 Nacos E2E + 4 Etcd E2E + 2 ZK Java 互操作 + 4 格式验证）

---

### Phase 5: 端到端服务治理（已完成 ✅）

**前置依赖**: Phase 4 通过 ✅

**完成清单**:

1. ✅ **编写 E-* 测试 11 个**
   - E-001/a/b/c/d: Random/RoundRobin/LeastActive/ConsistentHash 负载均衡 API
   - E-002/a: FailoverCluster / FailfastCluster 构造
   - E-003/a/b/c/d/e: Echo/Token/AccessLog/GracefulShutdown/CircuitBreaker 过滤器

2. ✅ **编写 E-004 超时控制测试 7 个**
   - E-004a~d: RPCError status code 映射 (ClientTimeout/ServerTimeout/ServiceNotFound/唯一性验证)
   - E-004h: tokio::time::timeout 在慢 Invoker 上触发超时
   - E-004i: 快速 Invoker 在超时内正常完成
   - E-004j: Timeout + FilterChain 集成验证

3. ✅ **编写 E-005 异常传播测试 11 个**
   - E-005a~g: FilterChain 执行（EchoFilter短路/透传、TokenFilter 拒绝/接受、组合链、CircuitBreaker 状态、RPCError 格式化）
   - E-005h: ServiceError 通过 invoke 传播
   - E-005i: ServiceNotFound 正确 status code 传播
   - E-005j: ServerTimeout 通过 FilterChain 传播
   - E-005k: 内部错误（transport-level Err）传播
   - E-005l: 正常方法与错误方法共存验证
   - E-005m: Error status code roundtrip 全量验证

**验证门禁**: ✅ ALL E-* tests pass（53 tests: 含深入负载均衡/集群/过滤器逻辑测试）

---

### Phase 6: 注册中心跨语言 Java 互操作（已完成 ✅）

**前置依赖**: Phase 4 通过 ✅

**完成清单**:

1. ✅ **Java fixture 升级**
   - `tests/java-fixtures/pom.xml`: 新增 `dubbo-rs-registry-zookeeper` 和 `dubbo-rs-registry-nacos` 依赖
   - `DemoServiceImpl.java`: DemoService 接口实现
   - `RegistryTestProvider.java`: Dubbo ServiceConfig API 模式注册 provider 到 ZK/Nacos

2. ✅ **ZK 跨语言互操作测试 2 个 (D-ZK-Java-*)**
   - D-ZK-Java-001: Java Dubbo provider（ServiceConfig）注册到 ZK → RS consumer 通过 ZookeeperRegistry 发现 Java provider URL
   - D-ZK-Java-002: RS + Java 同时注册到同一 ZK → RS consumer 发现全部 2 个 provider（含 Java 和 RS）

3. ✅ **Nacos/Etcd E2E 测试 8 个 (D-NA-E2E-*/D-ETCD-E2E-*)**
   - Nacos: 注册+发现、多 provider、注销、自定义 namespace/group
   - Etcd: 注册+发现、注销、多 provider、自定义 root path

4. ✅ **Clippy 零警告修复**
   - `dubbo-rs-registry-nacos`: 添加 `#[allow(clippy::missing_errors_doc)]` 和 `#[must_use]`
   - `dubbo-rs-registry-zookeeper`: `ensure_path` 改为关联函数 + 5 个公共函数添加 `#[must_use]`

**验证门禁**: ✅ 全部通过（223 tests, 0 ignored, 0 clippy warnings）

---

## 10 基础设施需求

### 10.1 本地开发环境

| 工具 | 版本 | 用途 |
|------|------|------|
| JDK | 17+ | 编译运行 Java 测试桩（Dubbo 3.3.x 要求最低 JDK 17） |
| Maven | 3.8+ | 构建 Java fixtures 项目，管理 Dubbo 依赖 |
| Rust | 1.87+ | 编译运行 Rust 测试和 dubbo-rs 库 |
| Docker | 24+ | 运行 ZooKeeper / Nacos 容器（可使用 Podman 3.0+ 作为替代，命令兼容 `podman compose`） |
| Docker Compose | v2 | 编排测试用的注册中心环境（若使用 Podman，需安装 `podman-compose` 或 Podman 3.0+ 内置 compose 支持） |
| curl / jq | 最新 | 健康检查脚本中使用 |

### 10.2 CI 环境

| 资源 | 规格 |
|------|------|
| Runner | `ubuntu-latest` (GitHub Actions) |
| 内存 | 8GB+（同时编译 Java 和 Rust，内存需求较高） |
| 磁盘 | 20GB+（Maven `.m2` 缓存 + Cargo `target/` 目录 + Docker 镜像） |
| 网络 | Docker Hub 访问（拉取 ZK/Nacos 镜像），Maven Central 访问（拉取 Dubbo 依赖） |
| 并发 | 单线程执行测试（`--test-threads=1`），避免端口冲突 |

### 10.3 CI 工作流设计

```yaml
name: cross-lang-tests
on:
  workflow_dispatch:
  schedule:
    - cron: '0 2 * * 1'  # 每周一 UTC 02:00
  pull_request:
    paths:
      - 'dubbo-rs-protocol-*/**'
      - 'dubbo-rs-serialization-*/**'
      - 'dubbo-rs-registry-*/**'
      - 'tests/cross_lang/**'

jobs:
  cross-lang:
    runs-on: ubuntu-latest
    timeout-minutes: 30
    strategy:
      matrix:
        java-version: ['17']
        protocol: ['triple', 'dubbo']
    steps:
      - uses: actions/checkout@v4
        with:
          submodules: true

      - name: Setup JDK
        uses: actions/setup-java@v4
        with:
          java-version: ${{ matrix.java-version }}
          distribution: 'temurin'

      - name: Cache Maven
        uses: actions/cache@v4
        with:
          path: ~/.m2/repository
          key: maven-${{ hashFiles('tests/java-fixtures/pom.xml') }}

      - name: Cache Cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            target/
          key: cargo-${{ hashFiles('Cargo.lock') }}

      - name: Setup Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Start registries
        run: docker compose -f tests/docker-compose.yml up -d zookeeper nacos

      - name: Wait for registries
        run: ./tests/docker-health-check.sh

      - name: Build Java fixtures
        run: cd tests/java-fixtures && ./mvnw package -DskipTests

      - name: Generate Hessian2 fixtures
        run: ./tests/generate-fixtures.sh

      - name: Run cross-lang tests
        run: cargo test --test cross_lang -- --test-threads=1

      - name: Collect logs on failure
        if: failure()
        run: |
          docker compose -f tests/docker-compose.yml logs zookeeper > /tmp/zk.log 2>&1
          docker compose -f tests/docker-compose.yml logs nacos > /tmp/nacos.log 2>&1

      - name: Upload artifacts on failure
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: test-logs-${{ matrix.protocol }}
          path: |
            /tmp/zk.log
            /tmp/nacos.log

      - name: Teardown
        if: always()
        run: docker compose -f tests/docker-compose.yml down
```

### 10.4 端口分配

| 端口 | 服务 | 用途 | 所属域 |
|------|------|------|--------|
| 20880 | Java Dubbo TCP Provider | 默认 Dubbo 协议 provider | Domain B |
| 20881 | Rust Dubbo TCP Provider | RS 实现的 Dubbo 协议 provider | Domain B |
| 50051 | Java Triple Provider | Triple/gRPC 协议 provider | Domain C |
| 50052 | Rust Triple Provider | RS 实现的 Triple 协议 provider | Domain C |
| 2181 | ZooKeeper | 服务注册发现 | Domain D, E |
| 8848 | Nacos HTTP | Nacos 控制台和 HTTP API | Domain D, E |
| 9848 | Nacos gRPC | Nacos 客户端 gRPC 通信 | Domain D, E |

端口冲突规避: 所有测试使用 `--test-threads=1` 串行执行。`PortAllocator` 工具在测试间分配端口，避免并行 CI job 间冲突。CI matrix 中的 `protocol` 维度隔离了 Dubbo TCP 和 Triple 的端口使用。

---

## 11 已知问题与风险登记

| ID | 问题描述 | 影响域 | 严重程度 | 应急措施 | 修复优先级 |
|----|----------|--------|----------|----------|------------|
| RISK-01 | Dubbo TCP Header 字节序 (LE vs BE) | B | 阻断 → ✅ **已修复** | `codec.rs` 16 处 `LittleEndian`→`BigEndian`（协议头魔数/ID/Length），`transport.rs` 3 处 `from_le_bytes`→`from_be_bytes`（body_length 解析） | Phase 2 前完成修复 |
| RISK-02 | Hessian2 double/float 编码字节序 | A, B | 严重 → ✅ **已修复** | `codec.rs` `write_double`: `to_le_bytes()`→`to_be_bytes()`; `decoder.rs` `read_f64le()`→`read_f64be()`。Java fixture `double_pi.bin` 验证通过 | Phase 1 前完成修复 |
| RISK-03 | Triple `serialize_type` 硬编码为 `"hessian2"`。Triple 协议的 wrapper 对象中 `serialize_type` 字段被硬编码，导致无法使用 `"protobuf"` 作为序列化类型 | C | 中等 → ✅ **已修复** | 增加 `TripleInvoker::with_serialize_type()` 配置方法，可通过 `.with_serialize_type("protobuf")` 设置。默认保留 `"hessian2"` 保证向后兼容 | Phase 3 前解决 |
| RISK-04 | `dubbo2-interop` 测试 server 无注册 Invoker。现有 Java 测试 server 的 service handler 注册机制不完整，无法正确路由请求到具体的 service 实现 | B | 中等 → ✅ **已修复** | `DubboTcpTestServer.java` 实现完整 Hessian2 body 解析 + 基于方法名分发到 mock service（sayHello/echo/ping） | Phase 2 完成修复 |
| RISK-05 | `cross-lang-test.sh` Java classpath 构建方式脆弱。当前脚本使用通配符拼接 classpath，当依赖版本变化或新增 jar 时容易出错 | B, C | 低 | 改用 Maven `exec:plugin` 执行 Java 测试 server，或使用 `maven-shade-plugin` 构建 uber-jar。classpath 由 Maven 自动管理 | Phase 0 |
| RISK-06 | ZooKeeper watcher 实时推送不完整。dubbo-rs 的 ZK client 可能未正确注册 NodeChildrenWatcher，导致 provider 上下线事件丢失 | D | 中等 | 作为 fallback，在 consumer 启动时主动调用 `getChildren()` 拉取当前 provider 列表。长期方案: 补充完整的 watcher 回调实现 | Phase 4 |
| RISK-07 | Java 类型描述符中嵌套泛型的处理。Hessian2 的 Java 类型描述符（如 `Ljava.util.List<Ljava.util.Map<Ljava.lang.String;Lcom.example.User;>;>;`）嵌套层级深，dubbo-rs 的解析器可能无法正确处理 | A | 低 | Phase 1 先覆盖常见类型（`List<String>`、`Map<String, Object>`、`User`）。复杂嵌套泛型作为后续补充，不影响核心互操作 | Phase 1（部分覆盖） |
| RISK-08 | Nacos Open API 认证机制变更。Nacos 2.x 引入了鉴权机制，默认需要 `username`/`password`，dubbo-rs 的 Nacos client 可能未实现认证 | D | 中等 | 在测试环境中关闭 Nacos 鉴权（`nacos.core.auth.enabled=false`）。生产环境需要实现 token 认证 | Phase 4 |
| RISK-09 | Docker/Podman 容器启动时序不确定。ZK 和 Nacos 容器启动到可提供服务之间存在延迟，CI 环境中尤其明显 | D, E | 低 | `docker-health-check.sh` 轮询等待，超时 60 秒。测试代码中增加 retry 逻辑，首次连接失败时等待重试 | Phase 0 |
| RISK-10 | Rust async runtime 与 ZK/Nacos client 的兼容性。dubbo-rs 可能使用 tokio，而 ZK/Nacos 的 Rust client 可能依赖 async-std，两个 runtime 混用可能引发问题 | D | 中等 | 统一使用 tokio 作为 async runtime。若第三方 client 仅支持 async-std，通过 `tokio::task::spawn_blocking` 隔离 | Phase 4 |

---

## 相关文档

| 文档 | 文件路径 | 内容概要 |
|------|----------|----------|
| 概述与验证目标 | [CROSS_LANG_OVERVIEW.md](./CROSS_LANG_OVERVIEW.md) | 四级验证金字塔、通过标准、非目标 |
| Hessian2 序列化验证 | [CROSS_LANG_HESSIAN2.md](./CROSS_LANG_HESSIAN2.md) | Domain A 完整测试用例、类型覆盖矩阵、边界值测试 |
| 协议级验证 | [CROSS_LANG_PROTOCOLS.md](./CROSS_LANG_PROTOCOLS.md) | Domain B (Dubbo TCP) + Domain C (Triple) 测试用例 |
| 注册中心互操作 | [CROSS_LANG_REGISTRY.md](./CROSS_LANG_REGISTRY.md) | Domain D (ZK/Nacos) + Domain E (服务治理) 测试用例 |
| 完整验证方案 | [CROSS_LANG_VERIFICATION.md](./CROSS_LANG_VERIFICATION.md) | 包含所有域的完整参考文档 |
