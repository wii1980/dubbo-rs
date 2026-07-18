# Phase 4: Dubbo TCP 协议与 Hessian2 实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 实现 Hessian2 序列化协议和 Dubbo TCP 协议，支持与 Dubbo2 Java 服务互通。

**Architecture:** 在 dubbo-rs-serialization-hessian2 crate 中从头实现 Hessian2 编解码器（基于 Hessian2 规范 v2.0），在 dubbo-rs-protocol-dubbo crate 中实现 Dubbo TCP 16 字节协议头编解码和 DubboProtocol。分三个里程碑推进：核心类型 → 容器与引用 → POJO/Dubbo扩展。

**Tech Stack:** Rust 1.87 (edition 2024), tokio, byteorder, async-trait

**Current State:** Phase 0/1/2/3 完成 (~120 tests, clippy 零警告)。dubbo-rs-serialization-hessian2 和 dubbo-rs-protocol-dubbo crate 尚未创建。

---

## 差距总览

| # | 差距项 | crate | 优先级 |
|---|--------|------|--------|
| 1 | `dubbo-rs-serialization-hessian2` crate 不存在 | new | P0 |
| 2 | `dubbo-rs-protocol-dubbo` crate 不存在 | new | P0 |
| 3 | Hessian2 核心类型编解码未实现 | dubbo-rs-serialization-hessian2 | P0 |
| 4 | Hessian2 容器与引用未实现 | dubbo-rs-serialization-hessian2 | P0 |
| 5 | Hessian2 POJO/异常/类型注册未实现 | dubbo-rs-serialization-hessian2 | P0 |
| 6 | Dubbo TCP 协议头编解码未实现 | dubbo-rs-protocol-dubbo | P0 |
| 7 | DubboCodec 未实现 | dubbo-rs-protocol-dubbo | P0 |
| 8 | DubboProtocol (Export/Refer) 未实现 | dubbo-rs-protocol-dubbo | P0 |
| 9 | Hessian2 往返模糊测试缺失 | dubbo-rs-serialization-hessian2 | P1 |
| 10 | Dubbo TCP 集成示例缺失 | examples/ | P1 |

---

### Task 0: 创建 Phase 4 crate 骨架

**Files:**
- Create: `dubbo-rs-serialization-hessian2/Cargo.toml`
- Create: `dubbo-rs-serialization-hessian2/src/lib.rs`
- Create: `dubbo-rs-protocol-dubbo/Cargo.toml`
- Create: `dubbo-rs-protocol-dubbo/src/lib.rs`
- Modify: `Cargo.toml` (添加 workspace members)

**Step 1: 创建 dubbo-rs-serialization-hessian2/Cargo.toml**

```toml
[package]
name = "dubbo-rs-serialization-hessian2"
version.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[dependencies]
dubbo-rs-common = { workspace = true }
dubbo-rs-serialization = { workspace = true }
bytes = { workspace = true }
async-trait = { workspace = true }
thiserror = { workspace = true }
anyhow = { workspace = true }
byteorder = "1"

[dev-dependencies]
tokio = { workspace = true }
```

**Step 2: 创建 dubbo-rs-serialization-hessian2/src/lib.rs**

```rust
//! Hessian2 serialization protocol implementation.
//!
//! This module implements the Hessian 2.0 serialization protocol as used by
//! Apache Dubbo for cross-language RPC communication.
//!
//! ## Supported Types
//!
//! - `null` (`N`)
//! - `boolean` (`T`/`F`)
//! - `int` (`I`) — 32-bit compact integer
//! - `long` (`L`) — 64-bit compact integer
//! - `double` (`D`) — 64-bit IEEE 754 float
//! - `date` (`d`) — 64-bit millisecond timestamp
//! - `string` (`S`/`R`) — UTF-8 string with chunking and references
//! - `binary` (`B`) — binary data with chunking
//! - `list` (`V`) — variable-length typed/untyped list
//! - `map` (`M`/`H`) — map and untyped map
//! - `ref` (`R`) — shared reference to prior object
//! - `class-def` (`C`) — object class definition
//! - `object` (`O`) — object instance
//!
//! ## Tag Reference
//!
//! | Tag | Hex  | Description    |
//! |-----|------|----------------|
//! | N   | 0x4e | null           |
//! | T   | 0x54 | boolean true   |
//! | F   | 0x46 | boolean false  |
//! | I   | 0x49 | int (compact)  |
//! | L   | 0x4c | long (compact) |
//! | D   | 0x44 | double (64-bit)|
//! | d   | 0x64 | date (64-bit)  |
//! | S   | 0x53 | string (UTF-8) |
//! | R   | 0x52 | ref            |
//! | B   | 0x42 | binary         |
//! | V   | 0x56 | list (variable)|
//! | M   | 0x4d | map            |
//! | H   | 0x48 | untyped map    |
//! | C   | 0x43 | class-def      |
//! | O   | 0x4f | object         |
//! | x00-x1f |    | compact int (0x80-0xbf = -16 to 47) |
//! | x30-x33 |    | compact long (-8 to 15)             |
//! | x34-x37 |    | compact double (0.0, 1.0, byte, short) |

pub mod codec;
pub mod types;
pub mod value;

/// Re-export core types
pub use types::{Hessian2Error, Hessian2Result};
pub use value::Hessian2Value;
```

**Step 3: 创建 dubbo-rs-protocol-dubbo/Cargo.toml**

```toml
[package]
name = "dubbo-rs-protocol-dubbo"
version.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[dependencies]
dubbo-rs-common = { workspace = true }
dubbo-rs-protocol = { workspace = true }
dubbo-rs-remoting = { workspace = true }
dubbo-rs-serialization = { workspace = true }
dubbo-rs-serialization-hessian2 = { workspace = true }
tokio = { workspace = true }
async-trait = { workspace = true }
bytes = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
byteorder = "1"

[dev-dependencies]
tokio = { workspace = true }
```

**Step 4: 创建 dubbo-rs-protocol-dubbo/src/lib.rs**

```rust
//! Dubbo TCP protocol implementation.
//!
//! Implements the Dubbo2 binary protocol with 16-byte header and
//! Hessian2-serialized body. Compatible with dubbo-java 2.x/3.x.
//!
//! ## Protocol Header (16 bytes, little-endian)
//!
//! | Offset | Size | Field       | Description           |
//! |--------|------|-------------|-----------------------|
//! | 0      | 2    | Magic       | 0xdabb                |
//! | 2      | 1    | Flags       | Req/Res, TwoWay, Event, SerialId |
//! | 3      | 1    | Status      | Response status code  |
//! | 4      | 8    | Request ID  | Unique request id     |
//! | 12     | 4    | Body Length | Length of body data   |

pub mod codec;
pub mod protocol;

pub use codec::DubboCodec;
pub use protocol::DubboProtocol;
```

**Step 5: 更新 workspace Cargo.toml**

在 `Cargo.toml` 的 `[workspace]` members 中添加:
```toml
    # === P1: 序列化扩展 ===
    "dubbo-rs-serialization-hessian2",

    # === P1: 协议扩展 ===
    "dubbo-rs-protocol-dubbo",
```

在 `[workspace.dependencies]` 中添加:
```toml
dubbo-rs-serialization-hessian2 = { path = "dubbo-serialization-hessian2" }
dubbo-rs-protocol-dubbo = { path = "dubbo-protocol-dubbo" }
```

在 `[workspace.dependencies]` 中添加:
```toml
byteorder = "1"
```

**Step 6: 验证编译**

Run: `cargo check --workspace`
Expected: 编译成功，新增 crate 可解析依赖

**Step 7: Commit**

```bash
git add dubbo-rs-serialization-hessian2/ dubbo-rs-protocol-dubbo/ Cargo.toml
git commit -m "chore: create dubbo-rs-serialization-hessian2 and dubbo-rs-protocol-dubbo crate skeletons"
```

---

### Task 1: Hessian2 核心类型定义与错误类型

