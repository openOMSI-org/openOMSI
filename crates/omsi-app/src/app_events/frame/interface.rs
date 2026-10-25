//! The interface over the picture in the window's frame: the notes, the navigator, the
//! plugins' panels, the menus and the rest of the HUD.

use super::*;

impl App {
    /// The interface over the picture. The VR navigator's display (for the headset's
    /// picture).
    pub(super) fn frame_ui(&mut self, dt: f32) -> Option<crate::vr_navigator::Display> {
        // (the game menu's lines, for the interface below)
        let menu_lines = if self.menus.game_menu.is_some() { self.game_menu_items() } else { Vec::new() };
        // (the mirror editor's keys and the panel under the cursor, while it is on)
        let mirror_help = match (self.player.as_ref(), self.mirror_hud_size()) {
            (Some(p), Some(size)) => self.gfx.mirror_hud.help_lines(p, self.hud_cursor(), size),
            _ => Vec::new(),
        };
        let vr_nav_display = self.vr_nav_display();
        let vr_active = self.vr_active();
        // the interface over the picture
        // (the pages of an open settings window)
        let menu_tabs = match self.menus.list_kind.as_ref() {
            Some(k) if self.menus.chooser.is_some() => crate::game_lists::page_titles(self, k),
            _ => None,
        };
        if !(self.world.is_some() && self.renderer.is_some() && self.scene.is_some()) {
            return vr_nav_display;
        }
        // the photo mode: the photo and its panel, none of the game's interface
        if self.photo.is_some() {
            if let Some(scene) = self.scene.as_mut() {
                scene.overlays.clear();
            }
            self.frame_navigator(dt, [0.0, 0.0, 1.0, 1.0], vr_active, true, vr_nav_display);
            return vr_nav_display;
        }
        let (notes, tooltip) = self.frame_notes(dt, mirror_help);
        let __t = Instant::now();
        // (the frame's overlays start empty; the notes are the interface's, in
        // Inter - OMSI's bitmap font HUD is the start menu's and the offscreen
        // pictures' only)
        if let Some(scene) = self.scene.as_mut() {
            scene.overlays.clear();
        }
        let hud = self
            .gfx.surface
            .as_ref()
            .map(|s| {
                self.settings
                    .hud_viewport((s.config.width, s.config.height))
            })
            .unwrap_or([0.0, 0.0, 1.0, 1.0]);
        // (not under the open game menu: the pause menu's rail covers the left
        // edge, and the navigator's panel stood out from under it)
        let nav_hidden = !vr_active && self.menus.game_menu.is_some();
        // (a plugin's panel for the navigator's board, as tall as it was laid out last: the
        // navigator makes room for it and says where it goes - under its map, or the whole
        // frame for one that fills it, its map where the panel has it)
        if let Some(n) = self.menus.navigator.as_mut() {
            let docked = self.integrations.plugin_panels.docked;
            n.dock = docked.filter(|d| !d.frame).map(|d| d.height);
            n.frame_panel = docked.filter(|d| d.frame).map(|d| navigator::FramePanel {
                height: d.height,
                map: d.map.map(|m| navigator::FrameMap { rect: m.rect, cards: m.cards }),
                background: d.background,
            });
        }
        self.frame_navigator(dt, hud, vr_active, nav_hidden, vr_nav_display);
        // the Lua plugins' panels (`omsi.ui`): over the picture and the navigator,
        // under the game's own interface; not under its menus, nor in VR
        let plugin_focus = crate::plugin_ui::focused(&self.integrations.plugins);
        self.frame_plugin_panels(dt, hud, vr_active);
        // the game's own panels (the quick menu, the trip summary, ...: `game_panels`)
        self.frame_game_panels(dt, hud, vr_active);
        // the mirror panels (Ctrl+M): over the picture and the navigator, under the notes
        // and the menus - drawn after them, they covered the pause menu (#1880)
        if self.cam.in_cab {
            if let (Some(r), Some(scene), Some(w)) = (self.renderer.as_ref(), self.scene.as_mut(), self.world.as_ref()) {
                self.gfx.mirror_hud.ensure_frame(r, scene);
                steps::push_mirror_hud(&self.gfx.mirror_hud, scene, w, hud, (self.input.cursor.0 - hud[0], self.input.cursor.1));
            }
        }
        self.frame_ui_draw(dt, hud, vr_active, plugin_focus, &notes, tooltip, menu_lines, menu_tabs);
        // the loading screen fading over the first pictures, over all of the interface
        self.frame_loading_fade();
        // the pause menu, drawn with the launcher's toolkit over all of it
        self.frame_shell(dt);
        self.perf.profile.add(crate::profile_keys::ProfKey::Hud, __t.elapsed().as_secs_f64());
        vr_nav_display
    }

