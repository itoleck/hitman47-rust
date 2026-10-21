//! Pixel format handling: sizes, conversion of DirectDraw surface memory to
//! BGRA8 for upload, and back (for locks of GPU-rendered surfaces).

use crate::types::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fmt {
    /// Generic RGB/alpha/luminance with bit masks, `bpp` bits per pixel.
    Masked { bpp: u32 },
    Pal8,
    Pal4,
    Dxt { fourcc: u32 },
    Depth { bpp: u32 },
}

pub fn classify(pf: &DDPIXELFORMAT) -> Fmt {
    if pf.dwFlags & DDPF_FOURCC != 0 {
        return Fmt::Dxt { fourcc: pf.dwFourCC };
    }
    if pf.dwFlags & DDPF_ZBUFFER != 0 {
        return Fmt::Depth { bpp: pf.dwRGBBitCount };
    }
    if pf.dwFlags & DDPF_PALETTEINDEXED8 != 0 {
        return Fmt::Pal8;
    }
    if pf.dwFlags & DDPF_PALETTEINDEXED4 != 0 {
        return Fmt::Pal4;
    }
    Fmt::Masked { bpp: pf.dwRGBBitCount.max(8) }
}

pub fn dxt_block_bytes(fourcc: u32) -> u32 {
    if fourcc == FOURCC_DXT1 {
        8
    } else {
        16
    }
}

/// (pitch, total bytes) for a surface level.
pub fn level_layout(pf: &DDPIXELFORMAT, w: u32, h: u32) -> (u32, u32) {
    match classify(pf) {
        Fmt::Dxt { fourcc } => {
            let bw = w.div_ceil(4).max(1);
            let bh = h.div_ceil(4).max(1);
            let pitch = bw * dxt_block_bytes(fourcc);
            (pitch, pitch * bh)
        }
        Fmt::Pal4 => {
            let pitch = (w.div_ceil(2) + 3) & !3;
            (pitch, pitch * h)
        }
        Fmt::Pal8 => {
            let pitch = (w + 3) & !3;
            (pitch, pitch * h)
        }
        Fmt::Masked { bpp } | Fmt::Depth { bpp } => {
            let pitch = ((w * bpp.div_ceil(8)) + 3) & !3;
            (pitch, pitch * h)
        }
    }
}

pub fn pf_rgb(bpp: u32, r: u32, g: u32, b: u32, a: u32) -> DDPIXELFORMAT {
    DDPIXELFORMAT {
        dwSize: 32,
        dwFlags: DDPF_RGB | if a != 0 { DDPF_ALPHAPIXELS } else { 0 },
        dwFourCC: 0,
        dwRGBBitCount: bpp,
        dwRBitMask: r,
        dwGBitMask: g,
        dwBBitMask: b,
        dwRGBAlphaBitMask: a,
    }
}
pub fn pf_xrgb8888() -> DDPIXELFORMAT {
    pf_rgb(32, 0xFF0000, 0xFF00, 0xFF, 0)
}
pub fn pf_argb8888() -> DDPIXELFORMAT {
    pf_rgb(32, 0xFF0000, 0xFF00, 0xFF, 0xFF000000)
}
pub fn pf_rgb565() -> DDPIXELFORMAT {
    pf_rgb(16, 0xF800, 0x07E0, 0x001F, 0)
}
pub fn pf_xrgb1555() -> DDPIXELFORMAT {
    pf_rgb(16, 0x7C00, 0x03E0, 0x001F, 0)
}
pub fn pf_argb1555() -> DDPIXELFORMAT {
    pf_rgb(16, 0x7C00, 0x03E0, 0x001F, 0x8000)
}
pub fn pf_argb4444() -> DDPIXELFORMAT {
    pf_rgb(16, 0x0F00, 0x00F0, 0x000F, 0xF000)
}
pub fn pf_pal8() -> DDPIXELFORMAT {
    DDPIXELFORMAT { dwSize: 32, dwFlags: DDPF_PALETTEINDEXED8 | DDPF_RGB, dwRGBBitCount: 8, ..Default::default() }
}
pub fn pf_fourcc(fcc: u32) -> DDPIXELFORMAT {
    DDPIXELFORMAT { dwSize: 32, dwFlags: DDPF_FOURCC, dwFourCC: fcc, ..Default::default() }
}
pub fn pf_depth(bits: u32, zmask: u32, stencil_bits: u32, smask: u32) -> DDPIXELFORMAT {
    DDPIXELFORMAT {
        dwSize: 32,
        dwFlags: DDPF_ZBUFFER | if stencil_bits != 0 { DDPF_STENCILBUFFER } else { 0 },
        dwFourCC: 0,
        dwRGBBitCount: bits,
        dwRBitMask: stencil_bits,
        dwGBitMask: zmask,
        dwBBitMask: smask,
        dwRGBAlphaBitMask: 0,
    }
}

