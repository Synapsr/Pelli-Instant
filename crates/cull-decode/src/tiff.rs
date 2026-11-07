//! Minimal lazy TIFF reader.
//!
//! Every RAW format except CR3/RAF is a TIFF container, and the EXIF block of
//! JPEG/PNG/HEIF/CR3/RAF files is a TIFF stream too. This reader only fetches
//! what it is asked for (an IFD, one tag value) through a [`ByteSource`], so a
//! 60 MB RAW is inspected with a few kilobytes of I/O. All sizes and offsets
//! read from the file are checked; nothing here panics on hostile input.

use crate::fault::{FResult, Fault};
use crate::source::ByteSource;

/// Largest number of entries accepted in one IFD (real files: < 300).
const MAX_ENTRIES: u16 = 4096;
/// Largest tag value fetched into memory (XMP packets are the biggest).
pub(crate) const MAX_VALUE: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Endian {
    Little,
    Big,
}

impl Endian {
    pub(crate) fn u16(self, b: [u8; 2]) -> u16 {
        match self {
            Endian::Little => u16::from_le_bytes(b),
            Endian::Big => u16::from_be_bytes(b),
        }
    }

    pub(crate) fn u32(self, b: [u8; 4]) -> u32 {
        match self {
            Endian::Little => u32::from_le_bytes(b),
            Endian::Big => u32::from_be_bytes(b),
        }
    }

    /// `u16` at `i` in `b`, `None` when out of bounds.
    pub(crate) fn u16_at(self, b: &[u8], i: usize) -> Option<u16> {
        let s = b.get(i..i.checked_add(2)?)?;
        Some(self.u16([s[0], s[1]]))
    }

    /// `u32` at `i` in `b`, `None` when out of bounds.
    pub(crate) fn u32_at(self, b: &[u8], i: usize) -> Option<u32> {
        let s = b.get(i..i.checked_add(4)?)?;
        Some(self.u32([s[0], s[1], s[2], s[3]]))
    }

    pub(crate) fn put_u16(self, v: u16) -> [u8; 2] {
        match self {
            Endian::Little => v.to_le_bytes(),
            Endian::Big => v.to_be_bytes(),
        }
    }
}

/// TIFF field types we decode.
pub(crate) mod ty {
    pub const BYTE: u16 = 1;
    pub const ASCII: u16 = 2;
    pub const SHORT: u16 = 3;
    pub const LONG: u16 = 4;
    pub const RATIONAL: u16 = 5;
    pub const SBYTE: u16 = 6;
    pub const UNDEFINED: u16 = 7;
    pub const SSHORT: u16 = 8;
    pub const SLONG: u16 = 9;
    pub const SRATIONAL: u16 = 10;
    pub const FLOAT: u16 = 11;
    pub const DOUBLE: u16 = 12;
    pub const IFD: u16 = 13;
}

fn type_size(typ: u16) -> Option<u64> {
    Some(match typ {
        ty::BYTE | ty::ASCII | ty::SBYTE | ty::UNDEFINED => 1,
        ty::SHORT | ty::SSHORT => 2,
        ty::LONG | ty::SLONG | ty::FLOAT | ty::IFD => 4,
        ty::RATIONAL | ty::SRATIONAL | ty::DOUBLE => 8,
        16..=18 => 8, // BigTIFF LONG8 / SLONG8 / IFD8
        _ => return None,
    })
}

/// One IFD entry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Entry {
    pub tag: u16,
    pub typ: u16,
    pub count: u32,
    /// The 4-byte value/offset field, in file byte order.
    raw: [u8; 4],
    /// Absolute position of `raw` in the source.
    raw_pos: u64,
}

