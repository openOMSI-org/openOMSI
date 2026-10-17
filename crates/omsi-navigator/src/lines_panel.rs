//! The lines panel of the city map: the lines of the map's package (`lines.lin`) in a list with a tick box each, the
//! data of the line in focus above them, their ways drawn on the map in a colour of their own, and the vehicles that
//! run on a ticked line ringed in that colour.

use super::*;
use omsi_map::package::{GROUP_BRT, GROUP_EXPRESS, GROUP_FEEDER, MODAL_BUS};
use omsi_ui::Vertex;

/// The panel is a dark card in either look of the map: its text, and the dimmer text of its labels.
pub(super) const TEXT: Color = Color::rgba(237, 237, 237, 1.0);
pub(super) const TEXT_DIM: Color = Color::rgba(184, 184, 184, 1.0);

/// The texts of the lines panel, looked up in the catalog (`locales/app.yml`) by their English wording: a language without its own gets English.
pub(super) struct LinesText {
    pub lines: String,
    line_data: String,
    map_lines: String,
    all: String,
    brt: String,
    express: String,
    feeders: String,
    length: String,
    stops: String,
    trip: String,
    pub(super) route: String,
    vehicles: String,
    circular: String,
    out_back: String,
    minutes: String,
    count: String,
    /// Said in the card while no line is open: how to open one.
    hint: [String; 2],
    /// The lists by kind of vehicle: bus, tram, metro, rail.
    modals: [String; 4],
    tick_all: String,
    tick_none: String,
    pub(super) timetable: String,
    pub(super) towards: String,
    /// Monday to Friday, Saturday, Sunday.
    pub(super) days: [String; 3],
    pub(super) predicted: String,
    /// Decimals with a comma (the language's own way of writing a number).
    comma: bool,
}

/// The languages that write a decimal with a comma.
fn decimal_comma(lang: &str) -> bool {
    let base = lang.split(['-', '_']).next().unwrap_or("");
    matches!(base, "es" | "ca" | "sv" | "pt" | "fr" | "de" | "nl" | "ru" | "pl" | "uk" | "it" | "cs" | "tr" | "hu" | "be" | "kk" | "id" | "vi")
}

pub(super) fn lines_text() -> LinesText {
    let t = |s: &str| omsi_ui::tr(s).into_owned();
    LinesText {
        lines: t("Lines"),
        line_data: t("LINE DATA"),
        map_lines: t("MAP LINES"),
        all: t("All"),
        brt: t("BRT"),
        express: t("Express"),
        feeders: t("Feeders"),
        length: t("Length"),
        stops: t("Stops"),
        trip: t("Trip time"),
        route: t("Route"),
        vehicles: t("Vehicles now"),
        circular: t("Circular"),
        out_back: t("Out and back"),
        minutes: t("min"),
        count: t("lines"),
        hint: [t("Double-click a line"), t("for its routes")],
        modals: [t("Bus"), t("Tram"), t("Metro"), t("Rail")],
        tick_all: t("Select all"),
        tick_none: t("Clear"),
        timetable: t("Timetable"),
        towards: t("Towards"),
        days: [t("Mon–Fri"), t("Sat"), t("Sun")],
        predicted: t("Times predicted from the map's timetable"),
        comma: decimal_comma(&omsi_ui::i18n::language()),
    }
}

/// The colour a line is drawn in on the city map (a fixed palette that keeps clear of the map's own colours).
pub(super) fn line_color(i: usize) -> Color {
    const P: [u32; 10] = [0xe83f9b, 0x17b8c4, 0xff8a1f, 0xff4d4d, 0x8bd13f, 0xd46bff, 0x1fd1a0, 0xff6fb5, 0xf2c14e, 0x4da3ff];
    Color::hex(P[i % P.len()])
}

/// What a click on the lines panel of the city map does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum LinesHit {
    /// Tick or untick a line (its index in the package's lines).
    Toggle(usize),
    /// Tick a line, or open it with a second click.
    Focus(usize),
    /// The filter tab: 0 all, 1 BRT, 2 express, 3 feeders.
    Filter(u8),
    /// The kind of vehicle on show: 0 bus, 1 tram, 2 metro, 3 rail.
    Modal(u8),
    /// The route of the line in focus (0 = route 1).
    RoutePick(usize),
    /// The timetable in place of the list, and back.
    Sheet,
    /// A day group (a weekday: 0, 5, 6) and a departure (its time in s and its trip: two departures can leave at the same time).
    Day(usize),
    Dep((u32, u16)),
    /// Back from a departure's stops to the departures.
    Back,
    /// Tick every line of the tab on show, or clear them when all are ticked.
    All,
    /// A stop of the departure on show (its map object): the view goes to it.
    Stop(i64),
}

