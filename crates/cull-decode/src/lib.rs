//! Reading photos fast.
//!
//! Culling a 3 000-photo wedding must feel instant, so this crate never
//! demosaics a RAW: every modern camera embeds a full-size (or at least
//! 1 600 px) JPEG rendering in its RAW files, which is what the photographer
//! sees on the camera screen. We locate it, decode it with DCT scaling
//! (1/2, 1/4, 1/8 IDCT: decoding a 24 MP JPEG at 1/4 is ~10× faster than
//! full size), apply the EXIF orientation and hand an upright `RgbImage` to
//! the analysis.
//!
//! CONTRACT — these signatures are used by `cull-engine` and must stay
//! stable. Implementation details live in private modules.

use cull_types::{ExifInfo, FileKind, PhotoAsset, SourceFile};
use image::RgbImage;
use std::path::Path;

mod exif;
mod exif_write;
mod fastjpeg;
mod fault;
mod jpeg;
mod makernote;
mod pixels;
mod raw;
mod scan;
mod source;
mod tiff;

use crate::exif::MetaBuilder;
use fault::{guard, FResult, Fault};
use raw::{Container, PreviewFormat, RawProbe};
use source::{ByteSource, FileSource};

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("i/o error on {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("unsupported file format: {0}")]
    Unsupported(String),
    #[error("no embedded preview found in {0}")]
    NoPreview(String),
    #[error("corrupt image data in {path}: {reason}")]
    Corrupt { path: String, reason: String },
}

pub type Result<T> = std::result::Result<T, DecodeError>;

/// File the export leaves in the folders it fills (rejects folders, copy of
/// the selection): the scan never walks into a sub-folder holding it, so
/// exported photos are not imported again.
pub const EXPORT_MARKER: &str = ".pelli-instant-export";

#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Walk sub-folders (never those holding [`EXPORT_MARKER`]).
    pub recursive: bool,
    /// Pair `IMG_1.CR3` with `IMG_1.JPG` into one asset (RAW is primary).
    pub pair_raw_jpeg: bool,
    /// Skip folders whose name starts with these (case-insensitive).
    pub skip_dir_prefixes: Vec<String>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            recursive: true,
            pair_raw_jpeg: true,
            skip_dir_prefixes: vec!["_pelli".into(), ".".into(), "@eadir".into()],
        }
    }
}

/// List the photos of a folder.
///
/// - Ignores hidden files, macOS `._*` resource forks, `Thumbs.db`, sidecars
///   (`.xmp` are attached to their photo in `PhotoAsset::xmp_sidecar`).
/// - Pairs RAW + JPEG with the same stem in the same directory.
/// - Returns assets sorted by `rel_path` (natural sort: IMG_2 < IMG_10).
/// - `PhotoAsset::id` = hex SHA-1 of `rel_path` (ASCII-lowercased).
/// - Never follows symlinks out of `root`.
///
/// Details: hidden folders (dot-prefixed) and system folders (`$RECYCLE.BIN`,
/// `System Volume Information`, `@eaDir`, …) are never walked; symlinked
/// folders are never followed and symlinked files only when they resolve
/// inside `root`. A RAW absorbs every other image with its stem (JPEG, HEIF,
/// TIFF) as companions; the camera RAW wins over a DNG conversion. Both
/// `IMG_1.xmp` and `IMG_1.CR3.xmp` sidecars are recognised.
pub fn scan_folder(root: &Path, opts: &ScanOptions) -> Result<Vec<PhotoAsset>> {
    scan::scan_folder(root, opts)
}

/// Read EXIF metadata from any supported file (JPEG, TIFF, TIFF-based RAWs,
/// CR3, RAF, PNG eXIf, HEIF when possible). Missing tags are `None`; a file
/// without EXIF returns `ExifInfo { orientation: 1, ..Default }`, not an error.
///
/// Only the file's metadata structures are read (a few KiB, even for a 60 MB
/// RAW). Errors are I/O failures and recognised containers whose root
/// structure is unreadable.
pub fn read_exif(path: &Path) -> Result<ExifInfo> {
    read_exif_inner(path).map_err(|f| f.into_error(path))
}

/// Which pixels a preview came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewSource {
    /// JPEG embedded in a RAW file.
    EmbeddedJpeg,
    /// The file itself (JPEG/PNG/TIFF) decoded.
    File,
}

