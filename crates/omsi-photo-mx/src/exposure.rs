//! The camera body of the advanced photo mode: the sensor, the exposure triangle (f-number,
//! shutter time, ISO) in third stops as a camera's dials step them, the exposure modes (the
//! program, aperture or shutter priority, manual) against the light the meter reads, the
//! white balance in kelvin, the picture profiles and the depth of field's limits.
//!
//! The picture the game renders is taken as correctly exposed for the light of the scene
//! (`scene_ev`): a setting that lets in a stop more light than that makes it a stop brighter.

/// Sensors: (name, height mm, the diagonal's crop factor against full frame).
pub const SENSORS: [(&str, f32, f32); 5] = [
    ("Full frame (36 x 24 mm)", 24.0, 1.0),
    ("APS-C (1.5x)", 15.6, 1.54),
    ("Micro Four Thirds (2x)", 13.0, 2.0),
    ("Medium format (44 x 33 mm)", 33.0, 0.79),
    ("1-inch (2.7x)", 8.8, 2.7),
];

/// The f-numbers in third stops.
pub const APERTURES: [f32; 31] = [1.0, 1.1, 1.2, 1.4, 1.6, 1.8, 2.0, 2.2, 2.5, 2.8, 3.2, 3.5, 4.0, 4.5, 5.0, 5.6, 6.3, 7.1, 8.0, 9.0, 10.0, 11.0, 13.0, 14.0, 16.0, 18.0, 20.0, 22.0, 25.0, 29.0, 32.0];

/// Shutter times in third stops (s) and how a camera shows them.
pub const SHUTTERS: [(f32, &str); 55] = [
    (30.0, "30\""), (25.0, "25\""), (20.0, "20\""), (15.0, "15\""), (13.0, "13\""), (10.0, "10\""), (8.0, "8\""), (6.0, "6\""), (5.0, "5\""), (4.0, "4\""), (3.2, "3.2\""), (2.5, "2.5\""), (2.0, "2\""), (1.6, "1.6\""), (1.3, "1.3\""), (1.0, "1\""),
    (0.8, "0.8\""), (0.6, "0.6\""), (0.5, "0.5\""), (0.4, "0.4\""), (0.3, "0.3\""), (1.0 / 4.0, "1/4"), (1.0 / 5.0, "1/5"), (1.0 / 6.0, "1/6"), (1.0 / 8.0, "1/8"), (1.0 / 10.0, "1/10"), (1.0 / 13.0, "1/13"), (1.0 / 15.0, "1/15"), (1.0 / 20.0, "1/20"),
    (1.0 / 25.0, "1/25"), (1.0 / 30.0, "1/30"), (1.0 / 40.0, "1/40"), (1.0 / 50.0, "1/50"), (1.0 / 60.0, "1/60"), (1.0 / 80.0, "1/80"), (1.0 / 100.0, "1/100"), (1.0 / 125.0, "1/125"), (1.0 / 160.0, "1/160"), (1.0 / 200.0, "1/200"),
    (1.0 / 250.0, "1/250"), (1.0 / 320.0, "1/320"), (1.0 / 400.0, "1/400"), (1.0 / 500.0, "1/500"), (1.0 / 640.0, "1/640"), (1.0 / 800.0, "1/800"), (1.0 / 1000.0, "1/1000"), (1.0 / 1250.0, "1/1250"), (1.0 / 1600.0, "1/1600"), (1.0 / 2000.0, "1/2000"),
    (1.0 / 2500.0, "1/2500"), (1.0 / 3200.0, "1/3200"), (1.0 / 4000.0, "1/4000"), (1.0 / 5000.0, "1/5000"), (1.0 / 6400.0, "1/6400"), (1.0 / 8000.0, "1/8000"),
];

/// ISO sensitivities in third stops.
pub const ISOS: [f32; 28] = [50.0, 64.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 320.0, 400.0, 500.0, 640.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3200.0, 4000.0, 5000.0, 6400.0, 8000.0, 10000.0, 12800.0, 16000.0, 25600.0, 51200.0];

