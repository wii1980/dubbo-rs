//! Dubbo TCP protocol body serialization using Hessian2.
//!
//! Handles encoding/decoding of Dubbo request and response bodies
//! in the Dubbo2 binary protocol format.
//!
//! ## Request Body Format (Hessian2)
//!
//! | Offset | Field               | Type         |
//! |--------|---------------------|-------------|
//! | 0      | dubbo_version       | string      |
//! | 1      | service_path        | string      |
//! | 2      | service_version     | string      |
//! | 3      | method_name         | string      |
//! | 4      | param_types_desc    | string      |
//! | 5      | arguments           | list        |
//! | 6      | attachments         | map         |
//!
//! ## Response Body Format
//!
//! | Field  | Type  | Description                          |
//! |--------|-------|--------------------------------------|
//! | flag   | byte  | 1=null result, 2=value, 0=exception |
//! | result | bytes | Only present when flag == 2         |
//! | error  | bytes | Only present when flag == 0         |

use anyhow::{Context, Result};
use dubbo_rs_protocol::{InvocationContext, RPCResult};
use dubbo_rs_serialization_hessian2::codec::encoder::Encoder;
use dubbo_rs_serialization_hessian2::decoder::Decoder;

const DUBBO_VERSION: &str = "2.0.2";

const RESPONSE_NULL_VALUE: i32 = 1;
const RESPONSE_VALUE: i32 = 2;
const RESPONSE_WITH_EXCEPTION: i32 = 0;

/// Encode a request body from an InvocationContext into Hessian2 bytes
/// following the Dubbo TCP body format.
///
/// # Errors
///
/// Returns an error if encoding fails (should not happen in practice
/// since the Encoder writes to an in-memory buffer).
pub fn encode_request_body(ctx: &InvocationContext) -> Result<Vec<u8>> {
    let mut enc = Encoder::new();

    enc.write_string(DUBBO_VERSION);

    let service_path = ctx.url.path.trim_start_matches('/');
    enc.write_string(service_path);

    let version = ctx.url.get_param_or_default("version", "1.0.0");
    enc.write_string(&version);

    enc.write_string(&ctx.method_name);

    let desc = build_parameter_descriptor(&ctx.parameter_types);
    enc.write_string(&desc);

    enc.write_list_begin(ctx.arguments.len());
    for arg in &ctx.arguments {
        enc.write_binary(arg);
    }

    enc.write_map_begin();
    for (key, value) in &ctx.attachments {
        enc.write_string(key);
        enc.write_string(value);
    }
    enc.write_map_end();

    Ok(enc.into_bytes())
}

/// Decode Hessian2 bytes into an InvocationContext following the Dubbo TCP body format.
///
/// # Errors
///
/// Returns an error if the data is malformed or truncated.
pub fn decode_request_body(
    data: &[u8],
    base_url: &dubbo_rs_common::url::URL,
) -> Result<InvocationContext> {
    let mut dec = Decoder::new(data);

    let _dubbo_version = dec.read_string().context("failed to read dubbo_version")?;

    let _service_path = dec.read_string().context("failed to read service_path")?;

    let _service_version = dec
        .read_string()
        .context("failed to read service_version")?;

    let method_name = dec.read_string().context("failed to read method_name")?;

    let param_desc = dec
        .read_string()
        .context("failed to read param_types_desc")?;
    let parameter_types = parse_parameter_descriptor(&param_desc);

    let arg_count = dec
        .read_list_begin()
        .context("failed to read argument list header")?;
    let mut arguments = Vec::with_capacity(arg_count);
    for i in 0..arg_count {
        let arg = dec
            .read_binary()
            .with_context(|| format!("failed to read argument {i}"))?;
        arguments.push(arg);
    }

    let _is_typed = dec
        .read_map_begin()
        .context("failed to read attachments map header")?;

    let mut attachments = std::collections::HashMap::new();
    loop {
        let tag = dec
            .peek()
            .map_err(|e| anyhow::anyhow!("failed to peek map entry: {e}"))?;
        if tag == b'Z' {
            let _ = dec.read_u8();
            break;
        }
        let key = dec.read_string().context("failed to read attachment key")?;
        let value = dec
            .read_string()
            .context("failed to read attachment value")?;
        attachments.insert(key, value);
    }

    let url = base_url.clone();
    let mut ctx = InvocationContext::new(method_name, url)
        .with_parameter_types(parameter_types)
        .with_arguments(arguments);
    ctx.attachments = attachments;
    Ok(ctx)
}

/// Encode an RPCResult into the Dubbo TCP response body format.
///
/// # Errors
///
/// Returns an error if encoding fails.
pub fn encode_response_body(result: &RPCResult) -> Result<Vec<u8>> {
    let mut enc = Encoder::new();

    if result.is_error() {
        enc.write_int(RESPONSE_WITH_EXCEPTION);
        if let Some(ref err) = result.error {
            enc.write_string(&err.to_string());
        } else {
            enc.write_string("unknown error");
        }
    } else if let Some(ref value) = result.value {
        enc.write_int(RESPONSE_VALUE);
        enc.write_binary(value);
    } else {
        enc.write_int(RESPONSE_NULL_VALUE);
        enc.write_null();
    }

    Ok(enc.into_bytes())
}

