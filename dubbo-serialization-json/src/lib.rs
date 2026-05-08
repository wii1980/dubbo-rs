pub use dubbo_rs_common;
pub use dubbo_rs_serialization;

use anyhow::Result;
use dubbo_rs_serialization::Serialization;

pub struct JsonSerialization;

impl JsonSerialization {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for JsonSerialization {
    fn default() -> Self {
        Self::new()
    }
}

impl Serialization for JsonSerialization {
    fn content_type(&self) -> &'static str {
        "application/json"
    }

    fn serialize(&self, data: &[u8]) -> Result<Vec<u8>> {
        let value: serde_json::Value = serde_json::from_slice(data)?;
        serde_json::to_vec(&value).map_err(anyhow::Error::from)
    }

    fn deserialize(&self, data: &[u8]) -> Result<Vec<u8>> {
        let value: serde_json::Value = serde_json::from_slice(data)?;
        serde_json::to_vec(&value).map_err(anyhow::Error::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_content_type() {
        let ser = JsonSerialization::new();
        assert_eq!(ser.content_type(), "application/json");
    }

    #[test]
    fn test_json_serialize_valid() {
        let ser = JsonSerialization::new();
        let input = br#"{"key":"value"}"#;
        let result = ser.serialize(input).expect("serialize should succeed");
        let parsed: serde_json::Value =
            serde_json::from_slice(&result).expect("should be valid JSON");
        assert_eq!(parsed["key"], "value");
    }

    #[test]
    fn test_json_deserialize_valid() {
        let ser = JsonSerialization::new();
        let input = br#"{"hello":"world"}"#;
        let result = ser.deserialize(input).expect("deserialize should succeed");
        let parsed: serde_json::Value =
            serde_json::from_slice(&result).expect("should be valid JSON");
        assert_eq!(parsed["hello"], "world");
    }

    #[test]
    fn test_json_serialize_invalid() {
        let ser = JsonSerialization::new();
        let result = ser.serialize(b"not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_json_deserialize_invalid() {
        let ser = JsonSerialization::new();
        let result = ser.deserialize(b"not json");
        assert!(result.is_err());
    }
}
