//! A specialised baseline JPEG decoder for *reduced-size* output.
//!
//! Decoding a 24 MP camera JPEG to a 1/4 or 1/8 thumbnail is dominated by
//! entropy decoding. General decoders (jpeg-decoder included) decode all 64
//! coefficients of every block and then run a small IDCT; this decoder only
//! materialises the coefficients the reduced IDCT uses (1, 4 or 16 of 64)
//! and skips the others with combined Huffman+magnitude lookup tables,
//! writing each block's pixels straight into component planes.
//!
//! Scope: baseline/extended sequential Huffman (SOF0/SOF1), 8-bit, one
//! interleaved scan, 1 or 3 components with power-of-two chroma subsampling,
//! restart markers. Anything else returns an error and the caller falls back
//! to jpeg-decoder. Reduced IDCTs follow jpeg-decoder (top-left N×N
//! coefficients, N-point IDCT), so output quality is the same.
//!
//! Hostile input never panics: every table index is masked, plane writes
//! are bounds-checked by construction and bit reads past the data yield
//! zeros.

use image::RgbImage;

use crate::fault::{FResult, Fault};

/// Bits resolved by the fast Huffman lookup.
const FAST_BITS: u32 = 10;
const FAST_SIZE: usize = 1 << FAST_BITS;
/// Table-resolved symbols (at most `FAST_BITS` bits each) that fit in the 32
/// bits `Bits::ensure` guarantees.
const SKIP_BATCH: u32 = 32 / FAST_BITS;
/// Largest image accepted (pixels): beyond, let jpeg-decoder's limits act.
const MAX_PIXELS: u64 = 400_000_000;

/// Natural (row-major) index of each zig-zag position.
const UNZIGZAG: [u8; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21,
    28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54,
    47, 55, 62, 63,
];

/// Error of the entropy-decoding hot path (zero-sized: keeps `Result`s in
/// registers); converted to a [`Fault`] once per image.
#[derive(Debug)]
struct BadData;

impl From<BadData> for Fault {
    fn from(_: BadData) -> Fault {
        Fault::corrupt("corrupt JPEG entropy-coded data")
    }
}

fn unsupported(what: &str) -> Fault {
    Fault::Unsupported(format!("fast JPEG path: {what}"))
}

/// Canonical Huffman table with a fast lookup.
struct Huff {
    /// `len << 8 | symbol` for codes of at most `FAST_BITS` bits, 0 otherwise.
    fast: Vec<u16>,
    /// AC only: for codes whose length plus magnitude bits fit in
    /// `FAST_BITS`: `value << 8 | run << 4 | total_bits` (0 = not cached).
    fast_ac: Vec<i32>,
    /// Largest code of each length (-1 if none), and value index offset.
    maxcode: [i32; 18],
    valoffset: [i32; 17],
    values: [u8; 256],
}

impl Huff {
    fn new(counts: &[u8; 16], symbols: &[u8], is_ac: bool) -> FResult<Huff> {
        let total: usize = counts.iter().map(|&c| c as usize).sum();
        if total > 256 || symbols.len() < total {
            return Err(Fault::corrupt("bad Huffman table"));
        }
        let mut h = Huff {
            fast: vec![0; FAST_SIZE],
            fast_ac: if is_ac { vec![0; FAST_SIZE] } else { Vec::new() },
            maxcode: [-1; 18],
            valoffset: [0; 17],
            values: [0; 256],
        };
        h.values[..total].copy_from_slice(&symbols[..total]);
        let mut code: u32 = 0;
        let mut k = 0usize;
        for len in 1..=16u32 {
            let n = counts[len as usize - 1] as usize;
            h.valoffset[len as usize] = k as i32 - code as i32;
            for _ in 0..n {
                if len <= FAST_BITS {
                    let shift = FAST_BITS - len;
                    let start = (code << shift) as usize;
                    for e in start..start + (1usize << shift) {
                        if let Some(slot) = h.fast.get_mut(e) {
                            *slot = ((len as u16) << 8) | h.values[k] as u16;
                        }
                    }
                }
                code += 1;
                k += 1;
            }
            if n > 0 {
                h.maxcode[len as usize] = code as i32 - 1;
            }
            if code > (1 << len) {
                return Err(Fault::corrupt("over-subscribed Huffman table"));
            }
            code <<= 1;
        }
        h.maxcode[17] = i32::MAX;
        if is_ac {
            for i in 0..FAST_SIZE {
                let e = h.fast[i];
                if e == 0 {
                    continue;
                }
                let len = (e >> 8) as u32;
                let rs = (e & 0xFF) as u32;
                let (run, size) = (rs >> 4, rs & 15);
                if size == 0 || len + size > FAST_BITS {
                    continue;
                }
                // Magnitude bits follow the code inside the lookup window.
                let bits = (i as u32 >> (FAST_BITS - len - size)) & ((1 << size) - 1);
                let value = extend(bits, size);
                h.fast_ac[i] = (value << 8) | ((run as i32) << 4) | (len + size) as i32;
            }
        }
        Ok(h)
    }
}

