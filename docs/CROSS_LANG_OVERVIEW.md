# dubbo-rs ↔ dubbo-java 跨语言验证方案 — 概述与架构

| 字段 | 值 |
|------|-----|
| **状态** | ✅ COMPLETED — 全部 5 个 Phase 测试已实现并运行 |
| **最后更新** | 2026-05-06 |
| **适用版本** | dubbo-rs (current main) ↔ dubbo-java 3.3.x |
| **作者** | dubbo-rs team |

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|----------|
| v0.1 | 2026-05-06 | 初始草案：§1 概述、§2 测试架构、§3 序列化级验证 |
| v1.0 | 2026-05-06 | ✅ 全部 5 个 Phase 实现完成：122 tests passed / 2 ignored。覆盖 Hessian2 (54)、Dubbo TCP (28)、Triple (5+2)、注册中心 (8)、服务治理 (11)、基础设施 (16) |
| v1.1 | 2026-05-06 | 🚀 tonic 升级 0.12→0.14, Triple streaming 集成(Server/Client/Bidi 3 种模式), 修复 tower buffer panic, 消除全部 ignored tests。总计 **127 tests, 0 ignored** |

## 范围

本文档覆盖 **dubbo-rs ↔ dubbo-java 3.3.x** 之间的跨语言互操作性验证。dubbo-go 作为可选的辅助验证参照，不在主要验证路径上。

## 术语定义

| 术语 | 定义 |
|------|------|
| **dubbo-rs** | Apache Dubbo 协议的 Rust 实现，本项目的被测主体 |
| **dubbo-java** | Apache Dubbo 的 Java 参考实现，版本 3.3.x，作为互操作性的基准参照 |
| **dubbo-go** | Apache Dubbo 的 Go 实现，可选的辅助参照 |
| **Hessian2** | Dubbo 默认使用的二进制序列化协议，基于 Hessian 2.0 规范 |
| **Dubbo TCP** | Dubbo 原生 TCP 通信协议，16 字节固定头部 |
| **Triple** | Dubbo 3.x 引入的基于 HTTP/2 的协议，兼容 gRPC |
| **Fixture** | 预计算的参考字节流，由 Java 侧确定性生成，用于 Rust 侧解码验证 |
| **Roundtrip** | 往返测试：Java 编码 → Rust 解码 → Rust 重编码 → 语义等价比对 |
| **Sidecar** | 与 `.bin` fixture 文件配对的 `.meta.json` 元数据文件，记录编码参数 |
| **Harness** | 测试基础设施框架，管理进程生命周期、端口分配和断言 |

---

## §1 概述与验证目标

### 1.1 项目背景

dubbo-rs 项目旨在用 Rust 实现 Apache Dubbo 协议栈，使 Rust 服务能够与现有的 Java Dubbo 集群无缝互操作。互操作性是 dubbo-rs 生存的根基：一个 Rust 消费者必须能调用 Java 提供者，一个 Java 消费者也必须能调用 Rust 提供者。

跨语言验证的核心挑战在于：Java 和 Rust 对同一协议规范可能有不同的理解。字节序差异、序列化编码细节、协议头部字段解析，这些低层次的分歧会导致运行时故障。单独的单元测试无法暴露这些问题，因为它们在单一语言内运行，无法验证"对方能否读懂我发的字节"。

本文档定义了一套分层验证方案，从字节级序列化开始，逐层向上验证到端到端的跨语言调用。每一层都有明确的通过标准和具体的测试用例。

### 1.2 四级验证金字塔

验证按照金字塔结构组织，底层覆盖面最广、测试成本最低，顶层覆盖面最窄但最贴近真实场景。

```
                    ┌─────────────┐
                    │   Level 4   │
                    │  E2E 端到端  │  2-4 个场景
                    │  跨语言调用  │
                   ┌┴─────────────┴┐
                   │    Level 3     │
                   │  注册中心互发现 │  4-6 个场景
                   │  ZK / Nacos    │
                  ┌┴───────────────┴┐
                  │     Level 2      │
                  │  协议级互操作性   │  8-12 个场景
                  │  Dubbo TCP / Triple│
                 ┌┴─────────────────┴┐
                 │      Level 1       │
                 │  序列化级验证       │  25+ 个测试用例
                 │  Hessian2 编解码   │
                 └───────────────────┘
```

