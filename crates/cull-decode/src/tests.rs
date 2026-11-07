//! API-level tests on synthetic fixtures: small TIFF/RAW-like containers,
//! CR3/RAF/RW2 skeletons and JPEGs generated in code.

use super::*;
use crate::jpeg::test_util::{jpeg, pattern};
use crate::tiff::build::{tiff_le, IfdSpec, Val};
use std::path::PathBuf;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let p = std::env::temp_dir().join(format!("cull-decode-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }

    fn write(&self, rel: &str, data: &[u8]) -> PathBuf {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, data).unwrap();
        p
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn source(path: &Path) -> SourceFile {
    let kind = kind_of(path).unwrap_or(FileKind::Raw);
    SourceFile { path: path.to_path_buf(), kind, size: 0, modified_ms: 0 }
}

/// EXIF APP1 → JPEG.
fn jpeg_with_exif(w: u32, h: u32, info: &ExifInfo) -> Vec<u8> {
    let data = jpeg(w, h);
    let hdr = jpeg::read_header_mem(&data).unwrap();
    with_exif(&data, &hdr, info)
}

/// A NEF/DNG-like TIFF RAW: IFD0 (metadata, 160×120 RGB thumbnail, EXIF and
/// SubIFD pointers), SubIFD0 = big JPEG via 0x0201/0x0202, SubIFD1 = raw CFA,
/// IFD1 = small JPEG thumbnail; `padding` zero bytes of fake raw data sit
/// between the structure and the big JPEG (to check minimal I/O).
fn tiff_raw(big: &[u8], small: &[u8], orientation: u16, padding: usize) -> Vec<u8> {
    let build = |p: [u32; 6]| {
        // p = [exif ifd, subifd0, subifd1, thumb rgb, small jpeg, big jpeg]
        let ifds = [
            IfdSpec {
                entries: vec![
                    (0x00FE, Val::Long(vec![1])),
                    (0x0100, Val::Long(vec![160])),
                    (0x0101, Val::Long(vec![120])),
                    (0x0102, Val::Short(vec![8, 8, 8])),
                    (0x0103, Val::Short(vec![1])),
                    (0x0106, Val::Short(vec![2])),
                    (0x010F, Val::Ascii("NIKON CORPORATION".into())),
                    (0x0110, Val::Ascii("NIKON Z 6".into())),
                    (0x0111, Val::Long(vec![p[3]])),
                    (0x0112, Val::Short(vec![orientation])),
                    (0x0115, Val::Short(vec![3])),
                    (0x0117, Val::Long(vec![160 * 120 * 3])),
                    (0x014A, Val::Long(vec![p[1], p[2]])),
                    (0x8769, Val::Long(vec![p[0]])),
                ],
            },
            IfdSpec {
                entries: vec![
                    (0x829A, Val::Rational(vec![(1, 250)])),
                    (0x829D, Val::Rational(vec![(28, 10)])),
                    (0x8827, Val::Short(vec![800])),
                    (0x9003, Val::Ascii("2024:06:15 14:30:05".into())),
                    (0x9291, Val::Ascii("42".into())),
                    (0x920A, Val::Rational(vec![(850, 10)])),
                    (0xA431, Val::Ascii("6035906".into())),
                    (0xA434, Val::Ascii("NIKKOR Z 85mm f/1.8 S".into())),
                ],
            },
            IfdSpec {
                entries: vec![
                    (0x00FE, Val::Long(vec![1])),
                    (0x0103, Val::Short(vec![6])),
                    (0x0201, Val::Long(vec![p[5]])),
                    (0x0202, Val::Long(vec![big.len() as u32])),
                ],
            },
            IfdSpec {
                entries: vec![
                    (0x00FE, Val::Long(vec![0])),
                    (0x0100, Val::Long(vec![4000])),
                    (0x0101, Val::Long(vec![3000])),
                    (0x0102, Val::Short(vec![14])),
                    (0x0103, Val::Short(vec![34713])),
                    (0x0106, Val::Short(vec![32803])),
                ],
            },
            IfdSpec {
                entries: vec![
                    (0x0103, Val::Short(vec![6])),
                    (0x0201, Val::Long(vec![p[4]])),
                    (0x0202, Val::Long(vec![small.len() as u32])),
                ],
            },
        ];
        tiff_le(42, &ifds, false)
    };
    // Pass 1 for the layout, pass 2 with the real pointers.
    let (first, offs) = build([0; 6]);
    let thumb_at = first.len() as u32;
    let small_at = thumb_at + 160 * 120 * 3;
    let big_at = small_at + small.len() as u32 + padding as u32;
    let (mut out, offs2) = build([offs[1], offs[2], offs[3], thumb_at, small_at, big_at]);
    assert_eq!(offs, offs2);
    // IFD0 → IFD1 chain (the small JPEG IFD is the last one built).
    let next_pos = 8 + 2 + 14 * 12;
    out[next_pos..next_pos + 4].copy_from_slice(&offs[4].to_le_bytes());
    out.extend(pattern(160, 120).into_raw());
    out.extend_from_slice(small);
    out.extend(std::iter::repeat_n(0u8, padding));
    out.extend_from_slice(big);
    out
}

#[test]
fn tiff_raw_preview_exif_and_best_jpeg() {
    let dir = TempDir::new("tiffraw");
    let big = jpeg(1600, 1000);
    let small = jpeg(160, 100);
    let path = dir.write("DSC_0001.NEF", &tiff_raw(&big, &small, 6, 0));
    let file = source(&path);

    let p = load_preview(&file, 400).unwrap();
    assert_eq!(p.source, PreviewSource::EmbeddedJpeg);
    // 1600×1000 decoded at 1/4 (400 px long edge) then rotated 90°.
    assert_eq!(p.image.dimensions(), (250, 400));
    assert_eq!((p.source_width, p.source_height), (1000, 1600));
    let e = &p.exif;
    assert_eq!(e.orientation, 6);
    assert_eq!(e.camera_make.as_deref(), Some("NIKON CORPORATION"));
    assert_eq!(e.camera_model.as_deref(), Some("NIKON Z 6"));
    assert_eq!(e.camera_serial.as_deref(), Some("6035906"));
    assert_eq!(e.lens.as_deref(), Some("NIKKOR Z 85mm f/1.8 S"));
    assert_eq!(e.iso, Some(800));
    assert_eq!(e.exposure_time_s, Some(0.004));
    assert_eq!(e.f_number, Some(2.8));
    assert_eq!(e.focal_length_mm, Some(85.0));
    assert_eq!(e.captured_at_ms, exif::parse_datetime("2024:06:15 14:30:05", Some("42")));
    // Full size: raw IFD 4000×3000 (the 1600 px JPEG is too small to be it), upright.
    assert_eq!((e.width, e.height), (Some(3000), Some(4000)));
    assert_eq!(read_exif(&path).unwrap(), p.exif);

    // Tiny request: the thumbnail would do but the cheapest sufficient wins.
    let p = load_preview(&file, 100).unwrap();
    assert_eq!(p.image.dimensions(), (100, 160));
    // Bigger than any preview: the largest one, at full size.
    let p = load_preview(&file, 5000).unwrap();
    assert_eq!(p.image.dimensions(), (1000, 1600));

    // best_jpeg_bytes: the big JPEG, gaining an EXIF with orientation 6.
    let b = best_jpeg_bytes(&file).unwrap();
    let h = jpeg::read_header_mem(&b).unwrap();
    assert_eq!((h.width, h.height), (1600, 1000));
    assert_eq!(jpeg::orientation_slot(&b, &h).unwrap().value, 6);
    let mut m = MetaBuilder::default();
    let (o, l) = h.exif.unwrap();
    m.read_tiff_bytes(&b[o as usize..o as usize + l]);
    let carried = m.finish(false);
    assert_eq!(carried.camera_model.as_deref(), Some("NIKON Z 6"));
    assert_eq!(carried.captured_at_ms, e.captured_at_ms);
    // Pixel data untouched: same scan bytes as the embedded JPEG.
    assert!(b.ends_with(&big[big.len() - 1000..]));
}

#[test]
fn minimal_io_on_big_raw() {
    let dir = TempDir::new("io");
    let big = jpeg(1600, 1000);
    let small = jpeg(160, 100);
    let pad = 30 << 20; // 30 MB of "raw data"
    let path = dir.write("DSC_0002.NEF", &tiff_raw(&big, &small, 1, pad));
    let probe = probe_previews(&path, 400).unwrap();
    assert_eq!(probe.previews.len(), 3, "{probe:?}");
    let chosen = probe.chosen.unwrap();
    assert_eq!((chosen.width, chosen.height), (1600, 1000));
    // Structure + candidate headers only: a few 64 KiB blocks.
    assert!(probe.bytes_read < 512 * 1024, "read {} bytes", probe.bytes_read);
    let p = load_preview(&source(&path), 400).unwrap();
    assert_eq!(p.image.dimensions(), (400, 250));
    let info = read_exif(&path).unwrap();
    assert_eq!(info.orientation, 1);
}

#[test]
fn rgb_thumbnail_only() {
    // A DNG whose only preview is an uncompressed RGB strip.
    let dir = TempDir::new("rgb");
    let (first, _) = tiff_le(
        42,
        &[IfdSpec {
            entries: vec![
                (0x0100, Val::Long(vec![256])),
                (0x0101, Val::Long(vec![192])),
                (0x0102, Val::Short(vec![8, 8, 8])),
                (0x0103, Val::Short(vec![1])),
                (0x0106, Val::Short(vec![2])),
                (0x0111, Val::Long(vec![0])),
                (0x0115, Val::Short(vec![3])),
                (0x0117, Val::Long(vec![256 * 192 * 3])),
            ],
        }],
        false,
    );
    let at = first.len() as u32;
    let (mut data, _) = tiff_le(
        42,
        &[IfdSpec {
            entries: vec![
                (0x0100, Val::Long(vec![256])),
                (0x0101, Val::Long(vec![192])),
                (0x0102, Val::Short(vec![8, 8, 8])),
                (0x0103, Val::Short(vec![1])),
                (0x0106, Val::Short(vec![2])),
                (0x0111, Val::Long(vec![at])),
                (0x0115, Val::Short(vec![3])),
                (0x0117, Val::Long(vec![256 * 192 * 3])),
            ],
        }],
        false,
    );
    data.extend(pattern(256, 192).into_raw());
    let path = dir.write("IMG_1.DNG", &data);
    let p = load_preview(&source(&path), 100).unwrap();
    assert_eq!(p.image.dimensions(), (128, 96));
    assert_eq!(p.image, pixels::reduce_pow2(pattern(256, 192), 100));
    let b = best_jpeg_bytes(&source(&path)).unwrap();
    assert_eq!(jpeg::read_header_mem(&b).unwrap().width, 256);
}

/// ISOBMFF box.
fn bx(typ: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut v = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
    v.extend_from_slice(typ);
    v.extend_from_slice(payload);
    v
}

fn uuid_box(uuid: &[u8; 16], payload: &[u8]) -> Vec<u8> {
    let mut p = uuid.to_vec();
    p.extend_from_slice(payload);
    bx(b"uuid", &p)
}

/// THMB v0: version/flags, width, height, size, 4 unknown bytes, JPEG.
fn thmb_box(w: u16, h: u16, jpeg: &[u8]) -> Vec<u8> {
    let mut p = vec![0u8; 4];
    p.extend_from_slice(&w.to_be_bytes());
    p.extend_from_slice(&h.to_be_bytes());
    p.extend_from_slice(&(jpeg.len() as u32).to_be_bytes());
    p.extend_from_slice(&[0, 1, 0, 0]);
    p.extend_from_slice(jpeg);
    bx(b"THMB", &p)
}

/// PRVW (version 0 or 1): version/flags, ?, width, height, ?, size, image.
fn prvw_box(version: u8, w: u16, h: u16, image: &[u8]) -> Vec<u8> {
    let mut p = vec![version, 0, 0, 0, 0, 1];
    p.extend_from_slice(&w.to_be_bytes());
    p.extend_from_slice(&h.to_be_bytes());
    p.extend_from_slice(&[0, 1]);
    p.extend_from_slice(&(image.len() as u32).to_be_bytes());
    p.extend_from_slice(image);
    bx(b"PRVW", &p)
}

fn cr3(full: &[u8], prvw: &[u8], thmb: &[u8], orientation: u16) -> Vec<u8> {
    let cmt1 = tiff_le(
        42,
        &[IfdSpec {
            entries: vec![
                (0x010F, Val::Ascii("Canon".into())),
                (0x0110, Val::Ascii("Canon EOS R6".into())),
                (0x0112, Val::Short(vec![orientation])),
            ],
        }],
        false,
    )
    .0;
    let cmt2 = tiff_le(
        42,
        &[IfdSpec {
            entries: vec![
                (0x9003, Val::Ascii("2023:09:02 16:45:10".into())),
                (0x9291, Val::Ascii("37".into())),
                (0x8827, Val::Short(vec![1600])),
                (0xA434, Val::Ascii("RF24-70mm F2.8 L IS USM".into())),
            ],
        }],
        false,
    )
    .0;
    let cmt3 = tiff_le(42, &[IfdSpec { entries: vec![(0x000C, Val::Long(vec![123456789]))] }], false).0;
    let canon = [
        bx(b"CNCV", b"CanonCR3_001/00.09.00/00.00.00"),
        bx(b"CMT1", &cmt1),
        bx(b"CMT2", &cmt2),
        bx(b"CMT3", &cmt3),
        thmb_box(160, 120, thmb),
    ]
    .concat();
    let canon_uuid = uuid_box(&crate::raw::test_uuids::CANON, &canon);
    let ftyp = bx(b"ftyp", b"crx \0\0\0\x01crx isom");
    let xmp = uuid_box(&crate::raw::test_uuids::XMP, br#"<x:xmpmeta><rdf:Description xmp:Rating="4"/></x:xmpmeta>"#);
    let mut prvw_payload = vec![0u8; 8];
    prvw_payload.extend(prvw_box(0, 1620, 1080, prvw));
    let preview = uuid_box(&crate::raw::test_uuids::PREVIEW, &prvw_payload);

    // The track points into mdat: compute the layout with a placeholder.
    let trak = |off: u64| {
        let mut stsz = vec![0u8; 4];
        stsz.extend_from_slice(&(full.len() as u32).to_be_bytes());
        stsz.extend_from_slice(&1u32.to_be_bytes());
        let mut co64 = vec![0u8; 4];
        co64.extend_from_slice(&1u32.to_be_bytes());
        co64.extend_from_slice(&off.to_be_bytes());
        let stbl = bx(b"stbl", &[bx(b"stsz", &stsz), bx(b"co64", &co64)].concat());
        bx(b"trak", &bx(b"mdia", &bx(b"minf", &stbl)))
    };
    let layout = |off: u64| {
        let moov = bx(b"moov", &[canon_uuid.clone(), trak(off)].concat());
        [ftyp.clone(), moov, xmp.clone(), preview.clone()].concat()
    };
    let head_len = layout(0).len() as u64;
    let mut out = layout(head_len + 8);
    out.extend(bx(b"mdat", full));
    out
}

#[test]
fn cr3_previews_and_metadata() {
    let dir = TempDir::new("cr3");
    let full = jpeg(2400, 1600);
    let prvw = jpeg(1620, 1080);
    let thmb = jpeg(160, 120);
    let path = dir.write("IMG_0001.CR3", &cr3(&full, &prvw, &thmb, 8));
    let file = source(&path);

    let probe = probe_previews(&path, 1000).unwrap();
    let mut sizes: Vec<u32> = probe.previews.iter().map(|p| p.width).collect();
    sizes.sort();
    assert_eq!(sizes, vec![160, 1620, 2400], "{probe:?}");
    // Whichever is cheaper (PRVW at 1/1 or the full JPEG at 1/2), the
    // decoded size follows from it (rotated by orientation 8).
    let chosen = probe.chosen.unwrap();
    assert!(chosen.width >= 1620);
    let d = jpeg::scale_denominator(chosen.width, 1000);
    let (cw, ch) = jpeg::scaled_size(chosen.width, chosen.height, d);
    let p = load_preview(&file, 1000).unwrap();
    assert_eq!(p.image.dimensions(), (ch, cw));
    assert_eq!(p.exif.orientation, 8);
    // Thumbnail-sized request: the THMB.
    assert_eq!(load_preview(&file, 100).unwrap().image.dimensions(), (120, 160));
    assert_eq!(p.exif.camera_model.as_deref(), Some("Canon EOS R6"));
    assert_eq!(p.exif.lens.as_deref(), Some("RF24-70mm F2.8 L IS USM"));
    assert_eq!(p.exif.iso, Some(1600));
    assert_eq!(p.exif.rating, Some(4));
    assert_eq!(p.exif.camera_serial.as_deref(), Some("123456789"));
    assert_eq!(p.exif.captured_at_ms, exif::parse_datetime("2023:09:02 16:45:10", Some("37")));
    assert_eq!((p.exif.width, p.exif.height), (Some(1600), Some(2400)));

    // 1/2 of the full JPEG is 1200 px: not enough for 1700, so the full one at 1/1.
    let p = load_preview(&file, 1700).unwrap();
    assert_eq!(p.image.dimensions(), (1600, 2400));

    let b = best_jpeg_bytes(&file).unwrap();
    let h = jpeg::read_header_mem(&b).unwrap();
    assert_eq!(h.width, 2400);
    assert_eq!(jpeg::orientation_slot(&b, &h).unwrap().value, 8);
}

#[test]
fn raf_preview_and_exif_from_embedded_jpeg() {
    let dir = TempDir::new("raf");
    let info = ExifInfo {
        camera_make: Some("FUJIFILM".into()),
        camera_model: Some("X-T3".into()),
        orientation: 6,
        captured_at_ms: Some(1_537_894_452_000),
        ..Default::default()
    };
    let embedded = jpeg_with_exif(1920, 1280, &info);
    let mut data = b"FUJIFILMCCD-RAW 0201FF383501X-T3".to_vec();
    data.resize(160, 0);
    let jpeg_at = 160u32;
    data[84..88].copy_from_slice(&jpeg_at.to_be_bytes());
    data[88..92].copy_from_slice(&(embedded.len() as u32).to_be_bytes());
    let dir_at = jpeg_at + embedded.len() as u32;
    data[92..96].copy_from_slice(&dir_at.to_be_bytes());
    let mut rafdir = vec![0, 0, 0, 1, 0x01, 0x11, 0, 4];
    rafdir.extend_from_slice(&4160u16.to_be_bytes());
    rafdir.extend_from_slice(&6240u16.to_be_bytes());
    data[96..100].copy_from_slice(&(rafdir.len() as u32).to_be_bytes());
    data.extend_from_slice(&embedded);
    data.extend_from_slice(&rafdir);
    let path = dir.write("DSCF0001.RAF", &data);

    let p = load_preview(&source(&path), 1440).unwrap();
    assert_eq!(p.image.dimensions(), (1280, 1920));
    assert_eq!(p.exif.camera_model.as_deref(), Some("X-T3"));
    assert_eq!(p.exif.captured_at_ms, info.captured_at_ms);
    assert_eq!((p.exif.width, p.exif.height), (Some(4160), Some(6240)));
    // The embedded JPEG already has the right orientation: returned as is.
    assert_eq!(best_jpeg_bytes(&source(&path)).unwrap(), embedded);
}

#[test]
fn rw2_jpg_from_raw() {
    let dir = TempDir::new("rw2");
    let embedded = jpeg(1920, 1440);
    let build = |at: u32| {
        tiff_le(
            0x55,
            &[IfdSpec {
                entries: vec![
                    (0x0004, Val::Short(vec![8])),
                    (0x0005, Val::Short(vec![8])),
                    (0x0006, Val::Short(vec![3896])),
                    (0x0007, Val::Short(vec![5192])),
                    (0x002E, Val::Undefined(vec![0; 8])),
                    (0x010F, Val::Ascii("Panasonic".into())),
                    (0x0110, Val::Ascii("DC-GH5".into())),
                    (0x0112, Val::Short(vec![3])),
                    (0x0111, Val::Long(vec![at])),
                ],
            }],
            false,
        )
        .0
    };
    // Make JpgFromRaw point at the appended JPEG: patch its count/offset.
    let mut data = build(0);
    let entry = 8 + 2 + 4 * 12; // 5th entry (sorted by tag) = 0x002E
    let at = data.len() as u32;
    data[entry + 4..entry + 8].copy_from_slice(&(embedded.len() as u32).to_le_bytes());
    data[entry + 8..entry + 12].copy_from_slice(&at.to_le_bytes());
    data.extend_from_slice(&embedded);
    let path = dir.write("P1000001.RW2", &data);
    let p = load_preview(&source(&path), 900).unwrap();
    assert_eq!(p.image.dimensions(), (960, 720));
    assert_eq!(p.exif.orientation, 3);
    assert_eq!((p.exif.width, p.exif.height), (Some(5184), Some(3888)));
    assert_eq!(p.exif.camera_model.as_deref(), Some("DC-GH5"));
}

#[test]
fn jpeg_png_tiff_files() {
    let dir = TempDir::new("files");
    let info = ExifInfo { orientation: 6, camera_make: Some("SONY".into()), ..Default::default() };
    let j = jpeg_with_exif(800, 600, &info);
    let path = dir.write("A.JPG", &j);
    let p = load_preview(&source(&path), 200).unwrap();
    assert_eq!(p.source, PreviewSource::File);
    assert_eq!(p.image.dimensions(), (150, 200));
    assert_eq!((p.source_width, p.source_height), (600, 800));
    assert_eq!((p.exif.width, p.exif.height), (Some(600), Some(800)));
    assert_eq!(p.exif.camera_make.as_deref(), Some("SONY"));
    assert_eq!(best_jpeg_bytes(&source(&path)).unwrap(), j);
    assert_eq!(read_exif(&path).unwrap().orientation, 6);

    let img = pattern(400, 300);
    let png = dir.0.join("B.png");
    img.save(&png).unwrap();
    let p = load_preview(&source(&png), 100).unwrap();
    assert_eq!(p.image.dimensions(), (100, 75));
    assert_eq!((p.source_width, p.source_height), (400, 300));
    let b = best_jpeg_bytes(&source(&png)).unwrap();
    assert_eq!(jpeg::read_header_mem(&b).unwrap().width, 400);
    assert_eq!(read_exif(&png).unwrap().width, Some(400));

    let tif = dir.0.join("C.tif");
    img.save(&tif).unwrap();
    let p = load_preview(&source(&tif), 300).unwrap();
    assert_eq!(p.image.dimensions(), (400, 300));
    assert_eq!(read_exif(&tif).unwrap().height, Some(300));
    assert!(best_jpeg_bytes(&source(&tif)).is_ok());

    // HEIF: metadata only.
    let heic = dir.write("D.HEIC", &bx(b"ftyp", b"heic\0\0\0\0mif1heic"));
    assert!(matches!(load_preview(&source(&heic), 100), Err(DecodeError::Unsupported(_))));
    assert_eq!(read_exif(&heic).unwrap().orientation, 1);
}

#[test]
fn fallback_scan_on_unknown_container() {
    let dir = TempDir::new("scan");
    let mut data = b"\0MRM\0\0\x01\0unknown container".to_vec();
    data.extend((0..100_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8));
    data.extend_from_slice(&jpeg(1200, 800));
    data.extend((0..5000u32).map(|i| i as u8));
    let path = dir.write("PICT0001.MRW", &data);
    let p = load_preview(&source(&path), 600).unwrap();
    assert_eq!(p.image.dimensions(), (600, 400));
    assert_eq!(read_exif(&path).unwrap().orientation, 1);
}

#[test]
fn corrupt_inputs_never_panic() {
    let dir = TempDir::new("corrupt");
    let big = jpeg(640, 480);
    let small = jpeg(160, 120);
    let samples: Vec<(&str, Vec<u8>)> = vec![
        ("a.nef", tiff_raw(&big, &small, 6, 0)),
        ("b.cr3", cr3(&big, &small, &small, 6)),
        ("c.jpg", big.clone()),
        ("d.raf", {
            let mut d = b"FUJIFILMCCD-RAW 0201".to_vec();
            d.resize(100, 0xFF);
            d.extend_from_slice(&big);
            d
        }),
        ("e.rw2", {
            let mut d = b"IIU\0\x08\0\0\0".to_vec();
            d.extend_from_slice(&[0xFF; 64]);
            d
        }),
    ];
    let mut seed = 0x9E37_79B9u32;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let mut checked = 0;
    for (name, data) in &samples {
        // Truncations.
        let n = data.len();
        let cuts: Vec<usize> = [0, 1, 2, 4, 7, 8, 9, 16, 30, 64, 100, 200, 500, 1000, n / 3, n / 2, n - 1]
            .into_iter()
            .filter(|&c| c < n)
            .collect();
        for cut in cuts {
            let p = dir.write(&format!("t{cut}_{name}"), &data[..cut]);
            let _ = load_preview(&source(&p), 200);
            let _ = best_jpeg_bytes(&source(&p));
            let _ = read_exif(&p);
            checked += 1;
        }
        // Random byte flips.
        for k in 0..40 {
            let mut d = data.clone();
            for _ in 0..1 + k % 8 {
                let i = rnd() as usize % d.len().min(4096);
                d[i] = rnd() as u8;
            }
            let p = dir.write(&format!("f{k}_{name}"), &d);
            let _ = load_preview(&source(&p), 200);
            let _ = best_jpeg_bytes(&source(&p));
            let _ = read_exif(&p);
            checked += 1;
        }
    }
    // Pure garbage under every RAW extension.
    for ext in ["cr2", "cr3", "nef", "arw", "raf", "dng", "orf", "rw2", "pef", "srw", "jpg", "png", "tif"] {
        let d: Vec<u8> = (0..3000).map(|_| rnd() as u8).collect();
        let p = dir.write(&format!("garbage.{ext}"), &d);
        assert!(load_preview(&source(&p), 200).is_err());
        let _ = read_exif(&p);
    }
    // An empty file and a missing file.
    let p = dir.write("empty.nef", b"");
    assert!(load_preview(&source(&p), 200).is_err());
    let missing = dir.0.join("missing.cr3");
    assert!(matches!(load_preview(&source(&missing), 200), Err(DecodeError::Io { .. })));
    assert!(matches!(read_exif(&missing), Err(DecodeError::Io { .. })));
    assert!(checked > 100);
}

#[test]
fn hevc_cr3_is_unsupported() {
    let dir = TempDir::new("hevc");
    let mut hevc = b"\0\0\0\x14CISZ\0\0\0\0\0\0\x06\x80\0\0\x04\x38\0\0\0\xb0hvcC".to_vec();
    hevc.resize(400, 7);
    let mut prvw_payload = vec![0u8; 8];
    prvw_payload.extend(prvw_box(1, 1620, 1080, &hevc));
    let data = [bx(b"ftyp", b"crx \0\0\0\x01"), uuid_box(&crate::raw::test_uuids::PREVIEW, &prvw_payload)].concat();
    let path = dir.write("IMG_9999.CR3", &data);
    match load_preview(&source(&path), 100) {
        Err(DecodeError::Unsupported(msg)) => assert!(msg.contains("HEVC"), "{msg}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn scan_pairs_sidecars_and_skips() {
    let dir = TempDir::new("scanfolder");
    let f = |rel: &str| {
        dir.write(rel, b"x");
    };
    f("IMG_1.CR3");
    f("IMG_1.JPG");
    f("IMG_1.xmp");
    f("IMG_2.NEF");
    f("IMG_2.NEF.xmp");
    f("IMG_2.xmp");
    f("IMG_10.jpg");
    f("IMG_3.dng");
    f("IMG_3.CR2");
    f("IMG_4.jpg");
    f("IMG_4.heic");
    f("IMG_5.jpg");
    f("IMG_5.png");
    f("._IMG_1.JPG");
    f(".DS_Store");
    f("Thumbs.db");
    f("notes.txt");
    f(".hidden/IMG_6.jpg");
    f("_pelli_export/IMG_7.jpg");
    f("@eaDir/IMG_8.jpg");
    f("Day 2/IMG_9.ARW");
    f("Day 2/sub/IMG_11.jpg");

    let all = scan_folder(&dir.0, &ScanOptions::default()).unwrap();
    let rels: Vec<&str> = all.iter().map(|a| a.rel_path.as_str()).collect();
    assert_eq!(
        rels,
        vec![
            "Day 2/IMG_9.ARW",
            "Day 2/sub/IMG_11.jpg",
            "IMG_1.CR3",
            "IMG_2.NEF",
            "IMG_3.CR2",
            "IMG_4.jpg",
            "IMG_5.jpg",
            "IMG_5.png",
            "IMG_10.jpg",
        ],
    );
    let get = |rel: &str| all.iter().find(|a| a.rel_path == rel).unwrap();
    let a1 = get("IMG_1.CR3");
    assert_eq!(a1.stem, "IMG_1");
    assert_eq!(a1.primary.kind, FileKind::Raw);
    assert_eq!(a1.companions.len(), 1);
    assert_eq!(a1.companions[0].kind, FileKind::Jpeg);
    assert_eq!(a1.xmp_sidecar.as_ref().unwrap().file_name().unwrap(), "IMG_1.xmp");
    assert_eq!(a1.id, scan::asset_id("img_1.cr3"));
    assert_eq!(a1.primary.size, 1);
    assert!(a1.primary.modified_ms > 0);
    // The exact `IMG_2.NEF.xmp` wins over `IMG_2.xmp`.
    assert_eq!(get("IMG_2.NEF").xmp_sidecar.as_ref().unwrap().file_name().unwrap(), "IMG_2.NEF.xmp");
    // Camera RAW over DNG.
    assert_eq!(get("IMG_3.CR2").companions[0].path.file_name().unwrap(), "IMG_3.dng");
    assert_eq!(get("IMG_4.jpg").companions[0].kind, FileKind::Heif);
    assert!(get("IMG_5.jpg").companions.is_empty());

    // Unpaired: the stem sidecar goes to the RAW.
    let opts = ScanOptions { pair_raw_jpeg: false, recursive: false, ..Default::default() };
    let flat = scan_folder(&dir.0, &opts).unwrap();
    assert_eq!(flat.len(), 10);
    let jpg = flat.iter().find(|a| a.rel_path == "IMG_1.JPG").unwrap();
    assert!(jpg.xmp_sidecar.is_none());
    let cr3 = flat.iter().find(|a| a.rel_path == "IMG_1.CR3").unwrap();
    assert!(cr3.xmp_sidecar.is_some());

    assert!(matches!(scan_folder(&dir.0.join("nope"), &opts), Err(DecodeError::Io { .. })));
}

#[cfg(unix)]
#[test]
fn scan_never_escapes_through_symlinks() {
    let dir = TempDir::new("symlinks");
    let outside = TempDir::new("symlinks-outside");
    outside.write("SECRET.jpg", b"x");
    outside.write("sub/OUT.jpg", b"x");
    let root = dir.0.join("shoot");
    dir.write("shoot/IMG_1.jpg", b"x");
    std::os::unix::fs::symlink(outside.0.join("SECRET.jpg"), root.join("LINK_OUT.jpg")).unwrap();
    std::os::unix::fs::symlink(outside.0.join("sub"), root.join("linked_dir")).unwrap();
    std::os::unix::fs::symlink(root.join("IMG_1.jpg"), root.join("LINK_IN.jpg")).unwrap();
    std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();
    let all = scan_folder(&root, &ScanOptions::default()).unwrap();
    let rels: Vec<&str> = all.iter().map(|a| a.rel_path.as_str()).collect();
    assert_eq!(rels, vec!["IMG_1.jpg", "LINK_IN.jpg"]);
}

#[test]
fn scan_skips_folders_filled_by_the_export() {
    let dir = TempDir::new("exported");
    dir.write("IMG_1.CR3", b"x");
    dir.write("_Rejets Pelli/IMG_2.CR3", b"x");
    dir.write(&format!("_Rejets Pelli/{EXPORT_MARKER}"), b"");
    dir.write("Selects/IMG_1.CR3", b"x");
    dir.write(&format!("Selects/{EXPORT_MARKER}"), b"");
    dir.write("Selects/Day 2/IMG_3.CR3", b"x");
    dir.write("Day 2/IMG_3.CR3", b"x");
    let rels = |root: &Path| -> Vec<String> {
        scan_folder(root, &ScanOptions::default()).unwrap().into_iter().map(|a| a.rel_path).collect()
    };
    assert_eq!(rels(&dir.0), ["Day 2/IMG_3.CR3", "IMG_1.CR3"]);
    // Opened on its own, an export folder is a shoot like another.
    assert_eq!(rels(&dir.0.join("Selects")), ["Day 2/IMG_3.CR3", "IMG_1.CR3"]);
}

#[test]
fn scan_is_fast_on_10k_files() {
    let dir = TempDir::new("scan10k");
    for d in 0..10 {
        for i in 0..500 {
            dir.write(&format!("card{d}/IMG_{i:04}.CR3"), b"");
            dir.write(&format!("card{d}/IMG_{i:04}.JPG"), b"");
        }
    }
    let t = std::time::Instant::now();
    let all = scan_folder(&dir.0, &ScanOptions::default()).unwrap();
    let ms = t.elapsed().as_millis();
    assert_eq!(all.len(), 5000);
    assert!(all.iter().all(|a| a.companions.len() == 1));
    assert_eq!(all[0].rel_path, "card0/IMG_0000.CR3");
    assert_eq!(all[4999].rel_path, "card9/IMG_0499.CR3");
    eprintln!("scan_folder: 10 000 files in {ms} ms");
    // Generous bound for slow CI disks.
    assert!(ms < 2000, "scan of 10 000 files took {ms} ms");
}

#[test]
fn resize_and_encode_api() {
    let img = pattern(1000, 500);
    assert_eq!(resize_long_edge(&img, 250).dimensions(), (250, 125));
    assert_eq!(resize_long_edge(&img, 2000).dimensions(), (1000, 500));
    let j = encode_jpeg(&img, 80);
    let back = jpeg::decode_scaled(&j, 0).unwrap();
    assert_eq!(back.dimensions(), (125, 63));
}

/// A camera JPEG: the main image, then a smaller copy listed in an APP2
/// Multi-Picture Format index (as Sony, Nikon, Canon write them).
fn camera_jpeg_with_mpf(main: &[u8], preview: &[u8], preview_type: u32) -> Vec<u8> {
    // MPF TIFF: header, IFD with one MP Entry tag pointing at two entries.
    let tiff_len = 8 + 2 + 12 + 4 + 32;
    let app2_len = 2 + 4 + tiff_len;
    let main_len = main.len() + 2 + app2_len;
    let mut t = Vec::new();
    t.extend_from_slice(b"II*\0");
    t.extend_from_slice(&8u32.to_le_bytes());
    t.extend_from_slice(&1u16.to_le_bytes());
    t.extend_from_slice(&0xB002u16.to_le_bytes());
    t.extend_from_slice(&7u16.to_le_bytes());
    t.extend_from_slice(&32u32.to_le_bytes());
    t.extend_from_slice(&26u32.to_le_bytes()); // entries right after the IFD
    t.extend_from_slice(&0u32.to_le_bytes());
    // Entry 1: the primary image; entry 2: the preview, offset from the TIFF
    // header (APP2 payload after "MPF\0", 2 + 2 + 2 + 4 bytes into the file).
    let tiff_start = 2 + 4 + 4;
    for (attr, size, offset) in
        [(0x2003_0000u32, main_len as u32, 0u32), (preview_type, preview.len() as u32, (main_len - tiff_start) as u32)]
    {
        t.extend_from_slice(&attr.to_le_bytes());
        t.extend_from_slice(&size.to_le_bytes());
        t.extend_from_slice(&offset.to_le_bytes());
        t.extend_from_slice(&[0, 0, 0, 0]);
    }
    assert_eq!(t.len(), tiff_len);
    let mut out = vec![0xFF, 0xD8, 0xFF, 0xE2];
    out.extend_from_slice(&(app2_len as u16).to_be_bytes());
    out.extend_from_slice(b"MPF\0");
    out.extend_from_slice(&t);
    out.extend_from_slice(&main[2..]);
    assert_eq!(out.len(), main_len);
    out.extend_from_slice(preview);
    out
}

#[test]
fn camera_jpeg_mpf_preview_is_used_when_large_enough() {
    let dir = TempDir::new("mpf");
    // Main 3000x2000 (decoded at 1/2 = 1500x1000), camera copy 1620x1080.
    let main = jpeg(3000, 2000);
    let path = dir.write("cam.jpg", &camera_jpeg_with_mpf(&main, &jpeg(1620, 1080), 0x0001_0002));
    let p = load_preview(&source(&path), 1440).unwrap();
    assert_eq!(p.image.dimensions(), (1620, 1080), "the MPF copy");
    assert_eq!((p.source_width, p.source_height), (3000, 2000));
    // Too small for the request: the main image, DCT-scaled.
    let p = load_preview(&source(&path), 1700).unwrap();
    assert_eq!(p.image.dimensions(), (3000, 2000));
    // A letterboxed preview (other aspect ratio) or another MP type is ignored.
    let path = dir.write("box.jpg", &camera_jpeg_with_mpf(&main, &jpeg(1920, 1080), 0x0001_0002));
    assert_eq!(load_preview(&source(&path), 1440).unwrap().image.dimensions(), (1500, 1000));
    let path = dir.write("pano.jpg", &camera_jpeg_with_mpf(&main, &jpeg(1620, 1080), 0x0002_0001));
    assert_eq!(load_preview(&source(&path), 1440).unwrap().image.dimensions(), (1500, 1000));
    // Damaged index: never fails the decode.
    let mut bad = camera_jpeg_with_mpf(&main, &jpeg(1620, 1080), 0x0001_0002);
    bad[14..18].copy_from_slice(&[0xFF; 4]);
    let path = dir.write("bad.jpg", &bad);
    assert_eq!(load_preview(&source(&path), 1440).unwrap().image.dimensions(), (1500, 1000));
}
