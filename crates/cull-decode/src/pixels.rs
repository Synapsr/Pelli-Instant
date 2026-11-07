//! Pixel operations: EXIF orientation, resizing, JPEG encoding.

use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::RgbImage;

/// Applies an EXIF orientation (1..=8) so the image is upright.
pub(crate) fn orient(img: RgbImage, orientation: u8) -> RgbImage {
    use image::imageops::{flip_horizontal_in_place, flip_vertical_in_place, rotate180_in_place};
    let mut img = img;
    match orientation {
        2 => flip_horizontal_in_place(&mut img),
        3 => rotate180_in_place(&mut img),
        4 => flip_vertical_in_place(&mut img),
        5 => return transpose(&img, false, false),
        6 => return transpose(&img, true, false),
        7 => return transpose(&img, true, true),
        8 => return transpose(&img, false, true),
        _ => {}
    }
    img
}

/// Transposition with optional mirroring of the output axes, which covers
/// the four EXIF orientations that swap width and height:
/// 5 = transpose, 6 = rotate 90° CW, 7 = transverse, 8 = rotate 90° CCW.
fn transpose(src: &RgbImage, mirror_x: bool, mirror_y: bool) -> RgbImage {
    let (w, h) = (src.width() as usize, src.height() as usize);
    let s = src.as_raw();
    let mut out = vec![0u8; w * h * 3];
    // Output is h wide and w tall; output pixel (X, Y) = source (Y, X)
    // before mirroring. Work in tiles to stay cache friendly.
    const T: usize = 32;
    for ty in (0..h).step_by(T) {
        for tx in (0..w).step_by(T) {
            for y in ty..(ty + T).min(h) {
                let row = &s[y * w * 3..(y + 1) * w * 3];
                let ox = if mirror_x { h - 1 - y } else { y };
                for x in tx..(tx + T).min(w) {
                    let oy = if mirror_y { w - 1 - x } else { x };
                    let o = (oy * h + ox) * 3;
                    out[o..o + 3].copy_from_slice(&row[x * 3..x * 3 + 3]);
                }
            }
        }
    }
    // Sizes come from an existing image: the buffer length always matches.
    RgbImage::from_raw(h as u32, w as u32, out).unwrap_or_else(|| RgbImage::new(h as u32, w as u32))
}

/// Size with the long edge set to `long_edge`, aspect ratio kept.
pub(crate) fn fit_long_edge(w: u32, h: u32, long_edge: u32) -> (u32, u32) {
    let long = w.max(h).max(1);
    let scale = long_edge as f64 / long as f64;
    (((w as f64 * scale).round() as u32).max(1), ((h as f64 * scale).round() as u32).max(1))
}

/// Resizes to exactly `nw`×`nh` with a SIMD convolution filter.
pub(crate) fn resize_exact(img: &RgbImage, nw: u32, nh: u32, filter: FilterType) -> RgbImage {
    let (w, h) = img.dimensions();
    if (w, h) == (nw, nh) {
        return img.clone();
    }
    let fast = || -> Option<RgbImage> {
        let src = ImageRef::new(w, h, img.as_raw(), PixelType::U8x3).ok()?;
        let mut dst = Image::new(nw, nh, PixelType::U8x3);
        let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(filter));
        Resizer::new().resize(&src, &mut dst, &opts).ok()?;
        RgbImage::from_raw(nw, nh, dst.into_vec())
    };
    fast().unwrap_or_else(|| image::imageops::resize(img, nw, nh, image::imageops::FilterType::CatmullRom))
}

/// Halves the image (box filter) while the long edge stays >= `min_long_edge`:
/// the PNG/TIFF/RGB counterpart of JPEG DCT scaling.
pub(crate) fn reduce_pow2(img: RgbImage, min_long_edge: u32) -> RgbImage {
    let (w, h) = img.dimensions();
    let denom = crate::jpeg::scale_denominator(w.max(h), min_long_edge);
    if denom == 1 {
        return img;
    }
    let (nw, nh) = crate::jpeg::scaled_size(w, h, denom);
    resize_exact(&img, nw, nh, FilterType::Box)
}

