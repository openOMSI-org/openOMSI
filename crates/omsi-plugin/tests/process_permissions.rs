use omsi_plugin::{
    HostConfig, PluginIo, Plugins,
    api::process::{
        ProcessDecision, ProcessGrant, ProcessGrants, ProcessPermissions, ProcessPrompt,
    },
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct Game {
    messages: Vec<String>,
    frames: f32,
}
impl PluginIo for Game {
    fn system(&mut self, _: &str) -> Option<f32> {
        None
    }
    fn set_system(&mut self, _: &str, _: f32) {}
    fn has_vehicle(&self) -> bool {
        true
    }
    fn var(&mut self, _: &str) -> Option<f32> {
        Some(self.frames)
    }
    fn set_var(&mut self, _: &str, v: f32) {
        self.frames = v;
    }
    fn string(&mut self, _: &str) -> Option<String> {
        None
    }
    fn set_string(&mut self, _: &str, _: &str) {}
    fn fire(&mut self, _: &str, _: bool) {}
    fn message(&mut self, s: &str, _: f32) {
        self.messages.push(s.into());
    }
}
struct Fixture {
    root: PathBuf,
    folder: PathBuf,
    marker: PathBuf,
    saved: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "omsi-consent-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let folder = root.join("plugins");
        std::fs::create_dir_all(&folder).unwrap();
        let helper = if cfg!(windows) {
            "helper.exe"
        } else {
            "helper"
        };
        std::fs::copy(std::env::current_exe().unwrap(), folder.join(helper)).unwrap();
        let fixture = Self {
            marker: root.join("child-started"),
            saved: root.join("user/permissions.json"),
            root,
            folder,
        };
        fixture.script(helper);
        std::fs::write(
            fixture.folder.join("z_other.lua"),
            "function on_frame() omsi.set_var('frames', omsi.var('frames') + 1) end",
        )
        .unwrap();
        fixture
    }
    fn script(&self, helper: &str) {
        let quote = |s: &str| serde_json::to_string(s).unwrap();
        let source = format!(
            r#"
            local handle = assert(omsi.process.start({}, {{"--exact", "permission_child", "--ignored"}}, {{env={{PLUGIN_CONSENT_TEST_MARKER={}}}}}))
            local code, state, reason = omsi.process.wait(handle)
            assert(omsi.process.forget(handle))
            omsi.message(state .. ":" .. tostring(code) .. ":" .. tostring(reason))
        "#,
            quote(helper),
            quote(self.marker.to_str().unwrap())
        );
        std::fs::write(self.folder.join("a_plugin.lua"), source).unwrap();
    }
    fn broker(&self) -> Arc<ProcessPermissions> {
        Arc::new(ProcessPermissions::new(Some(self.saved.clone())))
    }
    fn load(&self, broker: Arc<ProcessPermissions>) -> Plugins {
        Plugins::load_with_process_permissions(
            std::slice::from_ref(&self.folder),
            &HostConfig::default(),
            ProcessGrants::default(),
            broker,
        )
    }
}
fn drive(host: &mut Plugins, game: &mut Game, done: impl Fn(&Game) -> bool) {
    // CI Windows runners sometimes need extra time to start a copy of the test
    // executable as a child process (including antivirus scanning). The game
    // remains nonblocking: only this integration test's completion wait grows.
    // Keep the shorter limit on other platforms to catch actual stalls quickly.
    let timeout = if cfg!(windows) { 45 } else { 15 };
    let deadline = Instant::now() + Duration::from_secs(timeout);
    while !done(game) && Instant::now() < deadline {
        host.frame(game);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        done(game),
        "plugin process test timed out after {timeout}s (frames={}, messages={:?})",
        game.frames, game.messages
    );
}
fn prompt(host: &mut Plugins, game: &mut Game, broker: &ProcessPermissions) -> ProcessPrompt {
    drive(host, game, |_| broker.prompt().is_some());
    broker.prompt().unwrap()
}
#[test]
#[ignore = "subprocess fixture"]
fn permission_child() {
    std::fs::write(
        std::env::var_os("PLUGIN_CONSENT_TEST_MARKER").unwrap(),
        "started",
    )
    .unwrap();
    std::process::exit(42);
}
#[test]
fn once_waits_without_blocking_frames_then_prompts_again_and_deny_stays_denied() {
    let f = Fixture::new();
    let broker = f.broker();
    let mut host = f.load(broker.clone());
    let mut game = Game::default();
    let p = prompt(&mut host, &mut game, &broker);
    assert!(p.file.is_absolute());
    assert_eq!(p.plugin, "a_plugin");
    assert!(!f.marker.exists());
    let before = game.frames;
    for _ in 0..128 {
        host.frame(&mut game);
    }
    assert_eq!(game.frames, before + 128.0);
    assert!(game.messages.is_empty());
    assert!(broker.respond(p.id, ProcessDecision::AllowOnce));
    drive(&mut host, &mut game, |g| !g.messages.is_empty());
    assert_eq!(game.messages, ["exited:42:nil"]);
    assert!(f.marker.exists());
    assert!(!f.saved.exists());
    host.finalize();
    std::fs::remove_file(&f.marker).unwrap();
    let mut host = f.load(broker.clone());
    let mut game = Game::default();
    let next = prompt(&mut host, &mut game, &broker);
    assert_ne!(next.id, p.id);
    assert!(!broker.respond(p.id, ProcessDecision::AlwaysAllow));
    assert!(broker.respond(next.id, ProcessDecision::Deny));
    drive(&mut host, &mut game, |g| !g.messages.is_empty());
    assert!(game.messages[0].contains("denied by the player"));
    assert!(!f.marker.exists());
    host.finalize();
    let mut host = f.load(broker.clone());
    let mut game = Game::default();
    drive(&mut host, &mut game, |g| !g.messages.is_empty());
    assert!(game.messages[0].contains("denied"));
    assert!(broker.prompt().is_none());
    host.finalize();
    // Denial is about this entry/helper for the session, not the current hashes.
    let entry = f.folder.join("a_plugin.lua");
    let source = std::fs::read_to_string(&entry).unwrap();
    std::fs::write(&entry, format!("{source}\n-- content changed after denial\n")).unwrap();
    std::fs::write(f.folder.join("a_plugin.save.lua"), "return { counter = 2 }").unwrap();
    std::fs::write(f.folder.join(if cfg!(windows) { "helper.exe" } else { "helper" }), "changed helper").unwrap();
    let mut host = f.load(broker.clone());
    let mut game = Game::default();
    drive(&mut host, &mut game, |g| !g.messages.is_empty());
    assert!(game.messages[0].contains("denied"));
    assert!(broker.prompt().is_none());
    assert!(!f.marker.exists());
    host.finalize();
    std::fs::remove_dir_all(f.root).unwrap();
}

