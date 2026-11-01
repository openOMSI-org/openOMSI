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
    let quantile = draw(seed, n);
    inverse_cdf(quantile)
}


fn inverse_cdf(quantile: f32) -> f32 {
    const A1: f32 = -3.969683028665376e+01;
    const A2: f32 = 2.209460984245205e+02;
    const A3: f32 = -2.759285104469687e+02;
    const A4: f32 = 1.383577518672690e+02;
    const A5: f32 = -3.066479806614716e+01;
    const A6: f32 = 2.506628277459239e+00;
    const B1: f32 = -5.447609879822406e+01;
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
    const EPSILON_CUTOFF: f32 = 5.1665773;

    match quantile {
        _ if (0.0..f32::EPSILON).contains(&quantile) => -EPSILON_CUTOFF,
        p if (f32::EPSILON..LOWER_THRESHOLD).contains(&quantile) => {
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
        p if (UPPER_THRESHOLD..1.0).contains(&quantile) => {
            let q = (-2.0 * (1.0 - p).ln()).sqrt();
            -(((((C1 * q + C2) * q + C3) * q + C4) * q + C5) * q + C6)
                / ((((D1 * q + D2) * q + D3) * q + D4) * q + 1.0)
        }
        1.0 => EPSILON_CUTOFF,
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

#[test]
fn inverse_cdf_works_as_expected() {
    let seed = 42;
    let mut random_data = vec![];
    const SAMPLE_SIZE: u64 = 10000;
    for n in 0..SAMPLE_SIZE {
        let quantile = draw(seed, n);
        let gaussian = inverse_cdf(quantile);
        // crude symmetry test: we don't need higher accuracy
        assert!((inverse_cdf(1.0 - quantile) + gaussian).abs() < 0.001);
        random_data.push(gaussian);
    }
    let s: f32 = random_data.iter().sum();
    let sample_average = s / SAMPLE_SIZE as f32;
    let q: f32 = random_data.iter().map(|x| (x - sample_average) * (x - sample_average)).sum();
    let sample_variance = q / (SAMPLE_SIZE - 1) as f32;

    // use 95% confidence intervals
    let confidence_threshold = 1.96 / (SAMPLE_SIZE as f32).sqrt();
    assert!((sample_average-confidence_threshold..sample_average+confidence_threshold).contains(&0.0));
    // chi-squared coefficients are hard-coded for n=10000; must be recalculated upon changing SAMPLE_TEST
    let variance_interval = 0.9728*sample_variance..1.0283*sample_variance;
    assert!(variance_interval.contains(&1.0));
}