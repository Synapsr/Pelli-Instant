//! JPEG stream handling: header parsing (frame size, EXIF/XMP location),
//! DCT-scaled decoding, extent detection for the fallback scanner and APP1
//! editing for [`crate::best_jpeg_bytes`].

use image::RgbImage;

use crate::fault::{guard, FResult, Fault};
use crate::source::{ByteSource, MemSource};
use crate::tiff::{Endian, Tiff};

/// Upper bound for jpeg-decoder's internal buffers (a 100 MP progressive
/// JPEG needs ~600 MB of coefficients; anything above is hostile).
const MAX_DECODE_BUFFER: usize = 1 << 30;
/// Most segments we walk before giving up on a header.
const MAX_SEGMENTS: usize = 1024;

const EXIF_ID: &[u8] = b"Exif\0\0";
const XMP_ID: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const MPF_ID: &[u8] = b"MPF\0";

/// What the markers before the scan data say about a JPEG stream.
#[derive(Debug, Clone, Default)]
pub(crate) struct JpegHeader {
    pub width: u32,
    pub height: u32,
    pub components: u8,
    pub progressive: bool,
    /// Absolute offset and length of the TIFF stream of the APP1 Exif segment.
    pub exif: Option<(u64, usize)>,
    /// Absolute offset and length of the whole APP1 Exif segment (marker included).
    pub exif_segment: Option<(u64, usize)>,
    /// Absolute offset and length of the XMP packet (APP1).
    pub xmp: Option<(u64, usize)>,
    /// Absolute offset and length of the Multi-Picture Format index (APP2
    /// `MPF`, TIFF stream): where the camera stored its other images.
    pub mpf: Option<(u64, usize)>,
}

/// Parses the markers of the JPEG stream at `offset` (at most `limit` bytes
/// long) up to its frame header. Only DCT frames we can decode are accepted:
/// baseline, extended sequential and progressive Huffman (SOF0/1/2). Lossless
/// JPEG (the RAW data of CR2/DNG) and arithmetic coding are rejected.
pub(crate) fn read_header(src: &mut dyn ByteSource, offset: u64, limit: u64) -> FResult<JpegHeader> {
    let end = offset.saturating_add(limit).min(src.len());
    if offset.saturating_add(4) > end {
        return Err(Fault::corrupt("JPEG stream too short"));
    }
    if src.read_at(offset, 2)? != [0xFF, 0xD8] {
        return Err(Fault::corrupt("missing JPEG SOI marker"));
    }
    let mut h = JpegHeader::default();
    let mut pos = offset + 2;
    for _ in 0..MAX_SEGMENTS {
        if pos + 4 > end {
            break;
        }
        let b = src.read_at(pos, 4)?;
        if b[0] != 0xFF {
            return Err(Fault::corrupt(format!("bad JPEG marker at {pos}")));
        }
        let m = b[1];
        match m {
            0xFF => {
                pos += 1; // fill byte
                continue;
            }
            0x01 | 0xD0..=0xD8 => {
                pos += 2; // standalone markers
                continue;
            }
            0xD9 | 0xDA => return Err(Fault::corrupt("JPEG scan data before frame header")),
            _ => {}
        }
        let seg_len = u16::from_be_bytes([b[2], b[3]]) as u64;
        if seg_len < 2 {
            return Err(Fault::corrupt("bad JPEG segment length"));
        }
        let payload = pos + 4;
        let next = pos + 2 + seg_len;
        if next > end {
            return Err(Fault::corrupt("JPEG segment past end of stream"));
        }
        match m {
            0xC0..=0xC2 => {
                if seg_len < 8 {
                    return Err(Fault::corrupt("short JPEG frame header"));
                }
                let f = src.read_at(payload, 6)?;
                let precision = f[0];
                h.height = u16::from_be_bytes([f[1], f[2]]) as u32;
                h.width = u16::from_be_bytes([f[3], f[4]]) as u32;
                h.components = f[5];
                h.progressive = m == 0xC2;
                if precision != 8 && precision != 12 {
                    return Err(Fault::Unsupported(format!("{precision}-bit JPEG")));
                }
                if h.width == 0 || h.height == 0 {
                    return Err(Fault::corrupt("JPEG frame without dimensions"));
                }
                if !matches!(h.components, 1 | 3 | 4) {
                    return Err(Fault::corrupt(format!("JPEG with {} components", h.components)));
                }
                if seg_len < 8 + 3 * h.components as u64 {
                    return Err(Fault::corrupt("short JPEG frame header"));
                }
                return Ok(h);
            }
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                return Err(Fault::Unsupported("lossless, hierarchical or arithmetic JPEG".into()));
            }
            0xE1 => {
                let id_len = (seg_len - 2).min(XMP_ID.len() as u64) as usize;
                let id = src.read_at(payload, id_len)?;
                let body = (seg_len - 2) as usize;
                if id.starts_with(EXIF_ID) && h.exif.is_none() {
                    h.exif = Some((payload + EXIF_ID.len() as u64, body - EXIF_ID.len()));
                    h.exif_segment = Some((pos, seg_len as usize + 2));
                } else if id.starts_with(XMP_ID) && h.xmp.is_none() {
                    h.xmp = Some((payload + XMP_ID.len() as u64, body - XMP_ID.len()));
                }
            }
            // An optional index: a read error only means no MPF preview.
            0xE2 if seg_len >= 2 + 8 + MPF_ID.len() as u64
                && h.mpf.is_none()
                && src.read_at(payload, MPF_ID.len()).is_ok_and(|id| id == MPF_ID) =>
            {
                let body = (seg_len - 2) as usize;
                h.mpf = Some((payload + MPF_ID.len() as u64, body - MPF_ID.len()));
            }
            _ => {}
        }
        pos = next;
    }
    Err(Fault::corrupt("no JPEG frame header found"))
}