/// Decode Hessian2 bytes into an RPCResult following the Dubbo TCP response format.
///
/// # Errors
///
/// Returns an error if the data is malformed.
pub fn decode_response_body(data: &[u8]) -> Result<RPCResult> {
    let mut dec = Decoder::new(data);

    let flag = dec.read_int().context("failed to read response flag")?;

    match flag {
        RESPONSE_VALUE => {
            let value = dec.read_binary().context("failed to read response value")?;
            Ok(RPCResult::success(value))
        }
        RESPONSE_NULL_VALUE => Ok(RPCResult::success(vec![])),
        RESPONSE_WITH_EXCEPTION => {
            let err_msg = dec
                .read_string()
                .context("failed to read exception message")?;
            Ok(RPCResult::from_error(
                dubbo_rs_common::error::RPCError::ServiceError(err_msg),
            ))
        }
        _ => Ok(RPCResult::from_error(
            dubbo_rs_common::error::RPCError::ServerError(format!("unknown response flag: {flag}")),
        )),
    }
}

/// Build a Java-style method descriptor from parameter types.
///
/// For empty parameter list, returns an empty string.
/// Otherwise, concatenates the parameter type descriptors.
fn build_parameter_descriptor(types: &[String]) -> String {
    if types.is_empty() {
        String::new()
    } else {
        types.join("")
    }
}

