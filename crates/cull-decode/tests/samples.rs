//! Integration tests on real camera files.
//!
//! Runs on whatever `scripts/fetch-samples.sh` put in `samples/raw/`
//! (CC0 files from raw.pixls.us, never committed). With no samples the tests
//! pass and say so. Known files are also checked against reference metadata
//! (exiv2 dumps published by raw.pixls.us).

use std::path::{Path, PathBuf};

use cull_decode::{best_jpeg_bytes, decode_jpeg, load_preview, probe_previews, read_exif, DecodeError, PreviewSource};
use cull_types::{FileKind, SourceFile};

const MIN_EDGE: u32 = 1440;

fn samples() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/raw");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect())
        .unwrap_or_default();
    v.retain(|p| p.extension().and_then(|e| e.to_str()).and_then(FileKind::from_extension).is_some());
    v.sort();
    if v.is_empty() {
        eprintln!("note: no samples in {} — run scripts/fetch-samples.sh; skipping", dir.display());
    }
    v
}

fn source(path: &Path) -> SourceFile {
    let kind = path.extension().and_then(|e| e.to_str()).and_then(FileKind::from_extension).unwrap_or(FileKind::Raw);
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    SourceFile { path: path.to_path_buf(), kind, size, modified_ms: 0 }
}

/// Reference values for the files of `scripts/fetch-samples.sh`.
struct Expect {
    file: &'static str,
    model: &'static str,
    serial: Option<&'static str>,
    lens: Option<&'static str>,
    iso: Option<u32>,
    /// Full-resolution size (landscape samples, orientation 1).
    size: Option<(u32, u32)>,
    /// `YYYY:MM:DD HH:MM:SS.mmm` as Unix ms.
    captured_ms: Option<i64>,
    /// Largest embedded preview.
    preview: Option<(u32, u32)>,
    /// `true` when no preview can be decoded (HEVC CR3).
    unsupported: bool,
}

const EXPECT: &[Expect] = &[
    Expect {
        file: "canon-5d-mark-iv.cr2",
        model: "Canon EOS 5D Mark IV",
        serial: Some("013021003973"),
        lens: Some("24-70mm"),
        iso: Some(100),
        size: Some((6720, 4480)),
        captured_ms: Some(1_485_423_459_060),
        preview: Some((6720, 4480)),
        unsupported: false,
    },
    Expect {
        file: "canon-eos-r6.cr3",
        model: "Canon EOS R6",
        serial: Some("093022004357"),
        lens: Some("RF70-200mm F2.8 L IS USM"),
        iso: Some(100),
        size: Some((5472, 3648)),
        captured_ms: None,
        preview: Some((5472, 3648)),
        unsupported: false,
    },
    Expect {
        file: "canon-eos-r5-mark-ii-craw.cr3",
        model: "Canon EOS R5m2",
        serial: None,
        lens: Some("RF135mm F1.8 L IS USM"),
        iso: Some(640),
        size: None,
        captured_ms: None,
        preview: None,
        unsupported: true,
    },
    Expect {
        file: "fujifilm-x-t3.raf",
        model: "X-T3",
        serial: Some("8CQ01785"),
        lens: Some("XF35mmF2 R WR"),
        iso: Some(160),
        size: Some((6240, 4160)),
        captured_ms: Some(1_537_894_452_000),
        preview: Some((4416, 2944)),
        unsupported: false,
    },
    Expect {
        file: "google-pixel-6-pro.dng",
        model: "Pixel 6 Pro",
        serial: None,
        lens: None,
        iso: Some(44),
        size: None,
        captured_ms: None,
        preview: Some((680, 512)),
        unsupported: false,
    },
    Expect {
        file: "nikon-d750.nef",
        model: "NIKON D750",
        serial: Some("6025730"),
        lens: None,
        iso: Some(100),
        size: Some((6016, 4016)),
        captured_ms: Some(1_484_516_307_590),
        preview: Some((6016, 4016)),
        unsupported: false,
    },
    Expect {
        file: "nikon-z6.nef",
        model: "NIKON Z 6",
        serial: Some("6035906"),
        lens: Some("NIKKOR Z 24-70mm f/4 S"),
        iso: Some(100),
        size: Some((6048, 4024)),
        captured_ms: Some(1_575_047_644_680),
        preview: Some((6048, 4024)),
        unsupported: false,
    },
    Expect {
        file: "olympus-e-m1-mark-ii.orf",
        model: "E-M1MarkII",
        serial: Some("BHUA30758"),
        lens: Some("OLYMPUS M.12-40mm F2.8"),
        iso: Some(400),
        size: None,
        captured_ms: Some(1_512_073_276_000),
        preview: Some((3200, 2400)),
        unsupported: false,
    },
    Expect {
        file: "panasonic-gh5.rw2",
        model: "DC-GH5",
        serial: Some("XHL1704170065"),
        lens: None,
        iso: Some(400),
        size: Some((5184, 3888)),
        captured_ms: Some(1_497_370_002_687),
        preview: Some((1920, 1440)),
        unsupported: false,
    },
    Expect {
        file: "pentax-k-1-mark-ii.pef",
        model: "PENTAX K-1 Mark II",
        serial: Some("8023510"),
        lens: None,
        iso: Some(200),
        size: Some((4800, 3200)),
        captured_ms: None,
        preview: Some((4800, 3200)),
        unsupported: false,
    },
    Expect {
        file: "ricoh-gr-iii.dng",
        model: "RICOH GR III",
        serial: None,
        lens: None,
        iso: Some(100),
        size: Some((6000, 4000)),
        captured_ms: Some(1_555_265_359_000),
        preview: Some((6000, 4000)),
        unsupported: false,
    },
    Expect {
        file: "samsung-nx500.srw",
        model: "NX500",
        serial: None,
        lens: Some("NX 16-50mm F3.5-5.6 Power Zoom"),
        iso: Some(100),
        size: Some((6480, 4320)),
        captured_ms: None,
        preview: Some((6480, 4320)),
        unsupported: false,
    },
    Expect {
        file: "sony-a7m3.arw",
        model: "ILCE-7M3",
        serial: None,
        lens: Some("FE 16-35mm F2.8 GM"),
        iso: Some(400),
        size: Some((6000, 4000)),
        captured_ms: Some(1_520_959_093_000),
        preview: Some((1616, 1080)),
        unsupported: false,
    },
    Expect {
        file: "sony-a7m4.arw",
        model: "ILCE-7M4",
        serial: None,
        lens: Some("FE 50mm F2.5 G"),
        iso: Some(640),
        size: Some((4608, 3072)),
        captured_ms: None,
        preview: Some((4608, 3072)),
        unsupported: false,
    },
];