/// The exposure modes, as a camera's dial names them.
pub const MODES: [&str; 4] = ["P", "A", "S", "M"];
pub const METERING: [&str; 3] = ["Matrix", "Centre-weighted", "Spot"];

/// The exposure value at ISO 100 of an f-number and a shutter time at an ISO: what light the
/// setting is right for (higher: brighter light).
pub fn ev100(fstop: f32, shutter: f32, iso: f32) -> f32 {
    (fstop * fstop / shutter.max(1e-6)).log2() - (iso / 100.0).log2()
}

fn nearest(list: &[f32], x: f32) -> usize {
    list.iter().enumerate().min_by(|a, b| (a.1.ln() - x.ln()).abs().total_cmp(&(b.1.ln() - x.ln()).abs())).map(|e| e.0).unwrap_or(0)
}

pub fn nearest_aperture(f: f32) -> usize {
    nearest(&APERTURES, f)
}

pub fn nearest_shutter(t: f32) -> usize {
    nearest(&SHUTTERS.map(|s| s.0), t)
}

pub fn nearest_iso(iso: f32) -> usize {
    nearest(&ISOS, iso)
}

/// What the camera is set to (indices into the lists) and how it decides the rest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Exposure {
    pub mode: usize,
    pub aperture: usize,
    pub shutter: usize,
    pub iso: usize,
    pub auto_iso: bool,
    /// Exposure compensation (stops) in the automatic modes.
    pub compensation: f32,
    pub metering: usize,
}

impl Default for Exposure {
    fn default() -> Exposure {
        Exposure { mode: 1, aperture: nearest_aperture(4.0), shutter: nearest_shutter(1.0 / 250.0), iso: nearest_iso(100.0), auto_iso: true, compensation: 0.0, metering: 0 }
    }
}

/// The exposure as the camera takes it: the triangle, and how many stops brighter (positive)
/// or darker the photo comes out than the meter wants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metered {
    pub fstop: f32,
    pub shutter: f32,
    pub iso: f32,
    /// The settings chosen (indices), the automatic ones included.
    pub aperture_ix: usize,
    pub shutter_ix: usize,
    pub iso_ix: usize,
    /// Stops of light against the meter's reading (0 correct; in the automatic modes the
    /// compensation, unless the lens or the shutter ran out of range).
    pub offset: f32,
}

/// The exposure for a meter reading `scene_ev` (EV at ISO 100); `lens` the brightest and
/// darkest f-numbers the lens closes to, `focal_eq` the focal length (full-frame
/// equivalent, mm) for the slowest shutter time held still enough.
pub fn meter(e: &Exposure, scene_ev: f32, lens: (f32, f32), focal_eq: f32) -> Metered {
    let target = scene_ev - if e.mode == 3 { 0.0 } else { e.compensation };
    let mut iso_ix = e.iso.min(ISOS.len() - 1);
    let mut ap_ix = e.aperture.min(APERTURES.len() - 1);
    let mut sh_ix = e.shutter.min(SHUTTERS.len() - 1);
    let (lo, hi) = (nearest_aperture(lens.0), nearest_aperture(lens.1));
    ap_ix = ap_ix.clamp(lo, hi);
    let iso_of = |k: usize| ISOS[k];
    // (the slowest time held by hand: one over the focal length)
    let handheld = 1.0 / focal_eq.max(30.0);
    let solve = |iso_ix: usize, ap_ix: &mut usize, sh_ix: &mut usize| {
        let iso = iso_of(iso_ix);
        // the light the triangle has to let in: Av + Tv = target + log2(ISO / 100)
        let total = target + (iso / 100.0).log2();
        match e.mode {
            // the program: the aperture opens with the dark and closes in the bright
            0 => {
                let av = (total * 0.42 - 0.6).clamp(2.0 * lens.0.log2(), 2.0 * 11f32.log2().min(lens.1.log2()));
                *ap_ix = nearest_aperture(2f32.powf(av * 0.5)).clamp(lo, hi);
                let n = APERTURES[*ap_ix];
                *sh_ix = nearest_shutter(n * n / 2f32.powf(total));
            }
            // aperture priority: the time follows
            1 => {
                let n = APERTURES[*ap_ix];
                *sh_ix = nearest_shutter(n * n / 2f32.powf(total));
            }
            // shutter priority: the aperture follows, within the lens
            2 => {
                let t = SHUTTERS[*sh_ix].0;
                *ap_ix = nearest_aperture((2f32.powf(total) * t).sqrt()).clamp(lo, hi);
            }
            _ => {}
        }
    };
    solve(iso_ix, &mut ap_ix, &mut sh_ix);
    if e.auto_iso {
        if e.mode == 3 {
            // manual with Auto ISO: the sensitivity makes the exposure right
            let need = ev100(APERTURES[ap_ix], SHUTTERS[sh_ix].0, 100.0) - target;
            iso_ix = nearest_iso((100.0 * 2f32.powf(need)).clamp(100.0, 12800.0));
        } else {
            // the slowest time held still: the sensitivity rises before the time grows longer
            iso_ix = nearest_iso(100.0);
            while SHUTTERS[sh_ix].0 > handheld * 1.05 && ISOS[iso_ix] < 12800.0 {
                iso_ix += 1;
                solve(iso_ix, &mut ap_ix, &mut sh_ix);
            }
        }
    }
    let (fstop, shutter, iso) = (APERTURES[ap_ix], SHUTTERS[sh_ix].0, ISOS[iso_ix]);
    let offset = scene_ev - ev100(fstop, shutter, iso);
    Metered { fstop, shutter, iso, aperture_ix: ap_ix, shutter_ix: sh_ix, iso_ix, offset }
}

