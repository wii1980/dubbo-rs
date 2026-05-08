use std::borrow::Cow;

use crate::class_def::ClassDef;
use crate::codec::tags;
use crate::types::{Hessian2Error, Hessian2Result};

pub struct Decoder<'a> {
    buf: &'a [u8],
    pos: usize,
    refs: Vec<Cow<'a, str>>,
    class_defs: Vec<ClassDef<'a>>,
}

impl<'a> Decoder<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self {
            buf,
            pos: 0,
            refs: Vec::new(),
            class_defs: Vec::new(),
        }
    }

    pub fn peek(&self) -> Hessian2Result<u8> {
        self.buf
            .get(self.pos)
            .copied()
            .ok_or(Hessian2Error::UnexpectedEof { pos: self.pos })
    }

    pub fn read_u8(&mut self) -> Hessian2Result<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Ok(b)
    }

    fn read_slice(&mut self, n: usize) -> Hessian2Result<&'a [u8]> {
        if self.pos + n > self.buf.len() {
            return Err(Hessian2Error::UnexpectedEof { pos: self.pos });
        }
        let slice = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn read_i32be(&mut self) -> Hessian2Result<i32> {
        let bytes: [u8; 4] = self
            .read_slice(4)?
            .try_into()
            .expect("read_slice(4) returns exactly 4 bytes");
        Ok(i32::from_be_bytes(bytes))
    }

    fn read_i64be(&mut self) -> Hessian2Result<i64> {
        let bytes: [u8; 8] = self
            .read_slice(8)?
            .try_into()
            .expect("read_slice(8) returns exactly 8 bytes");
        Ok(i64::from_be_bytes(bytes))
    }

    fn read_f64be(&mut self) -> Hessian2Result<f64> {
        let bytes: [u8; 8] = self
            .read_slice(8)?
            .try_into()
            .expect("read_slice(8) returns exactly 8 bytes");
        Ok(f64::from_be_bytes(bytes))
    }

    fn read_f32be_bits(&mut self) -> Hessian2Result<u32> {
        let bytes: [u8; 4] = self
            .read_slice(4)?
            .try_into()
            .expect("read_slice(4) returns exactly 4 bytes");
        Ok(u32::from_be_bytes(bytes))
    }

    fn read_string_chars(&mut self, count: usize) -> Hessian2Result<Cow<'a, str>> {
        let start = self.pos;
        let mut remaining = count;
        let mut has_surrogate = false;

        while remaining > 0 {
            if self.pos >= self.buf.len() {
                return Err(Hessian2Error::UnexpectedEof { pos: self.pos });
            }

            let b0 = self.buf[self.pos];
            if b0 < 0xC0 {
                self.pos += 1;
                remaining -= 1;
            } else if b0 < 0xE0 {
                if self.pos + 2 > self.buf.len() {
                    return Err(Hessian2Error::UnexpectedEof { pos: self.pos });
                }
                self.pos += 2;
                remaining -= 1;
            } else if b0 < 0xF0 {
                if b0 == 0xED
                    && self.pos + 6 <= self.buf.len()
                    && (0xA0..=0xAF).contains(&self.buf[self.pos + 1])
                    && self.buf[self.pos + 3] == 0xED
                    && (0xB0..=0xBF).contains(&self.buf[self.pos + 4])
                {
                    has_surrogate = true;
                    self.pos += 6;
                    remaining = remaining.saturating_sub(2);
                } else {
                    if self.pos + 3 > self.buf.len() {
                        return Err(Hessian2Error::UnexpectedEof { pos: self.pos });
                    }
                    self.pos += 3;
                    remaining -= 1;
                }
            } else {
                if self.pos + 4 > self.buf.len() {
                    return Err(Hessian2Error::UnexpectedEof { pos: self.pos });
                }
                self.pos += 4;
                remaining -= 1;
            }
        }

        if !has_surrogate {
            let s = std::str::from_utf8(&self.buf[start..self.pos]).map_err(|e| {
                Hessian2Error::InvalidUtf8 {
                    pos: start,
                    source: e,
                }
            })?;
            return Ok(Cow::Borrowed(s));
        }

        Ok(Cow::Owned(self.decode_string_with_surrogates(start, count)))
    }

    /// Decode a UTF-8 string that contains surrogate pairs (CESU-8 encoded).
    fn decode_string_with_surrogates(
        &mut self,
        start: usize,
        count: usize,
    ) -> String {
        self.pos = start;
        let mut result = String::with_capacity(self.pos - start);
        let mut rem = count;

        while rem > 0 {
            let b0 = self.buf[self.pos];
            if b0 < 0xC0 {
                result.push(self.buf[self.pos] as char);
                self.pos += 1;
                rem -= 1;
            } else if b0 < 0xE0 {
                let slice = &self.buf[self.pos..self.pos + 2];
                result.push_str(std::str::from_utf8(slice).unwrap());
                self.pos += 2;
                rem -= 1;
            } else if b0 < 0xF0 {
                let sp = b0 == 0xED
                    && self.pos + 6 <= self.buf.len()
                    && (0xA0..=0xAF).contains(&self.buf[self.pos + 1])
                    && self.buf[self.pos + 3] == 0xED
                    && (0xB0..=0xBF).contains(&self.buf[self.pos + 4]);
                if sp {
                    let hi = (u32::from(self.buf[self.pos] & 0x0F) << 12)
                        | (u32::from(self.buf[self.pos + 1] & 0x3F) << 6)
                        | u32::from(self.buf[self.pos + 2] & 0x3F);
                    let lo = (u32::from(self.buf[self.pos + 3] & 0x0F) << 12)
                        | (u32::from(self.buf[self.pos + 4] & 0x3F) << 6)
                        | u32::from(self.buf[self.pos + 5] & 0x3F);
                    let cp = 0x1_0000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                    if let Some(ch) = char::from_u32(cp) {
                        result.push(ch);
                    }
                    self.pos += 6;
                    rem = rem.saturating_sub(2);
                } else {
                    let slice = &self.buf[self.pos..self.pos + 3];
                    result.push_str(std::str::from_utf8(slice).unwrap());
                    self.pos += 3;
                    rem -= 1;
                }
            } else {
                let slice = &self.buf[self.pos..self.pos + 4];
                result.push_str(std::str::from_utf8(slice).unwrap());
                self.pos += 4;
                rem -= 1;
            }
        }

        result
    }

    pub fn read_null(&mut self) -> Hessian2Result<()> {
        let tag = self.read_u8()?;
        if tag == tags::NULL {
            Ok(())
        } else {
            Err(Hessian2Error::TypeMismatch {
                expected: "null".into(),
                got: format!("0x{tag:02x}"),
                pos: self.pos - 1,
            })
        }
    }

    pub fn read_bool(&mut self) -> Hessian2Result<bool> {
        let tag = self.read_u8()?;
        match tag {
            tags::TRUE => Ok(true),
            tags::FALSE => Ok(false),
            _ => Err(Hessian2Error::TypeMismatch {
                expected: "boolean".into(),
                got: format!("0x{tag:02x}"),
                pos: self.pos - 1,
            }),
        }
    }

    pub fn read_int(&mut self) -> Hessian2Result<i32> {
        let tag = self.read_u8()?;
        if tags::is_compact_int(tag) {
            Ok(tag as i32 - tags::COMPACT_INT_BASE as i32)
        } else if (0xc0..=0xcf).contains(&tag) {
            let low = self.read_u8()? as i32;
            let high = tag as i32 - 0xc8;
            let val = (high << 8) | low;
            Ok(if val >= 0x800 { val - 0x1000 } else { val })
        } else if (0xd0..=0xd7).contains(&tag) {
            let b1 = self.read_u8()? as i32;
            let b0 = self.read_u8()? as i32;
            let high = (tag - 0xd0) as i32;
            let val = (high << 16) | (b1 << 8) | b0;
            Ok(if val >= 0x40000 { val - 0x80000 } else { val })
        } else if tag == tags::INT {
            self.read_i32be()
        } else {
            Err(Hessian2Error::TypeMismatch {
                expected: "int".into(),
                got: format!("0x{tag:02x}"),
                pos: self.pos - 1,
            })
        }
    }

    pub fn read_long(&mut self) -> Hessian2Result<i64> {
        let tag = self.read_u8()?;
        if tags::is_compact_long(tag) {
            Ok(tag as i64 - tags::COMPACT_LONG_BASE as i64)
        } else if (0xf0..=0xff).contains(&tag) {
            let low = self.read_u8()? as i64;
            let high = (tag - 0xf0) as i64;
            let val = (high << 8) | low;
            Ok(if val >= 0x800 { val - 0x1000 } else { val })
        } else if (0x38..=0x3f).contains(&tag) {
            let b1 = self.read_u8()? as i64;
            let b0 = self.read_u8()? as i64;
            let high = (tag - 0x38) as i64;
            let val = (high << 16) | (b1 << 8) | b0;
            Ok(if val >= 0x40000 { val - 0x80000 } else { val })
        } else if tag == tags::COMPACT_LONG_32 {
            Ok(self.read_i32be()? as i64)
        } else if tag == tags::LONG {
            self.read_i64be()
        } else {
            Err(Hessian2Error::TypeMismatch {
                expected: "long".into(),
                got: format!("0x{tag:02x}"),
                pos: self.pos - 1,
            })
        }
    }

    pub fn read_double(&mut self) -> Hessian2Result<f64> {
        let tag = self.read_u8()?;
        match tag {
            tags::DOUBLE_ZERO => Ok(0.0),
            tags::DOUBLE_ONE => Ok(1.0),
            tags::DOUBLE_BYTE => {
                let b = self.read_u8()? as i8;
                Ok(b as f64)
            }
            tags::DOUBLE_SHORT => {
                let bytes: [u8; 2] = self
                    .read_slice(2)?
                    .try_into()
                    .expect("read_slice(2) returns exactly 2 bytes");
                let val = i16::from_be_bytes(bytes);
                Ok(val as f64)
            }
            tags::DOUBLE_FLOAT => {
                let bits = self.read_f32be_bits()?;
                Ok(f32::from_bits(bits) as f64)
            }
            tags::DOUBLE => self.read_f64be(),
            _ => Err(Hessian2Error::TypeMismatch {
                expected: "double".into(),
                got: format!("0x{tag:02x}"),
                pos: self.pos - 1,
            }),
        }
    }

    pub fn read_date(&mut self) -> Hessian2Result<i64> {
        let tag = self.read_u8()?;
        if tag == tags::DATE {
            self.read_i64be()
        } else if tag == tags::DATE_MINUTE {
            // Compact date: 'K' + 4-byte BE minutes since epoch
            let bytes: [u8; 4] = self.read_slice(4)?.try_into().expect("read_slice(4)");
            let minutes = i32::from_be_bytes(bytes) as i64;
            Ok(minutes * 60_000)
        } else {
            Err(Hessian2Error::TypeMismatch {
                expected: "date".into(),
                got: format!("0x{tag:02x}"),
                pos: self.pos - 1,
            })
        }
    }

    pub fn read_str(&mut self) -> Hessian2Result<Cow<'a, str>> {
        let tag = self.peek()?;
        if tags::is_string_chunk(tag) {
            self.pos += 1;
            let char_count = tag as usize;
            if char_count == 0 {
                let s = Cow::Borrowed("");
                self.refs.push(s.clone());
                return Ok(s);
            }
            let s = self.read_string_chars(char_count)?;
            self.refs.push(s.clone());
            Ok(s)
        } else if tag == 0x30 || tag == 0x31 || tag == 0x32 || tag == 0x33 {
            self.pos += 1;
            let length = match tag {
                0x30 => self.read_u8()? as usize,
                0x31 => {
                    let bytes: [u8; 2] = self.read_slice(2)?.try_into().expect("read_slice(2)");
                    u16::from_be_bytes(bytes) as usize
                }
                0x32 => {
                    let bytes: [u8; 3] = self.read_slice(3)?.try_into().expect("read_slice(3)");
                    (u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]) & 0x00FF_FFFF) as usize
                }
                _ => unreachable!(),
            };
            let data = self.read_slice(length)?;
            let s = std::str::from_utf8(data)
                .map(Cow::Borrowed)
                .map_err(|e| Hessian2Error::InvalidUtf8 {
                    pos: self.pos - length,
                    source: e,
                })?;
            self.refs.push(s.clone());
            Ok(s)
        } else if tag == tags::STRING {
            self.pos += 1;
            let b0 = self.read_u8()? as usize;
            let b1 = self.read_u8()? as usize;
            let char_count = (b0 << 8) | b1;
            let s = self.read_string_chars(char_count)?;
            self.refs.push(s.clone());
            Ok(s)
        } else {
            Err(Hessian2Error::TypeMismatch {
                expected: "string".into(),
                got: format!("0x{tag:02x}"),
                pos: self.pos,
            })
        }
    }

    pub fn read_string(&mut self) -> Hessian2Result<String> {
        Ok(self.read_str()?.into_owned())
    }

    pub fn read_ref(&mut self) -> Hessian2Result<Cow<'a, str>> {
        let tag = self.read_u8()?;
        if tag != tags::REF {
            return Err(Hessian2Error::TypeMismatch {
                expected: "ref".into(),
                got: format!("0x{tag:02x}"),
                pos: self.pos - 1,
            });
        }
        let index = self.read_int()? as usize;
        self.refs
            .get(index)
            .cloned()
            .ok_or(Hessian2Error::ReferenceNotFound { index })
    }

    pub fn read_binary(&mut self) -> Hessian2Result<Vec<u8>> {
        let tag = self.read_u8()?;
        if tag == tags::BINARY {
            let b0 = self.read_u8()? as usize;
            let b1 = self.read_u8()? as usize;
            let len = (b0 << 8) | b1;
            Ok(self.read_slice(len)?.to_vec())
        } else if (0x20..=0x2f).contains(&tag) {
            // Compact binary: length = tag - 0x20 (0-15)
            let len = (tag - 0x20) as usize;
            Ok(self.read_slice(len)?.to_vec())
        } else if (0x34..=0x35).contains(&tag) {
            // Medium binary: 0x34 + 1-byte length (0-255), 0x35 + 2-byte BE length (0-65535)
            if tag == 0x34 {
                let len = self.read_u8()? as usize;
                Ok(self.read_slice(len)?.to_vec())
            } else {
                let bytes: [u8; 2] = self.read_slice(2)?.try_into().expect("read_slice(2)");
                let len = u16::from_be_bytes(bytes) as usize;
                Ok(self.read_slice(len)?.to_vec())
            }
        } else {
            Err(Hessian2Error::TypeMismatch {
                expected: "binary".into(),
                got: format!("0x{tag:02x}"),
                pos: self.pos - 1,
            })
        }
    }

    pub fn read_list_begin(&mut self) -> Hessian2Result<usize> {
        let tag = self.read_u8()?;
        if tag == tags::LIST_VAR_TYPED {
            // Typed var-length: read and skip type, caller reads until 'Z'
            self.read_str()?;
            Ok(0)
        } else if tag == tags::LIST_VAR_UNTYPED {
            // Untyped var-length: caller reads until 'Z'
            Ok(0)
        } else if tag == tags::LIST_FIXED_UNTYPED {
            // Untyped fixed-length: int count follows
            self.read_int().map(|n| n as usize)
        } else if tag == tags::LIST {
            self.read_str()?;
            let b = self.read_u8()?;
            if b < 0x7f {
                Ok(b as usize)
            } else {
                self.read_int().map(|n| n as usize)
            }
        } else if (0x70..=0x77).contains(&tag) {
            let count = (tag - 0x70) as usize;
            self.read_str()?;
            Ok(count)
        } else if (0x78..=0x7f).contains(&tag) {
            // Untyped compact: count = tag - 0x78 (0-7)
            Ok((tag - 0x78) as usize)
        } else {
            Err(Hessian2Error::TypeMismatch {
                expected: "list".into(),
                got: format!("0x{tag:02x}"),
                pos: self.pos - 1,
            })
        }
    }

    #[must_use]
    pub fn peek_is_list_end(&self) -> bool {
        self.buf.get(self.pos).copied() == Some(b'Z')
    }

    pub fn read_list_end(&mut self) -> Hessian2Result<()> {
        let tag = self.read_u8()?;
        if tag != b'Z' {
            return Err(Hessian2Error::TypeMismatch {
                expected: "end-of-list (Z)".into(),
                got: format!("0x{tag:02x}"),
                pos: self.pos - 1,
            });
        }
        Ok(())
    }

    pub fn read_map_begin(&mut self) -> Hessian2Result<bool> {
        let tag = self.read_u8()?;
        match tag {
            tags::MAP => Ok(true),
            tags::UNTYPED_MAP => Ok(false),
            _ => Err(Hessian2Error::TypeMismatch {
                expected: "map".into(),
                got: format!("0x{tag:02x}"),
                pos: self.pos - 1,
            }),
        }
    }

    pub fn read_class_def(&mut self) -> Hessian2Result<usize> {
        let tag = self.read_u8()?;
        if tag != tags::CLASS_DEF {
            return Err(Hessian2Error::TypeMismatch {
                expected: "class-def".into(),
                got: format!("0x{tag:02x}"),
                pos: self.pos - 1,
            });
        }
        let class_name = self.read_str()?;
        let field_count = self.read_int()? as usize;
        let mut field_names = Vec::with_capacity(field_count);
        for _ in 0..field_count {
            field_names.push(self.read_str()?);
        }
        let idx = self.class_defs.len();
        self.class_defs.push(ClassDef {
            class_name,
            field_names,
        });
        Ok(idx)
    }

    /// Read an object instance (`O` tag).
    pub fn read_object_begin(&mut self) -> Hessian2Result<usize> {
        let tag = self.read_u8()?;
        if tag == tags::OBJECT {
            let class_def_index = self.read_int()? as usize;
            if class_def_index >= self.class_defs.len() {
                return Err(Hessian2Error::ReferenceNotFound {
                    index: class_def_index,
                });
            }
            Ok(class_def_index)
        } else if (0x60..=0x6f).contains(&tag) {
            // Compact object: class def index = tag - 0x60 (0-15)
            let class_def_index = (tag - 0x60) as usize;
            if class_def_index >= self.class_defs.len() {
                return Err(Hessian2Error::ReferenceNotFound {
                    index: class_def_index,
                });
            }
            Ok(class_def_index)
        } else {
            Err(Hessian2Error::TypeMismatch {
                expected: "object".into(),
                got: format!("0x{tag:02x}"),
                pos: self.pos - 1,
            })
        }
    }

    #[must_use]
    pub fn get_class_def(&self, index: usize) -> Option<&ClassDef<'a>> {
        self.class_defs.get(index)
    }
}
