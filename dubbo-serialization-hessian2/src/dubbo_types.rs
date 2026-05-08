use crate::codec::encoder::Encoder;
use crate::decoder::Decoder;
use crate::types::{Hessian2Error, Hessian2Result};

// ── StackTraceElement ──

/// Java `StackTraceElement` for exception stack trace serialization.
///
/// Hessian2 encodes this as an object with class name
/// `java.lang.StackTraceElement` and fields:
/// `declaringClass`, `methodName`, `fileName`, `lineNumber`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackTraceElement {
    pub declaring_class: String,
    pub method_name: String,
    pub file_name: String,
    pub line_number: i32,
}

impl StackTraceElement {
    pub fn new(
        declaring_class: impl Into<String>,
        method_name: impl Into<String>,
        file_name: impl Into<String>,
        line_number: i32,
    ) -> Self {
        Self {
            declaring_class: declaring_class.into(),
            method_name: method_name.into(),
            file_name: file_name.into(),
            line_number,
        }
    }

    pub fn encode(&self, enc: &mut Encoder) {
        let class_idx = enc.write_class_def(
            "java.lang.StackTraceElement",
            &[
                "declaringClass".to_string(),
                "methodName".to_string(),
                "fileName".to_string(),
                "lineNumber".to_string(),
            ],
        );
        enc.write_object_begin(class_idx);
        enc.write_string(&self.declaring_class);
        enc.write_string(&self.method_name);
        enc.write_string(&self.file_name);
        enc.write_int(self.line_number);
    }

    pub fn decode(dec: &mut Decoder) -> Hessian2Result<Self> {
        let tag = dec.peek()?;
        if tag == crate::codec::tags::CLASS_DEF {
            dec.read_class_def()?;
        }

        let cd_idx = dec.read_object_begin()?;
        let class_def = dec
            .get_class_def(cd_idx)
            .ok_or(Hessian2Error::ReferenceNotFound { index: cd_idx })?;

        let field_names: Vec<String> = class_def.field_names.iter().map(|f| f.to_string()).collect();
        let _ = class_def;

        let mut declaring_class = String::new();
        let mut method_name = String::new();
        let mut file_name = String::new();
        let mut line_number = 0i32;

        for field_name in &field_names {
            match field_name.as_str() {
                "declaringClass" => declaring_class = dec.read_string()?,
                "methodName" => method_name = dec.read_string()?,
                "fileName" => file_name = dec.read_string()?,
                "lineNumber" => line_number = dec.read_int()?,
                other => {
                    return Err(Hessian2Error::InvalidTypeDescriptor {
                        desc: format!("unknown StackTraceElement field: {other}"),
                    });
                }
            }
        }

        Ok(Self {
            declaring_class,
            method_name,
            file_name,
            line_number,
        })
    }
}

// ── JavaException ──

/// Java-compatible exception representation for Hessian2 serialization.
///
/// Encoded as an object with class name matching the Java exception class
/// and fields: `detailMessage`, `stackTrace`, `cause`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaException {
    pub class_name: String,
    pub detail_message: String,
    pub stack_trace: Vec<StackTraceElement>,
    pub cause: Option<Box<JavaException>>,
}

impl JavaException {
    pub fn new(class_name: impl Into<String>, detail_message: impl Into<String>) -> Self {
        Self {
            class_name: class_name.into(),
            detail_message: detail_message.into(),
            stack_trace: Vec::new(),
            cause: None,
        }
    }

    pub fn with_stack_trace(mut self, trace: Vec<StackTraceElement>) -> Self {
        self.stack_trace = trace;
        self
    }

    pub fn with_cause(mut self, cause: JavaException) -> Self {
        self.cause = Some(Box::new(cause));
        self
    }

    pub fn encode(&self, enc: &mut Encoder) {
        let class_idx = enc.write_class_def(
            &self.class_name,
            &[
                "detailMessage".to_string(),
                "stackTrace".to_string(),
                "cause".to_string(),
            ],
        );
        enc.write_object_begin(class_idx);

        // detailMessage
        enc.write_string(&self.detail_message);

        // stackTrace: variable-length list of StackTraceElement
        enc.write_list_begin(self.stack_trace.len());
        for elem in &self.stack_trace {
            elem.encode(enc);
        }

        // cause: nullable object or null
        match &self.cause {
            Some(c) => c.encode(enc),
            None => enc.write_null(),
        }
    }

