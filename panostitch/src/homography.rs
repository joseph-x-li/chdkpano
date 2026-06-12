//! Planar homography estimation: normalized DLT + RANSAC.
//!
//! Given matched point correspondences (some fraction of which are wrong), find
//! the 3x3 homography H that maps src points onto dst points. Pure Rust math via
//! nalgebra; a tiny xorshift PRNG drives RANSAC sampling (deterministic, so a
//! given match set always yields the same H — handy for debugging).

use crate::features::Corr;
use nalgebra::{DMatrix, Matrix3, Vector3};

/// Robustly estimate the homography mapping src -> dst. Returns None if it
/// can't find a model with enough inlier support.
pub fn ransac(matches: &[Corr]) -> Option<Matrix3<f64>> {
    if matches.len() < 4 {
        return None;
    }
    const ITERS: usize = 2000;
    const THRESH: f64 = 3.0; // inlier reprojection error, pixels

    let mut rng = XorShift::new(0x9E3779B97F4A7C15 ^ matches.len() as u64);
    let mut best_inliers: Vec<usize> = Vec::new();

    for _ in 0..ITERS {
        let sample = pick4(matches.len(), &mut rng);
        let corr: Vec<Corr> = sample.iter().map(|&i| matches[i]).collect();
        let Some(h) = dlt(&corr) else { continue };
        let inliers = inliers(matches, &h, THRESH);
        if inliers.len() > best_inliers.len() {
            best_inliers = inliers;
        }
    }

    if best_inliers.len() < 8 {
        return None;
    }
    // Refit on all inliers for a better final estimate.
    let inl: Vec<Corr> = best_inliers.iter().map(|&i| matches[i]).collect();
    dlt(&inl)
}

/// Compute homography from >= 4 correspondences via normalized DLT.
fn dlt(corr: &[Corr]) -> Option<Matrix3<f64>> {
    let n = corr.len();
    if n < 4 {
        return None;
    }
    let src: Vec<(f64, f64)> = corr.iter().map(|c| c.0).collect();
    let dst: Vec<(f64, f64)> = corr.iter().map(|c| c.1).collect();
    let (ts, nsrc) = normalize(&src);
    let (td, ndst) = normalize(&dst);

    // Build the 2n x 9 DLT matrix.
    let mut a = DMatrix::<f64>::zeros(2 * n, 9);
    for i in 0..n {
        let (x, y) = nsrc[i];
        let (xp, yp) = ndst[i];
        a[(2 * i, 0)] = -x;
        a[(2 * i, 1)] = -y;
        a[(2 * i, 2)] = -1.0;
        a[(2 * i, 6)] = xp * x;
        a[(2 * i, 7)] = xp * y;
        a[(2 * i, 8)] = xp;
        a[(2 * i + 1, 3)] = -x;
        a[(2 * i + 1, 4)] = -y;
        a[(2 * i + 1, 5)] = -1.0;
        a[(2 * i + 1, 6)] = yp * x;
        a[(2 * i + 1, 7)] = yp * y;
        a[(2 * i + 1, 8)] = yp;
    }

    // Null-space = right singular vector of the smallest singular value.
    let svd = a.svd(false, true);
    let vt = svd.v_t?;
    let row = vt.row(vt.nrows() - 1);
    let hn = Matrix3::new(
        row[0], row[1], row[2], row[3], row[4], row[5], row[6], row[7], row[8],
    );

    // Denormalize: H = Td^-1 * Hn * Ts
    let td_inv = td.try_inverse()?;
    let mut h = td_inv * hn * ts;
    let s = h[(2, 2)];
    if s.abs() < 1e-12 {
        return None;
    }
    h /= s;
    if h.iter().any(|v| !v.is_finite()) {
        return None;
    }
    Some(h)
}

/// Hartley normalization: centre points at origin, scale mean distance to √2.
fn normalize(pts: &[(f64, f64)]) -> (Matrix3<f64>, Vec<(f64, f64)>) {
    let n = pts.len() as f64;
    let (mut cx, mut cy) = (0.0, 0.0);
    for &(x, y) in pts {
        cx += x;
        cy += y;
    }
    cx /= n;
    cy /= n;
    let mean_d: f64 = pts
        .iter()
        .map(|&(x, y)| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt())
        .sum::<f64>()
        / n;
    let s = if mean_d > 1e-12 { 2.0_f64.sqrt() / mean_d } else { 1.0 };
    let t = Matrix3::new(s, 0.0, -s * cx, 0.0, s, -s * cy, 0.0, 0.0, 1.0);
    let normed = pts.iter().map(|&(x, y)| ((x - cx) * s, (y - cy) * s)).collect();
    (t, normed)
}

/// Project a point through H (perspective divide).
pub fn project(h: &Matrix3<f64>, (x, y): (f64, f64)) -> (f64, f64) {
    let p = h * Vector3::new(x, y, 1.0);
    (p[0] / p[2], p[1] / p[2])
}

fn inliers(matches: &[Corr], h: &Matrix3<f64>, thresh: f64) -> Vec<usize> {
    let t2 = thresh * thresh;
    matches
        .iter()
        .enumerate()
        .filter_map(|(i, &(s, d))| {
            let (px, py) = project(h, s);
            let e2 = (px - d.0).powi(2) + (py - d.1).powi(2);
            if e2.is_finite() && e2 < t2 {
                Some(i)
            } else {
                None
            }
        })
        .collect()
}

/// Pick 4 distinct indices in `[0, n)`.
fn pick4(n: usize, rng: &mut XorShift) -> [usize; 4] {
    let mut out = [0usize; 4];
    let mut count = 0;
    while count < 4 {
        let c = (rng.next() % n as u64) as usize;
        if !out[..count].contains(&c) {
            out[count] = c;
            count += 1;
        }
    }
    out
}

/// Minimal xorshift64 PRNG — avoids pulling in `rand` (and its version churn).
struct XorShift(u64);
impl XorShift {
    fn new(seed: u64) -> Self {
        XorShift(seed | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}