/// MP entry types of the camera's large previews (CIPA DC-007: "Large
/// Thumbnail", VGA and Full-HD classes).
const MPF_LARGE_THUMBNAILS: [u32; 2] = [0x01_0001, 0x01_0002];
/// MP Entry tag of the MP Index IFD.
const MPF_ENTRY_TAG: u16 = 0xB002;

/// Byte ranges (absolute offset, length) of the large previews a camera JPEG
/// lists in its Multi-Picture Format index (Sony, Nikon, Canon, Fujifilm
/// store a ~1616-1920 px copy after the main image). `mpf` is the MPF TIFF
/// stream; `base` its absolute offset in the file.
pub(crate) fn mpf_previews(mpf: &[u8], base: u64) -> Vec<(u64, u64)> {
    let endian = match mpf.get(..4) {
        Some(b"II*\0") => Endian::Little,
        Some(b"MM\0*") => Endian::Big,
        _ => return Vec::new(),
    };
    let entries = || -> Option<Vec<(u64, u64)>> {
        let ifd = endian.u32_at(mpf, 4)? as usize;
        let count = endian.u16_at(mpf, ifd)? as usize;
        let mut out = Vec::new();
        for k in 0..count.min(64) {
            let e = ifd + 2 + 12 * k;
            if endian.u16_at(mpf, e)? != MPF_ENTRY_TAG {
                continue;
            }
            let len = endian.u32_at(mpf, e + 4)? as usize;
            let at = endian.u32_at(mpf, e + 8)? as usize;
            let table = mpf.get(at..at.checked_add(len)?)?;
            // 16 bytes per image; the first one is the primary image.
            for entry in table.chunks_exact(16).skip(1) {
                let attr = endian.u32_at(entry, 0)?;
                let size = endian.u32_at(entry, 4)? as u64;
                let offset = endian.u32_at(entry, 8)? as u64;
                if MPF_LARGE_THUMBNAILS.contains(&(attr & 0x00FF_FFFF)) && size > 0 && offset > 0 {
                    out.push((base + offset, size));
                }
            }
        }
        Some(out)
    };
    entries().unwrap_or_default()
}

/// Header of an in-memory JPEG.
pub(crate) fn read_header_mem(data: &[u8]) -> FResult<JpegHeader> {
    read_header(&mut MemSource::new(data), 0, data.len() as u64)
}

/// Largest IDCT reduction (1, 2, 4 or 8) that keeps the long edge at least
/// `min_long_edge`.
pub(crate) fn scale_denominator(long_edge: u32, min_long_edge: u32) -> u32 {
    for d in [8u32, 4, 2] {
        if long_edge.div_ceil(d) >= min_long_edge {
            return d;
        }
    }
    1
}

/// Output size of a `w`×`h` JPEG decoded with 1/`denom` IDCT scaling.
pub(crate) fn scaled_size(w: u32, h: u32, denom: u32) -> (u32, u32) {
    (w.div_ceil(denom), h.div_ceil(denom))
}

