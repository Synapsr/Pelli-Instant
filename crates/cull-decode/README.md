# cull-decode

Folder scanning, EXIF, RAW embedded previews and fast reduced-size JPEG
decoding for Pelli Instant. RAWs are never demosaiced: the camera's own JPEG
rendering embedded in the file is located from the container structure, read
with a single ranged read and decoded directly at 1/2, 1/4 or 1/8 size.

Public API (contract used by `cull-engine`): `scan_folder`, `read_exif`,
`load_preview`, `best_jpeg_bytes`, `resize_long_edge`, `encode_jpeg`, plus
`decode_jpeg` (scaled decode of JPEG bytes) and `probe_previews`
(diagnostics: previews found, the one chosen, bytes read).

## Supported formats

| Format | Where the preview is found | Verified on real file |
| --- | --- | --- |
| JPEG | the file itself | synthetic tests |
| PNG, TIFF | decoded by `image`, reduced by powers of two | synthetic tests |
| CR2 (Canon) | IFD0 single strip, Compression 6 (full-size JPEG); IFD1 thumbnail | EOS 5D Mark IV |
| CR3 (Canon) | ISOBMFF: `PRVW` in uuid `eaf42b5e…` (1620 px), track 1 first sample via `stsz`/`co64` (full size), `THMB`; metadata from `CMT1..CMT4` TIFF blocks, rating from the XMP uuid box | EOS R6 |
| CR3 HDR PQ (R5 II, R1…) | previews are HEVC (`CISZ`/`hvcC`): reported `Unsupported` | EOS R5 Mark II (CRAW, HDR PQ) |
| NEF/NRW (Nikon) | SubIFDs (0x014A) with `JPEGInterchangeFormat` (JpgFromRaw full size + 1620 px preview) | D750, Z 6 |
| ARW/SR2/SRF (Sony) | IFD0 `JPEGInterchangeFormat` (1616 px), extra IFD with full-size JPEG on newer bodies | A7 III, A7 IV |
| DNG | IFD chain and SubIFDs: preview IFDs with Compression 7 (single strip) or 0x0201/0x0202; uncompressed 8-bit RGB thumbnails as last resort | Ricoh GR III, Pixel 6 Pro |
| RAF (Fujifilm) | big-endian header: JPEG offset at 84, length at 88; EXIF read from that JPEG; sensor size from the RAF directory (offset 92) | X-T3 |
| ORF (Olympus/OM) | MakerNote `CameraSettings` (0x2020) `PreviewImageStart/Length`; magic `IIRO`/`IIRS`/`MMOR` | E-M1 Mark II |
| RW2 (Panasonic) | IFD0 tag 0x002E `JpgFromRaw` (magic `IIU`); its EXIF completes the metadata | GH5 |
| PEF (Pentax) | IFD chain `JPEGInterchangeFormat` (full-size JPEG at the end of file); MakerNote preview (0x0004/0x0005) | K-1 Mark II |
| SRW (Samsung) | SubIFDs `JPEGInterchangeFormat` | NX500 |
| Other TIFF RAWs (3FR, ERF, MEF, MOS, KDC, DCR, IIQ…) | same TIFF walk | not verified |
| Unknown containers (CRW, MRW, X3F…) | fallback: read the file and keep the largest complete SOI…EOI stream with a decodable frame header | synthetic test |
| HEIF/HEIC | `Unsupported` for pixels; EXIF read via kamadak-exif | — |

Every candidate is validated (SOI, a baseline/extended/progressive frame
header, inside the file) before use; lossless JPEG (the raw data of CR2/DNG)
is rejected by its SOF3 marker. `best_jpeg_bytes` returns the largest
preview; `load_preview` decodes the preview that is *cheapest* to bring to
`min_long_edge` (e.g. a NEF's 1620 px preview rather than its 6048 px
JpgFromRaw at 1/4), using a cost model fitted on the samples.

The RAW's orientation is applied to its preview. When the embedded JPEG has
no EXIF (CR2, CR3, NEF), `best_jpeg_bytes` inserts an APP1 block with the
orientation and the shooting data (camera, lens, exposure, date, GPS) so
viewers rotate it and Pelli galleries can display the metadata; an embedded
orientation that disagrees with the RAW's is patched in place.

### Metadata

Capture time with sub-seconds (EXIF IFD `DateTimeOriginal` + `SubSecTimeOriginal`
win over IFD0 copies, read as UTC), make, model, body serial (EXIF
`BodySerialNumber`, DNG `CameraSerialNumber`, else MakerNote: Canon, Nikon,
Olympus, Pentax, Fujifilm, Panasonic), lens (`LensModel`, Canon/Olympus
MakerNote), ISO (incl. the 32-bit tags above 65535), exposure, aperture,
focal length, orientation, full-resolution size (DefaultCropSize, sensor
borders, EXIF pixel size or full-size preview, upright), rating (EXIF 0x4746,
`xmp:Rating` in embedded XMP) and GPS. Sony bodies have no readable serial.

### Minimal I/O

Files are read through a 16 × 64 KiB block cache; the chosen preview is one
direct ranged read. Finding and validating the previews of the samples reads
64–384 KiB per file (e.g. 192 KiB of a 34 MB CR2); the whole file is read
only by the fallback scanner on unknown containers.

## JPEG decoding

