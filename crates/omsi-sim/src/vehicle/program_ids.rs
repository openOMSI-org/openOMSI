//! Variables of the vehicle's own scripts the engine reads or writes every frame by name
//! (the driver's seat, the text displays' refresh, the cabin air the scripts may keep
//! themselves, the radio's strings): looked up once per program, not as a lower-cased
//! copy and a hash look-up each per vehicle per frame. They are the program's - what
//! `program.var` / `program.str_var` answer - so they change only with the type
//! (`reload::retyped` makes them again).

use super::*;
use omsi_script::{StrVarId, VarId};

/// The vehicle script's inputs to the window condensation model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScriptedCabinAir {
    /// Cabin air temperature (°C).
    pub temperature: f32,
    /// Absolute humidity (g/m³), when the script supplies a usable value.
    pub humidity: Option<f32>,
    /// Outlet temperature (°C) while the front blower is feeding the glass.
    pub front_air: Option<f32>,
}

#[derive(Clone, Copy)]
pub(crate) struct ProgramIds {
    /// `Refresh_Strings`, cleared before the text textures are drawn.
    pub(super) refresh_strings: Option<VarId>,
    /// `Driver_Seat_VertTransl` (see `update_driver_seat`).
    pub(super) driver_seat: Option<VarId>,
    /// The scripts write `Cabinair_Temp` / `Cabinair_relHum` themselves (see
    /// `update_engine_vars`).
    pub(super) owns_cabin_temp: bool,
    pub(super) owns_cabin_rel_hum: bool,
    /// `fanheater_t_target` and `cabinair_Vrate_fanheater` (see `scripted_cabin_air`).
    pub(super) fan_jet_target: Option<VarId>,
    pub(super) fan_heater_rate: Option<VarId>,
    /// The NLC heater computes its outlet temperature inline from coolant and effectiveness.
    nlc_heater: Option<(VarId, f32)>,
    /// The radio's strings (see `show_radio_text`).
    pub(super) snd_radio_text: Option<StrVarId>,
    pub(super) magnitola: Option<StrVarId>,
    pub(super) mp3_freq: Option<StrVarId>,
    pub(super) mp3_track: Option<StrVarId>,
}

impl ProgramIds {
    fn of(p: &Program) -> ProgramIds {
        let owns = |name: &str| p.var(name).is_some_and(|v| p.stores(v));
        let fan_jet_target = p.var("fanheater_t_target");
        let nlc_heater = p
            .var("engine_temperature")
            .zip(p.constant("heat_exchanger_effectivity"))
            .filter(|&(_, effectiveness)| {
                fan_jet_target.is_none()
                    && owns("Podestluefter_RPM")
                    && owns("cabinair_Qrate_engine_fanheatcooling")
                    && (0.0..=1.0).contains(&effectiveness)
            });
        ProgramIds {
            refresh_strings: p.var("Refresh_Strings"),
            driver_seat: p.var("Driver_Seat_VertTransl"),
            owns_cabin_temp: owns("Cabinair_Temp"),
            owns_cabin_rel_hum: owns("Cabinair_relHum"),
            fan_jet_target,
            fan_heater_rate: p.var("cabinair_Vrate_fanheater"),
            nlc_heater,
            snd_radio_text: p.str_var("Snd_Radio_Text"),
            magnitola: p.str_var("magnitola_1"),
            mp3_freq: p.str_var("mp3_freq"),
            mp3_track: p.str_var("mp3_display_track_name"),
        }
    }
}

