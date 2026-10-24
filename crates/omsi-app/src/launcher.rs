//! The launcher's side in this program. The launchers are other programs or other front ends:
//! the desktop launcher (`launcher/`, a Tauri window beside this program, see `launcher_link`),
//! the launcher page in a web view on Android (`android_launcher`) and the Swift launcher on
//! Apple's systems (`apple_bridge`). What they ask of the game is here: the helper service the
//! desktop launcher runs (`openomsi --launcher-service`: the bus preview drawn with the game's
//! own scene in `showroom`, the city map, the controllers), and the program's side of the
//! launcher's model (`AppHost`): the interface's language, machine translation and the city map
//! read the way the navigator reads it.

mod key_actions;
mod pads;
mod service;
pub(crate) mod showroom;
pub(crate) use service::run as run_service;
#[cfg_attr(not(target_os = "android"), allow(unused_imports))]
pub(crate) use service::InProcess;

#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(unused_imports))]
pub(crate) use omsi_launcher_model::mapview;
#[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(unused_imports))]
pub(crate) use omsi_launcher_model::crash_of;
#[allow(unused_imports)]
pub(crate) use omsi_launcher_model::state;

use omsi_launcher_model::host::{CityMap, Host, MapRoad};
use std::path::{Path, PathBuf};

/// The program's side of the launcher's model, for every model made from here on. Whatever
/// makes a launcher `state::State` (another front end bridging to it) or reads a map through
/// `mapview` calls this first: the state sets the interface language and machine translation
/// through it.
pub(crate) fn install() {
    omsi_launcher_model::host::set(Box::new(AppHost));
}

/// Keep the player's shading, anti-aliasing, render scale and texture options. A bus on
/// a floor needs only a small shadow map, with no ambient occlusion or ray tracing.
/// Plain graphics still omit the enhanced resources on every backend.
pub(crate) fn showroom_options(settings: &crate::settings::Settings) -> omsi_render::RenderOptions {
    let enhanced = showroom_enhanced(settings);
    let render_scale = if settings.render_scale > 0.0 { settings.render_scale } else { 1.0 };
    omsi_render::RenderOptions { msaa: settings.msaa.clamp(1, 4), ssao: false, shadow_size: settings.shadow_size.min(if cfg!(any(target_os = "android", target_os = "ios")) { 512 } else { 1024 }), render_scale, preview_only: !enhanced, no_enhanced: !enhanced, ray_tracing: false, fsr1: false, ..settings.render_options() }
}

pub(crate) fn showroom_enhanced(settings: &crate::settings::Settings) -> bool {
    crate::startup::SessionSettings::for_showroom(settings).enhanced && !cfg!(any(target_os = "android", target_os = "ios"))
}

struct AppHost;

