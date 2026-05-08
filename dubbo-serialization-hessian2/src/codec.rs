pub mod tags {
    pub const NULL: u8 = b'N';
    pub const TRUE: u8 = b'T';
    pub const FALSE: u8 = b'F';

    pub const INT: u8 = b'I';
    pub const LONG: u8 = b'L';
    pub const DOUBLE: u8 = b'D';

    pub const DATE: u8 = b'd';
    /// Compact date (minute precision, 4 bytes BE minutes since epoch)
    pub const DATE_MINUTE: u8 = 0x4b;

    pub const STRING: u8 = b'S';
    pub const BINARY: u8 = b'B';

    pub const REF: u8 = b'R';

    pub const LIST: u8 = b'V';
    /// Typed variable-length list: `U` type value* `Z`
    pub const LIST_VAR_TYPED: u8 = 0x55;
    /// Untyped variable-length list: `W` value* `Z`
    pub const LIST_VAR_UNTYPED: u8 = 0x57;
    /// Untyped fixed-length list: `X` int-length value*
    pub const LIST_FIXED_UNTYPED: u8 = 0x58;
    /// Compact untyped list base: 0x78 + length (0-7)
    pub const LIST_COMPACT_UNTYPED: u8 = 0x78;
    pub const MAP: u8 = b'M';
    pub const UNTYPED_MAP: u8 = b'H';

    pub const CLASS_DEF: u8 = b'C';
    pub const OBJECT: u8 = b'O';

    pub const COMPACT_INT_MIN: i32 = -16;
    pub const COMPACT_INT_MAX: i32 = 47;
    pub const COMPACT_INT_BASE: u8 = 0x90;

    pub fn is_compact_int(b: u8) -> bool {
        (0x80..=0xbf).contains(&b)
    }

    pub const COMPACT_LONG_MIN: i64 = -8;
    pub const COMPACT_LONG_MAX: i64 = 15;
    pub const COMPACT_LONG_BASE: u8 = 0xe0;

    pub fn is_compact_long(b: u8) -> bool {
        (0xd8..=0xef).contains(&b)
    }

    pub const DOUBLE_ZERO: u8 = 0x5b;
    pub const DOUBLE_ONE: u8 = 0x5c;
    pub const DOUBLE_BYTE: u8 = 0x5d;
    pub const DOUBLE_SHORT: u8 = 0x5e;
    pub const DOUBLE_FLOAT: u8 = 0x5f;

    pub const COMPACT_LONG_32: u8 = 0x59;

    pub const STRING_CHUNK_MAX_LEN: usize = 31;

    pub fn is_string_chunk(b: u8) -> bool {
        b <= 0x1f
    }
}