/// The lines panel of the city map: which lines are ticked and what is on show.
#[derive(Default)]
pub struct LinesView {
    pub(super) open: bool,
    /// Lines ticked, in the order they were ticked.
    pub(super) selected: Vec<usize>,
    /// The line the data card is about (opened by a double click).
    pub(super) focus: Option<usize>,
    /// The last line clicked in the list and when (the navigator's clock), to tell a double click.
    last_click: Option<(usize, f32)>,
    /// The route chosen for a line (index into its routes).
    pub(super) route: HashMap<usize, usize>,
    filter: u8,
    modal: u8,
    /// First row of the list on show.
    pub(super) scroll: f32,
    /// The timetable of the line in focus is on show in place of the list; the day group picked (else today's); the departure picked
    /// (its time in s and, once clicked, its trip); and the list is put at the next departure when it is drawn.
    pub(super) sheet: bool,
    pub(super) day: Option<usize>,
    pub(super) dep: Option<(u32, Option<u16>)>,
    pub(super) jump: bool,
    /// Bumped whenever the choice changes: the ways are drawn again.
    version: u64,
    /// The ways' mesh (buffer 18): the version it was built for, its size and anchor.
    mesh: Option<(u64, u32, DVec2)>,
    /// The panel and its list, and what a click lands on (map picture pixels).
    panel: Option<Rect>,
    list: Option<Rect>,
    hits: Vec<(Rect, LinesHit)>,
    /// The view is moved to the line just ticked at the next frame.
    fit: bool,
}

impl LinesView {
    /// Whether the panel is open and has this point (map picture pixels).
    pub(super) fn has_point(&self, p: Vec2) -> bool {
        self.open && self.panel.is_some_and(|r| r.contains(p))
    }

    /// Whether the panel's list is open and has this point.
    pub(super) fn list_has_point(&self, p: Vec2) -> bool {
        self.open && self.list.is_some_and(|r| r.contains(p))
    }

    /// What a press at `p` lands on in the panel.
    pub(super) fn hit(&self, p: Vec2) -> Option<LinesHit> {
        self.hits.iter().find(|(r, _)| r.contains(p)).map(|h| h.1)
    }

    pub(super) fn scroll_by(&mut self, rows: f32) {
        self.scroll += rows;
    }

    pub(super) fn toggle(&mut self) {
        self.open = !self.open;
    }
}

impl Navigator {
    /// The lines the list shows: those that hold on the session's date, of the kind of vehicle chosen and (buses) of
    /// the group chosen.
    pub(super) fn shown_lines(&self) -> Vec<usize> {
        let Some(sf) = self.surface.as_ref() else { return Vec::new() };
        let (modal, filter) = (self.city.lines.modal, self.city.lines.filter);
        (0..sf.lines().len()).filter(|&i| sf.line_on(i, self.date) && sf.lines()[i].modal == modal && (modal != MODAL_BUS || filter == 0 || sf.lines()[i].group == filter)).collect()
    }

