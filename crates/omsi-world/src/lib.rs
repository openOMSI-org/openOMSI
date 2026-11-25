//! openOMSI's map as data, below the renderer: the map-wide index of every spline and
//! object, the tiles read with their chrono patches, and where `[attachObj]` objects and
//! `[splineAttachement]` rows go (`map_index`). The game's world opens and draws the tiles
//! with what is decided here.

pub mod map_index;
pub mod stop_numbers;
