//! Wire primitives of Minecraft 26.3 play packets: big-endian numbers, VarInt and VarLong,
//! length-prefixed UTF-8 strings, UUIDs, packed block and section positions, low-precision
//! vectors and network NBT (`nbt.rs`). Engine-free: the glue hands packet bodies to the wrappers
//! and gets them back from Horizon's own encoder (see `Registry` for the parts that need the
//! item, particle and entity data registries).

use crate::horizon_packetevents_core::pe::nbt::Tag;
use std::fmt;

/// A malformed or unsupported packet body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Error(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Parts of the protocol whose length depends on registries: item components, particle
/// payloads and entity data values. A wrapper keeps them as raw bytes and asks the registry for
/// their length when it reads a packet. The defaults know the simple cases; the glue overrides
/// every method with Horizon's own codecs (`Registry` is object safe, so wrappers take `&dyn`).
pub trait Registry: Send + Sync {
    /// Byte length of the item slot at the start of `data`, count prefix included.
    fn item_len(&self, data: &[u8]) -> Result<usize> {
        basic_item_len(data)
    }

    /// Byte length of the particle at the start of `data`, its type VarInt included.
    fn particle_len(&self, data: &[u8]) -> Result<usize> {
        let mut r = Reader::new(data);
        let id = r.varint()?;
        let head = r.position();
        match id {
            // Particles whose payload is one block state (a VarInt).
            1 | 2 | 36 | 121 | 125 => {
                let mut state = Reader::new(&data[head..]);
                state.varint()?;
                Ok(head + state.position())
            }
            // Particles whose payload is one ARGB colour (an int).
            28 | 46 | 52 => {
                r.skip(4)?;
                Ok(r.position())
            }
            _ => Err(Error::new(format!(
                "particle {id} has a payload this registry cannot measure"
            ))),
        }
    }

    /// Byte length of one synched entity value (`SynchedEntityData`), serializer id given.
    fn entity_value_len(&self, serializer: i32, data: &[u8]) -> Result<usize> {
        fixed_entity_value_len(serializer, data)
    }
}

/// The registry-free default: items with no components only.
pub struct BasicRegistry;

impl Registry for BasicRegistry {}

/// An item slot without components: a count, and for a stack an item id and two zero
/// VarInts (components added and removed). Anything else needs the item codec.
pub fn basic_item_len(data: &[u8]) -> Result<usize> {
    let mut r = Reader::new(data);
    let count = r.varint()?;
    if count <= 0 {
        return Ok(r.position());
    }
    let _item = r.varint()?;
    let added = r.varint()?;
    let removed = r.varint()?;
    if added != 0 || removed != 0 {
        return Err(Error::new(
            "item components need the item codec of the registry",
        ));
    }
    Ok(r.position())
}

/// The serializers whose value length is fixed or a VarInt (1.21.x to 26.3 ids for the simple
/// ones). Anything else needs the registry.
pub fn fixed_entity_value_len(serializer: i32, data: &[u8]) -> Result<usize> {
    let mut r = Reader::new(data);
    match serializer {
        0 | 8 => r.skip(1)?,                                   // byte, boolean
        1 | 12 | 14 | 15 | 19 | 20 => r.varint().map(|_| ())?, // int, direction, block states, optional uint, pose
        2 => r.varlong().map(|_| ())?,                         // long
        3 => r.skip(4)?,                                       // float
        4 => r.string(32767).map(|_| ())?,                     // string
        5 => r.nbt_raw()?,                                     // component
        6 => {
            if r.bool()? {
                r.nbt_raw()?;
            }
        }
        9 | 39 => r.skip(12)?, // rotations, vector3
        40 => r.skip(16)?,     // quaternion
        10 => r.skip(8)?,      // block pos
        11 => {
            if r.bool()? {
                r.skip(8)?;
            }
        }
        18 => {
            r.varint()?;
            r.varint()?;
            r.varint()?; // villager data
        }
        _ => {
            return Err(Error::new(format!(
                "entity data serializer {serializer} needs the registry"
            )));
        }
    }
    Ok(r.position())
}