    /// A click on the panel.
    pub(super) fn lines_click(&mut self, h: LinesHit) {
        let n = self.surface.as_ref().map_or(0, |s| s.lines().len());
        let shown = if matches!(h, LinesHit::All) { self.shown_lines() } else { Vec::new() };
        let st = &mut self.city.lines;
        // (a click on a line leaves the list where it is: the player is scrolled down it)
        if matches!(h, LinesHit::Toggle(_) | LinesHit::Focus(_)) {
            (st.dep, st.jump) = (None, true);
        }
        match h {
            LinesHit::Sheet => (st.sheet, st.dep, st.jump, st.scroll) = (!st.sheet, None, true, 0.0),
            LinesHit::Day(d) => (st.day, st.dep, st.jump, st.scroll) = (Some(d), None, true, 0.0),
            LinesHit::Dep((t, trip)) => (st.dep, st.scroll) = (Some((t, Some(trip))), 0.0),
            LinesHit::Back => (st.dep, st.jump, st.scroll) = (None, true, 0.0),
            LinesHit::Stop(id) => {
                if let Some(p) = self.stop_pos.get(&id) {
                    self.city.center = p.truncate();
                    self.city.follow = false;
                    self.city.mpp = self.city.mpp.min(3.0);
                }
            }
            LinesHit::All => {
                if shown.iter().all(|i| st.selected.contains(i)) {
                    st.selected.retain(|i| !shown.contains(i));
                } else {
                    let new: Vec<usize> = shown.iter().copied().filter(|i| !st.selected.contains(i)).collect();
                    st.selected.extend(&new);
                }
                st.focus = st.focus.filter(|f| st.selected.contains(f));
                st.version += 1;
            }
            LinesHit::Modal(k) => (st.modal, st.filter, st.scroll) = (k, 0, 0.0),
            LinesHit::Filter(k) => {
                st.filter = k;
                st.scroll = 0.0;
            }
            LinesHit::Toggle(i) if i < n => {
                if let Some(p) = st.selected.iter().position(|&x| x == i) {
                    st.selected.remove(p);
                    if st.focus == Some(i) {
                        st.focus = None;
                    }
                } else {
                    st.selected.push(i);
                    st.fit = true;
                }
                st.version += 1;
            }
            // a click on the name ticks the line (both its routes are drawn); a double click opens it
            LinesHit::Focus(i) if i < n => {
                let double = st.last_click.is_some_and(|(j, t)| j == i && self.time - t < 0.45);
                st.last_click = Some((i, self.time));
                if double {
                    if !st.selected.contains(&i) {
                        st.selected.push(i);
                    }
                    st.focus = Some(i);
                } else if let Some(p) = st.selected.iter().position(|&x| x == i) {
                    st.selected.remove(p);
                    if st.focus == Some(i) {
                        st.focus = None;
                    }
                } else {
                    st.selected.push(i);
                }
                st.fit = true;
                st.version += 1;
            }
            LinesHit::RoutePick(r) => {
                (st.dep, st.jump, st.scroll) = (None, true, 0.0);
                if let Some(i) = st.focus {
                    st.route.insert(i, r);
                    st.version += 1;
                    st.fit = true;
                }
            }
            _ => {}
        }
    }

    /// The view goes to a line just ticked, and the ticked ways (and their stops) are a mesh (buffer 18): its new
    /// vertices, or `None` when the buffer is as it was.
    pub(super) fn lines_mesh(&mut self, anchor: DVec2, w: f32, h: f32, s: f32) -> Option<Vec<Vertex>> {
        let sf = self.surface.clone().filter(|_| self.city.lines.open)?;
        let route_of = |st: &LinesView, i: usize| sf.lines().get(i).and_then(|l| l.routes.get(st.route.get(&i).copied().unwrap_or(0).min(l.routes.len().saturating_sub(1))));
        if std::mem::take(&mut self.city.lines.fit) {
            if let Some(r) = self.city.lines.focus.or(self.city.lines.selected.last().copied()).and_then(|i| route_of(&self.city.lines, i)) {
                let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
                for q in &r.pts {
                    lo = lo.min(DVec2::new(q.0, q.1));
                    hi = hi.max(DVec2::new(q.0, q.1));
                }
                if lo.x <= hi.x {
                    let (free_w, free_h) = (((w - 332.0 * s) - 60.0 * s).max(300.0) as f64, (h - 44.0 * s - 60.0 * s).max(200.0) as f64);
                    let mpp = (((hi.x - lo.x) / free_w).max((hi.y - lo.y) / free_h) * 1.12).clamp(0.25, self.max_mpp());
                    self.city.mpp = mpp;
                    // (the panel takes the right side: the way is centred in what is left)
                    self.city.center = (lo + hi) * 0.5 + DVec2::new(166.0 * s as f64 * mpp, 0.0);
                    self.city.follow = false;
                }
            }
        }
        let key = (self.city.lines.version << 8) ^ self.surface_version;
        if self.city.lines.mesh.is_some_and(|m| m.0 == key && m.2 == anchor) {
            return None;
        }
        let rel = |q: (f64, f64)| Vec3::new((q.0 - anchor.x) as f32, (q.1 - anchor.y) as f32, 0.0);
        let mut p = Painter::new();
        for &i in &self.city.lines.selected {
            let Some(l) = sf.lines().get(i) else { continue };
            let c = line_color(i);
            for r in &l.routes {
                let v: Vec<Vec3> = r.pts.iter().map(|q| rel(*q)).collect();
                p.ribbon(&v, 7.0, 3.4, c.alpha(0.62), false);
                for q in &r.stops {
                    p.world_disc(rel(*q), 4.5, 3.6, Color::rgba(8, 8, 8, 0.85));
                    p.world_disc(rel(*q), 3.0, 2.4, Color::WHITE.alpha(0.95));
                }
            }
        }
        self.city.lines.mesh = Some((key, p.len(), anchor));
        Some(p.verts)
    }