/// Decodes a JPEG with DCT scaling: the output is the smallest 1/1, 1/2, 1/4
/// or 1/8 reduction whose long edge is still >= `min_long_edge`.
///
/// Reduced sizes use [`crate::fastjpeg`] (2–4× faster than jpeg-decoder on
/// camera JPEGs), full size uses zune-jpeg (SIMD); jpeg-decoder handles
/// everything they decline (progressive, CMYK, odd sampling, damaged data).
pub(crate) fn decode_scaled(data: &[u8], min_long_edge: u32) -> FResult<RgbImage> {
    if let Ok(h) = read_header_mem(data) {
        let denom = scale_denominator(h.width.max(h.height), min_long_edge);
        let fast = if denom > 1 {
            guard("JPEG", || crate::fastjpeg::decode_reduced(data, denom))
        } else {
            guard("JPEG", || decode_full_zune(data))
        };
        match fast {
            Ok(img) => return Ok(img),
            Err(e) => log::debug!("fast JPEG path declined ({e:?}), using jpeg-decoder"),
        }
    }
    decode_scaled_reference(data, min_long_edge)
}

/// Full-size decode with zune-jpeg.
fn decode_full_zune(data: &[u8]) -> FResult<RgbImage> {
    let options =
        zune_core::options::DecoderOptions::default().jpeg_set_out_colorspace(zune_core::colorspace::ColorSpace::RGB);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(std::io::Cursor::new(data), options);
    let pixels = dec.decode().map_err(|e| Fault::corrupt(format!("JPEG: {e:?}")))?;
    let info = dec.info().ok_or_else(|| Fault::corrupt("JPEG without frame"))?;
    let (w, h) = (info.width as u32, info.height as u32);
    let n = w as usize * h as usize;
    let rgb = if pixels.len() == n * 3 {
        pixels
    } else if pixels.len() == n {
        pixels.iter().flat_map(|&l| [l, l, l]).collect()
    } else {
        return Err(Fault::corrupt("unexpected JPEG output size"));
    };
    RgbImage::from_raw(w, h, rgb).ok_or_else(|| Fault::corrupt("short JPEG output"))
}

/// jpeg-decoder with its own DCT scaling: the reference implementation.
pub(crate) fn decode_scaled_reference(data: &[u8], min_long_edge: u32) -> FResult<RgbImage> {
    guard("JPEG", || {
        let mut dec = jpeg_decoder::Decoder::new(data);
        dec.set_max_decoding_buffer_size(MAX_DECODE_BUFFER);
        dec.read_info().map_err(jpeg_err)?;
        let info = dec.info().ok_or_else(|| Fault::corrupt("JPEG without frame"))?;
        let (w, h) = (info.width as u32, info.height as u32);
        let denom = scale_denominator(w.max(h), min_long_edge);
        if denom > 1 {
            let (rw, rh) = scaled_size(w, h, denom);
            dec.scale(rw as u16, rh as u16).map_err(jpeg_err)?;
        }
        let pixels = dec.decode().map_err(jpeg_err)?;
        let info = dec.info().ok_or_else(|| Fault::corrupt("JPEG without frame"))?;
        to_rgb(info, pixels)
    })
}

fn jpeg_err(e: jpeg_decoder::Error) -> Fault {
    match e {
        jpeg_decoder::Error::Unsupported(f) => Fault::Unsupported(format!("JPEG feature {f:?}")),
        jpeg_decoder::Error::Io(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            Fault::corrupt("truncated JPEG data")
        }
        other => Fault::corrupt(format!("JPEG: {other}")),
    }
}

fn to_rgb(info: jpeg_decoder::ImageInfo, pixels: Vec<u8>) -> FResult<RgbImage> {
    use jpeg_decoder::PixelFormat;
    let (w, h) = (info.width as u32, info.height as u32);
    let n = w as usize * h as usize;
    let rgb = match info.pixel_format {
        PixelFormat::RGB24 => pixels,
        PixelFormat::L8 => pixels.iter().take(n).flat_map(|&l| [l, l, l]).collect(),
        PixelFormat::L16 => pixels.chunks_exact(2).take(n).flat_map(|c| [c[0], c[0], c[0]]).collect(),
        PixelFormat::CMYK32 => pixels
            .chunks_exact(4)
            .take(n)
            .flat_map(|p| {
                // Adobe CMYK as returned by jpeg-decoder (same as `image`).
                let k = 255 - p[3] as u16;
                [p[0], p[1], p[2]].map(|c| (((255 - c as u16) * k) / 255) as u8)
            })
            .collect(),
    };
    RgbImage::from_raw(w, h, rgb).ok_or_else(|| Fault::corrupt("JPEG decoder returned a short buffer"))
}

