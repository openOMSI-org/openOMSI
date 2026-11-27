//! Console and administration on the dedicated server's simulation thread.
//! Uses the existing moderator rights, ban records and audit log.

use omsi_admin::metrics::TickMeter;
use omsi_admin::profiler::Profiler;
use omsi_lan::admin::{ServerAdmin, ServerRules};
use omsi_net::LanSession;

#[allow(clippy::too_many_arguments)]
pub fn service_commands(
    l: &mut LanSession,
    admin: &mut ServerAdmin,
    metrics: &TickMeter,
    profiler: &mut Profiler,
    positions: &dyn Fn(u32) -> Option<(glam::DVec3, f64)>,
    rules: &ServerRules<'_>,
    hub: Option<&omsi_admin::Hub>,
    floor: &dyn Fn(f64, f64, f64) -> Option<f64>,
    dt: f32,
) {
    admin.prune(l);
    omsi_lan::bans::sync_host(l);
    // the administration, whatever way it came (see `omsi_lan::admin::exec`): the
    // players' games, a tool on this machine (POST /admin, checked by the gateway: an
    // admin of its own), the web panel's requests
    for (from, text) in l.take_commands() {
        if !omsi_lan::parked::server_command(l, from, &text) {
            omsi_lan::admin::server_command(l, from, &text, admin, positions, rules, hub);
        }
    }
    omsi_lan::parked::server_tick(l, dt as f32);
    // Commands typed into this machine's terminal are executed on the
    // simulation thread, not on the stdin reader. They use the same
    // authority checks, command parser and audit trail as the local API.
    for text in crate::console::take() {
        if crate::console::run_builtin(&text, l, metrics, profiler) {
            continue;
        }
        if omsi_lan::parked::server_admin(&text, &|x, y, near| floor(x, y, near)) {
            continue;
        }
        if omsi_lan::admin::Action::parse(&text).is_none() {
            let verb = text.split_whitespace().next().unwrap_or("");
            log::warn!(
                "server console: unknown command {:?} (type help)",
                omsi_admin::clean(verb, 24)
            );
            continue;
        }
        admin
            .admins
            .insert(omsi_lan::admin::CONSOLE_ADMIN, omsi_admin::Role::Admin);
        omsi_lan::admin::server_command(
            l,
            omsi_lan::admin::CONSOLE_ADMIN,
            &format!("admin {text}"),
            admin,
            positions,
            rules,
            hub,
        );
    }
    for text in omsi_lan::gateway::take_local_admin() {
        if omsi_lan::parked::server_admin(&text, &|x, y, near| floor(x, y, near)) {
            continue;
        }
        admin
            .admins
            .insert(omsi_lan::admin::LOCAL_ADMIN, omsi_admin::Role::Admin);
        omsi_lan::admin::server_command(
            l,
            omsi_lan::admin::LOCAL_ADMIN,
            &format!("admin {text}"),
            admin,
            positions,
            rules,
            hub,
        );
    }
    if let Some(h) = hub {
        for request in h.take_profiles() {
            use omsi_admin::hub::ProfileAction;
            match request.action {
                ProfileAction::Start => profiler.start(),
                ProfileAction::Stop => profiler.stop(),
                ProfileAction::Reset => profiler.reset(),
            }
            h.note(
                &request.actor.label,
                Some(request.actor.role),
                "profiler",
                "applied",
                request.action.name(),
            );
        }
        for job in h.take(16) {
            omsi_lan::admin::run_job(l, admin, job, positions, rules, h);
        }
    }
}

/// Apply authoritative world changes only on the server tick, keeping the
/// existing role restrictions, audit records and traffic limits.
pub fn apply_world_jobs<'a>(
    lan: &'a mut LanSession,
    admin: &'a mut ServerAdmin,
    traffic: Option<&'a mut dyn omsi_lan::admin::world::ServerTraffic>,
    clock: f64,
    traffic_max: usize,
    hub: Option<&omsi_admin::Hub>,
) {
    let jobs = admin.take_world();
    if jobs.is_empty() {
        return;
    }
    let mut world = omsi_lan::admin::world::ServerWorld {
        lan,
        shift: &mut admin.shift,
        passenger_density: &mut admin.passenger_density,
        clock,
        traffic,
        traffic_max,
    };
    for job in jobs {
        let outcome = omsi_lan::admin::world::apply(&mut world, &job.action);
        omsi_lan::admin::report(world.lan, hub, &job.actor, job.record, &outcome);
    }
}

/// The native CPU simulation's AI traffic, used by the same validated
/// administrative world-command handler as the graphical host.
pub struct NativeTraffic<'a>(pub &'a mut omsi_sim::ai_traffic::TrafficSim);

impl omsi_lan::admin::world::ServerTraffic for NativeTraffic<'_> {
    fn set_density(&mut self, n: usize) {
        self.0.target = n;
    }

    fn clear_cars(&mut self) -> usize {
        use omsi_sim::ai_traffic::control::Removal;
        let mut removed = 0;
        for index in (0..self.0.cars.len()).rev() {
            if !self.0.cars[index].is_bus() {
                let car = self.0.take_off(index, Removal::Finished);
                // Retain the removal until the network publisher has sent it.
                self.0.retired.push(car.id);
                removed += 1;
            }
        }
        removed
    }
}

#[cfg(test)]
mod native_traffic_tests {
    use super::*;
    use omsi_lan::admin::world::ServerTraffic;

    #[test]
    fn admin_density_changes_the_population_target_not_just_the_panel() {
        let random = omsi_sim::ai_traffic::setup::RandomTypes {
            types: Vec::new(),
            groups: Vec::new(),
            group_curves: false,
            group_uvg: Vec::new(),
            uvg_defaults: Vec::new(),
        };
        let mut traffic = crate::traffic::assemble(
            std::path::Path::new("."),
            omsi_sim::traffic::Network::default(),
            random,
            Vec::new(),
            Vec::new(),
            8,
        );
        let mut admin = NativeTraffic(&mut traffic);
        admin.set_density(3);
        assert_eq!(admin.0.target, 3);
        assert_eq!(admin.clear_cars(), 0);
    }
}