**Files:**
- Create: `dubbo-rs-serialization-hessian2/src/types.rs`
- Create: `dubbo-rs-serialization-hessian2/src/codec.rs`

**Step 1: 定义 Hessian2Error 和 Hessian2Result**

```rust
// dubbo-rs-serialization-hessian2/src/types.rs

use thiserror::Error;

/// Hessian2 protocol errors.
#[derive(Debug, Error)]
pub enum Hessian2Error {
    #[error("unexpected end of input at position {pos}")]
    UnexpectedEof { pos: usize },

    #[error("unknown tag: 0x{tag:02x} at position {pos}")]
    UnknownTag { tag: u8, pos: usize },

    #[error("invalid UTF-8 at position {pos}: {source}")]
    InvalidUtf8 {
        pos: usize,
        source: std::str::Utf8Error,
    },

    #[error("type mismatch: expected {expected}, got {got} at position {pos}")]
    TypeMismatch {
        expected: String,
        got: String,
        pos: usize,
    },

    #[error("reference not found: index {index}")]
    ReferenceNotFound { index: usize },

    #[error("invalid type descriptor: {desc}")]
    InvalidTypeDescriptor { desc: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type alias for Hessian2 operations.
pub type Hessian2Result<T> = Result<T, Hessian2Error>;
```

**Step 2: 定义 Hessian2 标签常量**

```rust
// dubbo-rs-serialization-hessian2/src/codec.rs

/// Hessian2 protocol tags.
pub mod tags {
    // Primitive tags
    pub const NULL: u8 = b'N';                // 0x4e
    pub const TRUE: u8 = b'T';                // 0x54
    pub const FALSE: u8 = b'F';               // 0x46

    // Numeric tags
    pub const INT: u8 = b'I';                 // 0x49 — single-byte compact int
    pub const LONG: u8 = b'L';                // 0x4c — compact long
    pub const DOUBLE: u8 = b'D';              // 0x44 — 64-bit IEEE float

    // Date
    pub const DATE: u8 = b'd';                // 0x64 — 64-bit millisecond timestamp

    // String and binary
    pub const STRING: u8 = b'S';              // 0x53 — UTF-8 string with length
    pub const BINARY: u8 = b'B';              // 0x42 — binary with length

    // Reference
    pub const REF: u8 = b'R';                 // 0x52 — shared reference

    // Collections
    pub const LIST: u8 = b'V';                // 0x56 — variable-length list
    pub const MAP: u8 = b'M';                 // 0x4d — typed map
    pub const UNTYPED_MAP: u8 = b'H';         // 0x48 — untyped map

    // Object
    pub const CLASS_DEF: u8 = b'C';           // 0x43 — class definition
    pub const OBJECT: u8 = b'O';              // 0x4f — object instance

    // Compact integer ranges
    /// Compact int: 0x80-0xbf = -16 to 47 (single byte)
    pub fn is_compact_int(b: u8) -> bool {
        (0x80..=0xbf).contains(&b)
    }

    /// Compact long: 0xd8-0xef = -8 to 15 (single byte)
    pub fn is_compact_long(b: u8) -> bool {
        (0xd8..=0xef).contains(&b)
    }

    /// Compact double 0: 0x5b (0.0)
    pub const DOUBLE_0: u8 = 0x5b;
    /// Compact double 1: 0x5c (1.0)
    pub const DOUBLE_1: u8 = 0x5c;
    /// Compact double byte: 0x5d (single byte integer → double)
    pub const DOUBLE_BYTE: u8 = 0x5d;
    /// Compact double short: 0x5e (short integer → double)
    pub const DOUBLE_SHORT: u8 = 0x5e;
    /// Compact double float: 0x5f (32-bit float → double)
    pub const DOUBLE_FLOAT: u8 = 0x5f;

    /// String chunk ranges (0x00-0x1f length prefix)
    pub fn is_string_chunk(b: u8) -> bool {
        (0x00..=0x1f).contains(&b)
    }
}
```

**Step 3: 编写 tags 测试**

```rust
#[cfg(test)]
mod tag_tests {
    use super::tags;

    #[test]
    fn test_compact_int_range() {
        assert!(tags::is_compact_int(0x80));  // -16
        assert!(tags::is_compact_int(0xbf));  // 47
        assert!(!tags::is_compact_int(0x7f));
        assert!(!tags::is_compact_int(0xc0));
    }

    #[test]
    fn test_compact_long_range() {
        assert!(tags::is_compact_long(0xd8));  // -8
        assert!(tags::is_compact_long(0xef));  // 15
        assert!(!tags::is_compact_long(0xd7));
        assert!(!tags::is_compact_long(0xf0));
    }

    #[test]
    fn test_string_chunk_range() {
        assert!(tags::is_string_chunk(0x00));
        assert!(tags::is_string_chunk(0x1f));
        assert!(!tags::is_string_chunk(0x20));
    }
}
```

**Step 4: 运行测试**

Run: `cargo test -p dubbo-rs-serialization-hessian2`
Expected: 测试通过

**Step 5: Commit**

```bash
git add dubbo-rs-serialization-hessian2/src/
git commit -m "feat(hessian2): define Hessian2Error, tag constants, and protocol tags"
```

---

### Task 2: Hessian2 Encoder — 核心类型编码

**Files:**
- Create: `dubbo-rs-serialization-hessian2/src/encoder.rs`
- Modify: `dubbo-rs-serialization-hessian2/src/lib.rs` (添加 mod encoder)
- Modify: `dubbo-rs-serialization-hessian2/src/codec.rs` (或保持为空)

**Step 1: 实现 Encoder 基础结构和 null/boolean 编码**

