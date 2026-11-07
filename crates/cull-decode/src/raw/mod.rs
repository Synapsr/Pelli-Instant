//! Embedded previews of RAW files.
//!
//! Each container parser lists *candidates* (offset + length of something
//! that should be a JPEG, or an uncompressed RGB thumbnail) from the file
//! structure alone. Candidates are then validated by reading their JPEG
//! header (SOI + frame size), which costs one small read each. Only when the
//! structure yields nothing usable do we fall back to scanning the whole file
//! for JPEG streams.

mod cr3;
mod fallback;
mod raf;
mod tiff_raw;

#[cfg(test)]
pub(crate) mod test_uuids {
    pub(crate) use super::cr3::{UUID_CANON as CANON, UUID_PREVIEW as PREVIEW, UUID_XMP as XMP};
}

use crate::exif::MetaBuilder;
use crate::fault::{FResult, Fault};
use crate::jpeg::{self, JpegHeader};
use crate::source::ByteSource;

/// Container family, from magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Container {
    /// TIFF and TIFF-based RAWs (CR2, NEF, ARW, DNG, ORF, PEF, SRW, RW2, …).
    Tiff,
    /// Canon CR3 (ISOBMFF, brand `crx `).
    Cr3,
    /// HEIF/HEIC/AVIF (ISOBMFF image brands).
    Heif,
    /// Fujifilm RAF.
    Raf,
    Jpeg,
    Png,
    Unknown,
}

pub(crate) fn sniff(head: &[u8]) -> Container {
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Container::Jpeg;
    }
    if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Container::Png;
    }
    if head.starts_with(b"FUJIFILMCCD-RAW") {
        return Container::Raf;
    }
    if crate::tiff::sniff(head).is_some() {
        return Container::Tiff;
    }
    if head.get(4..8) == Some(b"ftyp") {
        let brand = head.get(8..12).unwrap_or_default();
        if brand == b"crx " {
            return Container::Cr3;
        }
        return Container::Heif;
    }
    Container::Unknown
}

/// Where a preview candidate came from (diagnostics, tie-breaking).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    Structure,
    MakerNote,
    Scan,
}

/// Something the structure says should be a preview.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Candidate {
    pub offset: u64,
    pub len: u64,
    pub alt_offset: Option<u64>,
    /// `Some((w, h))` for an uncompressed 8-bit RGB strip.
    pub rgb: Option<(u32, u32)>,
    pub origin: Origin,
}

