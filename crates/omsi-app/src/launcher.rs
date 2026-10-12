//! The launcher in this program: its window and pages are the crate omsi-launcher-ui; this
//! is the program's side of it (`AppHost`, installed before the window opens): the settings,
//! the renderer's start, the bus preview drawn with the game's own scene (`showroom`), the
//! city map read the way the navigator reads it, the interface's language and machine
//! translation, the bus radio's stations and the support package.
//! The old paths (`crate::launcher::ui`, `::state`, `::drive` ...) are the crate's.

mod pads;
mod service;
pub(crate) mod showroom;
pub(crate) use service::run as run_service;
#[cfg_attr(not(target_os = "android"), allow(unused_imports))]
pub(crate) use service::InProcess;

pub(crate) use omsi_launcher_ui::{drive, pages, theme, ui, Launcher, RAIL_W};
#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(unused_imports))]
pub(crate) use omsi_launcher_ui::mapview;
#[cfg_attr(not(target_os = "android"), allow(unused_imports))]
pub(crate) use omsi_launcher_ui::crash_of;
#[allow(unused_imports)]
pub(crate) use omsi_launcher_ui::state;

use omsi_launcher_ui::host::{CityMap, Host, MapRoad, Selection, SupportJob};
use omsi_render::{RenderOptions, Renderer};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Run the launcher window until it is closed.
pub fn run(instance: wgpu::Instance) -> anyhow::Result<()> {
    install();
    omsi_launcher_ui::run(instance)
}

/// The launcher, not yet in a window (a phone's, see `android`).
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn new(instance: wgpu::Instance) -> Launcher {
    install();
    Launcher::new(instance)
}

/// The program's side of the launcher, for every launcher made from here on. Whatever makes a
/// launcher `state::State` on its own (another front end bridging to it) calls this first:
/// the state sets the interface language and machine translation through it.
pub(crate) fn install() {
    omsi_launcher_ui::host::set(Box::new(AppHost));
}

/// The showroom's renderer: a bus on a floor needs none of the game's costly passes - no
/// ambient occlusion, a small shadow map, 4x MSAA for the edges whatever the game uses -
/// and it draws Vanilla+ whatever the game does (`showroom::lighting_for`): no enhanced
/// pipelines, reflection probe or cloud noise either (`RenderOptions::preview_only`).
pub(crate) fn showroom_options(settings: &crate::settings::Settings) -> omsi_render::RenderOptions {
    omsi_render::RenderOptions { msaa: 4, ssao: false, shadow_size: 1024, render_scale: 1.0, preview_only: true, no_enhanced: true, ray_tracing: false, ..settings.render_options() }
}

struct AppHost;

