//! Tray icon variants, derived from `icons/tray.png` (RGBA, 32x32) pixel by
//! pixel, so they stay as crisp as the original at 16 and 32 px.
//!
//! - **Running:** the original plus a 2 px neon-cyan rim on the tile, so a
//!   moving belt reads at a glance (1 px at 16 px, still visible).
//! - **Idle:** the original, unchanged.
//! - **Offline:** greyscale, darker: the treadmill is not connected.
//! - **Update dot:** any of the above plus a small pink dot, ringed in the
//!   tile's dark blue, in the top-right corner: an update is waiting.
//!
//! The macOS menu bar takes template images instead: black, with the shape in
//! alpha, which macOS tints to suit a light or dark menu bar (`template`). The
//! tile goes; the chevrons stay. Idle is the chevrons, running adds the belt
//! line under them, offline dims the chevrons, and the update dot is a solid
//! dot with a clear ring cut round it (`template_dot`).
//!
//! Pure, no dependencies: `build.rs` includes this file (`#[path]`) to write
//! the variants into `OUT_DIR` at build time, and the tests run against it from
//! `tray.rs`.

/// The rim colour: the cyan of the chevrons (#00f3ff).
const RIM: [u8; 3] = [0x00, 0xf3, 0xff];
/// Rim width, px at 32 px.
const RIM_PX: i32 = 2;
/// A pixel at least this opaque is part of the tile.
const SOLID: u8 = 128;

/// The dot's colour: the magenta of the belt line (#ff2bd6).
const DOT: [u8; 3] = [0xff, 0x2b, 0xd6];
/// The ring round the dot: the tile's colour (#060a10), so the dot stands
/// apart from the chevron and from a light taskbar.
const DOT_RING: [u8; 3] = [0x06, 0x0a, 0x10];
/// Dot centre and radii, as fractions of the icon's size (32 px: centre
/// (26, 6), dot radius 3.75 px, ring out to 5.25 px). At 16 px that is a
/// dot about 4 px across.
const DOT_CX: f32 = 26.0 / 32.0;
const DOT_CY: f32 = 6.0 / 32.0;
const DOT_R: f32 = 3.75 / 32.0;
const DOT_RING_R: f32 = 5.25 / 32.0;
/// Samples per pixel side for the dot's anti-aliasing.
const AA: u32 = 4;