impl Entry {
    /// Size of the value in bytes, `None` for unknown types or overflow.
    pub(crate) fn byte_len(&self) -> Option<u64> {
        type_size(self.typ)?.checked_mul(self.count as u64)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Ifd {
    pub entries: Vec<Entry>,
    /// Offset of the next IFD in the chain (0 = end).
    pub next: u32,
}

impl Ifd {
    pub(crate) fn get(&self, tag: u16) -> Option<&Entry> {
        self.entries.iter().find(|e| e.tag == tag)
    }
}

/// A TIFF stream located at `base` in a source; all offsets inside it are
/// relative to `base`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Tiff {
    pub base: u64,
    pub endian: Endian,
    /// TIFF magic number: 42 (TIFF), 0x4F52/0x5352 (Olympus ORF), 0x55 (RW2).
    pub magic: u16,
    /// Offset of IFD0.
    pub ifd0: u32,
}

pub(crate) const MAGIC_TIFF: u16 = 42;
pub(crate) const MAGIC_RW2: u16 = 0x55;
pub(crate) const MAGIC_ORF: u16 = 0x4F52;
pub(crate) const MAGIC_ORF_S: u16 = 0x5352;

/// `Some(endian, magic)` if `head` starts like a (possibly vendor) TIFF.
pub(crate) fn sniff(head: &[u8]) -> Option<(Endian, u16)> {
    let endian = match head.get(0..2)? {
        b"II" => Endian::Little,
        b"MM" => Endian::Big,
        _ => return None,
    };
    let magic = endian.u16_at(head, 2)?;
    match magic {
        MAGIC_TIFF | MAGIC_RW2 | MAGIC_ORF | MAGIC_ORF_S => Some((endian, magic)),
        _ => None,
    }
}

impl Tiff {
    /// Parses the TIFF header at `base`.
    pub(crate) fn parse(src: &mut dyn ByteSource, base: u64) -> FResult<Tiff> {
        let head = src.read_at(base, 8).map_err(|_| Fault::corrupt("truncated TIFF header"))?;
        let (endian, magic) = sniff(&head).ok_or_else(|| Fault::corrupt("not a TIFF stream"))?;
        let ifd0 = endian.u32_at(&head, 4).unwrap_or(0);
        Ok(Tiff { base, endian, magic, ifd0 })
    }

    /// A TIFF-like structure whose IFD offsets are relative to `base` but
    /// that has no header of its own (MakerNotes).
    pub(crate) fn headerless(base: u64, endian: Endian) -> Tiff {
        Tiff { base, endian, magic: MAGIC_TIFF, ifd0: 0 }
    }

    /// Absolute position of a relative offset.
    pub(crate) fn abs(&self, offset: u32) -> u64 {
        self.base + offset as u64
    }

    pub(crate) fn read_ifd(&self, src: &mut dyn ByteSource, offset: u32) -> FResult<Ifd> {
        if offset == 0 {
            return Err(Fault::corrupt("null IFD offset"));
        }
        let pos = self.abs(offset);
        let count = self.endian.u16(arr2(&src.read_at(pos, 2)?));
        if count == 0 || count > MAX_ENTRIES {
            return Err(Fault::corrupt(format!("implausible IFD entry count {count}")));
        }
        let table_len = count as usize * 12;
        // The next-IFD pointer may be missing at the very end of a file.
        let table = src.read_upto(pos + 2, table_len + 4)?;
        if table.len() < table_len {
            return Err(Fault::corrupt("truncated IFD"));
        }
        let mut entries = Vec::with_capacity(count as usize);
        for (i, e) in table[..table_len].chunks_exact(12).enumerate() {
            entries.push(Entry {
                tag: self.endian.u16([e[0], e[1]]),
                typ: self.endian.u16([e[2], e[3]]),
                count: self.endian.u32([e[4], e[5], e[6], e[7]]),
                raw: [e[8], e[9], e[10], e[11]],
                raw_pos: pos + 2 + i as u64 * 12 + 8,
            });
        }
        let next = self.endian.u32_at(&table, table_len).unwrap_or(0);
        Ok(Ifd { entries, next })
    }

    /// Absolute position of the value of `e` (inline values live in the entry).
    pub(crate) fn value_pos(&self, e: &Entry) -> u64 {
        match e.byte_len() {
            Some(n) if n <= 4 => e.raw_pos,
            _ => self.abs(self.endian.u32(e.raw)),
        }
    }

    /// Raw value bytes, at most `limit` of them.
    pub(crate) fn bytes(&self, src: &mut dyn ByteSource, e: &Entry, limit: usize) -> FResult<Vec<u8>> {
        let n = e.byte_len().ok_or_else(|| Fault::corrupt(format!("tag {:#x}: bad type", e.tag)))?;
        if n <= 4 {
            return Ok(e.raw[..n as usize].to_vec());
        }
        let n = usize::try_from(n).unwrap_or(usize::MAX).min(limit);
        src.read_at(self.abs(self.endian.u32(e.raw)), n)
    }

    /// Unsigned integer values (BYTE/SHORT/LONG/IFD and signed variants).
    pub(crate) fn uints(&self, src: &mut dyn ByteSource, e: &Entry) -> FResult<Vec<u32>> {
        let b = self.bytes(src, e, MAX_VALUE)?;
        let en = self.endian;
        Ok(match e.typ {
            ty::BYTE | ty::UNDEFINED | ty::SBYTE => b.iter().map(|&v| v as u32).collect(),
            ty::SHORT | ty::SSHORT => b.chunks_exact(2).map(|c| en.u16([c[0], c[1]]) as u32).collect(),
            ty::LONG | ty::SLONG | ty::IFD => b.chunks_exact(4).map(|c| en.u32([c[0], c[1], c[2], c[3]])).collect(),
            _ => Vec::new(),
        })
    }