/// Parse a Java-style method descriptor into individual parameter types.
///
/// Handles both primitive descriptors (e.g., `"I"`, `"J"`) and
/// object descriptors (e.g., `"Ljava/lang/String;"`), as well as
/// array descriptors (e.g., `"[B"`, `"[Ljava/lang/String;"`).
fn parse_parameter_descriptor(desc: &str) -> Vec<String> {
    if desc.is_empty() {
        return Vec::new();
    }

    let mut types = Vec::new();
    let chars: Vec<char> = desc.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let start = i;
        match chars[i] {
            '[' => {
                i += 1;
                if i < chars.len() && chars[i] == 'L' {
                    i += 1;
                    while i < chars.len() && chars[i] != ';' {
                        i += 1;
                    }
                    i += 1;
                } else {
                    i += 1;
                }
            }
            'L' => {
                i += 1;
                while i < chars.len() && chars[i] != ';' {
                    i += 1;
                }
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
        types.push(desc[start..i].to_string());
    }

    types
}

#[cfg(test)]
mod tests {
    use super::*;
    use dubbo_rs_common::url::URL;

    // ── Parameter descriptor parsing ──────────────────────────────────

    #[test]
    fn test_parse_parameter_descriptor_empty() {
        assert_eq!(parse_parameter_descriptor(""), Vec::<String>::new());
    }

    #[test]
    fn test_parse_parameter_descriptor_primitive() {
        assert_eq!(parse_parameter_descriptor("I"), vec!["I"]);
        assert_eq!(parse_parameter_descriptor("J"), vec!["J"]);
        assert_eq!(parse_parameter_descriptor("Z"), vec!["Z"]);
    }

    #[test]
    fn test_parse_parameter_descriptor_object() {
        assert_eq!(
            parse_parameter_descriptor("Ljava/lang/String;"),
            vec!["Ljava/lang/String;"]
        );
    }

    #[test]
    fn test_parse_parameter_descriptor_multiple() {
        let desc = "ILjava/lang/String;Z";
        let types = parse_parameter_descriptor(desc);
        assert_eq!(types, vec!["I", "Ljava/lang/String;", "Z"]);
    }

    #[test]
    fn test_parse_parameter_descriptor_array() {
        assert_eq!(parse_parameter_descriptor("[B"), vec!["[B"]);
        assert_eq!(
            parse_parameter_descriptor("[Ljava/lang/String;"),
            vec!["[Ljava/lang/String;"]
        );
    }

    #[test]
    fn test_build_parameter_descriptor_empty() {
        assert_eq!(build_parameter_descriptor(&[]), "");
    }

    #[test]
    fn test_build_parameter_descriptor_nonempty() {
        let types = vec!["I".to_string(), "Ljava/lang/String;".to_string()];
        assert_eq!(build_parameter_descriptor(&types), "ILjava/lang/String;");
    }

    // ── Request body roundtrip ────────────────────────────────────────

    #[test]
    fn test_encode_decode_request_body_roundtrip() {
        let mut url = URL::new("dubbo", "/com.example.Greeter");
        url.set_param("version", "1.0.0");

        let ctx = InvocationContext::new("sayHello", url)
            .with_parameter_types(vec!["Ljava/lang/String;".to_string()])
            .with_arguments(vec![b"world".to_vec()]);

        let encoded = encode_request_body(&ctx).expect("encode should succeed");
        assert!(!encoded.is_empty(), "encoded body should not be empty");

        let base_url = URL::new("dubbo", "/com.example.Greeter");
        let decoded = decode_request_body(&encoded, &base_url).expect("decode should succeed");

        assert_eq!(decoded.method_name, "sayHello");
        assert_eq!(decoded.parameter_types, vec!["Ljava/lang/String;"]);
        assert_eq!(decoded.arguments.len(), 1);
        assert_eq!(decoded.arguments[0], b"world");
    }

    #[test]
    fn test_encode_decode_request_body_no_args() {
        let url = URL::new("dubbo", "/com.example.EmptyService");

        let ctx = InvocationContext::new("ping", url);

        let encoded = encode_request_body(&ctx).expect("encode should succeed");
        let base_url = URL::new("dubbo", "/com.example.EmptyService");
        let decoded = decode_request_body(&encoded, &base_url).expect("decode should succeed");

        assert_eq!(decoded.method_name, "ping");
        assert_eq!(decoded.arguments.len(), 0);
        assert_eq!(decoded.parameter_types.len(), 0);
    }

    #[test]
    fn test_encode_decode_request_body_with_attachments() {
        let url = URL::new("dubbo", "/com.example.Greeter");

        let mut ctx = InvocationContext::new("greet", url);
        ctx.attachments
            .insert("path".to_string(), "com.example.Greeter".to_string());
        ctx.attachments
            .insert("interface".to_string(), "com.example.Greeter".to_string());
        ctx.attachments
            .insert("version".to_string(), "1.0.0".to_string());

        let encoded = encode_request_body(&ctx).expect("encode should succeed");
        let base_url = URL::new("dubbo", "/com.example.Greeter");
        let decoded = decode_request_body(&encoded, &base_url).expect("decode should succeed");

        assert_eq!(decoded.method_name, "greet");
        assert_eq!(
            decoded.attachments.get("path"),
            Some(&"com.example.Greeter".to_string())
        );
        assert_eq!(
            decoded.attachments.get("interface"),
            Some(&"com.example.Greeter".to_string())
        );
        assert_eq!(
            decoded.attachments.get("version"),
            Some(&"1.0.0".to_string())
        );
    }

    #[test]
    fn test_encode_decode_request_body_multiple_args() {
        let url = URL::new("dubbo", "/com.example.CalcService");

        let ctx = InvocationContext::new("add", url)
            .with_parameter_types(vec!["I".to_string(), "I".to_string()])
            .with_arguments(vec![
                vec![0x00, 0x00, 0x00, 0x01], // int 1 as binary
                vec![0x00, 0x00, 0x00, 0x02], // int 2 as binary
            ]);

        let encoded = encode_request_body(&ctx).expect("encode should succeed");
        let base_url = URL::new("dubbo", "/com.example.CalcService");
        let decoded = decode_request_body(&encoded, &base_url).expect("decode should succeed");

        assert_eq!(decoded.method_name, "add");
        assert_eq!(decoded.arguments.len(), 2);
        assert_eq!(decoded.arguments[0], vec![0x00, 0x00, 0x00, 0x01]);
        assert_eq!(decoded.arguments[1], vec![0x00, 0x00, 0x00, 0x02]);
    }

    // ── Response body roundtrip ───────────────────────────────────────

    #[test]
    fn test_encode_decode_response_value() {
        let result = RPCResult::success(b"hello dubbo".to_vec());
        let encoded = encode_response_body(&result).expect("encode should succeed");
        let decoded = decode_response_body(&encoded).expect("decode should succeed");

        assert!(!decoded.is_error());
        assert_eq!(decoded.value, Some(b"hello dubbo".to_vec()));
    }

    #[test]
    fn test_encode_decode_response_null() {
        let result = RPCResult::success(vec![]);
        let encoded = encode_response_body(&result).expect("encode should succeed");
        let decoded = decode_response_body(&encoded).expect("decode should succeed");

        assert!(!decoded.is_error());
    }

    #[test]
    fn test_encode_decode_response_exception() {
        let err = dubbo_rs_common::error::RPCError::ServiceError("something went wrong".into());
        let result = RPCResult::from_error(err);
        let encoded = encode_response_body(&result).expect("encode should succeed");
        let decoded = decode_response_body(&encoded).expect("decode should succeed");

        assert!(decoded.is_error());
    }

    // ── Response with empty data test ─────────────────────────────────

    #[test]
    fn test_encode_decode_empty_request() {
        let url = URL::new("dubbo", "/com.example.VoidService");

        let ctx = InvocationContext::new("doNothing", url);

        let encoded = encode_request_body(&ctx).expect("encode should succeed");
        let base_url = URL::new("dubbo", "/com.example.VoidService");
        let decoded = decode_request_body(&encoded, &base_url).expect("decode should succeed");

        assert_eq!(decoded.method_name, "doNothing");
        assert_eq!(decoded.arguments.len(), 0);
    }
}