/// A cursor over a packet body. Every read checks the bounds.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }

    /// Bytes read so far.
    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    /// Fails unless the whole body was read.
    pub fn finish(&self) -> Result<()> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(Error::new(format!("{} bytes left over", self.remaining())))
        }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.remaining() < n {
            return Err(Error::new(format!(
                "needs {n} bytes, {} left",
                self.remaining()
            )));
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        self.take(n)
    }

    /// Everything not read yet, without reading it.
    pub fn peek(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }

    /// Everything not read yet.
    pub fn rest(&mut self) -> &'a [u8] {
        let slice = &self.data[self.pos..];
        self.pos = self.data.len();
        slice
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }

    pub fn bool(&mut self) -> Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(Error::new(format!("boolean {other}"))),
        }
    }

    pub fn i16(&mut self) -> Result<i16> {
        let b = self.take(2)?;
        Ok(i16::from_be_bytes([b[0], b[1]]))
    }

    pub fn i32(&mut self) -> Result<i32> {
        let b = self.take(4)?;
        Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i64(&mut self) -> Result<i64> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(i64::from_be_bytes(a))
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.i32()? as u32))
    }

    pub fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_bits(self.i64()? as u64))
    }

    /// A VarInt: up to five bytes, seven bits each, least significant group first.
    pub fn varint(&mut self) -> Result<i32> {
        let mut value: u32 = 0;
        for shift in (0..35).step_by(7) {
            let byte = self.u8()?;
            value |= u32::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Ok(value as i32);
            }
        }
        Err(Error::new("VarInt is longer than five bytes"))
    }

    /// A VarLong: up to ten bytes.
    pub fn varlong(&mut self) -> Result<i64> {
        let mut value: u64 = 0;
        for shift in (0..70).step_by(7) {
            let byte = self.u8()?;
            value |= u64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Ok(value as i64);
            }
        }
        Err(Error::new("VarLong is longer than ten bytes"))
    }

    /// A UUID as two longs (most significant first).
    pub fn uuid(&mut self) -> Result<u128> {
        let hi = self.i64()? as u64;
        let lo = self.i64()? as u64;
        Ok((u128::from(hi) << 64) | u128::from(lo))
    }

    /// A string of at most `max` characters (the count is a VarInt of bytes, at most `max * 3`).
    pub fn string(&mut self, max: usize) -> Result<String> {
        let len = self.varint()?;
        if len < 0 || len as usize > max * 3 {
            return Err(Error::new(format!(
                "string length {len} is past {max} characters"
            )));
        }
        let bytes = self.take(len as usize)?;
        let s = std::str::from_utf8(bytes).map_err(|_| Error::new("string is not UTF-8"))?;
        if s.chars().count() > max {
            return Err(Error::new(format!("string is past {max} characters")));
        }
        Ok(s.to_owned())
    }

    /// A namespaced identifier (`minecraft:stone`), at most 32767 bytes.
    pub fn identifier(&mut self) -> Result<String> {
        self.string(32767)
    }

    /// A packed block position: x (26 bits), z (26 bits), y (12 bits).
    pub fn block_pos(&mut self) -> Result<[i32; 3]> {
        Ok(unpack_block_pos(self.i64()?))
    }

    /// A low-precision velocity (`LpVec3`).
    pub fn lp_vec3(&mut self) -> Result<[f64; 3]> {
        let lowest = u64::from(self.u8()?);
        if lowest == 0 {
            return Ok([0.0; 3]);
        }
        let middle = u64::from(self.u8()?);
        let highest = u64::from(self.i32()? as u32);
        let buffer = highest << 16 | middle << 8 | lowest;
        let mut scale = lowest & 3;
        if lowest & 4 == 4 {
            scale |= u64::from(self.varint()? as u32) << 2;
        }
        let s = scale as f64;
        Ok([
            unpack_lp(buffer >> 3) * s,
            unpack_lp(buffer >> 18) * s,
            unpack_lp(buffer >> 33) * s,
        ])
    }

    /// One network NBT value with an anonymous root (1.20.2 and later).
    pub fn nbt(&mut self) -> Result<Tag> {
        let start = self.pos;
        let (tag, used) = Tag::read_network(&self.data[start..])?;
        self.pos = start + used;
        Ok(tag)
    }

    /// Skips one network NBT value.
    pub fn nbt_raw(&mut self) -> Result<()> {
        let start = self.pos;
        let (_, used) = Tag::read_network(&self.data[start..])?;
        self.pos = start + used;
        Ok(())
    }

    /// A count (VarInt) of at most `max` elements.
    pub fn count(&mut self, max: usize) -> Result<usize> {
        let n = self.varint()?;
        if n < 0 || n as usize > max {
            return Err(Error::new(format!("count {n} is past {max}")));
        }
        Ok(n as usize)
    }

    /// A VarInt-prefixed byte array of at most `max` bytes.
    pub fn byte_array(&mut self, max: usize) -> Result<Vec<u8>> {
        let n = self.count(max)?;
        Ok(self.take(n)?.to_vec())
    }
}