/// Draw `colour` over the pixel at index `i` with coverage `cov` (0..1).
fn blend(rgba: &mut [u8], i: usize, colour: [u8; 3], cov: f32) {
    if cov <= 0.0 {
        return;
    }
    let a = rgba[i + 3] as f32 / 255.0;
    let out_a = cov + a * (1.0 - cov);
    for c in 0..3 {
        let v = (colour[c] as f32 * cov + rgba[i + c] as f32 * a * (1.0 - cov)) / out_a;
        rgba[i + c] = v.round().clamp(0.0, 255.0) as u8;
    }
    rgba[i + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// The update dot's coverage of pixel (x, y): (disc, dot), each 0..1. The
/// disc is the dot and its ring together.
fn dot_coverage(x: u32, y: u32, width: u32, height: u32) -> (f32, f32) {
    let size = width.min(height) as f32;
    let (cx, cy) = (DOT_CX * size, DOT_CY * size);
    let (r, ring) = (DOT_R * size, DOT_RING_R * size);
    let (mut in_ring, mut in_dot) = (0u32, 0u32);
    for sy in 0..AA {
        for sx in 0..AA {
            let px = x as f32 + (sx as f32 + 0.5) / AA as f32 - cx;
            let py = y as f32 + (sy as f32 + 0.5) / AA as f32 - cy;
            let d = (px * px + py * py).sqrt();
            if d <= r {
                in_dot += 1;
            } else if d <= ring {
                in_ring += 1;
            }
        }
    }
    let n = (AA * AA) as f32;
    ((in_ring + in_dot) as f32 / n, in_dot as f32 / n)
}

/// Add the "update waiting" dot in the top-right corner, anti-aliased.
pub fn update_dot(rgba: &mut [u8], width: u32, height: u32) {
    for y in 0..height {
        for x in 0..width {
            let (disc, dot) = dot_coverage(x, y, width, height);
            if disc <= 0.0 {
                continue;
            }
            let i = ((y * width + x) * 4) as usize;
            // The ring covers the whole disc first, then the dot goes on top.
            blend(rgba, i, DOT_RING, disc);
            blend(rgba, i, DOT, dot);
        }
    }
}

/// How far a colour channel is from the tile's (`floor`) towards full art
/// (`top`), 0..1.
fn ramp(v: u8, floor: u8, top: u8) -> f32 {
    ((v as f32 - floor as f32) / (top as f32 - floor as f32)).clamp(0.0, 1.0)
}

/// Opacity of the offline template's chevrons.
pub const TEMPLATE_OFFLINE_ALPHA: f32 = 0.4;

/// The macOS template variant of the colour art: black pixels whose alpha
/// is the chevrons (cyan) and, with `belt`, the belt line (magenta), scaled
/// by `opacity`. The dark tile becomes clear.
pub fn template(rgba: &mut [u8], belt: bool, opacity: f32) {
    for px in rgba.as_chunks_mut::<4>().0 {
        let [r, g, b, a] = *px;
        // Cyan: green and blue both high, red low. Magenta: red high, green low.
        let chevron = ramp(g.min(b), 0x10, 0xf3) * (1.0 - ramp(r, 0x06, 0xff));
        let belt_line = if belt && r > g {
            ramp(r, 0x06, 0xff)
        } else {
            0.0
        };
        let shape = chevron.max(belt_line) * (a as f32 / 255.0) * opacity;
        *px = [0, 0, 0, (shape * 255.0).round().clamp(0.0, 255.0) as u8];
    }
}

/// The update dot on a template image: the ring is cut clear, so the dot
/// stands apart from the chevron, then the dot is drawn solid.
pub fn template_dot(rgba: &mut [u8], width: u32, height: u32) {
    for y in 0..height {
        for x in 0..width {
            let (disc, dot) = dot_coverage(x, y, width, height);
            if disc <= 0.0 {
                continue;
            }
            let i = ((y * width + x) * 4) as usize;
            let cut = rgba[i + 3] as f32 * (1.0 - disc);
            let a = cut * (1.0 - dot) + 255.0 * dot;
            rgba[i..i + 4].copy_from_slice(&[0, 0, 0, a.round().clamp(0.0, 255.0) as u8]);
        }
    }
}

/// Turn RGBA pixels greyscale and darker, keeping alpha.
pub fn offline(rgba: &mut [u8]) {
    for px in rgba.as_chunks_mut::<4>().0 {
        let l = 0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32;
        let g = (24.0 + l * 0.5).round().clamp(0.0, 255.0) as u8;
        px[0] = g;
        px[1] = g;
        px[2] = g;
    }
}

/// Paint the outer `RIM_PX` of the opaque tile (and its anti-aliased edge) in
/// the rim colour, keeping alpha so the rounded corners stay smooth.
pub fn running(rgba: &mut [u8], width: u32, height: u32) {
    let (w, h) = (width as i32, height as i32);
    let src = rgba.to_vec();
    let alpha = |x: i32, y: i32| -> u8 {
        if x < 0 || y < 0 || x >= w || y >= h {
            0
        } else {
            src[((y * w + x) * 4 + 3) as usize]
        }
    };
    for y in 0..h {
        for x in 0..w {
            let a = alpha(x, y);
            if a == 0 {
                continue;
            }
            let edge = a < SOLID
                || (-RIM_PX..=RIM_PX).any(|dy| {
                    (-RIM_PX..=RIM_PX).any(|dx| {
                        // A round-ish brush, so the rim follows the corners evenly.
                        dx.abs() + dy.abs() <= RIM_PX + 1 && alpha(x + dx, y + dy) < SOLID
                    })
                });
            if edge {
                let i = ((y * w + x) * 4) as usize;
                rgba[i..i + 3].copy_from_slice(&RIM);
            }
        }
    }
}