| 层级 | 验证内容 | 主要手段 | 预期用例数 |
|------|---------|----------|-----------|
| **Level 1** | Hessian2 字节级编解码 | Fixture 比对 | 25+ |
| **Level 2** | Dubbo TCP / Triple 协议交互 | 进程间调用 | 8-12 |
| **Level 3** | 注册中心服务发现互操作 | ZK/Nacos 集群 | 4-6 |
| **Level 4** | 端到端跨语言业务调用 | 完整链路 | 2-4 |

### 1.3 通过标准

跨语言验证的通过标准分为三个等级：

**必须通过（MUST PASS）**：
- Level 1 全部测试用例通过，Hessian2 编解码字节兼容
- Level 2 基本的 Dubbo TCP 和 Triple 请求/响应链路打通
- Level 3 至少一种注册中心（ZK 或 Nacos）的服务发现互通

**应当通过（SHOULD PASS）**：
- Level 1 边界值和异常场景覆盖
- Level 2 超时、异常处理等非主路径场景
- Level 3 ZK 和 Nacos 两种注册中心均互通

**建议通过（NICE TO HAVE）**：
- Level 4 完整端到端场景
- dubbo-go 辅助参照验证
- 性能基线对比

### 1.4 非目标

以下内容明确不在本文档的验证范围内：

- **性能基准测试**：跨语言性能对比是独立课题，不在互操作性验证内
- **安全特性验证**：TLS/mTLS、认证鉴权等安全层另行规划
- **向后兼容性**：与 Dubbo 2.x 的兼容性不在 3.3.x 验证范围内
- **dubbo-go 全量验证**：Go 实现仅作为辅助参照，不做全量互操作验证
- **生产部署方案**：本文档聚焦测试方法论，不涉及部署架构

---

## §2 测试架构与基础设施设计

### 2.1 整体架构

```
┌──────────────────────────────────────────────────────────────────────┐
│                        CI 环境 (GitHub Actions)                       │
│                                                                      │
│  ┌─────────────────┐    ┌──────────────────┐    ┌────────────────┐  │
│  │  Rust Test       │    │  Docker Compose   │    │  Rust Test     │  │
│  │  Harness         │───▶│                   │───▶│  Runner        │  │
│  │  (orchestrator)  │    │  ┌─────────────┐  │    │  (cargo test)  │  │
│  └─────────────────┘    │  │ ZooKeeper   │  │    └────────────────┘  │
│          │               │  │ :2181       │  │            │          │
│          │               │  └─────────────┘  │            │          │
│          ▼               │  ┌─────────────┐  │            ▼          │
│  ┌─────────────────┐    │  │ Nacos       │  │    ┌────────────────┐  │
│  │  Fixture Files   │    │  │ :8848       │  │    │  Test Results  │  │
│  │  .bin + .meta    │    │  └─────────────┘  │    │  + Reports     │  │
│  │  (pre-generated) │    │  ┌─────────────┐  │    └────────────────┘  │
│  └─────────────────┘    │  │ Java Prov.  │  │                         │
│                          │  │ DubboTCP    │  │                         │
│                          │  │ :20880      │  │                         │
│                          │  └─────────────┘  │                         │
│                          │  ┌─────────────┐  │                         │
│                          │  │ Java Prov.  │  │                         │
│                          │  │ Triple      │  │                         │
│                          │  │ :50051      │  │                         │
│                          │  └─────────────┘  │                         │
│                          └──────────────────┘                         │
└──────────────────────────────────────────────────────────────────────┘
```

数据流向：

```
Java Reference Encoder (Hessian2ReferenceEncoder.java)
        │
        ▼  生成 fixture 文件
tests/fixtures/hessian2/*.bin + *.meta.json
        │
        ▼  Rust harness 加载
tests/cross_lang/harness/hessian_fixture.rs
        │
        ▼  测试执行
tests/cross_lang/hessian2_tests.rs
        │
        ├─ Decode: Java bytes → Rust value
        ├─ Encode:   Rust value → Rust bytes
        └─ Roundtrip: Java bytes → Rust value → Rust bytes → 语义比对
```

### 2.2 Rust 测试 Harness 结构

```
tests/cross_lang/
├── harness/
│   ├── mod.rs              # 共享测试基础设施（端口分配、超时、初始化）
│   ├── java_process.rs     # 管理 Java provider/consumer 生命周期
│   ├── hessian_fixture.rs  # 加载预计算的 Java Hessian2 字节 fixture
│   └── assertions.rs       # 跨语言断言辅助（assert_bytes_match, assert_value_eq）
├── hessian2_tests.rs       # Domain A — 序列化级测试
├── dubbo_tcp_tests.rs      # Domain B — Dubbo TCP 协议测试
├── triple_tests.rs         # Domain C — Triple 协议测试
├── registry_tests.rs       # Domain D — 注册中心测试
└── e2e_tests.rs            # Domain E — 端到端测试
```