/// Baseline JPEG encoding (SIMD encoder, `image` as fallback).
pub(crate) fn encode_jpeg(img: &RgbImage, quality: u8) -> Vec<u8> {
    let (w, h) = img.dimensions();
    let quality = quality.clamp(1, 100);
    if let (Ok(w16), Ok(h16)) = (u16::try_from(w), u16::try_from(h)) {
        let mut out = Vec::with_capacity((w as usize * h as usize) / 4);
        let enc = jpeg_encoder::Encoder::new(&mut out, quality);
        if enc.encode(img.as_raw(), w16, h16, jpeg_encoder::ColorType::Rgb).is_ok() {
            return out;
        }
    }
    let mut out = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
    // Writing to a Vec cannot fail for a valid RGB8 buffer.
    let _ = enc.encode(img.as_raw(), w, h, image::ExtendedColorType::Rgb8);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// 3×2 image with distinct pixels.
    fn sample() -> RgbImage {
        RgbImage::from_fn(3, 2, |x, y| Rgb([(x + 3 * y) as u8, 0, 0]))
    }

    fn values(img: &RgbImage) -> Vec<Vec<u8>> {
        (0..img.height()).map(|y| (0..img.width()).map(|x| img.get_pixel(x, y)[0]).collect()).collect()
    }

    #[test]
    fn orientations() {
        // Source:  0 1 2
        //          3 4 5
        assert_eq!(values(&orient(sample(), 1)), vec![vec![0, 1, 2], vec![3, 4, 5]]);
        assert_eq!(values(&orient(sample(), 2)), vec![vec![2, 1, 0], vec![5, 4, 3]]);
        assert_eq!(values(&orient(sample(), 3)), vec![vec![5, 4, 3], vec![2, 1, 0]]);
        assert_eq!(values(&orient(sample(), 4)), vec![vec![3, 4, 5], vec![0, 1, 2]]);
        assert_eq!(values(&orient(sample(), 5)), vec![vec![0, 3], vec![1, 4], vec![2, 5]]);
        assert_eq!(values(&orient(sample(), 6)), vec![vec![3, 0], vec![4, 1], vec![5, 2]]);
        assert_eq!(values(&orient(sample(), 7)), vec![vec![5, 2], vec![4, 1], vec![3, 0]]);
        assert_eq!(values(&orient(sample(), 8)), vec![vec![2, 5], vec![1, 4], vec![0, 3]]);
        // Same results as `image`'s own implementation.
        let big = crate::jpeg::test_util::pattern(70, 45);
        assert_eq!(orient(big.clone(), 6), image::imageops::rotate90(&big));
        assert_eq!(orient(big.clone(), 8), image::imageops::rotate270(&big));
    }

    #[test]
    fn resize_and_reduce() {
        let img = crate::jpeg::test_util::pattern(400, 300);
        assert_eq!(fit_long_edge(400, 300, 100), (100, 75));
        assert_eq!(resize_exact(&img, 100, 75, FilterType::CatmullRom).dimensions(), (100, 75));
        assert_eq!(reduce_pow2(img.clone(), 100).dimensions(), (100, 75));
        assert_eq!(reduce_pow2(img.clone(), 101).dimensions(), (200, 150));
        assert_eq!(reduce_pow2(img, 1000).dimensions(), (400, 300));
    }

    #[test]
    fn encodes() {
        let img = crate::jpeg::test_util::pattern(100, 60);
        let j = encode_jpeg(&img, 85);
        let h = crate::jpeg::read_header_mem(&j).unwrap();
        assert_eq!((h.width, h.height), (100, 60));
        // Too large for JPEG: falls back without panicking (empty output).
        assert!(encode_jpeg(&RgbImage::new(70_000, 1), 80).len() < 10);
    }
}