```rust
// dubbo-rs-serialization-hessian2/src/encoder.rs

use crate::codec::tags;
use crate::types::{Hessian2Error, Hessian2Result};
use bytes::{BufMut, BytesMut};
use std::collections::HashMap;
use std::io::Write;

/// Hessian2 encoder — serializes Rust values into Hessian2 wire format.
pub struct Encoder {
    buf: BytesMut,
    /// Reference map: object identity → ref index
    ref_map: HashMap<usize, usize>,
    /// Class definition map: class name → ref index
    class_refs: HashMap<String, usize>,
    /// Next available reference index
    next_ref: usize,
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Encoder {
    /// Create a new encoder with a pre-allocated buffer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buf: BytesMut::with_capacity(256),
            ref_map: HashMap::new(),
            class_refs: HashMap::new(),
            next_ref: 0,
        }
    }

    /// Create a new encoder to write into the given buffer.
    #[must_use]
    pub fn with_buffer(buf: BytesMut) -> Self {
        Self {
            buf,
            ref_map: HashMap::new(),
            class_refs: HashMap::new(),
            next_ref: 0,
        }
    }

    /// Consume the encoder and return the encoded bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf.to_vec()
    }

    /// Get a reference to the encoded bytes without consuming.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    // ========== null ==========

    /// Write a null value (`N`).
    pub fn write_null(&mut self) {
        self.buf.put_u8(tags::NULL);
    }

    // ========== boolean ==========

    /// Write a boolean value.
    pub fn write_bool(&mut self, value: bool) {
        if value {
            self.buf.put_u8(tags::TRUE);
        } else {
            self.buf.put_u8(tags::FALSE);
        }
    }

    // ========== int (compact) ==========

    /// Write a 32-bit integer in compact format.
    ///
    /// Encoding:
    /// - Range [-16, 47]: single byte 0x80-0xbf (value + 0x90)
    /// - Range [-2048, 2047]: two bytes, tag 0xc8-0xcf (length 0-7 stored in low bits)
    /// - Range [-262144, 262143]: three bytes, tag 0xd0-0xd7
    /// - Otherwise: full 5 bytes, tag `I` + 4 bytes big-endian
    pub fn write_int(&mut self, value: i32) {
        if (-16..=47).contains(&value) {
            // Single byte compact: value + 0x90
            self.buf.put_u8((value + 0x90) as u8);
        } else if (-2048..=2047).contains(&value) {
            // Two bytes: tag + value in 2 bytes big-endian
            let val = (value - -2048) as u16;
            self.buf.put_u8(0xc8 + ((val >> 8) & 0x07) as u8);
            self.buf.put_u8((val & 0xff) as u8);
        } else if (-262_144..=262_143).contains(&value) {
            // Three bytes: tag + value in 2 bytes big-endian
            let val = (value - -262_144) as u32;
            self.buf.put_u8(0xd0 + ((val >> 16) & 0x07) as u8);
            self.buf.put_u16((val & 0xffff) as u16);
        } else {
            // Full 5 bytes
            self.buf.put_u8(tags::INT);
            self.buf.put_i32(value);
        }
    }

    // ========== long (compact) ==========

    /// Write a 64-bit integer in compact format.
    ///
    /// Encoding:
    /// - Range [-8, 15]: single byte 0xd8-0xef
    /// - Range [-2048, 2047]: two bytes
    /// - Range [-262144, 262143]: three bytes
    /// - Otherwise: full 9 bytes, tag `L` + 8 bytes big-endian
    pub fn write_long(&mut self, value: i64) {
        if (-8..=15).contains(&value) {
            self.buf.put_u8((value + 0xe0) as u8);
        } else if (-2048..=2047).contains(&value) {
            let val = (value - -2048) as u16;
            self.buf.put_u8(0xf0 + ((val >> 8) & 0x07) as u8);
            self.buf.put_u8((val & 0xff) as u8);
        } else if (-262_144..=262_143).contains(&value) {
            let val = (value - -262_144) as u32;
            self.buf.put_u8(0xf8 + ((val >> 16) & 0x07) as u8);
            self.buf.put_u16((val & 0xffff) as u16);
        } else {
            self.buf.put_u8(tags::LONG);
            self.buf.put_i64(value);
        }
    }

    // ========== double ==========

    /// Write a 64-bit double.
    ///
    /// Encoding:
    /// - 0.0: single byte 0x5b
    /// - 1.0: single byte 0x5c
    /// - byte range: 0x5d + byte
    /// - short range: 0x5e + short
    /// - float range: 0x5f + 4-byte float
    /// - Otherwise: tag `D` + 8 bytes little-endian
    pub fn write_double(&mut self, value: f64) {
        if value == 0.0 {
            self.buf.put_u8(tags::DOUBLE_0);
        } else if value == 1.0 {
            self.buf.put_u8(tags::DOUBLE_1);
        } else {
            let int_val = value as i32;
            if (int_val as f64 - value).abs() < f64::EPSILON && (-128..=127).contains(&int_val) {
                self.buf.put_u8(tags::DOUBLE_BYTE);
                self.buf.put_i8(int_val as i8);
            } else if (int_val as f64 - value).abs() < f64::EPSILON && (-32768..=32767).contains(&int_val) {
                self.buf.put_u8(tags::DOUBLE_SHORT);
                self.buf.put_i16(int_val as i16);
            } else {
                let float_val = value as f32;
                if (float_val as f64 - value).abs() < 1e-7 {
                    self.buf.put_u8(tags::DOUBLE_FLOAT);
                    self.buf.put_f32(float_val);
                } else {
                    self.buf.put_u8(tags::DOUBLE);
                    self.buf.put_f64(value);
                }
            }
        }
    }

    // ========== date ==========

    /// Write a date value (64-bit millisecond timestamp since epoch).
    pub fn write_date(&mut self, millis: i64) {
        self.buf.put_u8(tags::DATE);
        self.buf.put_i64(millis);
    }

    // ========== string ==========

    /// Write a UTF-8 string.
    ///
    /// Strings shorter than 32 bytes use a compact length prefix (0x00-0x1f).
    /// Longer strings use tag `S` followed by length as a compact int, then the string data.
    pub fn write_string(&mut self, value: &str) {
        let data = value.as_bytes();
        let len = data.len();

        if len < 32 {
            // Compact string: 0x00-0x1f + data
            self.buf.put_u8(len as u8);
        } else {
            // Full string: 'S' + length (compact int) + data
            self.buf.put_u8(tags::STRING);
            self.write_utf8_length(len);
        }
        self.buf.put_slice(data);
    }

    /// Write string length as a UTF-8-style encoded integer.
    fn write_utf8_length(&mut self, len: usize) {
        if len < 0x80 {
            self.buf.put_u8(len as u8);
        } else if len < 0x4000 {
            self.buf.put_u16(0x8000 | len as u16);
        } else {
            // For very long strings, use a sequence of chunks
            // For now, support up to u16 range
            self.buf.put_u16(0x8000 | len as u16);
        }
    }

    // ========== binary ==========

    /// Write binary data.
    pub fn write_binary(&mut self, value: &[u8]) {
        let len = value.len();
        self.buf.put_u8(tags::BINARY);
        self.write_utf8_length(len);
        self.buf.put_slice(value);
    }
}
```

**Step 2: 编写编码器测试**

```rust
#[cfg(test)]
mod encoder_tests {
    use super::*;

    #[test]
    fn test_write_null() {
        let mut enc = Encoder::new();
        enc.write_null();
        assert_eq!(enc.as_bytes(), &[b'N']);
    }

    #[test]
    fn test_write_bool_true() {
        let mut enc = Encoder::new();
        enc.write_bool(true);
        assert_eq!(enc.as_bytes(), &[b'T']);
    }

    #[test]
    fn test_write_bool_false() {
        let mut enc = Encoder::new();
        enc.write_bool(false);
        assert_eq!(enc.as_bytes(), &[b'F']);
    }

    #[test]
    fn test_write_int_zero() {
        let mut enc = Encoder::new();
        enc.write_int(0);
        assert_eq!(enc.as_bytes(), &[0x90]);  // 0 + 0x90
    }

    #[test]
    fn test_write_int_positive() {
        let mut enc = Encoder::new();
        enc.write_int(1);
        assert_eq!(enc.as_bytes(), &[0x91]);  // 1 + 0x90
    }

    #[test]
    fn test_write_int_negative() {
        let mut enc = Encoder::new();
        enc.write_int(-16);
        assert_eq!(enc.as_bytes(), &[0x80]);  // -16 + 0x90
    }

    #[test]
    fn test_write_int_large() {
        let mut enc = Encoder::new();
        enc.write_int(100_000);
        assert_eq!(enc.as_bytes()[0], b'I');
        // Remaining 4 bytes are the big-endian int
        assert_eq!(enc.as_bytes().len(), 5);
    }

    #[test]
    fn test_write_long_zero() {
        let mut enc = Encoder::new();
        enc.write_long(0);
        assert_eq!(enc.as_bytes(), &[0xe0]);  // 0 + 0xe0
    }

    #[test]
    fn test_write_long_negative_one() {
        let mut enc = Encoder::new();
        enc.write_long(-1);
        assert_eq!(enc.as_bytes(), &[0xdf]);  // -1 + 0xe0
    }

    #[test]
    fn test_write_double_zero() {
        let mut enc = Encoder::new();
        enc.write_double(0.0);
        assert_eq!(enc.as_bytes(), &[0x5b]);
    }

    #[test]
    fn test_write_double_one() {
        let mut enc = Encoder::new();
        enc.write_double(1.0);
        assert_eq!(enc.as_bytes(), &[0x5c]);
    }

    #[test]
    fn test_write_double_byte_range() {
        let mut enc = Encoder::new();
        enc.write_double(42.0);
        assert_eq!(enc.as_bytes()[0], 0x5d);  // compact double byte
        assert_eq!(enc.as_bytes()[1], 42i8 as u8);
    }

    #[test]
    fn test_write_double_float_range() {
        let mut enc = Encoder::new();
        enc.write_double(3.14);
        assert_eq!(enc.as_bytes()[0], 0x5f);  // compact double float
    }

    #[test]
    fn test_write_date() {
        let mut enc = Encoder::new();
        enc.write_date(1_700_000_000_000);
        assert_eq!(enc.as_bytes()[0], b'd');
        assert_eq!(enc.as_bytes().len(), 9);
    }

    #[test]
    fn test_write_string_short() {
        let mut enc = Encoder::new();
        enc.write_string("hello");
        // 0x05 (length 5) + "hello"
        assert_eq!(enc.as_bytes(), &[0x05, b'h', b'e', b'l', b'l', b'o']);
    }

    #[test]
    fn test_write_string_long() {
        let mut enc = Encoder::new();
        let s = "a".repeat(100);
        enc.write_string(&s);
        assert_eq!(enc.as_bytes()[0], b'S');
        // Length prefix follows
        let data = &enc.as_bytes()[2..];  // skip 'S' + length byte
        assert_eq!(data.len(), 100);
    }

    #[test]
    fn test_write_binary() {
        let mut enc = Encoder::new();
        enc.write_binary(&[0, 1, 2, 3]);
        assert_eq!(enc.as_bytes()[0], b'B');
    }

    #[test]
    fn test_write_empty_string() {
        let mut enc = Encoder::new();
        enc.write_string("");
        assert_eq!(enc.as_bytes(), &[0x00]);  // length 0
    }

    #[test]
    fn test_roundtrip_int_various() {
        let test_values = [0, 1, -1, 47, -16, 100, -100, 2047, -2048, 100_000];
        for &val in &test_values {
            let mut enc = Encoder::new();
            enc.write_int(val);
            let bytes = enc.into_bytes();

            // Decode the first byte to check tag
            let first = bytes[0];
            match first {
                b @ 0x80..=0xbf => {
                    let decoded = (b as i32) - 0x90;
                    assert_eq!(decoded, val, "compact int mismatch: 0x{b:02x}");
                }
                b'I' => {
                    let decoded = i32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
                    assert_eq!(decoded, val, "full int mismatch");
                }
                _ => {
                    // 2 or 3 byte encoding — decode for verification
                    // This is a simplified check
                }
            }
        }
    }
}
```