#[derive(Debug, Clone)]
pub struct DecodedPreview {
    /// Upright (EXIF orientation applied) RGB8 image whose long edge is >=
    /// `min_long_edge` when the source allows it, and as small as possible
    /// above that (DCT scaling picks the smallest sufficient factor; the
    /// result is NOT resized further — callers resize).
    pub image: RgbImage,
    pub source: PreviewSource,
    /// Upright dimensions of the source pixels the preview was decoded from
    /// (embedded JPEG size, or file size).
    pub source_width: u32,
    pub source_height: u32,
    /// EXIF read while decoding (saves a second pass over the file).
    pub exif: ExifInfo,
}

/// Decode a photo for analysis/thumbnails.
///
/// For RAW: largest embedded JPEG (TIFF IFD chains incl. SubIFDs and
/// MakerNote previews for NEF/ARW/CR2/DNG/ORF/PEF, CR3 ISOBMFF `PRVW`/full
/// JPEG track, RAF header offsets, RW2 `JpgFromRaw`; generic fallback:
/// largest valid SOI…EOI stream). Must take < 30 ms for a 24 MP RAW at
/// `min_long_edge = 1440` on a laptop core.
///
/// Among the embedded JPEGs whose long edge reaches `min_long_edge`, the one
/// that is cheapest to decode is used (e.g. the 1620 px preview of a NEF
/// rather than its 6048 px JpgFromRaw decoded at 1/4); when none is large
/// enough, the largest one. PNG/TIFF files are reduced by powers of two the
/// same way DCT scaling reduces JPEGs. An uncompressed RGB thumbnail is used
/// only when a RAW has no JPEG at all (reported as `EmbeddedJpeg`).
pub fn load_preview(file: &SourceFile, min_long_edge: u32) -> Result<DecodedPreview> {
    load_preview_inner(file, min_long_edge).map_err(|f| f.into_error(&file.path))
}

/// Bytes of the best full-quality JPEG for this file, without re-encoding:
/// the file itself for a JPEG, the largest embedded JPEG for a RAW, or a
/// quality-92 re-encode for PNG/TIFF. Used to send photos to a Pelli gallery
/// and for the full-screen loupe. The EXIF orientation is NOT applied (the
/// returned JPEG keeps or gains an orientation tag so viewers rotate it).
///
/// An embedded JPEG without EXIF (CR2/CR3/NEF full-size previews) gains an
/// APP1 block with the RAW's orientation and shooting data (camera, lens,
/// exposure, capture time, GPS) so galleries can display them; an embedded
/// orientation that disagrees with the RAW's is corrected in place.
pub fn best_jpeg_bytes(file: &SourceFile) -> Result<Vec<u8>> {
    best_jpeg_bytes_inner(file).map_err(|f| f.into_error(&file.path))
}

/// Decode JPEG bytes (e.g. from [`best_jpeg_bytes`]) with DCT scaling: the
/// smallest 1/1, 1/2, 1/4 or 1/8 reduction whose long edge is still at
/// least `min_long_edge`. The EXIF orientation is NOT applied.
pub fn decode_jpeg(data: &[u8], min_long_edge: u32) -> Result<RgbImage> {
    jpeg::decode_scaled(data, min_long_edge).map_err(|f| f.into_error(Path::new("<memory>")))
}

/// Resize with a good quality filter (Lanczos3/CatmullRom, SIMD if
/// available) so that the long edge is `long_edge`. Never upscales.
pub fn resize_long_edge(img: &RgbImage, long_edge: u32) -> RgbImage {
    let (w, h) = img.dimensions();
    if w.max(h) <= long_edge || long_edge == 0 {
        return img.clone();
    }
    let (nw, nh) = pixels::fit_long_edge(w, h, long_edge);
    pixels::resize_exact(img, nw, nh, fast_image_resize::FilterType::CatmullRom)
}

/// Encode as baseline JPEG (thumbnails and cache files).
pub fn encode_jpeg(img: &RgbImage, quality: u8) -> Vec<u8> {
    pixels::encode_jpeg(img, quality)
}

/// One embedded preview found in a RAW (diagnostics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedPreview {
    /// Byte range of the preview in the file.
    pub offset: u64,
    pub len: u64,
    /// Stored (not upright) pixel size.
    pub width: u32,
    pub height: u32,
    /// `true` for a JPEG, `false` for an uncompressed RGB thumbnail.
    pub is_jpeg: bool,
}

