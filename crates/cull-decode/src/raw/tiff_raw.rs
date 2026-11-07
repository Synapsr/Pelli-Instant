//! TIFF-based RAWs: CR2, NEF/NRW, ARW/SR2/SRF, DNG, ORF, PEF, SRW, RW2, 3FR,
//! ERF, MEF, MOS, KDC, DCR, IIQ…
//!
//! Previews are found in the IFD chain and its SubIFDs (tag 0x014A):
//! - `JPEGInterchangeFormat`/`Length` (0x0201/0x0202) — NEF JpgFromRaw, ARW
//!   and PEF previews, SRW, thumbnails;
//! - a single strip with Compression 6/7 — CR2 IFD0 full-size JPEG, DNG
//!   preview IFDs (lossless raw strips are rejected later by their SOF3);
//! - RW2 `JpgFromRaw` (tag 0x002E of IFD0);
//! - uncompressed 8-bit RGB strips (NEF/DNG thumbnails) as a last resort;
//! - MakerNote previews (Olympus/OM System, Pentax) via [`crate::makernote`].

use super::{Candidate, Origin, RawProbe};
use crate::exif::MetaBuilder;
use crate::fault::FResult;
use crate::source::ByteSource;
use crate::tiff::{Ifd, Tiff, MAGIC_RW2};

/// Longest IFD chain / SubIFD fan-out we follow.
const MAX_CHAIN: usize = 16;
const MAX_DEPTH: u32 = 3;

pub(super) fn probe(src: &mut dyn ByteSource, probe: &mut RawProbe, cands: &mut Vec<Candidate>) -> FResult<()> {
    let tiff = Tiff::parse(src, 0)?;
    let ifd0 = tiff.read_ifd(src, tiff.ifd0)?;
    let mut w = Walker { tiff, visited: vec![tiff.ifd0], cands, meta: &mut probe.meta };
    w.collect(src, &ifd0, 0);
    let mut next = ifd0.next;
    for _ in 0..MAX_CHAIN {
        if next == 0 || w.visited.contains(&next) {
            break;
        }
        w.visited.push(next);
        match tiff.read_ifd(src, next) {
            Ok(ifd) => {
                w.collect(src, &ifd, 0);
                next = ifd.next;
            }
            Err(e) => {
                log::debug!("IFD chain broken at {next}: {e:?}");
                break;
            }
        }
    }

    probe.meta.read_tiff(src, &tiff, &ifd0);
    if tiff.magic == MAGIC_RW2 {
        probe.exif_in_preview = true;
        // Panasonic ISO (IFD0 tag 0x0017).
        if probe.meta.iso.is_none() {
            probe.meta.iso = ifd0.get(0x0017).and_then(|e| tiff.uint(src, e)).filter(|v| *v > 0 && *v < 65535);
        }
        // Panasonic sensor borders: top, left, bottom, right (tags 4..=7).
        let b: Vec<Option<u32>> = (4u16..=7).map(|t| ifd0.get(t).and_then(|e| tiff.uint(src, e))).collect();
        if let [Some(top), Some(left), Some(bottom), Some(right)] = b[..] {
            if right > left && bottom > top {
                probe.meta.sensor_dims = Some((right - left, bottom - top));
            }
        }
    }
    Ok(())
}

struct Walker<'a> {
    tiff: Tiff,
    visited: Vec<u32>,
    cands: &'a mut Vec<Candidate>,
    meta: &'a mut MetaBuilder,
}

