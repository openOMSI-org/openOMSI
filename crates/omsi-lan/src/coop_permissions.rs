//! Authorization policy for planned optional cooperative roles.
//!
//! This module has no network side effects. The caller must authenticate peer identity
//! and apply a successful decision on the host before any vehicle or fare state changes.
//! Neither role is enabled by default.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoopRoles {
    pub driver_handover: bool,
    pub conductor: bool,
}

/// Facts provided by a verified LAN session when transferring driving authority.
#[derive(Clone, Copy, Debug, Default)]
pub struct DriverRequest {
    pub same_session: bool,
    pub host_enabled: bool,
    pub owner_consented: bool,
    pub guest_consented: bool,
    pub guest_inside_bus: bool,
    pub bus_content_installed: bool,
    pub bus_speed_kmh: f32,
    pub authority_token: u64,
}

pub fn may_handover_driver(roles: CoopRoles, request: DriverRequest) -> bool {
    roles.driver_handover
        && request.same_session
        && request.host_enabled
        && request.owner_consented
        && request.guest_consented
        && request.guest_inside_bus
        && request.bus_content_installed
        && request.bus_speed_kmh.is_finite()
        && request.bus_speed_kmh.abs() <= 0.5
        && request.authority_token != 0
}

/// A narrow in-cabin request. It never transfers driving authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CabinAction {
    Door(usize),
    DoorSwitch(String),
    Window(usize),
    /// A key from the guest; resolved against the *owner's* keyboard.cfg.
    Key(i32, i32),
    Switch(String),
    Adjust(String, i8),
    Fare(FareAction),
    /// A dedicated two-tone departure signal, without touching door state.
    DepartureSignal,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CabinRequest {
    pub same_session: bool,
    pub owner_consented: bool,
    pub passenger_inside_bus: bool,
    pub speed_kmh: f32,
}

/// Door, boarding aid and cash-desk shortcuts require a stationary bus.
/// The remaining controls are validated against the owner's model before execution.
pub fn may_operate_cabin(roles: CoopRoles, req: CabinRequest, action: &CabinAction) -> bool {
    roles.conductor
        && req.same_session
        && req.owner_consented
        && req.passenger_inside_bus
        && req.speed_kmh.is_finite()
        && match action {
            CabinAction::Door(index) => *index < 9 && req.speed_kmh.abs() <= 0.5,
            CabinAction::DoorSwitch(event) => door_switch(event) && req.speed_kmh.abs() <= 0.5,
            CabinAction::Window(index) => *index < 9,
            CabinAction::Key(scan, modifiers) => valid_cabin_key(*scan, *modifiers),
            CabinAction::Switch(event) => {
                auxiliary_switch(event) && switch_speed_allowed(event, req.speed_kmh)
            }
            CabinAction::Adjust(event, direction) => {
                is_adjustable_control(event)
                    && matches!(direction, -1 | 1)
                    && switch_speed_allowed(event, req.speed_kmh)
            }
            CabinAction::Fare(_) | CabinAction::DepartureSignal => req.speed_kmh.abs() <= 0.5,
        }
}

/// Commands are routed through the authenticated LAN session, not chat text.
impl CabinAction {
    pub fn command(&self) -> String {
        match self {
            Self::Door(index) => format!("coop-door {index}"),
            Self::DoorSwitch(event) => format!("coop-door-switch {event}"),
            Self::Window(index) => format!("coop-window {index}"),
            Self::Key(scan, modifiers) => format!("coop-key {scan} {modifiers}"),
            Self::Switch(event) => format!("coop-switch {event}"),
            Self::Adjust(event, direction) => format!("coop-adjust {event} {direction}"),
            Self::Fare(fare) => format!("coop-fare {}", fare.as_str()),
            Self::DepartureSignal => "coop-signal departure".into(),
        }
    }
}