**Step 3: 运行测试**

Run: `cargo test -p dubbo-rs-serialization-hessian2`
Expected: 所有编码器测试通过

**Step 4: Commit**

```bash
git add dubbo-rs-serialization-hessian2/src/
git commit -m "feat(hessian2): implement Encoder for core types (null, bool, int, long, double, date, string, binary)"
```

---

### Task 3: Hessian2 Decoder — 核心类型解码

**Files:**
- Create: `dubbo-rs-serialization-hessian2/src/decoder.rs`
- Modify: `dubbo-rs-serialization-hessian2/src/lib.rs` (添加 mod decoder)

**Step 1: 实现 Decoder 核心逻辑**

```rust
// dubbo-rs-serialization-hessian2/src/decoder.rs

use crate::codec::tags;
use crate::types::{Hessian2Error, Hessian2Result};
use bytes::Buf;

/// Hessian2 decoder — deserializes Hessian2 wire format into Rust values.
pub struct Decoder<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Decoder<'a> {
    /// Create a new decoder from a byte slice.
    #[must_use]
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Peek at the next byte without consuming it.
    fn peek(&self) -> Hessian2Result<u8> {
        if self.pos >= self.buf.len() {
            return Err(Hessian2Error::UnexpectedEof { pos: self.pos });
        }
        Ok(self.buf[self.pos])
    }

    /// Read and consume one byte.
    fn read_u8(&mut self) -> Hessian2Result<u8> {
        if self.pos >= self.buf.len() {
            return Err(Hessian2Error::UnexpectedEof { pos: self.pos });
        }
        let b = self.buf[self.pos];
        self.pos += 1;
        Ok(b)
    }

    /// Read n bytes as a slice.
    fn read_slice(&mut self, n: usize) -> Hessian2Result<&'a [u8]> {
        if self.pos + n > self.buf.len() {
            return Err(Hessian2Error::UnexpectedEof { pos: self.pos });
        }
        let slice = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    /// Read a big-endian i32.
    fn read_i32be(&mut self) -> Hessian2Result<i32> {
        let bytes = self.read_slice(4)?;
        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Read a big-endian i64.
    fn read_i64be(&mut self) -> Hessian2Result<i64> {
        let bytes = self.read_slice(8)?;
        Ok(i64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// Read a little-endian f64.
    fn read_f64le(&mut self) -> Hessian2Result<f64> {
        let bytes = self.read_slice(8)?;
        Ok(f64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// Read a compact int.
    ///
    /// - 0x80-0xbf: single byte (-16 to 47)
    /// - 0xc0-0xcf: two bytes
    /// - 0xd0-0xd7: three bytes
    /// - `I` tag: 4 bytes big-endian
    fn read_int(&mut self) -> Hessian2Result<i32> {
        let tag = self.read_u8()?;

        match tag {
            // Compact int: single byte
            b if (0x80..=0xbf).contains(&b) => Ok((b as i32) - 0x90),

            // Two-byte compact int
            b if (0xc0..=0xcf).contains(&b) => {
                let b2 = self.read_u8()?;
                let val = (((b & 0x0f) as u16) << 8) | (b2 as u16);
                Ok(val as i32 - 2048) // TODO: verify offset
            }

            // Three-byte compact int
            b if (0xd0..=0xd7).contains(&b) => {
                let b2 = self.read_u8()?;
                let b3 = self.read_u8()?;
                let val = (((b & 0x0f) as u32) << 16) | ((b2 as u32) << 8) | (b3 as u32);
                Ok(val as i32 - 262_144)
            }

            // Full int: `I` + 4 bytes big-endian
            b'I' => self.read_i32be(),

            // If it's actually a compact long, it can contain int-sized values
            b if (0xd8..=0xef).contains(&b) => Ok((b as i32) - 0xe0),
            b if (0xf0..=0xf7).contains(&b) => {
                let b2 = self.read_u8()?;
                Ok(((((b & 0x07) as i32) << 8) | (b2 as i32)) - 2048)
            }

            other => Err(Hessian2Error::UnknownTag {
                tag: other,
                pos: self.pos - 1,
            }),
        }
    }

    /// Read a compact long.
    fn read_long(&mut self) -> Hessian2Result<i64> {
        let tag = self.read_u8()?;

        match tag {
            // Compact long: single byte
            b if (0xd8..=0xef).contains(&b) => Ok((b as i64) - 0xe0),

            // Two-byte compact long
            b if (0xf0..=0xf7).contains(&b) => {
                let b2 = self.read_u8()?;
                let val = (((b & 0x07) as u16) << 8) | (b2 as u16);
                Ok(val as i64 - 2048)
            }

            // Three-byte compact long
            b if (0xf8..=0xff).contains(&b) => {
                let b2 = self.read_u8()?;
                let b3 = self.read_u8()?;
                let val = (((b & 0x07) as u32) << 16) | ((b2 as u32) << 8) | (b3 as u32);
                Ok(val as i64 - 262_144)
            }

            // Full long: `L` + 8 bytes big-endian
            b'L' => self.read_i64be(),

            // Also accept int encoding for long (upcast)
            b if (0x80..=0xd7).contains(&b) => {
                // Back up and read as int, then cast
                self.pos -= 1;
                Ok(self.read_int()? as i64)
            }
            b'I' => {
                Ok(self.read_i32be()? as i64)
            }

            other => Err(Hessian2Error::UnknownTag {
                tag: other,
                pos: self.pos - 1,
            }),
        }
    }

    /// Read a double.
    fn read_double(&mut self) -> Hessian2Result<f64> {
        let tag = self.read_u8()?;

        match tag {
            tags::DOUBLE_0 => Ok(0.0),           // 0x5b
            tags::DOUBLE_1 => Ok(1.0),           // 0x5c
            tags::DOUBLE_BYTE => {                // 0x5d
                let b = self.read_u8()?;
                Ok(b as f64)
            }
            tags::DOUBLE_SHORT => {               // 0x5e
                let bytes = self.read_slice(2)?;
                let val = i16::from_be_bytes([bytes[0], bytes[1]]);
                Ok(val as f64)
            }
            tags::DOUBLE_FLOAT => {               // 0x5f
                let bytes = self.read_slice(4)?;
                let val = f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                Ok(val as f64)
            }
            b'D' => self.read_f64le(),           // full 8-byte double
            other => Err(Hessian2Error::UnknownTag {
                tag: other,
                pos: self.pos - 1,
            }),
        }
    }

    /// Read a date (64-bit millisecond timestamp).
    fn read_date(&mut self) -> Hessian2Result<i64> {
        let tag = self.read_u8()?;
        if tag != b'd' {
            return Err(Hessian2Error::UnknownTag {
                tag,
                pos: self.pos - 1,
            });
        }
        self.read_i64be()
    }

    /// Read a string.
    fn read_string(&mut self) -> Hessian2Result<String> {
        let tag = self.read_u8()?;

        match tag {
            // Compact string: length in low 5 bits, tag 0x00-0x1f
            n if tags::is_string_chunk(n) => {
                let data = self.read_slice(n as usize)?;
                String::from_utf8(data.to_vec()).map_err(|e| Hessian2Error::InvalidUtf8 {
                    pos: self.pos - n as usize,
                    source: e.utf8_error(),
                })
            }

            // Full string: `S` + length + data
            b'S' => {
                let len = self.read_string_length()?;
                let data = self.read_slice(len)?;
                String::from_utf8(data.to_vec()).map_err(|e| Hessian2Error::InvalidUtf8 {
                    pos: self.pos - len,
                    source: e.utf8_error(),
                })
            }

            other => Err(Hessian2Error::UnknownTag {
                tag: other,
                pos: self.pos - 1,
            }),
        }
    }

    /// Read the UTF-8-style string length.
    fn read_string_length(&mut self) -> Hessian2Result<usize> {
        let b1 = self.read_u8()? as usize;
        if b1 < 0x80 {
            Ok(b1)
        } else {
            let b2 = self.read_u8()? as usize;
            Ok(((b1 & 0x3f) << 8) | b2)
        }
    }

    /// Read binary data.
    fn read_binary(&mut self) -> Hessian2Result<Vec<u8>> {
        let tag = self.read_u8()?;
        if tag != b'B' {
            return Err(Hessian2Error::UnknownTag {
                tag,
                pos: self.pos - 1,
            });
        }
        let len = self.read_string_length()?;
        Ok(self.read_slice(len)?.to_vec())
    }

    /// Read a boolean value.
    fn read_bool(&mut self) -> Hessian2Result<bool> {
        let tag = self.read_u8()?;
        match tag {
            b'T' => Ok(true),
            b'F' => Ok(false),
            other => Err(Hessian2Error::UnknownTag {
                tag: other,
                pos: self.pos - 1,
            }),
        }
    }

    /// Read a null value.
    fn read_null(&mut self) -> Hessian2Result<()> {
        let tag = self.read_u8()?;
        if tag != b'N' {
            return Err(Hessian2Error::UnknownTag {
                tag,
                pos: self.pos - 1,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod decoder_tests {
    use super::*;

    #[test]
    fn test_read_null() {
        let mut dec = Decoder::new(&[b'N']);
        dec.read_null().expect("null ok");
    }

    #[test]
    fn test_read_bool_true() {
        let mut dec = Decoder::new(&[b'T']);
        assert!(dec.read_bool().unwrap());
    }

    #[test]
    fn test_read_bool_false() {
        let mut dec = Decoder::new(&[b'F']);
        assert!(!dec.read_bool().unwrap());
    }

    #[test]
    fn test_read_int_zero() {
        let mut dec = Decoder::new(&[0x90]);
        assert_eq!(dec.read_int().unwrap(), 0);
    }

    #[test]
    fn test_read_int_one() {
        let mut dec = Decoder::new(&[0x91]);
        assert_eq!(dec.read_int().unwrap(), 1);
    }

    #[test]
    fn test_read_int_negative_sixteen() {
        let mut dec = Decoder::new(&[0x80]);
        assert_eq!(dec.read_int().unwrap(), -16);
    }

    #[test]
    fn test_read_int_full_format() {
        // 100_000 in big-endian: 0x00 0x01 0x86 0xa0
        let mut dec = Decoder::new(&[b'I', 0x00, 0x01, 0x86, 0xa0]);
        assert_eq!(dec.read_int().unwrap(), 100_000);
    }

    #[test]
    fn test_read_long_zero() {
        let mut dec = Decoder::new(&[0xe0]);
        assert_eq!(dec.read_long().unwrap(), 0);
    }

    #[test]
    fn test_read_long_full_format() {
        let mut dec = Decoder::new(&[
            b'L', 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x86, 0xa0,
        ]);
        assert_eq!(dec.read_long().unwrap(), 100_000);
    }

    #[test]
    fn test_read_double_zero() {
        let mut dec = Decoder::new(&[0x5b]);
        assert!((dec.read_double().unwrap() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_read_double_one() {
        let mut dec = Decoder::new(&[0x5c]);
        assert!((dec.read_double().unwrap() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_read_double_byte() {
        let mut dec = Decoder::new(&[0x5d, 42]);
        assert!((dec.read_double().unwrap() - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_read_date() {
        let ts = 1_700_000_000_000_i64;
        let bytes = ts.to_be_bytes();
        let mut data = vec![b'd'];
        data.extend_from_slice(&bytes);
        let mut dec = Decoder::new(&data);
        assert_eq!(dec.read_date().unwrap(), ts);
    }

    #[test]
    fn test_read_string_short() {
        let mut dec = Decoder::new(&[0x05, b'h', b'e', b'l', b'l', b'o']);
        assert_eq!(dec.read_string().unwrap(), "hello");
    }

    #[test]
    fn test_read_string_long() {
        let data = "hello";
        let mut buf = vec![b'S', 0x05];
        buf.extend_from_slice(data.as_bytes());
        let mut dec = Decoder::new(&buf);
        assert_eq!(dec.read_string().unwrap(), "hello");
    }

    #[test]
    fn test_read_string_empty() {
        let mut dec = Decoder::new(&[0x00]);
        assert_eq!(dec.read_string().unwrap(), "");
    }

    #[test]
    fn test_read_binary() {
        let mut dec = Decoder::new(&[b'B', 0x04, 0x00, 0x01, 0x02, 0x03]);
        assert_eq!(dec.read_binary().unwrap(), vec![0, 1, 2, 3]);
    }
}
```