/// What [`probe_previews`] found.
#[derive(Debug, Clone)]
pub struct PreviewProbe {
    pub previews: Vec<EmbeddedPreview>,
    /// The preview [`load_preview`] would decode for this `min_long_edge`.
    pub chosen: Option<EmbeddedPreview>,
    /// Bytes read from the file to find and validate them.
    pub bytes_read: u64,
}

/// Lists the embedded previews of a RAW without decoding them (diagnostics,
/// benchmarks, I/O tests).
pub fn probe_previews(path: &Path, min_long_edge: u32) -> Result<PreviewProbe> {
    let run = || -> FResult<PreviewProbe> {
        let mut src = FileSource::open(path)?;
        let probe = probe_raw(&mut src)?;
        let conv = |p: &raw::Preview| EmbeddedPreview {
            offset: p.offset,
            len: p.len,
            width: p.width,
            height: p.height,
            is_jpeg: p.format == PreviewFormat::Jpeg,
        };
        Ok(PreviewProbe {
            previews: probe.previews.iter().map(conv).collect(),
            chosen: probe.cheapest_for(min_long_edge).map(conv),
            bytes_read: src.bytes_read(),
        })
    };
    run().map_err(|f| f.into_error(path))
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

/// How a file is handled, from its magic bytes and declared kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Handling {
    /// A JPEG file, decoded itself.
    Jpeg,
    /// PNG or plain TIFF decoded by `image`.
    Image,
    Heif,
    /// Embedded previews (RAW, or unknown containers).
    Raw,
}

fn handling(head: &[u8], kind: Option<FileKind>) -> Handling {
    match raw::sniff(head) {
        Container::Jpeg => Handling::Jpeg,
        Container::Png => Handling::Image,
        Container::Heif => Handling::Heif,
        // A `.tif` is an image; a TIFF-based RAW (or a DNG renamed) is not.
        Container::Tiff if kind == Some(FileKind::Tiff) => Handling::Image,
        Container::Tiff | Container::Cr3 | Container::Raf | Container::Unknown => Handling::Raw,
    }
}

fn kind_of(path: &Path) -> Option<FileKind> {
    path.extension().and_then(|e| e.to_str()).and_then(FileKind::from_extension)
}

fn read_exif_inner(path: &Path) -> FResult<ExifInfo> {
    let mut src = FileSource::open(path)?;
    let head = src.read_upto(0, 64)?;
    match handling(&head, kind_of(path)) {
        Handling::Jpeg => {
            let len = src.len();
            let h = jpeg::read_header(&mut src, 0, len)?;
            Ok(jpeg_meta(&mut src, &h).finish(false))
        }
        Handling::Image => Ok(image_meta(&mut src, &head)?.finish(false)),
        Handling::Heif => Ok(heif_meta(path).finish(false)),
        Handling::Raw => match raw::probe(&mut src, false) {
            Ok(p) => Ok(p.meta.finish(true)),
            // Unknown container without metadata we can read: not an error.
            Err(Fault::Unsupported(_)) => Ok(ExifInfo { orientation: 1, ..Default::default() }),
            Err(e) => Err(e),
        },
    }
}

/// EXIF + XMP rating + frame size of a JPEG.
fn jpeg_meta(src: &mut dyn ByteSource, h: &jpeg::JpegHeader) -> MetaBuilder {
    let mut m = MetaBuilder::default();
    if let Some((off, len)) = h.exif {
        if let Ok(b) = src.read_at(off, len) {
            m.read_tiff_bytes(&b);
        }
    }
    if let Some((off, len)) = h.xmp {
        if let Ok(b) = src.read_at(off, len) {
            m.read_xmp(&b);
        }
    }
    m.image_dims = Some((h.width, h.height));
    m
}

