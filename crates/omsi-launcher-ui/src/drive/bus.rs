//! The Drive page's first step: the bus - its maker, model and livery picked from the lists.

use super::*;

/// OMSI takes the manufacturer and the complete type from [friendlyname]. The
/// vehicle folder and rendering configuration do not define this hierarchy.
pub(super) fn build_bus_manufacturers(vehicles: &[omsi_launcher_lib::VehicleInfo], allowed: Option<&std::collections::HashSet<String>>, fresh: &std::collections::HashSet<String>) -> Vec<BusManufacturer> {
    let mut grouped = std::collections::BTreeMap::<String, BusManufacturer>::new();
    for vehicle in vehicles {
        if !allowed.map(|a| a.contains(&vehicle.file.replace('\\', "/").to_lowercase())).unwrap_or(true) {
            continue;
        }
        let maker = vehicle.manufacturer.trim();
        let key = maker.to_lowercase();
        let group = grouped.entry(key.clone()).or_insert_with(|| BusManufacturer {
            key, name: if maker.is_empty() { "Unknown manufacturer".into() } else { display_bus_name(maker) }, variants: Vec::new(),
        });
        let type_name = vehicle_type_label(&vehicle.type_name, std::path::Path::new(&vehicle.file));
        group.variants.push(BusVariant {
            file: vehicle.file.clone(), name: display_bus_name(&vehicle.name), variant: type_name,
            fresh: fresh.contains(&vehicle.file), installed: vehicle.installed,
            paints: vehicle.paints.len(), incomplete: !vehicle.missing_packs.is_empty(),
        });
    }
    let mut manufacturers: Vec<BusManufacturer> = grouped.into_values().collect();
    for maker in &mut manufacturers {
        // Distinct .bus files remain selectable even when add-ons repeat a friendly
        // type name. Show the pack, and the file only if the pack also repeats it.
        let mut counts = std::collections::HashMap::<String, usize>::new();
        for variant in &maker.variants { *counts.entry(variant.variant.to_lowercase()).or_default() += 1; }
        for variant in &mut maker.variants {
            if counts[&variant.variant.to_lowercase()] > 1 {
                let folder = variant.file.replace('\\', "/").split('/').nth(1).unwrap_or_default().to_string();
                variant.variant = format!("{} | {}", variant.variant, display_bus_name(&folder));
            }
        }
        let mut counts = std::collections::HashMap::<String, usize>::new();
        for variant in &maker.variants { *counts.entry(variant.variant.to_lowercase()).or_default() += 1; }
        for variant in &mut maker.variants {
            if counts[&variant.variant.to_lowercase()] > 1 {
                let stem = std::path::Path::new(&variant.file).file_stem().unwrap_or_default().to_string_lossy();
                variant.variant = format!("{} | {}", variant.variant, display_bus_name(&stem));
            }
        }
        maker.variants.sort_by(|a, b| bus_name_cmp(&a.variant, &b.variant).then_with(|| a.file.cmp(&b.file)));
    }
    manufacturers.sort_by(|a, b| bus_name_cmp(&a.name, &b.name).then_with(|| a.key.cmp(&b.key)));
    manufacturers
}

/// Sort numeric runs wherever they occur: DL9 precedes DL10; case does not change
/// a manufacturer's position.
pub(super) fn bus_name_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    let mut a = a.chars().peekable();
    let mut b = b.chars().peekable();
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let x: String = std::iter::from_fn(|| a.next_if(|c| c.is_ascii_digit())).collect();
                let y: String = std::iter::from_fn(|| b.next_if(|c| c.is_ascii_digit())).collect();
                let x = x.trim_start_matches('0');
                let y = y.trim_start_matches('0');
                let order = x.len().cmp(&y.len()).then_with(|| x.cmp(y));
                if order != Ordering::Equal { return order; }
            }
            (Some(x), Some(y)) => {
                let order = x.cmp(&y);
                if order != Ordering::Equal { return order; }
                a.next(); b.next();
            }
        }
    }
}

pub(crate) fn default_livery_label(vehicle: &omsi_launcher_lib::VehicleInfo) -> &str {
    if vehicle.default_paint.trim().is_empty() { "Default paint" } else { vehicle.default_paint.trim() }
}