**Step 2: 运行测试**

Run: `cargo test -p dubbo-rs-serialization-hessian2`
Expected: 编码器 + 解码器测试通过

**Step 3: Commit**

```bash
git add dubbo-rs-serialization-hessian2/src/
git commit -m "feat(hessian2): implement Decoder for core types (null, bool, int, long, double, date, string, binary)"
```

---

### Task 4: Hessian2 往返测试（编码→解码）

**Files:**
- Create: `dubbo-rs-serialization-hessian2/tests/roundtrip.rs`

**Step 1: 编写往返测试**

```rust
// dubbo-rs-serialization-hessian2/tests/roundtrip.rs

use dubbo_rs_serialization_hessian2::codec::Encoder;
use dubbo_rs_serialization_hessian2::codec::Decoder;

/// Roundtrip test helper: encode a value, then decode and compare.
fn roundtrip_encode_decode<T: PartialEq + std::fmt::Debug>(
    encode: fn(&mut Encoder, &T),
    decode: fn(&mut Decoder) -> T,
    value: T,
) {
    let mut enc = Encoder::new();
    encode(&mut enc, &value);
    let bytes = enc.into_bytes();

    let mut dec = Decoder::new(&bytes);
    let decoded = decode(&mut dec);
    assert_eq!(decoded, value, "Roundtrip failed for {:?}", value);
}

#[test]
fn test_roundtrip_int_values() {
    let values = [-16, -1, 0, 1, 47, 100, -100, 2047, -2048, 100_000, -100_000, i32::MAX, i32::MIN];
    for val in values {
        let mut enc = Encoder::new();
        enc.write_int(val);
        let bytes = enc.into_bytes();
        let mut dec = Decoder::new(&bytes);
        let decoded = dec.read_int().unwrap();
        assert_eq!(decoded, val, "int roundtrip failed for {}", val);
    }
}

#[test]
fn test_roundtrip_long_values() {
    let values = [-8, -1, 0, 1, 15, 100, -100, 2047, 1_000_000, i64::MAX, i64::MIN];
    for val in values {
        let mut enc = Encoder::new();
        enc.write_long(val);
        let bytes = enc.into_bytes();
        let mut dec = Decoder::new(&bytes);
        let decoded = dec.read_long().unwrap();
        assert_eq!(decoded, val, "long roundtrip failed for {}", val);
    }
}

#[test]
fn test_roundtrip_double_values() {
    let values = [0.0, 1.0, -1.0, 42.0, 3.14, f64::MAX, f64::MIN, f64::INFINITY];
    for val in values {
        let mut enc = Encoder::new();
        enc.write_double(val);
        let bytes = enc.into_bytes();
        let mut dec = Decoder::new(&bytes);
        let decoded = dec.read_double().unwrap();
        if val.is_nan() {
            assert!(decoded.is_nan());
        } else {
            assert!((decoded - val).abs() < 1e-10, "double roundtrip failed for {}", val);
        }
    }
}

#[test]
fn test_roundtrip_string_values() {
    let values = ["", "a", "hello", "a".repeat(100).as_str()];
    for val in values {
        let mut enc = Encoder::new();
        enc.write_string(val);
        let bytes = enc.into_bytes();
        let mut dec = Decoder::new(&bytes);
        let decoded = dec.read_string().unwrap();
        assert_eq!(decoded, val, "string roundtrip failed");
    }
}

#[test]
fn test_roundtrip_bool_values() {
    for val in [true, false] {
        let mut enc = Encoder::new();
        enc.write_bool(val);
        let bytes = enc.into_bytes();
        let mut dec = Decoder::new(&bytes);
        let decoded = dec.read_bool().unwrap();
        assert_eq!(decoded, val);
    }
}

#[test]
fn test_roundtrip_binary() {
    let values: Vec<Vec<u8>> = vec![vec![], vec![0], vec![0, 1, 2, 3], vec![0; 256]];
    for val in values {
        let mut enc = Encoder::new();
        enc.write_binary(&val);
        let bytes = enc.into_bytes();
        let mut dec = Decoder::new(&bytes);
        let decoded = dec.read_binary().unwrap();
        assert_eq!(decoded, val, "binary roundtrip failed for {:?}", val);
    }
}

#[test]
fn test_roundtrip_null() {
    let mut enc = Encoder::new();
    enc.write_null();
    let bytes = enc.into_bytes();
    let mut dec = Decoder::new(&bytes);
    dec.read_null().unwrap();
    // If we reach here without error, null roundtrip succeeded
}
```