/// Metadata of a PNG (IHDR + eXIf) or plain TIFF (IFD0 + EXIF).
fn image_meta(src: &mut dyn ByteSource, head: &[u8]) -> FResult<MetaBuilder> {
    let mut m = MetaBuilder::default();
    if raw::sniff(head) == Container::Png {
        let mut pos = 8u64;
        for _ in 0..4096 {
            let Ok(h) = src.read_at(pos, 8) else { break };
            let len = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as u64;
            match &h[4..8] {
                b"IHDR" if len >= 8 => {
                    let d = src.read_at(pos + 8, 8)?;
                    let w = u32::from_be_bytes([d[0], d[1], d[2], d[3]]);
                    let hh = u32::from_be_bytes([d[4], d[5], d[6], d[7]]);
                    m.image_dims = Some((w, hh));
                }
                b"eXIf" if len <= tiff::MAX_VALUE as u64 => {
                    if let Ok(b) = src.read_at(pos + 8, len as usize) {
                        m.read_tiff_bytes(&b);
                    }
                }
                b"IDAT" | b"IEND" => break,
                _ => {}
            }
            pos += 12 + len;
        }
    } else {
        let t = tiff::Tiff::parse(src, 0)?;
        let ifd0 = t.read_ifd(src, t.ifd0)?;
        m.read_tiff(src, &t, &ifd0);
        let w = ifd0.get(0x0100).and_then(|e| t.uint(src, e));
        let h = ifd0.get(0x0101).and_then(|e| t.uint(src, e));
        if let (Some(w), Some(h)) = (w, h) {
            m.image_dims = Some((w, h));
        }
    }
    Ok(m)
}

/// EXIF of a HEIF file (container parsed by kamadak-exif).
fn heif_meta(path: &Path) -> MetaBuilder {
    let mut m = MetaBuilder::default();
    let exif = std::fs::File::open(path).ok().and_then(|f| exif_container(&mut std::io::BufReader::new(f)));
    if let Some(buf) = exif {
        m.read_tiff_bytes(&buf);
    }
    m
}

fn exif_container(r: &mut std::io::BufReader<std::fs::File>) -> Option<Vec<u8>> {
    let reader = kamadak_exif::Reader::new();
    let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reader.read_from_container(r))).ok()?;
    parsed.ok().map(|e| e.buf().to_vec())
}

fn upright(w: u32, h: u32, orientation: u8) -> (u32, u32) {
    if (5..=8).contains(&orientation) {
        (h, w)
    } else {
        (w, h)
    }
}

fn load_preview_inner(file: &SourceFile, min_long_edge: u32) -> FResult<DecodedPreview> {
    let path = &file.path;
    let mut src = FileSource::open(path)?;
    let head = src.read_upto(0, 64)?;
    match handling(&head, Some(file.kind)) {
        Handling::Jpeg => {
            let len = src.len();
            let h = jpeg::read_header(&mut src, 0, len)?;
            let exif = jpeg_meta(&mut src, &h).finish(false);
            let (sw, sh) = upright(h.width, h.height, exif.orientation);
            // A camera JPEG carries a ~1616 px copy of itself (MPF): 10x
            // cheaper to decode than the 24 MP image when it is large enough.
            if let Some(img) = mpf_preview(&mut src, &h, min_long_edge) {
                return Ok(DecodedPreview {
                    image: pixels::orient(img, exif.orientation),
                    source: PreviewSource::File,
                    source_width: sw,
                    source_height: sh,
                    exif,
                });
            }
            // The whole file is needed to decode it: one read.
            let data = src.read_at(0, usize::try_from(len).unwrap_or(usize::MAX))?;
            let img = jpeg::decode_scaled(&data, min_long_edge)?;
            Ok(DecodedPreview {
                image: pixels::orient(img, exif.orientation),
                source: PreviewSource::File,
                source_width: sw,
                source_height: sh,
                exif,
            })
        }
        Handling::Image => match decode_image_file(path, &mut src, &head) {
            Ok((img, exif)) => {
                let (sw, sh) = upright(img.width(), img.height(), exif.orientation);
                let img = pixels::reduce_pow2(img, min_long_edge);
                Ok(DecodedPreview {
                    image: pixels::orient(img, exif.orientation),
                    source: PreviewSource::File,
                    source_width: sw,
                    source_height: sh,
                    exif,
                })
            }
            // A TIFF `image` cannot decode may still be a RAW in disguise.
            Err(e) if raw::sniff(&head) == Container::Tiff => {
                raw_preview(&mut src, min_long_edge).map_err(|raw_err| match raw_err {
                    Fault::NoPreview => e,
                    other => other,
                })
            }
            Err(e) => Err(e),
        },
        Handling::Heif => Err(heif_unsupported()),
        Handling::Raw => raw_preview(&mut src, min_long_edge),
    }
}

