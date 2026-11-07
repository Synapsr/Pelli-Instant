//! CLIP (OpenAI, MIT): image towers for embeddings, text towers for
//! natural-language search, and the LAION aesthetic linear heads. ViT-B/32
//! (512-d) in Standard precision, ViT-L/14 (768-d) in Maximum; both use the
//! same preprocessing and the same tokenizer (identical vocabulary and ids).
//!
//! Image preprocessing: the **whole** image, letterboxed. Its long side is
//! resized to 224 (bicubic, Catmull-Rom) and centred on a 224x224 canvas
//! whose bars are the CLIP mean colour (0 after normalisation); RGB scaled
//! to `[0, 1]` then normalised with the CLIP mean/std, NCHW.
//! `CLIPImageProcessor` crops the central square instead, which ignores the
//! outer thirds of a 3:2 frame: two framings that differ on the sides looked
//! identical, and a vertical portrait lost the head.
//!
//! Text: lower-cased, whitespace-collapsed, CLIP BPE (`instant-clip-
//! tokenizer`, OpenAI vocabulary), `<|startoftext|> … <|endoftext|>`
//! truncated to 77 tokens and zero-padded (the exported text model pools at
//! the first end-of-text token, the highest id).

use crate::imgproc::{resize_crop, RgbView};
use crate::runtime::{output_f32, SessionPool};
use crate::{MlError, Result};
use fast_image_resize::{FilterType, ResizeAlg};
use instant_clip_tokenizer::{Token, Tokenizer};
use ort::session::Session;
use ort::value::TensorRef;
use std::sync::Mutex;

/// Embedding size of ViT-B/32 (ViT-L/14: `L14_DIM`).
pub(crate) const EMBED_DIM: usize = 512;
pub(crate) const L14_DIM: usize = 768;
/// Revision of the image preprocessing (letterboxed whole image), part of
/// `models_id`: embeddings of different preprocessings are not comparable.
pub(crate) const PREPROCESS_TAG: &str = "lb";
const SIZE: usize = 224;
const CONTEXT: usize = 77;
const MEAN: [f32; 3] = [0.481_454_66, 0.457_827_5, 0.408_210_73];
const STD: [f32; 3] = [0.268_629_5, 0.261_302_6, 0.275_777_1];

/// L2-normalise in place; `false` when the vector is zero or not finite.
pub(crate) fn l2_normalize(v: &mut [f32]) -> bool {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if !n.is_finite() || n <= f32::EPSILON {
        return false;
    }
    v.iter_mut().for_each(|x| *x /= n);
    true
}

/// CLIP pixel tensor `[1, 3, 224, 224]` of `img`, letterboxed (see the
/// module docs).
pub(crate) fn preprocess(img: RgbView<'_>) -> Result<Vec<f32>> {
    // Long side to 224 (small images are enlarged), at least one pixel.
    let s = SIZE as f64 / img.w.max(img.h) as f64;
    let side = |v: u32| ((v as f64 * s).round() as u32).clamp(1, SIZE as u32);
    let (w, h) = (side(img.w), side(img.h));
    let full = (0.0, 0.0, img.w as f64, img.h as f64);
    let r = resize_crop(img, full, w, h, ResizeAlg::Convolution(FilterType::CatmullRom))?;
    let (ox, oy) = ((SIZE - w as usize) / 2, (SIZE - h as usize) / 2);
    let plane = SIZE * SIZE;
    // Bars: the mean colour, i.e. 0 once normalised.
    let mut out = vec![0f32; 3 * plane];
    for (y, row) in r.data.chunks_exact(3 * w as usize).take(h as usize).enumerate() {
        for (x, px) in row.chunks_exact(3).enumerate() {
            let i = (oy + y) * SIZE + ox + x;
            for c in 0..3 {
                out[c * plane + i] = (px[c] as f32 / 255.0 - MEAN[c]) / STD[c];
            }
        }
    }
    Ok(out)
}

pub(crate) struct ClipVision {
    pub pool: SessionPool,
    /// Embedding size (`EMBED_DIM` or `L14_DIM`).
    pub dim: usize,
}

impl ClipVision {
    /// L2-normalised embedding of `img` (any size: the working copy).
    pub(crate) fn embed(&self, img: RgbView<'_>) -> Result<Vec<f32>> {
        self.embed_tensor(&preprocess(img)?)
    }

