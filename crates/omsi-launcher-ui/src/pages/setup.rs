//! The Setup and Tutorials pages.

use crate::host::host;
use super::*;
use omsi_launcher_lib as core;

// --- setup -----------------------------------------------------------------------------------------

pub fn setup(l: &mut Launcher, area: Rect) {
    let body = area;
    let r = Rect::new(body.x, body.y, body.w.min(820.0), 300.0);
    l.ui.card(r);
    let inner = l.ui.heading(Rect::new(r.x + 20.0, r.y + 16.0, r.w - 40.0, r.h - 32.0), "Folders", Some("folder_open"));
    let mut root = l.pages.setup_root.clone().unwrap_or_else(|| l.state.config.root.clone());
    let mut game = l.pages.setup_game.clone().unwrap_or_else(|| l.state.config.game.clone());
    let mut y = inner.y;
    l.ui.label(Rect::new(inner.x, y, 150.0, ROW), "OMSI 2 folder");
    if l.ui.text_input("cfg-root", Rect::new(inner.x + 150.0, y, inner.w - 150.0 - 110.0, ROW), &mut root, "/path/to/OMSI 2", Some("folder_open")) {
        l.pages.setup_root = Some(root.clone());
    }
    if l.ui.button("browse-root", Rect::new(inner.right() - 100.0, y, 100.0, ROW), "Browse", None, ButtonKind::Normal) {
        if crate::mobile::mobile() {
            let start = root.clone();
            l.browse(crate::mobile::Purpose::Root, &start);
        } else if let Some(p) = core::pick_folder("The OMSI 2 folder (with maps and Vehicles in it)") {
            l.pages.setup_root = Some(p.to_string_lossy().to_string());
        }
    }
    y += ROW + 10.0;
    if core::IN_PROCESS_GAMES {
        // (a phone: the game is this app itself)
        y += l.ui.paragraph("Copy the whole OMSI 2 folder (with maps and Vehicles in it) onto the phone - by cable, from a PC or a USB stick - for example as openOMSI/OMSI 2 in the internal storage, then choose it here with Browse. Mods go into openOMSI/Mods or are installed from the Mods page.", Vec2::new(inner.x, y), inner.w, 12.5, Weight::Regular, TEXT_DIM);
    } else {
        l.ui.label(Rect::new(inner.x, y, 150.0, ROW), "Game binary");
        if l.ui.text_input("cfg-game", Rect::new(inner.x + 150.0, y, inner.w - 150.0 - 110.0, ROW), &mut game, "openomsi", Some("terminal")) {
            l.pages.setup_game = Some(game.clone());
        }
        if l.ui.button("browse-game", Rect::new(inner.right() - 100.0, y, 100.0, ROW), "Browse", None, ButtonKind::Normal) {
            if let Some(p) = core::pick_file("The openomsi program") {
                l.pages.setup_game = Some(p.to_string_lossy().to_string());
            }
        }
        y += ROW + 16.0;
        y += l.ui.paragraph("The OMSI 2 folder is the one with maps and Vehicles in it (any complete installation). The game binary is the openomsi program; it is found by itself when it sits next to the launcher.", Vec2::new(inner.x, y), inner.w, 12.5, Weight::Regular, TEXT_DIM);
    }
    y += 12.0;
    if l.ui.button("cfg-save", Rect::new(inner.x, y, 180.0, 42.0), "Save", Some("save"), ButtonKind::Primary) {
        let chosen = root.trim().to_string();
        // A folder that is no complete installation is said so, with what it lacks, and
        // nothing is saved: it was saved, then quietly replaced by the installation found
        // before, while the page said "Saved" (#1656).
        let missing = if chosen.is_empty() { Vec::new() } else { omsi_cfg::missing_original_essentials(std::path::Path::new(&chosen)) };
        if !missing.is_empty() {
            let shown: Vec<&str> = missing.iter().map(String::as_str).take(6).collect();
            let more = if missing.len() > shown.len() { format!(" and {} more", missing.len() - shown.len()) } else { String::new() };
            l.state.set_status(format!("Not saved: {chosen} is not a complete OMSI 2 installation - it lacks {}{more}", shown.join(", ")), true);
        } else {
            l.state.config.root = chosen.clone();
            l.state.config.game = game.trim().to_string();
            match core::save_config(&l.state.config) {
                Ok(()) => {
                    // (what was read of the folders before is forgotten: a folder copied or
                    // changed while the launcher ran is read as it is now)
                    omsi_cfg::content_changed();
                    l.state.config = core::load_config();
                    l.pages.setup_root = None;
                    l.pages.setup_game = None;
                    let same = chosen.is_empty() || std::path::Path::new(&chosen) == std::path::Path::new(&l.state.config.root);
                    if same {
                        l.state.set_status("Saved. Reading the content again...", false);
                    } else {
                        l.state.set_status(format!("Saved, but the OMSI 2 folder used is {}", l.state.config.root), true);
                    }
                    // (a phone: the gallery is kept out of the folder chosen at once, before
                    // its media scanner lists its textures as photos, #1632)
                    #[cfg(target_os = "android")]
                    host().hide_content_from_gallery();
                    l.state.load_content();
                }
                Err(e) => l.state.set_status(format!("{e:#}"), true),
            }
        }
    }
    support_panel(l, Rect::new(body.x, r.bottom() + 16.0, body.w.min(820.0), 0.0));
}

