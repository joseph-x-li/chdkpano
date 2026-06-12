//! The output canvas: accumulates warped images with distance-based feather
//! blending. Each pixel keeps a running coverage weight so overlapping images
//! blend by a weighted average (centre of each frame weighted highest).

use anyhow::{bail, Result};
use image::{DynamicImage, Rgba, RgbaImage};
use nalgebra::Matrix3;

use crate::homography::project;

/// Hard cap so a degenerate homography can't ask for a terabyte canvas.
const MAX_DIM: u32 = 20_000;

pub struct Canvas {
    img: RgbaImage,
    weight: Vec<f32>, // coverage weight per pixel, len = width*height
    width: u32,
    height: u32,
}

impl Canvas {
    /// Seed the canvas with the first image at its native position.
    pub fn from_image(src: &DynamicImage) -> Self {
        let img = src.to_rgba8();
        let (width, height) = img.dimensions();
        let mut weight = vec![0f32; (width * height) as usize];
        for y in 0..height {
            for x in 0..width {
                weight[(y * width + x) as usize] = feather(x as f64, y as f64, width, height);
            }
        }
        Canvas { img, weight, width, height }
    }

    /// Current panorama as a DynamicImage (used to re-detect features against).
    pub fn to_dynamic(&self) -> DynamicImage {
        DynamicImage::ImageRgba8(self.img.clone())
    }

    pub fn into_dynamic(self) -> DynamicImage {
        DynamicImage::ImageRgba8(self.img)
    }

    /// Warp `src` into the canvas using `h` (maps src pixel coords -> canvas
    /// coords), growing the canvas as needed and feather-blending overlaps.
    pub fn warp_into(&mut self, src: &DynamicImage, h: &Matrix3<f64>) -> Result<()> {
        let src = src.to_rgba8();
        let (sw, sh) = src.dimensions();
        let hinv = h.try_inverse().ok_or_else(|| anyhow::anyhow!("singular homography"))?;

        // Project src corners into canvas space to find the new bounds.
        let corners = [
            (0.0, 0.0),
            (sw as f64, 0.0),
            (sw as f64, sh as f64),
            (0.0, sh as f64),
        ];
        let (mut minx, mut miny) = (0.0_f64, 0.0_f64); // include existing canvas origin
        let (mut maxx, mut maxy) = (self.width as f64, self.height as f64);
        for &c in &corners {
            let (x, y) = project(h, c);
            if !x.is_finite() || !y.is_finite() {
                bail!("homography projects to infinity");
            }
            minx = minx.min(x);
            miny = miny.min(y);
            maxx = maxx.max(x);
            maxy = maxy.max(y);
        }

        // Offset so everything is non-negative, then size the new canvas.
        let ox = (-minx).max(0.0).ceil() as i64;
        let oy = (-miny).max(0.0).ceil() as i64;
        let new_w = ((maxx.ceil() as i64 + ox).max(self.width as i64 + ox)) as u32;
        let new_h = ((maxy.ceil() as i64 + oy).max(self.height as i64 + oy)) as u32;
        if new_w > MAX_DIM || new_h > MAX_DIM {
            bail!("canvas would be {new_w}x{new_h}, exceeds {MAX_DIM} cap (bad homography?)");
        }

        let mut nimg = RgbaImage::new(new_w, new_h);
        let mut nweight = vec![0f32; (new_w * new_h) as usize];

        // Blit the existing panorama into the offset position.
        for y in 0..self.height {
            for x in 0..self.width {
                let nx = (x as i64 + ox) as u32;
                let ny = (y as i64 + oy) as u32;
                nimg.put_pixel(nx, ny, *self.img.get_pixel(x, y));
                nweight[(ny * new_w + nx) as usize] = self.weight[(y * self.width + x) as usize];
            }
        }

        // Inverse-map the new image's bounding box, sampling + blending.
        let bx0 = ((minx + ox as f64).floor().max(0.0)) as u32;
        let by0 = ((miny + oy as f64).floor().max(0.0)) as u32;
        let bx1 = (((maxx + ox as f64).ceil() as i64).min(new_w as i64)) as u32;
        let by1 = (((maxy + oy as f64).ceil() as i64).min(new_h as i64)) as u32;

        for ny in by0..by1 {
            for nx in bx0..bx1 {
                let cx = nx as f64 - ox as f64;
                let cy = ny as f64 - oy as f64;
                let (sx, sy) = project(&hinv, (cx, cy));
                if sx < 0.0 || sy < 0.0 || sx >= (sw - 1) as f64 || sy >= (sh - 1) as f64 {
                    continue;
                }
                let sample = bilinear(&src, sx, sy);
                let wn = feather(sx, sy, sw, sh);
                let idx = (ny * new_w + nx) as usize;
                let wc = nweight[idx];
                let total = wc + wn;
                if total <= 0.0 {
                    continue;
                }
                let old = nimg.get_pixel(nx, ny);
                let blend = |o: u8, s: u8| ((o as f32 * wc + s as f32 * wn) / total) as u8;
                nimg.put_pixel(
                    nx,
                    ny,
                    Rgba([
                        blend(old[0], sample[0]),
                        blend(old[1], sample[1]),
                        blend(old[2], sample[2]),
                        255,
                    ]),
                );
                nweight[idx] = total;
            }
        }

        self.img = nimg;
        self.weight = nweight;
        self.width = new_w;
        self.height = new_h;
        Ok(())
    }
}

/// Feather weight: 1.0 at the centre of a frame, ramping to ~0 at the edges.
fn feather(x: f64, y: f64, w: u32, h: u32) -> f32 {
    let fx = (x + 0.5).min(w as f64 - x) / (w as f64 / 2.0);
    let fy = (y + 0.5).min(h as f64 - y) / (h as f64 / 2.0);
    ((fx.clamp(0.0, 1.0) * fy.clamp(0.0, 1.0)) as f32).max(0.01)
}

/// Bilinear sample from an RGBA buffer. Caller guarantees 0 <= x < w-1, same y.
fn bilinear(img: &RgbaImage, x: f64, y: f64) -> Rgba<u8> {
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let fx = (x - x0 as f64) as f32;
    let fy = (y - y0 as f64) as f32;
    let c00 = img.get_pixel(x0, y0);
    let c10 = img.get_pixel(x1, y0);
    let c01 = img.get_pixel(x0, y1);
    let c11 = img.get_pixel(x1, y1);
    let lerp = |a: u8, b: u8, t: f32| a as f32 * (1.0 - t) + b as f32 * t;
    let mut out = [0u8; 4];
    for k in 0..4 {
        let top = lerp(c00[k], c10[k], fx);
        let bot = lerp(c01[k], c11[k], fx);
        out[k] = (top * (1.0 - fy) + bot * fy) as u8;
    }
    Rgba(out)
}
