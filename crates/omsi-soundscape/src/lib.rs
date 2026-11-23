//! openBus's sounds of the world, on top of omsi-audio: the soundscape's recordings and
//! where they play (`pack`, `catalog`, `place`, `rules`, `voices`; the game's soundscape
//! puts them together each frame with the world it knows), the ambience's one-shots and
//! footfalls (`ambience`) and the bus radio (`radio`). Nothing here draws anything or knows
//! the game's `App`.

pub mod ambience;
pub mod catalog;
pub mod pack;
pub mod place;
pub mod radio;
pub mod rules;
pub mod voices;