#### 2.2.1 `harness/mod.rs` — 共享基础设施

职责：
- 定义全局常量（超时时间、重试次数、最大等待秒数）
- 端口分配管理，避免多个测试并行时端口冲突
- 提供 `init_cross_lang_test()` 初始化函数，确保 Docker/Podman 服务就绪
- 提供辅助宏 `cross_lang_test` 标记跨语言测试

关键常量：

```rust,ignore
// harness/mod.rs
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;
pub const MAX_WAIT_SECS: u64 = 120;
pub const RETRY_COUNT: u32 = 10;
pub const RETRY_INTERVAL_MS: u64 = 1000;

pub const PORT_JAVA_DUBBO_TCP: u16 = 20880;
pub const PORT_RUST_DUBBO_TCP: u16 = 20881;
pub const PORT_JAVA_TRIPLE: u16 = 50051;
pub const PORT_RUST_TRIPLE: u16 = 50052;
pub const PORT_ZOOKEEPER: u16 = 2181;
pub const PORT_NACOS: u16 = 8848;
```

#### 2.2.2 `harness/java_process.rs` — Java 进程管理

职责：
- 启动和停止 Java provider/consumer 进程
- 健康检查：TCP 端口探测 + HTTP readiness endpoint
- 进程日志收集，测试失败时输出 Java 侧日志辅助排查
- 超时控制和优雅关闭

#### 2.2.3 `harness/hessian_fixture.rs` — Fixture 加载

职责：
- 根据 fixture 名称加载 `.bin` 文件和配对的 `.meta.json`
- 解析 `.meta.json` 中的元数据（编码类型、Java 类名、编码时间戳）
- 提供 `load_fixture(name: &str) -> FixtureData` 和 `load_fixture_bytes(name: &str) -> Vec<u8>`
- 提供验证辅助：确认 fixture 文件完整性（SHA-256 校验和）

#### 2.2.4 `harness/assertions.rs` — 跨语言断言

职责：
- `assert_bytes_match(expected: &[u8], actual: &[u8])` — 字节级精确比对
- `assert_value_eq<T: PartialEq + Debug>(expected: T, actual: T)` — 语义值比对
- `assert_hessian_roundtrip(java_bytes: &[u8], rust_value: impl ...)` — Roundtrip 往返断言
- 差异报告：输出期望值和实际值的十六进制对照表，方便定位字节偏移

### 2.3 Java Fixture 工程

```
tests/java-fixtures/
├── pom.xml                                          # dubbo 3.3.x + hessian2 依赖
└── src/main/java/org/apache/dubbo/test/
    ├── Hessian2ReferenceEncoder.java                # 确定性字节级参考输出
    ├── DubboTcpTestServer.java                      # 最小化 Dubbo TCP 提供者 (echo 服务)
    ├── TripleTestServer.java                        # 最小化 Triple 提供者 (Greeter 服务)
    ├── RegistryTestServer.java                      # ZK/Nacos 注册提供者
    └── TestTypes.java                               # 全部 Hessian2 测试 POJO 类型
```

#### 2.3.1 `Hessian2ReferenceEncoder.java`

这是整个 Level 1 验证的锚点。它的职责是用 Java Dubbo 的 Hessian2 实现，对每种类型的每个测试值进行确定性编码，输出到 `tests/fixtures/hessian2/` 目录。

设计原则：
- **确定性**：相同输入必须产生完全相同的字节输出，不受 JVM 版本、随机种子影响
- **可追溯**：每个 fixture 文件配对一个 `.meta.json`，记录 Java 类型、编码器版本、时间戳
- **完整覆盖**：覆盖 Hessian2 规范中所有 Dubbo 使用的类型标签

#### 2.3.2 `DubboTcpTestServer.java`

最小化 Dubbo TCP 提供者，用于 Level 2 验证。暴露一个简单的 echo 服务：

```java
// 伪代码示意
public class EchoServiceImpl implements EchoService {
    public String echo(String input) { return "echo: " + input; }
    public Object echoObject(Object obj) { return obj; }
}
```

监听端口 20880，使用 Hessian2 序列化。

#### 2.3.3 `TripleTestServer.java`

最小化 Triple 提供者，用于 Level 2/3 验证。暴露 Greeter 服务：

