//! Canon CR3 (ISOBMFF, see lclevy/canon_cr3).
//!
//! ```text
//! ftyp 'crx '
//! moov
//!   uuid 85c0b687-820f-11e0-8111-f4ce462b6a48   Canon metadata
//!     CMT1  TIFF: IFD0          CMT2  TIFF: EXIF IFD
//!     CMT3  TIFF: Canon notes   CMT4  TIFF: GPS IFD
//!     THMB  160x120 JPEG
//!   trak × 4  (1: full-size JPEG, 2: small raw, 3: raw, 4: timed metadata)
//!     mdia/minf/stbl/{stsz, co64|stco}  → first sample = the image
//! uuid be7acfcb-97a9-42e8-9c71-999491e3afac     XMP packet
//! uuid eaf42b5e-1c98-4b88-b9fb-b7dc406e4d16     PRVW: 1620x1080 JPEG
//! mdat
//! ```
//!
//! Cameras shooting in HDR PQ mode (R5 Mark II, R1, …) store HEVC images
//! instead of JPEGs in THMB/PRVW/track 1 (`CISZ`/`hvcC`): those files have
//! no decodable preview and are reported as unsupported.

use super::{Candidate, RawProbe};
use crate::exif::find;
use crate::fault::{FResult, Fault};
use crate::makernote;
use crate::source::{ByteSource, MemSource};
use crate::tiff::Tiff;

pub(crate) const UUID_CANON: [u8; 16] = *b"\x85\xc0\xb6\x87\x82\x0f\x11\xe0\x81\x11\xf4\xce\x46\x2b\x6a\x48";
pub(crate) const UUID_XMP: [u8; 16] = *b"\xbe\x7a\xcf\xcb\x97\xa9\x42\xe8\x9c\x71\x99\x94\x91\xe3\xaf\xac";
pub(crate) const UUID_PREVIEW: [u8; 16] = *b"\xea\xf4\x2b\x5e\x1c\x98\x4b\x88\xb9\xfb\xb7\xdc\x40\x6e\x4d\x16";

/// Largest metadata box we load into memory.
const MAX_META_BOX: u64 = 4 * 1024 * 1024;
const MAX_BOXES: usize = 256;

#[derive(Debug, Clone, Copy)]
pub(crate) struct BoxHdr {
    pub start: u64,
    pub typ: [u8; 4],
    pub uuid: Option<[u8; 16]>,
    /// First byte after the header (and the uuid).
    pub payload: u64,
    pub end: u64,
}

/// Reads the box header at `pos` inside a parent ending at `parent_end`.
pub(crate) fn read_box(src: &mut dyn ByteSource, pos: u64, parent_end: u64) -> FResult<Option<BoxHdr>> {
    if pos.saturating_add(8) > parent_end {
        return Ok(None);
    }
    let h = src.read_at(pos, 8)?;
    let size32 = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
    let typ = [h[4], h[5], h[6], h[7]];
    let (size, mut hdr) = match size32 {
        0 => (parent_end - pos, 8u64),
        1 => (src.read_u64_be(pos + 8)?, 16u64),
        s => (s as u64, 8u64),
    };
    let end = pos.checked_add(size).ok_or_else(|| Fault::corrupt("box size overflow"))?;
    if size < hdr || end > parent_end {
        return Err(Fault::corrupt(format!("box '{}' overruns its parent", String::from_utf8_lossy(&typ))));
    }
    let mut uuid = None;
    if &typ == b"uuid" {
        if size < hdr + 16 {
            return Err(Fault::corrupt("short uuid box"));
        }
        let u = src.read_at(pos + hdr, 16)?;
        let mut a = [0u8; 16];
        a.copy_from_slice(&u);
        uuid = Some(a);
        hdr += 16;
    }
    Ok(Some(BoxHdr { start: pos, typ, uuid, payload: pos + hdr, end }))
}

/// Iterates over the children of a box (or of the file).
fn children(src: &mut dyn ByteSource, start: u64, end: u64) -> FResult<Vec<BoxHdr>> {
    let mut out = Vec::new();
    let mut pos = start;
    while let Some(b) = read_box(src, pos, end)? {
        out.push(b);
        if out.len() >= MAX_BOXES {
            break;
        }
        pos = b.end;
    }
    Ok(out)
}

pub(super) fn probe(src: &mut dyn ByteSource, probe: &mut RawProbe, cands: &mut Vec<Candidate>) -> FResult<()> {
    let len = src.len();
    let mut hevc = false;
    let mut pos = 0;
    let mut count = 0;
    // Top level: stop at the first unreadable box (truncated files still
    // expose their moov and previews).
    loop {
        let b = match read_box(src, pos, len) {
            Ok(Some(b)) => b,
            Ok(None) => break,
            Err(e) if count > 0 => {
                log::debug!("CR3: stopping at {pos}: {e:?}");
                break;
            }
            Err(e) => return Err(e),
        };
        count += 1;
        match (&b.typ, b.uuid) {
            (b"moov", _) => moov(src, &b, probe, cands, &mut hevc)?,
            (b"uuid", Some(UUID_XMP)) if b.end - b.payload <= MAX_META_BOX => {
                let xmp = src.read_at(b.payload, (b.end - b.payload) as usize)?;
                probe.meta.read_xmp(&xmp);
            }
            (b"uuid", Some(UUID_PREVIEW)) => {
                // 8 bytes of unknown data, then the PRVW box.
                for c in children(src, b.payload + 8, b.end)? {
                    if &c.typ == b"PRVW" {
                        image_box(src, &c, 20, cands, &mut hevc)?;
                    }
                }
            }
            _ => {}
        }
        if count >= MAX_BOXES {
            break;
        }
        pos = b.end;
    }
    if hevc {
        probe.unsupported = Some("CR3 with HEVC previews (HDR PQ / HEIF mode) is not supported".into());
    }
    Ok(())
}

