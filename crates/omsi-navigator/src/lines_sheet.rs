//! The timetable of the lines panel: a line's departures by direction, day and hour in place of the list of lines, the stops
//! of a departure with the time the bus is at each, and those times written on the city map.

use super::line_times::{clock, Dep, LineTimes, Sheet};
use super::*;

impl Navigator {
    /// The map's timetable for the session's date (the map's `TTData` with the eras in force laid over it). It is read on a
    /// worker the first time it is asked for (a big map's `TTData` takes a while, and the frame must not wait for it) and kept
    /// until the date changes; until it is there this is `None` (the sheet says it is loading).
    pub(super) fn line_times(&mut self) -> Option<std::sync::Arc<LineTimes>> {
        use std::sync::mpsc::TryRecvError;
        let sf = self.surface.clone()?;
        let date = self.date;
        if let Some((_, t)) = self.times.as_ref().filter(|(d, _)| *d == date) {
            return Some(t.clone());
        }
        if let Some((asked, rx)) = self.times_job.as_ref() {
            let asked = *asked;
            match rx.try_recv() {
                Ok(t) if asked == date => {
                    self.times_job = None;
                    self.times = Some((date, t.clone()));
                    return Some(t);
                }
                Err(TryRecvError::Empty) if asked == date => return None,
                // (the worker is gone: no timetable, rather than asking again every frame)
                Err(TryRecvError::Disconnected) if asked == date => {
                    self.times_job = None;
                    let t = std::sync::Arc::new(LineTimes::default());
                    self.times = Some((date, t.clone()));
                    return Some(t);
                }
                // (it was asked for another date: asked for this one below)
                _ => self.times_job = None,
            }
        }
        let map_dir = sf.map_dir().to_path_buf();
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name("navigator timetable".into()).spawn(move || {
            let chrono = omsi_map::active_chrono_dirs(&map_dir, date);
            let _ = tx.send(std::sync::Arc::new(LineTimes::load(&map_dir, &chrono, date)));
        });
        self.times_job = spawned.ok().map(|_| (date, rx));
        None
    }

    /// The timetable if it has been read already (for the session's date), without reading it: the Timetable button asks this, so
    /// that opening a line does not read the map's `TTData` (only pressing the button does).
    pub(super) fn line_times_read(&self) -> Option<std::sync::Arc<LineTimes>> {
        self.times.as_ref().filter(|(d, _)| *d == self.date).map(|(_, t)| t.clone())
    }

    /// What the sheet says while the timetable is being read.
    fn sheet_loading(&mut self, ui: &mut Painter, area: Rect, s: f32) -> Rect {
        let text = omsi_ui::tr("Loading the timetable...");
        ui.text(&mut self.atlas, &self.fonts, &text, 12.5 * s, Weight::Medium, Vec2::new(area.x + 14.0 * s, area.y + 36.0 * s), Align::Left, TEXT_DIM);
        area
    }

    /// The day group the timetable shows: the picked one, else today's (Monday to Friday, Saturday, Sunday).
    pub(super) fn sheet_day(&self) -> usize {
        self.city.lines.day.unwrap_or(if self.day_clock.1 <= 4 { 0 } else { self.day_clock.1 as usize })
    }

    /// The direction (in the sheet) of the route chosen for line `i`: route 1, route 2.
    pub(super) fn sheet_dir(&self, sheet: &Sheet, i: usize, sf: &omsi_map::package::Package) -> usize {
        let order = sheet.order(sf.lines()[i].routes.first().map_or("", |r| r.itinerary.as_str()));
        let r = self.city.lines.route.get(&i).copied().unwrap_or(0);
        order.get(r).or(order.last()).copied().unwrap_or(0)
    }