    /// What the game tells the driver (why the bus is not moving, the trip's end, the
    /// messages, the LAN and the passengers), turned into notifications when it starts
    /// (`ui::NoteWatch`); the editors' keys, for the top left corner while one is on; and
    /// the tooltip of the switch under the cursor.
    fn frame_notes(&mut self, dt: f32, mirror_help: Vec<String>) -> (Vec<String>, Option<String>) {
        use crate::ui::{note_key, NoticeKind};
        // (key, text, kind): the key names the condition, whose text may carry a value
        let mut said: Vec<(String, String, NoticeKind)> = Vec::new();
        let mut say = |key: String, text: String, kind: NoticeKind| said.push((key, text, kind));
        // why the bus is not moving, whenever the throttle is pressed and nothing
        // happens: the things a driver checks first
        if let Some(p) = self.player.as_ref() {
            for t in standing_reasons(&p.vehicle, &|a| crate::diagnostics::rebound_key(&p.bindings, a)) {
                say(note_key(&t), t, NoticeKind::Action);
            }
        }
        // the latest `$msg` of the player's bus scripts, which OMSI writes on the screen: a
        // note while the script keeps saying it and for a few seconds after (AI vehicles'
        // messages are not shown)
        let script_says = self.player.as_ref().and_then(|p| p.vehicle.host.fresh_message(SCRIPT_MESSAGE_SECS));
        // (one note whose text follows the script: a value it prints changes every frame)
        if let Some(text) = script_says {
            say("bus script message".to_string(), text.to_string(), NoticeKind::Info);
        }
        // what is under the cursor, in the player's language (the scripts only
        // know internal, mostly German names)
        let names = describe::names(&self.args.root, &self.settings.language);
        // next to the cursor (`ui`), when the setting asks for it
        let tooltip = self.menus.hover.as_ref().map(|h| names.control(h));
        // the editors' keys while one is on: help to look up, not news - they stay in the
        // corner as long as the editor does
        let mut lines: Vec<String> = mirror_help;
        if self.menus.editor.is_some() {
            lines.push("Object editor: click picks | drag moves | wheel turns (Shift lifts) | Del | C copy | V variant | Backspace undo | Ctrl+S save | Esc".into());
        }
        if let Some(d) = self.session.duty.as_ref().filter(|d| d.trip_done()) {
            let text = match d.trips.get(d.trip_index + 1) {
                Some(next) => format!(
                    "End of the trip. Next: {} to {}, from {} at {} (it starts by itself a minute before)",
                    if next.line.trim().is_empty() { omsi_ui::tr("service trip").into_owned() } else { omsi_ui::tr("line {}").replacen("{}", &next.line, 1) },
                    next.terminus.strip_prefix(&format!("{} ", next.line)).unwrap_or(&next.terminus),
                    next.stops.first().map(|s| s.name.trim()).unwrap_or("?"),
                    crate::schedule::hhmm(next.departure)
                ),
                None => "End of the duty: the tour's last trip is done".into(),
            };
            say(format!("trip end {}", d.trip_index), text, NoticeKind::Success);
        }
        // the game's messages (a setting saved, a key to press, the bus not loaded): one
        // card each, while the message stands
        if let Some((msg, left)) = self.service_msg.as_mut() {
            *left -= dt;
            if *left > 0.0 {
                let kind = message_kind(&omsi_ui::i18n::english(msg));
                say(format!("message {msg}"), msg.clone(), kind);
            }
        }
        self.service_msg = self.service_msg.take().filter(|(_, l)| *l > 0.0);
        if let Some(lan) = self.net.lan.as_ref() {
            for t in lan::hud_lines(lan) {
                say(t.clone(), t, NoticeKind::Info);
            }
            if let Some(v) = self.sound.voice.as_ref() {
                for t in v.notices() {
                    say(t.clone(), t, NoticeKind::Info);
                }
                // our own key: what we are transmitting on, and who is talking to us
                let tr = |s: &str| omsi_ui::tr(s).into_owned();
                match v.transmitting() {
                    Some(omsi_voice::Channel::Radio) => say("voice transmit".into(), tr("Voice: you are talking on the radio"), NoticeKind::Action),
                    Some(omsi_voice::Channel::Nearby) => say("voice transmit".into(), tr("Voice: you are talking to the players nearby"), NoticeKind::Action),
                    None => {}
                }
                if self.settings.voice_talker_list {
                    for (id, name, channel) in &v.talking {
                        let how = match channel {
                            omsi_voice::Channel::Radio => tr("on the radio"),
                            omsi_voice::Channel::Nearby => tr("nearby"),
                        };
                        let name = if name.is_empty() { format!("#{id}") } else { name.clone() };
                        say(format!("voice talker {id}"), format!("{name} - {how}"), NoticeKind::Info);
                    }
                }
            }
        }
        if let Some(h) = self.session.humans.as_ref() {
            if let Some(hint) = h.hint() {
                say(hint.clone(), hint, NoticeKind::Action);
            } else if let Some((name, value)) = &h.request {
                let t = omsi_ui::tr("Passenger wants: {name}  {value:.2}").replace("{name}", name).replace("{value:.2}", &format!("{value:.2}"));
                say(t.clone(), t, NoticeKind::Action);
            }
            if let Some((paid, value)) = h.paid {
                let t = format!("paid: {paid:.2}  (change {:.2})", (paid - value).max(0.0));
                say(t.clone(), t, NoticeKind::Info);
            }
            if let Some(owed) = h.change_due {
                let t = omsi_ui::tr("Change due: {owed:.2}").replace("{owed:.2}", &format!("{owed:.2}"));
                say(t.clone(), t, NoticeKind::Action);
            }
        }
        // (the cursor no longer works the cab: how to have it back)
        if crate::plugin_ui::focused(&self.integrations.plugins) {
            say("plugin focus".into(), crate::plugin_ui::FOCUS_NOTE.into(), NoticeKind::Action);
        }
        // (switched off, `Settings::notes`: the game keeps its hints to itself; the
        // server's, the plugins' and the update check's notifications still come)
        if !self.settings.notes {
            said.clear();
        }
        self.menus.note_watch.frame(dt, &said, &mut self.menus.notices);
        self.integrations.update_watch.tick(&mut self.menus.notices);
        if let Some((before, text, left)) = omsi_lan::mods::upload::notice() {
            self.menus.notices.retain(|n| n.text != before);
            crate::ui::push_notice(&mut self.menus.notices, crate::ui::Notice { kind: crate::ui::NoticeKind::Info, text, left, total: left });
        }
        for n in self.menus.notices.iter_mut() {
            n.left -= dt;
        }
        self.menus.notices.retain(|n| n.left > 0.0);
        (lines, tooltip)
    }