    /// First unsigned integer value.
    pub(crate) fn uint(&self, src: &mut dyn ByteSource, e: &Entry) -> Option<u32> {
        let inline = e.count >= 1 && e.byte_len().is_some_and(|n| n <= 4);
        if inline {
            match e.typ {
                ty::SHORT | ty::SSHORT => return Some(self.endian.u16([e.raw[0], e.raw[1]]) as u32),
                ty::LONG | ty::SLONG | ty::IFD => return Some(self.endian.u32(e.raw)),
                ty::BYTE | ty::UNDEFINED | ty::SBYTE => return Some(e.raw[0] as u32),
                _ => {}
            }
        }
        self.uints(src, e).ok()?.first().copied()
    }

    /// Numeric values as `f64` (rationals, integers, floats).
    pub(crate) fn numbers(&self, src: &mut dyn ByteSource, e: &Entry) -> FResult<Vec<f64>> {
        let en = self.endian;
        Ok(match e.typ {
            ty::RATIONAL | ty::SRATIONAL => {
                let b = self.bytes(src, e, MAX_VALUE)?;
                b.chunks_exact(8)
                    .map(|c| {
                        let n = en.u32([c[0], c[1], c[2], c[3]]);
                        let d = en.u32([c[4], c[5], c[6], c[7]]);
                        let (n, d) = if e.typ == ty::SRATIONAL {
                            (n as i32 as f64, d as i32 as f64)
                        } else {
                            (n as f64, d as f64)
                        };
                        if d == 0.0 {
                            f64::NAN
                        } else {
                            n / d
                        }
                    })
                    .collect()
            }
            ty::FLOAT => {
                let b = self.bytes(src, e, MAX_VALUE)?;
                b.chunks_exact(4).map(|c| f32::from_bits(en.u32([c[0], c[1], c[2], c[3]])) as f64).collect()
            }
            ty::DOUBLE => {
                let b = self.bytes(src, e, MAX_VALUE)?;
                b.chunks_exact(8)
                    .map(|c| {
                        let mut a = [0u8; 8];
                        a.copy_from_slice(c);
                        match en {
                            Endian::Little => f64::from_le_bytes(a),
                            Endian::Big => f64::from_be_bytes(a),
                        }
                    })
                    .collect()
            }
            ty::SSHORT => self.uints(src, e)?.into_iter().map(|v| v as u16 as i16 as f64).collect(),
            ty::SLONG => self.uints(src, e)?.into_iter().map(|v| v as i32 as f64).collect(),
            _ => self.uints(src, e)?.into_iter().map(|v| v as f64).collect(),
        })
    }

    pub(crate) fn number(&self, src: &mut dyn ByteSource, e: &Entry) -> Option<f64> {
        self.numbers(src, e).ok()?.first().copied().filter(|v| v.is_finite())
    }