impl Candidate {
    pub(crate) fn jpeg(offset: u64, len: u64) -> Candidate {
        Candidate { offset, len, alt_offset: None, rgb: None, origin: Origin::Structure }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreviewFormat {
    Jpeg,
    /// Uncompressed interleaved RGB8 (some DNG/NEF thumbnails).
    Rgb8,
}

/// A validated preview.
#[derive(Debug, Clone)]
pub(crate) struct Preview {
    pub offset: u64,
    pub len: u64,
    pub width: u32,
    pub height: u32,
    pub format: PreviewFormat,
    pub origin: Origin,
    pub header: Option<JpegHeader>,
}

impl Preview {
    pub(crate) fn pixels(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

/// Everything learnt about a RAW file from its structure.
#[derive(Debug, Default)]
pub(crate) struct RawProbe {
    pub previews: Vec<Preview>,
    pub meta: MetaBuilder,
    /// Why no preview could be used, when we know (e.g. HEVC-only CR3).
    pub unsupported: Option<String>,
    /// The container keeps the camera's full EXIF in its embedded JPEG
    /// (RAF, RW2): always merge it.
    pub exif_in_preview: bool,
}

impl RawProbe {
    /// Largest preview by pixel count (JPEG preferred on ties).
    pub(crate) fn largest(&self) -> Option<&Preview> {
        self.previews.iter().max_by_key(|p| (p.pixels(), p.format == PreviewFormat::Jpeg, std::cmp::Reverse(p.len)))
    }

    /// Cheapest preview to decode whose long edge reaches `min_long_edge`
    /// after DCT scaling; the largest one when none does.
    ///
    /// Decoding cost = entropy decoding (proportional to the compressed
    /// size) + per-block work (proportional to the source pixels) + IDCT and
    /// colour conversion (proportional to the output pixels), so a 1620 px
    /// camera preview beats a 24 MP JPEG decoded at 1/4.
    pub(crate) fn cheapest_for(&self, min_long_edge: u32) -> Option<&Preview> {
        let long = |p: &Preview| p.width.max(p.height);
        let cost = |p: &Preview| -> u64 {
            match p.format {
                PreviewFormat::Rgb8 => p.len,
                PreviewFormat::Jpeg => {
                    let d = jpeg::scale_denominator(long(p), min_long_edge);
                    let (w, h) = jpeg::scaled_size(p.width, p.height, d);
                    // In "compressed byte" units, fitted on the sample RAWs
                    // with examples/bench_decode.rs (~8 ms/MB, ~0.8 ms per
                    // source MP, ~6 ms per output MP on one 2.1 GHz core).
                    p.len + p.pixels() / 10 + (w as u64 * h as u64) * 4 / 5
                }
            }
        };
        let sufficient = self.previews.iter().filter(|p| long(p) >= min_long_edge);
        sufficient.min_by_key(|p| cost(p)).or_else(|| self.largest())
    }
}

/// Inspects a RAW (or any file) and lists its validated previews.
///
/// `allow_scan` permits reading the whole file when the structure gives
/// nothing usable (unknown formats).
pub(crate) fn probe(src: &mut dyn ByteSource, allow_scan: bool) -> FResult<RawProbe> {
    let head = src.read_upto(0, 64)?;
    let mut probe = RawProbe::default();
    let mut cands = Vec::new();
    let container = sniff(&head);
    match container {
        Container::Tiff => tiff_raw::probe(src, &mut probe, &mut cands)?,
        Container::Cr3 => cr3::probe(src, &mut probe, &mut cands)?,
        Container::Raf => raf::probe(src, &mut probe, &mut cands)?,
        Container::Jpeg => cands.push(Candidate::jpeg(0, src.len())),
        Container::Heif => {
            return Err(Fault::Unsupported("HEIF/HEVC images are not supported".into()));
        }
        Container::Png | Container::Unknown => {}
    }
    for n in &probe.meta.note_previews {
        cands.push(Candidate {
            offset: n.offset,
            len: n.len,
            alt_offset: n.alt_offset,
            rgb: None,
            origin: Origin::MakerNote,
        });
    }
    probe.previews = validate(src, &cands);

    // Structure gave nothing but thumbnails: the preview is somewhere we
    // don't know how to find (unknown RAW format or vendor quirk).
    let best_long = probe.largest().map_or(0, |p| p.width.max(p.height));
    if allow_scan && best_long < 400 {
        log::debug!("no structured preview >= 400 px, scanning the file");
        for p in fallback::scan(src)? {
            if !probe.previews.iter().any(|q| q.offset == p.offset) {
                probe.previews.push(p);
            }
        }
    }

    if let Some(p) = probe.largest() {
        probe.meta.preview_dims = Some((p.width, p.height));
    }
    // RAF, RW2 and unknown formats keep (part of) their EXIF in the embedded
    // JPEG: complete what the structure did not provide.
    let m = &probe.meta;
    if probe.exif_in_preview || !m.has_core() || m.iso.is_none() || m.exposure.is_none() {
        let exif = probe
            .previews
            .iter()
            .filter_map(|p| p.header.as_ref().and_then(|h| h.exif).map(|e| (p.pixels(), e)))
            .max_by_key(|(px, _)| *px)
            .map(|(_, e)| e);
        if let Some((off, len)) = exif {
            if let Ok(bytes) = src.read_at(off, len.min(crate::tiff::MAX_VALUE)) {
                probe.meta.read_tiff_bytes(&bytes);
            }
        }
    }
    Ok(probe)
}

/// Validates candidates: SOI, a decodable frame header, inside the file.
fn validate(src: &mut dyn ByteSource, cands: &[Candidate]) -> Vec<Preview> {
    let file_len = src.len();
    let mut out: Vec<Preview> = Vec::new();
    for c in cands {
        if out.iter().any(|p| p.offset == c.offset) {
            continue;
        }
        if let Some((w, h)) = c.rgb {
            let need = w as u64 * h as u64 * 3;
            if w > 0 && h > 0 && c.len >= need && crate::source::check_range(c.offset, need, file_len).is_ok() {
                out.push(Preview {
                    offset: c.offset,
                    len: need,
                    width: w,
                    height: h,
                    format: PreviewFormat::Rgb8,
                    origin: c.origin,
                    header: None,
                });
            }
            continue;
        }
        for off in std::iter::once(c.offset).chain(c.alt_offset) {
            if c.len < 64 || crate::source::check_range(off, c.len, file_len).is_err() {
                continue;
            }
            match jpeg::read_header(src, off, c.len) {
                Ok(h) => {
                    out.push(Preview {
                        offset: off,
                        len: c.len,
                        width: h.width,
                        height: h.height,
                        format: PreviewFormat::Jpeg,
                        origin: c.origin,
                        header: Some(h),
                    });
                    break;
                }
                Err(e) => log::trace!("candidate at {off} rejected: {e:?}"),
            }
        }
    }
    out
}
