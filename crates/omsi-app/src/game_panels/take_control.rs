//! Taking control of an AI bus from the map camera, as OMSI 2 does (manual 3.3.1, 3.3.5).

use super::{button, card, grow_row, note, title};
use crate::humans::{BusId, placed_bus_id};
use omsi_plugin::ui::{Anchor, Panel};
use omsi_ui::theme::Palette;

pub(crate) fn panel(name: &str, pal: Palette) -> Panel {
    card(
        Anchor::Center,
        0.0,
        0.0,
        380.0,
        236,
        pal,
        vec![
            title("Take control of this bus?"),
            note(name, pal),
            grow_row(vec![
                button("yes", "Yes", Some("check")),
                button("no", "Cancel", Some("close")),
            ]),
        ],
    )
}

/// (the `[boundingbox]`, as the collisions take it)
pub(crate) fn bus_under_cursor(app: &crate::App) -> Option<u64> {
    let (o, d, _) = app.cursor_ray_now()?;
    let d = d.as_dvec3();
    let t = app.session.traffic.as_ref()?;
    let mut best: Option<(u64, f64)> = None;
    for c in t.cars.iter().filter(|c| !c.gone && c.is_bus()) {
        for b in omsi_sim::ai_traffic::model::vehicle_bodies(&c.vehicle) {
            if let Some(hit) = ray_box(&b, o, d).filter(|h| best.is_none_or(|x| *h < x.1)) {
                best = Some((c.id, hit));
            }
        }
    }
    best.map(|b| b.0)
}

