//! Pipeline orchestration: incremental stitching.
//!
//! Start with the first image as the panorama, then for each subsequent image:
//! detect features on it and on the running panorama, match, estimate the
//! homography, and warp+blend it in. Re-detecting against the growing panorama
//! each step keeps every homography in the *current* canvas frame, so there's
//! no transform bookkeeping — at the cost of a few extra detections.
//!
//! This is the deliberately-simple baseline (no global bundle adjustment, no
//! exposure compensation, no fixed-rig calibration). Good enough to prove the
//! end-to-end path; the knobs come later.

use anyhow::{bail, Context, Result};
use image::DynamicImage;

use crate::features;
use crate::homography;
use crate::warp::Canvas;

pub fn stitch(images: Vec<DynamicImage>) -> Result<DynamicImage> {
    if images.len() < 2 {
        bail!("need at least 2 images");
    }

    let mut canvas = Canvas::from_image(&images[0]);

    for (i, img) in images.iter().enumerate().skip(1) {
        let pano = canvas.to_dynamic();
        let pano_feats = features::detect(&pano);
        let img_feats = features::detect(img);

        let matches = features::match_features(&img_feats, &pano_feats);
        if matches.len() < 8 {
            bail!(
                "image {i}: only {} good matches against the panorama so far \
                 (need >= 8) — not enough overlap or texture",
                matches.len()
            );
        }

        let h = homography::ransac(&matches)
            .with_context(|| format!("image {i}: RANSAC could not find a consistent homography"))?;

        canvas
            .warp_into(img, &h)
            .with_context(|| format!("image {i}: warp/blend failed"))?;

        tracing::info!("merged image {i} ({} matches)", matches.len());
    }

    Ok(canvas.into_dynamic())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, Rgb, RgbImage};

    /// A textured image with irregular colored rectangles — gives AKAZE plenty
    /// of distinctive (non-repetitive) features to lock onto.
    fn synth(w: u32, h: u32) -> RgbImage {
        let mut img = RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                img.put_pixel(x, y, Rgb([(x * 255 / w) as u8, (y * 255 / h) as u8, 128]));
            }
        }
        let mut s: u64 = 0x1234_5678_9abc_def0;
        let mut rnd = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        for _ in 0..200 {
            let rw = 10 + (rnd() % 60) as u32;
            let rh = 10 + (rnd() % 60) as u32;
            let rx = (rnd() % w as u64) as u32;
            let ry = (rnd() % h as u64) as u32;
            let col = Rgb([(rnd() % 256) as u8, (rnd() % 256) as u8, (rnd() % 256) as u8]);
            for y in ry..(ry + rh).min(h) {
                for x in rx..(rx + rw).min(w) {
                    img.put_pixel(x, y, col);
                }
            }
        }
        img
    }

    /// Helper to dump two overlapping JPEGs for manual / HTTP testing:
    /// `cargo test --release export_test_images -- --ignored`
    #[test]
    #[ignore]
    fn export_test_images() {
        let full = DynamicImage::ImageRgb8(synth(1000, 600));
        full.crop_imm(0, 0, 640, 600).save("/tmp/pano_left.jpg").unwrap();
        full.crop_imm(360, 0, 640, 600).save("/tmp/pano_right.jpg").unwrap();
    }

    #[test]
    fn stitches_two_overlapping_crops() {
        let full = DynamicImage::ImageRgb8(synth(1000, 600));
        // Two crops overlapping by ~280px (x in 360..640).
        let left = full.crop_imm(0, 0, 640, 600);
        let right = full.crop_imm(360, 0, 640, 600);

        let pano = stitch(vec![left, right]).expect("stitch should succeed");
        let (w, h) = (pano.width(), pano.height());

        // The panorama must be meaningfully wider than a single 640px crop
        // (they only overlap 280px, so a correct stitch is ~1000px wide).
        assert!(w >= 900, "panorama too narrow ({w}x{h}); stitch likely misaligned");
        assert!((560..=680).contains(&h), "unexpected panorama height: {h}");
    }
}
