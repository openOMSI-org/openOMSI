//! openOMSI's platform services: what the game and the launcher do with the world outside
//! the simulation - the updates from GitHub (`updater`), Discord's rich presence (`discord`),
//! the project's count of running games (`presence`), what the start guard needs to heal a
//! broken release (`recovery`) - and the version stamp of the build.
//!
//! Nothing here knows the game's state: the app hands over plain values (a status line, a
//! version) and shows what comes back.

pub mod avatars;
pub mod diagnostics;
pub mod discord;
pub mod memory;
pub mod presence;
pub mod proxy;
pub mod quit;
pub mod recovery;
pub mod support_bundle;
pub mod tomorrow;
pub mod updater;
pub mod webview2;

/// The commit the build came from, with `+` when the tree had changes, and its date (see
/// `build.rs`).
pub const BUILD: &str = env!("OMSI_BUILD");

/// The release version, `MAJOR.MINOR.PATCH` (see `build.rs` and docs/VERSIONING.md).
pub const VERSION: &str = env!("OPENOMSI_VERSION");

/// Android: the calls into Java the services need (the system's package installer, the
/// browser), which only the app's activity can make - the app installs them at its start
/// (`android_main`), before anything is updated or opened.
pub mod android {
    use std::sync::OnceLock;

    pub struct Hooks {
        /// Hand a downloaded APK to the system's package installer.
        pub install_apk: fn(&std::path::Path) -> anyhow::Result<()>,
        /// The package installer's answer so far (its code and message).
        pub install_status: fn() -> Option<(i32, String)>,
        /// A web page in the phone's browser.
        pub open_url: fn(&str),
    }

    static HOOKS: OnceLock<Hooks> = OnceLock::new();

    /// Set once, at the start (a second call changes nothing).
    pub fn install(hooks: Hooks) {
        let _ = HOOKS.set(hooks);
    }

    pub fn hooks() -> Option<&'static Hooks> {
        HOOKS.get()
    }
}
