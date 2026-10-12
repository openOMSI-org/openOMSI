//! The Mods page.

use super::*;
use omsi_launcher_lib as core;

// --- mods ----------------------------------------------------------------------------------------

pub fn mods(l: &mut Launcher, area: Rect) {
    if !l.state.mods_asked {
        l.state.load_mods();
    }
    let body = area;
    // (installing and the installs on the left, the list of every mod taking the rest)
    let side = (body.w * 0.27).clamp(280.0, 380.0);
    let list_w = body.w - 2.0 * (side + GAP * 2.0);
    let colr = |k: usize| match k {
        0 => Rect::new(body.x, body.y, side, body.h),
        1 => Rect::new(body.x + side + GAP * 2.0, body.y, side, body.h),
        _ => Rect::new(body.x + 2.0 * (side + GAP * 2.0), body.y, list_w, body.h),
    };
    // (asked again once an install has finished: the list shows what it put there)
    let done = l.state.jobs.iter().filter(|j| j.finished.is_some()).count();
    if done != l.state.mods_jobs_seen {
        l.state.mods_jobs_seen = done;
        l.state.load_mods();
    }
    // install
    let c0 = colr(0);
    l.ui.card(c0);
    let inner = l.ui.heading(Rect::new(c0.x + 18.0, c0.y + 14.0, c0.w - 36.0, c0.h - 28.0), "Install a mod", Some("download"));
    let mut y = inner.y;
    let half = (inner.w - GAP) * 0.5;
    if l.ui.button("mod-folder", Rect::new(inner.x, y, half, 40.0), "Choose a folder", Some("folder_open"), ButtonKind::Primary) {
        if crate::mobile::mobile() {
            l.browse(crate::mobile::Purpose::ModFolder, "");
        } else if let Some(p) = core::pick_mod(false) {
            l.state.install(p.to_string_lossy().to_string());
        }
    }
    if l.ui.button("mod-zip", Rect::new(inner.x + half + GAP, y, half, 40.0), "Choose archive", Some("inventory_2"), ButtonKind::Normal) {
        if crate::mobile::mobile() {
            l.browse(crate::mobile::Purpose::ModZip, "");
        } else if let Some(p) = core::pick_mod(true) {
            l.state.install(p.to_string_lossy().to_string());
        }
    }
    y += 52.0;
    l.ui.label(Rect::new(inner.x, y, inner.w, 20.0), "Archive install mode");
    y += 22.0;
    let mut m = l.state.mod_mode;
    if l.ui.segmented("mod-mode", Rect::new(inner.x, y, inner.w, 34.0), &mut m, &["Auto", "Unpacked", "Used in place"]) {
        l.state.mod_mode = m;
    }
    y += 44.0;
    let drop = Rect::new(inner.x, y, inner.w, 110.0);
    let hot = l.pages.drop_hover;
    let t = l.ui.anim(id_of("drop"), if hot { 1.0 } else { 0.0 }, 0.1);
    l.ui.p().rounded(drop, 12.0, accent().alpha(0.05 + 0.12 * t));
    // a dashed edge
    let per = 2.0 * (drop.w + drop.h);
    let n = (per / 14.0) as usize;
    for k in 0..n {
        let s = k as f32 * per / n as f32;
        let p = if s < drop.w {
            Vec2::new(drop.x + s, drop.y)
        } else if s < drop.w + drop.h {
            Vec2::new(drop.right(), drop.y + s - drop.w)
        } else if s < 2.0 * drop.w + drop.h {
            Vec2::new(drop.right() - (s - drop.w - drop.h), drop.bottom())
        } else {
            Vec2::new(drop.x, drop.bottom() - (s - 2.0 * drop.w - drop.h))
        };
        l.ui.p().circle(p, 1.3, accent().alpha(0.35 + 0.5 * t));
    }
    l.ui.icon("upload", Vec2::new(drop.center().x, drop.y + 38.0), 30.0, accent().alpha(0.6 + 0.4 * t));
    l.ui.text_in("...or drop a mod folder or .zip, .7z or .rar onto this window", Rect::new(drop.x, drop.y + 62.0, drop.w, 30.0), 12.5, Weight::Medium, TEXT_SOFT, Align::Center);
    y += 122.0;
    if !l.state.mod_path.is_empty() {
        let p = l.state.mod_path.clone();
        y += l.ui.paragraph(&p, Vec2::new(inner.x, y), inner.w, 11.5, Weight::Regular, TEXT_FAINT);
        match l.state.mod_info.clone() {
            Some(Ok(i)) if i.is_archive => {
                let fit = if i.fits { omsi_ui::tr("fits (%{free} free)").replace("%{free}", &fmt_bytes(i.free_bytes)) } else { omsi_ui::tr("does not fit: needs %{needed}, %{free} free").replace("%{needed}", &fmt_bytes(i.needed_bytes)).replace("%{free}", &fmt_bytes(i.free_bytes)) };
                let place = if i.in_place_ok { omsi_ui::tr("can be used in place").into_owned() } else { i.in_place.clone() };
                let what = omsi_ui::tr("%{size} archive, %{files} files, %{unpacked} unpacked").replace("%{size}", &fmt_bytes(i.archive_bytes)).replace("%{files}", &i.files.to_string()).replace("%{unpacked}", &fmt_bytes(i.unpacked_bytes));
                y += l.ui.paragraph(&format!("{what} - {fit}; {place}"), Vec2::new(inner.x, y), inner.w, 12.0, Weight::Regular, if i.fits { TEXT_DIM } else { WARN });
            }
            Some(Err(e)) => {
                y += l.ui.paragraph(&e, Vec2::new(inner.x, y), inner.w, 12.0, Weight::Regular, DANGER);
            }
            _ => {}
        }
    }
    y += 10.0;
    if let Some(m) = l.state.mods.clone() {
        l.ui.heading(Rect::new(inner.x, y, inner.w, 28.0), "The Mods folder", None);
        y += 30.0;
        // (the folder on a line of its own, cut to the column: a path has no spaces to break
        // at and ran out of the card)
        y += l.ui.paragraph("Anything put into this folder is installed by itself once it has finished copying.", Vec2::new(inner.x, y), inner.w, 12.0, Weight::Regular, TEXT_DIM);
        let at = Rect::new(inner.x, y + 2.0, inner.w, 18.0);
        l.ui.text_in(&m.inbox, at, 12.0, Weight::Medium, TEXT_SOFT, Align::Left);
        l.ui.tooltip(at, &m.inbox);
        y += 24.0;
        if !m.inbox_items.is_empty() {
            l.ui.paragraph(&omsi_ui::tr("In it now: %{items}").replace("%{items}", &m.inbox_items.join(", ")), Vec2::new(inner.x, y + 4.0), inner.w, 12.0, Weight::Regular, TEXT_SOFT);
        }
    }
    // installs
    let c1 = colr(1);
    l.ui.card(c1);
    let inner = l.ui.heading(Rect::new(c1.x + 18.0, c1.y + 14.0, c1.w - 36.0, c1.h - 28.0), "Installs", Some("inventory_2"));
    if l.ui.button("jobs-clear", Rect::new(c1.right() - 18.0 - 130.0, c1.y + 12.0, 130.0, 30.0), "Clear finished", None, ButtonKind::Ghost) {
        core::install::clear_finished();
        l.state.poll_now();
    }
    let jobs = l.state.jobs.clone();
    let mut cancel = None;
    l.ui.scroll_area("jobs", Rect::new(inner.x - 6.0, inner.y, inner.w + 12.0, inner.h), &mut |ui, v| {
        if jobs.is_empty() {
            ui.paragraph("Nothing installed since the launcher started. Big archives are checked against the free disk space before anything is unpacked; a cancelled or failed install leaves nothing behind.", Vec2::new(v.x + 6.0, v.y), v.w - 12.0, 12.5, Weight::Regular, TEXT_DIM);
            return 60.0;
        }
        let mut y = v.y;
        for j in &jobs {
            let running = j.finished.is_none();
            let msg_h = ui.paragraph_height(&j.message, v.w - 40.0, 12.0, Weight::Regular);
            let h = 50.0 + msg_h + if running { 44.0 } else { 0.0 } + j.warnings.len() as f32 * 18.0;
            let r = Rect::new(v.x + 6.0, y, v.w - 16.0, h);
            ui.p().rounded(r, 10.0, Color::WHITE.alpha(0.04));
            ui.text_in(&j.name, Rect::new(r.x + 12.0, r.y + 8.0, r.w - 120.0, 20.0), 13.5, Weight::Bold, TEXT, Align::Left);
            let sc = match j.state.as_str() {
                "done" => OK,
                "failed" => DANGER,
                "cancelled" => TEXT_DIM,
                _ => accent(),
            };
            ui.badge(Vec2::new(r.right() - 90.0, r.y + 10.0), &omsi_ui::tr(&j.state).to_uppercase(), sc);
            let mut yy = r.y + 34.0;
            if running {
                let frac = if j.bytes_total > 0 { j.bytes_done as f32 / j.bytes_total as f32 } else if j.files_total > 0 { j.files_done as f32 / j.files_total as f32 } else { 0.0 };
                ui.progress(Rect::new(r.x + 12.0, yy, r.w - 24.0, 8.0), frac, true);
                ui.text_in(&format!("{} | {} / {}", omsi_ui::tr("%{done} / %{total} files").replace("%{done}", &j.files_done.to_string()).replace("%{total}", &j.files_total.to_string()), fmt_bytes(j.bytes_done), fmt_bytes(j.bytes_total)), Rect::new(r.x + 12.0, yy + 10.0, r.w - 24.0, 16.0), 11.0, Weight::Regular, TEXT_DIM, Align::Left);
                yy += 30.0;
            }
            yy += ui.paragraph(&j.message, Vec2::new(r.x + 12.0, yy), r.w - 24.0, 12.0, Weight::Regular, if j.state == "failed" { DANGER } else { TEXT_SOFT });
            for w in &j.warnings {
                ui.text_in(&format!("⚠ {w}"), Rect::new(r.x + 12.0, yy, r.w - 24.0, 16.0), 11.0, Weight::Regular, WARN, Align::Left);
                yy += 18.0;
            }
            if running && ui.button(&format!("cancel-{}", j.id), Rect::new(r.x + 12.0, r.bottom() - 34.0, 100.0, 28.0), "Cancel", None, ButtonKind::Danger) {
                cancel = Some(j.id);
            }
            y += h + 10.0;
        }
        y - v.y
    });
    if let Some(id) = cancel {
        core::install::cancel(id);
        l.state.poll_now();
    }
    // waiting packs, under the install column's text
    if let Some(m) = l.state.mods.clone().filter(|m| !m.waiting.is_empty()) {
        let c0 = colr(0);
        let y0 = c0.bottom() - 24.0 - 22.0 * m.waiting.len().min(4) as f32;
        l.ui.text_in("Waiting for their bus", Rect::new(c0.x + 18.0, y0 - 26.0, c0.w - 36.0, 22.0), 12.5, Weight::Bold, TEXT, Align::Left);
        for (k, w) in m.waiting.iter().take(4).enumerate() {
            l.ui.text_in(w, Rect::new(c0.x + 18.0, y0 + k as f32 * 22.0, c0.w - 36.0, 20.0), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
        }
    }
    mod_list(l, colr(2));
}

/// The Mods page's filters over the list.
const MOD_FILTERS: [&str; 6] = ["All", "Buses", "Maps", "Archives", "Other", "Off"];

fn mod_passes(m: &core::mods::Mod, filter: usize) -> bool {
    use core::mods::Kind;
    match filter {
        1 => m.kind == Kind::Bus,
        2 => m.kind == Kind::Map,
        3 => m.kind == Kind::Archive,
        4 => m.kind == Kind::Other,
        5 => !m.enabled,
        _ => true,
    }
}

/// Every mod of the content folder (see `omsi_launcher_lib::mods`): found by a search and a
/// filter, each switched off and on - its folders out of the game's sight, nothing deleted -
/// or deleted after a question asked in its row.
fn mod_list(l: &mut Launcher, c: Rect) {
    use core::mods::Kind;
    l.ui.card(c);
    let inner = l.ui.heading(Rect::new(c.x + 18.0, c.y + 14.0, c.w - 36.0, c.h - 28.0), "Installed mods", Some("extension"));
    let Some(status) = l.state.mods.clone() else {
        l.ui.text_in("Reading the content folder...", Rect::new(inner.x, inner.y, inner.w, 20.0), 12.5, Weight::Regular, TEXT_DIM, Align::Left);
        return;
    };
    let mods = status.installed.clone();
    let on = mods.iter().filter(|m| m.enabled).count();
    // the content folder: where, how much room
    l.ui.text_in(&format!("{} mods, {on} on | {} free", mods.len(), fmt_bytes(status.free_bytes)), Rect::new(c.x + 220.0, c.y + 14.0, c.w - 238.0, 28.0), 12.0, Weight::Regular, TEXT_DIM, Align::Right);
    let mut y = inner.y;
    // (the folder cut to the column, the whole of it on hovering: a path has no spaces to
    // break at)
    let at = Rect::new(inner.x, y, inner.w - 90.0, 18.0);
    l.ui.text_in(&status.content_dir, at, 11.5, Weight::Regular, TEXT_FAINT, Align::Left);
    l.ui.tooltip(at, &status.content_dir);
    if l.ui.button("mods-open-folder", Rect::new(inner.right() - 80.0, y - 4.0, 80.0, 26.0), "Open", Some("open_in_new"), ButtonKind::Ghost) {
        omsi_platform::updater::open_url(&status.content_dir);
    }
    y += 26.0;
    // the search, and the filters with how many each holds
    let mut q = std::mem::take(&mut l.pages.mod_search);
    l.ui.text_input("mods-search", Rect::new(inner.x, y, inner.w, 34.0), &mut q, "Search mods...", Some("search"));
    l.pages.mod_search = q.clone();
    y += 42.0;
    let labels: Vec<String> = MOD_FILTERS.iter().enumerate().map(|(k, f)| format!("{f} {}", mods.iter().filter(|m| mod_passes(m, k)).count())).collect();
    let refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    let mut f = l.pages.mod_filter;
    if l.ui.segmented("mods-filter", Rect::new(inner.x, y, inner.w, 32.0), &mut f, &refs) {
        l.pages.mod_filter = f;
    }
    y += 42.0;
    let q = q.to_lowercase();
    let mut shown: Vec<core::mods::Mod> = mods.into_iter().filter(|m| mod_passes(m, l.pages.mod_filter)).filter(|m| q.is_empty() || m.name.to_lowercase().contains(&q) || m.paths.iter().any(|p| p.to_lowercase().contains(&q))).collect();
    // (switched-on first, then by name)
    shown.sort_by(|a, b| b.enabled.cmp(&a.enabled).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    let busy = l.state.mod_busy.clone();
    let confirm = l.pages.mod_confirm.clone();
    let empty = status.installed.is_empty();
    // what the rows asked: (id, Some(on)) switch, (id, None) delete asked, delete confirmed
    let mut toggle: Option<(String, bool)> = None;
    let mut ask: Option<Option<String>> = None;
    let mut delete: Option<String> = None;
    let mut open_toggle: Option<String> = None;
    let opened = l.pages.mod_open.clone();
    // (the install dates in the machine's time zone, as the service record writes its dates)
    let offset = core::utc_offset();
    let list = Rect::new(inner.x - 6.0, y, inner.w + 12.0, inner.bottom() - y);
    l.ui.scroll_area("mods-list", list, &mut |ui, v| {
        if shown.is_empty() {
            let t = if empty { "No mods yet. Choose a folder or an archive on the left, or drop one onto the window." } else { "No mod matches." };
            ui.paragraph(t, Vec2::new(v.x + 8.0, v.y + 6.0), v.w - 16.0, 12.5, Weight::Regular, TEXT_DIM);
            return 40.0;
        }
        let rh = 54.0;
        // (an opened mod shows what it holds under its row, a line per folder of the game it
        // put things into: Vehicles, Fonts, Texture, ...)
        let line_h = 18.0;
        let mut top = v.y;
        for m in shown.iter() {
            let groups = if opened.contains(&m.id) { mod_tree(&m.paths) } else { Vec::new() };
            let lines: usize = groups.iter().map(|(_, items)| items.len().div_ceil(3).max(1)).sum();
            let extra = if groups.is_empty() { 0.0 } else { lines as f32 * line_h + 8.0 };
            let full = Rect::new(v.x + 6.0, top, v.w - 16.0, rh - 6.0 + extra);
            top += rh + extra;
            if full.bottom() < list.y - rh || full.y > list.bottom() + rh {
                continue;
            }
            let r = full;
            let asking = confirm.as_deref() == Some(m.id.as_str());
            ui.p().rounded(full, 8.0, if asking { DANGER.alpha(0.12) } else { Color::WHITE.alpha(if m.enabled { 0.04 } else { 0.015 }) });
            if !groups.is_empty() {
                let mut ly = r.y + rh - 8.0;
                for (folder, items) in &groups {
                    for (row, chunk) in items.chunks(3).enumerate() {
                        if row == 0 {
                            ui.text_in(folder, Rect::new(r.x + 44.0, ly, 110.0, line_h), 11.5, Weight::Medium, TEXT_DIM, Align::Left);
                        }
                        ui.text_in(&chunk.join("   "), Rect::new(r.x + 156.0, ly, r.w - 170.0, line_h), 11.5, Weight::Regular, TEXT_FAINT, Align::Left);
                        ly += line_h;
                    }
                }
            }
            let r = Rect::new(r.x, r.y, r.w, rh - 6.0);
            let icon = match m.kind {
                Kind::Bus => "directions_bus",
                Kind::Map => "map",
                Kind::Archive => "inventory_2",
                Kind::Other => "extension",
            };
            ui.icon(icon, Vec2::new(r.x + 22.0, r.center().y), 20.0, if m.enabled { accent() } else { TEXT_FAINT });
            let tw = r.w - 230.0;
            ui.text_in(&m.name, Rect::new(r.x + 44.0, r.y + 6.0, tw, 20.0), 13.5, Weight::Medium, if m.enabled { TEXT } else { TEXT_DIM }, Align::Left);
            let open = opened.contains(&m.id);
            if ui.icon_button(&format!("mod-open-{}", m.id), Vec2::new(r.right() - 130.0, r.y + 24.0), 17.0, if open { "expand_less" } else { "expand_more" }, "What this mod holds, folder by folder") {
                open_toggle = Some(m.id.clone());
            }
            // (the folders it holds: under the row once opened by its arrow)
            let folders = mod_tree(&m.paths).iter().map(|(f, items)| format!("{f} {}", items.len())).collect::<Vec<_>>().join(", ");
            let mut sub = vec![folders, fmt_bytes(m.bytes)];
            if !m.enabled {
                sub.insert(0, "OFF".into());
            }
            if m.installed > 0 {
                sub.push(format!("installed {}", date_text(m.installed, offset, false)));
            } else if !m.noted {
                sub.push("found in the content folder".into());
            }
            ui.text_in(&sub.join(" | "), Rect::new(r.x + 44.0, r.y + 26.0, tw, 16.0), 11.0, Weight::Regular, TEXT_FAINT, Align::Left);
            if busy.as_deref() == Some(m.id.as_str()) {
                ui.text_in("...", Rect::new(r.right() - 60.0, r.y, 40.0, r.h), 16.0, Weight::Bold, TEXT_DIM, Align::Center);
                continue;
            }
            if asking {
                // the question, in the row: deleting cannot be undone
                if ui.button(&format!("mod-del-yes-{}", m.id), Rect::new(r.right() - 96.0, r.y + 9.0, 88.0, 30.0), "Delete", Some("delete"), ButtonKind::Danger) {
                    delete = Some(m.id.clone());
                }
                if ui.button(&format!("mod-del-no-{}", m.id), Rect::new(r.right() - 190.0, r.y + 9.0, 86.0, 30.0), "Keep", None, ButtonKind::Normal) {
                    ask = Some(None);
                }
                continue;
            }
            let mut on = m.enabled;
            if ui.toggle(&format!("mod-on-{}", m.id), Rect::new(r.right() - 96.0, r.y + 10.0, 50.0, 28.0), &mut on, "") {
                toggle = Some((m.id.clone(), on));
            }
            let dr = Rect::new(r.right() - 38.0, r.y + 10.0, 28.0, 28.0);
            if ui.icon_button(&format!("mod-del-{}", m.id), dr.center(), 17.0, "delete", "Delete this mod (asks first; switch it off instead to keep it)") {
                ask = Some(Some(m.id.clone()));
            }
        }
        top - v.y
    });
    if let Some(id) = open_toggle {
        if !l.pages.mod_open.remove(&id) {
            l.pages.mod_open.insert(id);
        }
    }
    if let Some((id, on)) = toggle {
        l.state.mod_toggle(id, on);
    }
    if let Some(a) = ask {
        l.pages.mod_confirm = a;
    }
    if let Some(id) = delete {
        l.pages.mod_confirm = None;
        l.state.mod_remove(id);
    }
}

/// A mod's paths grouped by the game's folder they lie in (`Vehicles`, `Fonts`, `Texture`,
/// ...), each with what of it is the mod's: `[("Fonts", ["a.oft", "b.oft"]), ...]`.
fn mod_tree(paths: &[String]) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for p in paths {
        let (folder, rest) = p.split_once('/').unwrap_or(("", p.as_str()));
        let folder = if folder.is_empty() { "(top)" } else { folder };
        match out.iter_mut().find(|(f, _)| f.eq_ignore_ascii_case(folder)) {
            Some((_, items)) => items.push(rest.to_string()),
            None => out.push((folder.to_string(), vec![rest.to_string()])),
        }
    }
    out
}

#[cfg(test)]
mod mod_tree_tests {
    #[test]
    fn a_mods_paths_are_grouped_by_folder() {
        let t = super::mod_tree(&["Vehicles/MAN_SD202".into(), "Fonts/a.oft".into(), "Fonts/b.oft".into(), "Texture/Signs".into()]);
        assert_eq!(t, vec![("Vehicles".to_string(), vec!["MAN_SD202".to_string()]), ("Fonts".to_string(), vec!["a.oft".to_string(), "b.oft".to_string()]), ("Texture".to_string(), vec!["Signs".to_string()])]);
    }
}
