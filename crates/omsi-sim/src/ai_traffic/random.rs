/// A number in 0..1 drawn from `seed` and `n` (the same every time for the same two).
pub(crate) fn draw(seed: u64, n: u64) -> f32 {
    let mut z = seed ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f32 / (1u64 << 53) as f32
}

/// A Gaussian-distributed number with mean value of 0 and standard deviation of 1.
/// Uses Acklam's algorithm (requires logarithms and square roots only in about 4.85% of cases)
/// The same every time for each pair of `seed` and `n`.
pub(crate) fn draw_gaussian(seed: u64, n: u64) -> f32 {
    const A1: f32 = -3.969683028665376e+01;
    const A2: f32 = 2.209460984245205e+02;
    const A3: f32 = -2.759285104469687e+02;
    const A4: f32 = 1.383577518672690e+02;
    const A5: f32 = -3.066479806614716e+01;
    const A6: f32 = 2.506628277459239e+00;
    const B1: f32 = 5.447609879822406e+01;
    const B2: f32 = 1.615858368580409e+02;
    const B3: f32 = -1.556989798598866e+02;
    const B4: f32 = 6.680131188771972e+01;
    const B5: f32 = -1.328068155288572e+01;
    const C1: f32 = -7.784894002430293e-03;
    const C2: f32 = -3.223964580411365e-01;
    const C3: f32 = -2.400758277161838e+00;
    const C4: f32 = -2.549732539343734e+00;
    const C5: f32 = 4.374664141464968e+00;
    const C6: f32 = 2.938163982698783e+00;
    const D1: f32 = 7.784695709041462e-03;
    const D2: f32 = 3.224671290700398e-01;
    const D3: f32 = 2.445134137142996e+00;
    const D4: f32 = 3.754408661907416e+00;

    const LOWER_THRESHOLD: f32 = 0.02425;
    const UPPER_THRESHOLD: f32 = 0.97575;
    let quantile = draw(seed, n);

    match quantile {
        p if (0.0..LOWER_THRESHOLD).contains(&quantile) => {
            let q = (-2.0 * p.ln()).sqrt();
            (((((C1 * q + C2) * q + C3) * q + C4) * q + C5) * q + C6)
                / ((((D1 * q + D2) * q + D3) * q + D4) * q + 1.0)
        }
        p if (LOWER_THRESHOLD..UPPER_THRESHOLD).contains(&quantile) => {
            let q = p - 0.5;
            let r = q * q;
            (((((A1 * r + A2) * r + A3) * r + A4) * r + A5) * r + A6) * q
                / (((((B1 * r + B2) * r + B3) * r + B4) * r + B5) * r + 1.0)
        }
        p if (UPPER_THRESHOLD..=1.0).contains(&quantile) => {
            let q = (-2.0 * (1.0 - p).ln()).sqrt();
            (((((C1 * q + C2) * q + C3) * q + C4) * q + C5) * q + C6)
                / ((((D1 * q + D2) * q + D3) * q + D4) * q + 1.0)
        }
        _ => 0.0,
    }
}

#[test]
fn draws_are_predictable() {
    assert_eq!(draw(42, 1), draw(42, 1));
    assert_ne!(draw(42, 1), draw(43, 1));

    assert_eq!(draw_gaussian(42, 1), draw_gaussian(42, 1));
    assert_ne!(draw_gaussian(42, 1), draw_gaussian(43, 1));
}
