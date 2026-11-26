//! openOMSI's PBR factor keywords of a material (see `MaterialDef::base_color` and the
//! rest): a value out of its range is taken to the nearer end.

use super::Model;
use omsi_cfg::CfgReader;

/// A 0..1 value of a material keyword: out of range taken to the nearer end, a line that is
/// no number 0 (as the format reads any number).
fn unit(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

impl Model {
    /// One of the factor keywords and its lines (`key` without its brackets); false for
    /// another keyword.
    pub(crate) fn read_factor(&mut self, key: &str, r: &mut CfgReader) -> bool {
        match key {
            "matl_basecolor" => {
                // (openOMSI: the PBR factors, see `MaterialDef::base_color` and the rest;
                // a value out of its range is taken to the nearer end)
                let v = r.f32s::<3>().map(|c| unit(c));
                if let Some(m) = self.cur_matl() {
                    m.base_color = Some(v);
                }
            }
            "matl_metallic" => {
                let v = unit(r.f32());
                if let Some(m) = self.cur_matl() {
                    m.metallic = Some(v);
                }
            }
            "matl_roughness" => {
                let v = unit(r.f32());
                if let Some(m) = self.cur_matl() {
                    m.roughness = Some(v);
                }
            }
            "matl_normal_strength" => {
                let v = r.f32();
                let v = if v.is_finite() {
                    v.clamp(0.0, 4.0)
                } else {
                    1.0
                };
                if let Some(m) = self.cur_matl() {
                    m.normal_strength = Some(v);
                }
            }
            "matl_occlusion_strength" => {
                let v = unit(r.f32());
                if let Some(m) = self.cur_matl() {
                    m.occlusion_strength = Some(v);
                }
            }
            _ => return false,
        }
        true
    }
}
