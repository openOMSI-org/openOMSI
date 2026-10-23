//! The shared registry's HOF functions through the actual Lua binding.
mod common;
use common::{Game, run};
use omsi_plugin::api::json;

#[test]
fn loaded_hof_snapshots_and_identifier_lookup() {
    let mut game = Game::new();
    game.hof = Some(json::decode(r#"{
      "path":"Vehicles/Bus/Depot.hof", "name":"Délpest", "service_trip":"Empty",
      "global_strings":["","Őrs"], "string_count_terminus":10,"string_count_busstop":2,
      "termini":[{"index":0,"code":123,"identifier":"[184] End (1)","all_exit":false,
       "terminus_stop":"Stop ID","strings":["Display","","","","","","","","NORMÁL","1"]}],
      "bus_stops":[{"index":0,"identifier":"Stop ID","strings":["Stop display",""]}],
      "info_trips":[{"index":0,"code":"018401","name":"Outward","route":"1","line":"184","extra":["extra"]}],
      "info_busstop_lists":[["Stop ID"]],"info_busstops":[["Stop ID","field",""]]
    }"#, true).unwrap());
    run(
        "hof-loaded",
        r#"
      function on_frame()
        assert(omsi.game.has('hof.get') and omsi.game.has('hof.terminus') and omsi.game.has('hof.busstop'))
        local h=omsi.hof.get()
        assert(h.name=='Délpest' and h.global_strings[1]=='' and h.global_strings[2]=='Őrs')
        assert(h.string_count_terminus==10 and h.string_count_busstop==2)
        local t=omsi.hof.terminus('[184] End (1)')
        assert(t.index==0 and t.code==123 and t.identifier=='[184] End (1)' and not t.all_exit)
        assert(t.terminus_stop=='Stop ID' and t.strings[9]=='NORMÁL' and t.strings[10]=='1')
        assert(omsi.hof.terminus('[184] end (1)')==nil and omsi.hof.terminus('missing')==nil)
        assert(omsi.hof.busstop('Stop ID').strings[1]=='Stop display')
        assert(omsi.hof.busstop('stop id')==nil)
        assert(h.info_trips[1].extra[1]=='extra' and h.info_busstop_lists[1][1]=='Stop ID')
        assert(h.info_busstops[1][3]=='')
        t.strings[9]='changed'; h.termini[1].identifier='changed'
        assert(omsi.hof.terminus('[184] End (1)').strings[9]=='NORMÁL')
        assert(omsi.hof.get().termini[1].identifier=='[184] End (1)')
        assert(not pcall(omsi.hof.terminus) and not pcall(omsi.hof.busstop, {}))
        omsi.message('HOF passed')
      end
    "#,
        &mut game,
        1,
    );
    assert_eq!(game.messages, ["HOF passed"]);
    assert!(
        game.log.borrow().is_empty(),
        "read-only calls must not mutate the fake game"
    );
}

#[test]
fn absent_vehicle_or_hof_returns_nil() {
    for vehicle in [false, true] {
        let mut game = Game::new();
        game.vehicle = vehicle;
        run(
            if vehicle { "hof-missing" } else { "hof-no-bus" },
            r#"
          function on_frame()
            assert(omsi.hof.get()==nil)
            assert(omsi.hof.terminus('End')==nil and omsi.hof.busstop('Stop')==nil)
            omsi.message('No HOF')
          end
        "#,
            &mut game,
            1,
        );
        assert_eq!(game.messages, ["No HOF"]);
    }
}

#[test]
fn wasm_uses_the_same_read_only_hof_api() {
    use omsi_plugin::{HostConfig, Plugins};
    use oop_format::{Header, Kind, OopBuilder};
    let guest = r#"(module
      (import "openomsi" "call" (func $call (param i32 i32 i32 i32) (result i64)))
      (memory (export "memory") 1)
      (global $next (mut i32) (i32.const 4096))
      (data (i32.const 0) "hof.get")
      (data (i32.const 16) "hof.terminus")
      (data (i32.const 32) "hof.busstop")
      (data (i32.const 48) "[]")
      (data (i32.const 64) "[\"End\"]")
      (data (i32.const 80) "[\"Stop\"]")
      (data (i32.const 96) "on")
      (data (i32.const 112) "[\"frame\",{\"$cb\":1}]")
      (data (i32.const 144) "message")
      (data (i32.const 160) "[\"WASM HOF passed\"]")
      (func (export "oop_abi") (result i32) (i32.const 1))
      (func (export "oop_alloc") (param $len i32) (result i32)
        (local $p i32)
        (local.set $p (global.get $next))
        (global.set $next (i32.add (global.get $next) (local.get $len)))
        (local.get $p))
      (func (export "oop_free") (param i32 i32))
      (func (export "oop_start")
        (drop (call $call (i32.const 96) (i32.const 2) (i32.const 112) (i32.const 19))))
      (func $object (param $name i32) (param $len i32) (param $args i32) (param $argc i32)
        (local $r i64)
        (local.set $r (call $call (local.get $name) (local.get $len) (local.get $args) (local.get $argc)))
        (if (i64.lt_s (local.get $r) (i64.const 0)) (then unreachable))
        (if (i32.ne (i32.load8_u (i32.wrap_i64 (i64.shr_u (local.get $r) (i64.const 32)))) (i32.const 123))
          (then unreachable)))
      (func (export "oop_callback") (param i64 i32 i32)
        (call $object (i32.const 0) (i32.const 7) (i32.const 48) (i32.const 2))
        (call $object (i32.const 16) (i32.const 12) (i32.const 64) (i32.const 7))
        (call $object (i32.const 32) (i32.const 11) (i32.const 80) (i32.const 8))
        (drop (call $call (i32.const 144) (i32.const 7) (i32.const 160) (i32.const 19)))))"#;
    let mut header = Header::new("org.test.hof", "HOF", "1.0.0", Kind::Wasm, "plugin.wasm");
    header.permissions = vec!["ui".into()];
    let package = OopBuilder::new(header)
        .file("plugin.wasm", guest)
        .write()
        .unwrap();
    let dir = common::dir("hof-wasm");
    std::fs::write(dir.join("hof.oop"), package).unwrap();
    let mut plugins = Plugins::load(&[dir], &HostConfig::default());
    assert_eq!(plugins.wasm.len(), 1);
    let mut game = Game::new();
    game.hof = Some(json::decode(r#"{"termini":[{"index":0,"identifier":"End"}],"bus_stops":[{"index":0,"identifier":"Stop"}]}"#, true).unwrap());
    plugins.frame(&mut game);
    assert!(!plugins.wasm[0].disabled);
    assert_eq!(game.messages, ["WASM HOF passed"]);
}
