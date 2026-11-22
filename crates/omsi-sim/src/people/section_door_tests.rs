//! How the doors of a train's sections open for the people: those of a section that runs
//! its own scripts.

use super::*;

/// A section that runs its own scripts and gives no `PAX_Exit<n>_Open` opens its exit by
/// its own exit leaf, past its front entry leaf (`door_1` for one entry), as a bus's exits
/// are numbered: not by `door_0`, its entry's, so an open entry leaves a shut exit shut.
#[test]
fn a_sections_exit_follows_its_own_exit_leaf() {
    section_doors_follow_their_leaves("door_");
}

/// The same with the leaves spelt `door0`, `door1`, as some cars' scripts name them: the
/// section's own leaves count, not the leading vehicle's doors.
#[test]
fn a_sections_doors_spelt_without_the_underscore_follow_their_leaves() {
    section_doors_follow_their_leaves("door");
}

/// An AI train whose leading car has no door variables, coupled to a section running its own
/// scripts that has them: the section's doors are as its leaves are, at once and after the
/// leading car's doors open for want of anything said (2.5 s at the stop), not with them.
#[test]
fn an_ai_trains_section_keeps_its_own_doors_when_the_leading_car_says_nothing() {
    let (mut v, cabin) = train("door_", false);
    assert!(
        !PeopleSim::reports_doors(&v, cabin.entries.len(), cabin.exits.len()),
        "the leading car says nothing of its doors"
    );
    let run = v.trailers[0].run.as_deref_mut().unwrap();
    assert!(run.set_own("door_0", 1.0) && run.set_own("door_1", 0.0));
    for (since, lead) in [(1.0, false), (3.0, true)] {
        let (entry, exit) = PeopleSim::ai_doors_open(&v, &cabin, since);
        assert_eq!(
            (entry[0], exit[0]),
            (lead, lead),
            "the leading car's after {since} s"
        );
        assert!(
            entry[1],
            "the section's entry, open by its leaf, after {since} s"
        );
        assert!(
            !exit[1],
            "the section's exit, shut by its leaf, after {since} s"
        );
    }
}

/// A train of a car and a section like it that runs its own scripts, each with an entry and
/// an exit, their leaves named `<leaf>0` and `<leaf>1` (the leading car's only where
/// `lead_leaves`), and its cabin.
fn train(leaf: &str, lead_leaves: bool) -> (VehicleInstance, Arc<Cabin>) {
    let dir = std::env::temp_dir().join(format!(
        "omsi-humans-exit-leaf-{}-{}-{}",
        leaf.len(),
        lead_leaves,
        std::process::id()
    ));
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
    write(
        "vars.txt",
        &format!("train_me_reverse\nturned\n{leaf}0\n{leaf}1\n"),
    );
    let lead = std::fs::read_to_string(dir.join("tram.bus")).unwrap();
    write("lead.bus", &lead.replace("vars.txt", "leadvars.txt"));
    write("leadvars.txt", "train_me_reverse\nturned\n");
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
    let lead = if lead_leaves {
        ty.clone()
    } else {
        Arc::new(crate::VehicleType::load(&dir, &dir.join("lead.bus")).unwrap())
    };
    let mut v = VehicleInstance::new(lead, crate::VehicleHost::new(Default::default()));
    v.attach_trailer_ex(ty, false);
    v.update(0.04);
    let mut people = PeopleSim::new(&dir, 0);
    let cabin = people.cabin_for(&v).expect("cabin");
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(cabin.exits[1].part, (1, 0));
    (v, cabin)
}

fn section_doors_follow_their_leaves(leaf: &str) {
    let (mut v, cabin) = train(leaf, true);
    let run = v.trailers[0]
        .run
        .as_deref_mut()
        .expect("the second car runs itself");
    let (entry_leaf, exit_leaf) = (format!("{leaf}0"), format!("{leaf}1"));
    assert!(run.set_own(&entry_leaf, 1.0) && run.set_own(&exit_leaf, 0.0));
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
        .set_own(&exit_leaf, 1.0);
    assert!(
        PeopleSim::doors_open_cabin(&v, &cabin).1[1],
        "and opens with its exit leaf"
    );
}
