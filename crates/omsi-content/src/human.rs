//! `.hum` humans (unit `mc_human`).

use omsi_cfg::CfgFile;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Human {
    pub path: PathBuf,
    pub model: String,
    pub seat_height: f32,
    pub feet_dist: f32,
    pub height: f32,
    pub links: Vec<f32>,
    pub voice: String,
    pub walk_param: [f32; 5],
    pub mass: f32,
    pub age: Option<i32>,
}

impl Human {
    pub fn load(path: &Path) -> Result<Human, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        let mut h = Human {
            path: f.path.clone(),
            walk_param: [1.4, 66.0, 1.0, 1.0, 0.0],
            ..Default::default()
        };
        let mut r = f.reader().disabled_blocks();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "model" => h.model = r.str().to_string(),
                "seatheight" => h.seat_height = r.f32(),
                "humangeom" => {
                    h.feet_dist = r.f32();
                    h.height = r.f32();
                }
                "links" => h.links = (0..22).map(|_| r.f32()).collect(),
                "voice" => h.voice = r.str().to_string(),
                "walk_param" => h.walk_param = r.f32s::<5>(),
                "mass" => h.mass = r.f32(),
                "age" => h.age = Some(r.i32()),
                _ => {}
            }
        }
        Ok(h)
    }
}
