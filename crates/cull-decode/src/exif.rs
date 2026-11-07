//! EXIF extraction into [`ExifInfo`].
//!
//! Metadata may come from several TIFF streams of one file (RAW IFDs, CR3
//! `CMT1..4` blocks, the EXIF of an embedded JPEG, a MakerNote). They are
//! merged into a [`MetaBuilder`] in priority order: the first source that
//! provides a field wins.

use cull_types::{ExifInfo, GpsPosition};

use crate::makernote;
use crate::source::{ByteSource, MemSource};
use crate::tiff::{Ifd, Tiff};

/// A JPEG preview found in a MakerNote (offset/length absolute in the file).
#[derive(Debug, Clone, Copy)]
pub(crate) struct NotePreview {
    pub offset: u64,
    pub len: u64,
    /// Second guess for the offset (MakerNote offset bases vary by model).
    pub alt_offset: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct MetaBuilder {
    pub captured: Option<i64>,
    captured_fallback: Option<i64>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    note_serial: Option<String>,
    pub lens: Option<String>,
    note_lens: Option<String>,
    pub iso: Option<u32>,
    pub exposure: Option<f32>,
    pub f_number: Option<f32>,
    pub focal: Option<f32>,
    pub orientation: Option<u8>,
    pub rating: Option<i8>,
    pub gps: Option<GpsPosition>,
    /// EXIF PixelXDimension/PixelYDimension.
    pub exif_dims: Option<(u32, u32)>,
    /// DNG/ARW DefaultCropSize of the raw image.
    pub crop_dims: Option<(u32, u32)>,
    /// Sensor active area declared by the container (RW2 borders, RAF).
    pub sensor_dims: Option<(u32, u32)>,
    /// Raw image IFD size (may include masked borders).
    pub raw_dims: Option<(u32, u32)>,
    /// Largest embedded preview.
    pub preview_dims: Option<(u32, u32)>,
    /// Pixel size of a plain image file (JPEG/PNG/TIFF).
    pub image_dims: Option<(u32, u32)>,
    /// MakerNote previews collected while reading metadata.
    pub note_previews: Vec<NotePreview>,
}

fn fill<T>(slot: &mut Option<T>, v: Option<T>) {
    if slot.is_none() {
        *slot = v;
    }
}

mod tag {
    pub const MAKE: u16 = 0x010F;
    pub const MODEL: u16 = 0x0110;
    pub const ORIENTATION: u16 = 0x0112;
    pub const DATE_TIME: u16 = 0x0132;
    pub const XMP: u16 = 0x02BC;
    pub const RATING: u16 = 0x4746;
    pub const EXPOSURE_TIME: u16 = 0x829A;
    pub const F_NUMBER: u16 = 0x829D;
    pub const EXIF_IFD: u16 = 0x8769;
    pub const ISO: u16 = 0x8827;
    pub const GPS_IFD: u16 = 0x8825;
    pub const SENSITIVITY_REI: u16 = 0x8832;
    pub const ISO_SPEED: u16 = 0x8833;
    pub const DATE_ORIGINAL: u16 = 0x9003;
    pub const DATE_DIGITIZED: u16 = 0x9004;
    pub const FOCAL_LENGTH: u16 = 0x920A;
    pub const MAKER_NOTE: u16 = 0x927C;
    pub const SUBSEC: u16 = 0x9290;
    pub const SUBSEC_ORIGINAL: u16 = 0x9291;
    pub const SUBSEC_DIGITIZED: u16 = 0x9292;
    pub const PIXEL_X: u16 = 0xA002;
    pub const PIXEL_Y: u16 = 0xA003;
    pub const BODY_SERIAL: u16 = 0xA431;
    pub const LENS_MODEL: u16 = 0xA434;
    pub const DNG_CAMERA_SERIAL: u16 = 0xC62F;
}

impl MetaBuilder {
    /// Reads IFD0 of `tiff`, its EXIF and GPS IFDs and the MakerNote.
    pub(crate) fn read_tiff(&mut self, src: &mut dyn ByteSource, tiff: &Tiff, ifd0: &Ifd) {
        self.read_main_ifd(src, tiff, ifd0);
        if let Some(e) = ifd0.get(tag::EXIF_IFD) {
            if let Some(off) = tiff.uint(src, e) {
                match tiff.read_ifd(src, off) {
                    Ok(exif) => self.read_exif_ifd(src, tiff, &exif),
                    Err(err) => log::debug!("EXIF IFD unreadable: {err:?}"),
                }
            }
        }
        if let Some(e) = ifd0.get(tag::GPS_IFD) {
            if let Some(off) = tiff.uint(src, e) {
                if let Ok(gps) = tiff.read_ifd(src, off) {
                    self.read_gps_ifd(src, tiff, &gps);
                }
            }
        }
    }

