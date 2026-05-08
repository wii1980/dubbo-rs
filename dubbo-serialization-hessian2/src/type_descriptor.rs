//! Java type descriptor parser.
//!
//! Parses Java JVM type descriptors as used in Dubbo protocol bodies:
//!
//! | Descriptor | Meaning |
//! |------------|---------|
//! | `B` | `byte` |
//! | `C` | `char` |
//! | `D` | `double` |
//! | `F` | `float` |
//! | `I` | `int` |
//! | `J` | `long` |
//! | `S` | `short` |
//! | `Z` | `boolean` |
//! | `V` | `void` |
//! | `Lcom/example/Foo;` | Object type `com.example.Foo` |
//! | `[B` | `byte[]` |
//! | `[Ljava/lang/String;` | `String[]` |

use crate::types::{Hessian2Error, Hessian2Result};

/// Represents a parsed Java type descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum JavaType {
    /// `B` — primitive byte
    Byte,
    /// `C` — primitive char
    Char,
    /// `D` — primitive double
    Double,
    /// `F` — primitive float
    Float,
    /// `I` — primitive int
    Int,
    /// `J` — primitive long
    Long,
    /// `S` — primitive short
    Short,
    /// `Z` — primitive boolean
    Boolean,
    /// `V` — void (return type)
    Void,
    /// `Ljava/lang/String;` — object reference
    Object(String),
    /// `[B`, `[I`, `[Ljava/lang/String;` — array of element type
    Array(Box<JavaType>),
}

impl std::fmt::Display for JavaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Byte => f.write_str("B"),
            Self::Char => f.write_str("C"),
            Self::Double => f.write_str("D"),
            Self::Float => f.write_str("F"),
            Self::Int => f.write_str("I"),
            Self::Long => f.write_str("J"),
            Self::Short => f.write_str("S"),
            Self::Boolean => f.write_str("Z"),
            Self::Void => f.write_str("V"),
            Self::Object(name) => write!(f, "L{name};"),
            Self::Array(elem) => write!(f, "[{elem}"),
        }
    }
}

/// Parse a single Java type descriptor from a string.
///
/// Returns the parsed type and the number of characters consumed.
///
/// # Examples
///
/// ```
/// # use dubbo_rs_serialization_hessian2::type_descriptor::{JavaType, parse_type_descriptor};
/// let (t, n) = parse_type_descriptor("I").unwrap();
/// assert_eq!(t, JavaType::Int);
/// assert_eq!(n, 1);
///
/// let (t, n) = parse_type_descriptor("Ljava/lang/String;").unwrap();
/// assert_eq!(t, JavaType::Object("java/lang/String".into()));
/// assert_eq!(n, 18);
/// ```
///
/// # Errors
///
/// Returns `InvalidTypeDescriptor` if the descriptor is malformed.
pub fn parse_type_descriptor(s: &str) -> Hessian2Result<(JavaType, usize)> {
    parse_descriptor_helper(s, 0)
}

/// Parse a single type descriptor starting at `offset`.
fn parse_descriptor_helper(s: &str, offset: usize) -> Hessian2Result<(JavaType, usize)> {
    let bytes = s.as_bytes();
    if offset >= bytes.len() {
        return Err(Hessian2Error::InvalidTypeDescriptor {
            desc: s.to_string(),
        });
    }
    match bytes[offset] {
        b'B' => Ok((JavaType::Byte, offset + 1)),
        b'C' => Ok((JavaType::Char, offset + 1)),
        b'D' => Ok((JavaType::Double, offset + 1)),
        b'F' => Ok((JavaType::Float, offset + 1)),
        b'I' => Ok((JavaType::Int, offset + 1)),
        b'J' => Ok((JavaType::Long, offset + 1)),
        b'S' => Ok((JavaType::Short, offset + 1)),
        b'Z' => Ok((JavaType::Boolean, offset + 1)),
        b'V' => Ok((JavaType::Void, offset + 1)),
        b'L' => {
            let end =
                s[offset..]
                    .find(';')
                    .ok_or_else(|| Hessian2Error::InvalidTypeDescriptor {
                        desc: s.to_string(),
                    })?;
            let class_name = s[offset + 1..offset + end].to_string();
            Ok((JavaType::Object(class_name), offset + end + 1))
        }
        b'[' => {
            let (elem, after_elem) = parse_descriptor_helper(s, offset + 1)?;
            Ok((JavaType::Array(Box::new(elem)), after_elem))
        }
        b => Err(Hessian2Error::InvalidTypeDescriptor {
            desc: format!("unexpected byte 0x{b:02x} in type descriptor"),
        }),
    }
}

