//! A small EXIF (TIFF) writer.
//!
//! The JPEGs embedded in RAWs often carry no EXIF at all (CR2/CR3 full-size
//! JPEG, NEF JpgFromRaw). When [`crate::best_jpeg_bytes`] hands such a JPEG to
//! a Pelli gallery or a viewer, we give it an APP1 block with the RAW's
//! orientation (so viewers rotate it) and its key shooting data (so the
//! gallery shows camera, lens, exposure, date and GPS).

use cull_types::ExifInfo;

use crate::exif::civil_from_days;

enum V {
    Ascii(String),
    Short(u16),
    Long(u32),
    Rational(Vec<(u32, u32)>),
    Undefined(Vec<u8>),
    Bytes(Vec<u8>),
}

impl V {
    fn encode(&self) -> (u16, u32, Vec<u8>) {
        match self {
            V::Ascii(s) => {
                let mut b: Vec<u8> = s.bytes().filter(|&c| c != 0).take(255).collect();
                b.push(0);
                (2, b.len() as u32, b)
            }
            V::Short(v) => (3, 1, v.to_le_bytes().to_vec()),
            V::Long(v) => (4, 1, v.to_le_bytes().to_vec()),
            V::Rational(r) => {
                (5, r.len() as u32, r.iter().flat_map(|(n, d)| [n.to_le_bytes(), d.to_le_bytes()].concat()).collect())
            }
            V::Undefined(b) => (7, b.len() as u32, b.clone()),
            V::Bytes(b) => (1, b.len() as u32, b.clone()),
        }
    }
}

/// An encoded entry: tag, type, count, value bytes.
type Encoded = (u16, u16, u32, Vec<u8>);

/// Serialises IFDs back to back. `links[i] = Some((tag, j))` makes entry
/// `tag` of IFD `i` point to IFD `j`.
fn write_tiff(ifds: Vec<Vec<(u16, V)>>, links: &[(usize, u16, usize)]) -> Vec<u8> {
    let encoded: Vec<Vec<Encoded>> = ifds
        .into_iter()
        .map(|mut entries| {
            entries.sort_by_key(|(t, _)| *t);
            entries
                .into_iter()
                .map(|(t, v)| {
                    let (typ, count, bytes) = v.encode();
                    (t, typ, count, bytes)
                })
                .collect()
        })
        .collect();
    // Layout: each IFD table followed by its out-of-line data (even-aligned).
    let mut offsets = Vec::new();
    let mut pos = 8u32;
    for entries in &encoded {
        offsets.push(pos);
        let data: u32 = entries.iter().map(|e| if e.3.len() > 4 { (e.3.len() as u32 + 1) & !1 } else { 0 }).sum();
        pos += 2 + 12 * entries.len() as u32 + 4 + data;
    }
    let mut out = Vec::with_capacity(pos as usize);
    out.extend_from_slice(b"II*\0");
    out.extend_from_slice(&8u32.to_le_bytes());
    for (i, entries) in encoded.iter().enumerate() {
        let table_end = offsets[i] + 2 + 12 * entries.len() as u32 + 4;
        let mut data = Vec::new();
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        for (tag, typ, count, bytes) in entries {
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&typ.to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            if let Some(&(_, _, target)) = links.iter().find(|(from, t, _)| *from == i && t == tag) {
                out.extend_from_slice(&offsets[target].to_le_bytes());
            } else if bytes.len() <= 4 {
                let mut b = bytes.clone();
                b.resize(4, 0);
                out.extend_from_slice(&b);
            } else {
                out.extend_from_slice(&(table_end + data.len() as u32).to_le_bytes());
                data.extend_from_slice(bytes);
                if data.len() % 2 == 1 {
                    data.push(0);
                }
            }
        }
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&data);
    }
    out
}

/// A TIFF stream holding only an orientation tag.
#[cfg(test)]
pub(crate) fn orientation_only(orientation: u8) -> Vec<u8> {
    write_tiff(vec![vec![(0x0112, V::Short(orientation.clamp(1, 8) as u16))]], &[])
}

