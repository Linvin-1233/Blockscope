use crate::error::{NbtError, NbtErrorCode};
use crate::limits::DecodeLimits;
use crate::value::{NbtCompound, NbtList, NbtRoot, NbtTagType, NbtValue};
use flate2::read::{GzDecoder, ZlibDecoder};
use std::borrow::Cow;
use std::io::Read;

/// Compression formats accepted for NBT containers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    Gzip,
    Zlib,
    Uncompressed,
}

impl Compression {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gzip => "gzip",
            Self::Zlib => "zlib",
            Self::Uncompressed => "uncompressed",
        }
    }
}

/// Detect the compression format from the first bytes of the input.
pub fn detect_compression(data: &[u8]) -> Compression {
    if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        Compression::Gzip
    } else if data.len() >= 2 && is_zlib_header(data[0], data[1]) {
        Compression::Zlib
    } else {
        Compression::Uncompressed
    }
}

fn is_zlib_header(cmf: u8, flg: u8) -> bool {
    cmf & 0x0f == 8 && cmf >> 4 <= 7 && (u16::from(cmf) << 8 | u16::from(flg)) % 31 == 0
}

/// Decode an NBT root tag, accepting gzip, zlib, or uncompressed input.
pub fn decode(input: &[u8], limits: &DecodeLimits) -> Result<NbtRoot, NbtError> {
    let data = decompress(input, limits)?;
    let mut decoder = Decoder::new(data.as_ref(), limits);
    let root = decoder.read_root()?;

    if !decoder.truncated && decoder.pos != data.len() {
        return Err(NbtError::new(
            NbtErrorCode::TrailingBytes,
            decoder.pos,
            "trailing bytes after root tag",
        ));
    }

    Ok(root)
}

fn decompress<'a>(input: &'a [u8], limits: &DecodeLimits) -> Result<Cow<'a, [u8]>, NbtError> {
    if input.len() > limits.max_input_bytes {
        return Err(limit_error(
            0,
            format!(
                "input size {} exceeds limit {}",
                input.len(),
                limits.max_input_bytes
            ),
        ));
    }

    match detect_compression(input) {
        Compression::Uncompressed => {
            if input.len() > limits.max_decompressed_bytes {
                return Err(limit_error(
                    0,
                    format!(
                        "uncompressed size {} exceeds limit {}",
                        input.len(),
                        limits.max_decompressed_bytes
                    ),
                ));
            }
            Ok(Cow::Borrowed(input))
        }
        Compression::Gzip => {
            read_limited(GzDecoder::new(input), limits.max_decompressed_bytes, 0).map(Cow::Owned)
        }
        Compression::Zlib => {
            read_limited(ZlibDecoder::new(input), limits.max_decompressed_bytes, 0).map(Cow::Owned)
        }
    }
}

fn read_limited<R: Read>(mut reader: R, limit: usize, offset: usize) -> Result<Vec<u8>, NbtError> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];

    loop {
        let read = reader.read(&mut buffer).map_err(|error| {
            NbtError::new(NbtErrorCode::InvalidCompression, offset, error.to_string())
        })?;
        if read == 0 {
            break;
        }
        if output.len().saturating_add(read) > limit {
            return Err(limit_error(
                offset,
                format!("decompressed size exceeds limit {limit}"),
            ));
        }
        output.extend_from_slice(&buffer[..read]);
    }

    Ok(output)
}

struct Decoder<'a> {
    data: &'a [u8],
    pos: usize,
    limits: &'a DecodeLimits,
    tag_count: usize,
    total_bytes: usize,
    truncated: bool,
}

impl<'a> Decoder<'a> {
    fn new(data: &'a [u8], limits: &'a DecodeLimits) -> Self {
        Self {
            data,
            pos: 0,
            limits,
            tag_count: 0,
            total_bytes: 0,
            truncated: false,
        }
    }

    fn read_root(&mut self) -> Result<NbtRoot, NbtError> {
        let tag_type = self.read_tag_type()?;
        if tag_type == NbtTagType::End {
            return Err(NbtError::new(
                NbtErrorCode::InvalidTag,
                self.pos.saturating_sub(1),
                "root tag cannot be end",
            ));
        }

        let name = self.read_string()?;
        let value = self.read_payload(tag_type, 0)?;
        Ok(NbtRoot { name, value })
    }