```java
// 伪代码示意
public class GreeterServiceImpl implements Greeter {
    public HelloReply sayHello(HelloRequest request) {
        return HelloReply.newBuilder()
            .setMessage("Hello " + request.getName())
            .build();
    }
}
```

监听端口 50051。

#### 2.3.4 `RegistryTestServer.java`

注册中心测试用提供者。分别支持 ZK 和 Nacos 两种注册中心，将服务注册到对应实例。用于 Level 3 验证。

#### 2.3.5 `TestTypes.java`

定义所有 Hessian2 测试所需的 POJO 类型：

```java
// 伪代码示意
public class TestTypes {
    public static class CustomPojo implements Serializable {
        private int id;
        private String name;
        private List<String> tags;
        // getters/setters/constructor
    }

    public static class NestedPojo implements Serializable {
        private CustomPojo inner;
        private Map<String, Object> metadata;
        // getters/setters/constructor
    }
}
```

### 2.4 Hessian2 Fixture 生成流程

#### 2.4.1 Fixture 文件格式

每个测试值生成两个文件：

**`.bin` 文件**：Hessian2 编码后的原始字节流。

**`.meta.json` 文件**：与 `.bin` 配对的元数据 sidecar 文件，格式如下：

```json
{
  "java_type": "java.lang.String",
  "rust_type": "String",
  "hessian2_tag": "S",
  "value_description": "Hello, Dubbo!",
  "encoder": "org.apache.dubbo.common.serialize.hessian2.Hessian2Serialization",
  "dubbo_version": "3.3.x",
  "generated_at": "2026-05-06T10:30:00Z",
  "sha256": "a1b2c3d4..."
}
```

#### 2.4.2 `generate-fixtures.sh` 脚本

```bash
#!/usr/bin/env bash
# generate-fixtures.sh
# 在 tests/java-fixtures/ 目录下运行
# 输出到 tests/fixtures/hessian2/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
FIXTURE_DIR="${SCRIPT_DIR}/../fixtures/hessian2"

# 构建 Java fixture 工程
cd "${SCRIPT_DIR}/java-fixtures"
mvn clean package -DskipTests -q

# 运行编码器，生成 fixture 文件
java -cp "target/fixture-generator.jar" \
  org.apache.dubbo.test.Hessian2ReferenceEncoder \
  --output-dir "${FIXTURE_DIR}"

# 校验完整性
echo "Generated fixtures in ${FIXTURE_DIR}:"
ls -la "${FIXTURE_DIR}"/*.bin | wc -l
echo "fixture files generated."

echo "Verifying SHA-256 checksums..."
cd "${FIXTURE_DIR}"
for meta in *.meta.json; do
  bin_file="${meta%.meta.json}.bin"
  expected=$(python3 -c "import json,sys; print(json.load(open('$meta'))['sha256'])")
  actual=$(sha256sum "$bin_file" | cut -d' ' -f1)
  if [ "$expected" != "$actual" ]; then
    echo "CHECKSUM MISMATCH: $bin_file"
    exit 1
  fi
done
echo "All checksums verified."
```

### 2.5 Docker Compose 配置

> **提示**: 如果本地没有安装 Docker，可以使用 Podman 作为替代方案。Podman 3.0+ 支持 `podman compose` 命令，与 `docker compose` 命令兼容。使用时将下文中的 `docker compose` 替换为 `podman compose` 即可。

```yaml
# tests/docker-compose.yml
version: "3.8"

services:
  zookeeper:
    image: zookeeper:3.8
    ports:
      - "2181:2181"
    healthcheck:
      test: ["CMD-SHELL", "echo ruok | nc localhost 2181 | grep imok"]
      interval: 5s
      timeout: 3s
      retries: 20

  nacos:
    image: nacos/nacos-server:v2.3.0
    environment:
      MODE: standalone
      JVM_XMS: 256m
      JVM_XMX: 256m
    ports:
      - "8848:8848"
      - "9848:9848"
    healthcheck:
      test: ["CMD-SHELL", "curl -sf http://localhost:8848/nacos/"]
      interval: 5s
      timeout: 3s
      retries: 30

  java-provider-dubbo:
    build:
      context: ../tests/java-fixtures
      dockerfile: Dockerfile
      args:
        SERVER_CLASS: org.apache.dubbo.test.DubboTcpTestServer
    ports:
      - "20880:20880"
    depends_on:
      zookeeper:
        condition: service_healthy
    healthcheck:
      test: ["CMD-SHELL", "nc -z localhost 20880"]
      interval: 5s
      timeout: 3s
      retries: 20

  java-provider-triple:
    build:
      context: ../tests/java-fixtures
      dockerfile: Dockerfile
      args:
        SERVER_CLASS: org.apache.dubbo.test.TripleTestServer
    ports:
      - "50051:50051"
    depends_on:
      zookeeper:
        condition: service_healthy
    healthcheck:
      test: ["CMD-SHELL", "nc -z localhost 50051"]
      interval: 5s
      timeout: 3s
      retries: 20
```

