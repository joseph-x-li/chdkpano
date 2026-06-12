//! Feature detection + matching.
//!
//! AKAZE gives us keypoints and 512-bit binary descriptors. We brute-force
//! match descriptors with Hamming distance + Lowe's ratio test. Pure Rust.

use akaze::Akaze;
use bitarray::BitArray;
use image::DynamicImage;

/// Keypoints (image-pixel coords) + their binary descriptors.
pub struct Feats {
    pub pts: Vec<(f64, f64)>,
    pub desc: Vec<BitArray<64>>,
}

/// A matched correspondence: (point in `src`, point in `dst`).
pub type Corr = ((f64, f64), (f64, f64));

/// Detect AKAZE features. Lower threshold = more (but slower) keypoints.
pub fn detect(img: &DynamicImage) -> Feats {
    let akaze = Akaze::new(0.0008);
    let (keypoints, descriptors) = akaze.extract(img);
    let pts = keypoints
        .iter()
        .map(|k| (k.point.0 as f64, k.point.1 as f64))
        .collect();
    Feats { pts, desc: descriptors }
}

/// Hamming distance between two 512-bit descriptors (XOR + popcount).
fn hamming(a: &BitArray<64>, b: &BitArray<64>) -> u32 {
    a.bytes
        .iter()
        .zip(b.bytes.iter())
        .map(|(x, y)| (x ^ y).count_ones())
        .sum()
}

/// Match `src` features against `dst` with a ratio test. Returns the surviving
/// correspondences as (src_point, dst_point) pairs.
pub fn match_features(src: &Feats, dst: &Feats) -> Vec<Corr> {
    let mut out = Vec::new();
    if dst.desc.len() < 2 {
        return out;
    }
    for (i, da) in src.desc.iter().enumerate() {
        // Find the two nearest descriptors in dst.
        let mut best = (usize::MAX, u32::MAX);
        let mut second = u32::MAX;
        for (j, db) in dst.desc.iter().enumerate() {
            let d = hamming(da, db);
            if d < best.1 {
                second = best.1;
                best = (j, d);
            } else if d < second {
                second = d;
            }
        }
        // Lowe's ratio test: keep only confident, unambiguous matches.
        if best.0 != usize::MAX && (best.1 as f32) < 0.8 * (second as f32) {
            out.push((src.pts[i], dst.pts[best.0]));
        }
    }
    out
}
