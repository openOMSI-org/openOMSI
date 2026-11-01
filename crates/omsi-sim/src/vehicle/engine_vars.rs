//! The variables the engine writes into every vehicle every frame (the weather and the road,
//! the odometer, the cabin air, an AI vehicle's lamps and stop), looked up once per vehicle.
//! By name each one was a hash look-up of a lower-cased copy, some 25 a vehicle a frame.
//! Made again when the vehicle gains a variable (`set_engine_var`), so what they read and
//! write is what the names would.

use super::*;
use omsi_script::VarId;

/// One of the engine's variables (see the module); the names are [`NAMES`]'s.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Ev {
    DirtNorm,
    DirtRate,
    PrecipRate,
    StreetCond,
    TrainFrontCoupling,
    TrainBackCoupling,
    TrainMeReverse,
    KmCounterKm,
    KmCounterM,
    HumansCount,
    ScheduleActive,
    PrecipType,
    CabinTemp,
    CabinAbsHum,
    CabinRelHum,
    Ai,
    AiBlinkerL,
    AiBlinkerR,
    AiBrakelight,
    AiLight,
    AiInteriorlight,
    AiEngine,
    AiAtStation,
    AiAtStationSide,
    PriorityWarning,
}

const COUNT: usize = Ev::PriorityWarning as usize + 1;

/// The variables' names, in the order of [`Ev`].
const NAMES: [&str; COUNT] = [
    "Dirt_Norm",
    "DirtRate",
    "PrecipRate",
    "StreetCond",
    "train_frontcoupling",
    "train_backcoupling",
    "train_me_reverse",
    "kmcounter_km",
    "kmcounter_m",
    "humans_count",
    "schedule_active",
    "precipType",
    "Cabinair_Temp",
    "Cabinair_absHum",
    "Cabinair_relHum",
    "AI",
    "AI_Blinker_L",
    "AI_Blinker_R",
    "AI_Brakelight",
    "AI_Light",
    "AI_Interiorlight",
    "AI_Engine",
    "AI_Scheduled_AtStation",
    "AI_Scheduled_AtStation_Side",
    "TrafficPriorityWarningNeeded",
];

/// The ids of [`NAMES`] in a vehicle's variables (None: it has none of that name).
pub(crate) type EngineVarIds = [Option<VarId>; COUNT];

impl VehicleInstance {
    /// Player-driven frames must not replay commands left by the last AI frame.
    /// Preserve the actual engine, doors and lamps; only release the AI inputs.
    pub(super) fn release_ai_commands(&mut self) {
        for ev in [Ev::Ai, Ev::AiEngine, Ev::AiBlinkerL, Ev::AiBlinkerR,
            Ev::AiBrakelight, Ev::AiLight, Ev::AiInteriorlight,
            Ev::AiAtStation, Ev::AiAtStationSide, Ev::PriorityWarning] {
            self.set_ev(ev, 0.0);
        }
    }

    fn ev_id(&self, ev: Ev) -> Option<VarId> {
        self.engine_ids.get_or_init(|| NAMES.map(|n| omsi_script::compile::with_lower(n, |k| self.var_index.get(k).copied())))[ev as usize]
    }

    /// The engine variable `ev`, as [`VehicleInstance::var`] its name.
    pub(crate) fn ev(&self, ev: Ev) -> Option<f32> {
        self.ev_id(ev).map(|i| self.state.vars[i as usize])
    }

    /// Set the engine variable `ev` when the vehicle has it, as [`VehicleInstance::set_var`].
    pub(crate) fn set_ev(&mut self, ev: Ev, v: f32) -> bool {
        match self.ev_id(ev) {
            Some(i) => {
                self.state.vars[i as usize] = v;
                true
            }
            None => false,
        }
    }

