//! Reusable grouping and search logic for settings interfaces.

/// A row of a settings window, taken apart.
pub struct Row<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub value: &'a str,
    pub desc: &'a str,
    pub frac: Option<f32>,
}

pub fn parse_row(label: &str) -> Row<'_> {
    let mut p = label.split('\u{1f}');
    let name = p.next().unwrap_or("");
    let kind = p.next().unwrap_or("a");
    let value = p.next().unwrap_or("");
    let desc = p.next().unwrap_or("");
    let frac = p.next().and_then(|x| x.parse().ok());
    Row {
        name,
        kind,
        value,
        desc,
        frac,
    }
}

#[derive(Default)]
pub struct SearchState {
    pub page: String,
    pub query: String,
    pub visible: Vec<usize>,
}

pub fn search_key(
    ui: &mut omsi_ui::kit::Ui,
    field: &str,
    code: winit::keyboard::KeyCode,
    text: Option<&str>,
) -> bool {
    use omsi_ui::kit::Key;
    use winit::keyboard::KeyCode as K;
    if ui.focus != Some(omsi_ui::kit::id_of(field)) {
        return false;
    }
    let key = match code {
        K::ArrowLeft => Some(Key::Left),
        K::ArrowRight => Some(Key::Right),
        K::Home => Some(Key::Home),
        K::End => Some(Key::End),
        K::Backspace => Some(Key::Backspace),
        K::Delete => Some(Key::Delete),
        K::Enter | K::NumpadEnter | K::Tab | K::Escape => {
            ui.focus = None;
            return true;
        }
        K::KeyA if ui.input.ctrl => Some(Key::SelectAll),
        _ => None,
    };
    if let Some(key) = key {
        ui.input.keys.push(key);
    } else if !ui.input.ctrl && !ui.input.alt {
        if let Some(text) = text {
            ui.input
                .text
                .extend(text.chars().filter(|c| !c.is_control()));
        }
    }
    true
}

pub fn toolbar_layout(area: omsi_ui::Rect, body: omsi_ui::Rect) -> (omsi_ui::Rect, f32) {
    if area.w >= 760.0 {
        let width = (area.w * 0.34).clamp(220.0, 360.0);
        (
            omsi_ui::Rect::new(area.right() - width - 44.0, area.y + 4.0, width, 36.0),
            0.0,
        )
    } else {
        (
            omsi_ui::Rect::new(body.x, body.y, (body.w - 44.0).max(40.0), 36.0),
            48.0,
        )
    }
}

pub fn option_sections(
    title: &str,
) -> Option<(&'static str, &'static [(&'static str, &'static str)])> {
    Some(match title {
        "Gameplay" => (
            "Navigation and map",
            &[
                ("get_up", "Driving and collisions"),
                ("timetable_win", "Journey and information"),
                ("exact_fare", "Passengers and fares"),
                ("maintenance", "Traffic and people"),
            ],
        ),
        "Graphics" => (
            "Quality and presets",
            &[
                ("ray_tracing", "Lighting and shadows"),
                ("shadows", "Lighting and shadows"),
                ("shadow_size", "Lighting and shadows"),
                ("reflections", "Lighting and shadows"),
                ("clouds", "Sky and atmosphere"),
                ("light_shadows", "Light effects"),
                ("rain_quality", "Rain and road effects"),
                ("vehicle_smoke", "Environment effects"),
            ],
        ),
        "Display and memory" => (
            "Screen and output",
            &[
                ("vsync", "Frame rate and performance"),
                ("view_distance", "Scene detail"),
                ("mirror_size", "Mirrors"),
                ("video_memory", "Memory"),
            ],
        ),
        "Sound" => (
            "Game and surroundings",
            &[("voice_enabled", "Voice chat"), ("milezero", "Radio")],
        ),
        "Camera" => (
            "View movement",
            &[
                ("headtrack", "Head tracking"),
                ("look_sens", "Looking and zoom"),
                ("fov", "Field of view and seating"),
                ("mirror_hud", "Mirror displays"),
            ],
        ),
        "Controls" => (
            "Mouse driving",
            &[
                ("steering_linear", "Keyboard steering"),
                ("ff", "Force feedback and wheel"),
                ("pedal_t", "Pedals and vehicle controls"),
                ("auto_clutch", "Gearbox"),
            ],
        ),
        "Interface" => (
            "Language and appearance",
            &[
                ("ui_scale", "Interface size"),
                ("tooltips", "Hints and notifications"),
                ("chat", "Multiplayer interface"),
            ],
        ),
        "VR" => ("Navigator placement", &[("vr_nav_opacity", "Appearance")]),
        _ => return None,
    })
}

