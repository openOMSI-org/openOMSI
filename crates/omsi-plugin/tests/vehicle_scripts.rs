//! The scripts of a vehicle (`[vehicle_script]`): they run as plugins while the vehicle is the
//! player's, with the permissions of vehicle content, and stop when the vehicle is left.
use omsi_plugin::{PluginIo, Plugins};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Default)]
struct Bus {
    vars: HashMap<String, f32>,
    messages: Vec<String>,
}

impl PluginIo for Bus {
    fn system(&mut self, _: &str) -> Option<f32> {
        None
    }
    fn set_system(&mut self, _: &str, _: f32) {}
    fn has_vehicle(&self) -> bool {
        true
    }
    fn var(&mut self, name: &str) -> Option<f32> {
        self.vars.get(name).copied()
    }
    fn set_var(&mut self, name: &str, v: f32) {
        self.vars.insert(name.into(), v);
    }
    fn string(&mut self, _: &str) -> Option<String> {
        None
    }
    fn set_string(&mut self, _: &str, _: &str) {}
    fn fire(&mut self, _: &str, _: bool) {}
    fn dt(&self) -> f32 {
        0.5
    }
    fn message(&mut self, text: &str, _: f32) {
        self.messages.push(text.into());
    }
}

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("omsi-vehicle-script-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_vehicle_script_runs_with_the_vehicle_and_leaves_with_it() {
    let d = dir("run");
    std::fs::write(d.join("cab.lua"), "function on_frame(dt) omsi.set_var('cab_ticks', (omsi.var('cab_ticks') or 0) + 1) end").unwrap();
    std::fs::write(d.join("other.lua"), "function on_frame(dt) omsi.set_var('other_ticks', 1) end").unwrap();
    let mut plugins = Plugins::default();
    let mut bus = Bus::default();
    bus.vars.insert("cab_ticks".into(), 0.0);
    bus.vars.insert("other_ticks".into(), 0.0);
    assert_eq!(plugins.attach_vehicle_scripts(&[d.join("cab.lua"), d.join("missing.lua"), d.join("x.txt")]), 1, "the missing file and the wrong kind are left out");
    assert!(plugins.has("vehicle:cab"));
    plugins.frame(&mut bus);
    plugins.frame(&mut bus);
    assert_eq!(bus.vars["cab_ticks"], 2.0);
    // another vehicle: the first one's script stops, the new one starts
    assert_eq!(plugins.attach_vehicle_scripts(&[d.join("other.lua")]), 1);
    assert!(!plugins.has("vehicle:cab") && plugins.has("vehicle:other"));
    plugins.frame(&mut bus);
    assert_eq!((bus.vars["cab_ticks"], bus.vars["other_ticks"]), (2.0, 1.0));
    plugins.detach_vehicle_scripts();
    assert!(plugins.is_empty());
}

#[test]
fn a_vehicle_script_cannot_reach_beyond_the_vehicle() {
    let d = dir("perm");
    // (the world's clock and the weather are for the player's own plugins)
    std::fs::write(d.join("greedy.lua"), "function on_frame(dt)\n  local ok = pcall(function() omsi.command('shot') end)\n  omsi.set_var('denied', ok and 0 or 1)\n  omsi.set_var('own', 1)\nend\n").unwrap();
    let mut plugins = Plugins::default();
    let mut bus = Bus::default();
    bus.vars.insert("denied".into(), 0.0);
    bus.vars.insert("own".into(), 0.0);
    assert_eq!(plugins.attach_vehicle_scripts(&[d.join("greedy.lua")]), 1);
    plugins.frame(&mut bus);
    assert_eq!(bus.vars.get("own"), Some(&1.0), "it may write its bus's variables");
    assert_eq!(bus.vars.get("denied"), Some(&1.0), "but not give the game menu commands: {:?}", bus.vars);
}
