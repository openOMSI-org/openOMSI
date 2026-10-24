//! The fuel pump (`veh_tank`) as a script runs it: what the "full tank" start and the petrol
//! station both fill the tank with.

use super::*;

/// A bus whose script starts it with 15 % of a 250 l tank and whose pump puts 3 l in a
/// second, up to 250 (the SD202's way); `pump`: whether it has the pump at all.
fn bus(pump: bool) -> VehicleInstance {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!("omsi_fuel_{}_{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("engine.osc");
    let trigger = if pump { "{trigger:veh_tank}\n(L.L.engine_tank_content) 3 (L.S.Timegap) * + 250 min (S.L.engine_tank_content)\n{end}\n" } else { "" };
    std::fs::write(&script, format!("{{init}}\n37.5 (S.L.engine_tank_content)\n{{end}}\n{trigger}")).unwrap();
    let vars = dir.join("vars.txt");
    std::fs::write(&vars, "engine_tank_content\n").unwrap();
    let program = omsi_script::compile(&omsi_script::CompileInput { scripts: vec![script], varlists: vec![vars], ..Default::default() });
    assert!(program.errors.is_empty(), "{:?}", program.errors);
    let ty = Arc::new(VehicleType {
        def: Default::default(),
        model: Model::default(),
        model_dir: dir.clone(),
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
    std::fs::remove_dir_all(dir).unwrap();
    VehicleInstance::new(ty, VehicleHost::new(Default::default()))
}

#[test]
fn the_pump_fills_the_tank_to_what_the_script_holds() {
    let mut v = bus(true);
    assert_eq!(v.var("engine_tank_content"), Some(37.5));
    // filled to the script's own top, and no further
    assert_eq!(v.refuel(), Some(250.0));
    assert_eq!(v.var("engine_tank_content"), Some(250.0));
    // a full tank stays full; the clock the script reads is left as it was
    let gap = v.host.clock.timegap;
    assert_eq!(v.refuel(), Some(250.0));
    assert_eq!(v.host.clock.timegap, gap);
}

#[test]
fn a_bus_without_a_pump_keeps_its_tank() {
    let mut v = bus(false);
    assert_eq!(v.refuel(), None);
    assert_eq!(v.var("engine_tank_content"), Some(37.5));
}
