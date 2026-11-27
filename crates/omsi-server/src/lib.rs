//! Dedicated openOMSI host services.
//!
//! This crate is deliberately **below** the graphical application: no wgpu,
//! renderer, scene graph, UI or audio dependency is permitted here.
//! The transport, administration and console can be reused by a true headless
//! runtime while the application-specific world adapters are being extracted.

pub mod admin;
pub mod ai_sync;
mod chrono;
pub mod clock;
pub mod collision;
pub mod collision_mesh;
pub mod commands;
pub mod console;
pub mod context_sync;
pub mod fleet;
pub mod interest;
pub mod lan_world;
pub mod lanes;
pub mod metar;
pub mod mod_policy;
mod passenger_dispatch;
pub mod people_sync;
mod people_upstream;
pub mod population;
pub mod reboot;
pub mod reload;
pub mod runtime;
mod scenery_ground;
pub mod signals;
pub mod simulation;
pub mod startup;
pub mod stops;
pub mod streaming;
pub mod surfaces;
pub mod tick;
pub mod timetable;
pub mod traffic;
pub mod transport;
pub mod updates;
pub mod weather;
pub mod world;