/// End (exclusive, relative to `data`) of the JPEG stream starting at
/// `start`, found by walking its segments and entropy-coded data up to EOI.
/// Used by the fallback scanner on formats whose structure we don't parse.
pub(crate) fn stream_end(data: &[u8], start: usize) -> Option<usize> {
    if data.get(start..start + 2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut pos = start + 2;
    let mut seen_frame = false;
    for _ in 0..MAX_SEGMENTS {
        // Skip fill bytes.
        while data.get(pos) == Some(&0xFF) && data.get(pos + 1) == Some(&0xFF) {
            pos += 1;
        }
        if *data.get(pos)? != 0xFF {
            return None;
        }
        let m = *data.get(pos + 1)?;
        match m {
            0xD9 => return seen_frame.then_some(pos + 2),
            0x01 | 0xD0..=0xD7 => {
                pos += 2;
                continue;
            }
            0xD8 | 0x00 => return None,
            _ => {}
        }
        let len = u16::from_be_bytes([*data.get(pos + 2)?, *data.get(pos + 3)?]) as usize;
        if len < 2 {
            return None;
        }
        if matches!(m, 0xC0..=0xC2) {
            seen_frame = true;
        }
        pos += 2 + len;
        if m == 0xDA {
            if !seen_frame {
                return None;
            }
            // Entropy-coded data: runs until a marker that is neither a
            // stuffed 0xFF00 nor a restart marker.
            let mut i = pos;
            loop {
                let rel = memchr_ff(data.get(i..)?)?;
                i += rel;
                let next = *data.get(i + 1)?;
                if next == 0x00 || (0xD0..=0xD7).contains(&next) || next == 0xFF {
                    i += if next == 0xFF { 1 } else { 2 };
                    continue;
                }
                break;
            }
            pos = i;
        }
    }
    None
}

fn memchr_ff(hay: &[u8]) -> Option<usize> {
    hay.iter().position(|&b| b == 0xFF)
}

// ---------------------------------------------------------------------------
// APP1 editing (orientation of exported JPEGs)
// ---------------------------------------------------------------------------

/// Orientation tag found in a JPEG's own EXIF.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OrientationSlot {
    pub value: u16,
    /// Absolute byte position of the 2-byte value inside the JPEG.
    pub pos: usize,
    pub endian: Endian,
}

/// The orientation tag of a JPEG's EXIF IFD0, if there is one.
pub(crate) fn orientation_slot(jpeg: &[u8], header: &JpegHeader) -> Option<OrientationSlot> {
    let (off, len) = header.exif?;
    let off = usize::try_from(off).ok()?;
    let tiff_bytes = jpeg.get(off..off.checked_add(len)?)?;
    let mut src = MemSource::new(tiff_bytes);
    let tiff = Tiff::parse(&mut src, 0).ok()?;
    let ifd0 = tiff.read_ifd(&mut src, tiff.ifd0).ok()?;
    let e = ifd0.get(0x0112)?;
    if e.typ != crate::tiff::ty::SHORT || e.count != 1 {
        return None;
    }
    let value = tiff.uint(&mut src, e)? as u16;
    let pos = off + usize::try_from(tiff.value_pos(e)).ok()?;
    Some(OrientationSlot { value, pos, endian: tiff.endian })
}

/// Builds an APP1 Exif segment (marker included) around a TIFF stream.
pub(crate) fn app1_segment(tiff: &[u8]) -> Option<Vec<u8>> {
    let len = 2 + EXIF_ID.len() + tiff.len();
    let len = u16::try_from(len).ok()?;
    let mut seg = Vec::with_capacity(len as usize + 2);
    seg.extend_from_slice(&[0xFF, 0xE1]);
    seg.extend_from_slice(&len.to_be_bytes());
    seg.extend_from_slice(EXIF_ID);
    seg.extend_from_slice(tiff);
    Some(seg)
}

/// Inserts (or replaces) the APP1 Exif segment of a JPEG.
pub(crate) fn set_exif_segment(jpeg: &[u8], header: &JpegHeader, segment: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(jpeg.len() + segment.len());
    if let Some((start, len)) = header.exif_segment {
        let (start, len) = (start as usize, len);
        if start.checked_add(len).is_some_and(|end| end <= jpeg.len()) {
            out.extend_from_slice(&jpeg[..start]);
            out.extend_from_slice(segment);
            out.extend_from_slice(&jpeg[start + len..]);
            return out;
        }
    }
    // After SOI, and after a leading JFIF APP0 (which must stay first).
    let mut at = 2.min(jpeg.len());
    if jpeg.get(2..4) == Some(&[0xFF, 0xE0]) {
        if let Some(l) = jpeg.get(4..6).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize) {
            if 4 + l <= jpeg.len() {
                at = 4 + l;
            }
        }
    }
    out.extend_from_slice(&jpeg[..at]);
    out.extend_from_slice(segment);
    out.extend_from_slice(&jpeg[at..]);
    out
}

