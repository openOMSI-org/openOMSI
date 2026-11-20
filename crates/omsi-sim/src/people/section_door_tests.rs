//! How the doors of a train's sections open for the people: those of a section that runs
//! its own scripts.

use super::*;

/// A section that runs its own scripts and gives no `PAX_Exit<n>_Open` opens its exit by
/// its own exit leaf, past its front entry leaf (`door_1` for one entry), as a bus's exits
/// are numbered: not by `door_0`, its entry's, so an open entry leaves a shut exit shut.
#[test]
fn a_sections_exit_follows_its_own_exit_leaf() {
    let dir = std::env::temp_dir().join(format!("omsi-humans-exit-leaf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let write = |name: &str, text: &str| std::fs::write(dir.join(name), text).unwrap();
    write(
        "tram.bus",
        concat!(
            "[model]\nmodel.cfg\n[varnamelist]\n1\nvars.txt\n[script]\n1\nmain.osc\n",
            "[passengercabin]\ncabin.cfg\n[paths]\npaths.cfg\n",
            "[coupling_front]\n0\n7.5\n0.3\n\n[coupling_back]\n0\n-7.5\n0.3\n",
        ),
    );
    write("model.cfg", "");
    write("vars.txt", "train_me_reverse\nturned\ndoor_0\ndoor_1\n");
    // (reads its place in the train, so it runs its own scripts)
    write(
        "main.osc",
        "{frame}\n(L.L.train_me_reverse) (S.L.turned)\n{end}\n",
    );
    write(
        "paths.cfg",
        "[pathpnt]\n1.2\n3\n0.4\n[pathpnt]\n1.2\n-3\n0.4\n[pathlink]\n0\n1\n",
    );
    write("cabin.cfg", "[entry]\n0\n[exit]\n1\n");
    let ty = Arc::new(crate::VehicleType::load(&dir, &dir.join("tram.bus")).unwrap());
    let mut v = VehicleInstance::new(ty.clone(), crate::VehicleHost::new(Default::default()));
    v.attach_trailer_ex(ty, false);
    v.update(0.04);
    let mut people = PeopleSim::new(&dir, 0);
    let cabin = people.cabin_for(&v).expect("cabin");
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(cabin.exits[1].part, (1, 0));
    let run = v.trailers[0]
        .run
        .as_deref_mut()
        .expect("the second car runs itself");
    assert!(run.set_own("door_0", 1.0) && run.set_own("door_1", 0.0));
    let (entry, exit) = PeopleSim::doors_open_cabin(&v, &cabin);
    assert!(entry[1], "its entry leaf is open");
    assert!(
        !exit[1],
        "its exit leaf is shut, though the entry's is open"
    );
    v.trailers[0]
        .run
        .as_deref_mut()
        .unwrap()
        .set_own("door_1", 1.0);
    assert!(
        PeopleSim::doors_open_cabin(&v, &cabin).1[1],
        "and opens with its exit leaf"
    );
}
