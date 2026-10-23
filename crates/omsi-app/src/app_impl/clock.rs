//! The clock: its speed, the real-time sync, jumps, and the world following the date.

use super::*;

impl App {
    /// OMSI's weather dialog, the short way: the next weather of the Weather folder, in
    /// force at once (the roads keep their wetness until the rain changes it).
    /// How fast the clock runs: the session's in LAN play (the host's, which its time speed
    /// setting or its administration set), else the settings'.
    pub(crate) fn time_speed(&self) -> f64 {
        time_speed(&self.settings, &self.net)
    }

    /// The clock follows the real time and cannot be changed (the `time_sync` setting). In a
    /// LAN session as a client the host's clock counts: the host or the server syncs, not us.
    pub(crate) fn real_time_locked(&self) -> bool {
        real_time_locked(&self.settings, &self.net)
    }

    /// With the real-time sync on: hold the clock to this device's date and time (a second
    /// off at most; a bigger gap - the game was paused - is jumped, the traffic's clock with it).
    pub(crate) fn sync_real_time(&mut self) {
        if !self.real_time_locked() {
            return;
        }
        let Some(real) = crate::real_time::clock_now(&self.clock) else { return };
        let gap = crate::real_time::gap(&self.clock, &real);
        if gap.abs() < 0.25 {
            return;
        }
        self.clock.year = real.year;
        self.clock.day_of_year = real.day_of_year;
        self.clock.time = real.time;
        if let Some(tr) = self.session.traffic.as_mut() {
            tr.day_time += gap;
        }
        if let Some(p) = self.player.as_mut() {
            p.vehicle.host.clock = self.clock.clone();
        }
    }

    /// Move the clock by `secs` (the traffic's clock with it), as OMSI's time dialog does.
    pub(crate) fn shift_clock(&mut self, secs: f64) {
        shift_clock(ShiftClockCx::of(&mut self.parts()), secs);
    }

    /// The world follows the sim date as OMSI's does at the day's change:
    /// the chrono scenarios in force (the tiles they change are read again) and the
    /// season's textures - also when the weather turns to snow or thaws (every loaded tile
    /// is read again with the other texture folder).
    pub(crate) fn follow_date(&mut self) {
        let Some(w) = self.world.clone() else { return };
        let date = self.clock.date_code();
        let snow = self.session.weather.as_ref().is_some_and(|x| x.snow);
        let on_road = self.session.weather.as_ref().is_some_and(|x| x.snow_on_road);
        let season = crate::world_load::season_folder_on(&self.args, &w.global, self.clock.day_of_year, snow, on_road).1;
        let Some((was_date, was_season)) = self.session.world_day.clone() else {
            self.session.world_day = Some((date, omsi_texture::season_folder()));
            return;
        };
        if was_date == date && was_season == season {
            return;
        }
        self.session.world_day = Some((date, season.clone()));
        let changed = if was_date != date { w.set_date(date) } else { Vec::new() };
        let Some(st) = self.gfx.streamer.as_mut() else { return };
        if was_season != season {
            log::info!("season: the textures of {:?} now (were {:?})", season, was_season);
            omsi_texture::set_season_folder(season);
            omsi_cfg::content_changed();
            st.reload(None);
        } else if !changed.is_empty() {
            st.reload(Some(&changed));
        }
    }
}

/// The clock's speed: 1 under the real-time sync, the host's in a LAN session, else the
/// settings' (`App::time_speed`).
pub(crate) fn time_speed(settings: &settings::Settings, net: &NetState) -> f64 {
    if real_time_locked(settings, net) {
        return 1.0;
    }
    match net.lan.as_ref() {
        Some(l) => l.clock_speed,
        None => settings.time_speed.clamp(1.0, 30.0),
    }
}

/// The clock follows the real time and cannot be changed (`App::real_time_locked`).
pub(crate) fn real_time_locked(settings: &settings::Settings, net: &NetState) -> bool {
    settings.time_sync && !net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client)
}

crate::app::stage_ctx!(
    /// Moving the clock: its part of the game.
    ShiftClockCx { reads: [net, input, world], writes: [clock, traffic, player, ui, gfx] }
);

crate::app::stage_ctx!(
    /// The timetable put out again after a jump of the clock: its part of the game.
    JumpCx { reads: [world], writes: [clock, traffic, gfx] }
);

/// Move the clock by `secs` (the traffic's clock with it), as OMSI's time dialog does.
pub(crate) fn shift_clock(cx: ShiftClockCx, secs: f64) {
    if real_time_locked(cx.common.settings, cx.net) {
        *cx.ui.service_msg = Some(("The time cannot be changed while the real-time sync is on".into(), 3.0));
        return;
    }
    let mut t = cx.clock.clock.time + secs;
    while t < 0.0 {
        t += 86400.0;
        cx.clock.clock.prev_day();
    }
    while t >= 86400.0 {
        t -= 86400.0;
        cx.clock.clock.next_day();
    }
    cx.clock.clock.time = t;
    if let Some(tr) = cx.traffic.traffic.as_mut() {
        tr.day_time += secs;
    }
    if let Some(p) = cx.player.player.as_mut() {
        p.vehicle.host.clock = cx.clock.clock.clone();
    }
    let h = (t / 3600.0) as u32;
    *cx.ui.service_msg = Some(if secs.abs() >= 86400.0 {
        let (d, m) = cx.clock.clock.day_month();
        (omsi_ui::i18n::tr_fill("Date: {date}", &[("{date}", &format!("{:04}-{m:02}-{d:02}", cx.clock.clock.year))]), 3.0)
    } else {
        (omsi_ui::i18n::tr_fill("Clock: {time}", &[("{time}", &format!("{h:02}:{:02}", ((t / 60.0) as u32) % 60))]), 3.0)
    });
    *cx.clock.clock_jump += secs;
    // (held Page Up/Down: once they are let go)
    if cx.input.clock_hold == 0.0 {
        timetable_after_clock_jump(crate::app::narrow!(cx => JumpCx { reads: [world], writes: [clock, traffic, gfx] }));
    }
}

/// After the clock was set by more than two minutes: the timetable's buses put out again
/// for the new time (`Schedule::restart`).
pub(crate) fn timetable_after_clock_jump(cx: JumpCx) {
    let jump = std::mem::take(&mut *cx.clock.clock_jump);
    if jump.abs() < 120.0 {
        return;
    }
    if let (Some(s), Some(w), Some(t), Some(r), Some(scene)) = (cx.traffic.schedule.as_mut(), cx.world.world.as_ref(), cx.traffic.traffic.as_mut(), cx.gfx.renderer.as_ref(), cx.gfx.scene.as_mut()) {
        let day_time = t.day_time;
        s.restart(t, day_time);
        crate::view_sync::apply_traffic(t, &mut cx.gfx.gfx.sim_view.traffic, w, r, scene);
    }
}