pub fn sectioned_rows(
    rows: Vec<(String, String)>,
    first: &str,
    starts: &[(&str, &str)],
    adjust: &str,
) -> Vec<(Option<String>, (String, String))> {
    let mut out = Vec::new();
    let mut current = None;
    for (label, action) in rows {
        let key = action.strip_prefix("sel ").unwrap_or(&action);
        let key = key.strip_suffix(adjust).unwrap_or(key);
        let title = starts
            .iter()
            .find(|(id, _)| *id == key)
            .map(|(_, title)| *title)
            .or_else(|| out.is_empty().then_some(first));
        let heading = title.filter(|title| current != Some(*title));
        if let Some(title) = heading {
            current = Some(title);
        }
        out.push((heading.map(str::to_owned), (label, action)));
    }
    out
}

pub fn settings_groups(
    items: &[(&str, &str)],
    heading: &str,
    excluded: Option<usize>,
) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (index, &(id, _)) in items.iter().enumerate() {
        if excluded == Some(index) {
            continue;
        }
        if groups.is_empty() || id == heading {
            groups.push(Vec::new());
        }
        groups.last_mut().unwrap().push(index);
    }
    groups
}

pub fn settings_columns(
    groups: Vec<Vec<usize>>,
    two: bool,
    mut row_height: impl FnMut(usize) -> f32,
) -> Vec<Vec<Vec<usize>>> {
    if !two || groups.is_empty() {
        return vec![groups];
    }
    if groups.len() == 1 {
        let rows = &groups[0];
        if rows.len() < 2 {
            return vec![groups];
        }
        let heights: Vec<_> = rows.iter().map(|&index| row_height(index)).collect();
        let half = heights.iter().sum::<f32>() * 0.5;
        let mut height = 0.0;
        let mut split = rows.len() - 1;
        for (index, row) in heights.iter().enumerate() {
            if index > 0 && height >= half {
                split = index;
                break;
            }
            height += row;
        }
        return vec![vec![rows[..split].to_vec()], vec![rows[split..].to_vec()]];
    }
    let heights: Vec<f32> = groups
        .iter()
        .map(|group| group.iter().map(|&index| row_height(index)).sum::<f32>() + 40.0)
        .collect();
    let total: f32 = heights.iter().sum();
    let split = (1..groups.len())
        .min_by(|&a, &b| {
            let left_a: f32 = heights[..a].iter().sum();
            let left_b: f32 = heights[..b].iter().sum();
            (left_a - total * 0.5)
                .abs()
                .total_cmp(&(left_b - total * 0.5).abs())
        })
        .unwrap();
    vec![groups[..split].to_vec(), groups[split..].to_vec()]
}