/// Scale helper for one channel.
#[derive(Clone, Copy)]
struct Chan {
    shift: u32,
    bits: u32,
    mask: u32,
}
impl Chan {
    fn new(mask: u32) -> Chan {
        if mask == 0 {
            return Chan { shift: 0, bits: 0, mask: 0 };
        }
        Chan { shift: mask.trailing_zeros(), bits: (mask >> mask.trailing_zeros()).trailing_ones(), mask }
    }
    #[inline]
    fn get(&self, px: u32, default: u8) -> u8 {
        if self.bits == 0 {
            return default;
        }
        let v = (px & self.mask) >> self.shift;
        if self.bits >= 8 {
            (v >> (self.bits - 8)) as u8
        } else {
            let max = (1u32 << self.bits) - 1;
            ((v * 255 + max / 2) / max) as u8
        }
    }
    #[inline]
    fn put(&self, v: u8) -> u32 {
        if self.bits == 0 {
            return 0;
        }
        let x = if self.bits >= 8 { (v as u32) << (self.bits - 8) } else { (v as u32 * ((1 << self.bits) - 1) + 127) / 255 };
        (x << self.shift) & self.mask
    }
}

#[inline]
fn read_px(row: &[u8], x: usize, bytes: usize) -> u32 {
    let o = x * bytes;
    match bytes {
        1 => row[o] as u32,
        2 => u16::from_le_bytes([row[o], row[o + 1]]) as u32,
        3 => row[o] as u32 | (row[o + 1] as u32) << 8 | (row[o + 2] as u32) << 16,
        _ => u32::from_le_bytes([row[o], row[o + 1], row[o + 2], row[o + 3]]),
    }
}

#[inline]
fn write_px(row: &mut [u8], x: usize, bytes: usize, v: u32) {
    let o = x * bytes;
    match bytes {
        1 => row[o] = v as u8,
        2 => row[o..o + 2].copy_from_slice(&(v as u16).to_le_bytes()),
        3 => {
            row[o] = v as u8;
            row[o + 1] = (v >> 8) as u8;
            row[o + 2] = (v >> 16) as u8;
        }
        _ => row[o..o + 4].copy_from_slice(&v.to_le_bytes()),
    }
}

