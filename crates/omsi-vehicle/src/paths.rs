//! `paths.cfg` - passenger walking paths inside a vehicle.

use omsi_cfg::CfgFile;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PathPoint {
    pub pos: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VehiclePaths {
    pub step_sound_packs: Vec<Vec<String>>,
    pub points: Vec<PathPoint>,
    /// (a, b, one_way)
    pub links: Vec<(i32, i32, bool)>,
    /// Each link's `[stepsoundpack]` (index into `step_sound_packs`; -1 = no steps heard)
    /// and room height: the `[next_stepsound]` / `[next_roomheight]` in force where the
    /// link is read. Omsi.exe (paths.cfg parser 0x724300) keeps both with the links, not
    /// the points, and starts the step sound at -1.
    pub link_step_sound: Vec<i32>,
    pub link_room_height: Vec<f32>,
}

impl VehiclePaths {
    pub fn load(path: &Path) -> Result<VehiclePaths, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        Ok(Self::parse(&f))
    }

    pub fn parse(f: &CfgFile) -> VehiclePaths {
        let mut p = VehiclePaths::default();
        let mut room_height = 2.0;
        let mut step_sound = -1;
        let mut r = f.reader().disabled_blocks();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "stepsoundpack" => {
                    let n = r.count();
                    p.step_sound_packs
                        .push((0..n).map(|_| r.str().to_string()).collect());
                }
                "next_roomheight" => room_height = r.f32(),
                "next_stepsound" => step_sound = r.i32(),
                "pathpnt" => p.points.push(PathPoint { pos: r.f32s::<3>() }),
                "pathlink" | "pathlink_oneway" => {
                    let a = r.i32();
                    let b = r.i32();
                    p.links.push((a, b, k == "pathlink_oneway"));
                    p.link_step_sound.push(step_sound);
                    p.link_room_height.push(room_height);
                }
                _ => {}
            }
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The SD200's layout: every point before the first `[next_stepsound]`, the links
    /// after it - the step sound goes with the links read after it, -1 before any.
    #[test]
    fn step_sounds_go_with_the_links() {
        let text = "[stepsoundpack]\n1\nStep_01.wav\n\n[stepsoundpack]\n1\nStep_OV_01.wav\n\n[stepsoundpack]\n1\nStep_St_01.wav\n\n\
                    [pathpnt]\n0\n0\n0\n\n[pathpnt]\n0\n1\n0\n\n[pathpnt]\n0\n2\n0\n\n\
                    [pathlink]\n0\n1\n\n[next_stepsound]\n2\n\n[pathlink_oneway]\n1\n2\n\n[next_stepsound]\n0\n[pathlink]\n2\n0\n";
        let f = CfgFile::from_str("paths.cfg", text);
        let p = VehiclePaths::parse(&f);
        assert_eq!(p.step_sound_packs.len(), 3);
        assert_eq!(p.links, vec![(0, 1, false), (1, 2, true), (2, 0, false)]);
        assert_eq!(p.link_step_sound, vec![-1, 2, 0]);
        assert_eq!(p.link_room_height, vec![2.0; 3]);
    }
}