fn luma(img: &image::RgbImage) -> Vec<f32> {
    img.pixels().map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32).collect()
}

#[test]
fn read_exif_matches_reference() {
    for path in samples() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let e = read_exif(&path).unwrap_or_else(|err| panic!("{name}: {err}"));
        assert!(e.camera_make.is_some() && e.camera_model.is_some(), "{name}: {e:?}");
        assert!(e.captured_at_ms.is_some(), "{name}: no capture time");
        assert!((1..=8).contains(&e.orientation), "{name}");
        assert!(e.width.is_some() && e.height.is_some(), "{name}: no size");
        let Some(x) = EXPECT.iter().find(|x| x.file == name) else { continue };
        assert_eq!(e.camera_model.as_deref(), Some(x.model), "{name}");
        if let Some(s) = x.serial {
            assert_eq!(e.camera_serial.as_deref(), Some(s), "{name}");
        }
        if let Some(l) = x.lens {
            assert_eq!(e.lens.as_deref(), Some(l), "{name}");
        }
        assert_eq!(e.iso, x.iso, "{name}");
        if let Some((w, h)) = x.size {
            assert_eq!((e.width, e.height), (Some(w), Some(h)), "{name}");
        }
        if let Some(t) = x.captured_ms {
            assert_eq!(e.captured_at_ms, Some(t), "{name}");
        }
        assert!(e.exposure_time_s.is_some() && e.f_number.is_some() && e.focal_length_mm.is_some(), "{name}: {e:?}");
    }
}

