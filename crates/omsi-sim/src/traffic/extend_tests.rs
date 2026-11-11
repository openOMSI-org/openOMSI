use super::*;

fn straight(x: f64, y0: f64, y1: f64, id: i64, tile: (i32, i32)) -> Lane {
    let mut l = LaneBuilder::polyline(vec![DVec3::new(x, y0, 0.0), DVec3::new(x, y1, 0.0)], LaneKind::Street, 3.0);
    l.key = Some(LaneKey { tile, id, path: 0 });
    l.source = 1;
    l
}

#[test]
fn extend_links_like_link() {
    let all = vec![straight(0.0, 0.0, 100.0, 1, (0, 0)), straight(0.0, 100.0, 200.0, 2, (0, 0)), straight(0.0, 200.0, 300.0, 3, (0, 1)), straight(0.0, 300.0, 400.0, 4, (0, 1))];
    let mut whole = Network { lanes: all.clone(), ..Default::default() };
    whole.link(1.5);
    let mut grown = Network { lanes: all[..2].to_vec(), ..Default::default() };
    grown.link(1.5);
    let r = grown.extend(all[2..].to_vec(), 1.5);
    assert_eq!(r, 2..4);
    for i in 0..4 {
        assert_eq!(grown.lanes[i].next, whole.lanes[i].next, "lane {i}");
        assert_eq!(grown.prev[i], whole.prev[i], "lane {i}");
    }
    assert_eq!(grown.reach, whole.reach);
    assert_eq!(grown.crossings.len(), 4);
    assert_eq!(grown.find(LaneKey { tile: (0, 1), id: 3, path: 0 }, None), Some(2));
    assert_eq!(grown.nearest_lane(DVec3::new(0.5, 350.0, 0.0), LaneKind::Street).map(|n| n.0), Some(3));
}

#[test]
fn links_do_not_skip_signalized_segments() {
    let mut signal = straight(0.0, 10.0, 11.0, 2, (0, 0));
    signal.traffic_light = Some((0, 0));
    let mut net = Network {
        lanes: vec![
            straight(0.0, 0.0, 10.0, 1, (0, 0)),
            signal,
            straight(0.0, 11.0, 21.0, 3, (0, 0)),
        ],
        ..Default::default()
    };
    net.link(1.5);

    assert_eq!(net.lanes[0].next, vec![1]);
    assert_eq!(net.lanes[1].next, vec![2]);
    assert_eq!(net.prev[2], vec![1]);
}

#[test]
fn extending_with_a_signalized_segment_removes_the_existing_bypass_link() {
    let before = vec![
        straight(0.0, 0.0, 10.0, 1, (0, 0)),
        straight(0.0, 11.0, 21.0, 3, (0, 0)),
    ];
    let mut grown = Network {
        lanes: before.clone(),
        ..Default::default()
    };
    grown.link(1.5);
    assert_eq!(grown.lanes[0].next, vec![1]);

    let mut signal = straight(0.0, 10.0, 11.0, 2, (0, 1));
    signal.traffic_light = Some((0, 0));
    grown.extend(vec![signal.clone()], 1.5);

    let mut whole = Network {
        lanes: vec![before[0].clone(), signal, before[1].clone()],
        ..Default::default()
    };
    whole.link(1.5);
    let keys = |net: &Network, lanes: &[usize]| {
        let mut keys: Vec<_> = lanes
            .iter()
            .map(|&i| {
                let key = net.lanes[i].key.unwrap();
                (key.tile, key.id, key.path)
            })
            .collect();
        keys.sort_unstable();
        keys
    };
    for (grown_i, lane) in grown.lanes.iter().enumerate() {
        let whole_i = whole
            .find(lane.key.unwrap(), None)
            .expect("same lane in whole network");
        assert_eq!(
            keys(&grown, &grown.lanes[grown_i].next),
            keys(&whole, &whole.lanes[whole_i].next),
            "next links for {:?}",
            lane.key
        );
        assert_eq!(
            keys(&grown, &grown.prev[grown_i]),
            keys(&whole, &whole.prev[whole_i]),
            "previous links for {:?}",
            lane.key
        );
        assert_eq!(
            grown.reach[grown_i], whole.reach[whole_i],
            "reach for {:?}",
            lane.key
        );
    }
    assert_eq!(grown.lanes[0].next, vec![2]);
    assert_eq!(grown.lanes[2].next, vec![1]);
}

