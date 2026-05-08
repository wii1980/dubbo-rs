use anyhow::{bail, Result};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use dubbo_rs_remoting::{Codec, Request, Response};

/// Dubbo protocol magic number (big-endian)
pub const DUBBO_MAGIC: u16 = 0xdabb;
/// Flag bit: this is a request
pub const FLAG_REQUEST: u8 = 0x80;
/// Flag bit: two-way (expects response)
pub const FLAG_TWOWAY: u8 = 0x40;
/// Flag bit: heartbeat/event
pub const FLAG_EVENT: u8 = 0x20;
/// Mask for serialization ID in flags byte
pub const FLAG_SERIAL_MASK: u8 = 0x1f;

/// Minimum header size in bytes
pub const HEADER_LENGTH: usize = 16;

/// Dubbo serialization protocol identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerializationId {
    Hessian2 = 2,
    Protobuf = 12,
    Json = 21,
}

impl SerializationId {
    /// Convert from raw byte value
    #[must_use]
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            2 => Some(Self::Hessian2),
            12 => Some(Self::Protobuf),
            21 => Some(Self::Json),
            _ => None,
        }
    }

    /// Convert to raw byte value
    #[must_use]
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

/// Dubbo TCP protocol codec for 16-byte header + body framing
pub struct DubboCodec {
    serialization_id: u8,
}

impl DubboCodec {
    #[must_use]
    pub fn new(serialization_id: SerializationId) -> Self {
        Self {
            serialization_id: serialization_id.to_u8(),
        }
    }
}

impl Codec for DubboCodec {
    fn encode_request(&self, req: &Request) -> Result<Vec<u8>> {
        let body_len = req.data.len() as u32;
        let mut buf = Vec::with_capacity(HEADER_LENGTH + body_len as usize);

        // Magic (offset 0: 2 bytes, little-endian)
        buf.write_u16::<BigEndian>(DUBBO_MAGIC)?;

        // Flags (offset 2: 1 byte)
        let mut flags = FLAG_REQUEST | (self.serialization_id & FLAG_SERIAL_MASK);
        if req.is_twoway {
            flags |= FLAG_TWOWAY;
        }
        if req.is_event {
            flags |= FLAG_EVENT;
        }
        buf.write_u8(flags)?;

        // Status (offset 3: 1 byte, unused for requests)
        buf.write_u8(0)?;

        // Request ID (offset 4: 8 bytes, little-endian)
        buf.write_u64::<BigEndian>(req.id)?;

        // Body length (offset 12: 4 bytes, little-endian)
        buf.write_u32::<BigEndian>(body_len)?;

        // Body (offset 16+)
        buf.extend_from_slice(&req.data);

        Ok(buf)
    }

    fn decode_request(&self, data: &[u8]) -> Result<Request> {
        if data.len() < HEADER_LENGTH {
            bail!(
                "frame too short: {} bytes, need at least {HEADER_LENGTH}",
                data.len()
            );
        }

        let mut reader = std::io::Cursor::new(data);

        // Magic
        let magic = reader.read_u16::<BigEndian>()?;
        if magic != DUBBO_MAGIC {
            bail!("invalid magic number: 0x{magic:04x}, expected 0x{DUBBO_MAGIC:04x}");
        }

        // Flags
        let flags = reader.read_u8()?;
        if flags & FLAG_REQUEST == 0 {
            bail!("not a request frame (FLAG_REQUEST not set)");
        }
        let is_twoway = (flags & FLAG_TWOWAY) != 0;
        let is_event = (flags & FLAG_EVENT) != 0;

        // Status (skip for request)
        let _status = reader.read_u8()?;

        // Request ID
        let id = reader.read_u64::<BigEndian>()?;

        // Body length
        let body_len = reader.read_u32::<BigEndian>()? as usize;

        // Body
        let body_start = HEADER_LENGTH;
        let body_end = body_start + body_len;
        if data.len() < body_end {
            bail!(
                "frame too short for body: expected {body_end} bytes, got {}",
                data.len()
            );
        }
        let body = data[body_start..body_end].to_vec();

        Ok(Request {
            id,
            is_twoway,
            is_event,
            data: body,
        })
    }

    fn encode_response(&self, resp: &Response) -> Result<Vec<u8>> {
        let body_len = resp.data.len() as u32;
        let mut buf = Vec::with_capacity(HEADER_LENGTH + body_len as usize);

        // Magic
        buf.write_u16::<BigEndian>(DUBBO_MAGIC)?;

        // Flags: response (NO FLAG_REQUEST), only serialization bits
        let flags = self.serialization_id & FLAG_SERIAL_MASK;
        buf.write_u8(flags)?;

        // Status
        buf.write_u8(resp.status)?;

        // Request ID
        buf.write_u64::<BigEndian>(resp.id)?;

        // Body length
        buf.write_u32::<BigEndian>(body_len)?;

        // Body
        buf.extend_from_slice(&resp.data);

        Ok(buf)
    }

