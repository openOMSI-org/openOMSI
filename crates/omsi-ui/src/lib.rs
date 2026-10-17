//! Interface drawing shared by the game's navigator and the launcher.
//!
//! * [`text`]: Inter (variable weight) laid out and rasterised with `ab_glyph`;
//! * [`icons`]: Google's Material Symbols (Rounded, filled), rasterised with `resvg`;
//! * [`edit`]: what the keys do to a text field's line (caret, selection, words, clipboard);
//! * [`atlas`]: one RGBA texture both live in, filled on demand and uploaded by region;
//! * [`paint`]: a vertex list of anti-aliased shapes - rounded boxes with soft shadows,
//!   circles, rings, lines, text and icons in pixels, and ribbons and markers in a 3D
//!   world whose width can be given in metres, in pixels, or the larger of the two;
//! * [`gpu`]: the wgpu pipeline that draws such lists (MSAA, premultiplied alpha,
//!   rounded clipping), into a window or a texture;
//! * [`kit`] and [`theme`]: the immediate-mode widgets and their look, which draw the game's
//!   pause menu, photo mode and developer tools.

pub mod atlas;
pub mod edit;
pub mod gpu;
pub mod i18n;
pub mod icons;
pub mod kit;
pub mod paint;
pub mod text;
pub mod theme;

pub use atlas::{Atlas, Sprite};
pub use gpu::{Draw, Gpu, Layer, Vertices};
pub use i18n::tr;
pub use paint::{Color, Painter, Rect, Vertex};
pub use text::{Fonts, Weight};

pub mod performance;

pub mod profile_times;
pub mod units;
