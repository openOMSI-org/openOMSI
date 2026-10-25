//! Runs without a window: exporting a bus (`--export-glb`) and the service commands.

use super::*;

/// The weather a bus is made in for the launcher's previews (the showroom and
/// `--export-glb`): 22 °C, and the absolute humidity of half-saturated air at that
/// temperature (g/m³).
const PREVIEW_TEMPERATURE: f32 = 22.0;
const PREVIEW_ABS_HUMIDITY: f32 = 9.7;

/// Make the next vehicles on a mild day. A preview has no map and no weather: made without
/// one a bus stood at 0 °C, and the heating scripts that read `Weather_Temperature` in
/// {init} set it up for a cold day - the Kajosoft Citaros and the 628 fog every window up
/// below 10 °C (`para_*` in cockpit.osc), so the launcher showed them misted over.
pub(crate) fn preview_weather() {
    omsi_sim::host::set_ambient_weather(PREVIEW_TEMPERATURE, PREVIEW_ABS_HUMIDITY);
}

/// `--export-glb`: the vehicle alone, no map, written as glTF.
pub(crate) fn run_export(args: &Args, out: &PathBuf) -> Result<()> {
    let bus = args
        .bus
        .clone()
        .ok_or_else(|| anyhow!("--export-glb needs --bus"))?;
    let path = player_bus_path(&args.root, &bus)?;
    let vt = Arc::new(omsi_sim::VehicleType::load(&args.root, &path)?);
    let scheme = paint_scheme(&vt, args.paint.as_deref());
    preview_weather();
    let mut host = omsi_sim::VehicleHost::new(start_clock(args));
    host.paint_scheme = Some(scheme);
    let mut vehicle = omsi_sim::VehicleInstance::new(vt.clone(), host);
    vehicle.apply_paint_vars(scheme);
    // the coupled rear of an articulated bus
    load_coupled_parts(&args.root, &mut vehicle);
    for _ in 0..3 {
        vehicle.update(1.0 / 30.0);
    }
    export::export_glb(&args.root, &vt, &vehicle, scheme, out)
}

/// Does the vehicle's box overlap one of the loaded `[petrolstation]` or `[depot]` objects?
/// That is OMSI's test for the pump, the wash and a repair without travel time.
pub(crate) fn at_petrol_station(world: &World, v: &omsi_sim::VehicleInstance) -> bool {
    let f = crate::lan::footprint_of(v, [2.5, 11.5, 3.0, 0.0, 0.0, 1.5]);
    let me = omsi_sim::collision::Obb {
        center: glam::DVec2::new(f.x, f.y),
        half: glam::DVec2::new(f.width as f64 * 0.5, f.length as f64 * 0.5),
        heading: (f.heading as f64).to_radians(),
        z0: f.z - 3.0,
        z1: f.z + 4.0,
        velocity: glam::DVec2::ZERO,
        mass: 0.0,
        pole: None,
        id: -1,
    };
    let data = world.data.peek();
    let stations = &data.lists.petrol_stations;
    if omsi_cfg::flags::OMSI_DEBUG_SERVICES.is_set() {
        for p in stations.iter() {
            log::info!("petrol station box at ({:.1}, {:.1}) {:.1} x {:.1} m: bus {:.1} m away", p.center.x, p.center.y, p.half.x * 2.0, p.half.y * 2.0, me.separation(p));
        }
    }
    stations.iter().any(|p| me.separation(p) < 0.0)
}

