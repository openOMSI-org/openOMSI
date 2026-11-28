//! Reading picture files for the simulation: the bitmaps a script loads into its script
//! texture (`STLoadTex`), the font sheets of the text textures, the pictures of the HTML
//! pages, the look of a paint scheme's stand-in texture.
//!
//! The simulation does not decode pictures itself: that is the presentation's texture
//! loading (omsi-texture), which the simulation must not depend on. The game registers its
//! decoder once at start ([`install`]); everything here asks it through [`decode`]. The
//! pictures only come back as RGBA, which is all the simulation reads of them.

use std::path::Path;
use std::sync::OnceLock;

/// A decoded picture: RGBA8, row-major, top-left origin.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// True when the source had an alpha channel (bmp/jpg have none).
    pub has_alpha: bool,
}

/// Reads a picture file (bmp/tga/dds/jpg/png, from the disk or an archive).
pub type Decoder = fn(&Path) -> Result<DecodedImage, String>;

static DECODER: OnceLock<Decoder> = OnceLock::new();

/// Register the decoder the simulation reads pictures with. The first one stays: a second
/// call (another entry point of the same process) changes nothing.
pub fn install(decoder: Decoder) {
    let _ = DECODER.set(decoder);
}

/// Whether a decoder is registered.
pub fn installed() -> bool {
    DECODER.get().is_some()
}

/// Read the picture at `path` with the registered decoder. Without one every picture is
/// missing (and that is said once in the log).
pub fn decode(path: &Path) -> Result<DecodedImage, String> {
    // (the crate's own tests read pictures with the game's decoder, a dev-dependency)
    #[cfg(test)]
    install(texture_decoder);
    match DECODER.get() {
        Some(d) => d(path),
        None => {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| {
                log::warn!(
                    "no picture decoder registered: {} and every other picture stay empty",
                    path.display()
                )
            });
            Err("no picture decoder registered".to_string())
        }
    }
}

/// The game's decoder (omsi-texture), as the game registers it, for the tests.
#[cfg(test)]
fn texture_decoder(path: &Path) -> Result<DecodedImage, String> {
    omsi_texture::decode_file(path)
        .map(|i| DecodedImage {
            width: i.width,
            height: i.height,
            rgba: i.rgba,
            has_alpha: i.has_alpha,
        })
        .map_err(|e| e.to_string())
}
