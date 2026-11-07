//! Last resort for containers we don't parse (CRW, MRW, X3F, …) or files
//! whose structure lists no usable preview: read the file and list every
//! complete JPEG stream (SOI … EOI with a decodable frame header).

use super::{Origin, Preview, PreviewFormat};
use crate::fault::FResult;
use crate::jpeg;
use crate::source::ByteSource;

/// Files larger than this are not scanned (no camera RAW is that big).
const MAX_SCAN: u64 = 512 * 1024 * 1024;
/// Streams smaller than this are thumbnails or false positives.
const MIN_STREAM: usize = 2048;

pub(super) fn scan(src: &mut dyn ByteSource) -> FResult<Vec<Preview>> {
    let len = src.len();
    if len > MAX_SCAN {
        return Ok(Vec::new());
    }
    let data = src.read_at(0, len as usize)?;
    Ok(scan_bytes(&data))
}

pub(super) fn scan_bytes(data: &[u8]) -> Vec<Preview> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 3 <= data.len() {
        let Some(rel) = data[i..].iter().position(|&b| b == 0xFF) else { break };
        let s = i + rel;
        if data.get(s + 1) != Some(&0xD8) || data.get(s + 2) != Some(&0xFF) {
            i = s + 1;
            continue;
        }
        if let Some(end) = jpeg::stream_end(data, s) {
            if end - s >= MIN_STREAM {
                if let Ok(mut h) = jpeg::read_header_mem(&data[s..end]) {
                    let base = s as u64;
                    h.exif = h.exif.map(|(o, l)| (o + base, l));
                    h.exif_segment = h.exif_segment.map(|(o, l)| (o + base, l));
                    h.xmp = h.xmp.map(|(o, l)| (o + base, l));
                    out.push(Preview {
                        offset: base,
                        len: (end - s) as u64,
                        width: h.width,
                        height: h.height,
                        format: PreviewFormat::Jpeg,
                        origin: Origin::Scan,
                        header: Some(h),
                    });
                    // Nested streams (EXIF thumbnails) are smaller: skip them.
                    i = end;
                    continue;
                }
            }
        }
        i = s + 2;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jpeg::test_util::jpeg;

    #[test]
    fn finds_streams_in_noise() {
        let a = jpeg(320, 200);
        let b = jpeg(160, 120);
        let mut data: Vec<u8> = (0..5000u32).map(|i| (i * 7919 % 256) as u8).collect();
        data.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]); // false start
        let at_a = data.len();
        data.extend_from_slice(&a);
        data.extend(std::iter::repeat_n(0xFFu8, 10));
        let at_b = data.len();
        data.extend_from_slice(&b);
        data.extend_from_slice(&a[..a.len() / 2]); // truncated stream
        let found = scan_bytes(&data);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!((found[0].offset as usize, found[0].width), (at_a, 320));
        assert_eq!((found[1].offset as usize, found[1].len as usize), (at_b, b.len()));
    }
}