/// Converts surface memory to tightly packed BGRA8.
///
/// * `palette`: 256 entries for paletted formats.
/// * `colorkey`: source colour key range; matching texels get alpha 0 (others
///   keep their alpha, or 255 if the format has none).
pub fn to_bgra8(
    src: &[u8],
    pitch: u32,
    w: u32,
    h: u32,
    pf: &DDPIXELFORMAT,
    palette: Option<&[PALETTEENTRY; 256]>,
    palette_alpha: bool,
    colorkey: Option<DDCOLORKEY>,
) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let mut out = vec![0u8; w * h * 4];
    let pitch = pitch as usize;
    let key = |raw: u32| colorkey.map(|k| raw >= k.low && raw <= k.high).unwrap_or(false);
    match classify(pf) {
        Fmt::Masked { bpp } => {
            let bytes = bpp.div_ceil(8) as usize;
            let lum = pf.dwFlags & DDPF_LUMINANCE != 0;
            let alpha_only = pf.dwFlags & DDPF_ALPHA != 0 && pf.dwFlags & DDPF_RGB == 0;
            let has_alpha = pf.dwFlags & (DDPF_ALPHAPIXELS | DDPF_ALPHA) != 0 && pf.dwRGBAlphaBitMask != 0;
            let (r, g, b) = (Chan::new(pf.dwRBitMask), Chan::new(pf.dwGBitMask), Chan::new(pf.dwBBitMask));
            let a = Chan::new(if has_alpha { pf.dwRGBAlphaBitMask } else { 0 });
            // Fast path: X/A8R8G8B8 is already BGRA in memory.
            let is8888 = bytes == 4 && pf.dwRBitMask == 0xFF0000 && pf.dwGBitMask == 0xFF00 && pf.dwBBitMask == 0xFF;
            for y in 0..h {
                let row = &src[y * pitch..];
                let o = &mut out[y * w * 4..(y + 1) * w * 4];
                if is8888 && colorkey.is_none() {
                    o.copy_from_slice(&row[..w * 4]);
                    if !has_alpha {
                        for p in o.chunks_exact_mut(4) {
                            p[3] = 255;
                        }
                    }
                    continue;
                }
                for x in 0..w {
                    let px = read_px(row, x, bytes);
                    let (rr, gg, bb, aa) = if alpha_only {
                        (255, 255, 255, Chan::new(pf.dwRGBAlphaBitMask).get(px, 255))
                    } else if lum {
                        let l = r.get(px, 0);
                        (l, l, l, a.get(px, 255))
                    } else {
                        (r.get(px, 0), g.get(px, 0), b.get(px, 0), a.get(px, 255))
                    };
                    let aa = if key(px) { 0 } else { aa };
                    o[x * 4..x * 4 + 4].copy_from_slice(&[bb, gg, rr, aa]);
                }
            }
        }
        Fmt::Pal8 | Fmt::Pal4 => {
            let four = matches!(classify(pf), Fmt::Pal4);
            for y in 0..h {
                let row = &src[y * pitch..];
                for x in 0..w {
                    let idx = if four {
                        let b = row[x / 2];
                        if x % 2 == 0 {
                            b >> 4
                        } else {
                            b & 0xF
                        }
                    } else {
                        row[x]
                    } as usize;
                    let e = palette.map(|p| p[idx]).unwrap_or(PALETTEENTRY {
                        peRed: idx as u8,
                        peGreen: idx as u8,
                        peBlue: idx as u8,
                        peFlags: 255,
                    });
                    let mut aa = if palette_alpha { e.peFlags } else { 255 };
                    if key(idx as u32) {
                        aa = 0;
                    }
                    out[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&[e.peBlue, e.peGreen, e.peRed, aa]);
                }
            }
        }
        Fmt::Dxt { .. } => {
            let v = decode_dxt(src, w as u32, h as u32, pf.dwFourCC);
            out.copy_from_slice(&v);
        }
        Fmt::Depth { .. } => {}
    }
    out
}

/// Converts packed BGRA8 back into surface memory of format `pf`.
pub fn from_bgra8(bgra: &[u8], w: u32, h: u32, dst: &mut [u8], pitch: u32, pf: &DDPIXELFORMAT) {
    let (w, h, pitch) = (w as usize, h as usize, pitch as usize);
    if let Fmt::Masked { bpp } = classify(pf) {
        let bytes = bpp.div_ceil(8) as usize;
        let (r, g, b) = (Chan::new(pf.dwRBitMask), Chan::new(pf.dwGBitMask), Chan::new(pf.dwBBitMask));
        let a = Chan::new(pf.dwRGBAlphaBitMask);
        for y in 0..h {
            let srow = &bgra[y * w * 4..(y + 1) * w * 4];
            let drow = &mut dst[y * pitch..];
            if bytes == 4 && pf.dwRBitMask == 0xFF0000 && pf.dwGBitMask == 0xFF00 && pf.dwBBitMask == 0xFF {
                drow[..w * 4].copy_from_slice(srow);
                continue;
            }
            for x in 0..w {
                let p = &srow[x * 4..x * 4 + 4];
                let v = b.put(p[0]) | g.put(p[1]) | r.put(p[2]) | a.put(p[3]);
                write_px(drow, x, bytes, v);
            }
        }
    }
}