/// The depot services: the fuel pump, the bus wash and the workshop. OMSI offers them
/// from its menu, fires `veh_tank` / `veh_wash` while they run and asks the bus for its
/// repair time with `malfunction_gettime` before it lets the workshop start. The pump and
/// the wash work only at a petrol station (`at_station`); the workshop comes anywhere, and
/// away from one its team needs the map's `[repair_time_min]` to get there
/// (`DG_Repair3`: "Since you are not in the depot the reparation team needs … minutes").
pub(crate) fn run_services(
    args: &Args,
    v: &mut omsi_sim::VehicleInstance,
    clock: &mut omsi_sim::SimClock,
    repair_time_min: f32,
    at_station: bool,
) -> Vec<String> {
    let mut out = Vec::new();
    if (args.refuel || args.wash) && !at_station {
        out.push("Refuel and wash only at a petrol station or in the depot's wash yard".into());
    } else {
        if args.refuel {
            match v.refuel() {
                Some(l) => out.push(format!("refuelled: {l:.0} l in the tank")),
                None => out.push("this vehicle has no fuel pump handling (veh_tank)".into()),
            }
        }
        if args.wash {
            match v.wash() {
                Some(d) => out.push(format!("washed: dirt {:.0}%", d * 100.0)),
                None => out.push("this vehicle has no bus wash handling (veh_wash)".into()),
            }
        }
    }
    if args.repair {
        // (Omsi.exe, 0x70dc7c: no time, or none in 0 .. 100000 minutes - a bus without
        // `malfunction_gettime`, most of them - is repaired at once, with no team to wait
        // for; `malfunction_reset` runs either way, #1048)
        match v.repair_minutes().filter(|m| *m > 0.0 && *m < 100000.0) {
            Some(mins) => {
                let travel = if at_station { 0.0 } else { repair_time_min };
                clock.time += ((mins + travel) * 60.0) as f64;
                v.repair();
                out.push(if at_station {
                    format!("repaired: {mins:.0} min of work")
                } else {
                    format!("repaired: {mins:.0} min of work + {travel:.0} min for the team to get here")
                });
            }
            None if v.repair() => out.push("repaired".into()),
            None => out.push("this vehicle has no repair handling (malfunction_reset)".into()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vehicle of `script` alone, with the variables `vars`.
    fn scripted(script: &str, vars: &str) -> omsi_sim::VehicleInstance {
        let dir = std::env::temp_dir().join(format!("omsi_services_{}_{}", std::process::id(), vars.len()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.osc"), script).unwrap();
        std::fs::write(dir.join("vars.txt"), vars).unwrap();
        let program = omsi_script::compile(&omsi_script::CompileInput {
            scripts: vec![dir.join("main.osc")],
            varlists: vec![dir.join("vars.txt")],
            ..Default::default()
        });
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(program.errors.is_empty(), "{:?}", program.errors);
        let ty = Arc::new(omsi_sim::VehicleType {
            def: Default::default(),
            model: Default::default(),
            model_dir: PathBuf::new(),
            program: Arc::new(program),
            meshes: Vec::new(),
            lod_meshes: Vec::new(),
            paint_schemes: Vec::new(),
            texchanges: Vec::new(),
            wheel_meshes: Vec::new(),
            suspension_axles: Vec::new(),
            missing_packs: Vec::new(),
            mesh_bounds: Vec::new(),
            mesh_boxes: Vec::new(),
        });
        omsi_sim::VehicleInstance::new(ty, omsi_sim::VehicleHost::new(Default::default()))
    }

    #[test]
    fn a_bus_without_a_repair_time_is_repaired_at_once() {
        // (the NEOMAN Overhaul: a `malfunction_reset` and no `malfunction_gettime`)
        let mut v = scripted("{trigger:malfunction_reset}\n0 (S.L.Fail_Gelenk_true)\n{end}\n", "Fail_Gelenk_true\n");
        v.set_var("Fail_Gelenk_true", 1.0);
        let args = Args { repair: true, ..Args::parse_from(["openomsi"]) };
        let mut clock = omsi_sim::SimClock::default();
        let msg = run_services(&args, &mut v, &mut clock, 30.0, false);
        assert_eq!(msg, vec!["repaired".to_string()]);
        assert_eq!(v.var("Fail_Gelenk_true"), Some(0.0));
        assert_eq!(clock.time, omsi_sim::SimClock::default().time, "no team to wait for");
    }

    #[test]
    fn a_repair_time_is_waited_for_and_the_team_away_from_the_depot() {
        let mut v = scripted(
            "{trigger:malfunction_gettime}\n45\n{end}\n{trigger:malfunction_reset}\n0 (S.L.broken)\n{end}\n",
            "broken\n",
        );
        v.set_var("broken", 1.0);
        let args = Args { repair: true, ..Args::parse_from(["openomsi"]) };
        let mut clock = omsi_sim::SimClock::default();
        let was = clock.time;
        let msg = run_services(&args, &mut v, &mut clock, 30.0, false);
        assert_eq!(v.var("broken"), Some(0.0));
        assert_eq!(clock.time - was, (45.0 + 30.0) * 60.0, "{msg:?}");
    }
}