    /// Offscreen pictures (`OMSI_NAV_LINES=number[;route;HH:MM]`): the panel open on a line, its timetable and a departure picked.
    pub fn debug_lines(&mut self, spec: &str) {
        let mut it = spec.split(';');
        let n = it.next().unwrap_or("");
        let Some(i) = self.surface.as_ref().and_then(|s| s.lines().iter().position(|l| l.number == n)) else { return };
        let st = &mut self.city.lines;
        (st.open, st.selected, st.focus, st.fit, st.version) = (true, vec![i], Some(i), true, st.version + 1);
        if let Some(d) = it.next() {
            (st.sheet, st.jump) = (true, true);
            st.route.insert(i, d.parse().unwrap_or(0));
            if let Some((h, m)) = it.next().and_then(|t| t.split_once(':')) {
                // (a time alone: the first departure that leaves then)
                st.dep = Some((h.parse::<u32>().unwrap_or(0) * 3600 + m.parse::<u32>().unwrap_or(0) * 60, None));
            }
        }
    }

    /// How many vertices of the ways' mesh to draw.
    pub(super) fn lines_mesh_len(&self) -> u32 {
        if self.city.lines.open {
            self.city.lines.mesh.map_or(0, |m| m.1)
        } else {
            0
        }
    }

    /// The vehicles of the public transport that run on a ticked line, ringed in that line's colour (`transit`: position,
    /// heading and line of each); returns how many each ticked line has on the road now.
    pub(super) fn ring_ticked_lines(&self, transit: &[(DVec3, f64, Option<String>)], anchor: DVec2, dots: &mut Painter) -> HashMap<usize, usize> {
        let rel = |q: DVec3| Vec3::new((q.x - anchor.x) as f32, (q.y - anchor.y) as f32, 0.0);
        let mut on_line: HashMap<usize, usize> = HashMap::new();
        let mut ticked: HashMap<String, (usize, Color)> = HashMap::new();
        if let (true, Some(sf)) = (self.city.lines.open, self.surface.as_ref()) {
            for &i in &self.city.lines.selected {
                if let Some(l) = sf.lines().get(i) {
                    ticked.insert(l.number.clone(), (i, line_color(i)));
                }
            }
        }
        for (at, _heading, line) in transit {
            if let Some(&(idx, c)) = line.as_deref().and_then(|l| ticked.get(l.trim())) {
                *on_line.entry(idx).or_insert(0) += 1;
                dots.world_disc(rel(*at), 9.0, 11.0, c.alpha(0.30));
            }
        }
        on_line
    }

    /// The Lines button left of the close button and its panel left of the column of buttons (`size`: the window's width
    /// and height and the interface scale; `at`: the pad, the button size, the column's x and the close button).
    pub(super) fn draw_lines_ui(&mut self, ui: &mut Painter, size: (f32, f32, f32), at: (f32, f32, f32, Rect), on_line: &HashMap<usize, usize>, on_screen: &dyn Fn(DVec3) -> Vec2) {
        let ((w, h, s), (pad, bs, bx, close)) = (size, at);
        // (a map without lines: no texts are looked up, nothing is drawn)
        if !self.surface.as_ref().is_some_and(|sf| !sf.lines().is_empty()) {
            return;
        }
        let lt = lines_text();
        self.draw_lines_button(ui, pad, bs, close.x - 8.0 * s, s, &lt);
        if self.city.lines.open && self.surface.as_ref().is_some_and(|sf| !sf.lines().is_empty()) {
            // (a departure picked: the time at each stop, on the map)
            if let (true, Some(sf), Some(i)) = (self.city.lines.sheet, self.surface.clone(), self.city.lines.focus) {
                self.sheet_labels(ui, &sf, i, s, on_screen, (w, h));
            }
            self.lines_panel(ui, w, h, s, close.bottom() + 12.0 * s, bx - 12.0 * s, &lt, on_line);
        }
    }