/// Converts a D3D ARGB colour (as used by Blt colour fills on render targets)
/// from a surface-format pixel value to floating point RGBA.
pub fn pixel_to_rgba(px: u32, pf: &DDPIXELFORMAT) -> [f32; 4] {
    match classify(pf) {
        Fmt::Masked { .. } => {
            let (r, g, b) = (Chan::new(pf.dwRBitMask), Chan::new(pf.dwGBitMask), Chan::new(pf.dwBBitMask));
            let a = Chan::new(pf.dwRGBAlphaBitMask);
            [r.get(px, 0) as f32 / 255.0, g.get(px, 0) as f32 / 255.0, b.get(px, 0) as f32 / 255.0, a.get(px, 255) as f32 / 255.0]
        }
        _ => [0.0, 0.0, 0.0, 1.0],
    }
}

pub fn argb_to_rgba(c: u32) -> [f32; 4] {
    [
        ((c >> 16) & 0xFF) as f32 / 255.0,
        ((c >> 8) & 0xFF) as f32 / 255.0,
        (c & 0xFF) as f32 / 255.0,
        ((c >> 24) & 0xFF) as f32 / 255.0,
    ]
}

/// Fills a rectangle of surface memory with a raw pixel value.
pub fn fill(dst: &mut [u8], pitch: u32, pf: &DDPIXELFORMAT, rect: RECT, value: u32) {
    let bytes = match classify(pf) {
        Fmt::Masked { bpp } | Fmt::Depth { bpp } => bpp.div_ceil(8) as usize,
        Fmt::Pal8 => 1,
        _ => return,
    };
    for y in rect.top.max(0) as usize..rect.bottom.max(0) as usize {
        let row = &mut dst[y * pitch as usize..];
        for x in rect.left.max(0) as usize..rect.right.max(0) as usize {
            write_px(row, x, bytes, value);
        }
    }
}

/// CPU blit with nearest-neighbour stretching, optional mirroring and source
/// colour key. Both surfaces must share a pixel format (or both be BGRA8
/// intermediates).
#[allow(clippy::too_many_arguments)]
pub fn blit(
    src: &[u8],
    spitch: u32,
    srect: RECT,
    dst: &mut [u8],
    dpitch: u32,
    drect: RECT,
    bytes: usize,
    key: Option<DDCOLORKEY>,
    mirror_lr: bool,
    mirror_ud: bool,
) {
    let (sw, sh) = (srect.width().max(1) as i64, srect.height().max(1) as i64);
    let (dw, dh) = (drect.width().max(0) as i64, drect.height().max(0) as i64);
    if dw == 0 || dh == 0 {
        return;
    }
    let same = sw == dw && sh == dh && !mirror_lr && !mirror_ud;
    for dy in 0..dh {
        let sy = if same { dy } else { dy * sh / dh };
        let sy = if mirror_ud { sh - 1 - sy } else { sy };
        let srow = &src[((srect.top as i64 + sy) as usize) * spitch as usize..];
        let drow = &mut dst[((drect.top as i64 + dy) as usize) * dpitch as usize..];
        if same && key.is_none() {
            let so = srect.left as usize * bytes;
            let dof = drect.left as usize * bytes;
            let n = dw as usize * bytes;
            drow[dof..dof + n].copy_from_slice(&srow[so..so + n]);
            continue;
        }
        for dx in 0..dw {
            let sx = if same { dx } else { dx * sw / dw };
            let sx = if mirror_lr { sw - 1 - sx } else { sx };
            let px = read_px(srow, (srect.left as i64 + sx) as usize, bytes);
            if let Some(k) = key {
                if px >= k.low && px <= k.high {
                    continue;
                }
            }
            write_px(drow, (drect.left as i64 + dx) as usize, bytes, px);
        }
    }
}

// ------------------------------------------------------------ DXT decoding
// Used for CPU-side reads of compressed surfaces (and as a fallback when the
// GPU lacks BC texture support).

fn rgb565(c: u16) -> [u8; 3] {
    let r = ((c >> 11) & 31) as u32;
    let g = ((c >> 5) & 63) as u32;
    let b = (c & 31) as u32;
    [((r * 527 + 23) >> 6) as u8, ((g * 259 + 33) >> 6) as u8, ((b * 527 + 23) >> 6) as u8]
}

