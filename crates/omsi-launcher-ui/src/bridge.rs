//! Omsi-Hub's launcher modules came from the game program (`crates/omsi-app/src/launcher`),
//! where they named the program's own modules: `crate::mt`, `crate::spawn`, `crate::companion`
//! and so on. Here those names stand for what the launcher may reach from this crate - the
//! program's answers through `host::Host`, or the lower crates the modules moved into - so the
//! pages read as they did and the program stays the only one that knows its insides.

use crate::host::host;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The interface's machine translation (the program's `mt`).
pub(crate) mod mt {
    use super::host;

    /// Texts machine translation must leave alone: names of maps, buses, lines.
    pub(crate) fn protect<'a>(texts: impl IntoIterator<Item = &'a str>) {
        let mut it = texts.into_iter();
        host().mt_protect(&mut it);
    }

    #[allow(dead_code)]
    pub(crate) fn enable(on: bool) {
        host().mt_enable(on);
    }

    #[allow(dead_code)]
    pub(crate) fn status() -> String {
        host().mt_status()
    }
}

/// The program's version (`startup::VERSION`).
pub(crate) mod startup {
    pub(crate) use omsi_platform::VERSION;
}

/// A custom weather and the weather stations (the program's `weather_setup`, whose launcher
/// part is omsi-content's `weather_choice` now).
pub(crate) mod weather_setup {
    pub(crate) use omsi_content::weather_choice::*;
    #[allow(unused_imports)]
    pub(crate) use omsi_weather::conditions::cloud_drift_at;
}

/// The program's platform helpers.
pub(crate) mod platform {
    /// Show `path` in the file manager.
    pub(crate) fn reveal(path: &std::path::Path) {
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
        #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android"), not(target_os = "ios")))]
        let _ = std::process::Command::new("xdg-open").arg(path.parent().unwrap_or(path)).spawn();
        #[cfg(any(target_os = "android", target_os = "ios"))]
        let _ = path;
    }
}

/// The duty's companion on a phone (the program's `companion`): the tunnel's program and the
/// personnel number a driver signs on with.
pub(crate) mod companion {
    use super::host;
    pub(crate) use crate::host::Cloudflared;

    /// Whether cloudflared is there (or being fetched).
    pub(crate) fn cloudflared() -> Cloudflared {
        host().cloudflared()
    }

    /// Fetch cloudflared on a worker.
    pub(crate) fn download_cloudflared() {
        host().download_cloudflared();
    }

    /// The personnel number and code of driver `name` (made the first time it is asked).
    pub(crate) fn personnel_of_driver(name: &str) -> Option<(String, String)> {
        host().personnel_of_driver(name)
    }
}

/// The player's bus as the game puts it down (the program's `spawn`): its file, its coupled
/// parts, its livery and the bus options of `--setvar`.
pub mod spawn {
    use super::{host, Arc, Path, PathBuf};

    /// The vehicle file the game spawns for `bus` (an articulated bus's rear section: the
    /// whole bus).
    pub(crate) fn player_bus_path(root: &Path, bus: &str) -> anyhow::Result<PathBuf> {
        host().player_bus_path(root, bus)
    }

    /// The `[couple_back]` chain behind `vehicle`, read as the game reads it.
    #[allow(dead_code)]
    pub(crate) fn load_coupled_parts(root: &Path, vehicle: &mut omsi_sim::VehicleInstance) -> Vec<Arc<omsi_sim::VehicleType>> {
        host().load_coupled_parts(root, vehicle)
    }

    /// The paint scheme named `paint` (or numbered so) of `vt`, as the game picks it for
    /// `--paint` (its `spawn::paint_scheme`).
    pub(crate) fn paint_scheme(vt: &omsi_sim::VehicleType, paint: Option<&str>) -> Option<usize> {
        let asked = paint.map(str::trim).filter(|p| !p.is_empty())?;
        vt.paint_schemes.iter().position(|s| s.name.eq_ignore_ascii_case(asked)).or_else(|| asked.parse::<usize>().ok().filter(|i| *i < vt.paint_schemes.len()))
    }

    /// The paint scheme of a coupled part (an articulated bus's rear section) for the lead's
    /// `scheme`: the part's scheme of the same name, as OMSI pairs the repaints of both parts
    /// by their name; a part without one keeps the same place in its list (as before, when
    /// the parts' folders list the same repaints in the same order).
    pub fn part_scheme(lead: &omsi_sim::VehicleType, scheme: Option<usize>, part: &omsi_sim::VehicleType) -> Option<usize> {
        let i = scheme?;
        let named = lead.paint_schemes.get(i).and_then(|s| part.paint_schemes.iter().position(|p| p.name.eq_ignore_ascii_case(&s.name)));
        named.or_else(|| (i < part.paint_schemes.len()).then_some(i))
    }

    /// The variables of `--setvar` (`name=value,…`, the launcher's bus options): names
    /// trimmed, pieces without a name or an `=` left out.
    pub fn setvars(arg: Option<&str>) -> Vec<(String, f32)> {
        arg.into_iter()
            .flat_map(|sv| sv.split(','))
            .filter_map(|kv| kv.split_once('='))
            .filter(|(k, _)| !k.trim().is_empty())
            .map(|(k, v)| (k.trim().to_string(), omsi_cfg::parse_f32(v)))
            .collect()
    }
}

/// Show the interface in `code` (the settings' ENG / DEU ...): the program's tables. The
/// launcher's own tests read the game's tables themselves (`i18n!` in `lib.rs`).
pub(crate) fn ui_language(code: &str) {
    #[cfg(test)]
    {
        omsi_ui::i18n::set_lookup(|lang, text| crate::_rust_i18n_try_translate(lang, text).map(|t| t.into_owned()));
        omsi_ui::i18n::set_language(omsi_launcher_lib::language_iso(code));
    }
    #[cfg(not(test))]
    host().ui_language(code);
}