### 2.6 CI 集成

#### 2.6.1 GitHub Actions 工作流

```yaml
# .github/workflows/cross-lang-verification.yml
name: Cross-Language Verification

on:
  # 手动触发
  workflow_dispatch:
    inputs:
      java_version:
        description: "Dubbo Java version"
        default: "3.3.x"
        type: string

  # 定时调度：每周一 UTC 02:00
  schedule:
    - cron: "0 2 * * 1"

  # 路径触发
  push:
    paths:
      - "src/**"
      - "tests/cross_lang/**"
      - "tests/fixtures/**"
      - "tests/java-fixtures/**"
      - "tests/docker-compose.yml"
    branches: [main, develop]

  pull_request:
    paths:
      - "src/**"
      - "tests/cross_lang/**"
      - "tests/fixtures/**"
      - "tests/java-fixtures/**"
      - "tests/docker-compose.yml"

jobs:
  cross-lang-test:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        java-version: ["3.3.x"]

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Set up JDK 17
        uses: actions/setup-java@v4
        with:
          distribution: temurin
          java-version: "17"

      - name: Generate Hessian2 fixtures
        run: bash tests/generate-fixtures.sh

      - name: Start Docker services
        run: docker compose -f tests/docker-compose.yml up -d --wait

      - name: Wait for services healthy
        run: |
          timeout 120 bash -c '
            until nc -z localhost 20880 && nc -z localhost 50051; do
              sleep 2
            done
          '

      - name: Run cross-language tests
        run: |
          cargo test --test cross_lang \
            --features cross-lang-tests \
            -- \
            --nocapture \
            -Z unstable-options --format json | \
            tee target/cross-lang-test-results.json

      - name: Collect logs on failure
        if: failure()
        run: |
          docker compose -f tests/docker-compose.yml logs --no-color > target/docker-logs.txt
          echo "::group::Docker Compose Logs"
          cat target/docker-logs.txt
          echo "::endgroup::"

      - name: Stop Docker services
        if: always()
        run: docker compose -f tests/docker-compose.yml down -v

      - name: Upload test results
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: cross-lang-results-${{ matrix.java-version }}
          path: |
            target/cross-lang-test-results.json
            target/docker-logs.txt
```

### 2.7 端口分配表

| 端口 | 用途 | 方向 | 说明 |
|------|------|------|------|
| 20880 | Java Dubbo TCP Provider | Java 监听 | Level 2 Dubbo TCP 验证 |
| 20881 | Rust Dubbo TCP Provider | Rust 监听 | Level 2 Dubbo TCP 反向验证 |
| 50051 | Java Triple Provider | Java 监听 | Level 2/3 Triple 验证 |
| 50052 | Rust Triple Provider | Rust 监听 | Level 2/3 Triple 反向验证 |
| 2181 | ZooKeeper | 第三方服务 | Level 3 服务发现验证 |
| 8848 | Nacos HTTP | 第三方服务 | Level 3 服务发现验证 |
| 9848 | Nacos gRPC | 第三方服务 | Level 3 服务发现验证 |

端口使用规则：
- 每个测试在启动时通过 `harness::allocate_port()` 获取端口
- 测试结束后释放端口，避免并行测试冲突
- CI 环境中串行执行跨语言测试，避免端口竞争
- 本地开发时可使用环境变量覆盖默认端口

---

## 相关文档

| 文档 | 内容 |
|------|------|
| [概述与架构](./CROSS_LANG_OVERVIEW.md) | 验证目标、测试架构、基础设施设计 |
| [Hessian2 序列化](./CROSS_LANG_HESSIAN2.md) | 序列化级验证、类型覆盖矩阵、数据桩 |
| [协议级验证](./CROSS_LANG_PROTOCOLS.md) | Dubbo TCP + Triple 协议验证 |
| [注册发现与端到端](./CROSS_LANG_REGISTRY.md) | ZK/Nacos 注册中心、E2E 服务治理 |
| [实施计划](./CROSS_LANG_PLAN.md) | 测试用例索引、阶段计划、基础设施、风险 |