/// The light of the scene as a meter reads it (EV at ISO 100) from the strength of the sun
/// and the sky (`sun`, `sky`: their light's luminance, 1 at noon in a clear sky) and how
/// much night it is (0 .. 1: the game lifts its night's ambient light to be seen by, a
/// camera does not) - a sunny day is EV 15, a street at night under its lamps some EV 4.
pub fn scene_ev(sun: f32, sky: f32, night: f32) -> f32 {
    let l = (sun * 0.82 + sky * 0.18).max(1e-5);
    (15.0 + l.log2() - 4.5 * night.clamp(0.0, 1.0)).clamp(2.0, 16.0)
}

/// How much the meter's mode changes the reading (stops): the mean brightness (log) of the
/// whole picture against that of what the mode looks at. A spot on a bright sky reads more
/// light (the photo comes out darker); on a dark doorway less.
pub fn metering_shift(mode: usize, luma: &[f32], w: u32, h: u32) -> f32 {
    if mode == 0 || luma.is_empty() {
        return 0.0;
    }
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let (mut all, mut n_all, mut part, mut n_part) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let l = (luma[(y * w + x) as usize].max(1e-4) as f64).ln();
            all += l;
            n_all += 1.0;
            let (dx, dy) = ((x as f32 - cx) / h as f32, (y as f32 - cy) / h as f32);
            let r = (dx * dx + dy * dy).sqrt();
            let k = match mode {
                // centre-weighted: three quarters of the reading from a circle in the middle
                1 => (if r < 0.28 { 3.0 } else { 0.33 }) as f64,
                // spot: some 3 per cent of the frame
                _ => (if r < 0.07 { 1.0 } else { 0.0 }) as f64,
            };
            part += l * k;
            n_part += k;
        }
    }
    if n_part <= 0.0 || n_all <= 0.0 {
        return 0.0;
    }
    ((part / n_part - all / n_all) / std::f64::consts::LN_2) as f32
}

/// White balance presets: (name, kelvin; 0 for the light as rendered).
pub const WHITE_BALANCE: [(&str, f32); 8] = [
    ("As rendered", 0.0),
    ("Daylight (5500 K)", 5500.0),
    ("Cloudy (6500 K)", 6500.0),
    ("Shade (7500 K)", 7500.0),
    ("Tungsten (3200 K)", 3200.0),
    ("Fluorescent (4000 K)", 4000.0),
    ("Flash (5900 K)", 5900.0),
    ("Custom", -1.0),
];

