use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use crate::codec::encoder::Encoder;
use crate::decoder::Decoder;
use crate::types::{Hessian2Error, Hessian2Result};

/// Metadata for a registered Hessian2 class.
pub struct ClassInfo {
    pub class_name: String,
    pub field_names: Vec<String>,
}

/// Registry of Hessian2 class types.
///
/// Maps class names to their field schema, enabling automatic encoding
/// of class definitions and reading of object field values.
pub struct TypeRegistry {
    classes: HashMap<String, ClassInfo>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        Self {
            classes: HashMap::new(),
        }
    }

    /// Register a class with its Hessian2 class name and field names.
    ///
    /// The field names must match the order in which field values are
    /// written/read.
    pub fn register(&mut self, class_name: impl Into<String>, field_names: Vec<String>) {
        let name: String = class_name.into();
        self.classes.insert(
            name.clone(),
            ClassInfo {
                class_name: name,
                field_names,
            },
        );
    }

    /// Get class info by name.
    #[must_use]
    pub fn get(&self, class_name: &str) -> Option<&ClassInfo> {
        self.classes.get(class_name)
    }

    /// Encode a registered object: writes the class definition and object header,
    /// then calls `write_fields` to write individual field values.
    pub fn encode_object(
        &self,
        enc: &mut Encoder,
        class_name: &str,
        write_fields: impl FnOnce(&mut Encoder),
    ) -> Hessian2Result<()> {
        let info = self
            .get(class_name)
            .ok_or_else(|| Hessian2Error::InvalidTypeDescriptor {
                desc: format!("unregistered class: {class_name}"),
            })?;
        let class_idx = enc.write_class_def(&info.class_name, &info.field_names);
        enc.write_object_begin(class_idx);
        write_fields(enc);
        Ok(())
    }

    /// Decode an object: reads the class definition and object header,
    /// then calls `read_fields` to read individual field values.
    pub fn decode_object<T>(
        &self,
        dec: &mut Decoder,
        read_fields: impl FnOnce(&mut Decoder, &ClassInfo) -> Hessian2Result<T>,
    ) -> Hessian2Result<T> {
        let cd_idx = dec.read_class_def()?;
        let obj_idx = dec.read_object_begin()?;

        if cd_idx != obj_idx {
            return Err(Hessian2Error::InvalidTypeDescriptor {
                desc: format!("class def index mismatch: class_def={cd_idx}, object={obj_idx}"),
            });
        }

        let class_def = dec
            .get_class_def(cd_idx)
            .ok_or(Hessian2Error::ReferenceNotFound { index: cd_idx })?;

        let class_name = &class_def.class_name;
        let info = self
            .get(class_name)
            .ok_or_else(|| Hessian2Error::InvalidTypeDescriptor {
                desc: format!("unknown class from definition: {class_name}"),
            })?;

        read_fields(dec, info)
    }
}

impl Default for TypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe global type registry.
pub static GLOBAL_REGISTRY: LazyLock<RwLock<TypeRegistry>> =
    LazyLock::new(|| RwLock::new(TypeRegistry::new()));

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::encoder::Encoder;
    use crate::decoder::Decoder;

    #[test]
    fn test_register_and_get() {
        let mut reg = TypeRegistry::new();
        reg.register("com.example.User", vec!["name".into(), "age".into()]);

        let info = reg.get("com.example.User").expect("get User");
        assert_eq!(info.class_name, "com.example.User");
        assert_eq!(info.field_names, vec!["name", "age"]);
    }

    #[test]
    fn test_unregistered_get_returns_none() {
        let reg = TypeRegistry::new();
        assert!(reg.get("com.example.Unknown").is_none());
    }

    #[test]
    fn test_encode_decode_registered_object_roundtrip() {
        let mut reg = TypeRegistry::new();
        reg.register("com.example.Person", vec!["name".into(), "age".into()]);

        let mut enc = Encoder::new();
        reg.encode_object(&mut enc, "com.example.Person", |enc| {
            enc.write_string("Alice");
            enc.write_int(30);
        })
        .expect("encode");

        let bytes = enc.into_bytes();
        let mut dec = Decoder::new(&bytes);
        let result = reg
            .decode_object(&mut dec, |dec, info| {
                assert_eq!(info.class_name, "com.example.Person");
                let name = dec.read_string()?;
                let age = dec.read_int()?;
                Ok((name, age))
            })
            .expect("decode");

        assert_eq!(result, ("Alice".to_string(), 30));
    }

    #[test]
    fn test_encode_unregistered_class_fails() {
        let reg = TypeRegistry::new();
        let mut enc = Encoder::new();
        let err = reg
            .encode_object(&mut enc, "com.example.Unknown", |_| {})
            .unwrap_err();
        assert!(format!("{err}").contains("unregistered"));
    }
}