impl Walker<'_> {
    fn collect(&mut self, src: &mut dyn ByteSource, ifd: &Ifd, depth: u32) {
        let t = self.tiff;
        let get = |src: &mut dyn ByteSource, tag: u16| ifd.get(tag).and_then(|e| t.uint(src, e));
        let compression = get(src, 0x0103);
        let photometric = get(src, 0x0106);
        let spp = get(src, 0x0115).unwrap_or(1);
        let new_subfile = get(src, 0x00FE).unwrap_or(0);
        let bps: Vec<u32> = ifd.get(0x0102).and_then(|e| t.uints(src, e).ok()).unwrap_or_default();

        if let (Some(off), Some(len)) = (get(src, 0x0201), get(src, 0x0202)) {
            if off > 0 && len > 0 {
                self.cands.push(Candidate::jpeg(t.abs(off), len as u64));
            }
        }
        if matches!(compression, Some(6 | 7 | 99)) {
            if let Some((off, len)) = single_strip(src, &t, ifd) {
                self.cands.push(Candidate::jpeg(off, len));
            }
        }
        if t.magic == MAGIC_RW2 {
            if let Some(e) = ifd.get(0x002E) {
                if e.count > 4 {
                    self.cands.push(Candidate::jpeg(t.value_pos(e), e.count as u64));
                }
            }
        }
        let rgb8 = compression == Some(1)
            && photometric == Some(2)
            && spp == 3
            && bps.len() == 3
            && bps.iter().all(|&b| b == 8)
            && get(src, 0x011C).unwrap_or(1) == 1;
        if rgb8 {
            if let (Some(w), Some(h), Some((off, len))) =
                (get(src, 0x0100), get(src, 0x0101), contiguous_strips(src, &t, ifd))
            {
                self.cands.push(Candidate {
                    offset: off,
                    len,
                    alt_offset: None,
                    rgb: Some((w, h)),
                    origin: Origin::Structure,
                });
            }
        }

        // The raw image itself: its size is the full resolution.
        let deep = bps.len() == 1 && (10..=16).contains(&bps[0]) && spp <= 1;
        let is_raw = matches!(photometric, Some(32803 | 34892)) || deep;
        if is_raw && new_subfile & 1 == 0 {
            if let (Some(w), Some(h)) = (get(src, 0x0100), get(src, 0x0101)) {
                let area = |d: Option<(u32, u32)>| d.map_or(0, |(w, h)| w as u64 * h as u64);
                if w as u64 * h as u64 > area(self.meta.raw_dims) {
                    self.meta.raw_dims = Some((w, h));
                }
            }
            if self.meta.crop_dims.is_none() {
                if let Some(v) = ifd.get(0xC620).and_then(|e| t.numbers(src, e).ok()) {
                    if let [cw, ch, ..] = v[..] {
                        if cw >= 1.0 && ch >= 1.0 && cw < 1e6 && ch < 1e6 {
                            self.meta.crop_dims = Some((cw.round() as u32, ch.round() as u32));
                        }
                    }
                }
            }
        }

        if depth < MAX_DEPTH {
            if let Some(e) = ifd.get(0x014A) {
                let subs = t.uints(src, e).unwrap_or_default();
                for off in subs.into_iter().take(MAX_CHAIN) {
                    if off == 0 || self.visited.contains(&off) {
                        continue;
                    }
                    self.visited.push(off);
                    match t.read_ifd(src, off) {
                        Ok(sub) => self.collect(src, &sub, depth + 1),
                        Err(e) => log::debug!("SubIFD at {off} unreadable: {e:?}"),
                    }
                }
            }
        }
    }
}

/// The single strip of an IFD (multi-strip JPEG previews are independent
/// streams per strip and are not used).
fn single_strip(src: &mut dyn ByteSource, t: &Tiff, ifd: &Ifd) -> Option<(u64, u64)> {
    let offs = t.uints(src, ifd.get(0x0111)?).ok()?;
    let lens = t.uints(src, ifd.get(0x0117)?).ok()?;
    match (offs.as_slice(), lens.as_slice()) {
        ([o], [l]) if *o > 0 && *l > 0 => Some((t.abs(*o), *l as u64)),
        _ => None,
    }
}

/// Strips that follow each other on disk, as one range.
fn contiguous_strips(src: &mut dyn ByteSource, t: &Tiff, ifd: &Ifd) -> Option<(u64, u64)> {
    let offs = t.uints(src, ifd.get(0x0111)?).ok()?;
    let lens = t.uints(src, ifd.get(0x0117)?).ok()?;
    if offs.is_empty() || offs.len() != lens.len() {
        return None;
    }
    let mut end = offs[0] as u64;
    for (o, l) in offs.iter().zip(&lens) {
        if *o as u64 != end {
            return None;
        }
        end += *l as u64;
    }
    Some((t.abs(offs[0]), end - offs[0] as u64))
}
