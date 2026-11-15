//! Tray icon variants, derived from `icons/tray.png` (RGBA, 32x32) pixel by
//! pixel, so they stay as crisp as the original at 16 and 32 px.
//!
//! - **Running:** the original plus a 2 px neon-cyan rim on the tile, so a
//!   moving belt reads at a glance (1 px at 16 px, still visible).
//! - **Idle:** the original, unchanged.
//! - **Offline:** greyscale, darker: the treadmill is not connected.
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