    /// L2-normalised embedding of a preprocessed `[1, 3, 224, 224]` tensor.
    pub(crate) fn embed_tensor(&self, input: &[f32]) -> Result<Vec<f32>> {
        let dim = self.dim;
        let mut e = self.pool.with(|session| {
            let tensor = TensorRef::from_array_view(([1usize, 3, SIZE, SIZE], input))
                .map_err(|e| MlError::Runtime(format!("clip input: {e}")))?;
            let outputs =
                session.run(ort::inputs![tensor]).map_err(|e| MlError::Runtime(format!("clip vision: {e}")))?;
            Ok(output_f32(&outputs, "image_embeds", dim)?[..dim].to_vec())
        })?;
        if !l2_normalize(&mut e) {
            return Err(MlError::Runtime("clip vision: degenerate embedding".into()));
        }
        Ok(e)
    }
}

pub(crate) struct ClipText {
    pub session: Mutex<Session>,
    pub tokenizer: Tokenizer,
    /// Embedding size (`EMBED_DIM` or `L14_DIM`).
    pub dim: usize,
}

/// Token ids (`[CONTEXT]`, zero padded) of `text`.
pub(crate) fn tokenize(tokenizer: &Tokenizer, text: &str) -> Vec<i64> {
    // The tokenizer maps these literal strings to special tokens: drop them.
    let cleaned = text.replace("<start_of_text>", " ").replace("<end_of_text>", " ");
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut tokens: Vec<Token> = Vec::with_capacity(CONTEXT);
    tokenizer.encode(&cleaned, &mut tokens);
    tokens.truncate(CONTEXT - 2);
    let mut ids = Vec::with_capacity(CONTEXT);
    ids.push(i64::from(tokenizer.start_of_text().to_u16()));
    ids.extend(tokens.into_iter().map(|t| i64::from(t.to_u16())));
    ids.push(i64::from(tokenizer.end_of_text().to_u16()));
    ids.resize(CONTEXT, 0);
    ids
}

impl ClipText {
    pub(crate) fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.embed_ids(&tokenize(&self.tokenizer, text))
    }

    /// L2-normalised embedding of `CONTEXT` token ids.
    pub(crate) fn embed_ids(&self, ids: &[i64]) -> Result<Vec<f32>> {
        let mut session = self.session.lock().unwrap_or_else(|e| e.into_inner());
        let tensor = TensorRef::from_array_view(([1usize, CONTEXT], ids))
            .map_err(|e| MlError::Runtime(format!("clip text input: {e}")))?;
        let outputs = session.run(ort::inputs![tensor]).map_err(|e| MlError::Runtime(format!("clip text: {e}")))?;
        let mut e = output_f32(&outputs, "text_embeds", self.dim)?[..self.dim].to_vec();
        if !l2_normalize(&mut e) {
            return Err(MlError::Runtime("clip text: degenerate embedding".into()));
        }
        Ok(e)
    }
}

/// LAION-Aesthetics Predictor V1 linear head for ViT-B/32 (MIT): 512 weights
/// then the bias, little-endian f32.
static AESTHETIC_HEAD: &[u8; 2052] = include_bytes!("../assets/laion_aesthetic_linear_vit_b_32.f32");

/// LAION-Aesthetics Predictor V2 head for ViT-L/14 (Apache-2.0): its MLP
/// has no activation, so it collapses exactly into one linear map (768
/// weights then the bias, little-endian f32; see `models/manifest.json`).
static AESTHETIC_HEAD_L14: &[u8; 3076] = include_bytes!("../assets/laion_aesthetic_v2_linear_vit_l_14.f32");

fn linear_head(head: &[u8], embedding: &[f32]) -> Option<f32> {
    let dim = head.len() / 4 - 1;
    if embedding.len() != dim {
        return None;
    }
    let mut vals = head.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let dot: f32 = embedding.iter().zip(vals.by_ref().take(dim)).map(|(e, w)| e * w).sum();
    let bias = vals.next()?;
    let s = dot + bias;
    s.is_finite().then_some(s)
}

/// Aesthetic score (LAION scale, ~1..10) of an L2-normalised ViT-B/32 image
/// embedding.
pub(crate) fn aesthetic(embedding: &[f32]) -> Option<f32> {
    linear_head(AESTHETIC_HEAD, embedding)
}