#[inline(always)]
fn extend(v: u32, s: u32) -> i32 {
    if s == 0 {
        0
    } else if v < (1 << (s - 1)) {
        v as i32 - (1 << s) + 1
    } else {
        v as i32
    }
}

/// MSB-first bit reader over entropy-coded data with byte unstuffing.
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    acc: u64,
    n: u32,
    /// A marker (or the end of data) was reached: zeros are fed from now on.
    stopped: bool,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8], pos: usize) -> Bits<'a> {
        Bits { data, pos, acc: 0, n: 0, stopped: false }
    }

    #[inline(always)]
    fn refill(&mut self) {
        // Fast path: 8 bytes without 0xFF can be appended wholesale.
        if !self.stopped {
            if let Some(chunk) = self.data.get(self.pos..self.pos + 8) {
                let mut a = [0u8; 8];
                a.copy_from_slice(chunk);
                let w = u64::from_be_bytes(a);
                if !has_ff_byte(w) {
                    let take = (63 - self.n) / 8; // whole bytes that fit
                    if take > 0 {
                        let bits = take * 8;
                        self.acc |= (w >> (64 - bits)) << (64 - self.n - bits);
                        self.n += bits;
                        self.pos += take as usize;
                    }
                    return;
                }
            }
        }
        while self.n <= 56 {
            let byte = if self.stopped {
                0
            } else {
                match self.data.get(self.pos) {
                    Some(&0xFF) => match self.data.get(self.pos + 1) {
                        Some(&0x00) => {
                            self.pos += 2;
                            0xFF
                        }
                        _ => {
                            self.stopped = true;
                            0
                        }
                    },
                    Some(&b) => {
                        self.pos += 1;
                        b
                    }
                    None => {
                        self.stopped = true;
                        0
                    }
                }
            };
            self.acc |= (byte as u64) << (56 - self.n);
            self.n += 8;
        }
    }

    #[inline(always)]
    fn ensure(&mut self) {
        if self.n < 32 {
            self.refill();
        }
    }

    #[inline(always)]
    fn peek(&self, k: u32) -> u32 {
        (self.acc >> (64 - k)) as u32
    }

    #[inline(always)]
    fn consume(&mut self, k: u32) {
        self.acc <<= k;
        self.n -= k;
    }

    /// `s` magnitude bits, sign-extended (requires `ensure` beforehand).
    #[inline(always)]
    fn receive_extend(&mut self, s: u32) -> i32 {
        if s == 0 {
            return 0;
        }
        let v = self.peek(s);
        self.consume(s);
        extend(v, s)
    }

    /// One Huffman symbol (requires `ensure` beforehand).
    #[inline(always)]
    fn symbol(&mut self, h: &Huff) -> Result<u32, BadData> {
        let e = h.fast[(self.peek(FAST_BITS) as usize) & (FAST_SIZE - 1)];
        if e != 0 {
            self.consume((e >> 8) as u32);
            return Ok((e & 0xFF) as u32);
        }
        let look = self.peek(16) as i32;
        for len in FAST_BITS + 1..=16 {
            let code = look >> (16 - len);
            if code <= h.maxcode[len as usize] {
                self.consume(len);
                let idx = (code + h.valoffset[len as usize]) as usize;
                return Ok(h.values[idx & 0xFF] as u32);
            }
        }
        Err(BadData)
    }

    /// Resynchronises on a restart marker.
    fn restart(&mut self) {
        self.acc = 0;
        self.n = 0;
        self.stopped = false;
        // Skip to the RSTn marker (normally right here).
        while let Some(&b) = self.data.get(self.pos) {
            if b == 0xFF {
                match self.data.get(self.pos + 1) {
                    Some(m) if (0xD0..=0xD7).contains(m) => {
                        self.pos += 2;
                        return;
                    }
                    Some(0xFF) => self.pos += 1,
                    Some(0x00) => self.pos += 2,
                    // Another marker (EOI…): stop here.
                    _ => return,
                }
            } else {
                self.pos += 1;
            }
        }
    }
}

