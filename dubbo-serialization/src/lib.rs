pub use dubbo_rs_common;

use anyhow::Result;

pub trait Serialization: Send + Sync {
    fn content_type(&self) -> &'static str;

    /// Serialize raw bytes into wire-format payload.
    ///
    /// # Errors
    ///
    /// Returns an error if the serialization fails (e.g., invalid input data).
    fn serialize(&self, data: &[u8]) -> Result<Vec<u8>>;

    /// Deserialize wire-format payload back to raw bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the deserialization fails (e.g., malformed data).
    fn deserialize(&self, data: &[u8]) -> Result<Vec<u8>>;
}