pub mod encoder {
    use bytes::BytesMut;
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};

    pub struct Encoder {
        buf: BytesMut,
        ref_map: HashMap<u64, usize>,
        next_ref: usize,
        class_defs: HashMap<String, usize>,
    }

    impl Default for Encoder {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Encoder {
        pub fn new() -> Self {
            Self {
                buf: BytesMut::new(),
                ref_map: HashMap::new(),
                next_ref: 0,
                class_defs: HashMap::new(),
            }
        }

        pub fn with_buffer(buf: BytesMut) -> Self {
            Self {
                buf,
                ref_map: HashMap::new(),
                next_ref: 0,
                class_defs: HashMap::new(),
            }
        }

        pub fn into_bytes(self) -> Vec<u8> {
            self.buf.to_vec()
        }

        pub fn as_bytes(&self) -> &[u8] {
            &self.buf
        }

        fn track_ref(&mut self, hash: u64) -> Option<usize> {
            self.ref_map.get(&hash).copied().or_else(|| {
                let idx = self.next_ref;
                self.ref_map.insert(hash, idx);
                self.next_ref += 1;
                None
            })
        }

        pub fn write_null(&mut self) {
            self.buf.extend_from_slice(&[super::tags::NULL]);
        }

        pub fn write_bool(&mut self, value: bool) {
            if value {
                self.buf.extend_from_slice(&[super::tags::TRUE]);
            } else {
                self.buf.extend_from_slice(&[super::tags::FALSE]);
            }
        }

        pub fn write_int(&mut self, value: i32) {
            use super::tags;
            if (tags::COMPACT_INT_MIN..=tags::COMPACT_INT_MAX).contains(&value) {
                self.buf
                    .extend_from_slice(&[(value + tags::COMPACT_INT_BASE as i32) as u8]);
            } else if (-2048..=2047).contains(&value) {
                let tag = (0xc8i32 + (value >> 8)) as u8;
                let low = (value & 0xff) as u8;
                self.buf.extend_from_slice(&[tag, low]);
            } else if (-262144..=262143).contains(&value) {
                let tag = 0xd0 + (((value >> 16) as u8) & 0x07);
                let b1 = ((value >> 8) & 0xff) as u8;
                let b0 = (value & 0xff) as u8;
                self.buf.extend_from_slice(&[tag, b1, b0]);
            } else {
                self.buf.extend_from_slice(&[tags::INT]);
                self.buf.extend_from_slice(&value.to_be_bytes());
            }
        }

        pub fn write_long(&mut self, value: i64) {
            use super::tags;
            if (tags::COMPACT_LONG_MIN..=tags::COMPACT_LONG_MAX).contains(&value) {
                self.buf
                    .extend_from_slice(&[(value + tags::COMPACT_LONG_BASE as i64) as u8]);
            } else if (-2048..=2047).contains(&value) {
                let tag = 0xf0 + (((value >> 8) as u8) & 0x0f);
                let low = (value & 0xff) as u8;
                self.buf.extend_from_slice(&[tag, low]);
            } else if (-262144..=262143).contains(&value) {
                let tag = 0x38 + (((value >> 16) as u8) & 0x07);
                let b1 = ((value >> 8) & 0xff) as u8;
                let b0 = (value & 0xff) as u8;
                self.buf.extend_from_slice(&[tag, b1, b0]);
            } else if value >= i32::MIN as i64 && value <= i32::MAX as i64 {
                self.buf.extend_from_slice(&[tags::COMPACT_LONG_32]);
                self.buf.extend_from_slice(&(value as i32).to_be_bytes());
            } else {
                self.buf.extend_from_slice(&[tags::LONG]);
                self.buf.extend_from_slice(&value.to_be_bytes());
            }
        }

        pub fn write_double(&mut self, value: f64) {
            use super::tags;
            if value == 0.0 {
                self.buf.extend_from_slice(&[tags::DOUBLE_ZERO]);
            } else if value == 1.0 {
                self.buf.extend_from_slice(&[tags::DOUBLE_ONE]);
            } else {
                let int_val = value as i64;
                if value == int_val as f64 && (-128..=127).contains(&int_val) {
                    self.buf.extend_from_slice(&[tags::DOUBLE_BYTE]);
                    self.buf.extend_from_slice(&(int_val as i8).to_be_bytes());
                } else if value == int_val as f64 && (-32768..=32767).contains(&int_val) {
                    self.buf.extend_from_slice(&[tags::DOUBLE_SHORT]);
                    self.buf.extend_from_slice(&(int_val as i16).to_be_bytes());
                } else {
                    let float_val = value as f32;
                    if value == float_val as f64 {
                        self.buf.extend_from_slice(&[tags::DOUBLE_FLOAT]);
                        self.buf
                            .extend_from_slice(&float_val.to_bits().to_be_bytes());
                    } else {
                        self.buf.extend_from_slice(&[tags::DOUBLE]);
                        self.buf.extend_from_slice(&value.to_bits().to_be_bytes());
                    }
                }
            }
        }

        pub fn write_date(&mut self, millis: i64) {
            use super::tags;
            self.buf.extend_from_slice(&[tags::DATE]);
            self.buf.extend_from_slice(&millis.to_be_bytes());
        }

        pub fn write_string(&mut self, value: &str) {
            use super::tags;
            let data = value.as_bytes();
            let char_count = value.chars().count();
            if char_count <= tags::STRING_CHUNK_MAX_LEN {
                let tag = char_count as u8;
                self.buf.extend_from_slice(&[tag]);
                self.buf.extend_from_slice(data);
            } else {
                self.buf.extend_from_slice(&[tags::STRING]);
                self.buf
                    .extend_from_slice(&[(char_count >> 8) as u8, (char_count & 0xff) as u8]);
                self.buf.extend_from_slice(data);
            }
        }

        pub fn write_binary(&mut self, value: &[u8]) {
            let len = value.len();
            if len <= 15 {
                self.buf.extend_from_slice(&[(0x20 + len as u8)]);
            } else {
                self.buf
                    .extend_from_slice(&[0x42, (len >> 8) as u8, (len & 0xff) as u8]);
            }
            self.buf.extend_from_slice(value);
        }

        // ========== list ==========

        /// Write an untyped list header.
        ///
        /// For small lists (<= 7 elements), uses compact encoding (0x78 + count).
        /// For larger lists, uses fixed-length untyped encoding (0x58 + int count).
        pub fn write_list_begin(&mut self, count: usize) {
            use super::tags;
            if count <= 7 {
                // Compact untyped fixed-length list
                self.buf
                    .extend_from_slice(&[(tags::LIST_COMPACT_UNTYPED + count as u8)]);
            } else {
                // Fixed-length untyped list: 0x58 + int
                self.buf.extend_from_slice(&[tags::LIST_FIXED_UNTYPED]);
                self.write_int(count as i32);
            }
        }

        // ========== map ==========

        /// Write an untyped map header (`H`).
        ///
        /// Untyped maps have no type string.
        /// Entries follow as key-value pairs, terminated by `Z`.
        pub fn write_map_begin(&mut self) {
            use super::tags;
            self.buf.extend_from_slice(&[tags::UNTYPED_MAP]);
        }

        /// Write a map/collection end marker (`Z`).
        ///
        /// Used to terminate maps and variable-length lists.
        pub fn write_map_end(&mut self) {
            self.buf.extend_from_slice(b"Z");
        }

        // ========== class definition and object ==========

        /// Write a class definition (`C` tag).
        ///
        /// Format: `C` [class-name-as-string] [field-count-as-int] [field-name-n]...
        ///
        /// Returns the index of this class definition (for use with `write_object`).
        /// If the same class was already defined, returns the existing index.
        pub fn write_class_def(&mut self, class_name: &str, field_names: &[String]) -> usize {
            if let Some(&idx) = self.class_defs.get(class_name) {
                return idx;
            }

            let idx = self.class_defs.len();
            self.class_defs.insert(class_name.to_string(), idx);

            self.buf.extend_from_slice(&[super::tags::CLASS_DEF]);
            self.write_string(class_name);
            self.write_int(field_names.len() as i32);
            for field in field_names {
                self.write_string(field);
            }
            idx
        }

        /// Write an object instance (`O` tag).
        ///
        /// Format: `O` [class-def-index-as-int] [field-value-1] [field-value-2] ...
        ///
        /// The caller is responsible for writing field values **after** calling
        /// this method. The field values must be written in the same order as
        /// the field names in the class definition.
        pub fn write_object_begin(&mut self, class_def_index: usize) {
            self.buf.extend_from_slice(&[super::tags::OBJECT]);
            self.write_int(class_def_index as i32);
        }

        // ========== reference ==========

        /// Write a shared reference to a previously encoded value.
        ///
        /// Format: `R` [index-as-int]
        pub fn write_ref(&mut self, index: usize) {
            use super::tags;
            self.buf.extend_from_slice(&[tags::REF]);
            self.write_int(index as i32);
        }

        /// Write a string with reference deduplication.
        ///
        /// If the same string content has been written before, writes a `R` reference
        /// instead of the full string. Otherwise, writes the full string and stores
        /// its content hash for future reference.
        pub fn write_string_ref(&mut self, value: &str) {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            value.hash(&mut hasher);
            let hash = hasher.finish();

            if let Some(idx) = self.track_ref(hash) {
                self.write_ref(idx);
            } else {
                self.write_string(value);
            }
        }
    }
}