    /// The Lines button, only where the package has lines: `bs` high at `y`, its right edge at `right`; returns the new `right`.
    pub(super) fn draw_lines_button(&mut self, ui: &mut Painter, y: f32, bs: f32, right: f32, s: f32, lt: &LinesText) -> f32 {
        if !self.surface.as_ref().is_some_and(|sf| !sf.lines().is_empty()) {
            return right;
        }
        let bw = (self.fonts.width(lt.lines.as_str(), 13.5 * s, Weight::Bold) + 40.0 * s).max(92.0 * s);
        let r = Rect::new(right - bw, y, bw, bs);
        ui.rounded(r, 6.0 * s, if self.city.lines.open { Color::rgba(60, 98, 156, 1.0) } else { Color::rgba(37, 37, 37, 1.0) });
        ui.icon(&mut self.atlas, "route", Vec2::new(r.x + 17.0 * s, r.center().y), 17.0 * s, TEXT);
        ui.text_in(&mut self.atlas, &self.fonts, lt.lines.as_str(), 13.5 * s, Weight::Bold, Rect::new(r.x + 30.0 * s, r.y, r.w - 32.0 * s, r.h), Align::Center, TEXT);
        self.city.buttons.push((r, 5));
        r.x - 8.0 * s
    }
}

impl Navigator {
    /// The lines panel on the right of the city map (after the package's `lines.lin`): the data of
    /// the line in focus above, the list of the map's lines below with a tick box each, filters
    /// and a scroll. `on_line` is how many vehicles each ticked line has on the road now.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lines_panel(&mut self, ui: &mut Painter, w: f32, h: f32, s: f32, top: f32, right: f32, lt: &LinesText, on_line: &HashMap<usize, usize>) {
        let Some(sf) = self.surface.clone() else { return };
        let lines = sf.lines();
        let pw = (332.0 * s).min(w * 0.42);
        let x0 = right - pw;
        let focus = self.city.lines.focus.filter(|&i| i < lines.len());
        let card_h = if focus.is_some() { 178.0 * s } else { 84.0 * s };
        let ink = Color::rgba(22, 26, 33, 1.0);
        let pad = 14.0 * s;
        let mut hits: Vec<(Rect, LinesHit)> = Vec::new();
        let num = |v: f32, dec: usize| {
            let t = format!("{v:.dec$}");
            if lt.comma {
                t.replace('.', ",")
            } else {
                t
            }
        };