    /// Reads a complete in-memory TIFF stream (APP1 EXIF, PNG eXIf, …).
    pub(crate) fn read_tiff_bytes(&mut self, bytes: &[u8]) {
        let mut src = MemSource::new(bytes);
        let Ok(tiff) = Tiff::parse(&mut src, 0) else { return };
        if let Ok(ifd0) = tiff.read_ifd(&mut src, tiff.ifd0) {
            self.read_tiff(&mut src, &tiff, &ifd0);
        }
    }

    /// IFD0-level tags (also found in DNG/NEF IFD0 for some EXIF fields).
    pub(crate) fn read_main_ifd(&mut self, src: &mut dyn ByteSource, tiff: &Tiff, ifd: &Ifd) {
        let s = |src: &mut dyn ByteSource, t: u16| ifd.get(t).and_then(|e| tiff.string(src, e));
        fill(&mut self.make, s(src, tag::MAKE));
        fill(&mut self.model, s(src, tag::MODEL));
        if self.orientation.is_none() {
            self.orientation = ifd
                .get(tag::ORIENTATION)
                .and_then(|e| tiff.uint(src, e))
                .filter(|v| (1..=8).contains(v))
                .map(|v| v as u8);
        }
        if self.rating.is_none() {
            if let Some(v) = ifd.get(tag::RATING).and_then(|e| tiff.uint(src, e)) {
                self.rating = rating_from_exif(v);
            }
        }
        if let Some(e) = ifd.get(tag::XMP) {
            if let Ok(xmp) = tiff.bytes(src, e, crate::tiff::MAX_VALUE) {
                self.read_xmp(&xmp);
            }
        }
        fill(&mut self.serial, s(src, tag::DNG_CAMERA_SERIAL));
        // Some RAWs (NEF, DNG) repeat EXIF fields in IFD0. Their capture
        // date is only a fallback: NEF's IFD0 copy lacks the sub-seconds the
        // EXIF IFD has (bursts need them).
        self.read_capture_fields(src, tiff, ifd, false);
        if let Some(t) = s(src, tag::DATE_TIME).and_then(|d| parse_datetime(&d, None)) {
            fill(&mut self.captured_fallback, Some(t));
        }
    }

    pub(crate) fn read_exif_ifd(&mut self, src: &mut dyn ByteSource, tiff: &Tiff, ifd: &Ifd) {
        self.read_capture_fields(src, tiff, ifd, true);
        let s = |src: &mut dyn ByteSource, t: u16| ifd.get(t).and_then(|e| tiff.string(src, e));
        fill(&mut self.serial, s(src, tag::BODY_SERIAL).filter(|v| !is_placeholder(v)));
        fill(&mut self.lens, s(src, tag::LENS_MODEL).filter(|v| !is_placeholder(v)));
        if self.exif_dims.is_none() {
            let x = ifd.get(tag::PIXEL_X).and_then(|e| tiff.uint(src, e));
            let y = ifd.get(tag::PIXEL_Y).and_then(|e| tiff.uint(src, e));
            if let (Some(x), Some(y)) = (x, y) {
                if x > 0 && y > 0 {
                    self.exif_dims = Some((x, y));
                }
            }
        }
        if let Some(e) = ifd.get(tag::MAKER_NOTE) {
            let make = self.make.clone().unwrap_or_default();
            let note = makernote::read(src, tiff, e, &make);
            self.apply_note(note);
        }
    }

