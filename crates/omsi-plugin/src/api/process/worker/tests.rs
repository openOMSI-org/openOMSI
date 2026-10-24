use super::*;
use crate::api::process::{ProcessGrant, Processes};
use crate::{HostConfig, PluginIo, Plugins};
use std::collections::HashMap;
use std::sync::{Condvar, atomic::AtomicUsize};
use std::time::Instant;

static SERIAL: Mutex<()> = Mutex::new(());
#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
    entered: AtomicUsize,
}
impl Gate {
    fn wait(&self) {
        self.entered.fetch_add(1, Ordering::Release);
        let _ = self
            .changed
            .wait_timeout_while(self.open.lock().unwrap(), Duration::from_secs(5), |open| {
                !*open
            })
            .unwrap();
    }
    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }
}
struct Release(Arc<Gate>);
impl Drop for Release {
    fn drop(&mut self) {
        self.0.release();
    }
}

#[derive(Default)]
struct Game {
    vars: HashMap<String, f32>,
    messages: Vec<String>,
}
impl PluginIo for Game {
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
    fn set_var(&mut self, name: &str, value: f32) {
        self.vars.insert(name.into(), value);
    }
    fn string(&mut self, _: &str) -> Option<String> {
        None
    }
    fn set_string(&mut self, _: &str, _: &str) {}
    fn fire(&mut self, _: &str, _: bool) {}
    fn message(&mut self, text: &str, _: f32) {
        self.messages.push(text.into());
    }
}

fn fixture() -> (PathBuf, PathBuf, ProcessGrants) {
    let root = std::env::temp_dir().join(format!(
        "omsi-worker-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let entry = root.join("a_wait.lua");
    std::fs::write(
        &entry,
        r#"
        local handle = assert(omsi.process.start("helper.exe"))
        local _, state, reason = omsi.process.wait(handle)
        assert(state == "failed")
        omsi.message(reason)
    "#,
    )
    .unwrap();
    std::fs::write(
        root.join("b_frame.lua"),
        "function on_frame() omsi.set_var('frames', (omsi.var('frames') or 0) + 1) end",
    )
    .unwrap();
    std::fs::write(root.join("helper.exe"), "approved content").unwrap();
    let mut grants = ProcessGrants::default();
    grants
        .grants
        .push(ProcessGrant::capture(&entry, &root, &["helper.exe".into()]).unwrap());
    (root, entry, grants)
}

#[test]
fn frames_continue_during_slow_verification_and_queued_content_is_rechecked() {
    let _serial = SERIAL.lock().unwrap();
    let (root, _, mut grants) = fixture();
    let gate = Arc::new(Gate::default());
    let _release = Release(gate.clone());
    let copy = gate.clone();
    grants.before_verify = Some(Arc::new(move || copy.wait()));
    let at = Instant::now();
    let mut plugins = Plugins::load_with_process_grants(
        std::slice::from_ref(&root),
        &HostConfig::default(),
        grants,
    );
    assert!(
        at.elapsed() < Duration::from_secs(1),
        "plugin load blocked on worker"
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while gate.entered.load(Ordering::Acquire) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(gate.entered.load(Ordering::Acquire), 1);
    let mut game = Game::default();
    game.vars.insert("frames".into(), 0.0);
    let at = Instant::now();
    for _ in 0..256 {
        plugins.frame(&mut game);
    }
    assert!(
        at.elapsed() < Duration::from_secs(1),
        "frames blocked on validation"
    );
    assert_eq!(game.vars["frames"], 256.0);
    assert!(game.messages.is_empty());
    std::fs::write(root.join("helper.exe"), "changed after queueing").unwrap();
    gate.release();
    let deadline = Instant::now() + Duration::from_secs(5);
    while game.messages.is_empty() && Instant::now() < deadline {
        plugins.frame(&mut game);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        game.messages
            .iter()
            .any(|s| s.contains("plugin content changed")),
        "{:?}",
        game.messages
    );
    plugins.finalize();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn admission_limits_precede_verification_and_the_global_queue_is_bounded() {
    let _serial = SERIAL.lock().unwrap();
    let (root, entry, mut grants) = fixture();
    let gate = Arc::new(Gate::default());
    let _release = Release(gate.clone());
    let copy = gate.clone();
    grants.before_verify = Some(Arc::new(move || copy.wait()));
    let grants = Arc::new(grants);
    let request = || StartRequest {
        permissions: None,
        plugin: "test".into(),
        grants: grants.clone(),
        entry: entry.clone(),
        folder: Some(root.clone()),
        file: "helper.exe".into(),
        args: vec![],
        cwd: None,
        env: vec![],
    };
    let mut first = Processes::default();
    let mut second = Processes::default();
    for _ in 0..2 {
        first.start_checked(request()).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while gate.entered.load(Ordering::Acquire) != WORKERS && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(gate.entered.load(Ordering::Acquire), WORKERS);
    for _ in 2..32 {
        first.start_checked(request()).unwrap();
    }
    assert!(
        first
            .start_checked(request())
            .unwrap_err()
            .contains("limit 32")
    );
    for _ in 0..2 {
        second.start_checked(request()).unwrap();
    }
    let at = Instant::now();
    assert!(
        second
            .start_checked(request())
            .unwrap_err()
            .contains("queue is full")
    );
    assert!(at.elapsed() < Duration::from_millis(250));
    assert_eq!(gate.entered.load(Ordering::Acquire), WORKERS);
    first.stop_all();
    second.stop_all();
    gate.release();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Arc::strong_count(&grants) > 1 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        gate.entered.load(Ordering::Acquire),
        WORKERS,
        "cancelled queued starts performed verification"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn child_cleanup_continues_while_both_preparation_workers_are_blocked() {
    let _serial = SERIAL.lock().unwrap();
    let mut running = submit(Box::new(|_| {
        #[cfg(windows)]
        let (file, args) = (
            "cmd.exe",
            vec!["/C".into(), "for /L %i in (1,1,2147483647) do @rem".into()],
        );
        #[cfg(not(windows))]
        let (file, args) = ("/bin/sh", vec!["-c".into(), "exec sleep 60".into()]);
        super::super::spawn(file, &args, None, &[])
    }))
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while running.status().starting && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(running.status().pid > 0);
    let gate = Arc::new(Gate::default());
    let _release = Release(gate.clone());
    let mut blockers = Vec::new();
    for _ in 0..WORKERS {
        let gate = gate.clone();
        blockers.push(
            submit(Box::new(move |_| {
                gate.wait();
                Err("fixture released".into())
            }))
            .unwrap(),
        );
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while gate.entered.load(Ordering::Acquire) < WORKERS && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(gate.entered.load(Ordering::Acquire), WORKERS);
    running.kill();
    let deadline = Instant::now() + Duration::from_secs(2);
    while running.status().exit.is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        running.status().exit.is_some(),
        "cleanup waited for validation workers"
    );
    assert!(!*gate.open.lock().unwrap());
    drop(blockers);
    gate.release();
}