        // --- the data card
        let card = Rect::new(x0, top, pw, card_h);
        ui.rounded(card, 8.0 * s, ink);
        ui.text(&mut self.atlas, &self.fonts, lt.line_data.as_str(), 10.5 * s, Weight::Bold, Vec2::new(x0 + pad + 4.0 * s, top + 20.0 * s), Align::Left, Color::hex(0x6f9bd6));
        if focus.is_none() {
            ui.icon(&mut self.atlas, "touch_app", Vec2::new(x0 + pad + 14.0 * s, top + 54.0 * s), 26.0 * s, Color::hex(0x6f9bd6));
            for (k, t) in lt.hint.iter().enumerate() {
                ui.text(&mut self.atlas, &self.fonts, &self.fonts.fit(t, 13.0 * s, if k == 0 { Weight::Bold } else { Weight::Medium }, pw - 2.0 * pad - 40.0 * s), 13.0 * s, if k == 0 { Weight::Bold } else { Weight::Medium }, Vec2::new(x0 + pad + 36.0 * s, top + (50.0 + 18.0 * k as f32) * s), Align::Left, if k == 0 { TEXT } else { TEXT_DIM });
            }
        }
        if let Some(i) = focus {
            let l = &lines[i];
            // (the button is there while the timetable is being read; if the line has none after all it goes)
            // (nothing is read for it: until the timetable has been read, by pressing the button, every line has one)
            if self.line_times_read().is_none_or(|t| t.line(&l.number).is_some()) {
                let tw = self.fonts.width(lt.timetable.as_str(), 11.5 * s, Weight::Bold) + 40.0 * s;
                let rr = Rect::new(x0 + pw - pad - tw, top + 8.0 * s, tw, 22.0 * s);
                ui.rounded(rr, 4.0 * s, if self.city.lines.sheet { Color::rgba(60, 98, 156, 1.0) } else { Color::rgba(40, 46, 56, 1.0) });
                ui.icon(&mut self.atlas, "schedule", Vec2::new(rr.x + 15.0 * s, rr.center().y), 14.0 * s, TEXT);
                ui.text_in(&mut self.atlas, &self.fonts, lt.timetable.as_str(), 11.5 * s, Weight::Bold, Rect::new(rr.x + 24.0 * s, rr.y, rr.w - 26.0 * s, rr.h), Align::Center, TEXT);
                hits.push((rr, LinesHit::Sheet));
            }
            let ri = self.city.lines.route.get(&i).copied().unwrap_or(0).min(l.routes.len().saturating_sub(1));
            let r = &l.routes[ri];
            ui.rect(Rect::new(x0, top + 6.0 * s, 4.0 * s, card_h - 12.0 * s), line_color(i));
            let nw = ui.text(&mut self.atlas, &self.fonts, &l.number, 18.0 * s, Weight::Bold, Vec2::new(x0 + pad + 4.0 * s, top + 48.0 * s), Align::Left, TEXT);
            let name = self.fonts.fit(&l.name, 16.0 * s, Weight::Bold, pw - 2.0 * pad - nw - 18.0 * s);
            ui.text(&mut self.atlas, &self.fonts, &name, 16.0 * s, Weight::Bold, Vec2::new(x0 + pad + 4.0 * s + nw + 10.0 * s, top + 48.0 * s), Align::Left, TEXT);
            let itin = self.fonts.fit(&r.itinerary, 11.5 * s, Weight::Medium, pw - 2.0 * pad - 4.0 * s);
            ui.text(&mut self.atlas, &self.fonts, &itin, 11.5 * s, Weight::Medium, Vec2::new(x0 + pad + 4.0 * s, top + 68.0 * s), Align::Left, TEXT_DIM);
            // length, stops, trip time
            let colw = (pw - 2.0 * pad) / 3.0;
            let stats = [(lt.length.as_str(), format!("{} km", num(r.km, 1))), (lt.stops.as_str(), format!("{}", r.stops_n)), (lt.trip.as_str(), format!("{} {}", r.minutes, lt.minutes.as_str()))];
            for (k, (label, value)) in stats.iter().enumerate() {
                let sx = x0 + pad + 4.0 * s + colw * k as f32;
                ui.text(&mut self.atlas, &self.fonts, &self.fonts.fit(label, 10.0 * s, Weight::Medium, colw - 6.0 * s), 10.0 * s, Weight::Medium, Vec2::new(sx, top + 92.0 * s), Align::Left, TEXT_DIM);
                ui.text(&mut self.atlas, &self.fonts, value, 17.0 * s, Weight::Bold, Vec2::new(sx, top + 114.0 * s), Align::Left, TEXT);
            }
            let meta = [r.days.as_str(), r.model.as_str(), if r.circular { lt.circular.as_str() } else { lt.out_back.as_str() }].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join("  |  ");
            let meta = self.fonts.fit(&meta, 11.0 * s, Weight::Medium, pw - 2.0 * pad - 4.0 * s);
            ui.text(&mut self.atlas, &self.fonts, &meta, 11.0 * s, Weight::Medium, Vec2::new(x0 + pad + 4.0 * s, top + 138.0 * s), Align::Left, TEXT_DIM);
            let n = on_line.get(&i).copied().unwrap_or(0);
            ui.text(&mut self.atlas, &self.fonts, &format!("{}: {}", lt.vehicles.as_str(), n), 12.0 * s, Weight::Bold, Vec2::new(x0 + pad + 4.0 * s, top + 162.0 * s), Align::Left, if n > 0 { Color::hex(0x7bd88f) } else { TEXT_DIM });
            if l.routes.len() > 1 {
                let mut tx = x0 + pw - pad;
                for k in (0..l.routes.len()).rev() {
                    let label = format!("{} {}", lt.route.as_str(), k + 1);
                    let tw = self.fonts.width(&label, 11.5 * s, Weight::Bold) + 16.0 * s;
                    let rr = Rect::new(tx - tw, top + 148.0 * s, tw, 22.0 * s);
                    ui.rounded(rr, 4.0 * s, if k == ri { Color::rgba(60, 98, 156, 1.0) } else { Color::rgba(40, 46, 56, 1.0) });
                    ui.text_in(&mut self.atlas, &self.fonts, &label, 11.5 * s, Weight::Bold, rr, Align::Center, TEXT);
                    hits.push((rr, LinesHit::RoutePick(k)));
                    tx -= tw + 6.0 * s;
                }
            }
        }