    /// Set the engine variable `ev`, adding it when the vehicle has none of the name, as
    /// [`VehicleInstance::set_engine_var`].
    pub(crate) fn set_engine_ev(&mut self, ev: Ev, v: f32) {
        if !self.set_ev(ev, v) {
            self.set_engine_var(NAMES[ev as usize], v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names are in the order of the variants.
    #[test]
    fn names_follow_the_variants() {
        assert_eq!(NAMES[Ev::DirtNorm as usize], "Dirt_Norm");
        assert_eq!(NAMES[Ev::CabinRelHum as usize], "Cabinair_relHum");
        assert_eq!(NAMES[Ev::Ai as usize], "AI");
        assert_eq!(NAMES[Ev::AiAtStationSide as usize], "AI_Scheduled_AtStation_Side");
        assert_eq!(NAMES[Ev::PriorityWarning as usize], "TrafficPriorityWarningNeeded");
    }

    #[test]
    fn player_frames_release_stale_ai_engine_commands() {
        let script = "{frame}\n(L.L.AI_Engine) 0.5 > {if} 1 (S.L.engine_on) {endif}\n(L.L.AI_Engine) -0.5 < {if} 0 (S.L.engine_on) {endif}\n{end}\n";
        for (command, running) in [(1.0, 0.0), (-1.0, 1.0)] {
            let mut v = crate::timetable_run::tests::script_test_vehicle(
                script, "AI\nAI_Engine\nengine_on\n", "",
            );
            v.set_var("AI", 1.0);
            v.set_var("AI_Engine", command);
            v.set_var("engine_on", running);
            v.update(0.02);
            assert_eq!(v.var("engine_on"), Some(running), "stale AI command {command}");
            assert_eq!(v.var("AI"), Some(0.0));
            assert_eq!(v.var("AI_Engine"), Some(0.0));
        }
    }

    #[test]
    fn player_script_refresh_releases_stale_ai_engine_commands() {
        let mut v = crate::timetable_run::tests::script_test_vehicle(
            "{frame}\n(L.L.AI_Engine) 0.5 > {if} 1 (S.L.engine_on) {endif}\n{end}\n",
            "AI\nAI_Engine\nengine_on\n", "",
        );
        v.set_var("AI", 1.0);
        v.set_var("AI_Engine", 1.0);
        v.update_scripts_only(0.0);
        assert_eq!(v.var("engine_on"), Some(0.0));
        assert_eq!(v.var("AI"), Some(0.0));
    }

    #[test]
    fn ai_frames_keep_engine_control_after_player_handover() {
        let mut v = crate::timetable_run::tests::script_test_vehicle(
            "{frame}\n(L.L.AI_Engine) 0.5 > {if} 1 (S.L.engine_on) {endif}\n(L.L.AI_Engine) -0.5 < {if} 0 (S.L.engine_on) {endif}\n{end}\n",
            "AI\nAI_Engine\nengine_on\n", "",
        );
        for engine_off in [false, true] {
            v.update_ai(0.02, &super::super::AiFrame { engine_off, ..Default::default() });
            assert_eq!(v.var("AI"), Some(1.0));
            assert_eq!(v.var("AI_Engine"), Some(if engine_off { -1.0 } else { 1.0 }));
            assert_eq!(v.var("engine_on"), Some(if engine_off { 0.0 } else { 1.0 }));
            let player_state = if engine_off { 1.0 } else { 0.0 };
            v.set_var("engine_on", player_state);
            v.update(0.02);
            assert_eq!(v.var("engine_on"), Some(player_state));
            assert_eq!(v.var("AI"), Some(0.0));
        }
    }

    #[test]
    fn releasing_ai_inputs_preserves_actual_vehicle_state() {
        let mut v = crate::timetable_run::tests::script_test_vehicle(
            "", "AI\nAI_Engine\nAI_Scheduled_AtStation\nAI_Scheduled_AtStation_Side\nengine_on\ndoor_0\nlights_stand\n", "",
        );
        for name in ["AI", "AI_Engine", "AI_Scheduled_AtStation", "AI_Scheduled_AtStation_Side", "engine_on", "door_0", "lights_stand"] {
            v.set_var(name, 1.0);
        }
        v.release_ai_commands();
        for name in ["AI", "AI_Engine", "AI_Scheduled_AtStation", "AI_Scheduled_AtStation_Side"] {
            assert_eq!(v.var(name), Some(0.0), "{name}");
        }
        for name in ["engine_on", "door_0", "lights_stand"] {
            assert_eq!(v.var(name), Some(1.0), "{name}");
        }
    }

    #[test]
    #[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
    fn stock_ai_takeover_keeps_the_destination_after_player_frames() {
        let root = std::path::PathBuf::from(std::env::var_os("OMSI_ROOT").expect("OMSI_ROOT"));
        let file = root.join("Vehicles/MAN_SD202/MAN_D91_drucker.bus");
        let ai = std::sync::Arc::new(VehicleType::load_ai(&root, &file).unwrap());
        let player = std::sync::Arc::new(VehicleType::load(&root, &file).unwrap());
        let hof = std::sync::Arc::new(omsi_vehicle::Hof::load(&root.join("Vehicles/MAN_SD202/Spandau 94.hof")).unwrap());
        let mut host = VehicleHost::new(Default::default());
        host.hof = Some(hof.clone());
        let mut v = VehicleInstance::new(ai, host);
        crate::timetable_run::set_ai_destination(&mut v, Some(&hof), "137", "Falkensee", &[]);
        for _ in 0..2000 { v.update_ai(0.02, &super::super::AiFrame::default()); }
        let before = crate::timetable_run::shown_destination(&v, &hof, None);
        let sign = v.str_var("Matrix_Terminus");
        let people = crate::people::PeopleSim::new(&root, 100);
        let passenger_target = people.player_bus_view(&v).terminus;
        assert!(before.is_some_and(|i| !hof.termini[i].all_exit && hof.termini[i].code != 0));
        assert_eq!(v.ty.program.var_names, player.program.var_names);
        v.ty = player;
        for _ in 0..500 { v.update(0.02); }
        assert_eq!(crate::timetable_run::shown_destination(&v, &hof, None), before);
        assert_eq!(v.str_var("Matrix_Terminus"), sign);
        assert_eq!(people.player_bus_view(&v).terminus, passenger_target);

    }

    #[test]
    fn cabin_temperature_initialization_preserves_humidity_and_then_evolves() {
        let mut v = VehicleInstance::new(
            crate::vehicle::tests::coupling_test_type(None),
            VehicleHost::new(Default::default()),
        );
        v.host.temperature = -10.0;
        v.update_engine_vars(1.0);
        v.set_engine_ev(Ev::CabinAbsHum, 4.0);
        v.set_cabin_temperature(22.0);
        assert_eq!(v.ev(Ev::CabinTemp), Some(22.0));
        assert_eq!(v.ev(Ev::CabinAbsHum), Some(4.0));
        assert_eq!(v.ev(Ev::CabinRelHum), Some(relative_humidity(22.0, 4.0)));
        v.update_engine_vars(1.0);
        let t = v.ev(Ev::CabinTemp).unwrap();
        assert!(
            t < 22.0 && t > 21.0,
            "normal heat loss resumes from the chosen temperature: {t}"
        );
    }

    #[test]
    fn scripted_cabin_temperature_starts_once_and_keeps_simulating() {
        let mut v = crate::timetable_run::tests::script_test_vehicle(
            "{init}\n-5 (S.L.Cabinair_Temp)\n4 (S.L.Cabinair_absHum)\n{end}\n{frame}\n(L.L.Cabinair_Temp) (L.S.Timegap) - (S.L.Cabinair_Temp)\n{end}\n",
            "Cabinair_Temp\nCabinair_absHum\nCabinair_relHum\n",
            "",
        );
        v.set_cabin_temperature(22.0);
        assert_eq!(v.var("Cabinair_Temp"), Some(22.0));
        v.update(0.5);
        assert_eq!(v.var("Cabinair_Temp"), Some(21.5));
        assert_eq!(v.var("Cabinair_absHum"), Some(4.0));
    }

    #[test]
    fn cabin_temperature_updates_independently_scripted_sections() {
        let mut v = VehicleInstance::new(
            crate::vehicle::tests::coupling_test_type(None),
            VehicleHost::new(Default::default()),
        );
        let part = crate::timetable_run::tests::script_test_vehicle(
            "{init}\n-5 (S.L.Cabinair_Temp)\n6 (S.L.Cabinair_absHum)\n{end}\n{frame}\n(L.L.Cabinair_Temp) (L.S.Timegap) - (S.L.Cabinair_Temp)\n{end}\n",
            "Cabinair_Temp\nCabinair_absHum\nCabinair_relHum\n",
            "",
        );
        v.attach_trailer(part.ty.clone());
        let run = crate::vehicle::car_run::CarRun::start(&part.ty, &mut v, [1.0, 0.0, 0.0]);
        v.trailers[0].run = Some(Box::new(run));
        v.set_cabin_temperature(22.0);
        let run = v.trailers[0].run.as_ref().unwrap();
        let at = |name: &str| run.state.vars[run.names[name] as usize];
        assert_eq!(at("cabinair_temp"), 22.0);
        assert_eq!(at("cabinair_abshum"), 6.0);
        assert_eq!(at("cabinair_relhum"), relative_humidity(22.0, 6.0));
    }

    /// Read and written by id as by name, also for a variable the vehicle gains on the way.
    #[test]
    fn engine_vars_answer_as_by_name() {
        let ty = crate::vehicle::tests::coupling_test_type(None);
        let mut v = VehicleInstance::new(ty, VehicleHost::new(Default::default()));
        let all = [Ev::DirtNorm, Ev::StreetCond, Ev::CabinTemp, Ev::Ai, Ev::AiAtStation, Ev::PriorityWarning];
        for ev in all {
            assert_eq!(v.ev(ev), v.var(NAMES[ev as usize]));
        }
        // (the vehicle starts with the road's and the dirt's, not with the cabin air)
        assert!(!v.set_ev(Ev::CabinTemp, 2.0));
        v.set_engine_ev(Ev::CabinTemp, 2.0);
        v.set_engine_ev(Ev::DirtNorm, 0.25);
        assert_eq!(v.var("Cabinair_Temp"), Some(2.0));
        assert!(v.set_ev(Ev::DirtNorm, 0.5));
        for ev in all {
            assert_eq!(v.ev(ev), v.var(NAMES[ev as usize]), "{ev:?}");
        }
        assert_eq!(v.ev(Ev::DirtNorm), Some(0.5));
    }
}
