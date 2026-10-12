use omsi_platform::updater::{self, Release, Status, Updater};
use serde_json::{json, Value};
use std::sync::Mutex;

static UPDATER: Mutex<Option<Updater>> = Mutex::new(None);

fn with<T>(f: impl FnOnce(&mut Updater) -> T) -> T {
    let mut held = UPDATER.lock().unwrap_or_else(|e| e.into_inner());
    f(held.get_or_insert_with(Updater::default))
}

pub fn allowed() -> bool {
    !updater::STORE_BUILD && !omsi_cfg::flags::OMSI_NO_UPDATE.is_set()
}

fn release_json(r: &Release) -> Value {
    json!({ "version": r.version, "page": r.page, "size": r.size })
}

pub fn status() -> Value {
    with(|u| {
        u.poll();
        let mut v = match u.status() {
            Status::Idle => json!({ "state": "idle" }),
            Status::Checking => json!({ "state": "checking" }),
            Status::UpToDate => json!({ "state": "up_to_date" }),
            Status::Available(r) => json!({ "state": "available", "release": release_json(&r) }),
            Status::Downloaded(r) => json!({ "state": "available", "release": release_json(&r), "downloaded": true }),
            Status::Downloading { release, done, total } => json!({ "state": "downloading", "release": release_json(&release), "done": done, "total": total }),
            Status::Installing(r) => json!({ "state": "installing", "release": release_json(&r) }),
            Status::WaitingForInstaller(r) => json!({ "state": "installer", "release": release_json(&r) }),
            Status::Restarting(r) => json!({ "state": "restarting", "release": release_json(&r) }),
            Status::Failed(msg) => json!({ "state": "failed", "message": msg }),
        };
        v["current"] = json!(updater::current_version());
        v["allowed"] = json!(allowed());
        v["dismissed"] = json!(u.dismissed);
        v["dismissed_version"] = json!(u.dismissed_version);
        v["repo"] = json!(updater::REPO_URL);
        v
    })
}

pub fn check(again: bool) -> Value {
    if allowed() {
        with(|u| {
            if let Status::Failed(_) | Status::UpToDate = u.status() {
                u.dismiss();
            }
            if again {
                u.dismissed_version = None;
            }
            u.check();
        });
    }
    status()
}

pub fn install(p: &dyn crate::Platform) -> Value {
    if !omsi_launcher_lib::IN_PROCESS_GAMES && omsi_launcher_lib::instances::list().iter().any(|i| i.running) {
        let mut v = status();
        v["refused"] = json!("game");
        return v;
    }
    p.before_update();
    with(|u| {
        if let Status::Available(r) | Status::Downloaded(r) = u.status() {
            omsi_launcher_lib::log_to_file(&format!("update: installing {}", r.version));
            u.install(r);
        }
    });
    status()
}

pub fn dismiss() -> Value {
    with(|u| u.dismiss());
    status()
}

pub fn just_updated() -> Option<String> {
    let v = updater::just_updated()?;
    omsi_launcher_lib::log_to_file(&format!("update: this start follows the update to {v}"));
    Some(v)
}
