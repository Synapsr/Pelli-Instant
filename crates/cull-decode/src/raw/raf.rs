//! Fujifilm RAF.
//!
//! Big-endian header: magic `FUJIFILMCCD-RAW `, then at offset 84 the
//! embedded JPEG (offset, length) and at 92 the RAF directory (offset,
//! length). The embedded JPEG (1920 px on old bodies, 4416 px and more on
//! X-Trans IV) carries the camera's whole EXIF, which [`super::probe`] reads
//! from the validated preview. The directory gives the sensor crop size.

use super::{Candidate, RawProbe};
use crate::fault::FResult;
use crate::source::ByteSource;

pub(super) fn probe(src: &mut dyn ByteSource, probe: &mut RawProbe, cands: &mut Vec<Candidate>) -> FResult<()> {
    let jpeg_off = src.read_u32_be(84)? as u64;
    let jpeg_len = src.read_u32_be(88)? as u64;
    if jpeg_off > 0 && jpeg_len > 0 {
        cands.push(Candidate::jpeg(jpeg_off, jpeg_len));
        probe.exif_in_preview = true;
    }
    let dir_off = src.read_u32_be(92)? as u64;
    let dir_len = src.read_u32_be(96)? as u64;
    if dir_off > 0 && (4..=1 << 20).contains(&dir_len) {
        if let Ok(d) = src.read_at(dir_off, dir_len as usize) {
            probe.meta.sensor_dims = sensor_size(&d);
        }
    }
    Ok(())
}

/// RAF directory: `count(4)`, then `tag(2) size(2) data(size)` records.
/// 0x0111 = RawImageCroppedSize, 0x0100 = RawImageFullSize (u16 pairs).
fn sensor_size(d: &[u8]) -> Option<(u32, u32)> {
    let be16 = |i: usize| d.get(i..i + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let count = u32::from_be_bytes(d.get(0..4)?.try_into().ok()?);
    let mut pos = 4usize;
    let mut full = None;
    let mut cropped = None;
    for _ in 0..count.min(512) {
        let tag = be16(pos)?;
        let size = be16(pos + 2)? as usize;
        let data = pos + 4;
        if size == 4 && (tag == 0x0100 || tag == 0x0111) {
            let (a, b) = (be16(data)? as u32, be16(data + 2)? as u32);
            // Sensors are landscape: the larger value is the width.
            let dims = (a.max(b), a.min(b));
            if dims.1 > 0 {
                if tag == 0x0111 {
                    cropped = Some(dims)
                } else {
                    full = Some(dims)
                }
            }
        }
        pos = data + size;
    }
    cropped.or(full)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory() {
        let mut d = vec![0, 0, 0, 2];
        d.extend_from_slice(&[0x01, 0x00, 0, 4, 0x10, 0x56, 0x18, 0xF0]); // 4182 x 6384
        d.extend_from_slice(&[0x01, 0x11, 0, 4, 0x10, 0x40, 0x18, 0x60]); // 4160 x 6240
        assert_eq!(sensor_size(&d), Some((6240, 4160)));
        assert_eq!(sensor_size(&d[..10]), None);
        assert_eq!(sensor_size(&[]), None);
    }
}