    /// The loading screen after the world is there: its bar runs to the end, then it fades
    /// over the session's first pictures (`ui::Loading`).
    fn frame_loading_fade(&mut self) {
        if self.menus.loading.over_session().is_none() {
            return;
        }
        // (by its own clock: the frame's time is held short for the simulation, and the
        // session's first frames are slow)
        let dt = self.menus.loading.tick();
        self.menus.loading.step(1.0, dt);
        let Some(opacity) = self.menus.loading.over_session() else { return };
        let (Some(r), Some(scene), Some(ui), Some(s)) = (self.renderer.as_ref(), self.scene.as_mut(), self.ui.as_mut(), self.gfx.surface.as_ref()) else { return };
        let dpi = self.window.as_ref().map(|w| w.scale_factor() as f32).unwrap_or(1.0);
        let scale = dpi * ui::size_factor(s.config.height as f32, dpi, self.settings.ui_scale, self.settings.ui_scale_window);
        let title = std::mem::take(&mut self.menus.loading.title);
        ui.loading_screen(r, scene, s.config.width as f32, s.config.height as f32, scale, &title, self.menus.loading.shown(), opacity, self.menus.loading.clock(), self.menus.loading.tip());
        self.menus.loading.title = title;
    }

    /// The navigator and OMSI 2's dynamic route arrows.
    fn frame_navigator(
        &mut self,
        dt: f32,
        hud: [f32; 4],
        vr_active: bool,
        nav_hidden: bool,
        vr_nav_display: Option<crate::vr_navigator::Display>,
    ) {
        let (Some(r), Some(scene)) = (self.renderer.as_ref(), self.scene.as_mut()) else { return };
        // (on foot with no bus of one's own the navigator and the city map go on as well:
        // a walker who had deleted the bus could not open the map, #2118)
        let p = self.player.as_ref();
        let walking = self.session.on_foot.is_some();
        if let (Some(nav), Some(_), true) = (
            self.menus.navigator.as_mut(),
            self.gfx.surface.as_ref(),
            p.is_some() || walking,
        ) {
            let old_enabled = nav.enabled;
            let old_opacity = nav.opacity;
            nav.cockpit_display = vr_active;
            if vr_active {
                nav.enabled = vr_nav_display.is_some_and(|d| d.placement.enabled);
                nav.opacity = vr_nav_display.map(|d| d.placement.opacity).unwrap_or(0.95);
            }
            if nav_hidden {
                nav.enabled = false;
            }
            if let Some(w) = self.world.as_ref() {
                nav.start_map(w.clone());
            }
            // stops beyond the loaded tiles: their places from the navigator's map
            if let (Some(places), Some(d)) = (nav.places(), self.session.duty.as_mut()) {
                if !self.session.duty_places {
                    self.session.duty_places = true;
                    d.learn_places(places);
                }
            }
            let (line, terminus, stops, trip) = navigator::duty_parts(self.session.duty.as_ref());
            match (trip, self.session.schedule.as_ref(), self.session.traffic.as_ref(), self.world.as_ref()) {
                (Some((key, name)), Some(sch), _, _) if nav.map_net().is_some() => {
                    if nav.wants_route(&key, 0) {
                        let lanes = sch.trip_route_in(nav.map_net().unwrap(), &name);
                        let g = nav.global_version + (1 << 40);
                        nav.set_route(&key, lanes, true, g);
                    }
                }
                (Some((key, name)), Some(sch), Some(t), Some(w)) => {
                    if nav.wants_route(&key, t.lanes_generation) {
                        let (lanes, complete) = sch.trip_route(w, t, &name);
                        nav.set_route(&key, lanes, complete, t.lanes_generation);
                    }
                }
                _ => nav.clear_route(),
            }
            if let Some(sch) = self.session.schedule.as_ref() {
                if nav.map_net().is_some() && !nav.stop_names_learnt() {
                    nav.learn_stop_names(&sch.stop_names());
                }
            }
            // the timetable's vehicles the traffic does not drive, where the timetable has them
            if let (Some(sch), true) = (self.session.schedule.as_ref(), nav.show_ai && (nav.enabled || nav.map_open())) {
                let cars: Vec<(DVec3, String)> = match self.session.traffic.as_ref().filter(|_| nav.live_due(dt)) {
                    Some(t) => t.cars.iter().filter(|c| !c.gone && c.bus.is_some()).map(|c| (c.vehicle.position, c.vehicle.host.tt_line.clone())).collect(),
                    None => Vec::new(),
                };
                let ground = self.world.as_ref().map(|w| w.data());
                nav.update_live(sch, self.clock.time, dt, &cars, &|p| ground.as_ref().is_some_and(|d| d.has_ground(p.x, p.y)));
            }
            let (outside_temp, inside_temp) = match p {
                Some(p) => vehicle_temperatures(p),
                None => {
                    let t = omsi_sim::host::ambient_weather().map_or(15.0, |w| w.0);
                    (t, t)
                }
            };
            // (on foot the map follows the walker, not the bus left standing - but on a duty it
            // stays the bus's: the route and the stops are the bus's, and walking round the bus
            // standing at a stop recalculated the route and moved the stops on, #2426)
            let (at, heading) = match (self.session.on_foot.as_ref(), p) {
                (Some(_), Some(p)) if self.session.duty.is_some() => (p.vehicle.position, p.vehicle.heading),
                (Some(f), _) => (f.pos, f.heading),
                (None, Some(p)) => (p.vehicle.position, p.vehicle.heading),
                (None, None) => return,
            };
            let frame = navigator::NavFrame {
                traffic: self.session.traffic.as_deref(),
                touch: crate::platform::touch_controls(),
                players: self.net.lan.as_ref().map(|l| crate::lan::nav_players(&self.net.remotes, l.my_id)).unwrap_or_default(),
                bus: at,
                heading,
                speed_kmh: p.map_or(0.0, |p| p.vehicle.physics.velocity_kmh()),
                outside_temp,
                inside_temp,
                line,
                terminus,
                stops,
                delay: self.session.duty.as_ref().zip(p).map(|(_, p)| p.vehicle.host.tt_delay as f64),
                passengers: self.session.humans.as_ref().map(|h| h.riding()),
                stop_requested: p.is_some_and(|p| navigator::stop_requested(&p.vehicle)),
                time: self.clock.time,
                weekday: self.clock.weekday(),
                date: self.clock.date_code(),
                language: &self.settings.language,
                screen: if vr_active {
                    (1440.0, 1440.0)
                } else {
                    (hud[2], hud[3])
                },
                ui_scale: if vr_active {
                    1.0
                } else {
                    self.settings.ui_scale
                },
                follow_window: if vr_active {
                    true
                } else {
                    self.settings.ui_scale_window
                },
                dt,
                info_rect: self.ui.as_ref().and_then(|u| u.info_rect).filter(|_| !vr_active),
                touch_gap: self.input.touch.top_gap.filter(|_| self.input.touch.enabled && !vr_active).map(|[l, r, b]| [l - hud[0], r - hud[0], b]),
            };
            let __tn = Instant::now();
            nav.frame_at(r, scene, &frame, hud[0]);
            nav.enabled = old_enabled;
            nav.opacity = old_opacity;
            self.perf.profile.add(crate::profile_keys::ProfKey::HudNavigator, __tn.elapsed().as_secs_f64());
            // OMSI 2's dynamic route arrows over the junctions ahead
            if nav.arrows {
                if let Some(w) = self.world.as_ref() {
                    let spots = nav.arrow_spots(self.session.traffic.as_ref().map(|t| &t.net), 350.0, &|id| w.data.peek().objects.get(&id).map(|p| (p.0, p.1[0])));
                    self.gfx.route_arrows.tick(dt, w, r, scene, &spots);
                }
            } else if self.gfx.route_arrows.any() {
                // (switched off in the menu: the ones standing go too)
                if let Some(w) = self.world.as_ref() {
                    self.gfx.route_arrows.clear(w, r, scene);
                }
            }
        }
    }