| Output | Decoder |
| --- | --- |
| 1/2, 1/4, 1/8 | `fastjpeg.rs`: specialised baseline Huffman decoder that materialises only the coefficients of the reduced IDCT (1, 4 or 16 of 64), skips the others with combined code+magnitude tables, skips the IDCT of flat blocks, and writes pixels straight into planes (1.3–1.6× faster than jpeg-decoder's own scaling; its subsampled chroma is also more faithful) |
| 1/1 | zune-jpeg (SIMD) |
| anything else (progressive, CMYK, 4:1:1, damaged data) | jpeg-decoder with DCT scaling |

`turbojpeg` was not used: it needs CMake and NASM on every build machine
(macOS/Windows CI included) and links C code; the pure-Rust path is within
reach of the target. HEIF: no permissive pure-Rust HEVC decoder exists
(libheif/libde265 are LGPL), so HEIF previews are unsupported; platform
decoders (ImageIO, WIC) would be the way to add them in the desktop shell.

## Performance

Release build, one core (`RAYON_NUM_THREADS=1 taskset -c N`), Intel Xeon
2.1 GHz shared with other builds (load ≈ 5 on 4 cores), `min_long_edge = 1440`,
minimum of 9 runs, file in page cache. `examples/bench_decode.rs`:

| File | Preview used | Output | `load_preview` |
| --- | --- | --- | --- |
| Canon EOS R6 CR3 (20 MP) | PRVW 1620×1080 | 1620×1080 | 9.9 ms |
| Sony A7 IV ARW (crop 14 MP) | 1616×1080 | 1616×1080 | 10.3 ms |
| Nikon D750 NEF (24 MP) | 1620×1080 | 1620×1080 | 13.9 ms |
| Nikon Z 6 NEF (24 MP) | 1620×1080 | 1620×1080 | 14.3 ms |
| Panasonic GH5 RW2 (20 MP) | 1920×1440 | 1920×1440 | 16.5 ms |
| Sony A7 III ARW (24 MP) | 1616×1080 | 1616×1080 | 19.1 ms |
| Olympus E-M1 II ORF (20 MP) | 3200×2400 at 1/2 | 1600×1200 | 23.5 ms |
| Canon 5D Mark IV CR2 (30 MP) | 6720×4480 at 1/4 | 1680×1120 | 44.1 ms |
| Ricoh GR III DNG (24 MP) | 6000×4000 at 1/4 | 1500×1000 | 44.7 ms |
| Fujifilm X-T3 RAF (26 MP) | 4416×2944 at 1/2 | 2208×1472 | 57.8 ms |
| Samsung NX500 SRW (28 MP) | 6480×4320 at 1/4 | 1620×1080 | 61.7 ms |
| Pentax K-1 II PEF (crop 15 MP) | 4800×3200 at 1/2 (4.5 MB JPEG) | 2400×1600 | 76.4 ms |
| Google Pixel 6 Pro DNG | 680×512 (only preview) | 680×512 | 2.1 ms |

A 24 MP full-size JPEG decoded at 1/4 takes 24–28 ms (D750/Z 6 JpgFromRaw),
vs 38–44 ms with jpeg-decoder and ~100 ms with zune-jpeg full decode + resize.
Files whose only usable preview is a large full-size JPEG (CR2, DNG, RAF,
PEF, SRW) stay above 30 ms: entropy decoding of 2–4.5 MB dominates.
Throughput with all 4 cores busy (rayon, mixed samples): ~60 previews/s.
`read_exif`: < 0.1 ms per file from cache. `scan_folder`: 10 000 files in
27 ms. `resize_long_edge` 1620×1080 → 1024: 2.8 ms (fast_image_resize,
CatmullRom) vs 28 ms with `image`. `encode_jpeg` 512 px q85: 1.5 ms
(jpeg-encoder) vs 4.4 ms with `image`.

## Dependencies (all permissive)

| Crate | License | Use |
| --- | --- | --- |
| jpeg-decoder 0.3 | MIT OR Apache-2.0 | fallback JPEG decoding with DCT scaling |
| zune-jpeg / zune-core 0.5 | MIT OR Apache-2.0 OR Zlib | full-size JPEG decoding (already used by `image`) |
| kamadak-exif 0.6 | BSD-2-Clause | EXIF of HEIF containers |
| sha1_smol 1 | BSD-3-Clause | asset ids |
| fast_image_resize 6 | MIT OR Apache-2.0 | SIMD resizing |
| jpeg-encoder 0.7 | (MIT OR Apache-2.0) AND IJG | SIMD JPEG encoding; the IJG license asks binary distributions to state in their documentation that "this software is based in part on the work of the Independent JPEG Group" |
| image 0.25 | MIT OR Apache-2.0 | PNG/TIFF, buffers |

## Tests and benchmark

```sh
scripts/fetch-samples.sh            # CC0 RAWs from raw.pixls.us → samples/raw (gitignored)
cargo test -p cull-decode           # unit tests on synthetic fixtures + real samples (skipped when absent)
cargo clippy -p cull-decode --all-targets -- -D warnings
RAYON_NUM_THREADS=1 taskset -c 0 cargo run --release -p cull-decode --example bench_decode
```

Unit tests build TIFF/NEF-like, CR3, RAF and RW2 containers in code, check
truncated and randomly corrupted inputs never panic (the third-party
decoders are also wrapped in `catch_unwind`), compare the reduced decoder
with jpeg-decoder and with a box-filtered full decode, and check scanning
(pairing, sidecars, hidden/system folders, symlink escapes, 10 000 files).
Integration tests (`tests/samples.rs`) check every sample against reference
metadata, DCT-scaling bounds, bytes read and the fast decoder's luma against
jpeg-decoder.
