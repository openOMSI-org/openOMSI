//! The two-axle "Oldtimer-Straßenbahn" add-on: its map tells the tram everything through codes
//! on `[third_rail]`s under its two contact shoes (tunnels, slopes, the overhead conductor rail,
//! section breaks, points), and its one `sound.osc` squeals in bends by the head car's bogies.

use super::*;

fn tram() -> VehicleInstance {
    let root = original_root();
    let bus = root.join("Vehicles/Tram_Zweiachser/TW.bus");
    require_content(&[&bus]);
    let ty = Arc::new(VehicleType::load(&root, &bus).unwrap_or_else(|e| panic!("TW.bus: {e:#}")));
    let mut v = VehicleInstance::new(ty, VehicleHost::new(crate::SimClock::default()));
    for _ in 0..10 {
        v.update(0.02);
    }
    v
}

fn frames(v: &mut VehicleInstance, n: usize, at: &[BogieGround; 2]) {
    for _ in 0..n {
        v.feel_track(at);
        v.update(0.02);
    }
}

/// A rail of `code`: at the height of the first shoe (`low`) or the second.
fn rail(code: f32, low: bool) -> [f32; 6] {
    [0.0, if low { 0.3 } else { 0.7 }, 3.0, code, 0.0, 0.05]
}

#[test]
#[ignore = "needs the Oldtimer-Strassenbahn add-on (OMSI_ROOT)"]
fn it_squeals_in_a_tight_bend_only() {
    let mut v = tram();
    let on = |k: f32| [BogieGround::new(k, &[], true); 2];
    frames(&mut v, 200, &on(1.0 / 25.0));
    let bend = v.var("rad_am_limit_traege").unwrap();
    assert!(bend >= 10.0, "full volume in a 25 m bend: {bend}");
    frames(&mut v, 200, &on(0.0));
    // (it dies away over seconds; the sound starts at 3)
    assert!(
        v.var("rad_am_limit_traege").unwrap() < 3.0,
        "silent on the straight"
    );
    frames(&mut v, 200, &on(1.0 / 400.0));
    assert!(
        v.var("rad_am_limit_traege").unwrap() < 3.0,
        "a wide bend stays under the sound's first point"
    );
}

/// A world of a road at 0 and whatever else lies over it.
struct World {
    /// Faces over the road: (height, whether a wall's top the wheels' probe lists apart).
    faces: Vec<(f64, bool)>,
}

impl crate::rigid::Ground for World {
    fn probe(&self, _x: f64, _y: f64, top: f64) -> crate::rigid::GroundProbe {
        let road = self
            .faces
            .iter()
            .filter(|f| !f.1)
            .map(|f| f.0)
            .chain([0.0])
            .filter(|z| *z <= top)
            .fold(0.0, f64::max);
        // (the lowest face above `top`, and a wall's top wherever it stands)
        let above = self
            .faces
            .iter()
            .filter(|f| f.1 || f.0 > top)
            .map(|f| f.0)
            .fold(None, |a: Option<f64>, z| {
                Some(a.map_or(z, |a: f64| a.min(z)))
            });
        crate::rigid::GroundProbe {
            below: Some(road),
            above,
        }
    }
}

/// The pantograph's height under `faces`: its script's `6 - GetHeightAbovePoint`.
fn pantograph_under(faces: Vec<(f64, bool)>) -> f32 {
    let mut v = tram();
    v.contact = Some(Arc::new(World { faces }));
    v.set_var("buegelleine_1_soll", 1.0);
    v.set_var("buegelleine_2_soll", 1.0);
    let g = BogieGround::new(0.0, &[], true);
    frames(&mut v, 5, &[g, g]);
    v.var("stromabnehmer_hoehe_soll").unwrap()
}

#[test]
#[ignore = "needs the Oldtimer-Strassenbahn add-on (OMSI_ROOT)"]
fn the_pantograph_reads_what_is_over_the_tram() {
    assert_eq!(
        pantograph_under(vec![]),
        5.0,
        "the open street: the script's own height"
    );
    assert!(
        (pantograph_under(vec![(4.6, false)]) - 4.6).abs() < 0.05,
        "a low bridge"
    );
    assert!(
        (pantograph_under(vec![(5.5, true)]) - 5.5).abs() < 0.05,
        "a wire on a narrow height profile"
    );
    assert_eq!(
        pantograph_under(vec![(8.0, false)]),
        6.0,
        "2 m over the point, within the ray: the highest it goes"
    );
    assert_eq!(
        pantograph_under(vec![(9.5, false)]),
        5.0,
        "over the ray's start, as if not there"
    );
}

#[test]
#[ignore = "needs the Oldtimer-Strassenbahn add-on (OMSI_ROOT)"]
fn the_rails_tell_it_where_it_is() {
    let mut v = tram();
    let on = |rails: &[[f32; 6]], f: &mut dyn FnMut(&[BogieGround; 2])| {
        let g = BogieGround::new(0.0, rails, true);
        f(&[g, g]);
    };
    on(&[rail(9001.0, true), rail(8100.0, false)], &mut |g| {
        frames(&mut v, 5, g)
    });
    assert_eq!(
        v.var("Tunnel"),
        Some(1.0),
        "the first shoe: into the tunnel"
    );
    assert_eq!(
        v.var("weichenrumpeln"),
        Some(1.0),
        "the second shoe: points"
    );
    on(&[rail(9000.0, true)], &mut |g| frames(&mut v, 5, g));
    assert_eq!(
        v.var("Tunnel"),
        Some(0.0),
        "out of it, and the flag stays until told"
    );
    assert_eq!(v.var("weichenrumpeln"), Some(0.0));
    on(&[rail(8005.0, false)], &mut |g| frames(&mut v, 5, g));
    assert_eq!(v.var("steigung"), Some(5.0), "a slope");
    on(&[rail(9040.0, true)], &mut |g| frames(&mut v, 5, g));
    assert_eq!(
        v.var("stromabnehmer_hoehe_fix"),
        Some(1.0),
        "the overhead conductor rail holds the pantograph"
    );
    on(&[rail(9030.0, true)], &mut |g| frames(&mut v, 5, g));
    assert_eq!(v.var("trenner"), Some(1.0), "a section break");
    assert_eq!(v.var("Netzspannung"), Some(0.0), "no current over it");
}