    fn decode_response(&self, data: &[u8]) -> Result<Response> {
        if data.len() < HEADER_LENGTH {
            bail!(
                "frame too short: {} bytes, need at least {HEADER_LENGTH}",
                data.len()
            );
        }

        let mut reader = std::io::Cursor::new(data);

        // Magic
        let magic = reader.read_u16::<BigEndian>()?;
        if magic != DUBBO_MAGIC {
            bail!("invalid magic number: 0x{magic:04x}, expected 0x{DUBBO_MAGIC:04x}");
        }

        // Flags
        let flags = reader.read_u8()?;
        if flags & FLAG_REQUEST != 0 {
            bail!("not a response frame (FLAG_REQUEST is set)");
        }

        // Status
        let status = reader.read_u8()?;

        // Request ID
        let id = reader.read_u64::<BigEndian>()?;

        // Body length
        let body_len = reader.read_u32::<BigEndian>()? as usize;

        // Body
        let body_start = HEADER_LENGTH;
        let body_end = body_start + body_len;
        if data.len() < body_end {
            bail!(
                "frame too short for body: expected {body_end} bytes, got {}",
                data.len()
            );
        }
        let body = data[body_start..body_end].to_vec();

        Ok(Response {
            id,
            status,
            data: body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Roundtrip tests ───────────────────────────────────────────────

    #[test]
    fn test_encode_decode_request_roundtrip() {
        let codec = DubboCodec::new(SerializationId::Hessian2);
        let req = Request {
            id: 42,
            is_twoway: true,
            is_event: false,
            data: b"hello dubbo".to_vec(),
        };

        let encoded = codec.encode_request(&req).expect("encode should succeed");
        let decoded = codec
            .decode_request(&encoded)
            .expect("decode should succeed");

        assert_eq!(decoded.id, 42);
        assert!(decoded.is_twoway);
        assert!(!decoded.is_event);
        assert_eq!(decoded.data, b"hello dubbo");
    }

    #[test]
    fn test_encode_decode_response_roundtrip() {
        let codec = DubboCodec::new(SerializationId::Hessian2);
        let resp = Response {
            id: 99,
            status: 20,
            data: b"world dubbo".to_vec(),
        };

        let encoded = codec.encode_response(&resp).expect("encode should succeed");
        let decoded = codec
            .decode_response(&encoded)
            .expect("decode should succeed");

        assert_eq!(decoded.id, 99);
        assert_eq!(decoded.status, 20);
        assert_eq!(decoded.data, b"world dubbo");
    }

    // ── Header size test ──────────────────────────────────────────────

    #[test]
    fn test_header_size() {
        let codec = DubboCodec::new(SerializationId::Hessian2);
        let req = Request {
            id: 1,
            is_twoway: false,
            is_event: false,
            data: b"abc".to_vec(), // 3 bytes body
        };

        let encoded = codec.encode_request(&req).expect("encode should succeed");
        // 16-byte header + 3-byte body
        assert_eq!(encoded.len(), 16 + 3);

        let resp = Response {
            id: 2,
            status: 20,
            data: vec![0u8; 100],
        };
        let encoded = codec.encode_response(&resp).expect("encode should succeed");
        assert_eq!(encoded.len(), 16 + 100);
    }

    // ── Error cases ───────────────────────────────────────────────────

    #[test]
    fn test_decode_invalid_magic() {
        let codec = DubboCodec::new(SerializationId::Hessian2);
        // 16 bytes with wrong magic (0x0000)
        let mut frame = vec![0u8; 16];
        // Set FLAG_REQUEST so it looks like a request
        frame[2] = FLAG_REQUEST;
        frame[4..12].copy_from_slice(&1u64.to_be_bytes());

        let result = codec.decode_request(&frame);
        assert!(result.is_err(), "should error on invalid magic");
    }

    #[test]
    fn test_decode_frame_too_short() {
        let codec = DubboCodec::new(SerializationId::Hessian2);
        // Only 10 bytes — less than 16-byte header
        let frame = vec![0u8; 10];

        let result = codec.decode_request(&frame);
        assert!(result.is_err(), "should error on too-short frame");
    }

    // ── SerializationId tests ─────────────────────────────────────────

    #[test]
    fn test_serialization_id_hessian2() {
        assert_eq!(SerializationId::from_u8(2), Some(SerializationId::Hessian2));
        assert_eq!(SerializationId::Hessian2.to_u8(), 2);
    }

    #[test]
    fn test_serialization_id_protobuf() {
        assert_eq!(
            SerializationId::from_u8(12),
            Some(SerializationId::Protobuf)
        );
        assert_eq!(SerializationId::Protobuf.to_u8(), 12);
    }

    #[test]
    fn test_serialization_id_json() {
        assert_eq!(SerializationId::from_u8(21), Some(SerializationId::Json));
        assert_eq!(SerializationId::Json.to_u8(), 21);
    }

    #[test]
    fn test_serialization_id_unknown() {
        assert_eq!(SerializationId::from_u8(99), None);
    }

    // ── Flag encoding tests ───────────────────────────────────────────

    #[test]
    fn test_request_flags_set_in_encoded_header() {
        let codec = DubboCodec::new(SerializationId::Hessian2);
        let req = Request {
            id: 7,
            is_twoway: true,
            is_event: false,
            data: b"test".to_vec(),
        };

        let encoded = codec.encode_request(&req).expect("encode should succeed");

        // Verify FLAG_REQUEST is set in flags byte (offset 2)
        assert!(
            encoded[2] & FLAG_REQUEST != 0,
            "FLAG_REQUEST should be set for request"
        );
        assert!(
            encoded[2] & FLAG_TWOWAY != 0,
            "FLAG_TWOWAY should be set for two-way request"
        );
        // Hessian2 serialization ID = 2 should be in low 5 bits
        assert_eq!(encoded[2] & FLAG_SERIAL_MASK, 2);
    }

    #[test]
    fn test_response_flags_do_not_set_request_bit() {
        let codec = DubboCodec::new(SerializationId::Hessian2);
        let resp = Response {
            id: 7,
            status: 20,
            data: b"test".to_vec(),
        };

        let encoded = codec.encode_response(&resp).expect("encode should succeed");

        // Verify FLAG_REQUEST is NOT set in flags byte (offset 2)
        assert_eq!(
            encoded[2] & FLAG_REQUEST,
            0,
            "FLAG_REQUEST should NOT be set for response"
        );
        // Hessian2 serialization ID = 2 should be in low 5 bits
        assert_eq!(encoded[2] & FLAG_SERIAL_MASK, 2);
    }

    #[test]
    fn test_decode_request_fails_on_response_frame() {
        let codec = DubboCodec::new(SerializationId::Hessian2);
        let resp = Response {
            id: 1,
            status: 20,
            data: vec![],
        };
        let encoded = codec.encode_response(&resp).expect("encode should succeed");

        // Decoding a response frame as request should fail
        let result = codec.decode_request(&encoded);
        assert!(result.is_err(), "decoding response as request should fail");
    }

    #[test]
    fn test_decode_response_fails_on_request_frame() {
        let codec = DubboCodec::new(SerializationId::Hessian2);
        let req = Request {
            id: 1,
            is_twoway: true,
            is_event: false,
            data: vec![],
        };
        let encoded = codec.encode_request(&req).expect("encode should succeed");

        // Decoding a request frame as response should fail
        let result = codec.decode_response(&encoded);
        assert!(result.is_err(), "decoding request as response should fail");
    }

    // ── Event flag test ───────────────────────────────────────────────

    #[test]
    fn test_event_flag_roundtrip() {
        let codec = DubboCodec::new(SerializationId::Hessian2);
        let req = Request {
            id: 5,
            is_twoway: false,
            is_event: true,
            data: b"heartbeat".to_vec(),
        };

        let encoded = codec.encode_request(&req).expect("encode should succeed");
        let decoded = codec
            .decode_request(&encoded)
            .expect("decode should succeed");

        assert!(decoded.is_event, "is_event should be preserved");
        assert!(!decoded.is_twoway);
        assert_eq!(decoded.data, b"heartbeat");
    }

    // ── Zero-length body test ─────────────────────────────────────────

    #[test]
    fn test_zero_length_body_roundtrip() {
        let codec = DubboCodec::new(SerializationId::Json);
        let req = Request {
            id: 3,
            is_twoway: true,
            is_event: false,
            data: vec![],
        };
        let encoded = codec.encode_request(&req).expect("encode should succeed");
        let decoded = codec
            .decode_request(&encoded)
            .expect("decode should succeed");
        assert_eq!(decoded.data, vec![]);
        assert_eq!(encoded.len(), 16);
    }

    // ── Status code roundtrip ─────────────────────────────────────────

    #[test]
    fn test_response_status_roundtrip() {
        let codec = DubboCodec::new(SerializationId::Hessian2);
        let resp = Response {
            id: 1,
            status: 80, // SERVER_ERROR_STATUS
            data: b"internal error".to_vec(),
        };
        let encoded = codec.encode_response(&resp).expect("encode should succeed");
        let decoded = codec
            .decode_response(&encoded)
            .expect("decode should succeed");
        assert_eq!(decoded.status, 80);
    }
}