/// Writes a packet body.
#[derive(Default)]
pub struct Writer {
    pub data: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Writer { data: Vec::new() }
    }

    pub fn raw(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    pub fn u8(&mut self, v: u8) {
        self.data.push(v);
    }

    pub fn i8(&mut self, v: i8) {
        self.data.push(v as u8);
    }

    pub fn bool(&mut self, v: bool) {
        self.data.push(u8::from(v));
    }

    pub fn i16(&mut self, v: i16) {
        self.data.extend_from_slice(&v.to_be_bytes());
    }

    pub fn i32(&mut self, v: i32) {
        self.data.extend_from_slice(&v.to_be_bytes());
    }

    pub fn i64(&mut self, v: i64) {
        self.data.extend_from_slice(&v.to_be_bytes());
    }

    pub fn f32(&mut self, v: f32) {
        self.i32(v.to_bits() as i32);
    }

    pub fn f64(&mut self, v: f64) {
        self.i64(v.to_bits() as i64);
    }

    pub fn varint(&mut self, v: i32) {
        let mut v = v as u32;
        loop {
            if v & !0x7F == 0 {
                self.data.push(v as u8);
                return;
            }
            self.data.push((v & 0x7F | 0x80) as u8);
            v >>= 7;
        }
    }

    pub fn varlong(&mut self, v: i64) {
        let mut v = v as u64;
        loop {
            if v & !0x7F == 0 {
                self.data.push(v as u8);
                return;
            }
            self.data.push((v & 0x7F | 0x80) as u8);
            v >>= 7;
        }
    }

    pub fn uuid(&mut self, v: u128) {
        self.i64((v >> 64) as u64 as i64);
        self.i64(v as u64 as i64);
    }

    pub fn string(&mut self, s: &str) {
        self.varint(s.len() as i32);
        self.data.extend_from_slice(s.as_bytes());
    }

    pub fn identifier(&mut self, s: &str) {
        self.string(s);
    }

    pub fn position(&mut self, p: [i32; 3]) {
        self.i64(pack_block_pos(p));
    }

    /// A VarInt-prefixed byte array.
    pub fn byte_array(&mut self, bytes: &[u8]) {
        self.varint(bytes.len() as i32);
        self.data.extend_from_slice(bytes);
    }

    pub fn nbt(&mut self, tag: &Tag) {
        tag.write_network(&mut self.data);
    }

    /// `LpVec3`, quantized as the game does (the largest component sets the scale).
    pub fn lp_vec3(&mut self, v: [f64; 3]) {
        let v = v.map(|c| {
            if c.is_nan() {
                0.0
            } else {
                c.clamp(-1.717_986_918_3e10, 1.717_986_918_3e10)
            }
        });
        let length = v[0].abs().max(v[1].abs()).max(v[2].abs());
        if length < 3.051_944_088_384_301e-5 {
            self.u8(0);
            return;
        }
        // As Vec3.LP_STREAM_CODEC: the scale in the low bits, bit 2 when it needs a VarInt tail.
        let scale = length.ceil() as u64;
        let partial = scale & 3 != scale;
        let markers = if partial { scale & 3 | 4 } else { scale };
        let s = scale as f64;
        let pack = |c: f64| -> u64 { ((c * 0.5 + 0.5) * 32766.0).round() as u64 };
        let buffer = markers | pack(v[0] / s) << 3 | pack(v[1] / s) << 18 | pack(v[2] / s) << 33;
        self.u8(buffer as u8);
        self.u8((buffer >> 8) as u8);
        self.i32((buffer >> 16) as u32 as i32);
        if partial {
            self.varint((scale >> 2) as i32);
        }
    }
}

/// Packs a block position (`BlockPos.asLong`).
pub fn pack_block_pos(p: [i32; 3]) -> i64 {
    ((i64::from(p[0]) & 0x3FF_FFFF) << 38)
        | ((i64::from(p[2]) & 0x3FF_FFFF) << 12)
        | (i64::from(p[1]) & 0xFFF)
}

/// Unpacks a block position (`BlockPos.of`).
pub fn unpack_block_pos(v: i64) -> [i32; 3] {
    [
        (v >> 38) as i32,
        (v << 52 >> 52) as i32,
        (v << 26 >> 38) as i32,
    ]
}

/// Packs a section position: x (22 bits), z (22 bits), y (20 bits).
pub fn pack_section_pos(p: [i32; 3]) -> i64 {
    ((i64::from(p[0]) & 0x3F_FFFF) << 42)
        | ((i64::from(p[2]) & 0x3F_FFFF) << 20)
        | (i64::from(p[1]) & 0xF_FFFF)
}

/// Unpacks a section position.
pub fn unpack_section_pos(v: i64) -> [i32; 3] {
    [
        (v >> 42) as i32,
        (v << 44 >> 44) as i32,
        (v << 22 >> 42) as i32,
    ]
}

fn unpack_lp(v: u64) -> f64 {
    ((v & 32767) as f64).min(32766.0) * 2.0 / 32766.0 - 1.0
}