fn moov(
    src: &mut dyn ByteSource,
    moov: &BoxHdr,
    probe: &mut RawProbe,
    cands: &mut Vec<Candidate>,
    hevc: &mut bool,
) -> FResult<()> {
    for b in children(src, moov.payload, moov.end)? {
        match (&b.typ, b.uuid) {
            (b"uuid", Some(UUID_CANON)) => canon_uuid(src, &b, probe, cands, hevc)?,
            (b"trak", _) => {
                if let Some(c) = trak(src, &b)? {
                    cands.push(c);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn canon_uuid(
    src: &mut dyn ByteSource,
    uuid: &BoxHdr,
    probe: &mut RawProbe,
    cands: &mut Vec<Candidate>,
    hevc: &mut bool,
) -> FResult<()> {
    let boxes = children(src, uuid.payload, uuid.end)?;
    // Read in priority order: IFD0, EXIF, MakerNote, GPS.
    for name in [b"CMT1", b"CMT2", b"CMT3", b"CMT4"] {
        let Some(b) = boxes.iter().find(|b| &b.typ == name) else { continue };
        let size = b.end - b.payload;
        if size > MAX_META_BOX {
            continue;
        }
        let bytes = src.read_at(b.payload, size as usize)?;
        let mut mem = MemSource::new(&bytes);
        let Ok(tiff) = Tiff::parse(&mut mem, 0) else { continue };
        let Ok(ifd) = tiff.read_ifd(&mut mem, tiff.ifd0) else { continue };
        let meta = &mut probe.meta;
        match name {
            b"CMT1" => meta.read_main_ifd(&mut mem, &tiff, &ifd),
            b"CMT2" => meta.read_exif_ifd(&mut mem, &tiff, &ifd),
            b"CMT3" => meta.apply_note(makernote::canon(&mut mem, &tiff, &ifd)),
            _ => meta.read_gps_ifd(&mut mem, &tiff, &ifd),
        }
    }
    if let Some(b) = boxes.iter().find(|b| &b.typ == b"THMB") {
        image_box(src, b, 16, cands, hevc)?;
    }
    Ok(())
}

/// THMB / PRVW: a 16-byte header then the image (a JPEG in version 0, HEVC
/// in version 1 / HDR PQ files). The image size is at `size_at` from the box
/// start: +16 in THMB (`ver/flags, w, h, size, ?`), +20 in PRVW
/// (`ver/flags, ?, w, h, ?, size`).
fn image_box(
    src: &mut dyn ByteSource,
    b: &BoxHdr,
    size_at: u64,
    cands: &mut Vec<Candidate>,
    hevc: &mut bool,
) -> FResult<()> {
    let data = b.start + 24;
    if data + 4 > b.end {
        return Ok(());
    }
    let size = src.read_u32_be(b.start + size_at)? as u64;
    let probe = src.read_upto(data, 64)?;
    if probe.starts_with(&[0xFF, 0xD8]) {
        let size = if size > 0 && data + size <= b.end { size } else { b.end - data };
        cands.push(Candidate::jpeg(data, size));
    } else if find(&probe, b"hvcC").is_some() || find(&probe, b"CISZ").is_some() {
        *hevc = true;
    }
    Ok(())
}

/// First sample of a track (track 1 holds the full-size JPEG).
fn trak(src: &mut dyn ByteSource, trak: &BoxHdr) -> FResult<Option<Candidate>> {
    let Some(mdia) = child(src, trak, b"mdia")? else { return Ok(None) };
    let Some(minf) = child(src, &mdia, b"minf")? else { return Ok(None) };
    let Some(stbl) = child(src, &minf, b"stbl")? else { return Ok(None) };
    let mut size = None;
    let mut offset = None;
    for b in children(src, stbl.payload, stbl.end)? {
        match &b.typ {
            // version/flags(4) sample_size(4) count(4) [entries]
            b"stsz" if b.end >= b.payload + 12 => {
                let fixed = src.read_u32_be(b.payload + 4)?;
                let count = src.read_u32_be(b.payload + 8)?;
                size = if fixed != 0 {
                    Some(fixed as u64)
                } else if count > 0 && b.end >= b.payload + 16 {
                    Some(src.read_u32_be(b.payload + 12)? as u64)
                } else {
                    None
                };
            }
            // version/flags(4) count(4) entries
            b"co64" if b.end >= b.payload + 16 => offset = Some(src.read_u64_be(b.payload + 8)?),
            b"stco" if b.end >= b.payload + 12 => offset = Some(src.read_u32_be(b.payload + 8)? as u64),
            _ => {}
        }
    }
    Ok(match (offset, size) {
        (Some(o), Some(s)) if o > 0 && s > 0 => Some(Candidate::jpeg(o, s)),
        _ => None,
    })
}

fn child(src: &mut dyn ByteSource, parent: &BoxHdr, typ: &[u8; 4]) -> FResult<Option<BoxHdr>> {
    Ok(children(src, parent.payload, parent.end)?.into_iter().find(|b| &b.typ == typ))
}