    fn read_payload(&mut self, tag_type: NbtTagType, depth: usize) -> Result<NbtValue, NbtError> {
        if depth > self.limits.max_depth {
            return Err(limit_error(
                self.pos,
                format!("maximum NBT depth {} exceeded", self.limits.max_depth),
            ));
        }

        self.tag_count = self
            .tag_count
            .checked_add(1)
            .ok_or_else(|| limit_error(self.pos, "tag count overflow"))?;
        if self.tag_count > self.limits.max_tags {
            return Err(limit_error(
                self.pos,
                format!("tag count exceeds limit {}", self.limits.max_tags),
            ));
        }

        match tag_type {
            NbtTagType::End => Err(NbtError::new(
                NbtErrorCode::InvalidTag,
                self.pos,
                "end tag has no payload",
            )),
            NbtTagType::Byte => Ok(NbtValue::Byte(self.read_i8()?)),
            NbtTagType::Short => Ok(NbtValue::Short(self.read_i16()?)),
            NbtTagType::Int => Ok(NbtValue::Int(self.read_i32()?)),
            NbtTagType::Long => Ok(NbtValue::Long(self.read_i64()?)),
            NbtTagType::Float => Ok(NbtValue::Float(self.read_f32()?)),
            NbtTagType::Double => Ok(NbtValue::Double(self.read_f64()?)),
            NbtTagType::ByteArray => self.read_byte_array(),
            NbtTagType::String => Ok(NbtValue::String(self.read_string()?)),
            NbtTagType::List => self.read_list(depth),
            NbtTagType::Compound => self.read_compound(depth),
            NbtTagType::IntArray => self.read_int_array(),
            NbtTagType::LongArray => self.read_long_array(),
        }
    }

    fn read_compound(&mut self, depth: usize) -> Result<NbtValue, NbtError> {
        let mut compound = NbtCompound::default();

        loop {
            let tag_type = self.read_tag_type()?;
            if tag_type == NbtTagType::End {
                break;
            }

            let name = self.read_string()?;
            let value = if self.limits.ignore_block_entity_nbt
                && name == "TileEntities"
                && tag_type == NbtTagType::List
            {
                match self.skip_payload(tag_type, depth.saturating_add(1)) {
                    Ok(()) => NbtValue::List(empty_list()),
                    Err(error) if error.code == NbtErrorCode::TruncatedInput => {
                        self.truncated = true;
                        NbtValue::List(empty_list())
                    }
                    Err(error) => return Err(error),
                }
            } else {
                self.read_payload(tag_type, depth.saturating_add(1))?
            };
            self.add_allocation(64usize.saturating_add(name.len()), self.pos)?;
            compound.insert(name, value);
            if self.truncated {
                break;
            }
        }

        Ok(NbtValue::Compound(compound))
    }

    fn read_list(&mut self, depth: usize) -> Result<NbtValue, NbtError> {
        let element_type = self.read_tag_type()?;
        let offset = self.pos;
        let len = self.read_len(self.limits.max_list_len)?;

        if element_type == NbtTagType::End && len != 0 {
            return Err(NbtError::new(
                NbtErrorCode::InvalidTag,
                offset,
                "non-empty list cannot use end as its element type",
            ));
        }

        // Every element consumes at least one byte (a compound needs its end
        // tag, a scalar its payload). Reject impossible lengths before
        // reserving the element table, so a tiny truncated input cannot force a
        // large allocation.
        let remaining = self.remaining();
        if len > remaining {
            return Err(NbtError::new(
                NbtErrorCode::TruncatedInput,
                offset,
                format!("list declares {len} elements, only {remaining} bytes remain"),
            ));
        }

        let reserved = len
            .checked_mul(std::mem::size_of::<NbtValue>())
            .ok_or_else(|| limit_error(offset, "list allocation overflow"))?;
        self.add_allocation(reserved, offset)?;

        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(self.read_payload(element_type, depth.saturating_add(1))?);
            if self.truncated {
                break;
            }
        }