/// Parse a method descriptor `(ArgTypes)ReturnType`.
///
/// Returns `(argument_types, return_type)`.
///
/// # Examples
///
/// ```
/// # use dubbo_rs_serialization_hessian2::type_descriptor::{JavaType, parse_method_descriptor};
/// let (args, ret) = parse_method_descriptor("(Ljava/lang/String;I)V").unwrap();
/// assert_eq!(args.len(), 2);
/// assert_eq!(args[0], JavaType::Object("java/lang/String".into()));
/// assert_eq!(args[1], JavaType::Int);
/// assert_eq!(ret, JavaType::Void);
/// ```
pub fn parse_method_descriptor(s: &str) -> Hessian2Result<(Vec<JavaType>, JavaType)> {
    let bytes = s.as_bytes();
    if bytes.is_empty() || bytes[0] != b'(' {
        return Err(Hessian2Error::InvalidTypeDescriptor {
            desc: s.to_string(),
        });
    }

    let paren_end = s
        .find(')')
        .ok_or_else(|| Hessian2Error::InvalidTypeDescriptor {
            desc: s.to_string(),
        })?;

    let mut args = Vec::new();
    let mut pos = 1;
    while pos < paren_end {
        let (ty, next) = parse_descriptor_helper(s, pos)?;
        args.push(ty);
        pos = next;
    }

    let (ret, _) = parse_descriptor_helper(s, paren_end + 1)?;

    Ok((args, ret))
}

/// Convert a Java class name to its internal descriptor format.
///
/// `com.example.Foo` → `Lcom/example/Foo;`
#[must_use]
pub fn class_name_to_descriptor(class_name: &str) -> String {
    let internal = class_name.replace('.', "/");
    format!("L{internal};")
}