/// How many liveries a bus type with `repaints` has, as its Livery list counts them (its own
/// paint and the repaints): beside each type in the list, as a family says how many types it
/// has (#717).
pub(crate) fn liveries_text(repaints: usize) -> String {
    let n = repaints + 1;
    format!("{n} {}", omsi_ui::tr(if n == 1 { "livery" } else { "liveries" }))
}

/// A type of a bus family in its dropdown: the type and its liveries.
pub(super) fn variant_option(variant: &BusVariant) -> String {
    format!("{} | {}", variant.variant, liveries_text(variant.paints))
}

pub(super) fn variant_matches(variant: &BusVariant, q: &str) -> bool {
    q.is_empty() || variant.name.to_lowercase().contains(q) || variant.variant.to_lowercase().contains(q) || display_bus_name(&variant.file).to_lowercase().contains(q)
}

pub(super) fn manufacturer_matches(model: &BusManufacturer, q: &str) -> bool {
    q.is_empty() || model.name.to_lowercase().contains(q) || model.variants.iter().any(|v| variant_matches(v, q))
}

pub(super) fn step_bus(l: &mut Launcher, r: Rect) {
    l.ui.heading(Rect::new(r.x, r.y, r.w, 24.0), "Choose a bus", None);
    let search = Rect::new(r.x, r.y + 32.0, r.w, ROW);
    let search_changed = l.ui.text_input("bus-filter", search, &mut l.drive.bus_filter, "Search buses...", Some("search"));
    if search_changed {
        l.ui.scroll.remove(&id_of("bus-model-list"));
        l.ui.scroll.remove(&(id_of("bus-model-list") ^ 0xabc));
    }
    let q = display_bus_name(l.drive.bus_filter.trim()).to_lowercase();
    let norm = |f: &str| f.replace('\\', "/").to_lowercase();
    let allowed: Option<std::collections::HashSet<String>> = l.state.host_vehicles().map(|v| v.iter().map(|f| norm(f)).collect());
    if let Some(a) = allowed.as_ref() {
        if !l.state.choice.bus.is_empty() && !a.contains(&norm(&l.state.choice.bus)) {
            l.state.clear_bus();
        }
    }
    let allowed_key = allowed.as_ref().map(|a| a.iter().fold(0u64, |hash, file| hash ^ id_of(file))).unwrap_or(u64::MAX);
    let key = (l.state.vehicles.len(), allowed_key, l.state.fresh.len());
    if key != l.drive.bus_manufacturers_key || (l.drive.bus_manufacturers.is_empty() && !l.state.vehicles.is_empty()) {
        let fresh = l.state.fresh.keys().cloned().collect();
        l.drive.bus_manufacturers = std::sync::Arc::new(build_bus_manufacturers(&l.state.vehicles, allowed.as_ref(), &fresh));
        l.drive.bus_manufacturers_key = key;
    }
    let models = l.drive.bus_manufacturers.clone();
    let chosen = l.state.choice.bus.clone();
    let favs = l.drive.favourites.get_or_insert_with(read_favourites).clone();
    let is_fav = |file: &str| favs.contains(&fav_key(file));
    // (the starred ones only: a family with one of them, and in it only those - #524)
    let only = l.drive.only_favourites && !favs.is_empty();

    let visible: Vec<&BusManufacturer> = models.iter().filter(|m| manufacturer_matches(m, &q) && (!only || m.variants.iter().any(|v| is_fav(&v.file)))).collect();
    let count = format!("{} {}", visible.len(), omsi_ui::tr(if visible.len() == 1 { "manufacturer" } else { "manufacturers" }));
    l.ui.text_in(&count, Rect::new(r.x, search.bottom() + 6.0, r.w, 20.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
    let mut only_now = l.drive.only_favourites;
    let fav_w = l.ui.width(&omsi_ui::tr("Favourites only"), 13.0, Weight::Regular) + 50.0;
    if l.ui.toggle("bus-only-favourites", Rect::new(r.right() - fav_w, search.bottom() + 4.0, fav_w, 22.0), &mut only_now, "Favourites only") {
        l.drive.only_favourites = only_now;
        l.ui.scroll.remove(&id_of("bus-model-list"));
    }
    let list_y = search.bottom() + 32.0;
    let settings_h = if l.drive.vehicle_settings_open { 164.0 } else { 0.0 };
    let list = Rect::new(r.x, list_y, r.w, (r.bottom() - list_y - 132.0 - settings_h).max(100.0));
    l.ui.p().rounded(list, RADIUS, FIELD);
    l.ui.p().rounded_border(list, RADIUS, 1.0, EDGE);
    if !l.drive.bus_list_initialized && !models.is_empty() {
        if let Some(index) = models.iter().position(|m| m.variants.iter().any(|v| v.file == chosen)) {
            let model = &models[index];
            l.drive.expanded_manufacturer = Some(model.key.clone());
            let selected_y = 6.0 + index as f32 * 58.0;
            l.ui.scroll_to("bus-model-list", selected_y, if model.variants.len() > 1 { 120.0 } else { 54.0 }, list.h);
        }
        l.drive.bus_list_initialized = true;
    }
    if search_changed && !q.is_empty() {
        l.drive.expanded_manufacturer = visible.first().map(|m| m.key.clone());
    }
    let expanded = l.drive.expanded_manufacturer.clone();
    let mut toggle = None;
    let mut pick = None;
    let mut star: Option<String> = None;
    let loading = l.state.loading_content;
    l.ui.scroll_area("bus-model-list", list, &mut |ui, view| {
        let mut y = view.y + 6.0;
        if visible.is_empty() {
            ui.text_in(if loading { "Reading the buses..." } else { "No buses found. Try another search." }, Rect::new(view.x + 12.0, y, view.w - 24.0, 50.0), 12.5, Weight::Regular, TEXT_DIM, Align::Left);
        }
        for model in &visible {
            let open = model.variants.len() > 1 && expanded.as_deref() == Some(model.key.as_str());
            let row = Rect::new(view.x + 6.0, y, view.w - 18.0, 54.0);
            // Keep the scroll extent, but avoid shaping text and emitting geometry for
            // invisible families. The expanded dropdown retains its widget state.
            if !open && !ui.rect_visible(row) {
                y += 58.0;
                continue;
            }
            let selected = model.variants.iter().find(|v| v.file == chosen);
            if ui.row(&format!("bus-family-{}", model.key), row, selected.is_some()) {
                if model.variants.len() == 1 { pick = Some(model.variants[0].file.clone()); }
                else { toggle = Some(model.key.clone()); }
            }
            ui.icon("directions_bus", Vec2::new(row.x + 20.0, row.y + 23.0), 20.0, if selected.is_some() { accent() } else { TEXT_DIM });
            let title = Rect::new(row.x + 42.0, row.y + 7.0, row.w - 78.0, 20.0);
            ui.text_in(&model.name, title, 13.0, Weight::Medium, TEXT, Align::Left);
            ui.tooltip(title, &model.name);
            let subtitle = if model.variants.len() == 1 { format!("{} | {}", omsi_ui::tr(&model.variants[0].variant), liveries_text(model.variants[0].paints)) }
            else if let Some(v) = selected { format!("{} | {} {}", omsi_ui::tr(&v.variant), model.variants.len(), omsi_ui::tr("models")) }
            else { format!("{} {}", model.variants.len(), omsi_ui::tr("models")) };
            let subtitle = if selected.is_some_and(|v| v.incomplete) { format!("{subtitle} | {}", omsi_ui::tr("PARTS MISSING")) }
            else if selected.is_some_and(|v| v.fresh) { format!("{subtitle} | {}", omsi_ui::tr("NEW")) }
            else if selected.is_some_and(|v| v.installed) { format!("{subtitle} | {}", omsi_ui::tr("MOD")) } else { subtitle };
            ui.text_in(&subtitle, Rect::new(title.x, row.y + 29.0, title.w, 17.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
            ui.icon(if model.variants.len() == 1 { if selected.is_some() { "check" } else { "chevron_right" } } else if open { "expand_less" } else { "expand_more" }, Vec2::new(row.right() - 18.0, row.center().y), 18.0, if selected.is_some() { accent() } else { TEXT_DIM });
            // the star: a bus of its own is starred here, a family's types in its list below
            let starred = model.variants.iter().any(|v| is_fav(&v.file));
            let sr = Rect::new(row.right() - 62.0, row.center().y - 13.0, 26.0, 26.0);
            if model.variants.len() == 1 {
                let (hs, _, cs) = ui.interact(id_of(&format!("bus-star-{}", model.key)), sr);
                if cs {
                    star = Some(model.variants[0].file.clone());
                }
                ui.icon("star", sr.center(), 16.0, if starred { accent() } else if hs { TEXT_SOFT } else { Color::WHITE.alpha(0.16) });
                ui.tooltip(sr, if starred { "Remove from the favourites" } else { "Add to the favourites" });
            } else if starred {
                ui.icon("star", sr.center(), 14.0, accent().alpha(0.8));
            }
            y += 58.0;
            if open {
                let variants: Vec<&BusVariant> = model.variants.iter().filter(|variant| (q.is_empty() || model.name.to_lowercase().contains(&q) || variant.file == chosen || variant_matches(variant, &q)) && (!only || variant.file == chosen || is_fav(&variant.file))).collect();
                let selected_index = variants.iter().position(|variant| variant.file == chosen);
                let mut options: Vec<String> = variants.iter().map(|variant| variant_option(variant)).collect();
                let offset = if selected_index.is_none() { options.insert(0, "Choose a bus".into()); 1 } else { 0 };
                let mut sel = selected_index.unwrap_or(0);
                ui.label(Rect::new(view.x + 38.0, y, view.w - 50.0, 22.0), "Type / variant");
                y += 26.0;
                let dropdown = Rect::new(view.x + 38.0, y, view.w - 50.0 - 36.0, ROW);
                if !options.is_empty() && ui.select(&format!("bus-type-{}", model.key), dropdown, &mut sel, &options) {
                    if let Some(variant) = sel.checked_sub(offset).and_then(|index| variants.get(index)) { pick = Some(variant.file.clone()); }
                }
                // the star of the type chosen
                if let Some(variant) = selected {
                    let sr = Rect::new(dropdown.right() + 6.0, y, 30.0, ROW);
                    let on = is_fav(&variant.file);
                    let (hs, _, cs) = ui.interact(id_of(&format!("bus-star-type-{}", model.key)), sr);
                    if cs {
                        star = Some(variant.file.clone());
                    }
                    ui.icon("star", sr.center(), 18.0, if on { accent() } else if hs { TEXT_SOFT } else { Color::WHITE.alpha(0.2) });
                    ui.tooltip(sr, if on { "Remove from the favourites" } else { "Add to the favourites" });
                }
                if let Some(variant) = selected {
                    ui.tooltip(dropdown, &format!("{}\n{}\n{}", variant.name, variant.file, liveries_text(variant.paints)));
                }
                y += ROW + 10.0;
            }
        }
        y - view.y + 4.0
    });
    if let Some(key) = toggle {
        l.drive.expanded_manufacturer = if l.drive.expanded_manufacturer.as_ref() == Some(&key) { None } else { Some(key.clone()) };
        if l.drive.expanded_manufacturer.is_some() {
            if let Some(index) = visible.iter().position(|maker| maker.key == key) {
                l.ui.scroll_to("bus-model-list", 6.0 + index as f32 * 58.0, 120.0, list.h);
            }
        }
    }
    if let Some(file) = pick { l.state.select_bus(&file); }
    if let Some(file) = star {
        let f = l.drive.favourites.get_or_insert_with(read_favourites);
        let k = fav_key(&file);
        if !f.remove(&k) {
            f.insert(k);
        }
        write_favourites(f);
    }

    let mut y = list.bottom() + 16.0;
    if let Some(vehicle) = l.state.bus().cloned() {
        let paints: Vec<String> = std::iter::once(default_livery_label(&vehicle).to_string()).chain(vehicle.paints.iter().cloned()).collect();
        let mut paint_sel = vehicle.paints.iter().position(|p| *p == l.state.choice.paint).map(|i| i + 1).unwrap_or(0);
        l.ui.label(Rect::new(r.x, y, r.w, 22.0), "Livery");
        if paints.len() > 1 { l.ui.text_in(&format!("{} / {}", paint_sel + 1, paints.len()), Rect::new(r.right() - 70.0, y, 70.0, 22.0), 11.5, Weight::Regular, TEXT_DIM, Align::Right); }
        y += 28.0;
        let has_arrows = paints.len() > 1;
        let selector = Rect::new(r.x, y, r.w - if has_arrows { 88.0 } else { 0.0 }, ROW);
        let mut changed = l.ui.select("paint", selector, &mut paint_sel, &paints);
        if has_arrows {
            let prev = Rect::new(selector.right() + 8.0, y, ROW, ROW);
            let next = Rect::new(prev.right() + 8.0, y, ROW, ROW);
            if l.ui.button("paint-previous", prev, "", Some("chevron_left"), ButtonKind::Normal) { paint_sel = (paint_sel + paints.len() - 1) % paints.len(); changed = true; }
            if l.ui.button("paint-next", next, "", Some("chevron_right"), ButtonKind::Normal) { paint_sel = (paint_sel + 1) % paints.len(); changed = true; }
            l.ui.tooltip(prev, "Preview previous livery");
            l.ui.tooltip(next, "Preview next livery");
        }
        if changed {
            l.state.choice.paint = if paint_sel == 0 { String::new() } else { vehicle.paints[paint_sel - 1].clone() };
            l.state.touched();
        }
        y += ROW + 14.0;
        let settings = Rect::new(r.x, y, r.w, 32.0);
        if l.ui.row("vehicle-settings-toggle", settings, false) { l.drive.vehicle_settings_open = !l.drive.vehicle_settings_open; }
        l.ui.icon(if l.drive.vehicle_settings_open { "expand_less" } else { "expand_more" }, Vec2::new(settings.x + 12.0, settings.center().y), 18.0, TEXT_DIM);
        l.ui.text_in("Vehicle settings & details", Rect::new(settings.x + 30.0, settings.y, settings.w - 30.0, settings.h), 12.5, Weight::Medium, TEXT_DIM, Align::Left);
        y += 38.0;
        if l.drive.vehicle_settings_open {
            let details = Rect::new(r.x, y, r.w, (r.bottom() - y).max(0.0));
            let mut hof_pick = None;
            let mut number_pick = None;
            let number_options: Vec<String> = vehicle.numbers.iter().map(|(number, plate)| if plate.trim().is_empty() { number.clone() } else { format!("{number}  ({})", plate.trim()) }).collect();
            let mut number_sel = vehicle.numbers.iter().position(|(number, _)| *number == l.state.choice.number).unwrap_or(0);
            let mut plate = l.state.choice.plate.clone();
            let auto = l.state.default_hof();
            let mut hof_options = vec![omsi_ui::tr("Automatic (%{hof})").replace("%{hof}", &auto)];
            hof_options.extend(vehicle.hofs.iter().cloned());
            let mut hof_sel = if l.state.choice.hof_manual { vehicle.hofs.iter().position(|h| h.eq_ignore_ascii_case(&l.state.choice.hof)).map(|i| i + 1).unwrap_or(0) } else { 0 };
            let mut plate_changed = false;
            // the bus options first, under the livery (see `busoptions`)
            let options = l.state.bus_options.catalogue(&l.state.config.root, &vehicle.file).filter(|c| !c.options.is_empty());
            let (picks, paint, technical) = (l.state.bus_options.picks(&vehicle.file), l.state.choice.paint.clone(), l.state.bus_options.technical_open);
            let mut option_edit = None;
            l.ui.scroll_area("bus-details", details, &mut |ui, view| {
                let mut y = view.y;
                let field_w = view.w - 8.0;
                if let Some(cat) = options.as_deref() {
                    let section = crate::busoptions::Section { cat, livery: &paint, picks: &picks, technical_open: technical };
                    let (h, e) = crate::busoptions::section(ui, view.x, y, field_w, &section);
                    option_edit = e;
                    y += h + 10.0;
                    ui.p().rect(Rect::new(view.x, y, field_w, 1.0), HAIRLINE);
                    y += 14.0;
                }
                ui.label(Rect::new(view.x, y, 110.0, ROW), "Depot file");
                if ui.select("hof", Rect::new(view.x + 110.0, y, field_w - 110.0, ROW), &mut hof_sel, &hof_options) { hof_pick = Some(hof_sel); }
                y += ROW + 8.0;
                if !number_options.is_empty() {
                    ui.label(Rect::new(view.x, y, 110.0, ROW), "Fleet number");
                    if ui.select("number", Rect::new(view.x + 110.0, y, field_w - 110.0, ROW), &mut number_sel, &number_options) { number_pick = Some(number_sel); }
                    y += ROW + 8.0;
                }
                ui.label(Rect::new(view.x, y, 110.0, ROW), "Number plate");
                plate_changed = ui.text_input("plate", Rect::new(view.x + 110.0, y, field_w - 110.0, ROW), &mut plate, "Automatic", Some("badge"));
                y += ROW + 16.0;
                if !vehicle.missing_packs.is_empty() {
                    y += ui.paragraph(&omsi_ui::tr("Parts missing: needs %{packs}").replace("%{packs}", &vehicle.missing_packs.join(", ")), Vec2::new(view.x, y), field_w, 12.5, Weight::Regular, WARN) + 12.0;
                }
                let description = vehicle.description.replace('\t', " ").lines().map(str::trim).collect::<Vec<_>>().join("\n").trim().to_string();
                if !description.is_empty() { y += ui.paragraph(&description, Vec2::new(view.x, y), field_w, 12.0, Weight::Regular, TEXT_DIM) + 12.0; }
                y += ui.paragraph(&vehicle.file, Vec2::new(view.x, y), field_w, 10.5, Weight::Regular, TEXT_FAINT);
                y - view.y + 8.0
            });
            if let Some(e) = option_edit {
                l.state.edit_bus_options(&vehicle.file, e);
            }
            if let Some(sel) = hof_pick {
                l.state.choice.hof_manual = sel != 0;
                l.state.choice.hof = if sel == 0 { auto } else { vehicle.hofs[sel - 1].clone() };
                l.state.touched();
            }
            if let Some(sel) = number_pick {
                l.state.choice.number = vehicle.numbers[sel].0.clone();
                l.state.touched();
            }
            if plate_changed { l.state.choice.plate = plate; l.state.touched(); }
        }
    }
}

pub(crate) fn natural(s: &str) -> (u64, String) {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    (digits.parse().unwrap_or(u64::MAX), s.to_string())
}

#[cfg(test)]
mod vehicle_picker_tests {
    use super::*;

    fn vehicle(folder: &str, maker: &str, name: &str, file: &str) -> omsi_launcher_lib::VehicleInfo {
        omsi_launcher_lib::VehicleInfo {
            name: format!("{maker} {name}"), manufacturer: maker.into(), type_name: name.into(),
            folder: folder.into(), file: format!("Vehicles/{folder}/{file}.bus"),
            description: String::new(), paints: vec!["Paint".into()], hofs: vec![],
            installed: false, missing_packs: vec![], numbers: vec![], default_paint: "Beige".into(),
        }
    }

    #[test]
    fn a_tour_search_finds_the_next_trip_of_a_route_and_skips_the_ones_gone() {
        let trip = |index: usize, name: &str, departure: f64| omsi_launcher_lib::TripInfo {
            name: name.into(),
            index,
            line: "9106".into(),
            from: "Massy".into(),
            terminus: "Cormeilles".into(),
            departure,
            arrival: departure + 1800.0,
            stops: Vec::new(),
            km: 10.0,
        };
        let tour = |trips: Vec<omsi_launcher_lib::TripInfo>| omsi_launcher_lib::TourInfo {
            number: "12".into(),
            ai_group: String::new(),
            first: 0.0,
            last: 0.0,
            days: "Mon-Fri".into(),
            runs: true,
            next_run: None,
            trips,
        };
        let h = |x: f64| x * 3600.0;
        let morning_b = tour(vec![trip(1, "9106B_HC_MASSY", h(7.0)), trip(2, "9106A_HC_CORMEILLES", h(8.0)), trip(3, "9106A_HC_MASSY", h(13.0))]);
        assert_eq!(tour_trip(&morning_b, h(12.0), "9106b"), (false, None));
        assert_eq!(tour_trip(&morning_b, h(12.0), ""), (false, Some(2)));
        let later_b = tour(vec![trip(1, "9106A_HC_MASSY", h(11.5)), trip(2, "9106B_HC_CORMEILLES", h(12.5)), trip(3, "9106B_HC_MASSY", h(14.0))]);
        assert_eq!(tour_trip(&later_b, h(12.0), "9106b"), (false, Some(1)));
        assert_eq!(tour_trip(&later_b, h(12.0), "12"), (false, Some(1)));
        assert_eq!(tour_trip(&later_b, h(15.0), ""), (true, Some(2)));
        assert_eq!(tour_trip(&later_b, h(15.0), "9106b"), (true, Some(1)));
    }

    #[test]
    fn empty_vehicle_types_use_the_file_name() {
        for empty in ["", "   "] {
            let manufacturers = build_bus_manufacturers(
                &[vehicle("Pack", "MAN", empty, "NL_202")],
                None,
                &Default::default(),
            );
            assert_eq!(manufacturers[0].variants[0].variant, "NL 202");
        }
    }

    #[test]
    fn omsi_manufacturer_groups_dl_and_lions_city_across_packs() {
        let vehicles = vec![
            vehicle("MAN_DL05", "MAN", "DL05", "dl05"),
            vehicle("MAN_DL05", "MAN", "DL07", "dl07"),
            vehicle("MAN_DL05", "MAN", "DL08", "dl08"),
            vehicle("MAN_DL05", "MAN", "DL09", "dl09"),
            vehicle("MAN_LC_MVG", "MAN", "Lion's City (MVG)", "mvg"),
            vehicle("MAN_LC_GUE", "MAN", "Lion's City G (ORN)", "orn"),
        ];
        let manufacturers = build_bus_manufacturers(&vehicles, None, &Default::default());
        assert_eq!(manufacturers.len(), 1);
        assert_eq!(manufacturers[0].name, "MAN");
        assert_eq!(manufacturers[0].variants.len(), 6);
        assert!(manufacturers[0].variants.iter().any(|v| v.variant == "DL07"));
        assert!(manufacturers[0].variants.iter().any(|v| v.variant == "Lion's City G (ORN)"));
        assert!(manufacturer_matches(&manufacturers[0], "orn"));
        assert!(!manufacturer_matches(&manufacturers[0], "not a bus"));
    }

    #[test]
    fn author_defined_manufacturer_and_complete_type_are_preserved() {
        let vehicles = vec![
            vehicle("Pack", "Mercedes-Benz Release", "MB_C2_E6_GN_BVG_Leasing 2", "leasing"),
            vehicle("Pack", "Mercedes-Benz", "O530", "o530"),
        ];
        let manufacturers = build_bus_manufacturers(&vehicles, None, &Default::default());
        assert_eq!(manufacturers.len(), 2);
        let maker = manufacturers.iter().find(|m| m.name == "Mercedes-Benz Release").unwrap();
        assert_eq!(maker.variants[0].variant, "MB C2 E6 GN BVG Leasing 2");
    }

    #[test]
    fn duplicate_names_remain_selectable_and_host_filter_is_respected() {
        let vehicles = vec![
            vehicle("MAN", "MAN", "NL202", "en92"),
            vehicle("MAN", "MAN", "NL202", "en93"),
            vehicle("OtherPack", "MAN", "NL202", "en92"),
        ];
        let manufacturers = build_bus_manufacturers(&vehicles, None, &Default::default());
        let labels: std::collections::HashSet<_> = manufacturers[0].variants.iter().map(|v| &v.variant).collect();
        assert_eq!(labels.len(), 3);
        let allowed = std::collections::HashSet::from([vehicles[1].file.to_lowercase()]);
        let filtered = build_bus_manufacturers(&vehicles, Some(&allowed), &Default::default());
        assert_eq!(filtered[0].variants.len(), 1);
        assert_eq!(filtered[0].variants[0].file, vehicles[1].file);
    }

    /// Each type in a family's dropdown says how many liveries it has, as the Livery list
    /// beside counts them: its own paint and its repaints (#717).
    #[test]
    fn each_bus_type_says_how_many_liveries_it_has() {
        let mut one = vehicle("MAN_SD200", "MAN", "SD77", "sd77");
        one.paints.clear();
        let mut many = vehicle("MAN_SD200", "MAN", "SD78", "sd78");
        many.paints = vec!["BVG".into(), "Werbung".into(), "Neu".into()];
        let manufacturers = build_bus_manufacturers(&[one, many], None, &Default::default());
        let options: Vec<String> = manufacturers[0].variants.iter().map(variant_option).collect();
        assert_eq!(options, vec!["SD77 | 1 livery".to_string(), "SD78 | 4 liveries".to_string()]);
    }

    #[test]
    fn numbers_in_type_names_sort_naturally_and_default_livery_has_its_omsi_name() {
        assert_eq!(bus_name_cmp("DL9", "DL10"), std::cmp::Ordering::Less);
        assert_eq!(bus_name_cmp("MAN", "man"), std::cmp::Ordering::Equal);
        let mut vehicle = vehicle("MAN", "MAN", "DL07", "dl07");
        assert_eq!(default_livery_label(&vehicle), "Beige");
        vehicle.default_paint.clear();
        assert_eq!(default_livery_label(&vehicle), "Default paint");
    }
}