impl Host for AppHost {
    fn ui_language(&self, code: &str) {
        crate::ui_language(code);
    }
    fn mt_enable(&self, on: bool) {
        crate::mt::enable(on);
    }
    fn mt_status(&self) -> String {
        crate::mt::status()
    }
    fn mt_protect(&self, texts: &mut dyn Iterator<Item = &str>) {
        crate::mt::protect(texts);
    }
    #[cfg(target_os = "android")]
    fn previous_run_crash(&self) -> Option<(String, String)> {
        crate::android::previous_run_crash()
    }
    #[cfg(target_os = "ios")]
    fn previous_run_crash(&self) -> Option<(String, String)> {
        crate::ios::previous_run_crash()
    }
    #[cfg(target_os = "android")]
    fn hide_content_from_gallery(&self) {
        crate::android::hide_content_from_gallery();
    }
    fn radio_stations(&self) -> Vec<(String, String)> {
        crate::radio::own_stations()
    }
    fn save_radio_stations(&self, list: &[(String, String)]) -> std::io::Result<()> {
        crate::radio::save_stations(list)
    }
    fn navigator_placed_at(&self, corner: &str) -> Option<[f32; 2]> {
        crate::navigator::placed_at(corner)
    }
    fn support_bundle(&self, renderer: Option<&Renderer>, devices: Option<Vec<crate::controllers::Connected>>, (map, bus, line, tour): Selection<'_>) -> SupportJob {
        let snapshot = crate::support_bundle::snapshot(renderer, &crate::settings::Settings::load(), devices, Some((map, Some(bus), line, tour)), "launcher_selection");
        let out = crate::support_bundle::default_output();
        Box::new(move || crate::support_bundle::export(&out, &snapshot).map(|_| out).map_err(|e| format!("{e:#}")))
    }
    fn fetch_ambience(&self) {
        if crate::settings::Settings::load().ambient {
            crate::soundscape::pack::Pack::fetch_if_missing();
        }
    }
    fn showroom_options(&self) -> RenderOptions {
        showroom_options(&crate::settings::Settings::load())
    }
    fn begin_renderer(&self, instance: &mut wgpu::Instance, window: &Arc<winit::window::Window>, options: RenderOptions) -> anyhow::Result<Box<dyn omsi_launcher_ui::host::RendererStart>> {
        Ok(Box::new(crate::startup::RendererStart::begin(instance, window, options)?))
    }
    fn fatal_message(&self, text: &str) {
        crate::startup::fatal_message(text);
    }
    fn window_icon(&self) -> Option<winit::window::Icon> {
        crate::startup::window_icon()
    }
    fn fit_window(&self, event_loop: &winit::event_loop::ActiveEventLoop, w: f64, h: f64) -> (winit::dpi::LogicalSize<f64>, Option<winit::dpi::PhysicalPosition<i32>>) {
        crate::startup::fit_window(event_loop, w, h)
    }
    fn window_background(&self) -> wgpu::Color {
        crate::startup::WINDOW_BACKGROUND
    }
    fn new_showroom(&self) -> Box<dyn omsi_launcher_ui::host::Showroom> {
        Box::new(showroom::Showroom::new())
    }
    fn clear_showroom_mark(&self) {
        showroom::clear_placing_mark();
    }
    fn city_map(&self, root: &Path, tiles: &[(i32, i32, PathBuf)], chrono: &[PathBuf]) -> CityMap {
        let crate::scene::NavigationMap { lanes, road_surfaces, places, signs: _ } = crate::scene::navigation_map_of(root, tiles, chrono);
        let (roads, net) = crate::navigator::city_roads(lanes, &road_surfaces);
        CityMap { roads: roads.into_iter().map(|r| MapRoad { points: r.points, width: r.width, main: r.main }).collect(), net, places: places.into_iter().collect() }
    }
    fn city_map_colors(&self) -> [omsi_ui::Color; 4] {
        use crate::navigator::{ROAD, ROAD_CASING, ROAD_MAIN, ROUTE};
        [ROAD_CASING, ROAD, ROAD_MAIN, ROUTE]
    }
    fn simplify(&self, pts: &[glam::Vec3], tol: f32) -> Vec<glam::Vec3> {
        crate::navigator::simplify(pts, tol)
    }
}

impl omsi_launcher_ui::host::RendererStart for crate::startup::RendererStart {
    fn device(&self) -> &wgpu::Device {
        &self.device
    }
    fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
    fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
    fn early(&self) -> bool {
        crate::startup::RendererStart::early(self)
    }
    fn is_done(&self) -> bool {
        crate::startup::RendererStart::is_done(self)
    }
    fn abandon(self: Box<Self>) {
        crate::startup::RendererStart::abandon(*self)
    }
    fn wait(self: Box<Self>, instance: &mut wgpu::Instance) -> anyhow::Result<Renderer> {
        crate::startup::RendererStart::wait(*self, instance)
    }
}

impl omsi_launcher_ui::host::Showroom for showroom::Showroom {
    fn want_choice(&mut self, root: &str, choice: &omsi_launcher_ui::state::Choice) {
        showroom::Showroom::want_choice(self, root, choice)
    }
    fn orbit(&mut self, dx: f32, dy: f32) {
        showroom::Showroom::orbit(self, dx, dy)
    }
    fn zoom_by(&mut self, k: f32) {
        showroom::Showroom::zoom_by(self, k)
    }
    fn set_focus_x(&mut self, x: f32) {
        self.focus_x = x;
    }
    fn update(&mut self, renderer: &Renderer, dt: f32) -> bool {
        showroom::Showroom::update(self, renderer, dt)
    }
    fn preview(&mut self, renderer: &mut Renderer, w: u32, h: u32) -> Option<wgpu::TextureView> {
        showroom::Showroom::preview(self, renderer, w, h)
    }
    fn has_picture(&self) -> bool {
        showroom::Showroom::has_picture(self)
    }
    fn failed(&self) -> bool {
        self.error.is_some()
    }
    fn busy(&self) -> bool {
        self.busy
    }
    fn generation(&self) -> u64 {
        self.generation
    }
    fn drawn(&self) -> u64 {
        self.drawn
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    /// What the launcher's settings page saves is what the game reads (the page's own tests
    /// are omsi-launcher-ui's; the game's reading of the file is here).
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
}