    /// Text value (ASCII/UNDEFINED/BYTE): up to the first NUL, trimmed,
    /// `None` when empty.
    pub(crate) fn string(&self, src: &mut dyn ByteSource, e: &Entry) -> Option<String> {
        if !matches!(e.typ, ty::ASCII | ty::UNDEFINED | ty::BYTE) {
            return None;
        }
        let b = self.bytes(src, e, 64 * 1024).ok()?;
        clean_string(&b)
    }
}

/// Text up to the first NUL, whitespace-trimmed; `None` when empty.
pub(crate) fn clean_string(b: &[u8]) -> Option<String> {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    let s = match std::str::from_utf8(&b[..end]) {
        Ok(s) => s.to_string(),
        Err(_) => b[..end].iter().map(|&c| c as char).collect(), // Latin-1
    };
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

fn arr2(b: &[u8]) -> [u8; 2] {
    [b.first().copied().unwrap_or(0), b.get(1).copied().unwrap_or(0)]
}

#[cfg(test)]
pub(crate) mod build {
    //! A tiny TIFF writer for synthetic test fixtures.

    pub(crate) enum Val {
        Short(Vec<u16>),
        Long(Vec<u32>),
        Ascii(String),
        Rational(Vec<(u32, u32)>),
        Undefined(Vec<u8>),
    }

    pub(crate) struct IfdSpec {
        pub entries: Vec<(u16, Val)>,
    }

    /// Lays out IFDs one after another (little-endian), each pointing to the
    /// next when `chain` is set. Pointer values (offsets to IFDs or blobs)
    /// must be patched by the caller using the returned IFD offsets, so the
    /// helper also accepts `Long` placeholders. Returns (bytes, ifd offsets).
    pub(crate) fn tiff_le(magic: u16, ifds: &[IfdSpec], chain: bool) -> (Vec<u8>, Vec<u32>) {
        let mut out = Vec::new();
        out.extend_from_slice(b"II");
        out.extend_from_slice(&magic.to_le_bytes());
        out.extend_from_slice(&8u32.to_le_bytes());
        let mut offsets = Vec::new();
        // First pass: compute IFD sizes.
        let mut pos = 8u32;
        let mut layout = Vec::new();
        for ifd in ifds {
            let table = 2 + ifd.entries.len() as u32 * 12 + 4;
            let data: u32 = ifd.entries.iter().map(|(_, v)| ext_len(v)).sum();
            layout.push((pos, table));
            offsets.push(pos);
            pos += table + data;
        }
        for (k, ifd) in ifds.iter().enumerate() {
            let (start, table) = layout[k];
            assert_eq!(out.len() as u32, start);
            let mut data_pos = start + table;
            let mut data = Vec::new();
            out.extend_from_slice(&(ifd.entries.len() as u16).to_le_bytes());
            let mut sorted: Vec<&(u16, Val)> = ifd.entries.iter().collect();
            sorted.sort_by_key(|(t, _)| *t);
            for (tag, v) in sorted {
                let (typ, count, bytes) = encode(v);
                out.extend_from_slice(&tag.to_le_bytes());
                out.extend_from_slice(&typ.to_le_bytes());
                out.extend_from_slice(&count.to_le_bytes());
                if bytes.len() <= 4 {
                    let mut b = bytes.clone();
                    b.resize(4, 0);
                    out.extend_from_slice(&b);
                } else {
                    out.extend_from_slice(&data_pos.to_le_bytes());
                    data_pos += bytes.len() as u32;
                    data.extend_from_slice(&bytes);
                }
            }
            let next = if chain && k + 1 < ifds.len() { layout[k + 1].0 } else { 0 };
            out.extend_from_slice(&next.to_le_bytes());
            out.extend_from_slice(&data);
        }
        (out, offsets)
    }

    fn ext_len(v: &Val) -> u32 {
        let n = encode(v).2.len() as u32;
        if n <= 4 {
            0
        } else {
            n
        }
    }

    fn encode(v: &Val) -> (u16, u32, Vec<u8>) {
        match v {
            Val::Short(s) => (3, s.len() as u32, s.iter().flat_map(|x| x.to_le_bytes()).collect()),
            Val::Long(s) => (4, s.len() as u32, s.iter().flat_map(|x| x.to_le_bytes()).collect()),
            Val::Ascii(s) => {
                let mut b = s.as_bytes().to_vec();
                b.push(0);
                (2, b.len() as u32, b)
            }
            Val::Rational(r) => (
                5,
                r.len() as u32,
                r.iter().flat_map(|(n, d)| n.to_le_bytes().into_iter().chain(d.to_le_bytes())).collect(),
            ),
            Val::Undefined(b) => (7, b.len() as u32, b.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::build::*;
    use super::*;
    use crate::source::MemSource;

    #[test]
    fn reads_values() {
        let (bytes, offs) = tiff_le(
            42,
            &[IfdSpec {
                entries: vec![
                    (0x010F, Val::Ascii("Canon".into())),
                    (0x0112, Val::Short(vec![6])),
                    (0x829A, Val::Rational(vec![(1, 250)])),
                    (0x0111, Val::Long(vec![1, 2, 3])),
                ],
            }],
            false,
        );
        let mut src = MemSource::new(&bytes);
        let t = Tiff::parse(&mut src, 0).unwrap();
        assert_eq!(t.ifd0, offs[0]);
        let ifd = t.read_ifd(&mut src, t.ifd0).unwrap();
        assert_eq!(t.string(&mut src, ifd.get(0x010F).unwrap()).as_deref(), Some("Canon"));
        assert_eq!(t.uint(&mut src, ifd.get(0x0112).unwrap()), Some(6));
        assert_eq!(t.number(&mut src, ifd.get(0x829A).unwrap()), Some(0.004));
        assert_eq!(t.uints(&mut src, ifd.get(0x0111).unwrap()).unwrap(), vec![1, 2, 3]);
        assert_eq!(ifd.next, 0);
    }

    #[test]
    fn rejects_garbage() {
        let mut src = MemSource::new(b"II*\0\xff\xff\xff\x7f");
        let t = Tiff::parse(&mut src, 0).unwrap();
        assert!(t.read_ifd(&mut src, t.ifd0).is_err());
        let mut src = MemSource::new(b"XX*\0");
        assert!(Tiff::parse(&mut src, 0).is_err());
        // An entry whose out-of-line value points past the end.
        let mut b = b"II*\0\x08\0\0\0\x01\0\x0f\x01\x02\0\x10\0\0\0\xff\xff\0\0\0\0\0\0".to_vec();
        b.truncate(26);
        let mut src = MemSource::new(&b);
        let t = Tiff::parse(&mut src, 0).unwrap();
        let ifd = t.read_ifd(&mut src, 8).unwrap();
        assert_eq!(t.string(&mut src, &ifd.entries[0]), None);
    }
}