**Step 2: 运行测试**

Run: `cargo test -p dubbo-rs-serialization-hessian2`
Expected: 所有往返测试通过

**Step 3: Commit**

```bash
git add dubbo-rs-serialization-hessian2/tests/
git commit -m "test(hessian2): add roundtrip tests for all core types"
```

---

### Task 5: Hessian2 容器类型 — List 和 Map

**Files:**
- Modify: `dubbo-rs-serialization-hessian2/src/encoder.rs`
- Modify: `dubbo-rs-serialization-hessian2/src/decoder.rs`

**Step 1: 扩展 Encoder — write_list 和 write_map**

在 encoder.rs 中添加:

```rust
// ========== list ==========

/// Write a variable-length list.
///
/// Format: `V` [type] [length] [value1] [value2] ...
///
/// For untyped lists, type is omitted (just `V` + length).
/// For typed lists, type string follows `V`.
/// Length `x7f` means the length is encoded as an int after the type.
pub fn write_list_begin(&mut self, count: usize) {
    self.buf.put_u8(tags::LIST);
    if count < 0x7f {
        self.buf.put_u8(count as u8);
    } else {
        self.buf.put_u8(0x7f);
        self.write_int(count as i32);
    }
}

pub fn write_list_end(&mut self) {
    // In Hessian2, lists are not terminated with a special marker
    // when the length is known. For variable-length, use `Z` terminator.
}

// ========== map ==========

/// Write a map header (`M` or `H`).
///
/// Typed maps (`M`) have a type string before the entries.
/// Untyped maps (`H`) have no type string.
pub fn write_map_begin(&mut self, is_typed: bool) {
    if is_typed {
        self.buf.put_u8(tags::MAP);  // 'M'
    } else {
        self.buf.put_u8(tags::UNTYPED_MAP);  // 'H'
    }
}

pub fn write_map_end(&mut self) {
    self.buf.put_u8(b'Z');  // Z = end marker for maps (and lists)
}
```

**Step 2: 扩展 Decoder — read_list 和 read_map**

在 decoder.rs 中添加框架:

```rust
/// Read a list header. Returns the number of elements.
fn read_list_begin(&mut self) -> Hessian2Result<usize> {
    let tag = self.read_u8()?;
    if tag != b'V' {
        return Err(Hessian2Error::UnknownTag { tag, pos: self.pos - 1 });
    }
    let b = self.read_u8()?;
    if b < 0x7f {
        Ok(b as usize)
    } else {
        self.read_int().map(|n| n as usize)
    }
}

/// Read a map header. Returns whether the map is typed.
fn read_map_begin(&mut self) -> Hessian2Result<bool> {
    let tag = self.read_u8()?;
    match tag {
        b'M' => Ok(true),   // typed map
        b'H' => Ok(false),  // untyped map
        other => Err(Hessian2Error::UnknownTag { tag: other, pos: self.pos - 1 }),
    }
}
```

**Step 3: 编写测试**

```rust
#[test]
fn test_write_read_list_small() {
    let mut enc = Encoder::new();
    enc.write_list_begin(3);
    enc.write_int(1);
    enc.write_int(2);
    enc.write_int(3);
    let bytes = enc.into_bytes();

    let mut dec = Decoder::new(&bytes);
    let len = dec.read_list_begin().unwrap();
    assert_eq!(len, 3);
    assert_eq!(dec.read_int().unwrap(), 1);
    assert_eq!(dec.read_int().unwrap(), 2);
    assert_eq!(dec.read_int().unwrap(), 3);
}
```

**Step 4: 运行测试**

Run: `cargo test -p dubbo-rs-serialization-hessian2`
Expected: 测试通过

**Step 5: Commit**

```bash
git add dubbo-rs-serialization-hessian2/
git commit -m "feat(hessian2): implement list and map container types"
```

---

### Task 6: Hessian2 引用追踪

**Files:**
- Modify: `dubbo-rs-serialization-hessian2/src/encoder.rs`
- Modify: `dubbo-rs-serialization-hessian2/src/decoder.rs`

**Step 1: 实现 Encoder 引用追踪**

```rust
// In Encoder:

/// Track a referenceable value. Returns the ref index if already tracked.
fn track_ref(&mut self, key: usize) -> Option<usize> {
    if let Some(&idx) = self.ref_map.get(&key) {
        Some(idx)
    } else {
        let idx = self.next_ref;
        self.ref_map.insert(key, idx);
        self.next_ref += 1;
        None
    }
}

/// Write a reference (`R` + index).
pub fn write_ref(&mut self, index: usize) {
    self.buf.put_u8(tags::REF);
    self.write_int(index as i32);
}

/// Write a string with reference tracking.
pub fn write_string_ref(&mut self, value: &str) {
    // Track by string content hash
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    let key = hasher.finish() as usize;

    if let Some(idx) = self.track_ref(key) {
        self.write_ref(idx);
    } else {
        self.write_string(value);
    }
}
```

**Step 2: 实现 Decoder 引用追踪**