    /// Merges what a MakerNote provided.
    pub(crate) fn apply_note(&mut self, note: makernote::MakerNote) {
        fill(&mut self.note_serial, note.serial);
        fill(&mut self.note_lens, note.lens);
        self.note_previews.extend(note.previews);
    }

    fn read_capture_fields(&mut self, src: &mut dyn ByteSource, tiff: &Tiff, ifd: &Ifd, primary: bool) {
        let s = |src: &mut dyn ByteSource, t: u16| ifd.get(t).and_then(|e| tiff.string(src, e));
        let n = |src: &mut dyn ByteSource, t: u16| ifd.get(t).and_then(|e| tiff.number(src, e));
        let slot = if primary { &mut self.captured } else { &mut self.captured_fallback };
        if slot.is_none() {
            if let Some(d) = s(src, tag::DATE_ORIGINAL) {
                let sub = s(src, tag::SUBSEC_ORIGINAL).or_else(|| s(src, tag::SUBSEC));
                *slot = parse_datetime(&d, sub.as_deref());
            }
        }
        if let Some(d) = s(src, tag::DATE_DIGITIZED) {
            let sub = s(src, tag::SUBSEC_DIGITIZED);
            fill(&mut self.captured_fallback, parse_datetime(&d, sub.as_deref()));
        }
        fill(&mut self.exposure, n(src, tag::EXPOSURE_TIME).filter(|v| *v > 0.0).map(|v| v as f32));
        fill(&mut self.f_number, n(src, tag::F_NUMBER).filter(|v| *v > 0.0).map(|v| v as f32));
        fill(&mut self.focal, n(src, tag::FOCAL_LENGTH).filter(|v| *v > 0.0).map(|v| v as f32));
        if self.iso.is_none() {
            let iso = ifd.get(tag::ISO).and_then(|e| tiff.uint(src, e));
            // ISOSpeedRatings saturates at 65535: prefer the 32-bit tags then.
            self.iso = match iso {
                Some(v) if v > 0 && v < 65535 => Some(v),
                _ => ifd
                    .get(tag::ISO_SPEED)
                    .or_else(|| ifd.get(tag::SENSITIVITY_REI))
                    .and_then(|e| tiff.uint(src, e))
                    .filter(|v| *v > 0)
                    .or(iso.filter(|v| *v > 0)),
            };
        }
    }

    pub(crate) fn read_gps_ifd(&mut self, src: &mut dyn ByteSource, tiff: &Tiff, ifd: &Ifd) {
        if self.gps.is_some() {
            return;
        }
        let coord = |src: &mut dyn ByteSource, t: u16| -> Option<f64> {
            let v = tiff.numbers(src, ifd.get(t)?).ok()?;
            let deg = *v.first()?;
            let min = v.get(1).copied().unwrap_or(0.0);
            let sec = v.get(2).copied().unwrap_or(0.0);
            let c = deg + min / 60.0 + sec / 3600.0;
            c.is_finite().then_some(c)
        };
        let reference = |src: &mut dyn ByteSource, t: u16| -> Option<u8> {
            let b = tiff.bytes(src, ifd.get(t)?, 4).ok()?;
            b.first().copied()
        };
        let (Some(mut lat), Some(mut lon)) = (coord(src, 2), coord(src, 4)) else { return };
        if matches!(reference(src, 1), Some(b'S' | b's')) {
            lat = -lat;
        }
        if matches!(reference(src, 3), Some(b'W' | b'w')) {
            lon = -lon;
        }
        // Cameras without a fix often write zeros.
        if lat.abs() <= 90.0 && lon.abs() <= 180.0 && (lat != 0.0 || lon != 0.0) {
            self.gps = Some(GpsPosition { latitude: lat, longitude: lon });
        }
    }

    pub(crate) fn read_xmp(&mut self, xmp: &[u8]) {
        if self.rating.is_none() {
            self.rating = xmp_rating(xmp);
        }
    }

