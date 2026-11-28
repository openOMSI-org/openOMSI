//! `options.cfg` / `.oop` presets (unit `mc_form_options`).

use omsi_cfg::CfgFile;
use std::collections::BTreeMap;
use std::path::Path;

/// Options are kept as raw keyword → parameter lines so every option the original knows
/// round-trips even before it is interpreted.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Options {
    pub values: BTreeMap<String, Vec<String>>,
}

/// Number of parameter lines per option keyword (from the executable's option table).
pub fn option_arity(k: &str) -> usize {
    match k {
        "performance_dyn_redrefl" | "performance_dyn_tile_red" | "texfilter" | "texture" => 2,
        "smokesystems" => 4,
        "aimaxcountrandom" => 9,
        "useacttime"
        | "useactdate"
        | "useactyear"
        | "driverview_smooth"
        | "driverview_moving"
        | "altview"
        | "autocenter"
        | "no_collision"
        | "no_collision_terrain"
        | "no_collision_vehtoveh"
        | "no_collision_pedastrians"
        | "no_ticketinfo_visible"
        | "no_automaticclutch"
        | "no_schedanapopup"
        | "see_own_driver"
        | "nopreview"
        | "no_multithreading_calculate"
        | "no_multithreading_texload"
        | "loadalltiles"
        | "noautosave"
        | "showerrormessages"
        | "no_rain_refl"
        | "no_humans_on_rain_refl"
        | "no_stencilbuffer"
        | "sunglow"
        | "texmax256"
        | "texture_uselow"
        | "no_lightmap_terr"
        | "no_lightmap"
        | "no_nightmap"
        | "no_reflmap"
        | "no_bumpmap"
        | "gamectrleron"
        | "trackir_active"
        | "uselowailist"
        | "sound_ai"
        | "sound_scenery"
        | "sound_noreverb"
        | "no_tex_low_high_switch" => 0,
        _ => 1,
    }
}

impl Options {
    pub fn load(path: &Path) -> Result<Options, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        let mut o = Options::default();
        let mut r = f.reader();
        while let Some(k) = r.next_keyword() {
            let n = option_arity(&k);
            let v: Vec<String> = (0..n).map(|_| r.str().to_string()).collect();
            o.values.insert(k, v);
        }
        Ok(o)
    }

    pub fn flag(&self, k: &str) -> bool {
        self.values.contains_key(k)
    }

    pub fn str(&self, k: &str) -> Option<&str> {
        self.values
            .get(k)
            .and_then(|v| v.first())
            .map(|s| s.as_str())
    }

    pub fn f32(&self, k: &str, default: f32) -> f32 {
        self.str(k).map(omsi_cfg::parse_f32).unwrap_or(default)
    }

    pub fn i32(&self, k: &str, default: i32) -> i32 {
        self.str(k).map(omsi_cfg::parse_i32).unwrap_or(default)
    }
}