    /// The timetable of the line in focus in place of the list: its directions and day groups, the
    /// departures by the hour and - once one is picked - when its bus is at each stop (the same times are
    /// written on the map). Returns the scrolling area.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn sheet_panel(&mut self, ui: &mut Painter, sf: &omsi_map::package::Package, i: usize, area: Rect, s: f32, lt: &LinesText, hits: &mut Vec<(Rect, LinesHit)>) -> Rect {
        let Some(times) = self.line_times() else { return self.sheet_loading(ui, area, s) };
        // (read, and this line has no timetable after all: back to the list)
        let Some(sheet) = times.line(&sf.lines()[i].number) else {
            self.city.lines.sheet = false;
            return area;
        };
        let (x0, y0, pw, pad) = (area.x, area.y, area.w, 14.0 * s);
        let tab = |on: bool| if on { Color::rgba(60, 98, 156, 1.0) } else { Color::rgba(40, 46, 56, 1.0) };
        let (day, dir) = (self.sheet_day(), self.sheet_dir(sheet, i, sf));
        let deps = sheet.departures(dir, day);
        let pick = self.city.lines.dep.and_then(|p| deps.iter().copied().find(|d| d.is(p)));
        ui.text(&mut self.atlas, &self.fonts, &format!("{}  ·  {}", lt.timetable.as_str(), lt.towards.as_str()).to_uppercase(), 11.5 * s, Weight::Bold, Vec2::new(x0 + pad, y0 + 20.0 * s), Align::Left, Color::hex(0x6f9bd6));
        if pick.is_some() {
            let rr = Rect::new(x0 + pw - pad - 34.0 * s, y0 + 6.0 * s, 34.0 * s, 22.0 * s);
            ui.rounded(rr, 4.0 * s, tab(false));
            ui.icon(&mut self.atlas, "arrow_back", rr.center(), 16.0 * s, TEXT);
            hits.push((rr, LinesHit::Back));
        }
        // the route (the card's buttons choose it) and where it goes, then the day groups
        let r = self.city.lines.route.get(&i).copied().unwrap_or(0);
        let head = self.fonts.fit(&format!("{} {}  ·  {}", lt.route.as_str(), r + 1, sheet.dirs[dir].to), 14.5 * s, Weight::Bold, pw - 2.0 * pad);
        ui.text(&mut self.atlas, &self.fonts, &head, 14.5 * s, Weight::Bold, Vec2::new(x0 + pad, y0 + 48.0 * s), Align::Left, TEXT);
        let mut cx = x0 + pad;
        for (label, d) in lt.days.iter().zip([0usize, 5, 6]) {
            let cw = self.fonts.width(label, 13.0 * s, Weight::Bold) + 20.0 * s;
            let rr = Rect::new(cx, y0 + 60.0 * s, cw, 26.0 * s);
            ui.rounded(rr, 4.0 * s, tab(d == day));
            ui.text_in(&mut self.atlas, &self.fonts, label, 13.0 * s, Weight::Bold, rr, Align::Center, TEXT);
            hits.push((rr, LinesHit::Day(d)));
            cx += cw + 6.0 * s;
        }
        let rows = Rect::new(x0, y0 + 100.0 * s, pw, (area.bottom() - y0 - 106.0 * s - if pick.is_some() { 20.0 * s } else { 0.0 }).max(40.0 * s));
        let scroll_bar = |ui: &mut Painter, scroll: f32, max: f32, vis: usize, total: usize, row_h: f32| {
            if max > 0.0 {
                let track = Rect::new(x0 + pw - 7.0 * s, rows.y, 3.0 * s, vis as f32 * row_h);
                ui.rounded(track, 1.5 * s, Color::WHITE.alpha(0.08));
                let (th, tp) = (track.h * vis as f32 / total as f32, scroll / max);
                ui.rounded(Rect::new(track.x, track.y + (track.h - th) * tp, track.w, th.max(14.0 * s)), 1.5 * s, Color::WHITE.alpha(0.35));
            }
        };
        if let Some(d) = pick {
            // the stops the bus calls at, with the time it is there
            let stops: Vec<_> = sheet.stops(d).into_iter().filter(|x| x.stops).collect();
            let row_h = 31.0 * s;
            let vis = ((rows.h / row_h).floor() as usize).max(1);
            let max = stops.len().saturating_sub(vis) as f32;
            self.city.lines.scroll = self.city.lines.scroll.clamp(0.0, max);
            for (n, (k, st)) in stops.iter().enumerate().skip(self.city.lines.scroll.round() as usize).take(vis).enumerate() {
                let rr = Rect::new(x0 + 4.0 * s, rows.y + n as f32 * row_h, pw - 14.0 * s, row_h);
                let w = if k == 0 || k + 1 == stops.len() { Weight::Bold } else { Weight::Medium };
                ui.text_in(&mut self.atlas, &self.fonts, &clock(st.at), 14.5 * s, Weight::Bold, Rect::new(x0 + pad, rr.y, 58.0 * s, row_h), Align::Left, TEXT);
                let nw = pw - 2.0 * pad - 64.0 * s;
                ui.text_in(&mut self.atlas, &self.fonts, &self.fonts.fit(st.name, 14.0 * s, w, nw), 14.0 * s, w, Rect::new(x0 + pad + 62.0 * s, rr.y, nw, row_h), Align::Left, TEXT);
                hits.push((rr, LinesHit::Stop(st.id)));
            }
            scroll_bar(ui, self.city.lines.scroll, max, vis, stops.len(), row_h);
            ui.text(&mut self.atlas, &self.fonts, &self.fonts.fit(lt.predicted.as_str(), 11.0 * s, Weight::Medium, pw - 2.0 * pad), 11.0 * s, Weight::Medium, Vec2::new(x0 + pad, area.bottom() - 10.0 * s), Align::Left, TEXT_DIM);
        } else {
            // the departures by the hour; the next one after the game's clock is lit
            let per = (((pw - 2.0 * pad - 44.0 * s) / (43.0 * s)).floor() as usize).max(1);
            let mut lines: Vec<(bool, i64, Vec<&Dep>)> = Vec::new();
            for d in &deps {
                let h = (d.t / 3600.0) as i64;
                match lines.last_mut() {
                    Some((_, lh, v)) if *lh == h && v.len() < per => v.push(d),
                    _ => lines.push((lines.last().is_none_or(|l| l.1 != h), h, vec![d])),
                }
            }
            let next = deps.iter().find(|d| d.t >= self.day_clock.0 as f32).map(|d| d.t as u32);
            let row_h = 36.0 * s;
            let vis = ((rows.h / row_h).floor() as usize).max(1);
            let max = lines.len().saturating_sub(vis) as f32;
            if std::mem::take(&mut self.city.lines.jump) {
                self.city.lines.scroll = lines.iter().position(|l| l.2.iter().any(|d| Some(d.t as u32) == next)).unwrap_or(0) as f32;
            }
            self.city.lines.scroll = self.city.lines.scroll.clamp(0.0, max);
            if lines.is_empty() {
                ui.text_in(&mut self.atlas, &self.fonts, "—", 16.0 * s, Weight::Bold, rows, Align::Center, TEXT_DIM);
            }
            for (n, (first, h, v)) in lines.iter().skip(self.city.lines.scroll.round() as usize).take(vis).enumerate() {
                let ry = rows.y + n as f32 * row_h;
                if *first {
                    ui.text_in(&mut self.atlas, &self.fonts, &format!("{:02}", h % 24), 16.0 * s, Weight::Bold, Rect::new(x0 + pad, ry, 38.0 * s, row_h), Align::Left, TEXT);
                }
                for (k, d) in v.iter().enumerate() {
                    let rr = Rect::new(x0 + pad + 42.0 * s + k as f32 * 43.0 * s, ry + 3.0 * s, 39.0 * s, row_h - 6.0 * s);
                    ui.rounded(rr, 4.0 * s, tab(Some(d.t as u32) == next));
                    ui.text_in(&mut self.atlas, &self.fonts, &format!("{:02}", (d.t / 60.0).round() as i64 % 60), 14.5 * s, Weight::Bold, rr, Align::Center, TEXT);
                    hits.push((rr, LinesHit::Dep(d.key())));
                }
            }
            scroll_bar(ui, self.city.lines.scroll, max, vis, lines.len(), row_h);
        }
        rows
    }

    /// The departure picked, on the map: a dot and the time at each stop it calls at, as many as fit
    /// (the ends first); the stops are where the map's own stop objects stand.
    pub(super) fn sheet_labels(&mut self, ui: &mut Painter, sf: &omsi_map::package::Package, i: usize, s: f32, to_screen: &dyn Fn(DVec3) -> Vec2, view: (f32, f32)) {
        let Some(times) = self.line_times() else { return };
        let Some(sheet) = times.line(&sf.lines()[i].number) else { return };
        let (day, dir) = (self.sheet_day(), self.sheet_dir(sheet, i, sf));
        let Some(d) = self.city.lines.dep.and_then(|p| sheet.departures(dir, day).into_iter().find(|d| d.is(p))) else { return };
        let stops: Vec<_> = sheet.stops(d).into_iter().filter(|x| x.stops).collect();
        let n = stops.len();
        let mut placed: Vec<Rect> = Vec::new();
        for k in [0, n.saturating_sub(1)].into_iter().chain(1..n.saturating_sub(1)) {
            let Some(p) = stops.get(k).and_then(|st| self.stop_pos.get(&st.id)) else { continue };
            let c = to_screen(*p);
            if c.x < 0.0 || c.y < 0.0 || c.x > view.0 || c.y > view.1 {
                continue;
            }
            let t = super::line_times::clock(stops[k].at);
            let r = Rect::new(c.x + 8.0 * s, c.y - 26.0 * s, self.fonts.width(&t, 13.5 * s, Weight::Bold) + 14.0 * s, 22.0 * s);
            if placed.iter().any(|q| r.x < q.right() + 3.0 * s && q.x < r.right() + 3.0 * s && r.y < q.bottom() + 3.0 * s && q.y < r.bottom() + 3.0 * s) {
                continue;
            }
            placed.push(r);
            ui.rounded(Rect::new(c.x - 5.0 * s, c.y - 5.0 * s, 10.0 * s, 10.0 * s), 5.0 * s, Color::rgba(8, 8, 8, 0.9));
            ui.rounded(Rect::new(c.x - 3.5 * s, c.y - 3.5 * s, 7.0 * s, 7.0 * s), 3.5 * s, Color::WHITE);
            ui.rounded(r, 4.0 * s, Color::rgba(10, 12, 16, 0.92));
            ui.text_in(&mut self.atlas, &self.fonts, &t, 13.5 * s, Weight::Bold, r, Align::Center, TEXT);
        }
    }
}