/// The V2 head's scores spread differently from V1's (on the 570 photos of
/// the four calibration shoots: median 4.64, MAD 0.25 against 4.33 and 0.39;
/// rank correlation 0.20 between the two): mapped linearly onto the V1
/// scale (median and MAD matched) so the decision constants hold for both
/// precisions.
pub(crate) const L14_TO_V1_SCALE: f32 = 1.552;
pub(crate) const L14_TO_V1_OFFSET: f32 = -2.878;

/// Aesthetic score of an L2-normalised ViT-L/14 image embedding (LAION V2
/// head), on the V1 head's scale.
pub(crate) fn aesthetic_l14(embedding: &[f32]) -> Option<f32> {
    linear_head(AESTHETIC_HEAD_L14, embedding).map(|s| L14_TO_V1_OFFSET + L14_TO_V1_SCALE * s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer_matches_openai_reference() {
        let t = Tokenizer::new();
        let ids = tokenize(&t, "  A person   riding a motorcycle ");
        assert_eq!(&ids[..7], &[49406, 320, 2533, 6765, 320, 10297, 49407]);
        assert_eq!(ids.len(), CONTEXT);
        assert!(ids[7..].iter().all(|&i| i == 0));
        // Very long input is truncated, keeping the end-of-text token.
        let long = "wedding ".repeat(200);
        let ids = tokenize(&t, &long);
        assert_eq!(ids.len(), CONTEXT);
        assert_eq!(ids[CONTEXT - 1], 49407);
        // Empty / special strings never panic.
        assert_eq!(&tokenize(&t, "")[..2], &[49406, 49407]);
        assert_eq!(&tokenize(&t, "<end_of_text>")[..2], &[49406, 49407]);
    }

    #[test]
    fn aesthetic_head_is_sane() {
        // Weights norm ~27.6 and bias ~3.42 (checked against the .pth).
        let w: Vec<f32> =
            AESTHETIC_HEAD.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        assert_eq!(w.len(), 513);
        let norm = w[..512].iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 27.57).abs() < 0.01, "{norm}");
        assert!((w[512] - 3.4215).abs() < 1e-3);
        assert!(aesthetic(&[0.0; 3]).is_none());
        assert!((aesthetic(&[0.0; 512]).unwrap() - w[512]).abs() < 1e-6);
        // V2 for L/14: weights norm 17.9956, bias 5.236646 (model-prep report).
        let w: Vec<f32> =
            AESTHETIC_HEAD_L14.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        assert_eq!(w.len(), 769);
        let norm = w[..768].iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 17.9956).abs() < 1e-3, "{norm}");
        assert!((w[768] - 5.236646).abs() < 1e-5);
        assert!(aesthetic_l14(&[0.0; 512]).is_none());
    }

    #[test]
    fn whole_image_is_letterboxed() {
        // 300x200, left third red, rest blue: the red third must be seen
        // (a central square crop would drop it), bars are 0.
        let (w, h) = (300u32, 200u32);
        let data: Vec<u8> =
            (0..w * h).flat_map(|i| if (i % w) < w / 3 { [255u8, 0, 0] } else { [0, 0, 255] }).collect();
        let t = preprocess(RgbView::new(w, h, &data).unwrap()).unwrap();
        let plane = SIZE * SIZE;
        assert_eq!(t.len(), 3 * plane);
        let (top, mid) = (5 * SIZE + 112, 112 * SIZE);
        assert!(t[top].abs() < 1e-6 && t[plane + top].abs() < 1e-6, "bar is the mean colour");
        let red = (255.0 / 255.0 - MEAN[0]) / STD[0];
        assert!((t[mid + 10] - red).abs() < 0.05, "left third kept: {}", t[mid + 10]);
        // Tiny and very elongated images do not panic.
        let one = [9u8; 3];
        assert_eq!(preprocess(RgbView::new(1, 1, &one).unwrap()).unwrap().len(), 3 * plane);
        let strip = vec![9u8; 3 * 5000];
        assert_eq!(preprocess(RgbView::new(5000, 1, &strip).unwrap()).unwrap().len(), 3 * plane);
    }

    #[test]
    fn normalize() {
        let mut v = vec![3.0, 4.0];
        assert!(l2_normalize(&mut v));
        assert!((v[0] - 0.6).abs() < 1e-6);
        let mut z = vec![0.0; 4];
        assert!(!l2_normalize(&mut z));
        let mut n = vec![f32::NAN, 1.0];
        assert!(!l2_normalize(&mut n));
    }
}