pub fn parse_cabin_action(text: &str) -> Option<CabinAction> {
    let mut words = text.split_whitespace();
    let kind = words.next()?;
    let value = words.next()?;
    if kind == "coop-key" {
        let scan = value.parse::<i32>().ok()?;
        let modifiers = words.next()?.parse::<i32>().ok()?;
        return (words.next().is_none() && valid_cabin_key(scan, modifiers))
            .then_some(CabinAction::Key(scan, modifiers));
    }
    if kind == "coop-adjust" {
        let direction = words.next()?.parse::<i8>().ok()?;
        if words.next().is_none() && is_adjustable_control(value) && matches!(direction, -1 | 1) {
            return Some(CabinAction::Adjust(value.into(), direction));
        }
        return None;
    }
    if words.next().is_some() {
        return None;
    }
    match kind {
        "coop-door" => value.parse().ok().filter(|&i| i < 9).map(CabinAction::Door),
        "coop-door-switch" if door_switch(value) => Some(CabinAction::DoorSwitch(value.into())),
        "coop-window" => value
            .parse()
            .ok()
            .filter(|&i| i < 9)
            .map(CabinAction::Window),
        "coop-switch" if auxiliary_switch(value) => Some(CabinAction::Switch(value.into())),
        "coop-fare" => FareAction::parse(value).map(CabinAction::Fare),
        "coop-signal" if value == "departure" => Some(CabinAction::DepartureSignal),
        _ => None,
    }
}

/// A selected assistant is the only peer allowed to use cabin controls.
/// Id zero and an absent assignment always fail closed.
pub fn matches_assistant(assigned: Option<u32>, sender: u32) -> bool {
    sender != 0 && assigned == Some(sender)
}

/// Keyboard packets carry physical scan codes, never client-chosen script names.
fn valid_cabin_key(scan: i32, modifiers: i32) -> bool {
    (1..=255).contains(&scan) && modifiers & !(2 | 4 | 8) == 0
}

/// Only the owning player's local key bindings select a script action.
/// This filter also applies to model mouse events and preserves driving isolation.
pub fn keyboard_cabin_action(event: &str) -> bool {
    door_switch(event)
        || window_control(event)
        || auxiliary_switch(event)
        || matches!(
            event.to_ascii_lowercase().as_str(),
            "ticket_give"
                | "change_give"
                | "change_take"
                | "blinker_left_toggle"
                | "blinker_right_toggle"
        )
}

pub fn keyboard_cabin_speed_allowed(event: &str, speed_kmh: f32) -> bool {
    let name = event.to_ascii_lowercase();
    let stopped_only = door_switch(event)
        || ["ticket_give", "change_give", "change_take"].contains(&name.as_str())
        || ["kneel", "ramp", "lift_"]
            .iter()
            .any(|tag| name.contains(tag));
    speed_kmh.is_finite() && (!stopped_only || speed_kmh.abs() <= 0.5)
}

/// Door controls remain stationary-only, even when their script names are custom.
fn door_switch(event: &str) -> bool {
    if event.is_empty()
        || event.len() > 96
        || !event
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_+-".contains(&b))
    {
        return false;
    }
    let name = event.to_ascii_lowercase();
    (name.contains("door") || name.contains("tuer"))
        && ![
            "steer",
            "lenk",
            "throttle",
            "accelerator",
            "brake",
            "bremse",
            "gear",
            "gang",
            "clutch",
            "kupplung",
            "ignition",
            "engine",
            "motor",
            "starter",
            "anlasser",
            "battery",
            "batter",
            "retarder",
            "fahrstufe",
            "drive",
            "handbr",
            "pedal",
            "parking",
            "getriebe",
            "override",
        ]
        .iter()
        .any(|term| name.contains(term))
}

fn window_control(event: &str) -> bool {
    control_name(event).is_some_and(|name| name.contains("window") || name.contains("fenster"))
}

fn control_name(event: &str) -> Option<String> {
    if event.is_empty()
        || event.len() > 96
        || !event
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_+-".contains(&b))
    {
        return None;
    }

    let name = event.to_ascii_lowercase();
    if name.ends_with("_off")
        || name.ends_with("_drag")
        || [
            "steer",
            "lenk",
            "throttle",
            "gas_pedal",
            "accelerator",
            "brake",
            "bremse",
            "gear",
            "gang",
            "clutch",
            "kupplung",
            "ignition",
            "zuend",
            "zund",
            "engine",
            "motor",
            "starter",
            "anlasser",
            "battery",
            "batter",
            "retarder",
            "fahrstufe",
            "drive",
            "handbr",
            "holdbrake",
            "pedal",
            "door",
            "tuer",
            "shifter",
            "parking",
            "getriebe",
        ]
        .iter()
        .any(|term| name.contains(term))
    {
        return None;
    }

    Some(name)
}

