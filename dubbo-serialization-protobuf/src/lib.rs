//! Protobuf serialization support for Dubbo protocols.
//!
//! ## Serialization Strategy
//!
//! - **Triple protocol (gRPC)**: Protobuf encoding/decoding is handled natively
//!   by [`tonic`] at the transport layer. The code-generated service stubs
//!   produced by `tonic-build` or `dubbo-rs-codegen` already include proper
//!   `prost::Message` implementations, so no additional serialization is needed.
//!
//! - **Dubbo TCP protocol (SerializationId=12)**: When using Protobuf as the
//!   payload format over Dubbo's binary protocol, the [`dubbo-rs-protocol-dubbo`]
//!   transport handles the Hessian2-encoded request/response envelope. For
//!   typed Protobuf message encoding within the body, use `prost::Message`
//!   directly (`Message::encode` / `Message::decode`).
//!
//! - **Standalone**: The [`Serialization`] trait operates on raw bytes
//!   (`&[u8]` ↔ `&[u8]`), which is a lower-level interface than the typed
//!   message encoding that Protobuf requires. Therefore, this implementation
//!   acts as a pass-through at the byte level.

pub use dubbo_rs_common;
pub use dubbo_rs_serialization;

use anyhow::Result;
use dubbo_rs_serialization::Serialization;

/// Protobuf serialization implementation.
///
/// At the byte level, this is a pass-through since typed protobuf
/// encoding is handled by `tonic` (Triple) or `prost::Message` directly
/// (Dubbo TCP). See the [module-level documentation](self) for details.
pub struct ProtobufSerialization;

impl ProtobufSerialization {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProtobufSerialization {
    fn default() -> Self {
        Self::new()
    }
}

impl Serialization for ProtobufSerialization {
    fn content_type(&self) -> &'static str {
        "application/grpc+proto"
    }

    fn serialize(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }

    fn deserialize(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protobuf_content_type() {
        let ser = ProtobufSerialization::new();
        assert_eq!(ser.content_type(), "application/grpc+proto");
    }

    #[test]
    fn test_protobuf_serialize_pass_through() {
        let ser = ProtobufSerialization::new();
        let input = b"hello protobuf";
        let result = ser.serialize(input).expect("serialize should succeed");
        assert_eq!(result, input);
    }

    #[test]
    fn test_protobuf_deserialize_pass_through() {
        let ser = ProtobufSerialization::new();
        let input = b"hello protobuf";
        let result = ser.deserialize(input).expect("deserialize should succeed");
        assert_eq!(result, input);
    }

    #[test]
    fn test_protobuf_roundtrip() {
        let ser = ProtobufSerialization::new();
        let original = vec![0u8, 1, 2, 3, 255];
        let serialized = ser.serialize(&original).expect("serialize should succeed");
        let deserialized = ser
            .deserialize(&serialized)
            .expect("deserialize should succeed");
        assert_eq!(deserialized, original);
    }
}