/// Convert a internal descriptor to a Java class name.
///
/// `Lcom/example/Foo;` → `com.example.Foo`
///
/// # Errors
///
/// Returns `InvalidTypeDescriptor` if not a valid object descriptor.
pub fn descriptor_to_class_name(desc: &str) -> Hessian2Result<String> {
    let bytes = desc.as_bytes();
    if bytes.is_empty() || bytes[0] != b'L' || bytes[bytes.len() - 1] != b';' {
        return Err(Hessian2Error::InvalidTypeDescriptor {
            desc: desc.to_string(),
        });
    }
    let inner = &desc[1..desc.len() - 1];
    Ok(inner.replace('/', "."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_primitive() {
        let cases = [
            ("B", JavaType::Byte),
            ("C", JavaType::Char),
            ("D", JavaType::Double),
            ("F", JavaType::Float),
            ("I", JavaType::Int),
            ("J", JavaType::Long),
            ("S", JavaType::Short),
            ("Z", JavaType::Boolean),
            ("V", JavaType::Void),
        ];
        for (input, expected) in &cases {
            let (t, n) = parse_type_descriptor(input).unwrap();
            assert_eq!(&t, expected);
            assert_eq!(n, 1);
        }
    }

    #[test]
    fn test_parse_object() {
        let (t, n) = parse_type_descriptor("Ljava/lang/String;").unwrap();
        assert_eq!(t, JavaType::Object("java/lang/String".into()));
        assert_eq!(n, 18);

        let (t, n) = parse_type_descriptor("Lcom/example/Foo;").unwrap();
        assert_eq!(t, JavaType::Object("com/example/Foo".into()));
        assert_eq!(n, 17);
    }

    #[test]
    fn test_parse_array_primitive() {
        let (t, n) = parse_type_descriptor("[B").unwrap();
        assert_eq!(t, JavaType::Array(Box::new(JavaType::Byte)));
        assert_eq!(n, 2);

        let (t, n) = parse_type_descriptor("[I").unwrap();
        assert_eq!(t, JavaType::Array(Box::new(JavaType::Int)));
        assert_eq!(n, 2);
    }

    #[test]
    fn test_parse_array_object() {
        let (t, n) = parse_type_descriptor("[Ljava/lang/String;").unwrap();
        assert_eq!(
            t,
            JavaType::Array(Box::new(JavaType::Object("java/lang/String".into())))
        );
        assert_eq!(n, 19);
    }

    #[test]
    fn test_parse_multi_dimensional_array() {
        let (t, n) = parse_type_descriptor("[[I").unwrap();
        assert_eq!(
            t,
            JavaType::Array(Box::new(JavaType::Array(Box::new(JavaType::Int))))
        );
        assert_eq!(n, 3);

        let (t, n) = parse_type_descriptor("[[[B").unwrap();
        assert_eq!(
            t,
            JavaType::Array(Box::new(JavaType::Array(Box::new(JavaType::Array(
                Box::new(JavaType::Byte)
            )))))
        );
        assert_eq!(n, 4);
    }

    #[test]
    fn test_parse_invalid_descriptor() {
        assert!(parse_type_descriptor("X").is_err());
        assert!(parse_type_descriptor("Lno_semicolon").is_err());
        assert!(parse_type_descriptor("").is_err());
    }

    #[test]
    fn test_parse_method_descriptor_simple() {
        let (args, ret) = parse_method_descriptor("()V").unwrap();
        assert!(args.is_empty());
        assert_eq!(ret, JavaType::Void);
    }

    #[test]
    fn test_parse_method_descriptor_with_args() {
        let (args, ret) = parse_method_descriptor("(Ljava/lang/String;I)V").unwrap();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], JavaType::Object("java/lang/String".into()));
        assert_eq!(args[1], JavaType::Int);
        assert_eq!(ret, JavaType::Void);
    }

    #[test]
    fn test_parse_method_descriptor_complex() {
        let (args, ret) = parse_method_descriptor("(IB[D[Ljava/lang/String;)J").unwrap();
        assert_eq!(args.len(), 4);
        assert_eq!(args[0], JavaType::Int);
        assert_eq!(args[1], JavaType::Byte);
        assert_eq!(args[2], JavaType::Array(Box::new(JavaType::Double)));
        assert_eq!(
            args[3],
            JavaType::Array(Box::new(JavaType::Object("java/lang/String".into())))
        );
        assert_eq!(ret, JavaType::Long);
    }

    #[test]
    fn test_invalid_method_descriptor() {
        assert!(parse_method_descriptor("IV").is_err());
        assert!(parse_method_descriptor("(").is_err());
    }

    #[test]
    fn test_display_roundtrip() {
        let cases = [
            "I",
            "J",
            "Ljava/lang/String;",
            "[B",
            "[[I",
            "Lcom/example/Foo;",
        ];
        for desc in &cases {
            let (t, _) = parse_type_descriptor(desc).unwrap();
            assert_eq!(t.to_string(), *desc);
        }
    }

    #[test]
    fn test_class_name_to_descriptor() {
        assert_eq!(
            class_name_to_descriptor("com.example.Foo"),
            "Lcom/example/Foo;"
        );
        assert_eq!(
            class_name_to_descriptor("java.lang.String"),
            "Ljava/lang/String;"
        );
    }

    #[test]
    fn test_descriptor_to_class_name() {
        assert_eq!(
            descriptor_to_class_name("Lcom/example/Foo;").unwrap(),
            "com.example.Foo"
        );
        assert_eq!(
            descriptor_to_class_name("Ljava/lang/String;").unwrap(),
            "java.lang.String"
        );
        assert!(descriptor_to_class_name("I").is_err());
        assert!(descriptor_to_class_name("Lno_semicolon").is_err());
    }
}
