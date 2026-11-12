//! Ported from packetevents (https://github.com/retrooper/packetevents), Copyright (C) 2026
//! retrooper and contributors, GPL-3.0-or-later. See NOTICE. The structure (packet types,
//! events, priorities, wrappers) follows packetevents; the wire layouts are Minecraft 26.3's.
//!
//! Typed wrappers over packet bodies, one per packet type, named after packetevents' wrappers
//! (`WrapperPlayServerChatMessage` and so on). A wrapper reads its fields from a body (the bytes
//! after the packet id) and writes them back. Fields that are opaque to the core (item slots,
//! particles, entity data values) are kept as the raw bytes the registry measured, so they
//! round-trip exactly.

pub mod clientbound;
pub mod serverbound;

use crate::pe::codec::{Reader, Registry, Result, Writer};
use crate::pe::packet_type::PacketType;

/// A typed view of one packet type.
pub trait Wrapper: Sized {
    /// The packet this wrapper reads and writes.
    const TYPE: PacketType;

    /// Reads the fields from the cursor. `reg` measures the registry-dependent parts.
    fn read_fields(r: &mut Reader, reg: &dyn Registry) -> Result<Self>;

    /// Writes the fields.
    fn write_fields(&self, w: &mut Writer);

    /// Reads a whole body; fails when bytes are left over.
    fn from_body(body: &[u8], reg: &dyn Registry) -> Result<Self> {
        let mut r = Reader::new(body);
        let value = Self::read_fields(&mut r, reg)?;
        r.finish()?;
        Ok(value)
    }

    /// The body (the bytes after the packet id).
    fn to_body(&self) -> Vec<u8> {
        let mut w = Writer::new();
        self.write_fields(&mut w);
        w.data
    }
}

/// An item slot as the wire has it: a count, and for a stack an item id and two counts of
/// components, then those components. The core keeps the bytes; `count` reads the first VarInt.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RawItem(pub Vec<u8>);

impl RawItem {
    /// The empty slot.
    pub fn empty() -> RawItem {
        RawItem(vec![0])
    }

    /// A stack of `count` items of registry id `item`, with no components.
    pub fn simple(count: i32, item: i32) -> RawItem {
        let mut w = Writer::new();
        w.varint(count);
        if count > 0 {
            w.varint(item);
            w.varint(0);
            w.varint(0);
        }
        RawItem(w.data)
    }

    /// The stack size (0 for an empty slot), when the first VarInt is there.
    pub fn count(&self) -> Option<i32> {
        Reader::new(&self.0).varint().ok()
    }

    pub fn is_empty(&self) -> bool {
        self.count().unwrap_or(0) <= 0
    }
}

/// A particle as the wire has it: its type VarInt and the payload that type takes.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RawParticle(pub Vec<u8>);

impl RawParticle {
    /// The `minecraft:particle_type` id.
    pub fn id(&self) -> Option<i32> {
        Reader::new(&self.0).varint().ok()
    }
}

/// A synched entity value: its index, its serializer id and the value's bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityDataEntry {
    pub index: u8,
    pub serializer: i32,
    /// The value as the wire has it, without the index or serializer.
    pub value: Vec<u8>,
}

impl EntityDataEntry {
    /// The value of a byte (serializer 0).
    pub fn as_byte(&self) -> Option<i8> {
        (self.serializer == 0 && self.value.len() == 1).then(|| self.value[0] as i8)
    }

    /// The value of a VarInt serializer (int 1, direction 12, pose 20, ...).
    pub fn as_varint(&self) -> Option<i32> {
        if matches!(self.serializer, 1 | 12 | 14 | 15 | 19 | 20) {
            let mut r = Reader::new(&self.value);
            r.varint().ok().filter(|_| r.remaining() == 0)
        } else {
            None
        }
    }

    /// The value of a boolean (serializer 8).
    pub fn as_bool(&self) -> Option<bool> {
        (self.serializer == 8 && self.value.len() == 1).then(|| self.value[0] != 0)
    }

    /// The value of a float (serializer 3).
    pub fn as_float(&self) -> Option<f32> {
        (self.serializer == 3 && self.value.len() == 4).then(|| {
            f32::from_bits(u32::from_be_bytes([
                self.value[0],
                self.value[1],
                self.value[2],
                self.value[3],
            ]))
        })
    }
}

/// Reads an item slot: its length comes from the registry.
pub(crate) fn read_item(r: &mut Reader, reg: &dyn Registry) -> Result<RawItem> {
    let n = reg.item_len(r.peek())?;
    Ok(RawItem(r.bytes(n)?.to_vec()))
}

/// Reads one entity data list: entries until index 0xFF.
pub(crate) fn read_entity_data(r: &mut Reader, reg: &dyn Registry) -> Result<Vec<EntityDataEntry>> {
    let mut entries = Vec::new();
    loop {
        let index = r.u8()?;
        if index == 0xFF {
            return Ok(entries);
        }
        let serializer = r.varint()?;
        let n = reg.entity_value_len(serializer, r.peek())?;
        entries.push(EntityDataEntry {
            index,
            serializer,
            value: r.bytes(n)?.to_vec(),
        });
    }
}

pub(crate) fn write_entity_data(w: &mut Writer, entries: &[EntityDataEntry]) {
    for e in entries {
        w.u8(e.index);
        w.varint(e.serializer);
        w.raw(&e.value);
    }
    w.u8(0xFF);
}

/// A VarInt-prefixed list of `T`s, each read by `f`.
pub(crate) fn read_list<T>(
    r: &mut Reader,
    max: usize,
    mut f: impl FnMut(&mut Reader) -> Result<T>,
) -> Result<Vec<T>> {
    let n = r.count(max)?;
    let mut out = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        out.push(f(r)?);
    }
    Ok(out)
}

/// A boolean followed, when true, by a value.
pub(crate) fn read_opt<T>(
    r: &mut Reader,
    f: impl FnOnce(&mut Reader) -> Result<T>,
) -> Result<Option<T>> {
    if r.bool()? { f(r).map(Some) } else { Ok(None) }
}

pub(crate) fn write_opt<T>(w: &mut Writer, v: &Option<T>, f: impl FnOnce(&mut Writer, &T)) {
    w.bool(v.is_some());
    if let Some(x) = v {
        f(w, x);
    }
}