    /// The Lua plugins' panels (`omsi.ui`).
    fn frame_plugin_panels(&mut self, dt: f32, hud: [f32; 4], vr_active: bool) {
        let (Some(r), Some(scene)) = (self.renderer.as_ref(), self.scene.as_mut()) else { return };
        if let Some(plugin_ui) = self.integrations.plugins.as_ref().map(|p| p.ui.clone()) {
            let dpi = self
                .window
                .as_ref()
                .map_or(1.0, |w| w.scale_factor() as f32);
            let settings = &self.settings;
            let size = ui::size_factor(
                hud[3],
                dpi,
                settings.ui_scale,
                settings.ui_scale_window,
            );
            let map_open = self.menus.navigator.as_ref().is_some_and(|n| n.map_open());
            let frame = crate::plugin_ui::PanelsFrame {
                hud,
                scale: dpi * size,
                hidden: vr_active
                    || self.menus.game_menu.is_some()
                    || self.menus.chooser.is_some()
                    || map_open,
                cursor: self.input.cursor,
                dt,
                backdrop: ui::backdrop(settings.ui_opacity),
                navigator: self.menus.navigator.as_ref().and_then(|n| n.screen_rect()),
                dock: self.menus.navigator.as_ref().and_then(|n| n.dock_slot).map(|(r, scale, placed, frame)| crate::plugin_ui::Dock { width: r[2], room: r[3], scale, at: placed.then(|| glam::Vec2::new(r[0], r[1])), frame }),
            };
            let mut state = plugin_ui.borrow_mut();
            self.integrations.plugin_panels.frame(r, scene, &mut state, &frame);
        }
    }