    /// Whether the essential fields are known (used to decide whether a
    /// secondary source such as an embedded JPEG's EXIF is worth reading).
    pub(crate) fn has_core(&self) -> bool {
        self.captured.is_some() && self.make.is_some() && self.orientation.is_some()
    }

    /// Orientation as read from the file, `None` when no source had one.
    pub(crate) fn orientation_tag(&self) -> Option<u8> {
        self.orientation
    }

    /// Final metadata. `raw` selects the RAW dimension policy.
    pub(crate) fn finish(&self, raw: bool) -> ExifInfo {
        let orientation = self.orientation.unwrap_or(1);
        let dims = if raw { self.raw_dimensions() } else { self.image_dims.or(self.exif_dims) };
        let (width, height) = match dims {
            Some((w, h)) if (5..=8).contains(&orientation) => (Some(h), Some(w)),
            Some((w, h)) => (Some(w), Some(h)),
            None => (None, None),
        };
        ExifInfo {
            captured_at_ms: self.captured.or(self.captured_fallback),
            camera_make: self.make.clone(),
            camera_model: self.model.clone(),
            camera_serial: self.serial.clone().or_else(|| self.note_serial.clone()),
            lens: self.lens.clone().or_else(|| self.note_lens.clone()),
            iso: self.iso,
            exposure_time_s: self.exposure,
            f_number: self.f_number,
            focal_length_mm: self.focal,
            orientation,
            width,
            height,
            rating: self.rating,
            gps: self.gps,
        }
    }

