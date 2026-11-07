//! Vendor MakerNotes: only what EXIF lacks on some bodies — the body serial
//! (two bodies of the same model at a wedding), the lens name, and the
//! preview JPEG that Olympus/OM System and Pentax store there.
//!
//! Every vendor uses its own header and offset base; all parsing here is
//! best effort and failures are silent (the photo still decodes).

use crate::exif::NotePreview;
use crate::fault::FResult;
use crate::source::ByteSource;
use crate::tiff::{clean_string, ty, Endian, Entry, Ifd, Tiff};

#[derive(Debug, Default)]
pub(crate) struct MakerNote {
    pub serial: Option<String>,
    pub lens: Option<String>,
    pub previews: Vec<NotePreview>,
}

/// Reads the MakerNote stored in entry `e` of an EXIF IFD of `tiff`.
pub(crate) fn read(src: &mut dyn ByteSource, tiff: &Tiff, e: &Entry, make: &str) -> MakerNote {
    match read_inner(src, tiff, e, make) {
        Ok(n) => n,
        Err(err) => {
            log::debug!("MakerNote ({make}) unreadable: {err:?}");
            MakerNote::default()
        }
    }
}

fn read_inner(src: &mut dyn ByteSource, tiff: &Tiff, e: &Entry, make: &str) -> FResult<MakerNote> {
    let start = tiff.value_pos(e);
    if e.count < 16 {
        return Ok(MakerNote::default());
    }
    let head = src.read_upto(start, 16)?;
    let make = make.to_ascii_lowercase();

    if head.starts_with(b"Nikon\0") {
        // Type 3: "Nikon\0" + version + an embedded TIFF header at +10.
        let inner = Tiff::parse(src, start + 10)?;
        let ifd = inner.read_ifd(src, inner.ifd0)?;
        return Ok(MakerNote { serial: string(src, &inner, &ifd, 0x001D), ..Default::default() });
    }
    if head.starts_with(b"OLYMPUS\0") || head.starts_with(b"OM SYSTEM\0") {
        let (hdr, en_at) = if head.starts_with(b"OLYMPUS\0") { (12u32, 8) } else { (16u32, 12) };
        let endian = endian_mark(head.get(en_at..en_at + 2).unwrap_or_default()).unwrap_or(tiff.endian);
        return olympus(src, tiff, Tiff::headerless(start, endian), hdr);
    }
    if head.starts_with(b"OLYMP\0") {
        // Old style: offsets relative to the EXIF TIFF header.
        let rel = rel_offset(tiff, start + 8)?;
        return olympus(src, tiff, Tiff::headerless(tiff.base, tiff.endian), rel);
    }
    if head.starts_with(b"PENTAX \0") {
        let endian = endian_mark(head.get(8..10).unwrap_or_default()).unwrap_or(tiff.endian);
        return pentax(src, tiff, Tiff::headerless(start, endian), 10);
    }
    if head.starts_with(b"AOC\0") {
        let endian = endian_mark(head.get(4..6).unwrap_or_default()).unwrap_or(tiff.endian);
        let note = Tiff::headerless(tiff.base, endian);
        return pentax(src, tiff, note, rel_offset(tiff, start + 6)?);
    }
    if head.starts_with(b"FUJIFILM") {
        let off = Endian::Little.u32_at(&head, 8).unwrap_or(12);
        let note = Tiff::headerless(start, Endian::Little);
        let ifd = note.read_ifd(src, off)?;
        return Ok(MakerNote { serial: string(src, &note, &ifd, 0x0010), ..Default::default() });
    }
    if head.starts_with(b"Panasonic\0") {
        let note = Tiff::headerless(tiff.base, tiff.endian);
        let ifd = note.read_ifd(src, rel_offset(tiff, start + 12)?)?;
        return Ok(MakerNote { serial: string(src, &note, &ifd, 0x0025), ..Default::default() });
    }
    if make.starts_with("canon") {
        let note = Tiff::headerless(tiff.base, tiff.endian);
        let ifd = note.read_ifd(src, rel_offset(tiff, start)?)?;
        return Ok(canon(src, &note, &ifd));
    }
    Ok(MakerNote::default())
}