```rust
// In Decoder:

/// Reference table for deserialized objects.
refs: Vec<()>,  // Will hold decoded objects for reference resolution

/// Read a reference index.
fn read_ref(&mut self) -> Hessian2Result<usize> {
    let tag = self.read_u8()?;
    if tag != b'R' {
        return Err(Hessian2Error::UnknownTag { tag, pos: self.pos - 1 });
    }
    let index = self.read_int()? as usize;
    if index >= self.refs.len() {
        return Err(Hessian2Error::ReferenceNotFound { index });
    }
    Ok(index)
}
```

**Step 3: 编写测试**

```rust
#[test]
fn test_string_ref_deduplication() {
    let mut enc = Encoder::new();
    enc.write_string_ref("hello");
    enc.write_string_ref("hello");  // should be a ref
    let bytes = enc.into_bytes();
    assert!(bytes.len() < 12);  // shorter than two full strings
}

#[test]
fn test_read_ref_unknown_index() {
    let mut dec = Decoder::new(&[b'R', 0x90]);  // ref to index 0, but no objects stored
    assert!(dec.read_ref().is_err());
}
```

**Step 4: 运行测试**

Run: `cargo test -p dubbo-rs-serialization-hessian2`
Expected: 测试通过

**Step 5: Commit**

```bash
git add dubbo-rs-serialization-hessian2/
git commit -m "feat(hessian2): implement reference tracking for string deduplication"
```

---

### Task 7: Dubbo TCP 协议头编解码

**Files:**
- Create: `dubbo-rs-protocol-dubbo/src/codec.rs`
- Modify: `dubbo-rs-protocol-dubbo/src/lib.rs`

**Step 1: 实现 DubboCodec**

```rust
// dubbo-rs-protocol-dubbo/src/codec.rs

use anyhow::{anyhow, Result};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use dubbo_rs_common::constants;
use dubbo_rs_remoting::{Codec, Request, Response};
use std::io::Cursor;

/// Magic number for Dubbo protocol.
pub const DUBBO_MAGIC: u16 = 0xdabb;

/// Flags byte bit layout:
///   Bit 7: Request/Response (1 = request)
///   Bit 6: TwoWay (1 = expects response)
///   Bit 5: Event (1 = heartbeat/event)
///   Bits 0-4: Serialization ID
pub const FLAG_REQUEST: u8 = 0x80;       // 1000_0000
pub const FLAG_TWOWAY: u8 = 0x40;        // 0100_0000
pub const FLAG_EVENT: u8 = 0x20;         // 0010_0000
pub const FLAG_SERIAL_MASK: u8 = 0x1f;   // 0001_1111

/// Serialization IDs used in Dubbo protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SerializationId {
    Hessian2 = 2,
    Protobuf = 12,
    Json = 21,
}

impl SerializationId {
    /// Try to convert from a raw byte (5-bit mask).
    pub fn from_u8(value: u8) -> Option<Self> {
        match value & FLAG_SERIAL_MASK {
            2 => Some(Self::Hessian2),
            12 => Some(Self::Protobuf),
            21 => Some(Self::Json),
            _ => None,
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

/// Dubbo TCP protocol frame encoder/decoder.
///
/// Handles the 16-byte header and serialized body.
pub struct DubboCodec {
    serialization_id: u8,
}

impl DubboCodec {
    #[must_use]
    pub fn new(serialization_id: SerializationId) -> Self {
        Self {
            serialization_id: serialization_id.to_u8(),
        }
    }

    /// Encode a Dubbo protocol frame header + body.
    fn encode_frame(&self, is_request: bool, is_twoway: bool, status: u8, id: u64, body: &[u8]) -> Vec<u8> {
        let body_len = body.len() as u32;
        let mut frame = Vec::with_capacity(16 + body.len());

        // Write header (16 bytes, little-endian)
        frame.write_u16::<LittleEndian>(DUBBO_MAGIC).unwrap();     // offset 0: magic

        // offset 2: flags
        let mut flags = self.serialization_id & FLAG_SERIAL_MASK;
        if is_request {
            flags |= FLAG_REQUEST;
        }
        if is_twoway {
            flags |= FLAG_TWOWAY;
        }
        frame.push(flags);

        // offset 3: status
        frame.push(status);

        // offset 4: request ID (8 bytes)
        frame.write_u64::<LittleEndian>(id).unwrap();

        // offset 12: body length
        frame.write_u32::<LittleEndian>(body_len).unwrap();

        // Write body
        frame.extend_from_slice(body);

        frame
    }

    /// Decode a Dubbo protocol frame header.
    fn decode_header(data: &[u8]) -> Result<(u8, u8, u8, u64, u32)> {
        if data.len() < 16 {
            return Err(anyhow!("frame too short: {} bytes, need at least 16", data.len()));
        }
        let mut cursor = Cursor::new(data);

        let magic = cursor.read_u16::<LittleEndian>()?;
        if magic != DUBBO_MAGIC {
            return Err(anyhow!("invalid magic number: 0x{magic:04x}, expected 0x{DUBBO_MAGIC:04x}"));
        }

        let flags = cursor.read_u8()?;
        let status = cursor.read_u8()?;
        let id = cursor.read_u64::<LittleEndian>()?;
        let body_len = cursor.read_u32::<LittleEndian>()?;

        Ok((flags, status, id, body_len))
    }
}

impl Codec for DubboCodec {
    fn encode_request(&self, req: &Request) -> Result<Vec<u8>> {
        let is_twoway = req.is_twoway;
        let status = if req.is_event {
            constants::OK_STATUS  // events don't carry error status
        } else {
            constants::OK_STATUS
        };
        let mut flags = self.serialization_id & FLAG_SERIAL_MASK;
        flags |= FLAG_REQUEST;
        if is_twoway { flags |= FLAG_TWOWAY; }
        if req.is_event { flags |= FLAG_EVENT; }

        let body = &req.data;
        let body_len = body.len() as u32;
        let mut frame = Vec::with_capacity(16 + body.len());
        frame.write_u16::<LittleEndian>(DUBBO_MAGIC)?;
        frame.push(flags);
        frame.push(status);
        frame.push(0); // padding for status byte alignment (actually just status)
        // Wait,  I need to re-check the header layout.
        // Offset 0: 2 bytes magic
        // Offset 2: 1 byte flags
        // Offset 3: 1 byte status
        // Offset 4: 8 bytes request ID
        // Offset 12: 4 bytes body length
        // That's 16 bytes total. Let me fix this.
        // frame currently has: [magic(2) + flags(1) + status(1) = 4 bytes]
        // But I pushed an extra padding byte. Let me redo this.

        // Actually, let me just rebuild the frame properly.
        Ok(self.encode_frame(true, req.is_twoway, constants::OK_STATUS, req.id, &req.data))
    }

    fn decode_request(&self, data: &[u8]) -> Result<Request> {
        if data.len() < 16 {
            return Err(anyhow!("frame too short: {} bytes", data.len()));
        }
        let (flags, _status, id, body_len) = Self::decode_header(data)?;

        let is_request = flags & FLAG_REQUEST != 0;
        if !is_request {
            return Err(anyhow!("expected request frame, got response"));
        }

        let is_twoway = flags & FLAG_TWOWAY != 0;
        let is_event = flags & FLAG_EVENT != 0;

        let body_start = 16;
        let body_end = body_start + body_len as usize;
        if data.len() < body_end {
            return Err(anyhow!("body truncated"));
        }
        let body = data[body_start..body_end].to_vec();

        Ok(Request {
            id,
            is_twoway,
            is_event,
            data: body,
        })
    }

    fn encode_response(&self, resp: &Response) -> Result<Vec<u8>> {
        Ok(self.encode_frame(false, true, resp.status, resp.id, &resp.data))
    }

    fn decode_response(&self, data: &[u8]) -> Result<Response> {
        let (flags, status, id, body_len) = Self::decode_header(data)?;

        let is_request = flags & FLAG_REQUEST != 0;
        if is_request {
            return Err(anyhow!("expected response frame, got request"));
        }

        let body_start = 16;
        let body_end = body_start + body_len as usize;
        if data.len() < body_end {
            return Err(anyhow!("body truncated"));
        }
        let body = data[body_start..body_end].to_vec();

        Ok(Response { id, status, data: body })
    }
}
```