pub fn filter_groups(
    items: &[(&str, &str)],
    heading: &str,
    excluded: Option<usize>,
    query: &str,
) -> Vec<Vec<usize>> {
    let query = query.trim().to_lowercase();
    let mut groups = settings_groups(items, heading, excluded);
    if query.is_empty() {
        return groups;
    }
    for group in &mut groups {
        group.retain(|&index| {
            if items[index].0 == heading {
                return true;
            }
            let row = parse_row(items[index].1);
            omsi_ui::kit::matches(row.name, &query) || omsi_ui::kit::matches(row.desc, &query)
        });
    }
    groups.retain(|group| group.iter().any(|&index| items[index].0 != heading));
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_keys_preserve_text_modifiers_editing_and_focus() {
        use omsi_ui::kit::{Key, Ui, id_of};
        use winit::keyboard::KeyCode as K;
        let mut ui = Ui::new();
        assert!(!search_key(&mut ui, "search", K::KeyE, Some("é")));
        ui.focus = Some(id_of("search"));
        assert!(search_key(&mut ui, "search", K::KeyE, Some("é\n")));
        assert_eq!(ui.input.text, "é");
        ui.input.ctrl = true;
        assert!(search_key(&mut ui, "search", K::KeyA, Some("a")));
        assert_eq!(ui.input.keys, [Key::SelectAll]);
        assert_eq!(ui.input.text, "é");
        ui.input.ctrl = false;
        ui.input.alt = true;
        assert!(search_key(&mut ui, "search", K::KeyQ, Some("q")));
        assert_eq!(ui.input.text, "é");
        assert!(search_key(&mut ui, "search", K::Backspace, None));
        assert_eq!(ui.input.keys, [Key::SelectAll, Key::Backspace]);
        for code in [K::Escape, K::Enter, K::NumpadEnter, K::Tab] {
            ui.focus = Some(id_of("search"));
            assert!(search_key(&mut ui, "search", code, None));
            assert_eq!(ui.focus, None);
        }
    }

    #[test]
    fn settings_rows_parse_optional_fields_without_changing_defaults() {
        let row = parse_row("Volume\u{1f}v\u{1f}50%\u{1f}Engine sound\u{1f}0.5");
        assert_eq!(
            (row.name, row.kind, row.value, row.desc, row.frac),
            ("Volume", "v", "50%", "Engine sound", Some(0.5))
        );
        let row = parse_row("Heading");
        assert_eq!(
            (row.name, row.kind, row.value, row.desc, row.frac),
            ("Heading", "a", "", "", None)
        );
        assert_eq!(
            parse_row("A\u{1f}v\u{1f}1\u{1f}desc\u{1f}invalid").frac,
            None
        );
    }

    #[test]
    fn search_toolbar_moves_below_the_title_on_narrow_surfaces() {
        let wide = toolbar_layout(
            omsi_ui::Rect::new(300.0, 28.0, 1100.0, 800.0),
            omsi_ui::Rect::new(300.0, 98.0, 1100.0, 700.0),
        );
        assert_eq!(wide.0.y, 32.0);
        assert_eq!(wide.0.right(), 1356.0);
        assert_eq!(wide.1, 0.0);
        let narrow = toolbar_layout(
            omsi_ui::Rect::new(0.0, 28.0, 420.0, 700.0),
            omsi_ui::Rect::new(0.0, 98.0, 420.0, 600.0),
        );
        assert_eq!(narrow.0.y, 98.0);
        assert_eq!(narrow.1, 48.0);
    }

    #[test]
    fn gameplay_sections_skip_absent_options_and_keep_order() {
        let rows: Vec<(String, String)> = [
            ("Navigator", "navigator"),
            ("Get up", "get_up"),
            ("Exact fares", "exact_fare"),
            ("Passenger connections", "passenger_connections"),
            ("Maintenance", "sel maintenance"),
            ("Random traffic", "ai_unsched_factor"),
        ]
        .into_iter()
        .map(|(label, action)| (label.into(), action.into()))
        .collect();
        let (first, starts) = option_sections("Gameplay").unwrap();
        let grouped = sectioned_rows(rows.clone(), first, starts, "##adjust");
        assert_eq!(
            grouped
                .iter()
                .filter_map(|(heading, _)| heading.as_deref())
                .collect::<Vec<_>>(),
            [
                "Navigation and map",
                "Driving and collisions",
                "Passengers and fares",
                "Traffic and people"
            ]
        );
        assert_eq!(
            grouped.into_iter().map(|(_, row)| row).collect::<Vec<_>>(),
            rows
        );
        assert!(option_sections("Unknown").is_none());
    }

    #[test]
    fn graphics_lighting_has_one_card_with_mode_dependent_rows() {
        let (first, starts) = option_sections("Graphics").unwrap();
        for (visible, enhanced) in [
            (vec!["reflections"], false),
            (vec!["shadows", "shadow_size", "reflections"], false),
            (
                vec!["ray_tracing", "shadows", "shadow_size", "reflections"],
                true,
            ),
            (vec!["ray_tracing", "shadow_size"], true),
        ] {
            let actions: Vec<_> = ["graphics"]
                .into_iter()
                .chain(visible)
                .chain(["clouds"])
                .chain(
                    enhanced
                        .then_some(["light_shadows", "cloud_shadows", "light_glow"])
                        .into_iter()
                        .flatten(),
                )
                .chain([
                    "rain_quality",
                    "vehicle_smoke",
                    "puddles",
                    "windy_trees",
                    "road_snow",
                ])
                .map(|key| (key.to_string(), key.to_string()))
                .collect();
            let grouped = sectioned_rows(actions.clone(), first, starts, "##adjust");
            let mut expected = vec![
                "Quality and presets",
                "Lighting and shadows",
                "Sky and atmosphere",
            ];
            if enhanced {
                expected.push("Light effects");
            }
            expected.extend(["Rain and road effects", "Environment effects"]);
            assert_eq!(
                grouped
                    .iter()
                    .filter_map(|(title, _)| title.as_deref())
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(
                grouped.into_iter().map(|(_, row)| row).collect::<Vec<_>>(),
                actions
            );
        }
    }

    #[test]
    fn sectioned_rows_preserve_actions_and_adjustable_option_groups() {
        let rows = vec![
            ("A".into(), "a".into()),
            ("B".into(), "sel b".into()),
            ("Wind".into(), "wind_speed##adjust".into()),
        ];
        let grouped = sectioned_rows(
            rows.clone(),
            "First",
            &[("b", "Second"), ("wind_speed", "Weather")],
            "##adjust",
        );
        assert_eq!(
            grouped
                .iter()
                .map(|(_, row)| row.clone())
                .collect::<Vec<_>>(),
            rows
        );
        assert_eq!(
            grouped
                .iter()
                .filter_map(|(heading, _)| heading.as_deref())
                .collect::<Vec<_>>(),
            ["First", "Second", "Weather"]
        );
    }

    #[test]
    fn sectioned_rows_omit_missing_groups_and_handle_empty_input() {
        assert!(sectioned_rows(Vec::new(), "First", &[], "##adjust").is_empty());
        let rows = vec![("Wind".into(), "wind_speed##adjust".into())];
        assert_eq!(
            sectioned_rows(rows.clone(), "First", &[("wind_speed", "Wind")], "##adjust"),
            vec![(Some("Wind".into()), rows[0].clone())]
        );
    }

    #[test]
    fn filtered_search_excludes_back_without_reindexing() {
        let rows = [
            ("#", "Navigation"),
            ("back", "Back"),
            ("navigator", "Navigator"),
        ];
        assert_eq!(filter_groups(&rows, "#", Some(1), ""), vec![vec![0, 2]]);
        assert!(filter_groups(&rows, "#", Some(1), "back").is_empty());
        assert_eq!(
            filter_groups(&rows, "#", Some(1), "navigator"),
            vec![vec![0, 2]]
        );
        assert_eq!(filter_groups(&rows, "#", None, "back"), vec![vec![0, 1]]);
    }

    #[test]
    fn search_preserves_original_indices_and_drops_empty_groups() {
        let rows = [
            ("#", "Map"),
            ("a", "Navigator\u{1f}s\u{1f}on\u{1f}Route map"),
            ("#", "Audio"),
            ("b", "Volume\u{1f}v\u{1f}50%\u{1f}Engine sound"),
        ];
        assert_eq!(
            filter_groups(&rows, "#", None, "NAVIGATOR"),
            vec![vec![0, 1]]
        );
        assert_eq!(filter_groups(&rows, "#", None, "engine"), vec![vec![2, 3]]);
        assert!(filter_groups(&rows, "#", None, "missing").is_empty());
        assert_eq!(
            filter_groups(&rows, "#", None, "  "),
            vec![vec![0, 1], vec![2, 3]]
        );
    }

    #[test]
    fn long_unheaded_page_splits_rows_by_height_without_reindexing() {
        let groups = vec![vec![0, 2, 3, 4, 6, 7, 8, 9]];
        let columns = settings_columns(groups.clone(), true, |i| if i < 4 { 80.0 } else { 20.0 });
        assert_eq!(
            columns,
            vec![vec![vec![0, 2, 3]], vec![vec![4, 6, 7, 8, 9]]]
        );
        assert_eq!(
            settings_columns(groups.clone(), false, |_| 40.0),
            vec![groups]
        );
        assert_eq!(
            settings_columns(Vec::new(), true, |_| 40.0),
            vec![Vec::<Vec<usize>>::new()]
        );
        assert_eq!(
            settings_columns(vec![vec![3]], true, |_| 40.0),
            vec![vec![vec![3]]]
        );
    }

    #[test]
    fn groups_hide_back_without_reindexing_and_balance_whole_cards() {
        let rows = [
            ("#", "First"),
            ("a", "A"),
            ("back", "Back"),
            ("#", "Second"),
            ("b", "B"),
            ("#", "Third"),
            ("c", "C"),
        ];
        let groups = settings_groups(&rows, "#", Some(2));
        assert_eq!(groups, vec![vec![0, 1], vec![3, 4], vec![5, 6]]);
        assert_eq!(
            settings_columns(groups, true, |_| 34.0),
            vec![vec![vec![0, 1]], vec![vec![3, 4], vec![5, 6]]]
        );
    }
}
