//! openOMSI's scene: builds a renderable scene from a map and its vehicles - terrain,
//! splines, scenery objects, the vehicles' types and materials, their lights, the rain and
//! the wipers on their glass, the puddles - and keeps the world it was built from (`World`).
//! The game's own per-frame view sync (omsi-app's `view_sync`) and everything that needs the
//! game stay in omsi-app; what the game hands the scene is in `host`.
//!
//! Loading is parallel: every tile is parsed and tessellated on the rayon pool, scenery
//! object types are loaded once and shared, then everything is uploaded to the GPU on the
//! calling thread.

use omsi_world::map_index::{MapIndex, Pose};
use anyhow::{Context, Result};
use glam::{DVec2, DVec3, Mat4};
use hashbrown::HashMap;
use omsi_geometry::{
    build_spline_mesh, build_terrain_mesh, mesh_from_o3d, object_rotation, MeshData, SplineCurve,
    TileSurface,
};
use omsi_map::{tile_size, GlobalCfg, Terrain};
use omsi_model::{MaterialDef, MeshDef, Model};
use omsi_render::{
    AlphaMode, MaterialExtra, MaterialId, MeshId, RenderPhase, Renderer, Scene, TextureId,
};
use omsi_scenery::{SceneryObject, Spline};
use omsi_sim::traffic::{Lane, LaneBuilder, LaneKey, LaneKind, TrafficLightController};
use omsi_texture::{Image, TextureCache, TextureData};
use parking_lot::Mutex;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub mod camera_arm;
pub mod driver;
pub mod lights;
pub mod mapgen;
pub mod road_paint;
pub mod money;
pub mod puddles;
pub mod rain;
pub mod route_arrows;
pub mod threads;
pub mod trolley_derive;
pub mod arc_fx;
pub mod trolley_view;
pub mod wheel_surface;
pub mod window_wipers;

mod staging;
mod batching;
mod spline_layers;
mod terrain_paint;
mod sound_probe;
mod world;
mod open;
mod stage;
mod place;
mod cut_terrain;
mod upload;
mod place_step;
mod props;
mod lightmaps;
mod scripted;
pub use scripted::DayKind;
mod vehicle_materials;
mod vehicle_types;
mod freetex_reload;
pub use freetex_reload::FreeTextureReloads;
mod pane_uv;
/// Mesh `i` of a vehicle type as the GPU takes it (the glass with its film's coordinates in
/// metres, see `pane_uv`): for a mesh uploaded again after it was changed.
pub fn gpu_mesh_of(vt: &omsi_sim::VehicleType, i: usize) -> Option<omsi_geometry::MeshData> {
    pane_uv::gpu_mesh(vt, i).map(std::borrow::Cow::into_owned)
}
mod vehicles;
mod lanes;
mod season_looks;
pub mod host;
pub mod head_idle;
pub mod hud;
pub mod editor;
pub mod tiles;
pub mod schedule_paper;
pub mod check;
pub mod export;
pub mod camera_util;

pub use staging::*;
pub use batching::*;
pub use terrain_paint::*;
pub use world::*;
pub use open::*;
pub use props::*;
pub use scripted::*;
pub use vehicle_materials::*;
pub use vehicle_types::*;
use lanes::*;
use lightmaps::*;
pub use stage::*;
#[cfg(test)]
use place::field_height;

#[cfg(test)]
mod surf_map_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod material_tests;

#[cfg(test)]
mod navigation_road_tests;

#[cfg(test)]
mod object_path_tests;

#[cfg(test)]
mod terrain_mapping_tests;

#[cfg(test)]
mod crossing_deformation_tests;
