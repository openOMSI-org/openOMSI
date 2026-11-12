//! openOMSI's weather as state and physics: the physical weather model (`weather_model`),
//! the weather changing by itself (`weather_cycle`), a season chosen with its phase
//! (`season_phase`), the roads' wetness and the clouds' drift (`conditions`), the cabin air
//! and the condensation on the glass (`condensation`) and the drops running down it
//! (`window_drops`). Nothing here draws or plays anything: the game reads this state each
//! frame and hands it to the renderer and the sound.

pub mod condensation;
pub mod conditions;
pub mod season_phase;
pub mod climate;
pub mod weather_cycle;
pub mod weather_model;
pub mod window_drops;