/// The colour of a black body at `k` kelvin, linear sRGB (Tanner Helland's fit of the
/// Planckian locus, decoded from sRGB).
pub fn blackbody(k: f32) -> [f32; 3] {
    let t = k.clamp(1500.0, 15000.0) / 100.0;
    let r = if t <= 66.0 { 255.0 } else { 329.698_73 * (t - 60.0).powf(-0.133_204_76) };
    let g = if t <= 66.0 { 99.470_8 * t.ln() - 161.119_57 } else { 288.122_16 * (t - 60.0).powf(-0.075_514_85) };
    let b = if t >= 66.0 { 255.0 } else if t <= 19.0 { 0.0 } else { 138.517_73 * (t - 10.0).ln() - 305.044_8 };
    let lin = |c: f32| {
        let c = (c / 255.0).clamp(0.0, 1.0);
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    [lin(r), lin(g), lin(b)]
}

/// The gains of a camera set to `k` kelvin under the game's daylight (5500 K, as rendered):
/// set warmer than the light (Cloudy) the photo turns warmer, set to a lamp's (Tungsten)
/// under daylight it turns blue - the brightness kept.
pub fn kelvin_gains(k: f32) -> [f32; 3] {
    if k <= 0.0 {
        return [1.0; 3];
    }
    // (the camera divides out the light it assumes: set to 6500 K it takes the light for
    // bluer than it is and warms it; to 3200 K for redder, and cools it)
    let (assumed, real) = (blackbody(k), blackbody(5500.0));
    let m = [real[0] / assumed[0].max(1e-4), real[1] / assumed[1].max(1e-4), real[2] / assumed[2].max(1e-4)];
    let l = 0.2126 * m[0] + 0.7152 * m[1] + 0.0722 * m[2];
    [m[0] / l, m[1] / l, m[2] / l]
}

/// Picture profiles: (name, contrast, saturation, warmth) laid over the darkroom's settings.
pub const PROFILES: [(&str, f32, f32, f32); 6] = [
    ("Standard", 0.0, 0.0, 0.0),
    ("Neutral", -0.22, -0.15, 0.0),
    ("Landscape", 0.15, 0.25, -0.03),
    ("Portrait", -0.05, -0.06, 0.06),
    ("Vivid", 0.25, 0.35, 0.0),
    ("Flat (for grading)", -0.55, -0.30, 0.0),
];

/// The near and far limits of what is acceptably sharp (m; far infinite past the
/// hyperfocal distance) and the hyperfocal distance, for a real focal length `f_mm`, an
/// f-number, a focus distance and a sensor `sensor_h` mm high (the blur circle allowed: the
/// diagonal over 1500).
pub fn dof_limits(f_mm: f32, fstop: f32, focus: f32, sensor_h: f32) -> (f32, f32, f32) {
    let diag = (sensor_h * sensor_h * (1.0 + 2.25)).sqrt();
    let c = diag / 1500.0;
    let f = f_mm / 1000.0;
    let hyper = f * f / (fstop * c / 1000.0) + f;
    let s = focus.max(f * 1.01);
    let near = s * (hyper - f) / (hyper + s - 2.0 * f);
    let far = if s >= hyper { f32::INFINITY } else { s * (hyper - f) / (hyper - s) };
    (near, far, hyper)
}

/// The diffraction's blur (a Gaussian's sigma, pixels) of an f-number on a sensor `sensor_h`
/// mm high pictured `h_px` pixels high: the Airy disc of green light, 2.44 λ N across.
pub fn diffraction_px(fstop: f32, sensor_h: f32, h_px: f32) -> f32 {
    let airy_um = 2.44 * 0.55 * fstop;
    let pitch_um = sensor_h * 1000.0 / h_px.max(1.0);
    airy_um / pitch_um / 2.9
}

/// A lens as it comes, uncorrected: its distortion (k1: barrel below 0) and how much its
/// corners darken wide open, from its full-frame equivalent focal length and the f-number.
pub fn lens_character(focal_eq: f32, fstop: f32) -> (f32, f32) {
    let k1 = if focal_eq < 50.0 { -0.06 * (50.0 - focal_eq) / 36.0 } else { 0.018 * ((focal_eq - 50.0) / 150.0).min(1.0) };
    let open = (1.0 - (fstop.log2() * 2.0 - 1.0) / 3.0).clamp(0.0, 1.0);
    (k1.max(-0.08), 0.55 * open)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sunny_sixteen() {
        // f/16 at 1/100 s and ISO 100 is right for a sunny day
        assert!((ev100(16.0, 1.0 / 100.0, 100.0) - 14.6).abs() < 0.1);
        // a stop of ISO is a stop of EV
        assert!((ev100(8.0, 1.0 / 125.0, 200.0) - ev100(8.0, 1.0 / 125.0, 100.0) + 1.0).abs() < 1e-4);
    }

    #[test]
    fn the_automatic_modes_meet_the_meter() {
        for mode in 0..3 {
            for ev in [6.0, 10.0, 15.0] {
                let e = Exposure { mode, auto_iso: false, iso: nearest_iso(400.0), ..Exposure::default() };
                let m = meter(&e, ev, (1.4, 22.0), 50.0);
                assert!(m.offset.abs() < 0.4 || (mode == 2 && ev < 8.0), "mode {mode} ev {ev}: {m:?}");
            }
        }
        // compensation brightens
        let e = Exposure { compensation: 1.0, auto_iso: false, ..Exposure::default() };
        assert!((meter(&e, 12.0, (1.4, 22.0), 50.0).offset - 1.0).abs() < 0.4);
    }

    #[test]
    fn manual_shows_the_meter_and_auto_iso_fixes_it() {
        let e = Exposure { mode: 3, aperture: nearest_aperture(8.0), shutter: nearest_shutter(1.0 / 125.0), iso: nearest_iso(100.0), auto_iso: false, ..Exposure::default() };
        // EV 15 light, settings right for EV 13: two stops over
        let m = meter(&e, 15.0, (1.4, 22.0), 50.0);
        assert!((m.offset - 2.0).abs() < 0.2, "{m:?}");
        let m = meter(&Exposure { auto_iso: true, ..e }, 10.0, (1.4, 22.0), 50.0);
        assert!(m.offset.abs() < 0.4 && m.iso > 100.0, "{m:?}");
    }

    #[test]
    fn auto_iso_keeps_the_shutter_hand_held() {
        let e = Exposure { mode: 1, aperture: nearest_aperture(2.8), auto_iso: true, ..Exposure::default() };
        let m = meter(&e, 5.0, (1.4, 22.0), 50.0);
        assert!(m.shutter <= 1.0 / 40.0 && m.iso > 400.0, "{m:?}");
    }

    #[test]
    fn white_balance_goes_the_right_way() {
        let warm = kelvin_gains(7500.0);
        assert!(warm[0] > warm[2], "{warm:?}");
        let cold = kelvin_gains(3200.0);
        assert!(cold[2] > cold[0], "{cold:?}");
        let same = kelvin_gains(5500.0);
        assert!(same.iter().all(|g| (g - 1.0).abs() < 1e-3), "{same:?}");
    }

    #[test]
    fn depth_of_field_widens_closing_down() {
        let (n1, f1, _) = dof_limits(50.0, 2.0, 5.0, 24.0);
        let (n2, f2, _) = dof_limits(50.0, 11.0, 5.0, 24.0);
        assert!(n2 < n1 && f2 > f1, "{n1} {f1} {n2} {f2}");
        // at the hyperfocal distance the far limit is infinite
        let (_, _, h) = dof_limits(24.0, 8.0, 5.0, 24.0);
        assert!(dof_limits(24.0, 8.0, h * 1.01, 24.0).1.is_infinite());
        assert!(h > 2.0 && h < 4.0, "{h}");
    }

    #[test]
    fn spot_metering_on_a_bright_middle_reads_more_light() {
        let (w, h) = (64u32, 48u32);
        let luma: Vec<f32> = (0..w * h).map(|i| { let (x, y) = (i % w, i / w); if (x as i32 - 32).abs() < 4 && (y as i32 - 24).abs() < 4 { 0.9 } else { 0.1 } }).collect();
        assert!(metering_shift(2, &luma, w, h) > 2.0);
        assert_eq!(metering_shift(0, &luma, w, h), 0.0);
    }
}
