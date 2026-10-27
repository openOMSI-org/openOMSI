//! The Konstal 105Na consist (jjb's patch over michausto's 105N): every car has a script of
//! its own and they talk through control cables.

use super::*;

/// The content folder with `Vehicles/jjb/Konstal 105Na` (the patch) and
/// `Vehicles/Konstal_105N` (the base pack): `$KONSTAL_ROOT`.
fn konstal_root() -> std::path::PathBuf {
    let root = std::env::var_os("KONSTAL_ROOT").map(std::path::PathBuf::from).expect("set KONSTAL_ROOT to a folder with the Konstal 105Na patch and pack");
    require_content(&[&root.join("Vehicles/jjb/Konstal 105Na/2x105Na_Main.ovh"), &root.join("Vehicles/Konstal_105N")]);
    root
}

/// The two-car train as the game builds it: the head car `2x105Na_Main` and what its
/// `[couple_back]` chain names behind it.
fn konstal_train() -> VehicleInstance {
    let orig = original_root();
    let dir = konstal_root().join("Vehicles/jjb/Konstal 105Na");
    let load = |f: &str| Arc::new(VehicleType::load(&orig, &dir.join(f)).unwrap_or_else(|e| panic!("{f}: {e:#}")));
    let mut host = VehicleHost::new(crate::SimClock::default());
    host.paint_scheme = Some(Some(1));
    let mut v = VehicleInstance::new(load("2x105Na_Main.ovh"), host);
    v.apply_paint_vars(Some(1));
    for part in ["2x105Na_Bogie2.ovh", "2x105Na_Coupling.ovh", "1x105Na_Main.ovh", "1x105Na_Bogie2.ovh"] {
        v.attach_trailer_ex(load(part), false);
    }
    v
}

fn frames(v: &mut VehicleInstance, n: usize) {
    for _ in 0..n {
        v.update(0.02);
    }
}

/// The tail car, the one the script calls `Carriage::Idx` 0.
fn tail(v: &VehicleInstance) -> Tail<'_> {
    Tail(v.trailers.iter().rev().find(|t| t.run_var("Carriage::Idx") == Some(0.0)).expect("the tail car runs its own scripts"))
}

struct Tail<'a>(&'a TrailerPart);

impl Tail<'_> {
    fn var(&self, name: &str) -> Option<f32> {
        self.0.run_var(name)
    }
}

#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT) and the Konstal 105Na (KONSTAL_ROOT)"]
fn the_tail_car_follows_the_head_car_through_the_control_cables() {
    let mut v = konstal_train();
    frames(&mut v, 5);
    // the head car is the driver's: the tail car reads its neighbour's cable, it is no master
    assert_eq!(tail(&v).var("train_frontcoupling"), Some(1.0));
    assert_eq!(v.var("train_frontcoupling"), Some(0.0));
    for (n, x) in [("akumulatory", 1.0), ("sw_Umformer", 1.0), ("kierunek", 1.0), ("sw_Innenbeleuchtung", 1.0)] {
        v.set_var(n, x);
    }
    v.trigger("cp_105Na_Panto");
    frames(&mut v, 400);
    let t = tail(&v);
    assert_eq!(v.var("pantograf"), Some(1.0));
    assert_eq!(t.var("pantograf"), Some(1.0), "the collector goes up on the tail car too");
    assert!(t.var("Panto_Schleifleiste").unwrap() > 2.0, "{:?}", t.var("Panto_Schleifleiste"));
    for n in ["akumulatory", "sw_Umformer", "kierunek", "sw_Innenbeleuchtung"] {
        assert_eq!(t.var(n), Some(1.0), "{n} on the tail car");
    }
    // and it opens its doors with the head car's
    v.set_var("sw_Tuer1", 1.0);
    frames(&mut v, 20);
    assert_eq!(tail(&v).var("sw_Tuer1"), Some(1.0));
}

#[test]
#[ignore = "needs the original OMSI 2 install (OMSI_ROOT) and the Konstal 105Na (KONSTAL_ROOT)"]
fn every_car_gets_the_paint_scheme_variables() {
    let mut v = konstal_train();
    frames(&mut v, 5);
    let t = tail(&v);
    assert_eq!(t.var("Colorscheme"), Some(1.0));
    let set = v.ty.paint_schemes[1].set_vars.clone();
    assert!(!set.is_empty(), "the scheme sets nothing");
    for (name, x) in set {
        // (the tail car's script hides the coupling it has nothing at and works out its own
        // fleet number)
        let low = name.to_ascii_lowercase();
        if low.starts_with("vis_kupplung") || low.starts_with("vis_taborowy") {
            continue;
        }
        assert_eq!(t.var(&name), Some(x), "{name}");
        assert_eq!(v.var(&name), Some(x), "{name}");
    }
}