/// The camera's large MPF preview of a JPEG, decoded, when its long edge
/// reaches `min_long_edge` and it shows the same frame (aspect ratio within
/// 1%: some cameras letterbox their previews).
fn mpf_preview(src: &mut FileSource, h: &jpeg::JpegHeader, min_long_edge: u32) -> Option<RgbImage> {
    let (off, len) = h.mpf?;
    let index = src.read_at(off, len.min(64 * 1024)).ok()?;
    let ratio = h.width as f32 / h.height.max(1) as f32;
    for (at, size) in jpeg::mpf_previews(&index, off) {
        if size > src.len() / 2 || at.saturating_add(size) > src.len() {
            continue;
        }
        let Ok(bytes) = src.read_at(at, size as usize) else { continue };
        let Ok(ph) = jpeg::read_header_mem(&bytes) else { continue };
        let same_frame = ((ph.width as f32 / ph.height.max(1) as f32) / ratio - 1.0).abs() < 0.01;
        if ph.width.max(ph.height) >= min_long_edge && ph.width.max(ph.height) < h.width.max(h.height) && same_frame {
            match jpeg::decode_scaled(&bytes, min_long_edge) {
                Ok(img) => return Some(img),
                Err(e) => log::debug!("MPF preview at {at} undecodable: {e:?}"),
            }
        }
    }
    None
}

fn heif_unsupported() -> Fault {
    Fault::Unsupported("HEIF/HEIC decoding is not available (no permissive HEVC decoder)".into())
}

/// Full-resolution pixels (orientation not applied) and metadata of a PNG/TIFF.
fn decode_image_file(path: &Path, src: &mut FileSource, head: &[u8]) -> FResult<(RgbImage, ExifInfo)> {
    let exif = image_meta(src, head)?.finish(false);
    let img = guard("image", || {
        let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(1 << 30);
        reader.limits(limits);
        let img = reader.decode().map_err(image_err)?;
        Ok(img.into_rgb8())
    })?;
    Ok((img, exif))
}

fn image_err(e: image::ImageError) -> Fault {
    match e {
        image::ImageError::IoError(e) => Fault::Io(e),
        image::ImageError::Unsupported(u) => Fault::Unsupported(u.to_string()),
        other => Fault::Corrupt(other.to_string()),
    }
}

/// Probes a RAW, failing with the most helpful error when it has no preview.
fn probe_raw(src: &mut FileSource) -> FResult<RawProbe> {
    let probe = raw::probe(src, true)?;
    if probe.previews.is_empty() {
        return Err(match &probe.unsupported {
            Some(why) => Fault::Unsupported(why.clone()),
            None => Fault::NoPreview,
        });
    }
    Ok(probe)
}

fn raw_preview(src: &mut FileSource, min_long_edge: u32) -> FResult<DecodedPreview> {
    let probe = probe_raw(src)?;
    let exif = probe.meta.finish(true);
    // Preferred preview first; if it turns out undecodable, try the others
    // from the largest down.
    let first = probe.cheapest_for(min_long_edge);
    let mut rest: Vec<&raw::Preview> =
        probe.previews.iter().filter(|p| first.is_none_or(|f| f.offset != p.offset)).collect();
    rest.sort_by_key(|p| std::cmp::Reverse(p.pixels()));
    let order = first.into_iter().chain(rest);

    let mut last = Fault::NoPreview;
    for p in order {
        match decode_preview(src, p, min_long_edge) {
            Ok(img) => {
                let (sw, sh) = upright(p.width, p.height, exif.orientation);
                log::debug!(
                    "preview {}x{} at {} ({} bytes, {:?}); {} bytes read",
                    p.width,
                    p.height,
                    p.offset,
                    p.len,
                    p.origin,
                    src.bytes_read()
                );
                return Ok(DecodedPreview {
                    image: pixels::orient(img, exif.orientation),
                    source: PreviewSource::EmbeddedJpeg,
                    source_width: sw,
                    source_height: sh,
                    exif,
                });
            }
            Err(e) => {
                log::debug!("preview at {} undecodable: {e:?}", p.offset);
                last = e;
            }
        }
    }
    Err(last)
}

