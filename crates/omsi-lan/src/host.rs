//! What LAN play needs of the game around it that is not plain data: the game implements
//! [`LanHost`] and hands it to the session's calls.

use crate::config::LanConfig;
use omsi_sim::traffic::Lane;
use omsi_sim::world_data::WorldData;
use std::sync::Arc;

pub trait LanHost {
    /// The world as the simulation reads it, while one is loaded.
    fn world(&self) -> Option<Arc<WorldData>>;
    /// The world's own lanes (for a spawn off the traffic's network).
    fn world_lanes(&self) -> Vec<Lane>;
    /// With a world loaded: the season's texture folder it would have with `cfg`'s date,
    /// season and weather, and the one loaded now (None in either: the default look).
    fn season_folders(&self, cfg: &LanConfig) -> Option<(Option<String>, Option<String>)>;
    /// `weather` names a weather the game makes without a file (the natural model, the
    /// cycle, a custom one).
    fn weather_without_file(&self, weather: &str) -> bool;
    /// `weather` is a weather report (METAR): Some(whether the game can read it).
    fn weather_report(&self, weather: &str) -> Option<bool>;
    /// The depot file of vehicle type `ty` for the map and `cfg`'s depot choice.
    fn depot_of(&self, cfg: &LanConfig, ty: &omsi_sim::VehicleType) -> Option<Arc<omsi_vehicle::Hof>>;
    /// Another player's vehicle of type `ty` in paint scheme `scheme`, made as the game
    /// makes its vehicles (fonts, text textures, coupled rear sections), on `clock`; and
    /// the depot file its destination display is set from. Set the owner-only script texture
    /// and sound flags on its host before running its init scripts. `fonts`: a bus fetched by
    /// its content writes its displays with its own (`mods::vehicles::fonts_of`), else with the
    /// world's.
    fn remote_vehicle(
        &self,
        cfg: &LanConfig,
        ty: Arc<omsi_sim::VehicleType>,
        scheme: Option<usize>,
        clock: omsi_sim::SimClock,
        fonts: Option<crate::mods::vehicles::SharedFonts>,
    ) -> (omsi_sim::VehicleInstance, Option<Arc<omsi_vehicle::Hof>>);
    /// Set a vehicle's line and destination displays as the timetable sets an AI bus's.
    fn set_destination(&self, v: &mut omsi_sim::VehicleInstance, hof: Option<&omsi_vehicle::Hof>, line: &str, destination: &str);
    /// The game's version, for the session's public listing.
    fn version(&self) -> String;
    /// Offer the buses a joining player may choose (`only`, else every bus installed).
    fn publish_vehicles(&self, only: Vec<String>);
}