        // --- the list of the map's lines
        let ly = top + card_h + 10.0 * s;
        let panel_h = (h - ly - 12.0 * s).max(60.0 * s);
        let list_panel = Rect::new(x0, ly, pw, panel_h);
        ui.rounded(list_panel, 8.0 * s, ink);
        if let Some(i) = focus.filter(|&i| self.city.lines.sheet && self.line_times().is_none_or(|t| t.line(&lines[i].number).is_some())) {
            let rows = self.sheet_panel(ui, &sf, i, list_panel, s, lt, &mut hits);
            self.city.lines.panel = Some(Rect::new(x0, top, pw, ly + panel_h - top));
            self.city.lines.list = Some(rows);
            self.city.lines.hits = hits;
            return;
        }
        ui.text(&mut self.atlas, &self.fonts, lt.map_lines.as_str(), 10.5 * s, Weight::Bold, Vec2::new(x0 + pad, ly + 20.0 * s), Align::Left, Color::hex(0x6f9bd6));
        let dated: Vec<usize> = (0..lines.len()).filter(|&i| sf.line_on(i, self.date)).collect();
        let mut present = [false; 4];
        for &i in &dated {
            present[(lines[i].modal as usize).min(3)] = true;
        }
        if !present[self.city.lines.modal.min(3) as usize] {
            self.city.lines.modal = present.iter().position(|&p| p).unwrap_or(0) as u8;
        }
        let shown = self.shown_lines();
        ui.text(&mut self.atlas, &self.fonts, &format!("{} {}", shown.len(), lt.count.as_str()), 10.5 * s, Weight::Medium, Vec2::new(x0 + pad + self.fonts.width(lt.map_lines.as_str(), 10.5 * s, Weight::Bold) + 10.0 * s, ly + 20.0 * s), Align::Left, TEXT_DIM);
        let all_on = !shown.is_empty() && shown.iter().all(|i| self.city.lines.selected.contains(i));
        let label = if all_on { lt.tick_none.as_str() } else { lt.tick_all.as_str() };
        let bw = self.fonts.width(label, 11.5 * s, Weight::Bold) + 20.0 * s;
        let br = Rect::new(x0 + pw - pad - bw, ly + 6.0 * s, bw, 22.0 * s);
        ui.rounded(br, 4.0 * s, if all_on { Color::rgba(60, 98, 156, 1.0) } else { Color::rgba(40, 46, 56, 1.0) });
        ui.text_in(&mut self.atlas, &self.fonts, label, 11.5 * s, Weight::Bold, br, Align::Center, TEXT);
        hits.push((br, LinesHit::All));
        let mut ty = ly + 30.0 * s;
        // a tab for each kind of vehicle the map has lines of
        if present.iter().filter(|&&p| p).count() > 1 {
            let mut tx = x0 + pad;
            for k in (0..4u8).filter(|&k| present[k as usize]) {
                let label = lt.modals[k as usize].as_str();
                let tw = self.fonts.width(label, 12.0 * s, Weight::Bold) + 20.0 * s;
                let rr = Rect::new(tx, ty, tw, 24.0 * s);
                ui.rounded(rr, 4.0 * s, if self.city.lines.modal == k { Color::rgba(60, 98, 156, 1.0) } else { Color::rgba(40, 46, 56, 1.0) });
                ui.text_in(&mut self.atlas, &self.fonts, label, 12.0 * s, Weight::Bold, rr, Align::Center, TEXT);
                hits.push((rr, LinesHit::Modal(k)));
                tx += tw + 6.0 * s;
            }
            ty += 30.0 * s;
        }
        // the groups of the buses
        if self.city.lines.modal == MODAL_BUS {
            let mut tx = x0 + pad;
            for (k, label) in [(0u8, lt.all.as_str()), (GROUP_BRT, lt.brt.as_str()), (GROUP_EXPRESS, lt.express.as_str()), (GROUP_FEEDER, lt.feeders.as_str())] {
                let tw = self.fonts.width(label, 11.5 * s, Weight::Bold) + 16.0 * s;
                let rr = Rect::new(tx, ty, tw, 22.0 * s);
                ui.rounded(rr, 4.0 * s, if self.city.lines.filter == k { Color::rgba(60, 98, 156, 1.0) } else { Color::rgba(40, 46, 56, 1.0) });
                ui.text_in(&mut self.atlas, &self.fonts, label, 11.5 * s, Weight::Bold, rr, Align::Center, TEXT);
                hits.push((rr, LinesHit::Filter(k)));
                tx += tw + 6.0 * s;
            }
            ty += 28.0 * s;
        }
        let row_h = 27.0 * s;
        let rows = Rect::new(x0, ty + 2.0 * s, pw, (list_panel.bottom() - ty - 8.0 * s).max(row_h));
        let vis = ((rows.h / row_h).floor() as usize).max(1);
        let max_scroll = shown.len().saturating_sub(vis) as f32;
        self.city.lines.scroll = self.city.lines.scroll.clamp(0.0, max_scroll);
        let first = self.city.lines.scroll.round() as usize;
        for (n, &i) in shown.iter().skip(first).take(vis).enumerate() {
            let l = &lines[i];
            let ry = rows.y + n as f32 * row_h;
            let row = Rect::new(x0 + 4.0 * s, ry, pw - 14.0 * s, row_h);
            let ticked = self.city.lines.selected.contains(&i);
            if self.city.lines.focus == Some(i) && ticked {
                ui.rounded(row, 4.0 * s, Color::WHITE.alpha(0.07));
            }
            let cb = Rect::new(x0 + pad, ry + (row_h - 14.0 * s) * 0.5, 14.0 * s, 14.0 * s);
            if ticked {
                ui.rounded(cb, 3.0 * s, line_color(i));
                ui.icon(&mut self.atlas, "check", cb.center(), 12.0 * s, Color::rgba(10, 10, 10, 1.0));
            } else {
                ui.rounded_border(cb, 3.0 * s, 1.2, Color::WHITE.alpha(0.35));
            }
            hits.push((Rect::new(x0 + 4.0 * s, ry, 34.0 * s, row_h), LinesHit::Toggle(i)));
            let nx = x0 + pad + 24.0 * s;
            ui.text_in(&mut self.atlas, &self.fonts, &l.number, 13.0 * s, Weight::Bold, Rect::new(nx, ry, 44.0 * s, row_h), Align::Left, TEXT);
            let badge = match l.group {
                GROUP_BRT => Some(("BRT", Color::hex(0x2e8b45))),
                GROUP_EXPRESS => Some(("EXP", Color::hex(0x6b7480))),
                _ => None,
            };
            let name_w = pw - 2.0 * pad - 24.0 * s - 44.0 * s - 44.0 * s;
            let name = self.fonts.fit(&l.name, 12.0 * s, Weight::Medium, name_w);
            ui.text_in(&mut self.atlas, &self.fonts, &name, 12.0 * s, Weight::Medium, Rect::new(nx + 44.0 * s, ry, name_w, row_h), Align::Left, TEXT);
            if let Some((label, c)) = badge {
                let bw = 32.0 * s;
                let br = Rect::new(x0 + pw - pad - 6.0 * s - bw, ry + (row_h - 15.0 * s) * 0.5, bw, 15.0 * s);
                ui.rounded(br, 3.0 * s, c);
                ui.text_in(&mut self.atlas, &self.fonts, label, 9.5 * s, Weight::Bold, br, Align::Center, Color::WHITE);
            }
            hits.push((Rect::new(nx, ry, pw - 14.0 * s - (nx - x0), row_h), LinesHit::Focus(i)));
        }
        // a thin scroll bar, when the list is longer than its room
        if max_scroll > 0.0 {
            let track = Rect::new(x0 + pw - 7.0 * s, rows.y, 3.0 * s, vis as f32 * row_h);
            ui.rounded(track, 1.5 * s, Color::WHITE.alpha(0.08));
            let (th, tp) = (track.h * vis as f32 / shown.len() as f32, self.city.lines.scroll / max_scroll);
            ui.rounded(Rect::new(track.x, track.y + (track.h - th) * tp, track.w, th.max(14.0 * s)), 1.5 * s, Color::WHITE.alpha(0.35));
        }
        self.city.lines.panel = Some(Rect::new(x0, top, pw, ly + panel_h - top));
        self.city.lines.list = Some(rows);
        self.city.lines.hits = hits;
    }
}