/// Canon MakerNote IFD (also the content of CR3 `CMT3`).
pub(crate) fn canon(src: &mut dyn ByteSource, note: &Tiff, ifd: &Ifd) -> MakerNote {
    let serial = ifd
        .get(0x000C)
        .and_then(|e| note.uint(src, e))
        .filter(|v| *v != 0)
        .map(|v| v.to_string())
        .or_else(|| string(src, note, ifd, 0x0096));
    MakerNote { serial, lens: string(src, note, ifd, 0x0095), previews: Vec::new() }
}

fn olympus(src: &mut dyn ByteSource, exif: &Tiff, note: Tiff, ifd_off: u32) -> FResult<MakerNote> {
    let ifd = note.read_ifd(src, ifd_off)?;
    let mut out = MakerNote::default();
    if let Some(eq) = ifd.get(0x2010).and_then(|e| sub_ifd(src, &note, e)) {
        out.serial = string(src, &note, &eq, 0x0101);
        out.lens = string(src, &note, &eq, 0x0203);
    }
    if let Some(cs) = ifd.get(0x2020).and_then(|e| sub_ifd(src, &note, e)) {
        let valid = cs.get(0x0100).and_then(|e| note.uint(src, e)).unwrap_or(1);
        let start = cs.get(0x0101).and_then(|e| note.uint(src, e));
        let len = cs.get(0x0102).and_then(|e| note.uint(src, e));
        if let (true, Some(start), Some(len)) = (valid != 0, start, len) {
            if start > 0 && len > 0 {
                out.previews.push(NotePreview {
                    offset: note.abs(start),
                    len: len as u64,
                    alt_offset: Some(exif.abs(start)),
                });
            }
        }
    }
    Ok(out)
}

fn pentax(src: &mut dyn ByteSource, exif: &Tiff, note: Tiff, ifd_off: u32) -> FResult<MakerNote> {
    let ifd = note.read_ifd(src, ifd_off)?;
    let mut out = MakerNote { serial: string(src, &note, &ifd, 0x0229), ..Default::default() };
    let len = ifd.get(0x0004).and_then(|e| note.uint(src, e));
    let start = ifd.get(0x0005).and_then(|e| note.uint(src, e));
    if let (Some(start), Some(len)) = (start, len) {
        if start > 0 && len > 0 {
            out.previews.push(NotePreview {
                offset: note.abs(start),
                len: len as u64,
                alt_offset: Some(exif.abs(start)),
            });
        }
    }
    Ok(out)
}

/// Sub-IFD pointed to by a MakerNote tag: an offset (types LONG/IFD), or for
/// old Olympus notes an UNDEFINED blob that *is* the IFD.
fn sub_ifd(src: &mut dyn ByteSource, note: &Tiff, e: &Entry) -> Option<Ifd> {
    let off = if e.typ == ty::UNDEFINED && e.count > 4 {
        u32::try_from(note.value_pos(e).checked_sub(note.base)?).ok()?
    } else {
        note.uint(src, e)?
    };
    note.read_ifd(src, off).ok()
}

fn string(src: &mut dyn ByteSource, t: &Tiff, ifd: &Ifd, tag: u16) -> Option<String> {
    let e = ifd.get(tag)?;
    let b = t.bytes(src, e, 256).ok()?;
    clean_string(&b).filter(|s| s.bytes().any(|c| c != b'0' && c != b' '))
}

fn endian_mark(b: &[u8]) -> Option<Endian> {
    match b {
        b"II" => Some(Endian::Little),
        b"MM" => Some(Endian::Big),
        _ => None,
    }
}

fn rel_offset(tiff: &Tiff, abs: u64) -> FResult<u32> {
    abs.checked_sub(tiff.base)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| crate::fault::Fault::corrupt("MakerNote offset out of range"))
}
