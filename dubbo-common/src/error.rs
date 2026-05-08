use crate::constants::{
    BAD_REQUEST_STATUS, BAD_RESPONSE_STATUS, CLIENT_ERROR_STATUS, CLIENT_TIMEOUT_STATUS,
    SERVER_ERROR_STATUS, SERVER_THREADPOOL_EXHAUSTED, SERVER_TIMEOUT_STATUS, SERVICE_ERROR_STATUS,
    SERVICE_NOT_FOUND_STATUS,
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RPCError {
    #[error("Client timeout: {0}")]
    ClientTimeout(String),

    #[error("Server timeout: {0}")]
    ServerTimeout(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Bad response: {0}")]
    BadResponse(String),

    #[error("Service not found: {0}")]
    ServiceNotFound(String),

    #[error("Service error: {0}")]
    ServiceError(String),

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Client error: {0}")]
    ClientError(String),

    #[error("Server threadpool exhausted: {0}")]
    ServerThreadpoolExhausted(String),
}

impl RPCError {
    pub fn from_status_code(code: u8, message: impl Into<String>) -> Self {
        let msg = message.into();
        match code {
            CLIENT_TIMEOUT_STATUS => Self::ClientTimeout(msg),
            SERVER_TIMEOUT_STATUS => Self::ServerTimeout(msg),
            BAD_REQUEST_STATUS => Self::BadRequest(msg),
            BAD_RESPONSE_STATUS => Self::BadResponse(msg),
            SERVICE_NOT_FOUND_STATUS => Self::ServiceNotFound(msg),
            SERVICE_ERROR_STATUS => Self::ServiceError(msg),
            CLIENT_ERROR_STATUS => Self::ClientError(msg),
            SERVER_THREADPOOL_EXHAUSTED => Self::ServerThreadpoolExhausted(msg),
            _ => Self::ServerError(msg),
        }
    }

    #[must_use]
    pub fn status_code(&self) -> u8 {
        match self {
            Self::ClientTimeout(_) => CLIENT_TIMEOUT_STATUS,
            Self::ServerTimeout(_) => SERVER_TIMEOUT_STATUS,
            Self::BadRequest(_) => BAD_REQUEST_STATUS,
            Self::BadResponse(_) => BAD_RESPONSE_STATUS,
            Self::ServiceNotFound(_) => SERVICE_NOT_FOUND_STATUS,
            Self::ServiceError(_) => SERVICE_ERROR_STATUS,
            Self::ServerError(_) => SERVER_ERROR_STATUS,
            Self::ClientError(_) => CLIENT_ERROR_STATUS,
            Self::ServerThreadpoolExhausted(_) => SERVER_THREADPOOL_EXHAUSTED,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_error_status_code_roundtrip() {
        let err = RPCError::ServiceNotFound("test service".into());
        assert_eq!(err.status_code(), SERVICE_NOT_FOUND_STATUS);
    }

    #[test]
    fn test_from_status_code() {
        let err = RPCError::from_status_code(BAD_REQUEST_STATUS, "invalid input");
        assert_eq!(err, RPCError::BadRequest("invalid input".into()));
    }

    #[test]
    fn test_display() {
        let err = RPCError::ServerError("internal".into());
        assert_eq!(err.to_string(), "Server error: internal");
    }

    #[test]
    fn test_all_error_variants_have_unique_status_code() {
        let codes = [
            RPCError::ClientTimeout(String::new()).status_code(),
            RPCError::ServerTimeout(String::new()).status_code(),
            RPCError::BadRequest(String::new()).status_code(),
            RPCError::BadResponse(String::new()).status_code(),
            RPCError::ServiceNotFound(String::new()).status_code(),
            RPCError::ServiceError(String::new()).status_code(),
            RPCError::ServerError(String::new()).status_code(),
            RPCError::ClientError(String::new()).status_code(),
            RPCError::ServerThreadpoolExhausted(String::new()).status_code(),
        ];
        let unique: std::collections::HashSet<u8> = codes.iter().copied().collect();
        assert_eq!(unique.len(), 9);
    }

    #[test]
    fn test_status_code_roundtrip_all_variants() {
        let cases: Vec<(RPCError, u8, &str)> = vec![
            (
                RPCError::ClientTimeout("a".into()),
                CLIENT_TIMEOUT_STATUS,
                "a",
            ),
            (
                RPCError::ServerTimeout("b".into()),
                SERVER_TIMEOUT_STATUS,
                "b",
            ),
            (RPCError::BadRequest("c".into()), BAD_REQUEST_STATUS, "c"),
            (RPCError::BadResponse("d".into()), BAD_RESPONSE_STATUS, "d"),
            (
                RPCError::ServiceNotFound("e".into()),
                SERVICE_NOT_FOUND_STATUS,
                "e",
            ),
            (
                RPCError::ServiceError("f".into()),
                SERVICE_ERROR_STATUS,
                "f",
            ),
            (RPCError::ServerError("g".into()), SERVER_ERROR_STATUS, "g"),
            (RPCError::ClientError("h".into()), CLIENT_ERROR_STATUS, "h"),
            (
                RPCError::ServerThreadpoolExhausted("i".into()),
                SERVER_THREADPOOL_EXHAUSTED,
                "i",
            ),
        ];

        for (variant, expected_code, msg) in &cases {
            assert_eq!(
                variant.status_code(),
                *expected_code,
                "status_code mismatch for {variant:?}"
            );
            let roundtrip = RPCError::from_status_code(*expected_code, *msg);
            assert_eq!(
                roundtrip, *variant,
                "roundtrip mismatch for code {expected_code}"
            );
        }
    }

    #[test]
    fn test_from_status_code_unknown_fallback() {
        let err = RPCError::from_status_code(99, "unknown code");
        assert_eq!(err, RPCError::ServerError("unknown code".into()));
    }

    #[test]
    fn test_display_all_variants() {
        assert_eq!(
            RPCError::ClientTimeout("x".into()).to_string(),
            "Client timeout: x"
        );
        assert_eq!(
            RPCError::ServerTimeout("x".into()).to_string(),
            "Server timeout: x"
        );
        assert_eq!(
            RPCError::BadRequest("x".into()).to_string(),
            "Bad request: x"
        );
        assert_eq!(
            RPCError::BadResponse("x".into()).to_string(),
            "Bad response: x"
        );
        assert_eq!(
            RPCError::ServiceNotFound("x".into()).to_string(),
            "Service not found: x"
        );
        assert_eq!(
            RPCError::ServiceError("x".into()).to_string(),
            "Service error: x"
        );
        assert_eq!(
            RPCError::ServerError("x".into()).to_string(),
            "Server error: x"
        );
        assert_eq!(
            RPCError::ClientError("x".into()).to_string(),
            "Client error: x"
        );
        assert_eq!(
            RPCError::ServerThreadpoolExhausted("x".into()).to_string(),
            "Server threadpool exhausted: x"
        );
    }
}