    pub fn decode(dec: &mut Decoder) -> Hessian2Result<Self> {
        let tag = dec.peek()?;
        if tag == crate::codec::tags::CLASS_DEF {
            dec.read_class_def()?;
        }

        let cd_idx = dec.read_object_begin()?;
        let class_def = dec
            .get_class_def(cd_idx)
            .ok_or(Hessian2Error::ReferenceNotFound { index: cd_idx })?;

        let class_name = class_def.class_name.to_string();
        let field_names: Vec<String> = class_def.field_names.iter().map(|f| f.to_string()).collect();
        let _ = class_def;

        let mut detail_message = String::new();
        let mut stack_trace = Vec::new();
        let mut cause: Option<Box<JavaException>> = None;

        for field_name in &field_names {
            match field_name.as_str() {
                "detailMessage" => detail_message = dec.read_string()?,
                "stackTrace" => {
                    let list_len = dec.read_list_begin()?;
                    stack_trace = Vec::with_capacity(list_len);
                    for _ in 0..list_len {
                        stack_trace.push(StackTraceElement::decode(dec)?);
                    }
                }
                "cause" => {
                    cause = if dec.peek().is_ok_and(|b| b == crate::codec::tags::NULL) {
                        dec.read_null()?;
                        None
                    } else {
                        Some(Box::new(Self::decode(dec)?))
                    };
                }
                other => {
                    return Err(Hessian2Error::InvalidTypeDescriptor {
                        desc: format!("unknown exception field: {other}"),
                    });
                }
            }
        }

        Ok(Self {
            class_name,
            detail_message,
            stack_trace,
            cause,
        })
    }
}

// ── BigDecimal ──

/// Java `java.math.BigDecimal` representation.
///
/// Hessian2 encodes BigDecimal as a map with entries:
/// - `scale`: int
/// - `value`: string (unscaled value in base 10)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BigDecimal {
    pub scale: i32,
    pub value: String,
}

impl BigDecimal {
    pub fn new(scale: i32, value: impl Into<String>) -> Self {
        Self {
            scale,
            value: value.into(),
        }
    }

    pub fn encode(&self, enc: &mut Encoder) {
        enc.write_map_begin();
        enc.write_string("scale");
        enc.write_int(self.scale);
        enc.write_string("value");
        enc.write_string(&self.value);
        enc.write_map_end();
    }

    pub fn decode(dec: &mut Decoder) -> Hessian2Result<Self> {
        let _is_typed = dec.read_map_begin()?;

        let mut scale = 0i32;
        let mut value = String::new();

        loop {
            let tag = dec.peek()?;
            if tag == b'Z' {
                dec.read_u8()?; // consume 'Z'
                break;
            }
            let key = dec.read_str()?;
            match key.as_ref() {
                "scale" => {
                    scale = dec.read_int()?;
                }
                "value" => {
                    value = dec.read_string()?;
                }
                _ => {
                    return Err(Hessian2Error::InvalidTypeDescriptor {
                        desc: format!("unexpected BigDecimal key: {key}"),
                    });
                }
            }
        }

        Ok(Self { scale, value })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_trace_element_roundtrip() {
        let elem =
            StackTraceElement::new("com.example.Service", "handleRequest", "Service.java", 42);

        let mut enc = Encoder::new();
        elem.encode(&mut enc);
        let bytes = enc.into_bytes();

        let mut dec = Decoder::new(&bytes);
        let decoded = StackTraceElement::decode(&mut dec).expect("decode");
        assert_eq!(decoded, elem);
    }

    #[test]
    fn test_java_exception_simple_roundtrip() {
        let ex = JavaException::new("java.lang.RuntimeException", "Something went wrong")
            .with_stack_trace(vec![
                StackTraceElement::new("com.example.App", "main", "App.java", 10),
                StackTraceElement::new("com.example.Runner", "run", "Runner.java", 25),
            ]);

        let mut enc = Encoder::new();
        ex.encode(&mut enc);
        let bytes = enc.into_bytes();

        let mut dec = Decoder::new(&bytes);
        let decoded = JavaException::decode(&mut dec).expect("decode");
        assert_eq!(decoded, ex);
    }

    #[test]
    fn test_java_exception_with_cause_roundtrip() {
        let cause =
            JavaException::new("java.io.IOException", "Connection refused").with_stack_trace(vec![
                StackTraceElement::new("java.net.Socket", "connect", "Socket.java", 100),
            ]);

        let ex = JavaException::new("java.lang.RuntimeException", "Wrapped error")
            .with_cause(cause)
            .with_stack_trace(vec![StackTraceElement::new(
                "com.example.Main",
                "start",
                "Main.java",
                15,
            )]);

        let mut enc = Encoder::new();
        ex.encode(&mut enc);
        let bytes = enc.into_bytes();

        let mut dec = Decoder::new(&bytes);
        let decoded = JavaException::decode(&mut dec).expect("decode");
        assert_eq!(decoded, ex);
    }

    #[test]
    fn test_java_exception_null_cause_roundtrip() {
        let ex = JavaException::new("java.lang.Error", "Fatal error");

        let mut enc = Encoder::new();
        ex.encode(&mut enc);
        let bytes = enc.into_bytes();

        let mut dec = Decoder::new(&bytes);
        let decoded = JavaException::decode(&mut dec).expect("decode");
        assert_eq!(decoded.cause, None);
    }

    #[test]
    fn test_big_decimal_roundtrip() {
        let bd = BigDecimal::new(2, "12345");

        let mut enc = Encoder::new();
        bd.encode(&mut enc);
        let bytes = enc.into_bytes();

        let mut dec = Decoder::new(&bytes);
        let decoded = BigDecimal::decode(&mut dec).expect("decode");
        assert_eq!(decoded, bd);
    }
}
