//! What the game hands the scene: the settings it reads while it loads (the graphics' kind,
//! the scenery's volume, the mirrors' size, how many cars may park), and the decoders the
//! simulation draws its pictures with. The game sets these at its start; a world a test or
//! a tool opens takes the defaults.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering::Relaxed};

// The settings below are set by the game at every session start and read while worlds and
// vehicles load, where no game state is at hand.

/// The graphics' kind a scene is drawn in (the game's settings and arguments; a world a test
/// or a tool opens: Vanilla+).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GraphicsKind {
    #[default]
    VanillaPlus,
    /// Enhanced graphics (from the settings, `--enhanced`, or OMSI_ENHANCED=1).
    Enhanced,
    /// Vanilla graphics: the picture as OMSI 2 draws it (no Vanilla+ extras, see
    /// `omsi_render::Lighting::classic`).
    Classic,
}

/// The game's settings and the session's weather as a world's loading code and sounds read
/// them: each `World` holds its own (`World::settings`), the game sets them when the session
/// starts and again when they change; a world a test or a tool opens keeps the defaults.
/// (Atomics: the world is shared with the loading threads, and the game changes them while
/// it plays.)
#[derive(Debug)]
pub struct SceneSettings {
    /// Volume of the scenery's own sounds (OMSI's `sound_scenery`), as the bits of an f32.
    sound_scenery: AtomicU32,
    /// Edge of a mirror's picture (the `mirror_size` setting, OMSI's reflTexSize).
    mirror_size: AtomicU32,
    /// The `rain_quality` setting: 2 high, 1 medium, 0 low (see `rain::quality_of`).
    rain_quality: AtomicU8,
    /// The session's weather lies as snow (`[snow]` in the `.owt`): the vehicles' panes wear
    /// it as they go onto the GPU.
    snow: AtomicBool,
}

impl Default for SceneSettings {
    fn default() -> SceneSettings {
        SceneSettings { sound_scenery: AtomicU32::new(1.0f32.to_bits()), mirror_size: AtomicU32::new(256), rain_quality: AtomicU8::new(2), snow: AtomicBool::new(false) }
    }
}

impl SceneSettings {
    pub fn sound_scenery(&self) -> f32 {
        f32::from_bits(self.sound_scenery.load(Relaxed))
    }

    pub fn set_sound_scenery(&self, v: f32) {
        self.sound_scenery.store(v.to_bits(), Relaxed);
    }

    pub fn mirror_size(&self) -> u32 {
        self.mirror_size.load(Relaxed)
    }

    pub fn set_mirror_size(&self, v: u32) {
        self.mirror_size.store(v, Relaxed);
    }

    pub fn rain_quality(&self) -> u8 {
        self.rain_quality.load(Relaxed)
    }

    pub fn set_rain_quality(&self, v: u8) {
        self.rain_quality.store(v, Relaxed);
    }

    pub fn snow(&self) -> bool {
        self.snow.load(Relaxed)
    }

    pub fn set_snow(&self, v: bool) {
        self.snow.store(v, Relaxed);
    }
}

/// The settings a world reads when it opens.
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenSettings {
    /// Parked AI cars at most (`ai_max_parked`: 0 no limit, -1 none).
    pub max_parked: i64,
}

// (an install-once hook: the game's settings file is the process's, read when a world opens)
static OPEN_SETTINGS: std::sync::OnceLock<fn() -> OpenSettings> = std::sync::OnceLock::new();

/// Where a world opening reads the game's settings from (the game's settings file; set once,
/// a second call changes nothing).
pub fn install_open_settings(read: fn() -> OpenSettings) {
    let _ = OPEN_SETTINGS.set(read);
}

/// The game's settings as a world opening now reads them (the defaults without the game).
pub fn open_settings() -> OpenSettings {
    OPEN_SETTINGS.get().map(|read| read()).unwrap_or_default()
}

/// Register what the simulation draws and reads its pictures with, which omsi-sim does not
/// depend on: the picture decoder (`STLoadTex`, the text textures' font sheets, the HTML
/// pages' pictures; see `omsi_sim::image_decode`), the presentation's texture loading, and
/// the `[htmltexture]` pages' engine (omsi-htmlengine, or omsi-htmlpage for a page that asks for it, see `omsi_sim::htmltex::set_backend`).
/// Every entry point calls it before anything is loaded (a second call changes nothing).
pub fn install_sim_decoder() {
    omsi_sim::image_decode::install(|path| {
        omsi_texture::decode_file(path)
            .map(|i| omsi_sim::image_decode::DecodedImage { width: i.width, height: i.height, rgba: i.rgba, has_alpha: i.has_alpha })
            .map_err(|e| e.to_string())
    });
    // (the classic engine runs every page unless the page asks for the modern one)
    omsi_sim::htmltex::set_backend(|width, height, html, api| {
        if omsi_htmlpage::wanted(html) {
            Box::new(omsi_htmlpage::EngineRenderer::with_api(width, height, html, api))
        } else {
            Box::new(omsi_htmlengine::EngineRenderer::with_api(width, height, html, api))
        }
    });
}
