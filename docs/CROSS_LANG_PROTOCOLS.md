# dubbo-rs ↔ dubbo-java 跨语言验证方案 — 协议级验证

> 本文档为跨语言验证方案的 **协议级验证** 部分（Domain B: Dubbo TCP + Domain C: Triple/gRPC）。
> 完整方案拆分为 5 个文档，详见底部 [相关文档](#相关文档)。

---

## 4. Domain B — 协议级验证（Dubbo TCP）

### 4.1 方法论

**目标**: 验证 dubbo-rs 的 16 字节 Dubbo 协议头编码与 Hessian2 序列化体（body）的帧格式是否与 dubbo-java 完全兼容。

**验证路径**: Header 单元测试 → Body 单元测试 → 双向往返集成测试。先从最小可验证单元（16 字节头）开始，逐层叠加复杂度，最终在真实 TCP 连接上完成跨语言请求响应。

**核心风险点**:
- 字节序问题: Rust 在 x86_64 上原生为小端序（LE），Dubbo 协议定义的大端序（BE）字段（Request ID、Body Length）需要显式处理，任何遗漏都会导致帧解析失败
- 序列化 ID 映射: Flags 字段低 5 位标识序列化方式，Hessian2 对应值 2，需要确认 dubbo-rs 和 dubbo-java 使用相同映射
- 状态码一致性: dubbo-rs 返回的状态码必须落在 dubbo-java 定义的枚举范围内，否则 Java 端会解析失败

### 4.2 Dubbo TCP Header 格式参考

Dubbo 协议帧头固定 16 字节，采用大端序（Big Endian）编码:

| 偏移量 | 长度 | 字段 | 说明 |
|--------|------|------|------|
| 0 | 2 bytes | Magic Number | 固定值 `0xdabb`，用于帧边界识别 |
| 2 | 1 byte | Flags | 控制标志位，编码请求方向、通信模式、事件标记和序列化方式 |
| 3 | 1 byte | Status | 响应状态码，仅在响应帧中有效 |
| 4 | 8 bytes | Request Id | 有符号 64 位整数，用于请求响应配对 |
| 12 | 4 bytes | Body Length | 有符号 32 位整数，表示后续 body 的字节长度 |

**Flags 位编码**:

| 位 | 含义 | 说明 |
|----|------|------|
| bit 7 (0x80) | Req/Res | 1 = 请求, 0 = 响应 |
| bit 6 (0x40) | TwoWay | 1 = 双向（需响应）, 0 = 单向 |
| bit 5 (0x20) | Event | 1 = 事件/心跳 |
| bits 0-4 | SerialId | 序列化方式标识, 2 = Hessian2 |

**常用 Flags 组合值**:

| Flags 值 | 含义 | 展开说明 |
|----------|------|----------|
| `0xC2` | 标准双向请求 | bit7=1(Request), bit6=1(TwoWay), SerialId=2(Hessian2) |
| `0x82` | 双向响应 | bit7=0(Response), bit6=1(TwoWay), SerialId=2 |
| `0xE2` | 事件/心跳 | bit7=1, bit6=1, bit5=1(Event), SerialId=2 |
| `0x42` | 单向请求 | bit7=1, bit6=0(OneWay), SerialId=2 |

**Status 状态码**:

| 十六进制 | 十进制 | 含义 |
|------|--------|------|
| 0x14 | 20 | OK |
| 0x1E | 30 | CLIENT_TIMEOUT |
| 0x28 | 40 | BAD_REQUEST |
| 0x32 | 50 | BAD_RESPONSE |
| 0x3C | 60 | SERVICE_NOT_FOUND |
| 0x46 | 70 | SERVICE_ERROR |
| 0x50 | 80 | SERVER_ERROR |
| 0x5A | 90 | CLIENT_ERROR |

### 4.3 Header 编码测试用例（B-H 系列）

#### Test B-H-001: 标准双向请求头（reqID=1）
- **领域**: B (Dubbo TCP)
- **分类**: Header
- **方向**: N/A (header unit test)
- **输入**: Flags=0xC2, Status=0, RequestID=1, BodyLength=0
- **预期输出**: 16-byte hex: `dabb c2 00 0000000000000001 00000000`
- **通过标准**: 逐字节比对预期 hex 值，Request ID 和 Body Length 必须为大端序
- **已知风险**: Rust `i64::to_be_bytes()` 是否被正确调用；遗漏 `.to_be_bytes()` 会产生逆序输出

#### Test B-H-002: 标准双向请求头（reqID=42）
- **领域**: B (Dubbo TCP)
- **分类**: Header
- **方向**: N/A
- **输入**: Flags=0xC2, Status=0, RequestID=42, BodyLength=0
- **预期输出**: `dabb c2 00 000000000000002a 00000000`
- **通过标准**: 逐字节比对

#### Test B-H-003: 心跳请求头
- **领域**: B (Dubbo TCP)
- **分类**: Header
- **方向**: N/A
- **输入**: Flags=0xE2, Status=0, RequestID=1, BodyLength=0
- **预期输出**: `dabb e2 00 0000000000000001 00000000`
- **通过标准**: Flags 字节为 `0xE2`（Event 位 = 1）

#### Test B-H-004: 单向请求头
- **领域**: B (Dubbo TCP)
- **分类**: Header
- **方向**: N/A
- **输入**: Flags=0x42, Status=0, RequestID=1, BodyLength=0
- **预期输出**: `dabb 42 00 0000000000000001 00000000`
- **通过标准**: bit6 清零后 Java 端不应回复响应

#### Test B-H-005: OK 响应头
- **领域**: B (Dubbo TCP)
- **分类**: Header
- **方向**: N/A
- **输入**: Flags=0x82, Status=20, RequestID=1, BodyLength=0
- **预期输出**: `dabb 82 14 0000000000000001 00000000`

#### Test B-H-006: BAD_REQUEST 错误响应头
- **领域**: B (Dubbo TCP)
- **分类**: Header
- **方向**: N/A
- **输入**: Flags=0x82, Status=40, RequestID=1, BodyLength=0
- **预期输出**: `dabb 82 28 0000000000000001 00000000`

#### Test B-H-007: SERVICE_NOT_FOUND 响应头
- **领域**: B (Dubbo TCP)
- **分类**: Header
- **方向**: N/A
- **输入**: Flags=0x82, Status=60, RequestID=1, BodyLength=0
- **预期输出**: `dabb 82 3c 0000000000000001 00000000`

#### Test B-H-008: 字节序验证——与 Java 参考实现 Hex Dump 对比
- **领域**: B (Dubbo TCP)
- **分类**: Header
- **方向**: N/A
- **输入**: Flags=0xC2, Status=0, RequestID=305419896（0x12345678）, BodyLength=258（0x00000102）
- **预期输出**: `dabb c2 00 0000000012345678 00000102`
- **通过标准**: 使用 dubbo-java 生成的相同参数帧作为参考，Rust 输出必须与其逐字节一致

### 4.4 Body 编码测试用例（B-B 系列）

#### Test B-B-001: sayHello(String) 请求体
- **领域**: B (Dubbo TCP)
- **分类**: Body
- **方向**: RS→Java
- **输入**: Service=`org.apache.dubbo.api.demo.DemoService`, Method=`sayHello`, Param types=`Ljava/lang/String;`, Arguments=`["world"]`
- **预期输出**: Hessian2 编码的二进制流，含 dubbo_version → service → version → method → param_types → arguments → attachments
- **通过标准**: 与 dubbo-java 编码的 body 字节流二进制比对一致

#### Test B-B-002: greet(String, int) 请求体
- **领域**: B (Dubbo TCP)
- **分类**: Body
- **方向**: RS→Java
- **输入**: Method=`greet`, Param types=`Ljava/lang/String;I`, Arguments=`["test", 42]`
- **通过标准**: 多参数场景下 param_types 编码正确

#### Test B-B-003: 携带 attachments 的请求体
- **领域**: B (Dubbo TCP)
- **分类**: Body
- **方向**: RS→Java
- **输入**: Attachments=`{ "path": "...", "interface": "...", "version": "1.0.0" }`
- **通过标准**: attachments Map 的 Hessian2 编码完整正确

#### Test B-B-004: OK 响应体（含字符串返回值）
- **领域**: B (Dubbo TCP)
- **分类**: Body
- **方向**: Java→RS
- **输入**: Response flag=`2`（RESPONSE_VALUE，有返回值）, Value=`"Hello from provider"`
- **通过标准**: Rust 端正确解析 result flag 和字符串值

#### Test B-B-005: 错误响应体（含异常）
- **领域**: B (Dubbo TCP)
- **分类**: Body
- **方向**: Java→RS
- **输入**: Response flag=`0`（RESPONSE_WITH_EXCEPTION，异常）, Error=`"java.lang.RuntimeException: test error"`
- **通过标准**: Rust 端正确识别异常响应

> **⚠️ 实现注意**: 当前 `dubbo-rs-protocol-dubbo/src/body.rs` 使用 `write_binary(arg)` 编码每个参数。
> Java Dubbo 端按参数实际类型序列化（如 String 参数使用 Hessian2 String tag，int 使用 int tag），
> 而非统一用 binary 包装。这可能是仅次于字节序 bug 的第二大互操作风险，
> 在 Phase 2 测试中需重点验证 Test B-B-001/002 的参数编码是否正确被 Java 端解析。

### 4.5 完整往返测试用例（B-R 系列）

#### Test B-R-001: RS 消费者 → Java 提供者，sayHello("World")
- **领域**: B (Dubbo TCP)
- **分类**: Roundtrip
- **方向**: RS→Java
- **前置条件**: Java DubboTcpTestServer (由 JavaProc 管理)
- **操作步骤**:
  1. JavaProc 自动编译 Maven 项目并启动 Java DubboTcpTestServer
  2. RS DubboClient 连接 `127.0.0.1:{port}`
  3. 发送 `sayHello` 请求，参数 `"World"` 使用 Hessian2 编码
  4. 解析响应 header 和 body
- **预期结果**: Response Status=20(OK), body 中返回值包含 `"Hello"` 和 `"World"`
- **测试文件**: `tests/dubbo_tcp_test.rs :: b_r_roundtrip::br_001_rs_client_to_java_server_sayhello`
- **通过标准**: 响应状态 OK，body 返回值包含 "Hello" 和 "World"

#### Test B-R-002: RS 消费者 → Java 提供者，echo("ping")
- **领域**: B (Dubbo TCP)
- **分类**: Roundtrip
- **方向**: RS→Java
- **前置条件**: Java DubboTcpTestServer
- **操作步骤**: RS DubboClient 发送 `echo("ping")` 请求
- **预期结果**: Java 端回显回复包含 `"ping"`
- **测试文件**: `tests/dubbo_tcp_test.rs :: b_r_roundtrip::br_002_rs_client_to_java_server_echo`
- **通过标准**: 响应包含 "ping"

#### Test B-R-003: RS 消费者 → Java 提供者，ping() 无参调用
- **领域**: B (Dubbo TCP)
- **分类**: Roundtrip
- **方向**: RS→Java
- **前置条件**: Java DubboTcpTestServer
- **操作步骤**: RS DubboClient 发送 `ping()` 无参请求
- **预期结果**: 回复 `"pong"`
- **测试文件**: `tests/dubbo_tcp_test.rs :: b_r_roundtrip::br_003_rs_client_to_java_server_ping`
- **通过标准**: 精确匹配回复 "pong"

> **⚠️ 注意**: 当前 3 个 B-R 测试均为 RS→Java 方向。以下测试为**计划中但尚未实现**:
>   - Java→RS provider 完整调用
>   - RS provider → RS consumer 自闭环
>   - 多参数方法调用
>   - Attachments 透传验证
>   - 心跳请求/事件响应
>   - 连续 10 次调用成功率
>   - 大负载（100KB）正确性

---

## 5. Domain C — 协议级验证（Triple/gRPC）

### 5.1 方法论

**目标**: 验证 dubbo-rs（基于 tonic）与 dubbo-java 通过 Triple 协议（HTTP/2 + gRPC）进行通信的兼容性。

**为什么这个域最可靠**: Triple 协议建立在标准 gRPC 之上，两端共享相同的 `.proto` 定义文件。Protobuf 的二进制编码是确定性且跨语言标准化的，tonic 是 Rust 生态中成熟度最高的 gRPC 框架。

**验证路径**: Proto 编码兼容性 → Triple Wrapper 编解码 → 全链路往返测试。

### 5.2 .proto 兼容性验证（C-P 系列）

#### Test C-P-001: Proto 二进制编码兼容性
- **领域**: C (Triple/gRPC)
- **分类**: Proto
- **方向**: N/A
- **输入**: `HelloRequest { name: "test" }`
- **操作步骤**:
  1. Rust prost 编码 `HelloRequest { name: "test" }`
  2. Java protobuf-java 编码相同消息
  3. 比对输出字节
- **预期结果**: 两端字节流完全一致（protobuf 3 确定性编码）
- **通过标准**: 逐字节匹配

### 5.3 Triple Wrapper 编码验证（C-W 系列）

#### Test C-W-001: TripleRequestWrapper 序列化
- **领域**: C (Triple/gRPC)
- **分类**: Wrapper
- **方向**: RS→Java
- **输入**: `serialize_type="protobuf"`, `args=[HelloRequest 编码]`, `arg_types=["com.example.HelloRequest"]`
- **通过标准**: Java 端成功还原 HelloRequest，wrapper 字段正确

#### Test C-W-002: TripleResponseWrapper 反序列化
- **领域**: C (Triple/gRPC)
- **分类**: Wrapper
- **方向**: Java→RS
- **通过标准**: Rust 端提取 HelloReply 成功

### 5.4 完整往返测试用例（C-R 系列）

#### Test C-R-001: RS 消费者 → Java 提供者 Unary 调用
- **领域**: C (Triple/gRPC)
- **分类**: Roundtrip
- **方向**: RS→Java
- **前置条件**: Java Triple provider 端口 50051
- **操作步骤**: tonic channel → `sayHello(HelloRequest { name: "world" })`
- **预期结果**: gRPC OK, `HelloReply.message = "Hello world"`

#### Test C-R-002: Java 消费者 → RS 提供者 Unary 调用
- **领域**: C (Triple/gRPC)
- **分类**: Roundtrip
- **方向**: Java→RS
- **前置条件**: RS Triple provider 端口 50052
- **预期结果**: Java `HelloReply.getMessage()` = "Hello world"

#### Test C-R-003: 空名称 → 默认问候语
- **领域**: C (Triple/gRPC)
- **分类**: Roundtrip
- **方向**: RS→Java
- **输入**: `HelloRequest { name: "" }`
- **预期结果**: 返回有效默认问候语

#### Test C-R-004: Unicode 名称（"世界", "🌍"）→ UTF-8 正确处理
- **领域**: C (Triple/gRPC)
- **分类**: Roundtrip
- **方向**: RS→Java
- **通过标准**: 返回值完整保留 Unicode 字符，无乱码

#### Test C-R-005: curl → Java 提供者（JSON 模式）→ HTTP 200
- **领域**: C (Triple/gRPC)
- **分类**: Roundtrip
- **方向**: N/A
- **操作步骤**: `curl -X POST http://127.0.0.1:50051/... -H 'Content-Type: application/json' -d '{"name": "world"}'`
- **通过标准**: HTTP 200，JSON 响应中 message="Hello world"

#### Test C-R-006: grpcurl → Rust 提供者 → 正确响应
- **领域**: C (Triple/gRPC)
- **分类**: Roundtrip
- **方向**: N/A
- **操作步骤**: `grpcurl -plaintext -d '{"name": "world"}' 127.0.0.1:50052 ...Greeter/SayHello`
- **通过标准**: gRPC OK，响应 message 匹配

#### Test C-R-007: Server Streaming RS→Java (P2)
- **领域**: C (Triple/gRPC)
- **分类**: Roundtrip (Server Streaming)
- **方向**: RS→Java
- **优先级**: P2
- **前置条件**: Java Triple provider 支持服务端流式方法
- **操作步骤**: RS consumer 发起 server streaming RPC，接收多条响应消息
- **预期结果**: RS consumer 完整接收全部流式响应，消息顺序和内容正确
- **通过标准**: 收到预期数量的响应消息，每条消息内容匹配

#### Test C-R-008: Client Streaming RS→Java (P2)
- **领域**: C (Triple/gRPC)
- **分类**: Roundtrip (Client Streaming)
- **方向**: RS→Java
- **优先级**: P2
- **前置条件**: Java Triple provider 支持客户端流式方法
- **操作步骤**: RS consumer 发起 client streaming RPC，发送多条请求消息后接收聚合响应
- **预期结果**: Java provider 正确接收全部流式请求并返回聚合结果
- **通过标准**: 响应内容包含或反映了全部发送的请求消息

---

## 附录 B: Dubbo TCP 协议参考规范

### B.1 16 字节 Header 布局

Dubbo TCP 协议的每个请求/响应以固定的 16 字节 header 开头:

```
 偏移    长度    字段            说明
 ────────────────────────────────────────────────────────
 0-1     2B      Magic Number   固定值 0xDABB (大端序)
 2       1B      Req/Res Flag   第 8 位: 0=Response, 1=Request
                              第 0-7 位: 序列化类型 ID
                                2 = Hessian2
                                3 = JavaSerialization
                                4 = CompactedJavaSerialization
                                7 = FastJson
                                8 = native-hessian
 3       1B      Status         仅 Response 有效:
                                20 (OK)
                                30 (CLIENT_TIMEOUT)
                                31 (SERVER_TIMEOUT)
                                40 (BAD_REQUEST)
                                50 (BAD_RESPONSE)
                                60 (SERVICE_NOT_FOUND)
                                70 (SERVICE_ERROR)
                                80 (SERVER_ERROR)
                                90 (CLIENT_ERROR)
                                100 (SERVER_THREADPOOL_EXHAUSTED_ERROR)
 4-11    8B      Request ID     64 位无符号整数 (大端序)，用于匹配 Request 和 Response
 12-15   4B      Body Length    32 位无符号整数 (大端序)，表示 body 的字节长度
```

字节级视图:

```
 Byte:  [0]  [1]  [2]       [3]      [4] [5] [6] [7] [8] [9] [10][11] [12][13][14][15]
         DA   BB   Req|Ser   Status   Request ID (8 bytes, BE)   Body Length (4B, BE)
```

### B.2 Request Body 结构

Request body 使用 Hessian2 序列化，按顺序包含 7 个字段:

```
 Hessian2 Field            Java 类型          说明
 ──────────────────────────────────────────────────────────────────────
 1. Dubbo version          String            例: "2.0.2" 或 "3.3.0"
 2. Service interface      String            例: "org.apache.dubbo.DemoService"
 3. Service version        String            例: "1.0.0" 或空字符串 ""
 4. Method name            String            例: "sayHello"
 5. Parameter types        String            例: "Ljava.lang.String;"
 6. Arguments              Object[]          每个参数独立 Hessian2 序列化
 7. Attachments            Map<String,String> 含 path, interface, version 等元数据
```

编码示例 (简化):
```
 Hessian2("2.0.2")         →  53 00 05 32 2e 30 2e 32
 Hessian2("sayHello")      →  53 00 08 73 61 79 48 65 6c 6c 6f
 Hessian2("Ljava.lang.String;") →  53 00 12 4c 6a 61 76 61 ...
```

### B.3 Response Body 结构

Response body 同样使用 Hessian2 序列化:

```
 Hessian2 Field            Java 类型          说明
 ──────────────────────────────────────────────────────────────────────
 1. Result flag            byte              1 个字节:
                                               0x00 = RESPONSE_WITH_EXCEPTION
                                               0x01 = RESPONSE_NULL_VALUE
                                               0x02 = RESPONSE_VALUE
 2. Result value           Object            根据 flag:
                                               0x00 → Hessian2(Exception object)
                                               0x02 → Hessian2(return value)
                                               0x01 → 无后续数据 (null)
```

### B.4 Status Code 完整列表

| Code | 常量名 | 说明 |
|------|--------|------|
| 20 | `OK` | 调用成功 |
| 30 | `CLIENT_TIMEOUT` | 客户端超时 |
| 31 | `SERVER_TIMEOUT` | 服务端超时 |
| 40 | `BAD_REQUEST` | 请求格式错误 |
| 50 | `BAD_RESPONSE` | 响应格式错误 |
| 60 | `SERVICE_NOT_FOUND` | 服务未找到 |
| 70 | `SERVICE_ERROR` | 服务调用异常 |
| 80 | `SERVER_ERROR` | 服务端内部错误 |
| 90 | `CLIENT_ERROR` | 客户端内部错误 |
| 100 | `SERVER_THREADPOOL_EXHAUSTED_ERROR` | 服务端线程池耗尽 |

---

## 相关文档

| 文档 | 内容 |
|------|------|
| [概述与架构](./CROSS_LANG_OVERVIEW.md) | 验证目标、测试架构、基础设施设计 |
| [Hessian2 序列化](./CROSS_LANG_HESSIAN2.md) | 序列化级验证、类型覆盖矩阵、数据桩 |
| [协议级验证](./CROSS_LANG_PROTOCOLS.md) | Dubbo TCP + Triple 协议验证 |
| [注册发现与端到端](./CROSS_LANG_REGISTRY.md) | ZK/Nacos 注册中心、E2E 服务治理 |
| [实施计划](./CROSS_LANG_PLAN.md) | 测试用例索引、阶段计划、基础设施、风险 |