#[inline(always)]
fn has_ff_byte(w: u64) -> bool {
    // A byte is 0xFF iff the same byte of !w is zero.
    let x = !w;
    (x.wrapping_sub(0x0101_0101_0101_0101) & !x & 0x8080_8080_8080_8080) != 0
}

#[derive(Clone)]
struct Comp {
    id: u8,
    h: usize,
    v: usize,
    tq: usize,
    td: usize,
    ta: usize,
}

/// Decodes `data` to 1/`denom` size (`denom` ∈ {2, 4, 8}).
pub(crate) fn decode_reduced(data: &[u8], denom: u32) -> FResult<RgbImage> {
    let bs = match denom {
        2 => 4usize,
        4 => 2,
        8 => 1,
        _ => return Err(unsupported("scale")),
    };
    if data.get(0..2) != Some(&[0xFF, 0xD8]) {
        return Err(Fault::corrupt("missing SOI"));
    }
    let mut quant = [[0i32; 64]; 4];
    let mut dc: [Option<Huff>; 4] = [None, None, None, None];
    let mut ac: [Option<Huff>; 4] = [None, None, None, None];
    let mut comps: Vec<Comp> = Vec::new();
    let (mut width, mut height) = (0usize, 0usize);
    let mut restart = 0usize;
    let mut pos = 2usize;
    let be16 = |p: usize| -> FResult<usize> {
        data.get(p..p + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]) as usize)
            .ok_or_else(|| Fault::corrupt("truncated JPEG header"))
    };

    // Markers up to the (single) scan.
    let scan_start = loop {
        let Some(&ff) = data.get(pos) else { return Err(Fault::corrupt("no scan")) };
        if ff != 0xFF {
            return Err(Fault::corrupt("bad marker"));
        }
        let m = *data.get(pos + 1).ok_or_else(|| Fault::corrupt("truncated JPEG header"))?;
        if m == 0xFF {
            pos += 1;
            continue;
        }
        if (0xD0..=0xD7).contains(&m) || m == 0x01 {
            pos += 2;
            continue;
        }
        let len = be16(pos + 2)?;
        let seg = data.get(pos + 4..pos + 2 + len).ok_or_else(|| Fault::corrupt("truncated JPEG segment"))?;
        if len < 2 {
            return Err(Fault::corrupt("bad segment length"));
        }
        match m {
            0xDB => {
                let mut i = 0;
                while i < seg.len() {
                    let (pq, tq) = (seg[i] >> 4, (seg[i] & 3) as usize);
                    i += 1;
                    for (k, q) in quant[tq].iter_mut().enumerate() {
                        *q = if pq == 0 {
                            *seg.get(i + k).ok_or_else(|| Fault::corrupt("short DQT"))? as i32
                        } else {
                            let b = seg.get(i + 2 * k..i + 2 * k + 2).ok_or_else(|| Fault::corrupt("short DQT"))?;
                            u16::from_be_bytes([b[0], b[1]]) as i32
                        }; // zig-zag order
                    }
                    i += if pq == 0 { 64 } else { 128 };
                }
            }
            0xC4 => {
                let mut i = 0;
                while i + 17 <= seg.len() {
                    let (tc, th) = (seg[i] >> 4, (seg[i] & 3) as usize);
                    let mut counts = [0u8; 16];
                    counts.copy_from_slice(&seg[i + 1..i + 17]);
                    let total: usize = counts.iter().map(|&c| c as usize).sum();
                    let syms = seg.get(i + 17..i + 17 + total).ok_or_else(|| Fault::corrupt("short DHT"))?;
                    let table = Huff::new(&counts, syms, tc == 1)?;
                    match tc {
                        0 => dc[th] = Some(table),
                        1 => ac[th] = Some(table),
                        _ => return Err(Fault::corrupt("bad DHT class")),
                    }
                    i += 17 + total;
                }
            }
            0xC0 | 0xC1 => {
                if seg.len() < 6 || seg[0] != 8 {
                    return Err(unsupported("precision"));
                }
                height = u16::from_be_bytes([seg[1], seg[2]]) as usize;
                width = u16::from_be_bytes([seg[3], seg[4]]) as usize;
                let n = seg[5] as usize;
                if !(n == 1 || n == 3) || seg.len() < 6 + 3 * n {
                    return Err(unsupported("component count"));
                }
                for c in 0..n {
                    let b = &seg[6 + 3 * c..9 + 3 * c];
                    comps.push(Comp {
                        id: b[0],
                        h: (b[1] >> 4) as usize,
                        v: (b[1] & 15) as usize,
                        tq: (b[2] & 3) as usize,
                        td: 0,
                        ta: 0,
                    });
                }
            }
            0xC2 | 0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => return Err(unsupported("coding process")),
            0xDD => restart = be16(pos + 4)?,
            // Adobe APP14 with transform 0: RGB or CMYK samples, not YCbCr.
            0xEE if seg.starts_with(b"Adobe") && seg.get(11) == Some(&0) => {
                return Err(unsupported("Adobe RGB/CMYK colour transform"));
            }
            0xDA => {
                let n = *seg.first().ok_or_else(|| Fault::corrupt("short SOS"))? as usize;
                if comps.is_empty() || n != comps.len() || seg.len() < 1 + 2 * n {
                    return Err(unsupported("non-interleaved scan"));
                }
                for k in 0..n {
                    let id = seg[1 + 2 * k];
                    let t = seg[2 + 2 * k];
                    let c = comps
                        .iter_mut()
                        .find(|c| c.id == id)
                        .ok_or_else(|| Fault::corrupt("unknown scan component"))?;
                    c.td = (t >> 4) as usize & 3;
                    c.ta = (t & 15) as usize & 3;
                }
                break pos + 2 + len;
            }
            0xD9 => return Err(Fault::corrupt("EOI before scan")),
            _ => {}
        }
        pos += 2 + len;
    };

    if width == 0 || height == 0 || comps.is_empty() {
        return Err(Fault::corrupt("no frame"));
    }
    if (width as u64) * (height as u64) > MAX_PIXELS {
        return Err(unsupported("size"));
    }
    let hmax = comps.iter().map(|c| c.h).max().unwrap_or(1);
    let vmax = comps.iter().map(|c| c.v).max().unwrap_or(1);
    // Power-of-two subsampling ratios with luma at full resolution.
    for c in &comps {
        let ok = |f: usize, max: usize| f >= 1 && max.is_multiple_of(f) && (max / f).is_power_of_two() && max / f <= 4;
        if !ok(c.h, hmax) || !ok(c.v, vmax) {
            return Err(unsupported("sampling factors"));
        }
    }
    if comps.len() == 3 && comps.iter().map(|c| c.id).eq(*b"RGB") {
        return Err(unsupported("RGB components"));
    }
    if comps.len() == 3 && (comps[0].h != hmax || comps[0].v != vmax) {
        return Err(unsupported("luma not at full resolution"));
    }
    let single = comps.len() == 1;
    // A single-component scan is non-interleaved: MCU = one block.
    let (mcu_w, mcu_h) = if single { (8, 8) } else { (8 * hmax, 8 * vmax) };
    let mcus_x = width.div_ceil(mcu_w);
    let mcus_y = height.div_ceil(mcu_h);

    let mut planes = Vec::with_capacity(comps.len());
    for c in &comps {
        let (Some(dct), Some(act)) = (dc[c.td].as_ref(), ac[c.ta].as_ref()) else {
            return Err(Fault::corrupt("missing Huffman table"));
        };
        let (h, v) = if single { (1, 1) } else { (c.h, c.v) };
        let stride = mcus_x * h * bs;
        planes.push(Plane {
            dc: dct,
            ac: act,
            q: quant[c.tq],
            h,
            v,
            stride,
            data: vec![0u8; stride * mcus_y * v * bs],
            pred: 0,
        });
    }

    let mut bits = Bits::new(data, scan_start);
    let geometry = Geometry { mcus_x, mcus_y, restart };
    match bs {
        1 => scan::<1>(&mut bits, &mut planes, &geometry)?,
        2 => scan::<2>(&mut bits, &mut planes, &geometry)?,
        _ => scan::<4>(&mut bits, &mut planes, &geometry)?,
    };

    // Upsample chroma (nearest) and convert to RGB.
    let ow = width.div_ceil(8 / bs);
    let oh = height.div_ceil(8 / bs);
    let mut rgb = vec![0u8; ow * oh * 3];
    if single {
        let p = &planes[0];
        for (y, out) in rgb.chunks_exact_mut(ow * 3).enumerate() {
            let row = p.data.get(y * p.stride..y * p.stride + ow).unwrap_or_default();
            for (o, &l) in out.chunks_exact_mut(3).zip(row) {
                o.copy_from_slice(&[l, l, l]);
            }
        }
    } else {
        let shift = |f: usize, max: usize| (max / f).trailing_zeros();
        let (sx, sy) = (shift(comps[1].h, hmax), shift(comps[1].v, vmax));
        if (sx, sy) != (shift(comps[2].h, hmax), shift(comps[2].v, vmax)) || sx > 1 {
            return Err(unsupported("chroma sampling"));
        }
        let (py, pb, pr) = (&planes[0], &planes[1], &planes[2]);
        let mut cb_up = vec![0u8; ow];
        let mut cr_up = vec![0u8; ow];
        for (y, out) in rgb.chunks_exact_mut(ow * 3).enumerate() {
            let yrow = py.data.get(y * py.stride..y * py.stride + ow).unwrap_or_default();
            let cy = y >> sy;
            let cb = pb.data.get(cy * pb.stride..(cy + 1) * pb.stride).unwrap_or_default();
            let cr = pr.data.get(cy * pr.stride..(cy + 1) * pr.stride).unwrap_or_default();
            if sx == 0 {
                ycc_to_rgb(out, yrow, cb, cr);
            } else {
                // 2:1 horizontal: one chroma sample per two pixels.
                upsample2(&mut cb_up, cb);
                upsample2(&mut cr_up, cr);
                ycc_to_rgb(out, yrow, &cb_up, &cr_up);
            }
        }
    }
    RgbImage::from_raw(ow as u32, oh as u32, rgb).ok_or_else(|| Fault::corrupt("output size"))
}