#[cfg(test)]
pub(crate) mod test_util {
    use image::{Rgb, RgbImage};

    /// A colourful test pattern that survives JPEG compression.
    pub(crate) fn pattern(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            Rgb([
                (x * 255 / w.max(1)) as u8,
                (y * 255 / h.max(1)) as u8,
                if (x / 8 + y / 8) % 2 == 0 { 40 } else { 220 },
            ])
        })
    }

    pub(crate) fn jpeg(w: u32, h: u32) -> Vec<u8> {
        crate::encode_jpeg(&pattern(w, h), 90)
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::*;
    use super::*;

    #[test]
    fn header_and_scaling() {
        let data = jpeg(1000, 600);
        let h = read_header_mem(&data).unwrap();
        assert_eq!((h.width, h.height, h.components), (1000, 600, 3));
        assert_eq!(scale_denominator(6000, 1440), 4);
        assert_eq!(scale_denominator(5760, 1440), 4);
        assert_eq!(scale_denominator(5761, 1440), 4);
        assert_eq!(scale_denominator(5757, 1440), 4);
        assert_eq!(scale_denominator(5756, 1440), 2);
        assert_eq!(scale_denominator(1000, 1440), 1);
        assert_eq!(scale_denominator(8000, 0), 8);

        let img = decode_scaled(&data, 250).unwrap();
        assert_eq!(img.dimensions(), (250, 150));
        let img = decode_scaled(&data, 260).unwrap();
        assert_eq!(img.dimensions(), (500, 300));
        let img = decode_scaled(&data, 5000).unwrap();
        assert_eq!(img.dimensions(), (1000, 600));
    }

    #[test]
    fn extent() {
        let data = jpeg(64, 48);
        let mut buf = vec![0u8; 100];
        buf.extend_from_slice(&data);
        buf.extend_from_slice(&[0xFF, 0xD8, 0, 0, 1, 2, 3]);
        assert_eq!(stream_end(&buf, 100), Some(100 + data.len()));
        assert_eq!(stream_end(&buf, 100 + data.len()), None);
        assert_eq!(stream_end(&buf, 0), None);
    }

    #[test]
    fn rejects_garbage() {
        assert!(read_header_mem(b"").is_err());
        assert!(read_header_mem(b"\xFF\xD8\xFF\xC0\x00\x02").is_err());
        assert!(read_header_mem(b"\xFF\xD8\xFF\xE1\xFF\xFFabc").is_err());
        assert!(decode_scaled(b"\xFF\xD8\xFF\xD9", 100).is_err());
        let mut data = jpeg(200, 100);
        data.truncate(data.len() / 2);
        // Truncated scan data: either an error or a (partially grey) image, never a panic.
        let _ = decode_scaled(&data, 50);
        for cut in [3usize, 10, 30, 100, 200] {
            let d = jpeg(64, 64);
            let _ = read_header_mem(&d[..cut.min(d.len())]);
            let _ = decode_scaled(&d[..cut.min(d.len())], 8);
        }
    }

    #[test]
    fn exif_insert_and_patch() {
        let data = jpeg(32, 16);
        let h = read_header_mem(&data).unwrap();
        assert!(h.exif.is_none());
        let tiff = crate::exif_write::orientation_only(6);
        let seg = app1_segment(&tiff).unwrap();
        let out = set_exif_segment(&data, &h, &seg);
        let h2 = read_header_mem(&out).unwrap();
        let slot = orientation_slot(&out, &h2).unwrap();
        assert_eq!(slot.value, 6);
        // Still decodes.
        assert_eq!(decode_scaled(&out, 0).unwrap().dimensions(), (4, 2));
        // Replacing keeps a single EXIF segment.
        let seg3 = app1_segment(&crate::exif_write::orientation_only(3)).unwrap();
        let out3 = set_exif_segment(&out, &h2, &seg3);
        assert_eq!(out3.len(), out.len());
        let h3 = read_header_mem(&out3).unwrap();
        assert_eq!(orientation_slot(&out3, &h3).unwrap().value, 3);
    }
}