    /// The game's own interface: the menus, the chat, the name tags, the notes, the
    /// timetable, the info bar, the tutorial.
    #[allow(clippy::too_many_arguments)]
    fn frame_ui_draw(
        &mut self,
        dt: f32,
        hud: [f32; 4],
        vr_active: bool,
        plugin_focus: bool,
        notes: &[String],
        tooltip: Option<String>,
        menu_lines: Vec<crate::app_impl::MenuLine>,
        menu_tabs: Option<(Vec<String>, usize)>,
    ) {
        let menu_lines: Vec<(&str, &str)> = menu_lines.iter().map(|(id, label, _)| (*id, label.as_ref())).collect();
        // (the steering cross: before the renderer and the scene are borrowed)
        let steer_cross = self.steer_cross_point();
        self.refresh_perf_overlay();
        // (the pause menu is the launcher toolkit's, `frame_shell`, unless a headset shows it)
        let shell_menu = self.shell_takes_mouse();
        let (Some(r), Some(scene)) = (self.renderer.as_ref(), self.scene.as_mut()) else { return };
        if let (Some(ui), Some(s)) = (self.ui.as_mut(), self.gfx.surface.as_ref()) {
            let scale = self.window.as_ref().map(|w| w.scale_factor() as f32).unwrap_or(1.0);
            let (w, h) = (hud[2], hud[3]);
            self.net.remotes.chat.disabled = !self.settings.chat;
            let chat = (self.net.lan.is_some() && self.settings.chat).then(|| ui::ChatView {
                lines: &self.net.remotes.chat.lines,
                typing: self.net.remotes.chat.typing.as_deref(),
                error: self.net.remotes.chat.error(),
                open_key: self
                    .input
                    .game_keys
                    .iter()
                    .find(|b| b.action.eq_ignore_ascii_case("chat_open"))
                    .map(|b| (b.scan_code != 0).then(|| crate::keys::key_name(b.scan_code as i64, b.modifier as i64)))
                    .unwrap_or_else(|| Some("/".into())),
            });
            ui.chat.hidden = self.net.remotes.chat.hidden;
            let mut tags = if self.settings.name_tags {
                let voice = self.sound.voice.as_ref();
                let speaks = |_name: &str, id: u32| voice.is_some_and(|v| v.speaks(id).is_some());
                let on_radio = |_name: &str, id: u32| voice.is_some_and(|v| v.speaks(id) == Some(omsi_voice::Channel::Radio));
                let rig = (self.settings.triple.enabled
                    && !self.settings.vr_requested())
                .then(|| {
                    self.settings.triple.zoomed(
                        s.config.width,
                        s.config.height,
                        self.cam.view_zoom.get(&self.view).copied().unwrap_or(1.0),
                    )
                });
                self.camera
                    .as_ref()
                    .map(|c| {
                        lan::name_tags(
                            &self.net.remotes,
                            c,
                            s.config.width as f32,
                            h,
                            rig.as_ref(),
                            &speaks,
                            &on_radio,
                        )
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            for (pos, _, _, _) in &mut tags {
                pos.0 -= hud[0];
            }
            // the vehicle chooser shows its vehicles in the menu's place (the menu
            // scrolls a long list)
            // the name of the cab's switch under the cursor, unless the interface
            // covers the cab there (it read like a line of the menu over it)
            let (cx, cy) = self.input.cursor;
            let map_open = self.menus.navigator.as_ref().is_some_and(|n| n.map_open());
            let covered = self.menus.game_menu.is_some()
                || plugin_focus
                || self.xr.vr_nav_edit.is_some()
                || self.menus.chooser.is_some()
                || ui.chat.hovered
                || map_open
                || (!vr_active && self.menus.navigator.as_ref().is_some_and(|n| n.over_panel(cx, cy)));
            let dropdown = self.menus.dropdown.as_ref().filter(|_| self.menus.chooser.is_some()).map(|d| ui::DropdownView {
                row: d.row,
                items: d.items.iter().map(|x| x.0.as_str()).collect(),
                sel: d.sel,
                top: d.top,
                current: d.current,
            });
            let chooser_list = self.menus.admin_list.as_ref().unwrap_or(&self.menus.vehicle_list);
            let (chooser_items, chooser_sel): (Vec<(&str, &str)>, Option<usize>) = match self.menus.chooser {
                Some(sel) => {
                    let items = chooser_list.iter().map(|(name, path)| (path.as_str(), name.as_str())).collect();
                    (items, Some(sel))
                }
                None => (Vec::new(), None),
            };
            // (the game menu's greyed-out lines: the timetable needs an active route)
            let menu_disabled: &[&str] = &[];
            let (menu_kind, menu_head, menu_preview) = crate::game_lists::menu_extras(self.menus.list_kind.as_ref(), self.menus.admin_list.as_deref(), chooser_sel, self.session.schedule.as_ref().or(self.session.timetable_book.as_ref()), self.clock.time);
            crate::telemetry::publish(self.player.as_ref(), self.session.duty.as_ref(), self.session.humans.as_ref(), self.paused, self.session.schedule.as_ref(), self.session.traffic.as_ref());
            let mz_view = (self.settings.milezero && self.menus.game_menu.is_none()).then(|| {
                let m = &self.sound.milezero;
                let song = m.song().unwrap_or_default();
                let (cx, cy) = self.input.cursor;
                let over = ui.mz.rect.is_some_and(|q| cx >= q[0] && cx < q[2] && cy >= q[1] && cy < q[3]);
                ui::MzView {
                    title: song.title,
                    artist: song.artist,
                    dj: song.dj,
                    duration: song.duration,
                    light: self.menus.navigator.as_ref().map_or(self.settings.nav_light, |n| n.light),
                    art: song.cover,
                    elapsed: m.elapsed(),
                    playing: m.is_playing(),
                    loading: m.loading(),
                    quality: m.quality(),
                    typing: m.typing.clone(),
                    status: m.request_status(),
                    hover: over || m.typing.is_some(),
                }
            });
            let mz_gap = if mz_view.is_some() { ui.mz.extent } else { 0.0 };
            let frame = ui::Frame {
                scale,
                ui_scale: ui::size_factor(h, scale, self.settings.ui_scale, self.settings.ui_scale_window),
                opacity: ui::backdrop(self.settings.ui_opacity),
                light: self.settings.nav_light,
                sizes: self.settings.hud_sizes,
                width: w,
                height: h,
                cursor: (self.input.cursor.0 - hud[0], self.input.cursor.1),
                steer_cross: steer_cross.map(|(x, y)| (x - hud[0], y)),
                vr: {
                    #[cfg(windows)] { self.xr.vr.is_some() }
                    #[cfg(not(windows))] { false }
                },
                // (with the fingers: only while one holds a switch - the cursor stays where the
                // last finger was, and a switch there kept its name up in a corner)
                tooltip: tooltip.filter(|_| self.settings.tooltips && !self.input.dragging && !covered && self.menus.game_menu.is_none() && (!self.input.touch.enabled || self.input.touch.on_switch())),
                tooltip_above: self.input.touch.enabled,
                // (the editors' keys; not over the city map, whose header they covered
                // once they stood on the timetable's line)
                notes: if !map_open && self.menus.game_menu.is_none() { notes } else { &[] },
                fps: self.settings.show_fps.then_some(self.perf.fps),
                perf: self.settings.perf_overlay_on().then_some(&self.perf.overlay),
                perf_detailed: self.settings.perf_overlay == "detailed",
                perf_touch: self.input.touch.enabled.then(|| self.input.touch.top_gap.map(|g| g[2])),
                paused: self.paused,
                menu: match chooser_sel {
                    _ if shell_menu => None,
                    Some(k) => Some((k, &chooser_items[..])),
                    None => self.menus.game_menu.map(|k| (k, &menu_lines[..])),
                },
                menu_disabled,
                menu_kind,
                menu_head,
                menu_preview,
                pane_first: self.menus.pane_scroll.filter(|p| Some(p.0) == chooser_sel).map(|p| p.1),
                menu_tabs,
                dropdown,
                menu_kbd: self.menus.menu_kbd,
                menu_top: self.menus.menu_top,
                // (a seat or view slider held: the view behind is seen changing, #2359)
                menu_peek: self.menus.menu_drag.filter(|k| chooser_items.get(*k).is_some_and(|(id, _)| id.starts_with("seat") || *id == "fov")),
                // (not over the city map, which has the stops and their times: it
                // covered the map's zoom and close buttons)
                timetable: (self.menus.timetable && !map_open).then(|| timetable_rows(self.session.duty.as_ref(), self.player.as_ref().map(|p| p.vehicle.host.tt_delay as f64))).flatten(),
                info: self.menus.info_bar.then(|| info_line(&self.clock, self.player.as_ref(), self.session.duty.as_ref(), self.session.humans.as_ref().map(|h| h.riding()), self.session.career.metres, self.menus.navigator.as_ref().filter(|n| n.enabled).and_then(|n| n.speed_limit))),
                info_room: self.input.touch.info_room.filter(|_| self.input.touch.enabled),
                tutorial: self.menus.tutorial.as_ref().filter(|t| !t.hidden && self.menus.game_menu.is_none()).and_then(|t| t.page().map(|p| (p.title.as_str(), p.text.as_str(), p.image.as_deref(), t.at, t.pages.len()))),
                chat,
                chat_size: self.settings.chat_size,
                tags,
                notices: &self.menus.notices,
                notice_anchor: self.menus.navigator.as_ref().and_then(|n| n.screen_rect()).map(|mut a| {
                    // (the MileZero card stands between the navigator and the notices)
                    if a[1] > h * 0.5 { a[1] -= mz_gap } else { a[3] += mz_gap }
                    a
                }),
                milezero: mz_view.as_ref(),
                nav_rect: self.menus.navigator.as_ref().and_then(|n| n.screen_rect()),
            };
            ui.draw_at(r, scene, &frame, dt, hud[0]);
        }
    }
}

/// How long (seconds of play) a `$msg` of the player's bus scripts stays a note after the
/// script last said it.
const SCRIPT_MESSAGE_SECS: f64 = 4.0;

/// The kind of card a message of the game's is (`App::service_msg`): what went wrong is an
/// alert, a key to press is to do, a slow driver a warning, a change made is done, the
/// rest information.
fn message_kind(msg: &str) -> crate::ui::NoticeKind {
    use crate::ui::NoticeKind;
    let m = msg.to_ascii_lowercase();
    if ["could not", "was not", "were not", "not saved", "no duty", "failed", "error", "cannot"].iter().any(|w| m.contains(w)) {
        NoticeKind::Alert
    } else if m.contains("slow") || m.contains("only the host") || m.contains("the host sets") {
        NoticeKind::Warn
    } else if m.starts_with("press ") || m.contains(". press ") || m.starts_with("on foot") {
        NoticeKind::Action
    } else if m.contains("saved") || m.contains("applied") || m.contains("reloaded") {
        NoticeKind::Success
    } else {
        NoticeKind::Info
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_message_is_told_by_what_it_says() {
        use crate::ui::NoticeKind::*;
        assert_eq!(super::message_kind("Key binding was not saved: disk full"), Alert);
        assert_eq!(super::message_kind("No duty: no such tour"), Alert);
        assert_eq!(super::message_kind("Graphics driver is slow (40 MB/s): the game will stutter."), Warn);
        assert_eq!(super::message_kind("Press a key for this action (Delete clears it, Esc cancels)"), Action);
        assert_eq!(super::message_kind("Controller configuration saved and applied"), Success);
        assert_eq!(super::message_kind("Fleet number 2412"), Info);
    }
}