impl Host for AppHost {
    fn ui_language(&self, code: &str) {
        crate::ui_language(code);
    }
    fn mt_enable(&self, on: bool) {
        crate::mt::enable(on);
    }
    fn mt_protect(&self, texts: &mut dyn Iterator<Item = &str>) {
        crate::mt::protect(texts);
    }
    fn tr(&self, text: &str) -> String {
        omsi_ui::tr(text).into_owned()
    }
    #[cfg(target_os = "android")]
    fn previous_run_crash(&self) -> Option<(String, String)> {
        crate::android::previous_run_crash()
    }
    #[cfg(target_os = "ios")]
    fn previous_run_crash(&self) -> Option<(String, String)> {
        crate::ios::previous_run_crash()
    }
    fn city_map(&self, root: &Path, tiles: &[(i32, i32, PathBuf)], chrono: &[PathBuf]) -> CityMap {
        let crate::scene::NavigationMap { lanes, road_surfaces, places, signs: _, names: _ } = crate::scene::navigation_map_of(root, tiles, chrono);
        let (roads, net) = crate::navigator::city_roads(lanes, &road_surfaces);
        CityMap { roads: roads.into_iter().map(|r| MapRoad { points: r.points, width: r.width, main: r.main }).collect(), net, places: places.into_iter().collect() }
    }
    fn map_layers(&self, root: &Path, map_dir: &Path, tiles: &[(i32, i32, PathBuf)], chrono: &[PathBuf]) -> Option<omsi_map::layers::Layers> {
        let global = omsi_map::GlobalCfg::load(&map_dir.join("global.cfg")).ok()?;
        Some(crate::scene::mapgen::cached(root, map_dir, &global.ground_textures, tiles, chrono))
    }
    fn city_map_colors(&self) -> [[f32; 4]; 4] {
        use crate::navigator::{ROAD, ROAD_CASING, ROAD_MAIN, ROUTE};
        [ROAD_CASING.0, ROAD.0, ROAD_MAIN.0, ROUTE.0]
    }
    fn simplify(&self, pts: &[glam::Vec3], tol: f32) -> Vec<glam::Vec3> {
        crate::navigator::simplify(pts, tol)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn showroom_keeps_player_quality_without_the_expensive_passes() {
        for graphics in ["vanilla", "vanilla_plus", "enhanced", "enhanced_plus"] {
            let settings = crate::settings::Settings::from_text(&format!("graphics={graphics}\nmsaa=1\nrender_scale=0.75\nshadow_size=2048\nanisotropy=2\nreflections=0\n"));
            let options = super::showroom_options(&settings);
            assert_eq!(options.msaa, 1);
            assert_eq!(options.render_scale, 0.75);
            assert_eq!(options.anisotropy, 2);
            assert_eq!(options.reflections, settings.reflections > 0.0);
            assert_eq!(options.shadow_size, 1024);
            assert_eq!(options.preview_only, !super::showroom_enhanced(&settings));
            assert_eq!(options.no_enhanced, !super::showroom_enhanced(&settings));
            assert!(!options.ssao && !options.ray_tracing);
        }
    }

    #[test]
    fn the_preview_follows_the_quality_preset() {
        let options = |preset: &str| {
            let (_, p) = omsi_launcher_lib::graphics_presets().into_iter().find(|(name, _)| *name == preset).unwrap();
            let mut s = omsi_launcher_lib::settings_from_text(None);
            for (k, v) in p.as_object().unwrap() {
                s[k] = v.clone();
            }
            super::showroom_options(&crate::settings::Settings::from_text(&omsi_launcher_lib::settings_to_text(&s, None)))
        };
        let low = options("Low");
        assert_eq!((low.msaa, low.render_scale, low.anisotropy), (1, 0.75, 2));
        let medium = options("Medium");
        assert_eq!((medium.msaa, medium.render_scale), (4, 1.0));
        let ultra = options("Ultra");
        assert_eq!((ultra.msaa, ultra.render_scale, ultra.shadow_size, ultra.ssao), (4, 1.0, 1024, false));
    }

    #[test]
    fn only_drawing_settings_make_the_preview_anew() {
        let quality = |text: &str| super::showroom::Quality::of(&crate::settings::Settings::from_text(text));
        let base = quality("graphics=vanilla_plus\nmsaa=2\nvolume=0.5\ndetail_textures=1\n");
        let louder = quality("graphics=vanilla_plus\nmsaa=2\nvolume=NaN\ndetail_textures=1\n");
        assert!(base.options == louder.options && base.light == louder.light);
        let plain = quality("graphics=vanilla_plus\nmsaa=2\nvolume=0.5\ndetail_textures=0\n");
        assert!(base.options == plain.options && base.light != plain.light);
        let classic = quality("graphics=vanilla\nmsaa=2\nvolume=0.5\ndetail_textures=1\n");
        assert!(base.options == classic.options && base.light != classic.light);
        let sharper = quality("graphics=vanilla_plus\nmsaa=4\nvolume=0.5\ndetail_textures=1\n");
        assert!(base.options != sharper.options);
    }

    /// What the launcher's settings page saves is what the game reads (the launcher's own
    /// checks are its page's; the game's reading of the file is here).
    #[test]
    fn the_game_reads_what_the_settings_page_saves() {
        let mut s = omsi_launcher_lib::settings_from_text(None);
        s["cloud_quality"] = json!("low");
        s["mouse_smooth"] = json!(false);
        let saved = omsi_launcher_lib::settings_to_text(&s, None);
        let game = crate::settings::Settings::from_text(&saved);
        assert_eq!(game.cloud_quality, "low");
        assert!(!game.mouse_smooth);
        assert!(crate::settings::Settings::from_text("").mouse_smooth);
    }

    #[test]
    fn a_phone_launcher_starts_from_the_games_phone_defaults() {
        let launcher = omsi_launcher_lib::settings_from_text_on(None, true);
        let game = crate::settings::Settings::from_text(&omsi_launcher_lib::settings_to_text(&launcher, None));
        let d = crate::settings::Settings::phone();
        assert_eq!((game.msaa, game.anisotropy, game.ssao, game.shadow_size, game.mirror_size, game.max_fps), (d.msaa, d.anisotropy, d.ssao, d.shadow_size, d.mirror_size, d.max_fps));
        assert_eq!((game.max_obj_dist, game.pax_density, &game.navigator_corner), (d.max_obj_dist, d.pax_density, &d.navigator_corner));
        assert_eq!((game.shadows, game.detail_textures, game.reflections, &game.graphics), (d.shadows, d.detail_textures, d.reflections, &d.graphics));
    }
}
