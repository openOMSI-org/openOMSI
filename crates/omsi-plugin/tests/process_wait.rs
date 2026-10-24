mod common;
use common::{dir, Game};
use omsi_plugin::{
    api::{json, Value},
    HostConfig, Plugins,
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[test]
#[ignore = "subprocess helper, exercised by the process wait tests"]
fn process_wait_child() {
    let gate = std::env::var_os("PLUGIN_WAIT_TEST_GATE").expect("helper gate");
    std::fs::write(Path::new(&gate).with_extension("pid"), std::process::id().to_string()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !Path::new(&gate).exists() {
        if Instant::now() > deadline {
            std::process::exit(99);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    std::process::exit(7);
}

fn game() -> Game {
    let mut g = Game::new();
    for name in [
        "before_wait",
        "after_wait",
        "following",
        "other_frames",
        "child_pid",
        "stopped",
    ] {
        g.vars.insert(name.into(), 0.0);
    }
    g
}

fn child(gate: &Path) -> String {
    let quote = |s: &str| json::encode(&Value::Str(s.into()));
    format!(
        r#"assert(omsi.process.start({}, {{"--exact", "process_wait_child", "--ignored"}}, {{env = {{PLUGIN_WAIT_TEST_GATE = {}}}}}))"#,
        quote(if cfg!(windows) { "helper.exe" } else { "helper" }),
        quote(gate.to_str().unwrap())
    )
}

fn drive_until(p: &mut Plugins, g: &mut Game, done: impl Fn(&Game) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(g) && Instant::now() < deadline {
        p.frame(g);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(done(g), "did not finish: {:?}", g.messages);
}

#[test]
fn main_chunk_waits_without_blocking_game_or_other_plugins() {
    let d = dir("wait-main");
    let gate = dir("wait-main-gate").join("gate");
    let script = format!(
        r#"
        local handle = {}
        local count, state = omsi.process.wait(handle)
        assert(count == 7 and state == "exited")
        assert(omsi.process.exit_code(handle) == 7)
        omsi.process.forget(handle)
        omsi.set_var("after_wait", count)
        function on_start() omsi.message("started after wait") end
    "#,
        child(&gate)
    );
    std::fs::write(d.join("a_wait.lua"), script).unwrap();
    std::fs::write(
        d.join("b_other.lua"),
        "function on_frame() omsi.set_var('other_frames', (omsi.var('other_frames') or 0) + 1) end",
    )
    .unwrap();
    let mut p = load(&[d]);
    let mut g = game();
    for _ in 0..8 {
        let at = Instant::now();
        p.frame(&mut g);
        assert!(
            at.elapsed() < Duration::from_millis(250),
            "game frame blocked on child"
        );
        assert!(g.vars["after_wait"] == 0.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(g.vars["other_frames"], 8.0);
    std::fs::write(gate, "").unwrap();
    drive_until(&mut p, &mut g, |g| g.vars["after_wait"] != 0.0);
    assert_eq!(g.vars["after_wait"], 7.0);
    assert_eq!(g.messages, ["started after wait"]);
    p.finalize();
}

#[test]
fn wait_in_handler_preserves_locals_and_defers_following_callbacks() {
    let d = dir("wait-handler");
    let gate = dir("wait-handler-gate").join("gate");
    std::fs::write(
        d.join("wait.lua"),
        format!(
            r#"
        local once = false
        omsi.on("frame", function()
            if once then return end
            once = true
            local before = 35
            local handle = {}
            omsi.set_var("before_wait", 1)
            local count = omsi.process.wait(handle)
            omsi.set_var("after_wait", before + count)
        end)
        omsi.on("frame", function() omsi.set_var("following", (omsi.var("following") or 0) + 1) end)
    "#,
            child(&gate)
        ),
    )
    .unwrap();
    let mut p = load(&[d]);
    let mut g = game();
    p.frame(&mut g);
    assert_eq!(g.vars["before_wait"], 1.0);
    for _ in 0..3 {
        p.frame(&mut g);
    }
    assert!(g.vars["after_wait"] == 0.0 && g.vars["following"] == 0.0);
    std::fs::write(gate, "").unwrap();
    drive_until(&mut p, &mut g, |g| g.vars["after_wait"] != 0.0);
    assert_eq!(g.vars["after_wait"], 42.0);
    assert_eq!(g.vars["following"], 1.0);
    p.finalize();
}

#[test]
fn resumed_lua_keeps_its_instruction_budget() {
    let d = dir("wait-budget");
    let gate = dir("wait-budget-gate").join("gate");
    std::fs::write(
        d.join("wait.lua"),
        format!(
            r#"
        function on_start()
            omsi.process.wait({})
            while true do end
        end
    "#,
            child(&gate)
        ),
    )
    .unwrap();
    let mut p = load(&[d]);
    let mut g = game();
    p.frame(&mut g);
    assert!(!p.lua[0].disabled);
    std::fs::write(gate, "").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !p.lua[0].disabled && Instant::now() < deadline {
        p.frame(&mut g);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(p.lua[0].disabled, "resumed runaway escaped the budget");
    assert!(
        g.messages.iter().any(|s| s.contains("longer than 50 ms")),
        "{:?}",
        g.messages
    );
    p.finalize();
}

#[test]
fn timer_can_wait_and_be_cancelled_by_unloading() {
    let d = dir("wait-unload");
    let gate = dir("wait-unload-gate").join("gate");
    std::fs::write(
        d.join("wait.lua"),
        format!(
            r#"
        omsi.after(0.01, function()
            local handle = {}
            omsi.set_var("before_wait", 1)
            omsi.process.wait(handle)
            omsi.set_var("after_wait", 1)
        end)
        function on_stop() omsi.set_var("stopped", 1) end
    "#,
            child(&gate)
        ),
    )
    .unwrap();
    let mut p = load(&[d]);
    let mut g = game();
    let pid_file = gate.with_extension("pid");
    drive_until(&mut p, &mut g, |_| pid_file.exists());
    let pid: u32 = std::fs::read_to_string(pid_file).unwrap().parse().unwrap();
    p.lua[0].stop(&mut g);
    assert_eq!(g.vars["stopped"], 1.0);
    p.finalize();
    assert!(g.vars["after_wait"] == 0.0);
    #[cfg(target_os = "linux")]
    {
        let proc = std::path::PathBuf::from(format!("/proc/{pid}"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while proc.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!proc.exists(), "waiting child survived plugin unload");
    }
    #[cfg(not(target_os = "linux"))]
    let _ = pid;
}

#[test]
fn a_resumed_handler_can_wait_for_another_child() {
    let d = dir("wait-twice");
    let gates = dir("process-two-gates");
    let first = gates.join("first");
    let second = gates.join("second");
    std::fs::write(
        d.join("wait.lua"),
        format!(
            r#"
        local once = false
        function on_frame()
            if once then return end
            once = true
            local a = omsi.process.wait({})
            omsi.set_var("before_wait", a)
            local b = omsi.process.wait({})
            omsi.set_var("after_wait", a + b)
        end
    "#,
            child(&first),
            child(&second)
        ),
    )
    .unwrap();
    let mut p = load(&[d]);
    let mut g = game();
    p.frame(&mut g);
    assert_eq!(g.vars["before_wait"], 0.0);
    std::fs::write(first, "").unwrap();
    drive_until(&mut p, &mut g, |g| g.vars["before_wait"] == 7.0);
    for _ in 0..3 {
        p.frame(&mut g);
    }
    assert_eq!(g.vars["after_wait"], 0.0);
    std::fs::write(second, "").unwrap();
    drive_until(&mut p, &mut g, |g| g.vars["after_wait"] == 14.0);
    p.finalize();
}

fn load(dirs: &[PathBuf]) -> Plugins {
    use omsi_plugin::api::process::{ProcessGrant, ProcessGrants};
    let helper = if cfg!(windows) { "helper.exe" } else { "helper" };
    for d in dirs {
        std::fs::copy(std::env::current_exe().unwrap(), d.join(helper)).unwrap();
    }
    let grants = dirs.iter().flat_map(|d| omsi_plugin::lua::find_lua(d))
        .map(|entry| ProcessGrant::capture(&entry, entry.parent().unwrap(), &[helper.into()]).unwrap()).collect();
    Plugins::load_with_process_grants(
        dirs,
        &HostConfig::default(),
        ProcessGrants { grants },
    )
}

#[test]
fn plain_lua_cannot_start_a_process_without_player_opt_in() {
    let d = dir("process-default-denied");
    std::fs::write(
        d.join("denied.lua"),
        r#"
        function on_frame()
            local ok, err = pcall(omsi.process.start, "nonexistent-program")
            assert(not ok and tostring(err):find("OMSI_PLUGIN_PROCESS_ALLOW", 1, true))
            assert(omsi._managed_call == nil)
            omsi.message("denied")
        end
    "#,
    )
    .unwrap();
    let mut plugins =
        Plugins::load_with_process_grants(&[d], &HostConfig::default(), Default::default());
    let mut g = game();
    plugins.frame(&mut g);
    assert_eq!(g.messages, ["denied"]);
    plugins.finalize();
}

#[test]
#[ignore = "subprocess budget watchdog"]
fn nested_emit_budget_child() {
    let d = dir("nested-process-budget");
    std::fs::write(
        d.join("nested.lua"),
        r#"
        omsi.on("nested", function() end)
        function on_frame()
            omsi.emit("nested")
            assert(coroutine.resume(coroutine.create(function() return 1 end)))
            assert(coroutine.wrap(function() return 2 end)() == 2)
            while true do end
        end
    "#,
    )
    .unwrap();
    let mut plugins = load(&[d]);
    let mut g = game();
    plugins.frame(&mut g);
    assert!(plugins.lua[0].disabled);
    assert!(g.messages.iter().any(|m| m.contains("longer than 50 ms")));
    plugins.finalize();
}

#[test]
fn endless_loop_after_nested_emit_is_stopped() {
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "nested_emit_budget_child",
            "--ignored",
            "--nocapture",
        ])
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("nested emit lost its execution budget");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn managed_callbacks_reuse_finished_threads_and_preserve_return_values() {
    let d = dir("process-reuse");
    std::fs::write(
        d.join("reuse.lua"),
        r#"
        local threads = {}
        omsi.on("nested", function()
            local thread = coroutine.running()
            threads[thread] = true
            return 41, nil, 43
        end)
        function on_frame()
            omsi.emit("nested")
            local a, b, c = omsi._managed_call(function() return 41, nil, 43 end)
            assert(a == 41 and b == nil and c == 43)
            local count = 0
            for _ in pairs(threads) do count = count + 1 end
            assert(count == 1, "finished callbacks must reuse their coroutine")
            omsi.message("reused")
        end
    "#,
    )
    .unwrap();
    let mut p = load(&[d]);
    let mut g = game();
    for _ in 0..5 {
        p.frame(&mut g);
    }
    assert_eq!(g.messages, ["reused"; 5]);
    p.finalize();
}

#[test]
#[ignore = "manual callback timing; run with --ignored --exact callback_cost --nocapture"]
fn callback_cost() {
    let d = dir("process-callback-cost");
    let entry = d.join("callbacks.lua");
    std::fs::write(&entry, r#"
        local n = 0
        function on_frame() n = n + 1 end
        omsi.every(0.001, function() n = n + 1 end)
        omsi.every(0.001, function() n = n + 1 end)
        omsi.every(0.001, function() n = n + 1 end)
    "#).unwrap();
    for consent in [false, true] {
        let grants = omsi_plugin::api::process::ProcessGrants::from_paths(if consent { vec![entry.clone()] } else { vec![] });
        let mut p = Plugins::load_with_process_grants(std::slice::from_ref(&d), &HostConfig::default(), grants);
        let mut g = game();
        for _ in 0..1000 { p.frame(&mut g); }
        let start = Instant::now();
        for _ in 0..100000 { p.frame(&mut g); }
        println!("process_consent={consent}: {:.3} us/frame (on_frame + 3 timers)", start.elapsed().as_secs_f64() * 10.0);
        assert!(!p.lua[0].disabled);
        p.finalize();
    }
}

#[test]
fn reload_revokes_consent_after_entry_or_module_changes() {
    use omsi_plugin::api::process::ProcessGrants;
    for module in [false, true] {
        let d = dir("process-grant-reload");
        let entry = d.join("plugin.lua");
        let source = r#"function on_frame()
            local handle = assert(omsi.process.start("helper.exe"))
            local _, state, reason = omsi.process.wait(handle)
            assert(state == "failed")
            omsi.message(reason)
        end"#;
        std::fs::write(&entry, source).unwrap();
        let module_path = d.join("module.lua");
        std::fs::write(&module_path, "return 1").unwrap();
        let grants = ProcessGrants::from_paths([entry.clone()]);
        let mut p = Plugins::load_with_process_grants(std::slice::from_ref(&d), &HostConfig::default(), grants);
        let mut g = game();
        drive_until(&mut p, &mut g, |g| g.messages.iter().any(|s| s.contains("helper is not listed")));
        if module { std::fs::write(module_path, "return 2").unwrap(); }
        std::thread::sleep(Duration::from_millis(1100));
        std::fs::write(&entry, format!("{source}\n-- reloaded")).unwrap();
        drive_until(&mut p, &mut g, |g| g.messages.iter().any(|s| s.contains("plugin content changed")));
        p.finalize();
    }
}