/// These are the model control families that the owner's client may execute.
fn auxiliary_switch(event: &str) -> bool {
    let Some(name) = control_name(event) else {
        return false;
    };
    let family = name
        .strip_prefix("cp_")
        .or_else(|| name.strip_prefix("cockpit_"))
        .or_else(|| name.strip_prefix("taster_"))
        .unwrap_or(&name);
    [
        "licht",
        "light",
        "fahrerlicht",
        "innenlicht",
        "scheinwerfer",
        "blinker",
        "warnblinker",
        "horn",
        "hupe",
        "microphone",
        "ansage",
        "kneel",
        "autokneel",
        "rampe",
        "ramp_",
        "wheelchair_ramp",
        "lift_",
        "schalter_kinderwagen",
        "wiper",
        "wischer",
        "wascher",
        "heiz",
        "heat",
        "hvac",
        "klima",
        "lueft",
        "fan_",
        "vent_",
        "defrost",
        "spiegelheizung",
        "geblaese",
        "umluft",
        "rollo",
        "sunblind",
        "sonnenblende",
        "ibis",
        "ibox",
        "afc_",
        "matrix_",
        "ziel",
        "rollband",
        "bus_rollband",
        "rlbnd",
        "rlb_",
        "ticketprinter",
        "ticket_printer",
        "ticketmachine",
        "ticket_machine",
        "ticket_",
        "fare_",
        "cashdesk",
        "zahltisch",
        "kassentisch",
        "entwerter",
        "changer",
        "change_",
        "wechsel",
        "drucker",
        "radio",
        "lautsprecher",
        "speaker",
        "display",
        "dashboard",
        "instrument",
        "heizung",
        "heizregler",
        "intercom",
        "passenger_light",
        "saloon",
        "destination",
        "mirror",
        "spiegel",
        "windscreen",
        "windshield",
        "gong",
        "bell",
        "haltwunsch",
        "uhr",
        "clock",
    ]
    .iter()
    .any(|prefix| family.starts_with(prefix))
        // Vehicle mods often use custom cockpit names. Permit their physical
        // mouse/key events after rejecting all motion and engine-related names;
        // the owner still checks its own model or keyboard binding before firing.
        || ["cp_", "cockpit_", "taster_", "schalter_", "switch_", "button_"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

fn switch_speed_allowed(event: &str, speed_kmh: f32) -> bool {
    let name = event.to_ascii_lowercase();
    speed_kmh.abs() <= 0.5
        || !["kneel", "ramp", "lift_", "door", "tuer"]
            .iter()
            .any(|tag| name.contains(tag))
}

fn is_adjustable_control(event: &str) -> bool {
    (window_control(event) || auxiliary_switch(event)) && !momentary_button(event)
}

fn momentary_button(event: &str) -> bool {
    let name = event.to_ascii_lowercase();
    ["taste", "ibox", "ticket"]
        .iter()
        .any(|tag| name.contains(tag))
}

fn window_events(ty: &omsi_sim::VehicleType) -> Vec<&str> {
    let mut events: Vec<_> = ty
        .model
        .meshes
        .iter()
        .filter_map(|mesh| mesh.mouse_event.as_deref())
        .filter(|event| window_control(event))
        .collect();
    events.sort_unstable();
    events.dedup();
    events
}

pub fn click_cabin_window(vehicle: &mut omsi_sim::VehicleInstance, index: usize) -> bool {
    let ty = vehicle.ty.clone();
    window_events(&ty)
        .get(index)
        .is_some_and(|event| click_auxiliary_switch(vehicle, event))
}

pub fn cabin_adjustment_for_model(
    ty: &omsi_sim::VehicleType,
    event: &str,
    direction: i8,
) -> Option<CabinAction> {
    supports_adjustment(ty, event, direction).then(|| CabinAction::Adjust(event.into(), direction))
}

fn supports_adjustment(ty: &omsi_sim::VehicleType, event: &str, direction: i8) -> bool {
    is_adjustable_control(event)
        && matches!(direction, -1 | 1)
        && model_has_event(ty, event)
        && ty.program.trigger(&format!("{event}_drag")).is_some()
}

pub fn adjust_auxiliary_switch(
    vehicle: &mut omsi_sim::VehicleInstance,
    event: &str,
    direction: i8,
) -> bool {
    if !supports_adjustment(&vehicle.ty, event, direction) {
        return false;
    }
    let step = f32::from(direction) * 40.0;
    vehicle.host.mouse = if window_control(event) {
        (step, 0.0)
    } else {
        (0.0, step)
    };
    let moved = vehicle.trigger(&format!("{event}_drag"));
    vehicle.host.mouse = (0.0, 0.0);
    vehicle.trigger(&format!("{event}_off"));
    moved
}

/// Apply the switch only if this exact mouse event exists on the owner's model.
/// Run one script frame before releasing a momentary printer or IBIS key.
pub fn click_auxiliary_switch(vehicle: &mut omsi_sim::VehicleInstance, event: &str) -> bool {
    if !(window_control(event) || auxiliary_switch(event) || door_switch(event))
        || !model_has_event(&vehicle.ty, event)
        || !vehicle.trigger(event)
    {
        return false;
    }
    vehicle.update_scripts_only(0.0);
    vehicle.host.mouse = (0.0, 0.0);
    if momentary_button(event) {
        vehicle.trigger(&format!("{event}_drag"));
    }
    vehicle.trigger(&format!("{event}_off"));
    true
}

fn model_has_event(ty: &omsi_sim::VehicleType, event: &str) -> bool {
    ty.model
        .meshes
        .iter()
        .any(|mesh| mesh.mouse_event.as_deref() == Some(event))
}

/// Translate a real model click to the existing restricted cabin wire actions.
/// The event must belong to one of this vehicle's mouse-event meshes; never run
/// arbitrary script triggers supplied by network peers.
pub fn cabin_action_for_model(ty: &omsi_sim::VehicleType, event: &str) -> Option<CabinAction> {
    if !model_has_event(ty, event) {
        return None;
    }
    if let Some(index) = omsi_sim::door_keys::door_keys(ty).iter().position(|group| {
        group
            .iter()
            .any(|name| name.split('|').any(|trigger| trigger == event))
    }) {
        return (index < 9).then_some(CabinAction::Door(index));
    }
    if door_switch(event) {
        return Some(CabinAction::DoorSwitch(event.into()));
    }
    if !window_control(event) {
        return auxiliary_switch(event).then(|| CabinAction::Switch(event.into()));
    }
    window_events(ty)
        .iter()
        .position(|name| *name == event)
        .filter(|index| *index < 9)
        .map(CabinAction::Window)
}

/// Ticket operations are narrower than generic in-cab triggers. Normal passenger
/// stop bells are outside this grant; engine, door and steering actions are never fares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FareAction {
    IssueTicket,
    GiveChange,
    CollectPayment,
}

impl FareAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::IssueTicket => "ticket",
            Self::GiveChange => "change",
            Self::CollectPayment => "collect",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "ticket" => Some(Self::IssueTicket),
            "change" => Some(Self::GiveChange),
            "collect" => Some(Self::CollectPayment),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ConductorRequest {
    pub host_enabled: bool,
    pub owner_consented: bool,
    pub peer_inside_bus: bool,
    pub same_session: bool,
    pub actor: u32,
    pub epoch: u64,
    pub desk: u32,
    pub seq: u64,
}

/// The currently delegated desk transaction. The epoch must change for a new grant.
/// A command from an earlier ticket or a repeated sequence number is rejected.
#[derive(Clone, Copy, Debug, Default)]
pub struct FareGrant {
    pub actor: u32,
    pub epoch: u64,
    pub desk: u32,
    pub last_seq: u64,
}

impl FareGrant {
    pub fn authorize(
        &mut self,
        roles: CoopRoles,
        request: ConductorRequest,
        _action: FareAction,
    ) -> bool {
        if !roles.conductor
            || !request.host_enabled
            || !request.owner_consented
            || !request.peer_inside_bus
            || !request.same_session
            || request.actor == 0
            || request.actor != self.actor
            || request.epoch == 0
            || request.epoch != self.epoch
            || request.desk == 0
            || request.desk != self.desk
            || request.seq <= self.last_seq
        {
            return false;
        }
        self.last_seq = request.seq;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn driver() -> DriverRequest {
        DriverRequest {
            same_session: true,
            host_enabled: true,
            owner_consented: true,
            guest_consented: true,
            guest_inside_bus: true,
            bus_content_installed: true,
            bus_speed_kmh: 0.0,
            authority_token: 42,
        }
    }

    #[test]
    fn driving_authority_is_explicitly_opt_in_and_requires_a_stopped_bus() {
        let enabled = CoopRoles {
            driver_handover: true,
            ..Default::default()
        };
        assert!(!may_handover_driver(CoopRoles::default(), driver()));
        assert!(may_handover_driver(enabled, driver()));
        assert!(!may_handover_driver(
            enabled,
            DriverRequest {
                bus_speed_kmh: 1.0,
                ..driver()
            }
        ));
        assert!(!may_handover_driver(
            enabled,
            DriverRequest {
                bus_speed_kmh: f32::NAN,
                ..driver()
            }
        ));
        assert!(!may_handover_driver(
            enabled,
            DriverRequest {
                owner_consented: false,
                ..driver()
            }
        ));
        assert!(!may_handover_driver(
            enabled,
            DriverRequest {
                bus_content_installed: false,
                ..driver()
            }
        ));
        assert!(!may_handover_driver(
            enabled,
            DriverRequest {
                guest_inside_bus: false,
                ..driver()
            }
        ));
        assert!(!may_handover_driver(
            enabled,
            DriverRequest {
                authority_token: 0,
                ..driver()
            }
        ));
    }

    fn fare() -> ConductorRequest {
        ConductorRequest {
            host_enabled: true,
            owner_consented: true,
            peer_inside_bus: true,
            same_session: true,
            actor: 7,
            epoch: 123,
            desk: 11,
            seq: 1,
        }
    }

    #[test]
    fn cabin_controls_require_owner_opt_in_and_real_presence() {
        let roles = CoopRoles {
            conductor: true,
            ..Default::default()
        };
        let valid = CabinRequest {
            same_session: true,
            owner_consented: true,
            passenger_inside_bus: true,
            speed_kmh: 0.0,
        };
        assert!(!may_operate_cabin(
            CoopRoles::default(),
            valid,
            &CabinAction::Door(0)
        ));
        assert!(may_operate_cabin(roles, valid, &CabinAction::Door(0)));
        assert!(!may_operate_cabin(
            roles,
            CabinRequest {
                owner_consented: false,
                ..valid
            },
            &CabinAction::Door(0)
        ));
        assert!(!may_operate_cabin(
            roles,
            CabinRequest {
                passenger_inside_bus: false,
                ..valid
            },
            &CabinAction::Window(0)
        ));
        assert!(!may_operate_cabin(
            roles,
            CabinRequest {
                speed_kmh: 30.0,
                ..valid
            },
            &CabinAction::Door(0)
        ));
        assert!(may_operate_cabin(
            roles,
            CabinRequest {
                speed_kmh: 30.0,
                ..valid
            },
            &CabinAction::Window(0)
        ));
        let fare = CabinAction::Fare(FareAction::IssueTicket);
        assert!(may_operate_cabin(roles, valid, &fare));
        assert!(!may_operate_cabin(
            roles,
            CabinRequest {
                owner_consented: false,
                ..valid
            },
            &fare
        ));
        assert!(!may_operate_cabin(
            roles,
            CabinRequest {
                speed_kmh: 12.0,
                ..valid
            },
            &fare
        ));
        assert!(!may_operate_cabin(roles, valid, &CabinAction::Window(9)));
    }

    #[test]
    fn cabin_commands_cannot_be_used_to_inject_engine_or_steering_triggers() {
        assert_eq!(
            parse_cabin_action("coop-door 2"),
            Some(CabinAction::Door(2))
        );
        assert_eq!(
            parse_cabin_action("coop-window 0"),
            Some(CabinAction::Window(0))
        );
        for invalid in [
            "coop-door 9",
            "coop-door -1",
            "coop-door 0 engine_start",
            "trigger engine_start",
            "coop-window 100",
            "coop-open 0",
        ] {
            assert!(parse_cabin_action(invalid).is_none());
        }
        assert!(window_control("cp_fenster_rechts"));
        assert!(window_control("bus_window_left"));
        assert!(!window_control("window_engine_start"));
        assert!(!window_control("window_slider_drag"));
        assert!(!window_control("bus_doorfront0"));
    }

    #[test]
    fn generic_cabin_consent_cannot_authorize_other_players() {
        assert!(matches_assistant(Some(7), 7));
        assert!(!matches_assistant(Some(7), 8));
        assert!(!matches_assistant(Some(7), 0));
        assert!(!matches_assistant(None, 7));
    }

    #[test]
    fn auxiliary_switches_and_fare_actions_accept_only_supported_inputs() {
        for event in [
            "cp_licht_unterdeck",
            "cp_wischer_intervall",
            "ibox_taste_1",
            "ticketprinter_getticket",
            "hvac_fan",
        ] {
            let action = CabinAction::Switch(event.into());
            assert_eq!(parse_cabin_action(&action.command()), Some(action));
        }
        for event in [
            "engine_start",
            "cockpit_steering",
            "cp_batterietrennschalter_toggle",
            "ticketprinter_gear_up",
            "ibox_motor_off",
            "ticketprinter_print_drag",
            "override",
        ] {
            assert!(parse_cabin_action(&format!("coop-switch {event}")).is_none());
        }
        for fare in [
            FareAction::IssueTicket,
            FareAction::GiveChange,
            FareAction::CollectPayment,
        ] {
            let action = CabinAction::Fare(fare);
            assert_eq!(parse_cabin_action(&action.command()), Some(action));
        }
        assert!(parse_cabin_action("coop-fare change 500").is_none());
        assert_eq!(
            parse_cabin_action("coop-adjust cp_fenster_rechts -1"),
            Some(CabinAction::Adjust("cp_fenster_rechts".into(), -1))
        );
        assert!(parse_cabin_action("coop-adjust cp_fenster_rechts 2").is_none());
        assert!(parse_cabin_action("coop-adjust cp_wiper_engine 1").is_none());
        assert!(parse_cabin_action("coop-adjust ticketprinter_getticket 1").is_none());
        assert!(switch_speed_allowed("cp_rampe", 0.0));
        assert!(!switch_speed_allowed("cp_rampe", 15.0));
    }

    #[test]
    fn keybinds_use_owner_resolution_and_reject_movement_commands() {
        let valid = CabinRequest {
            same_session: true,
            owner_consented: true,
            passenger_inside_bus: true,
            speed_kmh: 0.0,
        };
        let roles = CoopRoles {
            conductor: true,
            ..Default::default()
        };
        assert_eq!(
            parse_cabin_action("coop-key 17 2"),
            Some(CabinAction::Key(17, 2))
        );
        assert!(may_operate_cabin(roles, valid, &CabinAction::Key(17, 2)));
        assert!(!may_operate_cabin(
            CoopRoles::default(),
            valid,
            &CabinAction::Key(17, 2)
        ));
        for invalid in [
            "coop-key 0 0",
            "coop-key 999 0",
            "coop-key 17 1",
            "coop-key 17 16",
            "coop-key 17 2 extra",
            "coop-key 17 -1",
        ] {
            assert!(parse_cabin_action(invalid).is_none(), "{invalid}");
        }
        for name in [
            "kw_s_1",
            "engine_start",
            "cp_bremse",
            "throttle",
            "cockpit_steering",
            "ticketprinter_gear_up",
        ] {
            assert!(!keyboard_cabin_action(name), "{name}");
        }
        assert!(keyboard_cabin_action("cp_licht_unterdeck"));
        assert!(keyboard_cabin_action("cockpit_custom_aux"));
        assert!(!keyboard_cabin_action("cockpit_engine_start"));
        assert!(keyboard_cabin_action("bus_doorfront0"));
        assert!(!keyboard_cabin_speed_allowed("bus_doorfront0", 25.0));
        assert!(keyboard_cabin_speed_allowed("cp_licht_unterdeck", 25.0));
        assert_eq!(
            parse_cabin_action("coop-door-switch cp_tuertaster1"),
            Some(CabinAction::DoorSwitch("cp_tuertaster1".into()))
        );
        assert!(may_operate_cabin(
            roles,
            valid,
            &CabinAction::DoorSwitch("cp_tuertaster1".into())
        ));
        assert!(!may_operate_cabin(
            roles,
            CabinRequest {
                speed_kmh: 15.0,
                ..valid
            },
            &CabinAction::DoorSwitch("cp_tuertaster1".into())
        ));
    }

    #[test]
    fn departure_signal_is_a_bounded_stationary_only_cabin_request() {
        let signal = CabinAction::DepartureSignal;
        assert_eq!(signal.command(), "coop-signal departure");
        assert_eq!(parse_cabin_action(&signal.command()), Some(signal.clone()));
        assert!(parse_cabin_action("coop-signal door-open").is_none());
        assert!(parse_cabin_action("coop-signal departure extra").is_none());
        let roles = CoopRoles {
            conductor: true,
            ..Default::default()
        };
        let approved = CabinRequest {
            same_session: true,
            owner_consented: true,
            passenger_inside_bus: true,
            speed_kmh: 0.0,
        };
        assert!(may_operate_cabin(roles, approved, &signal));
        assert!(!may_operate_cabin(
            roles,
            CabinRequest {
                speed_kmh: 8.0,
                ..approved
            },
            &signal
        ));
        assert!(!may_operate_cabin(
            roles,
            CabinRequest {
                owner_consented: false,
                ..approved
            },
            &signal
        ));
        assert!(!may_operate_cabin(
            roles,
            CabinRequest {
                passenger_inside_bus: false,
                ..approved
            },
            &signal
        ));
        assert!(!may_operate_cabin(CoopRoles::default(), approved, &signal));
    }

    #[test]
    fn ticket_grants_reject_replays_other_passengers_and_revoked_roles() {
        let enabled = CoopRoles {
            conductor: true,
            ..Default::default()
        };
        let grant = FareGrant {
            actor: 7,
            epoch: 123,
            desk: 11,
            last_seq: 0,
        };
        let mut g = grant;
        assert!(!g.authorize(CoopRoles::default(), fare(), FareAction::IssueTicket));
        assert_eq!(g.last_seq, 0);
        assert!(g.authorize(enabled, fare(), FareAction::IssueTicket));
        assert!(!g.authorize(enabled, fare(), FareAction::GiveChange));
        assert!(!g.authorize(
            enabled,
            ConductorRequest {
                actor: 8,
                seq: 2,
                ..fare()
            },
            FareAction::IssueTicket
        ));
        assert!(!g.authorize(
            enabled,
            ConductorRequest {
                epoch: 124,
                seq: 2,
                ..fare()
            },
            FareAction::IssueTicket
        ));
        assert!(!g.authorize(
            enabled,
            ConductorRequest {
                desk: 12,
                seq: 2,
                ..fare()
            },
            FareAction::GiveChange
        ));
        assert!(!g.authorize(
            enabled,
            ConductorRequest {
                owner_consented: false,
                seq: 2,
                ..fare()
            },
            FareAction::GiveChange
        ));
        assert!(!g.authorize(
            enabled,
            ConductorRequest {
                peer_inside_bus: false,
                seq: 2,
                ..fare()
            },
            FareAction::GiveChange
        ));
        assert!(g.authorize(
            enabled,
            ConductorRequest { seq: 2, ..fare() },
            FareAction::GiveChange
        ));
    }
}