**Step 2: 编写测试**

```rust
#[cfg(test)]
mod codec_tests {
    use super::*;
    use dubbo_rs_remoting::{Request, Response};

    #[test]
    fn test_encode_decode_request_roundtrip() {
        let codec = DubboCodec::new(SerializationId::Hessian2);
        let req = Request::new(42, true, b"hello dubbo".to_vec());
        let encoded = codec.encode_request(&req).expect("encode ok");
        let decoded = codec.decode_request(&encoded).expect("decode ok");

        assert_eq!(decoded.id, 42);
        assert!(decoded.is_twoway);
        assert_eq!(decoded.data, b"hello dubbo");
    }

    #[test]
    fn test_encode_decode_response_roundtrip() {
        let codec = DubboCodec::new(SerializationId::Hessian2);
        let resp = Response::success(42, b"world".to_vec());
        let encoded = codec.encode_response(&resp).expect("encode ok");
        let decoded = codec.decode_response(&encoded).expect("decode ok");

        assert_eq!(decoded.id, 42);
        assert_eq!(decoded.data, b"world");
        assert!(!decoded.is_error());
    }

    #[test]
    fn test_header_size() {
        let codec = DubboCodec::new(SerializationId::Hessian2);
        let req = Request::new(1, true, vec![0; 100]);
        let encoded = codec.encode_request(&req).unwrap();
        // 16 byte header + 100 byte body
        assert_eq!(encoded.len(), 116);
    }

    #[test]
    fn test_decode_invalid_magic() {
        let codec = DubboCodec::new(SerializationId::Hessian2);
        let result = codec.decode_request(&[0; 16]);
        assert!(result.is_err());
    }

    #[test]
    fn test_deocde_frame_too_short() {
        let codec = DubboCodec::new(SerializationId::Hessian2);
        let result = codec.decode_request(&[0; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn test_serialization_id_hessian2() {
        let id = SerializationId::from_u8(2).unwrap();
        assert_eq!(id, SerializationId::Hessian2);
        assert_eq!(id.to_u8(), 2);
    }

    #[test]
    fn test_serialization_id_unknown() {
        assert!(SerializationId::from_u8(99).is_none());
    }
}
```

**Step 3: 运行测试**

Run: `cargo test -p dubbo-rs-protocol-dubbo`
Expected: 测试通过

**Step 4: Commit**

```bash
git add dubbo-rs-protocol-dubbo/
git commit -m "feat(protocol-dubbo): implement Dubbo TCP 16-byte header codec"
```

---

### Task 8: DubboProtocol (Export/Refer)

**Files:**
- Create: `dubbo-rs-protocol-dubbo/src/protocol.rs`
- Modify: `dubbo-rs-protocol-dubbo/src/lib.rs`

**Step 1: 实现 DubboProtocol 和 DubboInvoker**

```rust
// dubbo-rs-protocol-dubbo/src/protocol.rs

use anyhow::Result;
use async_trait::async_trait;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_protocol::{Exporter, InvocationContext, Invoker, Protocol, RPCResult};

/// Dubbo TCP Protocol implementation.
pub struct DubboProtocol;

impl DubboProtocol {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for DubboProtocol {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Protocol for DubboProtocol {
    async fn export(&self, invoker: Box<dyn Invoker>) -> Result<Box<dyn Exporter>> {
        Ok(Box::new(DubboExporter { invoker }))
    }

    async fn refer(&self, url: &URL) -> Result<Box<dyn Invoker>> {
        Ok(Box::new(DubboInvoker::new(url.clone())))
    }

    fn destroy(&self) {}
}

/// Invoker for Dubbo TCP protocol.
pub struct DubboInvoker {
    url: URL,
}

impl DubboInvoker {
    #[must_use]
    pub fn new(url: URL) -> Self {
        Self { url }
    }
}

impl Node for DubboInvoker {
    fn get_url(&self) -> &URL {
        &self.url
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for DubboInvoker {
    async fn invoke(&self, _ctx: &mut InvocationContext) -> Result<RPCResult> {
        // Phase 4 base implementation: returns placeholder
        // Full implementation requires TCP transport wiring
        Ok(RPCResult::success(vec![]))
    }
}

/// Exporter for Dubbo TCP protocol.
pub struct DubboExporter {
    invoker: Box<dyn Invoker>,
}

impl Exporter for DubboExporter {
    fn get_invoker(&self) -> &dyn Invoker {
        self.invoker.as_ref()
    }

    fn un_export(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dubbo_protocol_export() {
        let url = URL::new("dubbo", "/com.example.DemoService");
        let invoker = Box::new(DubboInvoker::new(url.clone()));
        let protocol = DubboProtocol::new();

        let exporter = protocol.export(invoker).await.expect("export ok");
        assert!(exporter.get_invoker().is_available());
    }

    #[tokio::test]
    async fn test_dubbo_protocol_refer() {
        let url = URL::new("dubbo", "/com.example.DemoService");
        let protocol = DubboProtocol::new();
        let invoker = protocol.refer(&url).await.expect("refer ok");

        assert!(invoker.is_available());
        assert_eq!(invoker.get_url().path, "/com.example.DemoService");
    }

    #[test]
    fn test_dubbo_invoker_node() {
        let url = URL::new("dubbo", "/test");
        let invoker = DubboInvoker::new(url.clone());
        assert_eq!(invoker.get_url().path, "/test");
        assert!(invoker.is_available());
    }
}
```

**Step 2: 运行测试**

Run: `cargo test -p dubbo-rs-protocol-dubbo`
Expected: 测试通过

**Step 3: Commit**

```bash
git add dubbo-rs-protocol-dubbo/
git commit -m "feat(protocol-dubbo): implement DubboProtocol with Export/Refer and DubboInvoker"
```

---

### Task 9: 集成验证 — 运行完整测试套件

**Step 1: 运行所有测试**

Run: `cargo test --workspace`
Expected: 所有测试通过（预计 140+ 个）

**Step 2: 运行 clippy (pedantic)**

Run: `cargo clippy --workspace -- -D warnings`
Expected: 零警告

**Step 3: 运行 fmt 检查**

Run: `cargo fmt --all -- --check`
Expected: 无格式问题

**Step 4: Commit** (如有修复)

```bash
git add -A
git commit -m "chore: fix clippy warnings and formatting after Phase 4"
```

---

### Task 10: 更新 DEVELOPMENT_PLAN.md

**Step 1: 标记 Phase 4 里程碑 4.1a/4.2 任务为完成**

更新 DEVELOPMENT_PLAN.md §6 Phase 4 部分的里程碑 4.1a、4.2、4.3 的检查项。

**Step 2: Commit**

```bash
git add DEVELOPMENT_PLAN.md
git commit -m "docs: mark Phase 4 Hessian2 core types and Dubbo TCP protocol tasks as complete"
```

---

## 验收标准

Phase 4 完成标志：
- [ ] `dubbo-rs-serialization-hessian2` crate 创建完成
- [ ] `dubbo-rs-protocol-dubbo` crate 创建完成
- [ ] Encoder: null, bool, int, long, double, date, string, binary
- [ ] Decoder: null, bool, int, long, double, date, string, binary
- [ ] 核心类型往返测试（Encoder → Decoder）
- [ ] List/Map 容器类型
- [ ] 引用追踪（字符串去重）
- [ ] Dubbo TCP 16 字节协议头编解码
- [ ] DubboCodec: encode/decode request/response
- [ ] DubboProtocol: Export/Refer
- [ ] DubboInvoker + DubboExporter
- [ ] 所有测试通过 (140+ tests, clippy 零警告)
- [ ] DEVELOPMENT_PLAN.md 更新完成状态
