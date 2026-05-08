use thiserror::Error;

/// Hessian2 serialization/deserialization errors.
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

/// Alias for `Result<T, Hessian2Error>`.
pub type Hessian2Result<T> = std::result::Result<T, Hessian2Error>;
