use super::*;

#[test]
fn an_answer_to_an_earlier_asking_is_none_of_the_transfer_under_way() {
    // asked for bus 65001, then for 65002: the answers come the other way round
    let (first, mut second) = (ParkedTransfer::ask(65001), ParkedTransfer::ask(65002));
    assert_ne!(first.op, second.op);
    assert_eq!(second.hear(&format!("park-grant 65001 {}", first.op)), Heard::Stale);
    assert_eq!(second.hear(&format!("park-deny 65001 {}", first.op)), Heard::Stale);
    // (the same bus asked for twice: only the later asking's answer counts)
    assert_eq!(second.hear(&format!("park-grant 65002 {}", first.op)), Heard::Stale);
    assert_eq!(second.hear(&format!("park-grant 65002 {}", second.op)), Heard::Grant);
    assert_eq!(second.hear("tp-policy 1"), Heard::Other);
    // granted, and in the world as ours: said to the server, whose word alone ends it
    second.stage = Stage::Granted;
    assert_eq!(second.hear(&format!("park-grant 65002 {}", second.op)), Heard::Stale, "granted once");
    second.took(65001);
    assert_eq!(second.stage, Stage::Granted, "another bus came into the world");
    second.took(65002);
    assert_eq!(second.stage, Stage::Took);
    assert_eq!(second.hear(&format!("park-yours 65001 {}", first.op)), Heard::Stale, "the earlier transfer's last word");
    assert_eq!(second.hear(&format!("park-yours 65002 {}", second.op)), Heard::Yours);
}

#[test]
fn no_other_transfer_starts_while_one_is_past_the_asking() {
    let mut t = ParkedTransfer::ask(65001);
    assert!(!t.holds(), "asked only: another asking takes its place");
    t.stage = Stage::Granted;
    assert!(t.holds(), "the bus is on its way into the world");
    t.took(65001);
    assert!(t.holds(), "ours, and the server has not said so yet");
}

#[test]
fn left_to_the_game_it_draws_the_parked_buses_its_textures_have_room_for() {
    // the budgets measured: 3750 MB (10 buses fit), 2000 MB (5 do, 10 do not), 1000 MB (2 do,
    // 5 do not), 500 MB (2 cost 13 textures a level, the fewest drawn)
    assert_eq!([3_750_000_000, 2_000_000_000, 1_000_000_000, 500_000_000].map(|b| parked_drawn(-1, b)), [10, 5, 2, 2]);
    assert_eq!(parked_drawn(-1, 0), 10, "no budget set");
    // the player's own choice stands, whatever the machine
    assert_eq!([0, 5, 25].map(|n| parked_drawn(n, 460_000_000)), [0, 5, 25]);
}