/// A TIFF stream with the orientation and the shooting data of `info`.
pub(crate) fn from_info(info: &ExifInfo) -> Vec<u8> {
    let mut ifd0 = vec![(0x0112, V::Short(info.orientation.clamp(1, 8) as u16))];
    if let Some(s) = &info.camera_make {
        ifd0.push((0x010F, V::Ascii(s.clone())));
    }
    if let Some(s) = &info.camera_model {
        ifd0.push((0x0110, V::Ascii(s.clone())));
    }
    if let Some(r) = info.rating.filter(|r| (0..=5).contains(r)) {
        ifd0.push((0x4746, V::Short(r as u16)));
    }

    let mut exif = vec![(0x9000, V::Undefined(b"0232".to_vec()))];
    if let Some(t) = info.exposure_time_s.filter(|t| t.is_finite() && *t > 0.0) {
        exif.push((0x829A, V::Rational(vec![exposure_rational(t as f64)])));
    }
    if let Some(f) = info.f_number.filter(|f| f.is_finite() && *f > 0.0) {
        exif.push((0x829D, V::Rational(vec![tenths(f as f64)])));
    }
    if let Some(iso) = info.iso {
        exif.push((0x8827, V::Short(iso.min(65535) as u16)));
        if iso > 65535 {
            exif.push((0x8833, V::Long(iso)));
        }
    }
    if let Some(ms) = info.captured_at_ms {
        let (date, sub) = format_datetime(ms);
        exif.push((0x9003, V::Ascii(date)));
        exif.push((0x9291, V::Ascii(sub)));
    }
    if let Some(f) = info.focal_length_mm.filter(|f| f.is_finite() && *f > 0.0) {
        exif.push((0x920A, V::Rational(vec![tenths(f as f64)])));
    }
    if let Some(s) = &info.camera_serial {
        exif.push((0xA431, V::Ascii(s.clone())));
    }
    if let Some(s) = &info.lens {
        exif.push((0xA434, V::Ascii(s.clone())));
    }
    ifd0.push((0x8769, V::Long(0)));
    let mut ifds = vec![ifd0, exif];
    let mut links = vec![(0usize, 0x8769u16, 1usize)];

    if let Some(g) = info.gps {
        ifds[0].push((0x8825, V::Long(0)));
        let gps = vec![
            (0x0000, V::Bytes(vec![2, 3, 0, 0])),
            (0x0001, V::Ascii(if g.latitude < 0.0 { "S" } else { "N" }.into())),
            (0x0002, V::Rational(dms(g.latitude))),
            (0x0003, V::Ascii(if g.longitude < 0.0 { "W" } else { "E" }.into())),
            (0x0004, V::Rational(dms(g.longitude))),
        ];
        ifds.push(gps);
        links.push((0, 0x8825, 2));
    }
    write_tiff(ifds, &links)
}

fn tenths(v: f64) -> (u32, u32) {
    ((v * 10.0).round().clamp(0.0, u32::MAX as f64) as u32, 10)
}

/// 1/250 s → 1/250; 2.5 s → 25/10; 1/3.2 s → 312500/1000000.
fn exposure_rational(t: f64) -> (u32, u32) {
    if t >= 1.0 {
        return tenths(t);
    }
    let den = (1.0 / t).round();
    if den >= 1.0 && ((1.0 / den) - t).abs() / t < 0.01 {
        (1, den.min(u32::MAX as f64) as u32)
    } else {
        ((t * 1e6).round() as u32, 1_000_000)
    }
}

fn dms(v: f64) -> Vec<(u32, u32)> {
    let a = v.abs();
    let deg = a.floor();
    let min = ((a - deg) * 60.0).floor();
    let sec = ((a - deg) * 60.0 - min) * 60.0;
    vec![(deg as u32, 1), (min as u32, 1), ((sec * 10_000.0).round() as u32, 10_000)]
}

/// `YYYY:MM:DD HH:MM:SS` and the millisecond sub-second string.
fn format_datetime(ms: i64) -> (String, String) {
    let days = ms.div_euclid(86_400_000);
    let rem = ms.rem_euclid(86_400_000);
    let (y, m, d) = civil_from_days(days);
    let (h, mi, s, frac) = (rem / 3_600_000, rem / 60_000 % 60, rem / 1000 % 60, rem % 1000);
    (format!("{y:04}:{m:02}:{d:02} {h:02}:{mi:02}:{s:02}"), format!("{frac:03}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exif::MetaBuilder;
    use cull_types::GpsPosition;

    #[test]
    fn round_trip() {
        let info = ExifInfo {
            captured_at_ms: Some(1_485_423_459_060),
            camera_make: Some("Canon".into()),
            camera_model: Some("Canon EOS R6".into()),
            camera_serial: Some("012345".into()),
            lens: Some("RF24-70mm F2.8 L IS USM".into()),
            iso: Some(3200),
            exposure_time_s: Some(0.004),
            f_number: Some(2.8),
            focal_length_mm: Some(50.0),
            orientation: 6,
            width: None,
            height: None,
            rating: Some(4),
            gps: Some(GpsPosition { latitude: 48.390_4, longitude: -4.486_1 }),
        };
        let tiff = from_info(&info);
        let mut m = MetaBuilder::default();
        m.read_tiff_bytes(&tiff);
        let back = m.finish(false);
        assert_eq!(back.captured_at_ms, info.captured_at_ms);
        assert_eq!(back.camera_make, info.camera_make);
        assert_eq!(back.camera_model, info.camera_model);
        assert_eq!(back.camera_serial, info.camera_serial);
        assert_eq!(back.lens, info.lens);
        assert_eq!(back.iso, info.iso);
        assert_eq!(back.exposure_time_s, Some(0.004));
        assert_eq!(back.f_number, Some(2.8));
        assert_eq!(back.focal_length_mm, Some(50.0));
        assert_eq!(back.orientation, 6);
        assert_eq!(back.rating, Some(4));
        let g = back.gps.unwrap();
        assert!((g.latitude - 48.3904).abs() < 1e-6 && (g.longitude + 4.4861).abs() < 1e-6);

        let mut m = MetaBuilder::default();
        m.read_tiff_bytes(&orientation_only(8));
        assert_eq!(m.finish(false).orientation, 8);
    }

    #[test]
    fn rationals() {
        assert_eq!(exposure_rational(0.004), (1, 250));
        assert_eq!(exposure_rational(1.0 / 3200.0), (1, 3200));
        assert_eq!(exposure_rational(2.5), (25, 10));
        assert_eq!(format_datetime(0), ("1970:01:01 00:00:00".into(), "000".into()));
    }
}
