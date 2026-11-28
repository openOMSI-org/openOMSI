/// Script-side data belongs to the host's AI instance, including schedule_active.
pub(super) fn ai_context(host: &omsi_sim::VehicleHost) -> omsi_net::ai_context::Context {
    omsi_lan::mirror::ai_context(host)
}

pub(crate) fn apply_ai_context(
    host: &mut omsi_sim::VehicleHost,
    context: &omsi_net::ai_context::Context,
) {
    host.apply_ai_system_values(&context.system);
    // Between host snapshots, scripts need a progressing local frame clock.
    host.external_ai_clock = false;
    host.schedule_active = context.active;
    host.tt_line.clone_from(&context.line);
    host.tt_terminus_index = context.terminus;
    host.tt_busstop_index = context.index;
    host.tt_delay = context.delay;
    host.tt_stops = context
        .stops
        .iter()
        .map(|s| (s.1.clone(), s.2, s.3))
        .collect();
    host.tt_stop_ids = context.stops.iter().map(|s| s.0).collect();
}