        Ok(NbtValue::List(NbtList::new(element_type, values)))
    }

    /// Advance past a tag payload without building it. Used for block entity
    /// data, which can be orders of magnitude larger than the block array.
    fn skip_payload(&mut self, tag_type: NbtTagType, depth: usize) -> Result<(), NbtError> {
        if depth > self.limits.max_depth {
            return Err(limit_error(
                self.pos,
                format!("maximum NBT depth {} exceeded", self.limits.max_depth),
            ));
        }

        self.tag_count = self
            .tag_count
            .checked_add(1)
            .ok_or_else(|| limit_error(self.pos, "tag count overflow"))?;
        if self.tag_count > self.limits.max_tags {
            return Err(limit_error(
                self.pos,
                format!("tag count exceeds limit {}", self.limits.max_tags),
            ));
        }

        match tag_type {
            NbtTagType::End => Err(NbtError::new(
                NbtErrorCode::InvalidTag,
                self.pos,
                "end tag has no payload",
            )),
            NbtTagType::Byte => self.skip_bytes(1),
            NbtTagType::Short => self.skip_bytes(2),
            NbtTagType::Int | NbtTagType::Float => self.skip_bytes(4),
            NbtTagType::Long | NbtTagType::Double => self.skip_bytes(8),
            NbtTagType::ByteArray => {
                let len = self.read_len(self.limits.max_array_len)?;
                self.skip_bytes(len)
            }
            NbtTagType::String => {
                let len = usize::from(self.read_u16()?);
                self.skip_bytes(len)
            }
            NbtTagType::List => {
                let element_type = self.read_tag_type()?;
                let offset = self.pos;
                let len = self.read_len(self.limits.max_list_len)?;
                if element_type == NbtTagType::End && len != 0 {
                    return Err(NbtError::new(
                        NbtErrorCode::InvalidTag,
                        offset,
                        "non-empty list cannot use end as its element type",
                    ));
                }
                for _ in 0..len {
                    self.skip_payload(element_type, depth.saturating_add(1))?;
                }
                Ok(())
            }
            NbtTagType::Compound => loop {
                let child = self.read_tag_type()?;
                if child == NbtTagType::End {
                    break Ok(());
                }
                let name_len = usize::from(self.read_u16()?);
                self.skip_bytes(name_len)?;
                self.skip_payload(child, depth.saturating_add(1))?;
            },
            NbtTagType::IntArray => {
                let len = self.read_len(self.limits.max_array_len)?;
                let size = len
                    .checked_mul(4)
                    .ok_or_else(|| limit_error(self.pos, "int array size overflow"))?;
                self.skip_bytes(size)
            }
            NbtTagType::LongArray => {
                let len = self.read_len(self.limits.max_array_len)?;
                let size = len
                    .checked_mul(8)
                    .ok_or_else(|| limit_error(self.pos, "long array size overflow"))?;
                self.skip_bytes(size)
            }
        }
    }

    fn skip_bytes(&mut self, len: usize) -> Result<(), NbtError> {
        self.read_exact(len).map(|_| ())
    }

    fn read_byte_array(&mut self) -> Result<NbtValue, NbtError> {
        let offset = self.pos;
        let len = self.read_len(self.limits.max_array_len)?;
        self.add_allocation(len, offset)?;
        let bytes = self.read_exact(len)?;
        Ok(NbtValue::ByteArray(bytes.to_vec()))
    }

    fn read_int_array(&mut self) -> Result<NbtValue, NbtError> {
        let offset = self.pos;
        let len = self.read_len(self.limits.max_array_len)?;
        let size = len
            .checked_mul(4)
            .ok_or_else(|| limit_error(offset, "int array allocation overflow"))?;
        self.ensure_available(size, offset, "int array")?;
        self.add_allocation(size, offset)?;

        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(self.read_i32()?);
        }
        Ok(NbtValue::IntArray(values))
    }

    fn read_long_array(&mut self) -> Result<NbtValue, NbtError> {
        let offset = self.pos;
        let len = self.read_len(self.limits.max_array_len)?;
        let size = len
            .checked_mul(8)
            .ok_or_else(|| limit_error(offset, "long array allocation overflow"))?;
        self.ensure_available(size, offset, "long array")?;
        self.add_allocation(size, offset)?;

        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(self.read_i64()?);
        }
        Ok(NbtValue::LongArray(values))
    }

    fn read_string(&mut self) -> Result<String, NbtError> {
        let offset = self.pos;
        let len = usize::from(self.read_u16()?);
        if len > self.limits.max_string_bytes {
            return Err(limit_error(
                offset,
                format!(
                    "string length {len} exceeds limit {}",
                    self.limits.max_string_bytes
                ),
            ));
        }

        self.add_allocation(len, offset)?;
        let bytes = self.read_exact(len)?;
        decode_java_utf8(bytes)
            .map_err(|message| NbtError::new(NbtErrorCode::InvalidUtf8, offset, message))
    }

    fn read_len(&mut self, max: usize) -> Result<usize, NbtError> {
        let offset = self.pos;
        let value = self.read_i32()?;
        if value < 0 {
            return Err(NbtError::new(
                NbtErrorCode::InvalidTag,
                offset,
                format!("negative length {value}"),
            ));
        }

        let len = usize::try_from(value)
            .map_err(|_| limit_error(offset, "length does not fit in usize"))?;
        if len > max {
            return Err(limit_error(
                offset,
                format!("length {len} exceeds limit {max}"),
            ));
        }
        Ok(len)
    }

    fn read_tag_type(&mut self) -> Result<NbtTagType, NbtError> {
        let offset = self.pos;
        let byte = self.read_u8()?;
        NbtTagType::from_u8(byte).ok_or_else(|| {
            NbtError::new(
                NbtErrorCode::InvalidTag,
                offset,
                format!("unknown tag type {byte}"),
            )
        })
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8], NbtError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| limit_error(self.pos, "read offset overflow"))?;
        if end > self.data.len() {
            return Err(NbtError::new(
                NbtErrorCode::TruncatedInput,
                self.pos,
                format!(
                    "need {len} bytes, have {}",
                    self.data.len().saturating_sub(self.pos)
                ),
            ));
        }

        let data = self.data;
        let slice = &data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8, NbtError> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_i8(&mut self) -> Result<i8, NbtError> {
        Ok(self.read_u8()? as i8)
    }

    fn read_u16(&mut self) -> Result<u16, NbtError> {
        let bytes = self.read_exact(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn read_i16(&mut self) -> Result<i16, NbtError> {
        let bytes = self.read_exact(2)?;
        Ok(i16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn read_i32(&mut self) -> Result<i32, NbtError> {
        let bytes = self.read_exact(4)?;
        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_i64(&mut self) -> Result<i64, NbtError> {
        let bytes = self.read_exact(8)?;
        Ok(i64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn read_f32(&mut self) -> Result<f32, NbtError> {
        Ok(f32::from_bits(self.read_i32()? as u32))
    }

    fn read_f64(&mut self) -> Result<f64, NbtError> {
        Ok(f64::from_bits(self.read_i64()? as u64))
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// Reject a declared payload that cannot possibly fit in the remaining
    /// input before reserving memory for it.
    fn ensure_available(&self, bytes: usize, offset: usize, label: &str) -> Result<(), NbtError> {
        let remaining = self.remaining();
        if bytes > remaining {
            return Err(NbtError::new(
                NbtErrorCode::TruncatedInput,
                offset,
                format!("{label} needs {bytes} bytes, only {remaining} bytes remain"),
            ));
        }
        Ok(())
    }

    fn add_allocation(&mut self, bytes: usize, offset: usize) -> Result<(), NbtError> {
        self.total_bytes = self
            .total_bytes
            .checked_add(bytes)
            .ok_or_else(|| limit_error(offset, "allocation budget overflow"))?;
        if self.total_bytes > self.limits.max_total_bytes {
            return Err(limit_error(
                offset,
                format!(
                    "decoded allocation {} exceeds limit {}",
                    self.total_bytes, self.limits.max_total_bytes
                ),
            ));
        }
        Ok(())
    }
}

fn limit_error(offset: usize, message: impl Into<String>) -> NbtError {
    NbtError::new(NbtErrorCode::LimitExceeded, offset, message)
}

fn empty_list() -> NbtList {
    NbtList::new(NbtTagType::End, Vec::new())
}

/// Decode Java's modified UTF-8, also accepting standard four-byte sequences.
fn decode_java_utf8(bytes: &[u8]) -> Result<String, String> {
    let mut units = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        let first = bytes[index];
        if first < 0x80 {
            units.push(u16::from(first));
            index += 1;
            continue;
        }

        if first == 0xc0 && index + 1 < bytes.len() && bytes[index + 1] == 0x80 {
            units.push(0);
            index += 2;
            continue;
        }

        let (length, mut code_point) = if (0xc2..=0xdf).contains(&first) {
            (2, u32::from(first & 0x1f))
        } else if (0xe0..=0xef).contains(&first) {
            (3, u32::from(first & 0x0f))
        } else if (0xf0..=0xf7).contains(&first) {
            (4, u32::from(first & 0x07))
        } else {
            return Err(format!("invalid leading byte 0x{first:02x}"));
        };

        if index + length > bytes.len() {
            return Err("truncated UTF-8 sequence".to_string());
        }

        for &byte in &bytes[index + 1..index + length] {
            if byte & 0xc0 != 0x80 {
                return Err(format!("invalid continuation byte 0x{byte:02x}"));
            }
            code_point = (code_point << 6) | u32::from(byte & 0x3f);
        }
        index += length;

        if length == 3 && code_point < 0x800 {
            return Err("overlong three-byte UTF-8 sequence".to_string());
        }
        if length == 4 && code_point < 0x10000 {
            return Err("overlong four-byte UTF-8 sequence".to_string());
        }

        if length == 4 {
            let ch = char::from_u32(code_point)
                .ok_or_else(|| format!("invalid code point U+{code_point:X}"))?;
            let mut buffer = [0u16; 2];
            units.extend_from_slice(ch.encode_utf16(&mut buffer));
        } else {
            if code_point > 0xffff {
                return Err(format!(
                    "code point U+{code_point:X} does not fit in one UTF-16 unit"
                ));
            }
            units.push(code_point as u16);
        }
    }

    String::from_utf16(&units).map_err(|_| "unpaired UTF-16 surrogate".to_string())
}