fn decode_preview(src: &mut FileSource, p: &raw::Preview, min_long_edge: u32) -> FResult<RgbImage> {
    let bytes = src.read_at(p.offset, usize::try_from(p.len).unwrap_or(usize::MAX))?;
    match p.format {
        PreviewFormat::Jpeg => jpeg::decode_scaled(&bytes, min_long_edge),
        PreviewFormat::Rgb8 => {
            let img =
                RgbImage::from_raw(p.width, p.height, bytes).ok_or_else(|| Fault::corrupt("short RGB preview"))?;
            Ok(pixels::reduce_pow2(img, min_long_edge))
        }
    }
}

fn best_jpeg_bytes_inner(file: &SourceFile) -> FResult<Vec<u8>> {
    let path = &file.path;
    let mut src = FileSource::open(path)?;
    let head = src.read_upto(0, 64)?;
    match handling(&head, Some(file.kind)) {
        Handling::Jpeg => Ok(src.read_at(0, usize::try_from(src.len()).unwrap_or(usize::MAX))?),
        Handling::Image => match decode_image_file(path, &mut src, &head) {
            Ok((img, exif)) => reencode(&img, &exif),
            Err(e) if raw::sniff(&head) == Container::Tiff => {
                raw_best_jpeg(&mut src).map_err(|raw_err| match raw_err {
                    Fault::NoPreview => e,
                    other => other,
                })
            }
            Err(e) => Err(e),
        },
        Handling::Heif => Err(heif_unsupported()),
        Handling::Raw => raw_best_jpeg(&mut src),
    }
}

/// Quality-92 JPEG of decoded pixels, with an EXIF block carrying the
/// original orientation (pixels are not rotated).
fn reencode(img: &RgbImage, exif: &ExifInfo) -> FResult<Vec<u8>> {
    let jpeg_bytes = pixels::encode_jpeg(img, 92);
    let h = jpeg::read_header_mem(&jpeg_bytes)?;
    Ok(with_exif(&jpeg_bytes, &h, exif))
}

fn with_exif(jpeg_bytes: &[u8], h: &jpeg::JpegHeader, exif: &ExifInfo) -> Vec<u8> {
    match jpeg::app1_segment(&exif_write::from_info(exif)) {
        Some(seg) => jpeg::set_exif_segment(jpeg_bytes, h, &seg),
        None => jpeg_bytes.to_vec(),
    }
}

fn raw_best_jpeg(src: &mut FileSource) -> FResult<Vec<u8>> {
    let probe = probe_raw(src)?;
    let exif = probe.meta.finish(true);
    let raw_orientation = probe.meta.orientation_tag();
    let mut previews: Vec<&raw::Preview> = probe.previews.iter().collect();
    previews.sort_by_key(|p| (std::cmp::Reverse(p.pixels()), p.format != PreviewFormat::Jpeg));
    let mut last = Fault::NoPreview;
    for p in previews {
        let bytes = src.read_at(p.offset, usize::try_from(p.len).unwrap_or(usize::MAX))?;
        let result = match p.format {
            PreviewFormat::Rgb8 => match RgbImage::from_raw(p.width, p.height, bytes) {
                Some(img) => reencode(&img, &exif),
                None => Err(Fault::corrupt("short RGB preview")),
            },
            PreviewFormat::Jpeg => fix_embedded_orientation(bytes, &exif, raw_orientation),
        };
        match result {
            Ok(b) => return Ok(b),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Makes an embedded JPEG carry the RAW's orientation (see [`best_jpeg_bytes`]).
fn fix_embedded_orientation(mut bytes: Vec<u8>, exif: &ExifInfo, raw_orientation: Option<u8>) -> FResult<Vec<u8>> {
    // Trailing padding after EOI is common in RAW containers.
    if let Some(end) = jpeg::stream_end(&bytes, 0) {
        bytes.truncate(end);
    }
    let h = jpeg::read_header_mem(&bytes)?;
    if h.exif.is_none() {
        return Ok(with_exif(&bytes, &h, exif));
    }
    match (jpeg::orientation_slot(&bytes, &h), raw_orientation) {
        (Some(slot), Some(o)) if slot.value != o as u16 => {
            let v = slot.endian.put_u16(o as u16);
            if let Some(dst) = bytes.get_mut(slot.pos..slot.pos + 2) {
                dst.copy_from_slice(&v);
            }
            Ok(bytes)
        }
        (None, Some(o)) if o != 1 => Ok(with_exif(&bytes, &h, exif)),
        _ => Ok(bytes),
    }
}

#[cfg(test)]
mod tests;