/// Setup's support package (see `support_bundle`): what it holds, said before it is made,
/// and the button that saves it on this computer (nothing is sent).
fn support_panel(l: &mut Launcher, r: Rect) {
    const ABOUT: &str = "A ZIP for a bug report: the program and system versions, the graphics card, its driver and the graphics settings, the controllers, and the map and bus of the last game. No folders, names, chat, LAN codes or addresses; of the logs only which events happened. It is saved on this computer and its folder opened, so you can look inside before you attach it to an issue.";
    // (as high as its text needs: it wraps to more lines on a phone)
    let r = Rect::new(r.x, r.y, r.w, 120.0 + l.ui.paragraph_height(ABOUT, r.w - 40.0, 12.5, Weight::Regular));
    l.ui.card(r);
    let inner = l.ui.heading(Rect::new(r.x + 20.0, r.y + 16.0, r.w - 40.0, r.h - 32.0), "Support package", Some("help"));
    let h = l.ui.paragraph(ABOUT, Vec2::new(inner.x, inner.y), inner.w, 12.5, Weight::Regular, TEXT_DIM);
    if l.ui.button("export-diagnostics", Rect::new(inner.x, inner.y + h + 12.0, 220.0, 42.0), "Export diagnostics", Some("download"), ButtonKind::Normal) {
        let c = &l.state.choice;
        let job = host().support_bundle(l.renderer.as_ref(), l.pages.pads.io.as_ref().map(|d| d.connected()), (&c.map, &c.bus, c.line.as_deref(), c.tour.as_deref()));
        l.state.spawn(move || crate::state::Msg::Diagnostics(job()));
    }
}


// --- tutorials --------------------------------------------------------------------------------

/// OMSI's four tutorials: what each teaches, and a button to start it.
pub fn tutorials(l: &mut Launcher, area: Rect) {
    let body = area;
    static LIST: std::sync::OnceLock<Vec<(usize, String, String)>> = std::sync::OnceLock::new();
    let list = LIST.get_or_init(omsi_launcher_lib::tutorials);
    if list.is_empty() {
        l.ui.paragraph("No tutorials were found in the OMSI 2 folder (Tutorials).", Vec2::new(body.x, body.y), body.w, 13.0, Weight::Regular, TEXT_DIM);
        return;
    }
    let cols = 2;
    let cw = (body.w - GAP * 2.0) / cols as f32;
    let ch = ((body.h - GAP * 2.0) / 2.0).min(330.0);
    let mut start = None;
    for (k, (n, title, text)) in list.iter().enumerate() {
        let r = Rect::new(body.x + (k % cols) as f32 * (cw + GAP * 2.0), body.y + (k / cols) as f32 * (ch + GAP * 2.0), cw, ch);
        l.ui.card(r);
        l.ui.text_in(title, Rect::new(r.x + 18.0, r.y + 14.0, r.w - 36.0, 26.0), 17.0, Weight::Bold, TEXT, Align::Left);
        l.ui.push_clip(Rect::new(r.x + 18.0, r.y + 46.0, r.w - 36.0, r.h - 110.0), 0.0);
        l.ui.paragraph(text, Vec2::new(r.x + 18.0, r.y + 46.0), r.w - 36.0, 12.5, Weight::Regular, TEXT_DIM);
        l.ui.pop_clip();
        if l.ui.button(&format!("tut-{n}"), Rect::new(r.x + 18.0, r.bottom() - 58.0, 200.0, 40.0), "Start the lesson", Some("play_arrow"), ButtonKind::Primary) {
            start = Some(*n);
        }
    }
    if let Some(n) = start {
        l.state.launch_tutorial(n);
    }
}