/// One component being decoded: tables and its output plane.
struct Plane<'h> {
    dc: &'h Huff,
    ac: &'h Huff,
    /// Quantisation table, zig-zag order.
    q: [i32; 64],
    /// Blocks per MCU, horizontally and vertically.
    h: usize,
    v: usize,
    stride: usize,
    data: Vec<u8>,
    pred: i32,
}

struct Geometry {
    mcus_x: usize,
    mcus_y: usize,
    restart: usize,
}

/// Where each kept zig-zag coefficient goes in the 4×4 working array (255 =
/// not kept), and how many zig-zag positions to keep, for `BS`×`BS` output.
const fn slots<const BS: usize>() -> ([u8; 64], usize) {
    let need = match BS {
        1 => 1,
        2 => 5,
        _ => 25,
    };
    let mut slot = [255u8; 64];
    let mut k = 0;
    while k < need {
        let nat = UNZIGZAG[k] as usize;
        let (r, c) = (nat / 8, nat % 8);
        if r < BS && c < BS {
            slot[k] = (r * 4 + c) as u8;
        }
        k += 1;
    }
    (slot, need)
}

/// Entropy-decodes the scan, writing `BS`×`BS` pixels per block.
fn scan<const BS: usize>(bits: &mut Bits, planes: &mut [Plane], g: &Geometry) -> Result<(), BadData> {
    let (slot, need) = slots::<BS>();
    let total = g.mcus_x * g.mcus_y;
    let mut done = 0usize;
    for my in 0..g.mcus_y {
        for mx in 0..g.mcus_x {
            for p in planes.iter_mut() {
                for by in 0..p.v {
                    let row = (my * p.v + by) * BS * p.stride;
                    for bx in 0..p.h {
                        let mut coef = [0i32; 16];
                        let has_ac = decode_block(bits, p.dc, p.ac, &mut p.pred, &p.q, &mut coef, need, &slot)?;
                        let at = row + (mx * p.h + bx) * BS;
                        if BS == 1 || !has_ac {
                            // Flat block: every output pixel is the DC level
                            // (what both reduced IDCTs give without AC terms).
                            let v = clamp((coef[0].wrapping_add(1024 + 4)) >> 3);
                            for r in 0..BS {
                                if let Some(px) = p.data.get_mut(at + r * p.stride..at + r * p.stride + BS) {
                                    px.fill(v);
                                }
                            }
                        } else if BS == 2 {
                            idct_2x2(&coef, &mut p.data, at, p.stride);
                        } else {
                            idct_4x4(&coef, &mut p.data, at, p.stride);
                        }
                    }
                }
            }
            done += 1;
            if g.restart > 0 && done.is_multiple_of(g.restart) && done < total {
                bits.restart();
                planes.iter_mut().for_each(|p| p.pred = 0);
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn decode_block(
    bits: &mut Bits,
    dc: &Huff,
    ac: &Huff,
    pred: &mut i32,
    q: &[i32; 64],
    coef: &mut [i32; 16],
    need: usize,
    slot: &[u8; 64],
) -> Result<bool, BadData> {
    let mut any_ac = 0i32;
    bits.ensure();
    let s = bits.symbol(dc)?;
    if s > 11 {
        return Err(BadData);
    }
    bits.ensure();
    *pred = pred.wrapping_add(bits.receive_extend(s));
    coef[0] = pred.wrapping_mul(q[0]);

    let mut k = 1usize;
    // Coefficients we keep.
    while k < need {
        bits.ensure();
        let f = ac.fast_ac[(bits.peek(FAST_BITS) as usize) & (FAST_SIZE - 1)];
        if f != 0 {
            bits.consume((f & 15) as u32);
            k += ((f >> 4) & 15) as usize;
            if k > 63 {
                return Err(BadData);
            }
            if k < need && slot[k] != 255 {
                coef[(slot[k] & 15) as usize] = (f >> 8).wrapping_mul(q[k]);
                any_ac |= f >> 8;
            }
            k += 1;
            continue;
        }
        let rs = bits.symbol(ac)?;
        let (r, s) = ((rs >> 4) as usize, rs & 15);
        if s == 0 {
            if r == 15 {
                k += 16;
                continue;
            }
            return Ok(any_ac != 0); // EOB
        }
        k += r;
        if k > 63 {
            return Err(BadData);
        }
        bits.ensure();
        let v = bits.receive_extend(s);
        if k < need && slot[k] != 255 {
            coef[(slot[k] & 15) as usize] = v.wrapping_mul(q[k]);
            any_ac |= v;
        }
        k += 1;
    }
    // Coefficients we only skip. After `ensure` at least 32 bits are
    // buffered: enough for SKIP_BATCH table-resolved symbols.
    'skip: while k < 64 {
        bits.ensure();
        for _ in 0..SKIP_BATCH {
            let f = ac.fast_ac[(bits.peek(FAST_BITS) as usize) & (FAST_SIZE - 1)];
            if f == 0 {
                break;
            }
            bits.consume((f & 15) as u32);
            k += ((f >> 4) & 15) as usize + 1;
            if k >= 64 {
                break 'skip;
            }
        }
        bits.ensure();
        let f = ac.fast_ac[(bits.peek(FAST_BITS) as usize) & (FAST_SIZE - 1)];
        if f != 0 {
            continue;
        }
        let rs = bits.symbol(ac)?;
        let (r, s) = ((rs >> 4) as usize, rs & 15);
        if s == 0 {
            if r == 15 {
                k += 16;
                continue;
            }
            break;
        }
        bits.ensure();
        bits.consume(s);
        k += r + 1;
    }
    Ok(any_ac != 0)
}

#[inline(always)]
fn clamp(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

/// 2×2 from the top-left 2×2 coefficients (same as jpeg-decoder).
#[inline(always)]
fn idct_2x2(c: &[i32; 16], out: &mut [u8], at: usize, stride: usize) {
    let (s00, s01, s10, s11) = (c[0], c[1], c[4], c[5]);
    let x0 = s00.wrapping_add(s10).wrapping_add(4 + (128 << 3));
    let x2 = s00.wrapping_sub(s10).wrapping_add(4 + (128 << 3));
    let x1 = s01.wrapping_add(s11);
    let x3 = s01.wrapping_sub(s11);
    if let Some(r) = out.get_mut(at..at + 2) {
        r[0] = clamp(x0.wrapping_add(x1) >> 3);
        r[1] = clamp(x0.wrapping_sub(x1) >> 3);
    }
    if let Some(r) = out.get_mut(at + stride..at + stride + 2) {
        r[0] = clamp(x2.wrapping_add(x3) >> 3);
        r[1] = clamp(x2.wrapping_sub(x3) >> 3);
    }
}

/// 4×4 from the top-left 4×4 coefficients (same as jpeg-decoder).
#[inline(always)]
fn idct_4x4(c: &[i32; 16], out: &mut [u8], at: usize, stride: usize) {
    const CONST_BITS: u32 = 12;
    const PASS1_BITS: u32 = 2;
    const FINAL_BITS: u32 = CONST_BITS + PASS1_BITS + 3;
    const K0: i32 = 2217; // 0.541196100 * 4096
    const K1: i32 = -7568; // -1.847759065 * 4096
    const K2: i32 = 3135; // 0.765366865 * 4096
    let mut t = [0i32; 16];
    for i in 0..4 {
        let (s0, s1, s2, s3) = (c[i], c[4 + i], c[8 + i], c[12 + i]);
        let x0 = s0.wrapping_add(s2) << PASS1_BITS;
        let x2 = s0.wrapping_sub(s2) << PASS1_BITS;
        let p1 = s1.wrapping_add(s3).wrapping_mul(K0);
        let t0 = (p1.wrapping_add(s3.wrapping_mul(K1)).wrapping_add(512)) >> (CONST_BITS - PASS1_BITS);
        let t2 = (p1.wrapping_add(s1.wrapping_mul(K2)).wrapping_add(512)) >> (CONST_BITS - PASS1_BITS);
        t[i] = x0.wrapping_add(t2);
        t[12 + i] = x0.wrapping_sub(t2);
        t[4 + i] = x2.wrapping_add(t0);
        t[8 + i] = x2.wrapping_sub(t0);
    }
    for i in 0..4 {
        let (s0, s1, s2, s3) = (t[i * 4], t[i * 4 + 1], t[i * 4 + 2], t[i * 4 + 3]);
        let bias = (1 << (FINAL_BITS - 1)) + (128 << FINAL_BITS);
        let x0 = (s0.wrapping_add(s2) << CONST_BITS).wrapping_add(bias);
        let x2 = (s0.wrapping_sub(s2) << CONST_BITS).wrapping_add(bias);
        let p1 = s1.wrapping_add(s3).wrapping_mul(K0);
        let t0 = p1.wrapping_add(s3.wrapping_mul(K1));
        let t2 = p1.wrapping_add(s1.wrapping_mul(K2));
        if let Some(r) = out.get_mut(at + i * stride..at + i * stride + 4) {
            r[0] = clamp(x0.wrapping_add(t2) >> FINAL_BITS);
            r[3] = clamp(x0.wrapping_sub(t2) >> FINAL_BITS);
            r[1] = clamp(x2.wrapping_add(t0) >> FINAL_BITS);
            r[2] = clamp(x2.wrapping_sub(t0) >> FINAL_BITS);
        }
    }
}

/// Nearest-neighbour 2× horizontal upsampling of a chroma row.
#[inline]
fn upsample2(dst: &mut [u8], src: &[u8]) {
    for (d, &v) in dst.chunks_exact_mut(2).zip(src) {
        d[0] = v;
        d[1] = v;
    }
    if dst.len() % 2 == 1 {
        let last = dst.len() - 1;
        dst[last] = src.get(last / 2).copied().unwrap_or(0);
    }
}

/// JFIF YCbCr → interleaved RGB, 16-bit fixed point. Works on planar
/// chunks (auto-vectorised) and interleaves at the end.
#[inline]
fn ycc_to_rgb(out: &mut [u8], y: &[u8], cb: &[u8], cr: &[u8]) {
    const N: usize = 64;
    let n = out.len() / 3;
    let (y, cb, cr) = (&y[..n.min(y.len())], &cb[..n.min(cb.len())], &cr[..n.min(cr.len())]);
    let mut r = [0u8; N];
    let mut g = [0u8; N];
    let mut b = [0u8; N];
    for (((o, yc), bc), rc) in out.chunks_mut(3 * N).zip(y.chunks(N)).zip(cb.chunks(N)).zip(cr.chunks(N)) {
        let m = yc.len().min(bc.len()).min(rc.len());
        for i in 0..m {
            let l = (yc[i] as i32) << 16;
            let u = bc[i] as i32 - 128;
            let v = rc[i] as i32 - 128;
            r[i] = clamp((l + 91_881 * v + 32_768) >> 16);
            g[i] = clamp((l - 22_554 * u - 46_802 * v + 32_768) >> 16);
            b[i] = clamp((l + 116_130 * u + 32_768) >> 16);
        }
        for (i, px) in o.chunks_exact_mut(3).take(m).enumerate() {
            px[0] = r[i];
            px[1] = g[i];
            px[2] = b[i];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jpeg::test_util::{jpeg, pattern};

    /// Mean absolute difference per channel.
    fn mad(a: &RgbImage, b: &RgbImage) -> f64 {
        assert_eq!(a.dimensions(), b.dimensions());
        let s: u64 =
            a.as_raw().iter().zip(b.as_raw()).map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as u64).sum();
        s as f64 / a.as_raw().len() as f64
    }

    fn reference(data: &[u8], w: u32, h: u32, denom: u32) -> RgbImage {
        let mut d = jpeg_decoder::Decoder::new(data);
        d.read_info().unwrap();
        d.scale(w.div_ceil(denom) as u16, h.div_ceil(denom) as u16).unwrap();
        let px = d.decode().unwrap();
        let info = d.info().unwrap();
        let px = if info.pixel_format == jpeg_decoder::PixelFormat::L8 {
            px.iter().flat_map(|&l| [l, l, l]).collect()
        } else {
            px
        };
        RgbImage::from_raw(info.width as u32, info.height as u32, px).unwrap()
    }

    #[test]
    fn matches_jpeg_decoder() {
        for (w, h) in [(640u32, 480u32), (333, 217), (17, 9)] {
            let data = jpeg(w, h);
            for denom in [2u32, 4, 8] {
                let ours = decode_reduced(&data, denom).unwrap();
                let theirs = reference(&data, w, h, denom);
                let d = mad(&ours, &theirs);
                // Same IDCTs (`image` encodes 4:4:4): only rounding differs.
                assert!(d < 1.5, "{w}x{h} 1/{denom}: mean abs diff {d}");
            }
        }
    }

    #[test]
    fn grayscale_and_restart_markers() {
        let img = pattern(300, 200);
        // 4:4:4 + restart markers every 3 MCUs (jpeg-encoder).
        let mut out = Vec::new();
        let mut enc = jpeg_encoder::Encoder::new(&mut out, 80);
        enc.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
        enc.set_restart_interval(3);
        enc.encode(img.as_raw(), 300, 200, jpeg_encoder::ColorType::Rgb).unwrap();
        for d in [2, 4, 8] {
            let ours = decode_reduced(&out, d).unwrap();
            assert!(mad(&ours, &reference(&out, 300, 200, d)) < 1.5);
        }
        // Grayscale.
        let gray = image::DynamicImage::ImageRgb8(img).into_luma8();
        let mut g = Vec::new();
        jpeg_encoder::Encoder::new(&mut g, 85).encode(gray.as_raw(), 300, 200, jpeg_encoder::ColorType::Luma).unwrap();
        let ours = decode_reduced(&g, 2).unwrap();
        assert!(mad(&ours, &reference(&g, 300, 200, 2)) < 1.0);
    }

    #[test]
    fn hostile_input() {
        let data = jpeg(200, 120);
        for cut in (0..data.len()).step_by(37) {
            let _ = decode_reduced(&data[..cut], 4);
        }
        let mut seed = 12345u32;
        for _ in 0..200 {
            let mut d = data.clone();
            for _ in 0..4 {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                let i = (seed >> 8) as usize % d.len();
                d[i] = (seed >> 3) as u8;
            }
            let _ = decode_reduced(&d, 8);
            let _ = decode_reduced(&d, 2);
        }
        assert!(decode_reduced(b"\xFF\xD8\xFF\xD9", 4).is_err());
        assert!(decode_reduced(&data, 3).is_err());
    }

    /// Ground truth for a 1/d reduction: full decode, then d×d box average.
    fn truth(data: &[u8], d: u32) -> RgbImage {
        let mut dec = jpeg_decoder::Decoder::new(data);
        let px = dec.decode().unwrap();
        let info = dec.info().unwrap();
        let full = RgbImage::from_raw(info.width as u32, info.height as u32, px).unwrap();
        let (w, h) = full.dimensions();
        RgbImage::from_fn(w.div_ceil(d), h.div_ceil(d), |x, y| {
            let mut acc = [0u32; 3];
            let mut n = 0;
            for yy in y * d..((y + 1) * d).min(h) {
                for xx in x * d..((x + 1) * d).min(w) {
                    let p = full.get_pixel(xx, yy);
                    for c in 0..3 {
                        acc[c] += p[c] as u32;
                    }
                    n += 1;
                }
            }
            image::Rgb(acc.map(|v| (v / n.max(1)) as u8))
        })
    }

    #[test]
    fn subsampled_chroma_is_faithful() {
        // On 4:2:0 / 4:2:2 input our output is at least as close to the true
        // reduction as jpeg-decoder's (whose reduced chroma is smeared).
        let img = pattern(320, 240);
        for sf in [jpeg_encoder::SamplingFactor::F_2_2, jpeg_encoder::SamplingFactor::F_2_1] {
            let mut out = Vec::new();
            let mut enc = jpeg_encoder::Encoder::new(&mut out, 90);
            enc.set_sampling_factor(sf);
            enc.encode(img.as_raw(), 320, 240, jpeg_encoder::ColorType::Rgb).unwrap();
            for d in [2u32, 4, 8] {
                let t = truth(&out, d);
                let ours = mad(&decode_reduced(&out, d).unwrap(), &t);
                let theirs = mad(&reference(&out, 320, 240, d), &t);
                // At 1/8 a 4:2:0 chroma DC spans 16×16 px: both are equally coarse.
                let bound = if d == 8 { f64::MAX } else { 8.0 };
                assert!(
                    ours <= theirs + 0.5 && ours < bound,
                    "{sf:?} 1/{d}: ours {ours:.2} vs jpeg-decoder {theirs:.2}"
                );
            }
        }
    }

    #[test]
    fn ff_detection() {
        assert!(has_ff_byte(0x00FF_0000_0000_0000));
        assert!(has_ff_byte(0x0000_0000_0000_00FF));
        assert!(!has_ff_byte(0xFEFE_FEFE_FEFE_FEFE));
        assert!(!has_ff_byte(0));
    }
}
