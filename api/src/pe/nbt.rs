//! Network NBT, the form text components and some other fields take in 1.20.2 and later: a
//! tag id, then its payload, with no root name. Strings are modified UTF-8 (NUL and supplementary
//! characters are written the Java way).

use crate::pe::codec::{Error, Result};

/// A tag. `List` keeps its element id, as NBT does (an empty list is `End`).
#[derive(Clone, Debug, PartialEq)]
pub enum Tag {
    End,
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List(u8, Vec<Tag>),
    Compound(Vec<(String, Tag)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

/// Nesting limit (vanilla's NbtAccounter depth).
const MAX_DEPTH: usize = 512;

impl Tag {
    /// The plain text a component holds: a string root, or a compound whose only entry is `text`.
    /// None for anything with styling, translations or extra parts.
    pub fn plain_text(&self) -> Option<&str> {
        match self {
            Tag::String(s) => Some(s),
            Tag::Compound(entries) if entries.len() == 1 && entries[0].0 == "text" => match &entries[0].1 {
                Tag::String(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    /// A plain text component, as the game writes a string root.
    pub fn plain(text: &str) -> Tag {
        Tag::String(text.to_owned())
    }

    /// Decodes one network NBT value from the start of `data`; returns it and the bytes it used.
    pub fn read_network(data: &[u8]) -> Result<(Tag, usize)> {
        let mut cur = Cursor { data, pos: 0 };
        let id = cur.u8()?;
        let tag = cur.payload(id, 0)?;
        Ok((tag, cur.pos))
    }

    /// Appends the network form of this value (its id, then the payload).
    pub fn write_network(&self, out: &mut Vec<u8>) {
        out.push(self.id());
        self.write_payload(out);
    }

    fn id(&self) -> u8 {
        match self {
            Tag::End => 0,
            Tag::Byte(_) => 1,
            Tag::Short(_) => 2,
            Tag::Int(_) => 3,
            Tag::Long(_) => 4,
            Tag::Float(_) => 5,
            Tag::Double(_) => 6,
            Tag::ByteArray(_) => 7,
            Tag::String(_) => 8,
            Tag::List(..) => 9,
            Tag::Compound(_) => 10,
            Tag::IntArray(_) => 11,
            Tag::LongArray(_) => 12,
        }
    }

    fn write_payload(&self, out: &mut Vec<u8>) {
        match self {
            Tag::End => {}
            Tag::Byte(v) => out.push(*v as u8),
            Tag::Short(v) => out.extend_from_slice(&v.to_be_bytes()),
            Tag::Int(v) => out.extend_from_slice(&v.to_be_bytes()),
            Tag::Long(v) => out.extend_from_slice(&v.to_be_bytes()),
            Tag::Float(v) => out.extend_from_slice(&v.to_be_bytes()),
            Tag::Double(v) => out.extend_from_slice(&v.to_be_bytes()),
            Tag::ByteArray(v) => {
                out.extend_from_slice(&(v.len() as i32).to_be_bytes());
                out.extend(v.iter().map(|b| *b as u8));
            }
            Tag::String(s) => write_mutf8(s, out),
            Tag::List(id, items) => {
                out.push(*id);
                out.extend_from_slice(&(items.len() as i32).to_be_bytes());
                for item in items {
                    item.write_payload(out);
                }
            }
            Tag::Compound(entries) => {
                for (name, tag) in entries {
                    out.push(tag.id());
                    write_mutf8(name, out);
                    tag.write_payload(out);
                }
                out.push(0);
            }
            Tag::IntArray(v) => {
                out.extend_from_slice(&(v.len() as i32).to_be_bytes());
                for x in v {
                    out.extend_from_slice(&x.to_be_bytes());
                }
            }
            Tag::LongArray(v) => {
                out.extend_from_slice(&(v.len() as i32).to_be_bytes());
                for x in v {
                    out.extend_from_slice(&x.to_be_bytes());
                }
            }
        }
    }
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        if self.data.len() - self.pos < n {
            return Err(Error::new("NBT is cut short"));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn i32(&mut self) -> Result<i32> {
        let b = self.take(4)?;
        Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn len(&mut self) -> Result<usize> {
        let n = self.i32()?;
        if n < 0 {
            return Err(Error::new("negative NBT length"));
        }
        Ok(n as usize)
    }

    fn string(&mut self) -> Result<String> {
        let b = self.take(2)?;
        let n = u16::from_be_bytes([b[0], b[1]]) as usize;
        let bytes = self.take(n)?;
        read_mutf8(bytes)
    }

    fn payload(&mut self, id: u8, depth: usize) -> Result<Tag> {
        if depth > MAX_DEPTH {
            return Err(Error::new("NBT is nested too deep"));
        }
        Ok(match id {
            0 => Tag::End,
            1 => Tag::Byte(self.u8()? as i8),
            2 => {
                let b = self.take(2)?;
                Tag::Short(i16::from_be_bytes([b[0], b[1]]))
            }
            3 => Tag::Int(self.i32()?),
            4 => {
                let b = self.take(8)?;
                let mut a = [0u8; 8];
                a.copy_from_slice(b);
                Tag::Long(i64::from_be_bytes(a))
            }
            5 => Tag::Float(f32::from_bits(self.i32()? as u32)),
            6 => {
                let b = self.take(8)?;
                let mut a = [0u8; 8];
                a.copy_from_slice(b);
                Tag::Double(f64::from_bits(u64::from_be_bytes(a)))
            }
            7 => {
                let n = self.len()?;
                let b = self.take(n)?;
                Tag::ByteArray(b.iter().map(|x| *x as i8).collect())
            }
            8 => Tag::String(self.string()?),
            9 => {
                let elem = self.u8()?;
                let n = self.len()?;
                let mut items = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    items.push(self.payload(elem, depth + 1)?);
                }
                Tag::List(elem, items)
            }
            10 => {
                let mut entries = Vec::new();
                loop {
                    let child = self.u8()?;
                    if child == 0 {
                        break;
                    }
                    let name = self.string()?;
                    entries.push((name, self.payload(child, depth + 1)?));
                }
                Tag::Compound(entries)
            }
            11 => {
                let n = self.len()?;
                let mut v = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    v.push(self.i32()?);
                }
                Tag::IntArray(v)
            }
            12 => {
                let n = self.len()?;
                let mut v = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    let b = self.take(8)?;
                    let mut a = [0u8; 8];
                    a.copy_from_slice(b);
                    v.push(i64::from_be_bytes(a));
                }
                Tag::LongArray(v)
            }
            other => return Err(Error::new(format!("unknown NBT tag {other}"))),
        })
    }
}

/// Modified UTF-8 as Java writes it: NUL as two bytes, characters past U+FFFF as surrogate pairs.
fn write_mutf8(s: &str, out: &mut Vec<u8>) {
    let mut body = Vec::with_capacity(s.len());
    for c in s.chars() {
        let code = c as u32;
        match code {
            0 => body.extend_from_slice(&[0xC0, 0x80]),
            0x01..=0x7F => body.push(code as u8),
            0x80..=0x7FF => body.extend_from_slice(&[0xC0 | (code >> 6) as u8, 0x80 | (code & 0x3F) as u8]),
            0x800..=0xFFFF => body.extend_from_slice(&[
                0xE0 | (code >> 12) as u8,
                0x80 | ((code >> 6) & 0x3F) as u8,
                0x80 | (code & 0x3F) as u8,
            ]),
            _ => {
                let v = code - 0x1_0000;
                for unit in [0xD800 | (v >> 10), 0xDC00 | (v & 0x3FF)] {
                    body.extend_from_slice(&[
                        0xE0 | (unit >> 12) as u8,
                        0x80 | ((unit >> 6) & 0x3F) as u8,
                        0x80 | (unit & 0x3F) as u8,
                    ]);
                }
            }
        }
    }
    out.extend_from_slice(&(body.len() as u16).to_be_bytes());
    out.extend_from_slice(&body);
}

/// Reads modified UTF-8 (surrogate pairs joined, C0 80 read as NUL).
fn read_mutf8(bytes: &[u8]) -> Result<String> {
    let mut units: Vec<u16> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let (unit, used) = if b & 0x80 == 0 {
            (u16::from(b), 1)
        } else if b & 0xE0 == 0xC0 {
            let b2 = *bytes.get(i + 1).ok_or_else(|| Error::new("bad modified UTF-8"))?;
            (((u16::from(b) & 0x1F) << 6) | (u16::from(b2) & 0x3F), 2)
        } else if b & 0xF0 == 0xE0 {
            let b2 = *bytes.get(i + 1).ok_or_else(|| Error::new("bad modified UTF-8"))?;
            let b3 = *bytes.get(i + 2).ok_or_else(|| Error::new("bad modified UTF-8"))?;
            (((u16::from(b) & 0x0F) << 12) | ((u16::from(b2) & 0x3F) << 6) | (u16::from(b3) & 0x3F), 3)
        } else {
            return Err(Error::new("bad modified UTF-8"));
        };
        units.push(unit);
        i += used;
    }
    char::decode_utf16(units)
        .collect::<std::result::Result<String, _>>()
        .map_err(|_| Error::new("unpaired surrogate in NBT string"))
}
