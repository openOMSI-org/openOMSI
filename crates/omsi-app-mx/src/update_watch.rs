//! The game's eye on the updates (see `crate::updater`): a minute into a session, and every
//! hour after, it asks GitHub for the latest release in the background. A newer one is
//! downloaded during the session (`Updater::prefetch`) and a card over the navigator says so;
//! the launcher installs the file already downloaded when the session ends (by itself with
//! "Install updates without asking", else it offers it), so nobody waits for a download.
//!
//! Settings: `update_check` (looking at all, as the launcher's), `update_notify` (the cards
//! over the navigator; the download goes on without them). `OMSI_NO_UPDATE=1` switches it
//! off, `OMSI_UPDATE_URL` points it elsewhere (and lets a development build try it: one
//! never replaces itself, so it is not asked otherwise).

use crate::ui::{push_notice, Notice, NoticeKind};
use crate::updater::{self, Status, Updater};
use std::time::{Duration, Instant};

/// When the first look is taken (the start of a session has enough to do), and how often after.
const FIRST_CHECK: Duration = Duration::from_secs(60);
const EVERY: Duration = Duration::from_secs(3600);
/// After a failed look or download: the next try.
const AFTER_FAILURE: Duration = Duration::from_secs(15 * 60);

pub(crate) struct UpdateWatch {
    updater: Updater,
    enabled: bool,
    notify: bool,
    auto: bool,
    next_check: Instant,
    /// The version said (found / downloaded) over the navigator.
    told_found: Option<String>,
    told_ready: Option<String>,
}

impl UpdateWatch {
    pub(crate) fn new() -> UpdateWatch {
        let text = std::fs::read_to_string(omsi_launcher_lib::data_dir().join("settings.cfg")).ok();
        let s = omsi_launcher_lib::settings_from_text(text.as_deref());
        let on = |k: &str, d: bool| s.get(k).and_then(|v| v.as_bool()).unwrap_or(d);
        let testing = omsi_cfg::flags::OMSI_UPDATE_URL.is_set();
        let installable = testing || updater::install_place().is_ok() || cfg!(target_os = "android");
        let enabled = on("update_check", true)
            && !omsi_cfg::flags::OMSI_NO_UPDATE.is_set()
            && !updater::is_test_build(updater::current_version())
            && updater::asset_name(updater::current_version()).is_some()
            && installable;
        UpdateWatch {
            updater: Updater::default(),
            enabled,
            notify: on("update_notify", true),
            auto: on("update_auto", false),
            // (a test against OMSI_UPDATE_URL looks at once)
            next_check: Instant::now() + if testing { Duration::from_secs(3) } else { FIRST_CHECK },
            told_found: None,
            told_ready: None,
        }
    }

    /// Once a frame: look when it is time, download what was found, and say it.
    pub(crate) fn tick(&mut self, notices: &mut Vec<Notice>) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        match self.updater.status() {
            Status::Idle | Status::UpToDate if now >= self.next_check => {
                self.next_check = now + EVERY;
                self.updater.check();
            }
            Status::Available(r) => {
                if self.told_found.as_deref() != Some(r.version.as_str()) {
                    self.told_found = Some(r.version.clone());
                    log::info!("update: {} is out - downloading it in the background", r.version);
                    if self.notify {
                        let text = format!("openOMSI {} is out: it is downloaded in the background and installed when this session ends", r.version);
                        push_notice(notices, Notice { kind: NoticeKind::Update, text, left: 10.0, total: 10.0 });
                    }
                }
                self.updater.prefetch(r);
            }
            Status::Downloaded(r) if self.told_ready.as_deref() != Some(r.version.as_str()) => {
                self.told_ready = Some(r.version.clone());
                if self.notify {
                    // (a quick download: this card takes the place of "is out" still showing)
                    let found = format!("openOMSI {} is out", r.version);
                    notices.retain(|n| !(n.kind == NoticeKind::Update && n.text.starts_with(&found)));
                    let text = if cfg!(target_os = "android") || !self.auto {
                        format!("openOMSI {} is downloaded: the launcher offers it when this session ends", r.version)
                    } else {
                        format!("openOMSI {} is downloaded: it is installed when this session ends", r.version)
                    };
                    push_notice(notices, Notice { kind: NoticeKind::Update, text, left: 8.0, total: 8.0 });
                }
            }
            // (no card for a failure: it is tried again quietly, the log says why)
            Status::Failed(_) => {
                self.updater.dismiss();
                self.next_check = now + AFTER_FAILURE;
            }
            _ => {}
        }
    }
}