    /// Full-resolution size of a RAW (sensor orientation).
    fn raw_dimensions(&self) -> Option<(u32, u32)> {
        let area = |d: Option<(u32, u32)>| d.map_or(0, |(w, h)| w as u64 * h as u64);
        if self.crop_dims.is_some() {
            return self.crop_dims;
        }
        if self.sensor_dims.is_some() {
            return self.sensor_dims;
        }
        // EXIF PixelX/YDimension of RAWs is sometimes the preview size: only
        // trust it when it is at least as large as every preview.
        if self.exif_dims.is_some() && area(self.exif_dims) >= area(self.preview_dims) {
            return self.exif_dims;
        }
        // A full-size embedded JPEG is the best estimate of the developed size.
        if self.preview_dims.is_some() && area(self.preview_dims) * 10 >= area(self.raw_dims) * 7 {
            return self.preview_dims;
        }
        self.raw_dims.or(self.preview_dims).or(self.exif_dims)
    }
}

/// Values that some cameras write instead of leaving a tag out.
fn is_placeholder(v: &str) -> bool {
    v.bytes().all(|b| b == b'0' || b == b' ' || b == b'-') || v.eq_ignore_ascii_case("unknown")
}

/// EXIF `Rating` (0x4746): 0..=5 stars; some tools store -1 (0xFFFF) as rejected.
fn rating_from_exif(v: u32) -> Option<i8> {
    match v {
        0..=5 => Some(v as i8),
        0xFFFF | 0xFFFF_FFFF => Some(-1),
        _ => None,
    }
}

/// `xmp:Rating` from an XMP packet, as attribute (`xmp:Rating="3"`) or
/// element (`<xmp:Rating>3</xmp:Rating>`).
pub(crate) fn xmp_rating(xmp: &[u8]) -> Option<i8> {
    const KEYS: [&[u8]; 2] = [b"xmp:Rating", b"xap:Rating"];
    for key in KEYS {
        let mut from = 0;
        while let Some(rel) = find(&xmp[from..], key) {
            let mut i = from + rel + key.len();
            from = i;
            // Skip `="`, `='` or `>` plus whitespace.
            while i < xmp.len() && matches!(xmp[i], b'=' | b'"' | b'\'' | b'>' | b' ' | b'\t' | b'\n' | b'\r') {
                i += 1;
            }
            let neg = xmp.get(i) == Some(&b'-');
            if neg {
                i += 1;
            }
            let start = i;
            while i < xmp.len() && xmp[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                continue;
            }
            let v: i32 = std::str::from_utf8(&xmp[start..i]).ok()?.parse().ok()?;
            let v = if neg { -v } else { v };
            return match v {
                -1..=5 => Some(v as i8),
                _ => None,
            };
        }
    }
    None
}

pub(crate) fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Parses an EXIF date (`YYYY:MM:DD HH:MM:SS`, also tolerating `-`/`/`/`T`
/// separators and missing seconds) plus optional sub-second digits, as UTC
/// Unix milliseconds.
pub(crate) fn parse_datetime(s: &str, subsec: Option<&str>) -> Option<i64> {
    let nums: Vec<i64> = s
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .take(6)
        .map(|p| p.parse::<i64>().ok())
        .collect::<Option<Vec<_>>>()?;
    if nums.len() < 5 {
        return None;
    }
    let (y, mo, d, h, mi) = (nums[0], nums[1], nums[2], nums[3], nums[4]);
    let sec = nums.get(5).copied().unwrap_or(0);
    if !(1900..=9999).contains(&y) || !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    if h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    let days = days_from_civil(y, mo, d);
    let mut ms = ((days * 24 + h) * 60 + mi) * 60_000 + sec * 1000;
    if let Some(sub) = subsec {
        let digits: String = sub.trim().chars().take_while(|c| c.is_ascii_digit()).take(3).collect();
        if !digits.is_empty() {
            let v: i64 = digits.parse().ok()?;
            ms += v * 10i64.pow(3 - digits.len() as u32);
        }
    }
    Some(ms)
}

/// Days since 1970-01-01 of a proleptic Gregorian date (H. Hinnant).
pub(crate) fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`]: (year, month, day).
pub(crate) fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datetime() {
        assert_eq!(parse_datetime("1970:01:01 00:00:00", None), Some(0));
        assert_eq!(parse_datetime("2017:01:26 09:37:39", Some("06")), Some(1_485_423_459_060));
        assert_eq!(parse_datetime("2017:06:13 16:06:42", Some("687")), Some(1_497_370_002_687));
        assert_eq!(parse_datetime("2017-06-13T16:06:42", Some("5")), Some(1_497_370_002_500));
        assert_eq!(parse_datetime("2017:06:13 16:06", None), Some(1_497_369_960_000));
        assert_eq!(parse_datetime("0000:00:00 00:00:00", None), None);
        assert_eq!(parse_datetime("    :  :     :  :  ", None), None);
        assert_eq!(parse_datetime("2017:13:13 16:06:42", None), None);
        for days in [-1000i64, 0, 1, 17_000, 20_000, 100_000] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
    }

    #[test]
    fn xmp() {
        assert_eq!(xmp_rating(br#"<rdf:Description xmp:Rating="3" />"#), Some(3));
        assert_eq!(xmp_rating(b"<xmp:Rating>-1</xmp:Rating>"), Some(-1));
        assert_eq!(xmp_rating(b"<xap:Rating> 5 </xap:Rating>"), Some(5));
        assert_eq!(xmp_rating(b"xmp:Rating=\"9\""), None);
        assert_eq!(xmp_rating(b"<x:xmpmeta/>"), None);
        assert_eq!(xmp_rating(b"xmp:Rating"), None);
    }

    #[test]
    fn exif_rating() {
        assert_eq!(rating_from_exif(4), Some(4));
        assert_eq!(rating_from_exif(0xFFFF), Some(-1));
        assert_eq!(rating_from_exif(42), None);
    }

    #[test]
    fn raw_dims_policy() {
        let mut m =
            MetaBuilder { raw_dims: Some((6064, 4040)), preview_dims: Some((6048, 4024)), ..Default::default() };
        assert_eq!(m.finish(true).width, Some(6048));
        m.preview_dims = Some((1620, 1080));
        assert_eq!(m.finish(true).width, Some(6064));
        m.exif_dims = Some((6000, 4000));
        assert_eq!(m.finish(true).width, Some(6000));
        m.crop_dims = Some((5990, 3990));
        m.orientation = Some(6);
        let info = m.finish(true);
        assert_eq!((info.width, info.height, info.orientation), (Some(3990), Some(5990), 6));
    }
}