#[test]
fn previews_decode_upright_and_minimal() {
    for path in samples() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let expect = EXPECT.iter().find(|x| x.file == name);
        let file = source(&path);
        let result = load_preview(&file, MIN_EDGE);
        if expect.is_some_and(|x| x.unsupported) {
            assert!(matches!(result, Err(DecodeError::Unsupported(_))), "{name}: {result:?}");
            assert!(matches!(best_jpeg_bytes(&file), Err(DecodeError::Unsupported(_))), "{name}");
            eprintln!("{name}: unsupported as expected");
            continue;
        }
        let p = result.unwrap_or_else(|err| panic!("{name}: {err}"));
        assert_eq!(p.source, PreviewSource::EmbeddedJpeg, "{name}");
        assert_eq!(p.exif, read_exif(&path).unwrap(), "{name}: EXIF differs between load_preview and read_exif");

        // DCT scaling: long edge >= MIN_EDGE when the source allows it, and
        // less than twice that (no larger factor would have been enough).
        let (w, h) = p.image.dimensions();
        let src_long = p.source_width.max(p.source_height);
        let long = w.max(h);
        if src_long >= MIN_EDGE {
            assert!(long >= MIN_EDGE, "{name}: {w}x{h}");
            assert!(long < 2 * MIN_EDGE || long == src_long, "{name}: {w}x{h} from {src_long}");
        } else {
            assert_eq!(long, src_long, "{name}");
        }
        // Upright: same aspect as the upright source.
        assert_eq!(w >= h, p.source_width >= p.source_height, "{name}");

        // A real photo, not a blank/garbled buffer.
        let y = luma(&p.image);
        let mean = y.iter().sum::<f32>() / y.len() as f32;
        let var = y.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / y.len() as f32;
        assert!(var > 50.0, "{name}: luma variance {var}");

        // Structure-driven: only a few blocks read to find the preview.
        let probe = probe_previews(&path, MIN_EDGE).unwrap();
        let size = std::fs::metadata(&path).unwrap().len();
        assert!(probe.bytes_read <= (1 << 20).max(size / 20), "{name}: read {} of {size} bytes", probe.bytes_read);
        if let Some((pw, ph)) = expect.and_then(|x| x.preview) {
            let largest = probe.previews.iter().max_by_key(|p| p.width as u64 * p.height as u64).unwrap();
            assert_eq!((largest.width, largest.height), (pw, ph), "{name}");
        }
        eprintln!("{name}: {w}x{h} from {}x{} ({} KiB read)", p.source_width, p.source_height, probe.bytes_read / 1024);
    }
}

#[test]
fn best_jpeg_is_largest_preview_with_orientation() {
    for path in samples() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if EXPECT.iter().any(|x| x.file == name && x.unsupported) {
            continue;
        }
        let bytes = best_jpeg_bytes(&source(&path)).unwrap_or_else(|err| panic!("{name}: {err}"));
        let probe = probe_previews(&path, MIN_EDGE).unwrap();
        let largest = probe.previews.iter().max_by_key(|p| p.width as u64 * p.height as u64).unwrap();
        // Decodes, at the size of the largest preview.
        let img = decode_jpeg(&bytes, 0).unwrap_or_else(|err| panic!("{name}: {err}"));
        assert_eq!(img.dimensions(), (largest.width.div_ceil(8), largest.height.div_ceil(8)), "{name}");
        // Carries EXIF with an orientation and the shooting data.
        let dir = std::env::temp_dir().join(format!("cull-best-{}-{name}.jpg", std::process::id()));
        std::fs::write(&dir, &bytes).unwrap();
        let e = read_exif(&dir).unwrap();
        std::fs::remove_file(&dir).ok();
        let raw = read_exif(&path).unwrap();
        assert_eq!(e.orientation, raw.orientation, "{name}");
        assert_eq!(e.camera_model, raw.camera_model, "{name}");
        assert_eq!(e.captured_at_ms.map(|t| t / 1000), raw.captured_at_ms.map(|t| t / 1000), "{name}");
    }
}

/// The specialised reduced decoder against jpeg-decoder on real camera
/// JPEGs: identical luma IDCT, so luma must match closely.
#[test]
fn fast_decoder_matches_reference_on_camera_jpegs() {
    use std::io::{Read, Seek, SeekFrom};
    for path in samples() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let Ok(probe) = probe_previews(&path, MIN_EDGE) else { continue };
        for p in probe.previews.iter().filter(|p| p.is_jpeg && p.width.max(p.height) >= 2 * MIN_EDGE) {
            let mut f = std::fs::File::open(&path).unwrap();
            f.seek(SeekFrom::Start(p.offset)).unwrap();
            let mut data = vec![0u8; p.len as usize];
            f.read_exact(&mut data).unwrap();
            let ours = decode_jpeg(&data, MIN_EDGE).unwrap();
            let mut d = jpeg_decoder::Decoder::new(&data[..]);
            d.read_info().unwrap();
            d.scale(ours.width() as u16, ours.height() as u16).unwrap();
            let px = d.decode().unwrap();
            let info = d.info().unwrap();
            assert_eq!((info.width as u32, info.height as u32), ours.dimensions(), "{name}");
            let theirs = image::RgbImage::from_raw(info.width as u32, info.height as u32, px).unwrap();
            let (a, b) = (luma(&ours), luma(&theirs));
            let mad = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).sum::<f32>() / a.len() as f32;
            assert!(mad < 1.5, "{name} {}x{}: luma mean abs diff {mad}", p.width, p.height);
        }
    }
}
