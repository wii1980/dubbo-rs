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

pub mod body;
pub mod codec;
pub mod protocol;
pub mod transport;

pub use codec::DubboCodec;
pub use protocol::DubboProtocol;