#[test]
fn saving_plugin_data_while_awaiting_consent_and_after_restart_preserves_approval() {
    for folder_plugin in [false, true] {
        let f = Fixture::new();
        let old_entry = f.folder.join("a_plugin.lua");
        let source = std::fs::read_to_string(&old_entry).unwrap();
        let folder = if folder_plugin {
            let folder = f.folder.join("a_plugin");
            std::fs::create_dir(&folder).unwrap();
            let helper = if cfg!(windows) { "helper.exe" } else { "helper" };
            std::fs::copy(f.folder.join(helper), folder.join(helper)).unwrap();
            std::fs::remove_file(&old_entry).unwrap();
            folder
        } else { f.folder.clone() };
        let entry = folder.join(if folder_plugin { "main.lua" } else { "a_plugin.lua" });
        // Keep ordinary frame callbacks alive while native preparation/consent is pending.
        let start = source.split("local code, state, reason").next().unwrap();
        let source = format!(r#"
            {start}
            local done = false
            function on_frame()
                if done then return end
                omsi.data.counter = (omsi.data.counter or 0) + 1
                omsi.save()
                assert(omsi.files.write("state.txt", tostring(omsi.data.counter)))
                omsi.storage.set("counter", omsi.data.counter)
                omsi.storage.save()
                local code, state, reason = omsi.process.exit_code(handle)
                if state == "exited" or state == "failed" then
                    done = true
                    omsi.message(state .. ":" .. tostring(code) .. ":" .. tostring(reason))
                    assert(omsi.process.forget(handle))
                end
            end
        "#);
        std::fs::write(entry, source).unwrap();
        let broker = f.broker();
        let mut host = f.load(broker.clone());
        let mut game = Game::default();
        let p = prompt(&mut host, &mut game, &broker);
        // The snapshot has been captured. Save real omsi.data/storage/files repeatedly.
        for _ in 0..16 { host.frame(&mut game); }
        assert!(!f.marker.exists());
        assert!(broker.respond(p.id, ProcessDecision::AlwaysAllow));
        drive(&mut host, &mut game, |g| !g.messages.is_empty());
        assert_eq!(game.messages, ["exited:42:nil"]);
        host.finalize();
        std::fs::remove_file(&f.marker).unwrap();
        // Another plugin saves too, in the same shared plugins directory.
        std::fs::write(f.folder.join("z_other.save.lua"), "return { changed = true }").unwrap();
        let broker = f.broker();
        let mut host = f.load(broker.clone());
        let mut game = Game::default();
        drive(&mut host, &mut game, |g| !g.messages.is_empty());
        assert_eq!(game.messages, ["exited:42:nil"]);
        assert!(broker.prompt().is_none());
        host.finalize();
        std::fs::remove_dir_all(f.root).unwrap();
    }
}
#[test]
fn always_survives_restart_but_changes_reprompt_and_cannot_race_approval() {
    let f = Fixture::new();
    let broker = f.broker();
    let mut host = f.load(broker.clone());
    let mut game = Game::default();
    let p = prompt(&mut host, &mut game, &broker);
    assert!(broker.respond(p.id, ProcessDecision::AlwaysAllow));
    drive(&mut host, &mut game, |g| !g.messages.is_empty());
    assert_eq!(game.messages, ["exited:42:nil"]);
    host.finalize();
    assert!(f.saved.is_file());
    std::fs::remove_file(&f.marker).unwrap();
    let broker = f.broker();
    let mut host = f.load(broker.clone());
    let mut game = Game::default();
    drive(&mut host, &mut game, |g| !g.messages.is_empty());
    assert_eq!(game.messages, ["exited:42:nil"]);
    assert!(broker.prompt().is_none());
    host.finalize();
    std::fs::remove_file(&f.marker).unwrap();
    let entry = f.folder.join("a_plugin.lua");
    let source = std::fs::read_to_string(&entry).unwrap();
    std::fs::write(&entry, format!("{source}\n-- reviewed update\n")).unwrap();
    let mut host = f.load(broker.clone());
    let mut game = Game::default();
    let p = prompt(&mut host, &mut game, &broker);
    std::fs::write(
        &entry,
        format!("{source}\n-- changed while dialog was open\n"),
    )
    .unwrap();
    assert!(broker.respond(p.id, ProcessDecision::AlwaysAllow));
    drive(&mut host, &mut game, |g| !g.messages.is_empty());
    assert!(game.messages[0].contains("plugin content changed"));
    assert!(!f.marker.exists());
    host.finalize();
    std::fs::remove_dir_all(f.root).unwrap();
}
#[test]
fn unload_cancels_prompt_and_invalid_external_paths_never_prompt() {
    let f = Fixture::new();
    let broker = f.broker();
    let mut host = f.load(broker.clone());
    let mut game = Game::default();
    let p = prompt(&mut host, &mut game, &broker);
    host.finalize();
    let deadline = Instant::now() + Duration::from_secs(3);
    while broker.prompt().is_some() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(broker.prompt().is_none());
    assert!(!broker.respond(p.id, ProcessDecision::AllowOnce));
    assert!(!f.marker.exists());
    f.script("../outside.exe");
    let mut host = f.load(broker.clone());
    let mut game = Game::default();
    drive(&mut host, &mut game, |g| !g.messages.is_empty());
    assert!(game.messages[0].contains("failed"));
    assert!(broker.prompt().is_none());
    host.finalize();
    std::fs::remove_dir_all(f.root).unwrap();
}

#[test]
fn waiting_dialogs_do_not_occupy_preparation_workers() {
    let f = Fixture::new();
    let broker = f.broker();
    let mut first = f.load(broker.clone());
    let mut game = Game::default();
    let p = prompt(&mut first, &mut game, &broker);
    let mut second = f.load(broker.clone());
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        second.frame(&mut game);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!f.marker.exists());
    let helper = if cfg!(windows) {
        "helper.exe"
    } else {
        "helper"
    };
    let grant =
        ProcessGrant::capture(&f.folder.join("a_plugin.lua"), &f.folder, &[helper.into()]).unwrap();
    let mut approved = Plugins::load_with_process_grants(
        std::slice::from_ref(&f.folder),
        &HostConfig::default(),
        ProcessGrants {
            grants: vec![grant],
        },
    );
    let mut result = Game::default();
    drive(&mut approved, &mut result, |g| !g.messages.is_empty());
    assert_eq!(result.messages, ["exited:42:nil"]);
    assert_eq!(broker.prompt().unwrap().id, p.id);
    approved.finalize();
    first.finalize();
    second.finalize();
    std::fs::remove_dir_all(f.root).unwrap();
}

#[test]
fn packages_still_need_process_permission_with_an_interactive_host() {
    use oop_format::{Header, Kind, OopBuilder};
    let f = Fixture::new();
    std::fs::remove_file(f.folder.join("a_plugin.lua")).unwrap();
    let mut header = Header::new(
        "org.test.no-process",
        "Restricted",
        "1.0.0",
        Kind::Lua,
        "main.lua",
    );
    header.permissions = vec!["ui".into()];
    let package = OopBuilder::new(header).file("main.lua", "function on_frame() local ok=pcall(omsi.process.start,'helper.exe'); omsi.message(ok and 'allowed' or 'denied') end").write().unwrap();
    std::fs::write(f.folder.join("restricted.oop"), package).unwrap();
    let broker = f.broker();
    let mut host = f.load(broker.clone());
    let mut game = Game::default();
    assert_eq!(host.lua.len(), 2);
    host.frame(&mut game);
    assert_eq!(game.messages, ["denied"]);
    assert!(broker.prompt().is_none());
    assert!(!f.marker.exists());
    host.finalize();
    std::fs::remove_dir_all(f.root).unwrap();
}