fn ray_box(b: &omsi_sim::collision::Obb, o: glam::DVec3, d: glam::DVec3) -> Option<f64> {
    let (sh, ch) = (b.heading.sin(), b.heading.cos());
    // into the box's frame: x across (right), y along the heading
    let local = |v: glam::DVec2| glam::DVec2::new(v.x * ch - v.y * sh, v.x * sh + v.y * ch);
    let lo = local(o.truncate() - b.center);
    let ld = local(d.truncate());
    let (mut t0, mut t1) = (0.0f64, 2000.0f64);
    for (p, v, lo_, hi) in [
        (lo.x, ld.x, -b.half.x, b.half.x),
        (lo.y, ld.y, -b.half.y, b.half.y),
        (o.z, d.z, b.z0, b.z1),
    ] {
        if v.abs() < 1e-9 {
            if p < lo_ || p > hi {
                return None;
            }
            continue;
        }
        let (a, c) = ((lo_ - p) / v, (hi - p) / v);
        t0 = t0.max(a.min(c));
        t1 = t1.min(a.max(c));
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
}

/// Not in a LAN client, whose traffic is the host's, nor with a timetable running: OMSI 2
/// changes no bus then.
pub(crate) fn ask(app: &mut crate::App, id: u64) -> bool {
    if app
        .net
        .lan
        .as_ref()
        .is_some_and(|l| l.role == omsi_net::Role::Client)
    {
        return false;
    }
    if app.session.duty.is_some() {
        app.service_msg = Some((
            omsi_ui::tr("The bus cannot be changed while a timetable is active").into_owned(),
            4.0,
        ));
        return true;
    }
    app.menus.game_panels.take = Some(id);
    true
}

pub(crate) fn name_of(app: &crate::App, id: u64) -> Option<String> {
    let c = app
        .session
        .traffic
        .as_ref()?
        .cars
        .iter()
        .find(|c| c.id == id && !c.gone)?;
    let d = &c.vehicle.ty.def;
    let line = c.vehicle.host.tt_line.trim();
    let name = format!("{} {}", d.manufacturer.trim(), d.type_name.trim());
    Some(if line.is_empty() {
        name.trim().to_string()
    } else {
        format!("{} - {} {line}", name.trim(), omsi_ui::tr("Line"))
    })
}

/// (with its bus gone no panel shows the question, yet it would hold the mouse and keys)
pub(crate) fn forget_gone(app: &mut crate::App) {
    if app
        .menus
        .game_panels
        .take
        .is_some_and(|id| name_of(app, id).is_none())
    {
        app.menus.game_panels.take = None;
    }
}

pub(crate) fn click(app: &mut crate::App, el: &str) {
    let Some(id) = app.menus.game_panels.take.take() else {
        return;
    };
    if el == "yes" {
        if let Err(e) = take(app, id) {
            log::warn!("taking control of AI bus {id}: {e:#}");
            app.service_msg = Some((
                format!("{}: {e:#}", omsi_ui::tr("The bus could not be taken over")),
                5.0,
            ));
        }
    }
}

fn take(app: &mut crate::App, id: u64) -> anyhow::Result<()> {
    let w = app
        .world
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no world"))?;
    let c = app
        .session
        .traffic
        .as_ref()
        .and_then(|t| t.cars.iter().find(|c| c.id == id))
        .ok_or_else(|| anyhow::anyhow!("the bus has left"))?;
    let v = &c.vehicle;
    let bus = omsi_lan::game::content_relative(&v.ty.def.path, &app.args.root);
    let paint = v
        .host
        .paint_scheme
        .flatten()
        .and_then(|i| v.ty.paint_schemes.get(i))
        .map(|s| s.name.clone())
        .filter(|p| !p.is_empty());
    let hof = v
        .host
        .hof
        .as_ref()
        .map(|h| h.path.to_string_lossy().into_owned());
    let (at, heading, speed) = (v.position, v.heading, c.state.speed);
    let next_stop = usize::try_from(v.host.tt_busstop_index).ok();
    let mut args = crate::app_impl::placement_args(
        &app.args,
        bus.clone(),
        format!("{},{},{},{}", at.x, at.y, heading, at.z),
        paint,
        hof,
    );
    let p = &v.ty.program;
    args.situation_vars = p
        .var_names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            (
                n.clone(),
                v.state.vars.get(i).copied().unwrap_or(0.0) as f32,
            )
        })
        .collect();
    args.situation_strvars = p
        .str_var_names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            (
                n.clone(),
                v.state.str_vars.get(i).cloned().unwrap_or_default(),
            )
        })
        .collect();
    let tour = app
        .session
        .schedule
        .as_ref()
        .and_then(|s| s.sim.tour_of_car(id));
    // (in OMSI 2 it is the same vehicle, never without a driver)
    let r = app
        .renderer
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no renderer"))?;
    let mut prepared = crate::spawn::PreparedPlayer::load(&args)?;
    prepared.prefetch(w.placement_prefetch(r), args.paint.as_deref());
    let scene = app
        .scene
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("no scene"))?;
    let Some(mut q) = crate::spawn::spawn_player_prepared(&args, &w, r, scene, prepared)? else {
        anyhow::bail!("not placed")
    };
    q.vehicle.set_speed(speed);
    if let Some(h) = app.session.humans.as_mut() {
        h.remap_bus(BusId::Ai(id), BusId::Ai(placed_bus_id(q.uid)));
    }
    if let (Some(t), Some(r), Some(sc)) = (
        app.session.traffic.as_mut(),
        app.renderer.as_ref(),
        app.scene.as_mut(),
    ) {
        if t.remove_car(id) {
            crate::view_sync::apply_traffic(t, &mut app.gfx.sim_view.traffic, &w, r, sc);
        }
    }
    if let Some(s) = app.session.schedule.as_mut() {
        s.sim.forget_car(id);
    }
    log::info!(
        "took control of AI bus {id}: {bus} at ({:.1}, {:.1}), {:.0} km/h",
        at.x,
        at.y,
        speed * 3.6
    );
    app.session.placed.push(q);
    app.take_placed(app.session.placed.len() - 1);
    let Some((line, tour, trip)) = tour else {
        app.service_msg = Some((omsi_ui::tr("You have control of the bus").into_owned(), 4.0));
        return Ok(());
    };
    let now = app.clock.time;
    let pos = trip
        .or_else(|| {
            app.session
                .schedule
                .as_ref()
                .map(|s| s.sim.tour_trip_now(&line, &tour, now))
        })
        .unwrap_or(0);
    let stops = app
        .session
        .schedule
        .as_ref()
        .map(|s| s.sim.tour_trip_stops(&line, &tour, pos))
        .unwrap_or_default();
    let chosen = next_stop
        .and_then(|n| stops.iter().position(|s| s.1 == n))
        .or_else(|| stops.iter().position(|s| s.3 >= now - 60.0))
        .unwrap_or(0);
    crate::game_lists::start_duty_at(app, &line, &tour, pos, chosen);
    Ok(())
}

#[cfg(test)]
#[path = "take_control_tests.rs"]
mod tests;
