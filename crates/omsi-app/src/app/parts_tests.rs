use super::*;

#[test]
fn every_group_has_a_part() {
    use crate::frame_stages::Group::*;
    let all = [
        Input,
        Menus,
        Clock,
        Weather,
        World,
        Traffic,
        Player,
        People,
        Duty,
        Net,
        Integrations,
        Sound,
        View,
        Ui,
        Gfx,
        Perf,
    ];
    stage_ctx!(Everything {
        reads: [],
        writes: [
            input,
            menus,
            clock,
            weather,
            world,
            traffic,
            player,
            people,
            duty,
            net,
            integrations,
            sound,
            view,
            ui,
            gfx,
            perf
        ]
    });
    assert_eq!(Everything::WRITES, all);
    assert!(Everything::READS.is_empty());
}