#[test]
fn short_road_choices_progress_without_changing_linking_or_reach() {
    for heading in [0.0, 129.0, 309.0] {
        let mut p = DVec3::new(-1200.0, -600.0, 20.0);
        let mut all = Vec::new();
        for length in [30.0, 0.47, 1.0, 25.0] {
            let lane = LaneBuilder::arc(p, heading, length, 0.0, length * 0.05, LaneKind::Street, 3.0);
            p = lane.end();
            all.push(lane);
        }
        let mut whole = Network { lanes: all.clone(), ..Default::default() };
        whole.link(1.5);
        for i in 0..3 {
            assert!(whole.lanes[i].next.contains(&(i + 1)), "missing forward joint {i}");
            let mut driver = AiState::new(i, 0.0, 7);
            assert!(driver.choose_after(&whole, i).is_some_and(|j| j > i), "backward choice {i}");
        }
        assert!(whole.lanes[3].next.is_empty());
        assert!(whole.lanes[2].next.contains(&1), "original graph must be preserved");
        assert_eq!(whole.reach[0], REACH_MAX);
        // Both new->old and old->new joins must use the same test as link().
        for split in 1..all.len() {
            let mut grown = Network { lanes: all[..split].to_vec(), ..Default::default() };
            grown.link(1.5);
            grown.extend(all[split..].to_vec(), 1.5);
            for i in 0..all.len() {
                let mut got = grown.lanes[i].next.clone();
                let mut want = whole.lanes[i].next.clone();
                got.sort_unstable();
                want.sort_unstable();
                assert_eq!(got, want, "heading {heading}, split {split}, lane {i}");
            }
            assert_eq!(grown.reach, whole.reach);
            let mut driver = AiState::new(2, 0.0, 7);
            assert_eq!(driver.choose_after(&grown, 2), Some(3));
        }
    }
}

#[test]
fn short_forward_paths_keep_gaps_and_overlapping_starts() {
    for heading in [0.0, 129.0, 309.0] {
        let a = LaneBuilder::arc(DVec3::ZERO, heading, 30.0, 0.0, 0.0, LaneKind::Street, 3.0);
        for gap in [-0.4, 0.0, 0.4] {
            let start = a.end() + heading_dir(heading).extend(0.0) * gap;
            let b = LaneBuilder::arc(start, heading, 1.0, 0.0, 0.0, LaneKind::Street, 3.0);
            let mut net = Network { lanes: vec![a.clone(), b], ..Default::default() };
            net.link(1.5);
            assert_eq!(net.lanes[0].next, vec![1], "heading {heading}, gap {gap}");
            let mut driver = AiState::new(0, 0.0, 7);
            assert_eq!(driver.choose_after(&net, 0), Some(1));
        }
        // A sharp, short turn can end behind its entry tangent. When its start
        // is the joint itself, it has not been passed and remains a valid path.
        let turn = LaneBuilder::arc(a.end(), heading, 1.0, 0.3, 0.0, LaneKind::Street, 3.0);
        let mut net = Network { lanes: vec![a, turn], ..Default::default() };
        net.link(1.5);
        assert_eq!(net.lanes[0].next, vec![1], "short turn at heading {heading}");
        let mut driver = AiState::new(0, 0.0, 7);
        assert_eq!(driver.choose_after(&net, 0), Some(1));
    }
}

