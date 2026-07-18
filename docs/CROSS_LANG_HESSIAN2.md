# dubbo-rs ↔ dubbo-java 跨语言验证方案 — Hessian2 序列化

> 本文档为跨语言验证方案的 **序列化级验证** 部分（Domain A）。
> 完整方案拆分为 5 个文档，详见底部 [相关文档](#相关文档)。

---

## §3 Domain A — 序列化级验证（Hessian2）

### 3.1 验证方法论

Domain A 是整个验证金字塔的根基。它不依赖任何网络通信，不启动任何外部服务，纯粹验证 Rust 和 Java 对 Hessian2 字节流的理解是否一致。

#### 3.1.1 三种验证模式

**模式一：Decode 解码验证**

```
Java 编码输出 (.bin)
        │
        ▼
Rust Hessian2 解码器
        │
        ▼
Rust 值
        │
        ▼
与 .meta.json 中记录的 value_description 做语义比对
```

验证 Rust 能正确读懂 Java 生成的 Hessian2 字节流。

**模式二：Encode 编码验证**

```
Rust 构造目标值
        │
        ▼
Rust Hessian2 编码器
        │
        ▼
Rust 编码字节
        │
        ▼
与 Java fixture (.bin) 做字节级比对
```

验证 Rust 编码出的字节能被 Java 正确解读。这里要处理 Hessian2 的紧凑编码（compact encoding）：同一个值可能有多种合法的编码形式。

**模式三：Roundtrip 往返验证**

```
Java 编码 (.bin)
        │
        ▼
Rust 解码 → Rust 值
        │
        ▼
Rust 重编码 → Rust 字节
        │
        ▼
将 Rust 重编码的字节再次解码
        │
        ▼
与首次解码得到的 Rust 值做语义等价比对
```

Roundtrip 不要求 Rust 编码输出与 Java 字节完全一致，只要求语义等价。这是最实际也最重要的验证模式。

#### 3.1.2 紧凑编码兼容性

Hessian2 规范允许对多种类型使用紧凑编码。例如：
- 整数 `-16` 到 `47` 可以用单字节编码（`0x80` 到 `0xbf`）
- 长度不超过 31 的字符串可以用短格式（`0x00` 到 `0x1f` + UTF-8 字节）
- 长度不超过 15 的列表可以用短格式

Rust 解码器必须能接受 Java 使用的任何合法编码形式。Rust 编码器不要求输出与 Java 完全一致的字节，但必须产生语义等价的编码。

**宽松模式（默认）**：Roundtrip 测试采用语义等价比对，接受不同的合法编码。

**严格模式（可选）**：字节级精确比对。用于特定类型（如 boolean、null）的验证，这些类型的编码形式是唯一的。

### 3.2 类型覆盖矩阵

下表列出全部需要验证的类型。每一行对应至少一组 Decode + Encode + Roundtrip 测试用例。

| # | Java 类型 | Rust 类型 | Hessian2 标签 | Fixture 文件 | Test Decode ID | Test Encode ID | Test Roundtrip ID |
|---|-----------|-----------|--------------|-------------|----------------|----------------|-------------------|
| 1 | `boolean` (true) | `bool` | `T` | `bool_true` | A-D-001 | A-E-001 | A-R-001 |
| 2 | `boolean` (false) | `bool` | `F` | `bool_false` | A-D-002 | A-E-002 | A-R-002 |
| 3 | `byte` | `i8` | `I` (int 32 提升) | `byte_42` | A-D-003 | A-E-003 | A-R-003 |
| 4 | `java.lang.Byte` | `Option<i8>` | `I`/`N` | `byte_boxed_42` | A-D-004 | A-E-004 | A-R-004 |
| 5 | `short` | `i16` | `I` | `short_12345` | A-D-005 | A-E-005 | A-R-005 |
| 6 | `java.lang.Short` | `Option<i16>` | `I`/`N` | `short_boxed_12345` | A-D-006 | A-E-006 | A-R-006 |
| 7 | `int` | `i32` | `I` | `int_100000` | A-D-007 | A-E-007 | A-R-007 |
| 8 | `java.lang.Integer` | `Option<i32>` | `I`/`N` | `integer_boxed_100000` | A-D-008 | A-E-008 | A-R-008 |
| 9 | `long` | `i64` | `L` | `long_9876543210` | A-D-009 | A-E-009 | A-R-009 |
| 10 | `java.lang.Long` | `Option<i64>` | `L`/`N` | `long_boxed_9876543210` | A-D-010 | A-E-010 | A-R-010 |
| 11 | `float` | `f32` | `D` (double 提升) | `float_3_14f` | A-D-011 | A-E-011 | A-R-011 |
| 12 | `java.lang.Float` | `Option<f32>` | `D`/`N` | `float_boxed_3_14f` | A-D-012 | A-E-012 | A-R-012 |
| 13 | `double` | `f64` | `D` | `double_3_14159265358979` | A-D-013 | A-E-013 | A-R-013 |
| 14 | `java.lang.Double` | `Option<f64>` | `D`/`N` | `double_boxed_3_14159265358979` | A-D-014 | A-E-014 | A-R-014 |
| 15 | `java.lang.String` | `String` | `S`/`R` | `string_hello` | A-D-015 | A-E-015 | A-R-015 |
| 16 | `java.lang.String` (空串) | `String` | `S` | `string_empty` | A-D-016 | A-E-016 | A-R-016 |
| 17 | `java.lang.String` (64KB) | `String` | `R` (chunk) | `string_64kb` | A-D-017 | A-E-017 | A-R-017 |
| 18 | `byte[]` | `Vec<u8>` | `B` | `bytes_short` | A-D-018 | A-E-018 | A-R-018 |
| 19 | `java.util.Date` | `JavaDate(i64)` | `d` | `date_20260101` | A-D-019 | A-E-019 | A-R-019 |
| 20 | `java.math.BigDecimal` | `BigDecimal` | map def | `bigdecimal_pi` | A-D-020 | A-E-020 | A-R-020 |
| 21 | `java.util.List<String>` | `Vec<String>` | `V` | `list_string_3` | A-D-021 | A-E-021 | A-R-021 |
| 22 | `java.util.Map<String,String>` | `HashMap<String,String>` | `M` | `map_ss_3` | A-D-022 | A-E-022 | A-R-022 |
| 23 | `null` | `Option::None` | `N` | `null_value` | A-D-023 | A-E-023 | A-R-023 |
| 24 | `Throwable` | `JavaException` | map def | `throwable_runtime` | A-D-024 | A-E-024 | A-R-024 |
| 25 | Custom POJO | `struct` | `C`+`O` | `pojo_custom` | A-D-025 | A-E-025 | A-R-025 |
| 26 | 循环引用 | 带回指的图结构 | `R` (ref) | `circular_ref` | A-D-030 | A-E-027 | A-R-027 |
| 27 | `Set<String>` | `HashSet<String>` | `V` (untyped list) | `set_string_3` | A-D-026 | A-E-026 | A-R-026 |
| 28 | Custom Enum | `enum` | `I` (ordinal) | `enum_status` | A-D-027 | A-E-014 | — |

### 3.3 边界值测试

在基本类型覆盖之外，针对数值类型、字符串类型和二进制类型补充边界值测试。

**数值类型边界**：

| 类型 | 边界值 | Fixture 文件 | 说明 |
|------|--------|-------------|------|
| `i8` | `-128`, `0`, `127` | `byte_min`, `byte_zero`, `byte_max` | 全范围端点 |
| `i16` | `-32768`, `0`, `32767` | `short_min`, `short_zero`, `short_max` | 全范围端点 |
| `i32` | `-2147483648`, `0`, `2147483647` | `int_min`, `int_zero`, `int_max` | 全范围端点 |
| `i64` | `-9223372036854775808`, `0`, `9223372036854775807` | `long_min`, `long_zero`, `long_max` | 全范围端点 |
| `f32` | `f32::MIN`, `f32::MAX`, `NaN`, `+Inf`, `-Inf`, `-0.0` | `float_min`, `float_max`, `float_nan`, `float_inf`, `float_neg_inf`, `float_neg_zero` | 含特殊浮点值 |
| `f64` | `f64::MIN`, `f64::MAX`, `NaN`, `+Inf`, `-Inf`, `-0.0` | `double_min`, `double_max`, `double_nan`, `double_inf`, `double_neg_inf`, `double_neg_zero` | 含特殊浮点值 |

**字符串类型边界**：

| 场景 | 值 | Fixture 文件 | 说明 |
|------|-----|-------------|------|
| 空串 | `""` | `string_empty` | 已在矩阵中覆盖 |
| 单字节字符 | `"a"` | `string_single_char` | 最短非空 |
| ASCII 长字符串 | 65535 字节 `'a'` | `string_64kb` | 已在矩阵中覆盖 |
| 分块边界 | 65536 字节 (触发 R chunk) | `string_chunk_boundary` | 刚好触发分块 |
| 中文多字节 | `"你好，Dubbo！🎉"` | `string_unicode_mixed` | CJK + emoji |
| null 字符串 | `null` (Java null) | `string_null` | 区别于空串 |

**二进制类型边界**：

| 场景 | 值 | Fixture 文件 | 说明 |
|------|-----|-------------|------|
| 空数组 | `new byte[0]` | `bytes_empty` | 零长度 |
| 单字节 | `new byte[]{0x00}` | `bytes_single` | 最短非空 |
| 1024 字节 | `byte[1024]` 全 0xFF | `bytes_1k` | 中等长度 |
| 65535 字节 | `byte[65535]` | `bytes_64kb` | 短格式上限 |
| 65536 字节 | `byte[65536]` | `bytes_chunk_boundary` | 触发分块 |

### 3.4 具体测试用例

#### Test A-D-001: Decode boolean true

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `boolean`
- **Rust类型**: `bool`
- **Hessian2标签**: `T` (0x54)
- **前置条件**: `tests/fixtures/hessian2/bool_true.bin` + `bool_true.meta.json`
- **输入**: 单字节 `0x54`
- **操作步骤**:
  1. 加载 fixture `bool_true`
  2. 调用 `hessian2::decode(&bytes)` 解码
  3. 将结果 downcast 为 `bool`
- **预期结果**: 解码成功，值为 `true`
- **通过标准**: `assert_eq!(decoded, true)`

#### Test A-D-002: Decode boolean false

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `boolean`
- **Rust类型**: `bool`
- **Hessian2标签**: `F` (0x46)
- **前置条件**: `tests/fixtures/hessian2/bool_false.bin` + `bool_false.meta.json`
- **输入**: 单字节 `0x46`
- **操作步骤**:
  1. 加载 fixture `bool_false`
  2. 调用 `hessian2::decode(&bytes)` 解码
  3. 将结果 downcast 为 `bool`
- **预期结果**: 解码成功，值为 `false`
- **通过标准**: `assert_eq!(decoded, false)`

#### Test A-D-003: Decode byte value 42

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `byte`
- **Rust类型**: `i8`
- **Hessian2标签**: `I` (int 32 编码，Java byte 提升为 int)
- **前置条件**: `tests/fixtures/hessian2/byte_42.bin` + `byte_42.meta.json`
- **输入**: `I` 标签 + int 32 编码的值 `42`，即 `0x49 0x00 0x00 0x00 0x2a`
- **操作步骤**:
  1. 加载 fixture `byte_42`
  2. 调用 `hessian2::decode(&bytes)` 解码
  3. 验证解码值可以安全截断为 `i8`
- **预期结果**: 解码值为整数 `42`，可安全转换为 `i8`
- **通过标准**: `assert_eq!(decoded as i8, 42_i8)`

#### Test A-D-004: Decode boxed Byte (present)

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `java.lang.Byte`
- **Rust类型**: `Option<i8>`
- **Hessian2标签**: `I` (非 null 时)
- **前置条件**: `tests/fixtures/hessian2/byte_boxed_42.bin` + `byte_boxed_42.meta.json`
- **输入**: 与 Test A-D-003 相同的字节 `0x49 0x00 0x00 0x00 0x2a`
- **操作步骤**:
  1. 加载 fixture `byte_boxed_42`
  2. 解码为 `Option<i8>`
  3. 验证为 `Some(42)`
- **预期结果**: `Some(42_i8)`
- **通过标准**: `assert_eq!(decoded, Some(42_i8))`

#### Test A-D-005: Decode short value 12345

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `short`
- **Rust类型**: `i16`
- **Hessian2标签**: `I` (int 32 编码，Java short 提升为 int)
- **前置条件**: `tests/fixtures/hessian2/short_12345.bin` + `short_12345.meta.json`
- **输入**: `I` 标签 + int 32 编码的 `12345`，即 `0x49 0x00 0x00 0x30 0x39`
- **操作步骤**:
  1. 加载 fixture `short_12345`
  2. 解码并验证可安全转换为 `i16`
- **预期结果**: 整数 `12345`，在 `i16` 范围内
- **通过标准**: `assert_eq!(decoded as i16, 12345_i16)`

#### Test A-D-007: Decode int value 100000

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `int`
- **Rust类型**: `i32`
- **Hessian2标签**: `I` (可能为紧凑编码 `0x80`-`0xbf` 或完整 4 字节)
- **前置条件**: `tests/fixtures/hessian2/int_100000.bin` + `int_100000.meta.json`
- **输入**: `100000` 的 Hessian2 int 编码（紧凑或标准格式均可）
- **操作步骤**:
  1. 加载 fixture `int_100000`
  2. 解码为 `i32`
  3. 验证值等于 `100000`
- **预期结果**: `100000_i32`
- **通过标准**: `assert_eq!(decoded, 100000_i32)`

#### Test A-D-009: Decode long value 9876543210

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `long`
- **Rust类型**: `i64`
- **Hessian2标签**: `L` (可能为紧凑编码或完整 8 字节)
- **前置条件**: `tests/fixtures/hessian2/long_9876543210.bin` + `long_9876543210.meta.json`
- **输入**: `9876543210L` 的 Hessian2 long 编码
- **操作步骤**:
  1. 加载 fixture `long_9876543210`
  2. 解码为 `i64`
  3. 验证值等于 `9876543210`
- **预期结果**: `9876543210_i64`
- **通过标准**: `assert_eq!(decoded, 9876543210_i64)`

#### Test A-D-011: Decode float 3.14f (promoted to double)

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `float`
- **Rust类型**: `f32`
- **Hessian2标签**: `D` (double 编码，Java float 提升为 double)
- **前置条件**: `tests/fixtures/hessian2/float_3_14f.bin` + `float_3_14f.meta.json`
- **输入**: `3.14f` 的 Hessian2 double 编码 `0x44 0x40 0x09 0x1e 0xb8 0x51 0xeb 0x85 0x1f`
- **操作步骤**:
  1. 加载 fixture `float_3_14f`
  2. 解码为 `f64`
  3. 验证可以安全转换为 `f32`，且与 `(3.14_f32 as f64)` 语义等价
- **预期结果**: 解码的 double 值经 `f32` 舍入后等于 `3.14f`
- **通过标准**: `assert!((decoded as f32 - 3.14_f32).abs() < f32::EPSILON)`

#### Test A-D-013: Decode double 3.14159265358979

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `double`
- **Rust类型**: `f64`
- **Hessian2标签**: `D`
- **前置条件**: `tests/fixtures/hessian2/double_3_14159265358979.bin` + `double_3_14159265358979.meta.json`
- **输入**: `3.14159265358979` 的 Hessian2 double 编码
- **操作步骤**:
  1. 加载 fixture `double_3_14159265358979`
  2. 解码为 `f64`
  3. 验证值与 `3.14159265358979_f64` 在 IEEE 754 精度范围内等价
- **预期结果**: `3.14159265358979_f64`
- **通过标准**: `assert!((decoded - 3.14159265358979_f64).abs() < f64::EPSILON)`

#### Test A-D-015: Decode String "Hello, Dubbo!"

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `java.lang.String`
- **Rust类型**: `String`
- **Hessian2标签**: `S` (短字符串格式)
- **前置条件**: `tests/fixtures/hessian2/string_hello.bin` + `string_hello.meta.json`
- **输入**: `"Hello, Dubbo!"` 的 Hessian2 String 编码（长度 13，使用短格式 `0x0d` + UTF-8 字节）
- **操作步骤**:
  1. 加载 fixture `string_hello`
  2. 解码为 `String`
  3. 验证内容
- **预期结果**: `"Hello, Dubbo!"`
- **通过标准**: `assert_eq!(decoded, "Hello, Dubbo!")`

#### Test A-D-017: Decode 64KB String (chunked)

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `java.lang.String`
- **Rust类型**: `String`
- **Hessian2标签**: `R` (分块编码)
- **前置条件**: `tests/fixtures/hessian2/string_64kb.bin` + `string_64kb.meta.json`
- **输入**: 65535 字节 ASCII 字符串的 Hessian2 分块编码，使用 `R` 标签
- **操作步骤**:
  1. 加载 fixture `string_64kb`
  2. 解码为 `String`
  3. 验证长度和内容（所有字符为 `'a'`）
- **预期结果**: 长度为 65535 的字符串，每个字符均为 `'a'`
- **通过标准**: `assert_eq!(decoded.len(), 65535)` 且 `assert!(decoded.chars().all(|c| c == 'a'))`

#### Test A-D-018: Decode byte array

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `byte[]`
- **Rust类型**: `Vec<u8>`
- **Hessian2标签**: `B`
- **前置条件**: `tests/fixtures/hessian2/bytes_short.bin` + `bytes_short.meta.json`
- **输入**: `[0x00, 0x01, 0x02, ..., 0x0f]` 的 Hessian2 binary 编码（16 字节）
- **操作步骤**:
  1. 加载 fixture `bytes_short`
  2. 解码为 `Vec<u8>`
  3. 验证长度和每个字节
- **预期结果**: 16 字节的 `Vec<u8>`，值为 `0x00` 到 `0x0f`
- **通过标准**: `assert_eq!(decoded.len(), 16)` 且 `assert_eq!(decoded[5], 0x05)`

#### Test A-D-019: Decode java.util.Date

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `java.util.Date`
- **Rust类型**: `JavaDate(i64)`
- **Hessian2标签**: `d`
- **前置条件**: `tests/fixtures/hessian2/date_20260101.bin` + `date_20260101.meta.json`
- **输入**: `2026-01-01T00:00:00Z` 对应的毫秒时间戳的 Hessian2 Date 编码
- **操作步骤**:
  1. 加载 fixture `date_20260101`
  2. 解码为 `JavaDate(i64)`
  3. 验证时间戳对应 `2026-01-01T00:00:00Z`
- **预期结果**: `JavaDate(1767225600000_i64)` (UTC 2026-01-01 零点的毫秒时间戳)
- **通过标准**: `assert_eq!(decoded.0, 1767225600000_i64)`

#### Test A-D-020: Decode java.math.BigDecimal

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `java.math.BigDecimal`
- **Rust类型**: `BigDecimal`
- **Hessian2标签**: map def (通过自定义 map 结构编码)
- **前置条件**: `tests/fixtures/hessian2/bigdecimal_pi.bin` + `bigdecimal_pi.meta.json`
- **输入**: `BigDecimal("3.14159265358979323846")` 的 Hessian2 编码
- **操作步骤**:
  1. 加载 fixture `bigdecimal_pi`
  2. 解码为 `BigDecimal`
  3. 验证值与 `3.14159265358979323846` 精确匹配
- **预期结果**: `BigDecimal` 表示的 `3.14159265358979323846`
- **通过标准**: `assert_eq!(decoded.to_string(), "3.14159265358979323846")`

#### Test A-D-021: Decode List\<String\>

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `java.util.List<String>`
- **Rust类型**: `Vec<String>`
- **Hessian2标签**: `V`
- **前置条件**: `tests/fixtures/hessian2/list_string_3.bin` + `list_string_3.meta.json`
- **输入**: `["hello", "dubbo", "rust"]` 的 Hessian2 list 编码
- **操作步骤**:
  1. 加载 fixture `list_string_3`
  2. 解码为 `Vec<String>`
  3. 验证长度和每个元素
- **预期结果**: `vec!["hello".to_string(), "dubbo".to_string(), "rust".to_string()]`
- **通过标准**: `assert_eq!(decoded, vec!["hello", "dubbo", "rust"])`

#### Test A-D-022: Decode Map\<String, String\>

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `java.util.Map<String,String>`
- **Rust类型**: `HashMap<String,String>`
- **Hessian2标签**: `M`
- **前置条件**: `tests/fixtures/hessian2/map_ss_3.bin` + `map_ss_3.meta.json`
- **输入**: `{"key1": "val1", "key2": "val2", "key3": "val3"}` 的 Hessian2 map 编码
- **操作步骤**:
  1. 加载 fixture `map_ss_3`
  2. 解码为 `HashMap<String, String>`
  3. 验证三个键值对
- **预期结果**: 包含 `"key1"→"val1"`, `"key2"→"val2"`, `"key3"→"val3"` 的 HashMap
- **通过标准**: `assert_eq!(decoded.len(), 3)` 且 `assert_eq!(decoded["key1"], "val1")`

#### Test A-D-023: Decode null

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `null`
- **Rust类型**: `Option::None`
- **Hessian2标签**: `N` (0x4e)
- **前置条件**: `tests/fixtures/hessian2/null_value.bin` + `null_value.meta.json`
- **输入**: 单字节 `0x4e`
- **操作步骤**:
  1. 加载 fixture `null_value`
  2. 解码
  3. 验证为 null/None
- **预期结果**: 解码结果为 null/None
- **通过标准**: `assert!(decoded.is_none())`

#### Test A-D-024: Decode Throwable (RuntimeException)

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `Throwable` (`java.lang.RuntimeException`)
- **Rust类型**: `JavaException`
- **Hessian2标签**: map def (通过 map 结构编码，包含类名、message、stackTrace)
- **前置条件**: `tests/fixtures/hessian2/throwable_runtime.bin` + `throwable_runtime.meta.json`
- **输入**: `new RuntimeException("test error")` 的 Hessian2 序列化
- **操作步骤**:
  1. 加载 fixture `throwable_runtime`
  2. 解码为 `JavaException`
  3. 验证异常类名和 message
- **预期结果**: `JavaException` 包含类名 `java.lang.RuntimeException`，message 为 `"test error"`
- **通过标准**: `assert!(decoded.class_name.contains("RuntimeException"))` 且 `assert_eq!(decoded.message, "test error")`

#### Test A-D-025: Decode Custom POJO

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `CustomPojo` (自定义 Bean)
- **Rust类型**: `struct` (反序列化为通用的 Map 或对应的 Rust struct)
- **Hessian2标签**: `C` (class def) + `O` (object instance)
- **前置条件**: `tests/fixtures/hessian2/pojo_custom.bin` + `pojo_custom.meta.json`
- **输入**: `new CustomPojo(42, "test", ["a","b"])` 的 Hessian2 编码
- **操作步骤**:
  1. 加载 fixture `pojo_custom`
  2. 解码
  3. 验证字段 `id=42`, `name="test"`, `tags=["a","b"]`
- **预期结果**: 包含正确字段值的对象
- **通过标准**: `assert_eq!(decoded["id"], 42)` 且 `assert_eq!(decoded["name"], "test")`

#### Test A-D-030: Decode Circular Reference

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: 自引用对象
- **Rust类型**: 带回指的图结构
- **Hessian2标签**: `R` (reference，引用之前已编码的对象)
- **前置条件**: `tests/fixtures/hessian2/circular_ref.bin` + `circular_ref.meta.json`
- **输入**: 一个 `Node` 对象，其 `next` 字段指向自身的循环引用编码
- **操作步骤**:
  1. 加载 fixture `circular_ref`
  2. 解码
  3. 验证解码不崩溃，且引用关系正确建立
- **预期结果**: 成功解码，对象的 `next` 指针指向自身（或等价的循环结构）
- **通过标准**: 解码不 panic，且 `decoded.next` 引用指向 `decoded` 自身

#### Test A-D-026: Decode Set\<String\>

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: `java.util.Set<String>` (通常序列化为 `HashSet`)
- **Rust类型**: `HashSet<String>`
- **Hessian2标签**: `V` (untyped list，Java Set 在 Hessian2 中编码为 list)
- **前置条件**: `tests/fixtures/hessian2/set_string_3.bin` + `set_string_3.meta.json`
- **输入**: `["alpha", "beta", "gamma"]` 的 Hessian2 list 编码（Set 序列化为 list）
- **操作步骤**:
  1. 加载 fixture `set_string_3`
  2. 解码为 `HashSet<String>`
  3. 验证包含全部三个元素
- **预期结果**: 包含 `"alpha"`, `"beta"`, `"gamma"` 的 HashSet
- **通过标准**: `assert_eq!(decoded.len(), 3)` 且 `assert!(decoded.contains("alpha"))`

#### Test A-D-027: Decode Custom Enum

- **领域**: A (序列化)
- **分类**: Decode
- **Java类型**: 自定义 Enum（如 `Status { ACTIVE, INACTIVE }`）
- **Rust类型**: `enum` 或 `String`（根据 Hessian2 实际编码策略）
- **Hessian2标签**: `I` (ordinal) 或 `S` (name)，取决于 Java 实现
- **前置条件**: `tests/fixtures/hessian2/enum_status.bin` + `enum_status.meta.json`
- **输入**: `Status.ACTIVE` 的 Hessian2 编码
- **操作步骤**:
  1. 加载 fixture `enum_status`
  2. 解码
  3. 验证值对应 `ACTIVE`
- **预期结果**: 解码值可映射为 `ACTIVE` 状态
- **通过标准**: 解码不 panic，值与 `.meta.json` 中记录的 `value_description` 一致

#### Test A-D-028: Decode malformed bytes — truncated stream (negative test)

- **领域**: A (序列化)
- **分类**: Decode (negative)
- **优先级**: P2
- **前置条件**: 手工构造截断字节流
- **输入**: 合法 int 编码的前 3 字节 `0x49 0x00 0x00`（截断，缺少后 2 字节）
- **操作步骤**:
  1. 构造截断字节 `[0x49, 0x00, 0x00]`
  2. 调用 `hessian2::decode(&bytes)`
  3. 验证返回错误（非 panic）
- **预期结果**: 解码返回 `Err`，错误信息包含截断/不足相关描述
- **通过标准**: `assert!(result.is_err())`，进程不 panic

#### Test A-D-029: Decode malformed bytes — illegal tag 0xFF (negative test)

- **领域**: A (序列化)
- **分类**: Decode (negative)
- **优先级**: P2
- **前置条件**: 手工构造非法 tag 字节
- **输入**: 单字节 `0xFF`（不属于任何 Hessian2 合法 tag）
- **操作步骤**:
  1. 构造字节 `[0xFF]`
  2. 调用 `hessian2::decode(&bytes)`
  3. 验证返回错误（非 panic）
- **预期结果**: 解码返回 `Err`，错误信息包含未知/非法 tag 相关描述
- **通过标准**: `assert!(result.is_err())`，进程不 panic

#### Test A-E-001: Encode boolean true

- **领域**: A (序列化)
- **分类**: Encode
- **Java类型**: `boolean`
- **Rust类型**: `bool`
- **Hessian2标签**: `T` (0x54)
- **前置条件**: `tests/fixtures/hessian2/bool_true.bin` + `bool_true.meta.json`
- **输入**: Rust 值 `true`
- **操作步骤**:
  1. 调用 `hessian2::encode(&true)` 编码
  2. 获取编码字节
  3. 与 Java fixture `bool_true.bin` 做字节级比对
- **预期结果**: 编码输出为单字节 `0x54`
- **通过标准**: `assert_bytes_match(&fixture_bytes, &encoded_bytes)` (严格模式)

#### Test A-E-007: Encode int 100000

- **领域**: A (序列化)
- **分类**: Encode
- **Java类型**: `int`
- **Rust类型**: `i32`
- **Hessian2标签**: `I` (紧凑或标准格式)
- **前置条件**: `tests/fixtures/hessian2/int_100000.bin` + `int_100000.meta.json`
- **输入**: Rust 值 `100000_i32`
- **操作步骤**:
  1. 调用 `hessian2::encode(&100000_i32)` 编码
  2. 获取编码字节
  3. 将编码字节用 Java Hessian2 解码器验证（如可用），或与 fixture 比对
- **预期结果**: 编码字节可被 Java Hessian2 正确解码为 `100000`
- **通过标准**: 编码输出为合法 Hessian2 int 编码（不要求与 fixture 字节完全一致）

#### Test A-E-013: Encode double 3.14159265358979

- **领域**: A (序列化)
- **分类**: Encode
- **Java类型**: `double`
- **Rust类型**: `f64`
- **Hessian2标签**: `D`
- **前置条件**: `tests/fixtures/hessian2/double_3_14159265358979.bin` + `double_3_14159265358979.meta.json`
- **输入**: Rust 值 `3.14159265358979_f64`
- **操作步骤**:
  1. 调用 `hessian2::encode(&3.14159265358979_f64)` 编码
  2. 获取编码字节
  3. 与 Java fixture 做字节级比对
- **预期结果**: 编码字节以 `0x44` 开头，后跟 IEEE 754 big-endian 8 字节
- **通过标准**: `assert_bytes_match(&fixture_bytes, &encoded_bytes)` (严格模式)

#### Test A-E-026: Encode Set\<String\>

- **领域**: A (序列化)
- **分类**: Encode
- **Java类型**: `java.util.Set<String>`
- **Rust类型**: `HashSet<String>`
- **Hessian2标签**: `V` (untyped list)
- **前置条件**: `tests/fixtures/hessian2/set_string_3.bin` + `set_string_3.meta.json`
- **输入**: Rust `HashSet` 包含 `"alpha"`, `"beta"`, `"gamma"`
- **操作步骤**:
  1. 调用 `hessian2::encode(&set)` 编码
  2. 获取编码字节
  3. 与 Java fixture `set_string_3.bin` 做比对（归一化后，因为 Set 无序）
- **预期结果**: 编码为合法 Hessian2 list，包含全部 3 个字符串元素
- **通过标准**: 解码编码字节后得到的集合与原始 Set 语义等价

#### Test A-E-014: Encode Custom Enum

- **领域**: A (序列化)
- **分类**: Encode
- **Java类型**: 自定义 Enum (`Status`)
- **Rust类型**: `enum` 或 `String`
- **Hessian2标签**: 取决于 Java 实现（`I` ordinal 或 `S` name）
- **前置条件**: `tests/fixtures/hessian2/enum_status.bin` + `enum_status.meta.json`
- **输入**: Rust enum variant 对应 `Status.ACTIVE`
- **操作步骤**:
  1. 调用 `hessian2::encode` 编码 enum 值
  2. 与 Java fixture 比对
- **预期结果**: 编码字节可被 Java Hessian2 正确解码为对应 enum 值
- **通过标准**: 编码输出为合法 Hessian2 编码（不要求与 fixture 字节完全一致）

#### Test A-R-001: Roundtrip boolean true

- **领域**: A (序列化)
- **分类**: Roundtrip
- **Java类型**: `boolean`
- **Rust类型**: `bool`
- **Hessian2标签**: `T`
- **前置条件**: `tests/fixtures/hessian2/bool_true.bin` + `bool_true.meta.json`
- **输入**: fixture 字节 `0x54`
- **操作步骤**:
  1. 加载 fixture `bool_true`，解码为 `bool`，得到 `true`
  2. 将 `true` 重新编码
  3. 将重编码的字节再次解码
  4. 比对两次解码结果
- **预期结果**: 两次解码结果均为 `true`
- **通过标准**: `assert_eq!(first_decode, second_decode)`

#### Test A-R-007: Roundtrip int 100000

- **领域**: A (序列化)
- **分类**: Roundtrip
- **Java类型**: `int`
- **Rust类型**: `i32`
- **Hessian2标签**: `I`
- **前置条件**: `tests/fixtures/hessian2/int_100000.bin` + `int_100000.meta.json`
- **输入**: `100000` 的 Hessian2 int 编码字节
- **操作步骤**:
  1. 加载 fixture，解码为 `i32`，得到 `100000`
  2. 将 `100000` 重新编码
  3. 将重编码字节再次解码
  4. 比对两次解码结果
- **预期结果**: 两次解码均为 `100000_i32`
- **通过标准**: `assert_eq!(first_decode, 100000_i32)` 且 `assert_eq!(first_decode, second_decode)`

#### Test A-R-009: Roundtrip long 9876543210

- **领域**: A (序列化)
- **分类**: Roundtrip
- **Java类型**: `long`
- **Rust类型**: `i64`
- **Hessian2标签**: `L`
- **前置条件**: `tests/fixtures/hessian2/long_9876543210.bin` + `long_9876543210.meta.json`
- **输入**: `9876543210` 的 Hessian2 long 编码字节
- **操作步骤**:
  1. 加载 fixture，解码为 `i64`，得到 `9876543210`
  2. 重编码后再次解码
  3. 比对结果
- **预期结果**: 两次解码均为 `9876543210_i64`
- **通过标准**: `assert_eq!(first_decode, second_decode)`

#### Test A-R-015: Roundtrip String "Hello, Dubbo!"

- **领域**: A (序列化)
- **分类**: Roundtrip
- **Java类型**: `java.lang.String`
- **Rust类型**: `String`
- **Hessian2标签**: `S`
- **前置条件**: `tests/fixtures/hessian2/string_hello.bin` + `string_hello.meta.json`
- **输入**: `"Hello, Dubbo!"` 的 Hessian2 String 编码字节
- **操作步骤**:
  1. 加载 fixture，解码为 `String`，得到 `"Hello, Dubbo!"`
  2. 重编码后再次解码
  3. 比对结果
- **预期结果**: 两次解码均为 `"Hello, Dubbo!"`
- **通过标准**: `assert_eq!(first_decode, second_decode)`

#### Test A-R-017: Roundtrip 64KB String

- **领域**: A (序列化)
- **分类**: Roundtrip
- **Java类型**: `java.lang.String`
- **Rust类型**: `String`
- **Hessian2标签**: `R` (分块)
- **前置条件**: `tests/fixtures/hessian2/string_64kb.bin` + `string_64kb.meta.json`
- **输入**: 65535 字节字符串的 Hessian2 分块编码
- **操作步骤**:
  1. 加载 fixture，解码为 `String`
  2. 重编码后再次解码
  3. 比对结果长度和内容
- **预期结果**: 两次解码的字符串长度均为 65535，内容一致
- **通过标准**: `assert_eq!(first_decode.len(), 65535)` 且 `assert_eq!(first_decode, second_decode)`

#### Test A-R-021: Roundtrip List\<String\>

- **领域**: A (序列化)
- **分类**: Roundtrip
- **Java类型**: `java.util.List<String>`
- **Rust类型**: `Vec<String>`
- **Hessian2标签**: `V`
- **前置条件**: `tests/fixtures/hessian2/list_string_3.bin` + `list_string_3.meta.json`
- **输入**: `["hello", "dubbo", "rust"]` 的 Hessian2 list 编码
- **操作步骤**:
  1. 加载 fixture，解码为 `Vec<String>`
  2. 重编码后再次解码
  3. 逐元素比对
- **预期结果**: 两次解码均为 `vec!["hello", "dubbo", "rust"]`
- **通过标准**: `assert_eq!(first_decode, second_decode)`

#### Test A-R-023: Roundtrip null

- **领域**: A (序列化)
- **分类**: Roundtrip
- **Java类型**: `null`
- **Rust类型**: `Option::None`
- **Hessian2标签**: `N`
- **前置条件**: `tests/fixtures/hessian2/null_value.bin` + `null_value.meta.json`
- **输入**: 单字节 `0x4e`
- **操作步骤**:
  1. 加载 fixture，解码，确认为 None
  2. 将 None 重新编码
  3. 重编码字节应为 `0x4e`
  4. 再次解码确认为 None
- **预期结果**: 两次解码均为 None，重编码字节为 `0x4e`
- **通过标准**: `assert!(first_decode.is_none())` 且 `assert_bytes_match(&[0x4e], &encoded)`

#### Test A-R-025: Roundtrip Custom POJO

- **领域**: A (序列化)
- **分类**: Roundtrip
- **Java类型**: `CustomPojo`
- **Rust类型**: `struct`
- **Hessian2标签**: `C` + `O`
- **前置条件**: `tests/fixtures/hessian2/pojo_custom.bin` + `pojo_custom.meta.json`
- **输入**: `{id: 42, name: "test", tags: ["a","b"]}` 的 Hessian2 编码
- **操作步骤**:
  1. 加载 fixture，解码
  2. 验证字段值
  3. 重编码后再次解码
  4. 逐字段比对
- **预期结果**: 两次解码的字段值一致：`id=42`, `name="test"`, `tags=["a","b"]`
- **通过标准**: 所有字段值在两次解码间完全匹配

#### Test A-R-026: Roundtrip Set\<String\>

- **领域**: A (序列化)
- **分类**: Roundtrip
- **Java类型**: `java.util.Set<String>`
- **Rust类型**: `HashSet<String>`
- **Hessian2标签**: `V`
- **前置条件**: `tests/fixtures/hessian2/set_string_3.bin` + `set_string_3.meta.json`
- **输入**: `["alpha", "beta", "gamma"]` 的 Hessian2 list 编码
- **操作步骤**:
  1. 加载 fixture，解码为 `HashSet<String>`
  2. 重编码后再次解码
  3. 比对两次解码结果（集合语义等价）
- **预期结果**: 两次解码的集合包含相同的全部元素
- **通过标准**: `assert_eq!(first_decode.len(), second_decode.len())` 且 `first_decode == second_decode`

#### Test A-BND-001: Decode int MIN (boundary)

- **领域**: A (序列化)
- **分类**: Decode (边界值)
- **Java类型**: `int`
- **Rust类型**: `i32`
- **Hessian2标签**: `I`
- **前置条件**: `tests/fixtures/hessian2/int_min.bin` + `int_min.meta.json`
- **输入**: `-2147483648` 的 Hessian2 int 编码，即 `0x49 0x80 0x00 0x00 0x00`
- **操作步骤**:
  1. 加载 fixture `int_min`
  2. 解码为 `i32`
  3. 验证值
- **预期结果**: `i32::MIN` = `-2147483648`
- **通过标准**: `assert_eq!(decoded, i32::MIN)`

#### Test A-BND-002: Decode int MAX (boundary)

- **领域**: A (序列化)
- **分类**: Decode (边界值)
- **Java类型**: `int`
- **Rust类型**: `i32`
- **Hessian2标签**: `I`
- **前置条件**: `tests/fixtures/hessian2/int_max.bin` + `int_max.meta.json`
- **输入**: `2147483647` 的 Hessian2 int 编码，即 `0x49 0x7f 0xff 0xff 0xff`
- **操作步骤**:
  1. 加载 fixture `int_max`
  2. 解码为 `i32`
  3. 验证值
- **预期结果**: `i32::MAX` = `2147483647`
- **通过标准**: `assert_eq!(decoded, i32::MAX)`

#### Test A-BND-003: Decode long MIN (boundary)

- **领域**: A (序列化)
- **分类**: Decode (边界值)
- **Java类型**: `long`
- **Rust类型**: `i64`
- **Hessian2标签**: `L`
- **前置条件**: `tests/fixtures/hessian2/long_min.bin` + `long_min.meta.json`
- **输入**: `-9223372036854775808` 的 Hessian2 long 编码
- **操作步骤**:
  1. 加载 fixture `long_min`
  2. 解码为 `i64`
- **预期结果**: `i64::MIN` = `-9223372036854775808`
- **通过标准**: `assert_eq!(decoded, i64::MIN)`

#### Test A-BND-004: Decode long MAX (boundary)

- **领域**: A (序列化)
- **分类**: Decode (边界值)
- **Java类型**: `long`
- **Rust类型**: `i64`
- **Hessian2标签**: `L`
- **前置条件**: `tests/fixtures/hessian2/long_max.bin` + `long_max.meta.json`
- **输入**: `9223372036854775807` 的 Hessian2 long 编码
- **操作步骤**:
  1. 加载 fixture `long_max`
  2. 解码为 `i64`
- **预期结果**: `i64::MAX` = `9223372036854775807`
- **通过标准**: `assert_eq!(decoded, i64::MAX)`

#### Test A-BND-005: Decode double NaN (boundary)

- **领域**: A (序列化)
- **分类**: Decode (边界值)
- **Java类型**: `double`
- **Rust类型**: `f64`
- **Hessian2标签**: `D`
- **前置条件**: `tests/fixtures/hessian2/double_nan.bin` + `double_nan.meta.json`
- **输入**: `Double.NaN` 的 Hessian2 double 编码，即 `0x44 0x7f 0xf8 0x00 0x00 0x00 0x00 0x00 0x00`
- **操作步骤**:
  1. 加载 fixture `double_nan`
  2. 解码为 `f64`
  3. 验证 `is_nan()` 而非 `== NaN`（NaN 不等于自身）
- **预期结果**: `f64::is_nan() == true`
- **通过标准**: `assert!(decoded.is_nan())`

#### Test A-BND-006: Decode double +Infinity (boundary)

- **领域**: A (序列化)
- **分类**: Decode (边界值)
- **Java类型**: `double`
- **Rust类型**: `f64`
- **Hessian2标签**: `D`
- **前置条件**: `tests/fixtures/hessian2/double_inf.bin` + `double_inf.meta.json`
- **输入**: `Double.POSITIVE_INFINITY` 的 Hessian2 编码，即 `0x44 0x7f 0xf0 0x00 0x00 0x00 0x00 0x00 0x00`
- **操作步骤**:
  1. 加载 fixture `double_inf`
  2. 解码为 `f64`
  3. 验证为正无穷
- **预期结果**: `f64::INFINITY`
- **通过标准**: `assert!(decoded.is_infinite() && decoded.is_sign_positive())`

#### Test A-BND-007: Decode double -Infinity (boundary)

- **领域**: A (序列化)
- **分类**: Decode (边界值)
- **Java类型**: `double`
- **Rust类型**: `f64`
- **Hessian2标签**: `D`
- **前置条件**: `tests/fixtures/hessian2/double_neg_inf.bin` + `double_neg_inf.meta.json`
- **输入**: `Double.NEGATIVE_INFINITY` 的 Hessian2 编码，即 `0x44 0xff 0xf0 0x00 0x00 0x00 0x00 0x00 0x00`
- **操作步骤**:
  1. 加载 fixture `double_neg_inf`
  2. 解码为 `f64`
  3. 验证为负无穷
- **预期结果**: `f64::NEG_INFINITY`
- **通过标准**: `assert!(decoded.is_infinite() && decoded.is_sign_negative())`

#### Test A-BND-008: Decode double -0.0 (boundary)

- **领域**: A (序列化)
- **分类**: Decode (边界值)
- **Java类型**: `double`
- **Rust类型**: `f64`
- **Hessian2标签**: `D`
- **前置条件**: `tests/fixtures/hessian2/double_neg_zero.bin` + `double_neg_zero.meta.json`
- **输入**: `-0.0` 的 Hessian2 double 编码，即 `0x44 0x80 0x00 0x00 0x00 0x00 0x00 0x00 0x00`
- **操作步骤**:
  1. 加载 fixture `double_neg_zero`
  2. 解码为 `f64`
  3. 验证值为 `-0.0`，且符号位为负
- **预期结果**: `-0.0_f64`，`is_sign_negative() == true`
- **通过标准**: `assert!(decoded == 0.0 && decoded.is_sign_negative())`

#### Test A-BND-009: Decode String with mixed Unicode

- **领域**: A (序列化)
- **分类**: Decode (边界值)
- **Java类型**: `java.lang.String`
- **Rust类型**: `String`
- **Hessian2标签**: `S`
- **前置条件**: `tests/fixtures/hessian2/string_unicode_mixed.bin` + `string_unicode_mixed.meta.json`
- **输入**: `"你好，Dubbo！🎉"` 的 Hessian2 String 编码（含 CJK 字符和 emoji）
- **操作步骤**:
  1. 加载 fixture `string_unicode_mixed`
  2. 解码为 `String`
  3. 验证内容
- **预期结果**: `"你好，Dubbo！🎉"`
- **通过标准**: `assert_eq!(decoded, "你好，Dubbo！🎉")`

#### Test A-BND-010: Decode byte[] chunk boundary (65536 bytes)

- **领域**: A (序列化)
- **分类**: Decode (边界值)
- **Java类型**: `byte[]`
- **Rust类型**: `Vec<u8>`
- **Hessian2标签**: `B` (可能触发分块)
- **前置条件**: `tests/fixtures/hessian2/bytes_chunk_boundary.bin` + `bytes_chunk_boundary.meta.json`
- **输入**: 65536 字节数组的 Hessian2 binary 编码（超过短格式上限，触发分块）
- **操作步骤**:
  1. 加载 fixture `bytes_chunk_boundary`
  2. 解码为 `Vec<u8>`
  3. 验证长度
- **预期结果**: 长度为 65536 的 `Vec<u8>`
- **通过标准**: `assert_eq!(decoded.len(), 65536)`

---

## 附录 A: Hessian2 数据桩文件目录

| 文件名 | Java 类型 | Rust 类型 | 值 | 预期 Hex 摘要 |
|--------|-----------|-----------|-----|---------------|
| `bool_true.bin` | `boolean` | `bool` | `true` | `54` |
| `bool_false.bin` | `boolean` | `bool` | `false` | `46` |
| `int_0.bin` | `int` | `i32` | `0` | `90` |
| `int_1.bin` | `int` | `i32` | `1` | `91` |
| `int_minus1.bin` | `int` | `i32` | `-1` | `8f` |
| `int_42.bin` | `int` | `i32` | `42` | `ba` |
| `int_256.bin` | `int` | `i32` | `256` | `c9 01 00` |
| `int_max.bin` | `int` | `i32` | `2147483647` (2^31 - 1) | `49 7f ff ff ff` |
| `int_min.bin` | `int` | `i32` | `-2147483648` (−2^31) | `49 80 00 00 00` |
| `long_0.bin` | `long` | `i64` | `0L` | `e0` |
| `long_42.bin` | `long` | `i64` | `42L` | `ea` |
| `long_max.bin` | `long` | `i64` | `9223372036854775807L` (2^63 - 1) | `4c 7f ff ff ff ff ff ff ff` |
| `double_0.bin` | `double` | `f64` | `0.0` | `5b` |
| `double_1.bin` | `double` | `f64` | `1.0` | `5c` |
| `double_314.bin` | `double` | `f64` | `3.14` | `47 40 09 1e b8 51 eb 85 1f` |
| `double_minus1.bin` | `double` | `f64` | `-1.0` | `47 bf f0 00 00 00 00 00 00` |
| `double_max.bin` | `double` | `f64` | `1.7976931348623157e308` | `47 7f ef ff ff ff ff ff ff` |
| `float_0.bin` | `float` | `f32` | `0.0f` | `5b` |
| `float_314.bin` | `float` | `f32` | `3.14f` | `47 40 48 f5 c3` |
| `null.bin` | `null` | `Option::None` | `null` | `4e` |
| `string_empty.bin` | `String` | `String` | `""` | `53 00 00` |
| `string_hello.bin` | `String` | `String` | `"Hello, Dubbo!"` | `53 00 0d 48 65 6c 6c 6f 2c 20 44 75 62 62 6f 21` |
| `string_utf8.bin` | `String` | `String` | `"中文测试"` | `53 00 0c e4 b8 ad e6 96 87 e6 b5 8b e8 af 95` |
| `string_long.bin` | `String` | `String` | 1024 个 `a` | `53 04 00` + 1024 × `61` |
| `date_now.bin` | `Date` | `SystemTime` | `2025-01-15T08:30:00.000Z` (固定值) | `4a 00 00 01 93 98 56 38 80` |
| `bytes.bin` | `byte[]` | `Vec<u8>` | `[0x00, 0x01, 0x02, ..., 0x0f]` (16 bytes) | `42 10` + `00 01 ... 0f` |
| `list.bin` | `ArrayList` | `Vec<String>` | `["a", "b", "c"]` | `56 0a [string "a"] [string "b"] [string "c"]` |
| `map.bin` | `HashMap` | `HashMap<String, String>` | `{"key1": "val1", "key2": "val2"}` | `4d [string "key1"] [string "val1"] [string "key2"] [string "val2"] 7a` |
| `user.bin` | `User` (自定义) | `struct User` | `User{name:"alice", age:30}` | `4f [type def] [field values]` |
| `nested_generic.bin` | `List<Map<String, User>>` | `Vec<HashMap<String, User>>` | `[{"u1": User{name:"bob", age:25}}]` | `56 ... (complex)` |

> 注: 预期 Hex 摘要基于 Hessian 2.0 Serialization Specification 的 compact encoding 规则。实际值以 `Hessian2ReferenceEncoder.java` 生成的 fixture 文件为准。`...` 表示较长字节序列的省略。

---

## 附录 C: Hessian2 Compact 编码规整化规则

### C.1 问题背景

Hessian 2.0 规范允许许多类型使用 compact encoding 或 full encoding 两种合法格式。例如，整数 `0` 可以编码为单字节 `0x90`（compact），也可以编码为 5 字节 `0x49 0x00 0x00 0x00 0x00`（full I32 format）。两种编码在语义上完全等价，但字节序列不同。

这给跨语言测试带来了挑战: dubbo-rs 可能使用 compact encoding，而 Java Dubbo 使用 full encoding（或反过来）。直接逐字节比对会导致误报。我们需要一套规整化规则，在比对前将两种编码归一化为可比较的形式。

### C.2 规整化算法

以下是各类型的规整化策略:

```
规整化算法 normalize(bytes: &[u8]) -> Vec<u8>:

  读取第一个字节 tag:
  
  if tag ∈ [0x80..0xBF]:
    // 这是 compact int: value = tag - 0x90
    // 归一化为 full I32 encoding
    value = tag as i32 - 0x90
    return [0x49] + value.to_be_bytes()
  
  if tag == 0x49:
    // 已经是 full I32 encoding，不需要转换
    return bytes[0..5]
  
  if tag ∈ [0xD8..0xEF]:
    // 这是 compact long 的单字节形式
    value = tag as i64 - 0xE0
    return [0x4C] + value.to_be_bytes()
  
  if tag == 0x59:
    // compact long: 3 字节 (tag + 2 bytes)
    value = ((bytes[0] as i64 - 0xE0) << 8) | (bytes[1] as i64)
    return [0x4C] + value.to_be_bytes()
  
  if tag == 0x4C:
    // 已经是 full I64 encoding
    return bytes[0..9]
  
  if tag == 0x5B:
    // compact double: 0.0
    return [0x47] + 0.0f64.to_be_bytes()
  
  if tag == 0x5C:
    // compact double: 1.0
    return [0x47] + 1.0f64.to_be_bytes()
  
  if tag == 0x5D:
    // compact double: short integer representation (3 bytes)
    value = ((bytes[1] as i32) << 8) | (bytes[2] as i32)
    return [0x47] + (value as f64).to_be_bytes()
  
  if tag == 0x5E:
    // compact double: 4-byte float representation
    value = f32::from_be_bytes(bytes[1..5])
    return [0x47] + (value as f64).to_be_bytes()
  
  if tag == 0x47:
    // 已经是 full double encoding (9 bytes total)
    return bytes[0..9]
  
  // 其他类型 (String, List, Map, Object 等) 无 compact/full 区分
  // 直接返回原始字节
  return bytes
```

### C.3 比对策略

在实际测试中，采用以下三级比对策略:

**Level 1: 精确比对**
直接逐字节比较。适用于不存在 compact/full 歧义的类型（String、List、Map、Object）。也适用于双方使用相同编码策略的情况。

**Level 2: 规整化后比对**
对整数（int、long）和浮点数（double、float）应用上述规整化算法，将双方编码归一化为 full encoding 后再比对。这是最常用的策略。

**Level 3: 语义等价比对**
解码双方的字节流，得到语义层面的值，直接比对值是否相等。这是最宽松的策略，作为 Level 2 的 fallback。

### C.4 Compact 与 Full 编码对照表

| 类型 | 值 | Compact 编码 | Full 编码 | 字节差 |
|------|-----|-------------|-----------|--------|
| int | `0` | `90` (1B) | `49 00 00 00 00` (5B) | −4B |
| int | `1` | `91` (1B) | `49 00 00 00 01` (5B) | −4B |
| int | `-16` | `80` (1B) | `49 FF FF FF F0` (5B) | −4B |
| int | `42` | `BA` (1B) | `49 00 00 00 2A` (5B) | −4B |
| int | `256` | `C9 01 00` (3B) | `49 00 00 01 00` (5B) | −2B |
| long | `0L` | `E0` (1B) | `4C 00 00 00 00 00 00 00 00` (9B) | −8B |
| long | `42L` | `EA` (1B) | `4C 00 00 00 00 00 00 00 2A` (9B) | −8B |
| double | `0.0` | `5B` (1B) | `47 00 00 00 00 00 00 00 00` (9B) | −8B |
| double | `1.0` | `5C` (1B) | `47 3F F0 00 00 00 00 00 00` (9B) | −8B |
| String | `""` | `53 00 00` | `53 00 00` | 相同 |

### C.5 测试中的应用

在 A-E-*（编码比对）测试中:

```rust
fn assert_hessian2_equivalent(rs_bytes: &[u8], java_fixture: &[u8]) {
    // Step 1: Try exact match
    if rs_bytes == java_fixture {
        return; // Pass
    }
    
    // Step 2: Try normalized comparison
    let rs_normalized = normalize(rs_bytes);
    let java_normalized = normalize(java_fixture);
    if rs_normalized == java_normalized {
        return; // Pass (with compact encoding difference)
    }
    
    // Step 3: Semantic comparison as last resort
    let rs_value = hessian2_decode(rs_bytes);
    let java_value = hessian2_decode(java_fixture);
    assert_eq!(rs_value, java_value);
}
```

---

## 相关文档

| 文档 | 内容 |
|------|------|
| [概述与架构](./CROSS_LANG_OVERVIEW.md) | 验证目标、测试架构、基础设施设计 |
| [Hessian2 序列化](./CROSS_LANG_HESSIAN2.md) | 序列化级验证、类型覆盖矩阵、数据桩 |
| [协议级验证](./CROSS_LANG_PROTOCOLS.md) | Dubbo TCP + Triple 协议验证 |
| [注册发现与端到端](./CROSS_LANG_REGISTRY.md) | ZK/Nacos 注册中心、E2E 服务治理 |
| [实施计划](./CROSS_LANG_PLAN.md) | 测试用例索引、阶段计划、基础设施、风险 |