fn decode_color_block(b: &[u8], out: &mut [[u8; 4]; 16], dxt1: bool) {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let mut pal = [[0u8; 4]; 4];
    pal[0] = [p0[0], p0[1], p0[2], 255];
    pal[1] = [p1[0], p1[1], p1[2], 255];
    if c0 > c1 || !dxt1 {
        for i in 0..3 {
            pal[2][i] = ((2 * p0[i] as u32 + p1[i] as u32) / 3) as u8;
            pal[3][i] = ((p0[i] as u32 + 2 * p1[i] as u32) / 3) as u8;
        }
        pal[2][3] = 255;
        pal[3][3] = 255;
    } else {
        for i in 0..3 {
            pal[2][i] = ((p0[i] as u32 + p1[i] as u32) / 2) as u8;
        }
        pal[2][3] = 255;
        pal[3] = [0, 0, 0, 0];
    }
    let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    for (i, o) in out.iter_mut().enumerate() {
        *o = pal[((bits >> (2 * i)) & 3) as usize];
    }
}

pub fn decode_dxt(src: &[u8], w: u32, h: u32, fourcc: u32) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let mut out = vec![0u8; w * h * 4];
    let bw = w.div_ceil(4).max(1);
    let bh = h.div_ceil(4).max(1);
    let bsize = dxt_block_bytes(fourcc) as usize;
    for by in 0..bh {
        for bx in 0..bw {
            let off = (by * bw + bx) * bsize;
            if off + bsize > src.len() {
                return out;
            }
            let blk = &src[off..off + bsize];
            let mut px = [[0u8; 4]; 16];
            if fourcc == FOURCC_DXT1 {
                decode_color_block(blk, &mut px, true);
            } else {
                decode_color_block(&blk[8..], &mut px, false);
                if fourcc == FOURCC_DXT2 || fourcc == FOURCC_DXT3 {
                    for (i, p) in px.iter_mut().enumerate() {
                        let a = (blk[i / 2] >> ((i % 2) * 4)) & 0xF;
                        p[3] = a * 17;
                    }
                } else {
                    let (a0, a1) = (blk[0] as u32, blk[1] as u32);
                    let mut pal = [0u32; 8];
                    pal[0] = a0;
                    pal[1] = a1;
                    if a0 > a1 {
                        for i in 1..7 {
                            pal[i + 1] = ((7 - i as u32) * a0 + i as u32 * a1) / 7;
                        }
                    } else {
                        for i in 1..5 {
                            pal[i + 1] = ((5 - i as u32) * a0 + i as u32 * a1) / 5;
                        }
                        pal[6] = 0;
                        pal[7] = 255;
                    }
                    let mut bits: u64 = 0;
                    for i in 0..6 {
                        bits |= (blk[2 + i] as u64) << (8 * i);
                    }
                    for (i, p) in px.iter_mut().enumerate() {
                        p[3] = pal[((bits >> (3 * i)) & 7) as usize] as u8;
                    }
                }
            }
            for (i, p) in px.iter().enumerate() {
                let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
                if x < w && y < h {
                    // decoded as RGBA; store BGRA
                    out[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&[p[2], p[1], p[0], p[3]]);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb565_roundtrip() {
        let pf = pf_rgb565();
        let src = [0x1Fu8, 0xF8]; // 0xF81F magenta
        let out = to_bgra8(&src, 4, 1, 1, &pf, None, false, None);
        assert_eq!(out, vec![255, 0, 255, 255]);
        let mut back = [0u8; 4];
        from_bgra8(&out, 1, 1, &mut back, 4, &pf);
        assert_eq!(&back[..2], &src);
    }

    #[test]
    fn colorkey_masks_alpha() {
        let pf = pf_rgb565();
        let src = [0u8, 0, 0xFF, 0xFF];
        let out = to_bgra8(&src, 4, 2, 1, &pf, None, false, Some(DDCOLORKEY { low: 0, high: 0 }));
        assert_eq!(out[3], 0);
        assert_eq!(out[7], 255);
    }

    #[test]
    fn dxt1_solid() {
        // c0 = c1 = red, all indices 0
        let blk = [0x00, 0xF8, 0x00, 0xF8, 0, 0, 0, 0];
        let out = decode_dxt(&blk, 4, 4, FOURCC_DXT1);
        assert_eq!(&out[..4], &[0, 0, 255, 255]);
    }
}
