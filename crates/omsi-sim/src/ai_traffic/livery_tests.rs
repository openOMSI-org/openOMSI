//! The player's own livery is left to the player: no AI bus of the same vehicle file draws
//! its paint, and no depot bus of the ailists line that is the player's is put out as the
//! player's bus a second time (#3477).

use super::scenario_tests::{Fixture, traffic};
use super::*;
use crate::livery::PlayerLivery;
use crate::vehicle::PaintScheme;

/// The fixture's vehicle with two paint schemes and the given default paint.
fn painted(f: &Fixture, default_paint: &str) -> Arc<VehicleType> {
    let mut ty = (*f.ty).clone();
    ty.def.default_paint = default_paint.to_string();
    ty.paint_schemes = vec![
        PaintScheme {
            name: "BVG Gelb".into(),
            ..Default::default()
        },
        PaintScheme {
            name: "BVG Rot".into(),
            ..Default::default()
        },
    ];
    Arc::new(ty)
}

fn livery_of(ty: &VehicleType, paint: &str, number: &str, ident: &str) -> PlayerLivery {
    PlayerLivery {
        bus: ty.def.path.clone(),
        paint: paint.into(),
        number: number.into(),
        ident: ident.into(),
    }
}

#[test]
fn the_random_traffic_draws_another_paint_than_the_players() {
    let f = Fixture::new();
    let ty = painted(&f, "BVG Gelb");
    let mut t = traffic(&f, Network::default());
    // (the player drives a bus of this very file in its yellow)
    t.player_livery = Some(livery_of(&ty, "BVG Gelb", "1234", "B-V 1234"));
    let mut painted_yellow = 0;
    for pick in 0..40u64 {
        let scheme = t.ai_scheme(&ty, pick).expect("a paint of the two");
        if ty.paint_schemes[scheme].name == "BVG Gelb" {
            painted_yellow += 1;
        }
    }
    assert_eq!(
        painted_yellow, 0,
        "the player's paint drawn by the random traffic"
    );
    // and the other one is still drawn: the traffic keeps its variety
    assert!(
        (0..40u64).any(|pick| ty.paint_schemes[t.ai_scheme(&ty, pick).unwrap()].name == "BVG Rot")
    );
}

#[test]
fn a_vehicle_the_player_does_not_drive_keeps_every_paint() {
    let f = Fixture::new();
    let ty = painted(&f, "BVG Gelb");
    let other = painted(&f, "BVG Gelb");
    let mut other = (*other).clone();
    other.def.path = f.dir.join("other.bus");
    let other = Arc::new(other);
    let mut t = traffic(&f, Network::default());
    t.player_livery = Some(livery_of(&ty, "BVG Gelb", "1234", "B-V 1234"));
    let drawn: Vec<usize> = (0..40u64)
        .map(|pick| t.ai_scheme(&other, pick).unwrap())
        .collect();
    assert!(
        drawn.contains(&0) && drawn.contains(&1),
        "an AI type of its own lost a paint: {drawn:?}"
    );
}

#[test]
fn the_draw_keeps_its_own_where_the_traffic_has_nothing_else() {
    let f = Fixture::new();
    let ty = painted(&f, "BVG Gelb");
    let mut t = traffic(&f, Network::default());
    // (the player drives the red one: every draw lands on the yellow, the paint it has left)
    t.player_livery = Some(livery_of(&ty, "BVG Rot", "1234", "B-V 1234"));
    for pick in 0..40u64 {
        assert_eq!(
            ty.paint_schemes[t.ai_scheme(&ty, pick).unwrap()].name,
            "BVG Gelb"
        );
    }
    // (and a type of one paint only, the player's own, keeps it: one AI bus of that paint is
    // better than none of that type at all)
    let g = Fixture::new();
    let mut one = (*g.ty).clone();
    one.paint_schemes = vec![PaintScheme {
        name: "BVG Gelb".into(),
        ..Default::default()
    }];
    let one = Arc::new(one);
    let mut t = traffic(&g, Network::default());
    t.player_livery = Some(livery_of(&one, "BVG Gelb", "1234", "B-V 1234"));
    assert_eq!(t.ai_scheme(&one, 0), Some(0));
}