#[test]
fn short_merge_paths_and_nonroad_connections_keep_their_existing_links() {
    // Two approaches end together. The short approach starts behind the long
    // one's endpoint, but there is no return link and no two-path routing cycle.
    let a = straight(0.0, 0.0, 30.0, 1, (0, 0));
    let b = straight(0.0, 29.0, 29.999, 2, (0, 0));
    let mut net = Network { lanes: vec![a, b], ..Default::default() };
    net.link(1.5);
    assert_eq!(net.lanes[0].next, vec![1]);
    assert!(net.lanes[1].next.is_empty());
    let mut driver = AiState::new(0, 0.0, 7);
    assert_eq!(driver.choose_after(&net, 0), Some(1));

    // Short paths whose ends are near each other but do not have the inverse
    // join also retain their connection.
    let mut net = Network {
        lanes: vec![straight(0.0, 0.0, 1.4, 1, (0, 0)), straight(1.4, 0.9, 1.2, 2, (0, 0))],
        ..Default::default()
    };
    net.link(1.5);
    assert_eq!(net.lanes[0].next, vec![1]);
    assert!(net.lanes[1].next.is_empty());
    let mut driver = AiState::new(0, 0.0, 7);
    assert_eq!(driver.choose_after(&net, 0), Some(1));

    for kind in [LaneKind::Sidewalk, LaneKind::Rail, LaneKind::Air] {
        let mut all = vec![straight(0.0, 0.0, 0.47, 1, (0, 0)), straight(0.0, 0.47, 1.47, 2, (0, 1))];
        for l in &mut all { l.kind = kind; }
        let mut net = Network { lanes: all.clone(), ..Default::default() };
        net.link(1.5);
        assert_eq!(net.lanes[0].next, vec![1]);
        assert_eq!(net.lanes[1].next, vec![0]);
        let mut driver = AiState::new(1, 0.0, 7);
        assert_eq!(driver.choose_after(&net, 1), Some(0));
        let mut grown = Network::default();
        grown.extend(all[..1].to_vec(), 1.5);
        grown.extend(all[1..].to_vec(), 1.5);
        assert_eq!(grown.lanes[0].next, net.lanes[0].next);
        assert_eq!(grown.lanes[1].next, net.lanes[1].next);
    }
}

#[test]
fn start_grid_tracks_added_lanes_in_map_order() {
    let first = vec![straight(0.0, 0.0, 100.0, 1, (0, 0)), straight(300.0, 0.0, 100.0, 2, (1, 0))];
    let mut net = Network { lanes: first, ..Default::default() };
    net.link(1.5);
    let near = |net: &Network| net.lanes_starting_near(DVec3::ZERO, 60.0).into_iter()
        .filter(|&i| net.lanes[i].start().truncate().length() < 60.0).collect::<Vec<_>>();
    assert_eq!(near(&net), vec![0]);
    net.extend(vec![straight(25.0, 0.0, 100.0, 3, (0, 1))], 1.5);
    assert_eq!(near(&net), vec![0, 2]);
}

#[test]
fn added_lanes_only_change_reach_of_their_predecessors() {
    let old = vec![
        straight(0.0, 0.0, 100.0, 1, (0, 0)),
        straight(0.0, 100.0, 200.0, 2, (0, 0)),
        straight(500.0, 0.0, 100.0, 3, (2, 0)),
    ];
    let extra = vec![straight(0.0, 200.0, 300.0, 4, (0, 1))];
    let mut grown = Network { lanes: old.clone(), ..Default::default() };
    grown.link(1.5);
    let distant_reach = grown.reach[2];
    grown.extend(extra.clone(), 1.5);
    let mut whole = Network { lanes: old.into_iter().chain(extra).collect(), ..Default::default() };
    whole.link(1.5);
    assert_eq!(grown.reach, whole.reach);
    assert_eq!(grown.reach[2], distant_reach);
}

#[test]
fn lane_cells_once_each_in_the_order_reached() {
    // (there and back: the cells of the way back are already listed)
    let back = LaneBuilder::polyline(vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 120.0, 0.0), DVec3::new(0.0, 10.0, 0.0)], LaneKind::Street, 3.0);
    assert_eq!(Network::lane_cells(&back), vec![(0, 0), (0, 1), (0, 2)]);
    // a path drawn 1000 km long (GPM's TCSP8.sco has one): every cell once
    let cells = Network::lane_cells(&straight(10.0, 0.0, -1_000_000.0, 1, (0, 0)));
    assert_eq!(cells.len(), 20_001);
    assert_eq!((cells[0], cells[20_000]), ((0, 0), (0, -20_000)));
}