impl VehicleInstance {
    /// The bus's own cabin air when its scripts keep it (`heizung.osc` and its descendants).
    pub fn scripted_cabin_air(&self) -> Option<ScriptedCabinAir> {
        let ids = self.program_ids();
        if !ids.owns_cabin_temp {
            return None;
        }
        let temperature = self
            .ev(Ev::CabinTemp)
            .filter(|t| t.is_finite() && *t > -100.0)?;
        // The NLC script adds moisture in proportion to blower speed, even with nobody
        // aboard. Keep its temperature and controls and use the condensation model's
        // moisture balance for the glass.
        let humidity = if ids.nlc_heater.is_some() {
            None
        } else {
            self.ev(Ev::CabinAbsHum)
                .filter(|h| h.is_finite() && *h >= 0.0)
        };
        let at = |id: Option<VarId>| id.map(|i| self.state.vars[i as usize]);
        let front_air = at(ids.fan_heater_rate)
            .filter(|f| f.is_finite() && *f > 1e-3)
            .and_then(|_| {
                at(ids.fan_jet_target).or_else(|| {
                    let (coolant, effectiveness) = ids.nlc_heater?;
                    Some(self.state.vars[coolant as usize] * effectiveness)
                })
            })
            .filter(|t| t.is_finite());
        Some(ScriptedCabinAir {
            temperature,
            humidity,
            front_air,
        })
    }

    pub(super) fn program_ids(&self) -> &ProgramIds {
        self.program_ids
            .get_or_init(|| ProgramIds::of(&self.ty.program))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nlc_cabin_uses_simulated_moisture_and_its_front_blower() {
        let dir = std::env::temp_dir().join(format!("omsi-nlc-heater-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("heater.osc");
        let constants = dir.join("constants.txt");
        std::fs::write(
            &script,
            "\
{init}
18 (S.L.Cabinair_Temp)
20 (S.L.Cabinair_absHum)
80 (S.L.engine_temperature)
100 (S.L.Podestluefter_RPM)
1 (S.L.cabinair_Qrate_engine_fanheatcooling)
0.02 (S.L.cabinair_Vrate_fanheater)
{end}
",
        )
        .unwrap();
        std::fs::write(&constants, "[const]\nheat_exchanger_effectivity\n0.5\n").unwrap();
        let program = compile(&CompileInput {
            scripts: vec![script],
            constfiles: vec![constants],
            declare_unknown: true,
            ..Default::default()
        });
        std::fs::remove_dir_all(dir).unwrap();
        assert!(program.errors.is_empty(), "{:?}", program.errors);
        let mut ty = crate::vehicle::tests::coupling_test_type(None);
        Arc::get_mut(&mut ty).unwrap().program = Arc::new(program);
        let mut v = VehicleInstance::new(ty, VehicleHost::new(Default::default()));
        assert_eq!(
            v.scripted_cabin_air(),
            Some(ScriptedCabinAir {
                temperature: 18.0,
                humidity: None,
                front_air: Some(40.0),
            })
        );
        assert_eq!(v.var("Cabinair_absHum"), Some(20.0));
        assert!(v.set_var("cabinair_Vrate_fanheater", 0.0));
        assert_eq!(v.scripted_cabin_air().unwrap().front_air, None);
    }

    /// The ids are what the names give, for a type that has the variables and one that has not.
    #[test]
    fn program_ids_answer_as_the_names() {
        let ty = crate::vehicle::tests::coupling_test_type(None);
        let v = VehicleInstance::new(ty, VehicleHost::new(Default::default()));
        let p = &v.ty.program;
        let ids = v.program_ids();
        assert_eq!(ids.refresh_strings, p.var("Refresh_Strings"));
        assert_eq!(ids.driver_seat, p.var("Driver_Seat_VertTransl"));
        assert_eq!(
            ids.owns_cabin_temp,
            p.var("Cabinair_Temp").is_some_and(|i| p.stores(i))
        );
        assert_eq!(
            ids.owns_cabin_rel_hum,
            p.var("Cabinair_relHum").is_some_and(|i| p.stores(i))
        );
        assert_eq!(ids.snd_radio_text, p.str_var("Snd_Radio_Text"));
        assert_eq!(ids.magnitola, p.str_var("magnitola_1"));
        assert_eq!(ids.mp3_freq, p.str_var("mp3_freq"));
        assert_eq!(ids.mp3_track, p.str_var("mp3_display_track_name"));
    }
}
