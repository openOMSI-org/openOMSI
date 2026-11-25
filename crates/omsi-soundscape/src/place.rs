//! What stands around the listener, as the ambience hears it: read from the tiles' sound
//! data (`omsi_geometry::GroundSound`), which the map's own geometry fills - the houses, the
//! trees, the lanes and their speed limits, the tracks, the wires, the water - and the
//! objects the catalogue knows by what their authors filed them as.

use super::catalog::Spot;
use glam::DVec3;
use omsi_geometry::LineKind;

/// Around the listener (see the game's `World::sound_place`).
#[derive(Debug, Clone, Default)]
pub struct Place {
    /// How built-up (0 … 1): houses within some 150 m.
    pub urban: f32,
    /// Their mean height (m).
    pub height: f32,
    /// Leafy trees and conifers around (0 … 1 each), weighted like a source: a crown's size
    /// over its distance squared.
    pub deciduous: f32,
    pub conifer: f32,
    /// Neither houses nor trees (0 … 1): a field, a meadow.
    pub open: f32,
    /// The nearest water the map has (distance, where).
    pub water: Option<(f32, DVec3)>,
    /// How much water there is within some 150 m (0 … 1): a canal a little, a lake much.
    pub water_amount: f32,
    /// How much of the far roads reaches the listener (lanes with their speed, over their
    /// distance; 1 is a motorway some 300 m off), and from which way.
    pub fast_roads: f32,
    pub fast_roads_from: DVec3,
    /// The same for the ordinary roads farther than the traffic one hears car by car.
    pub far_roads: f32,
    /// The nearest road lane (m): a courtyard lies well back from the street.
    pub road: Option<f32>,
    /// Railway tracks within 40 m (a bundle of them is a yard or a station), and the nearest
    /// track (distance, where).
    pub tracks: u32,
    pub rail: Option<(f32, DVec3)>,
    /// The nearest overhead wire (m).
    pub wire: Option<f32>,
    /// Inside a tunnel's tube.
    pub tunnel: bool,
    /// The known objects in earshot: what, where, the map's id.
    pub spots: Vec<(Spot, DVec3, i64)>,
}

impl Place {
    /// Whether one of the map's own sound objects of `kind` is within `reach` metres.
    pub fn map_has(&self, kind: Spot, ear: DVec3, reach: f64) -> bool {
        self.spots
            .iter()
            .any(|(k, p, _)| *k == kind && (*p - ear).length() < reach)
    }

    pub fn near(&self, kind: Spot) -> impl Iterator<Item = &(Spot, DVec3, i64)> {
        self.spots.iter().filter(move |(k, _, _)| *k == kind)
    }
}

/// Sums what the tiles around a listener hold (tile-local data with each tile's corner).
#[derive(Default)]
pub struct Gather {
    ear: DVec3,
    houses: f64,
    water_cells: f64,
    height_sum: f64,
    deciduous: f64,
    conifer: f64,
    fast: f64,
    fast_dir: DVec3,
    far: f64,
    tracks: hashbrown::HashSet<usize>,
    line_index: usize,
    place: Place,
}

/// Trees farther than this are not heard (m).
const TREE_REACH: f64 = 80.0;
/// Houses counted for how built-up a place is (m).
const HOUSE_REACH: f64 = 150.0;
/// Known objects heard from this far (m; the church bells carry farthest).
pub const SPOT_REACH: f64 = 900.0;
/// A lane nearer than this is the traffic one hears car by car, not the far roar (m).
const ROAD_NEAR: f64 = 120.0;
const ROAD_REACH: f64 = 1200.0;

impl Gather {
    pub fn new(ear: DVec3) -> Gather {
        Gather {
            ear,
            ..Default::default()
        }
    }

    /// One tile's sound data, with the world position of its corner.
    pub fn tile(&mut self, g: &omsi_geometry::GroundSound, corner: DVec3) {
        let ear = self.ear;
        let at = |p: [f32; 3]| corner + DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
        for t in &g.trees {
            let d = DVec3::new(
                corner.x + t[0] as f64 - ear.x,
                corner.y + t[1] as f64 - ear.y,
                0.0,
            );
            let d2 = d.length_squared();
            if d2 > TREE_REACH * TREE_REACH {
                continue;
            }
            // a 10 m tree 10 m away counts 0.1: a park of a dozen around is near 1
            let w = (t[2] as f64).clamp(2.0, 30.0).powi(2) / (d2 + 25.0) * 0.1;
            if t[3] > 0.5 {
                self.conifer += w;
            } else {
                self.deciduous += w;
            }
        }
        for b in &g.buildings {
            let d = DVec3::new(
                corner.x + b[0] as f64 - ear.x,
                corner.y + b[1] as f64 - ear.y,
                0.0,
            )
            .length();
            if d < HOUSE_REACH {
                // (nearer houses shape the place more)
                let w = 1.0 - d / HOUSE_REACH * 0.5;
                self.houses += w;
                self.height_sum += w * b[2] as f64;
            }
        }
        for s in &g.spots {
            let Some(kind) = Spot::from_u16(s.kind) else {
                continue;
            };
            let p = at(s.pos);
            if (p - ear).length() < SPOT_REACH {
                self.place.spots.push((kind, p, s.id));
            }
        }
        for w in &g.water {
            let p = corner + DVec3::new(w[0] as f64, w[1] as f64, ear.z);
            let d = (DVec3::new(p.x - ear.x, p.y - ear.y, 0.0)).length() as f32;
            if d < 150.0 {
                self.water_cells += 1.0;
            }
            if self.place.water.is_none_or(|(best, _)| d < best) {
                self.place.water = Some((d, p));
            }
        }
        for line in &g.lines {
            self.line_index += 1;
            match line.kind {
                LineKind::Road => self.road(line, &at),
                LineKind::Rail => {
                    for p in line.points.iter().map(|p| at(*p)) {
                        let d = (p - ear).length() as f32;
                        if d < 40.0 {
                            self.tracks.insert(self.line_index);
                        }
                        if self.place.rail.is_none_or(|(best, _)| d < best) {
                            self.place.rail = Some((d, p));
                        }
                    }
                }
                LineKind::Wire => {
                    for p in line.points.iter().map(|p| at(*p)) {
                        let d = (p - ear).length() as f32;
                        if self.place.wire.is_none_or(|best| d < best) {
                            self.place.wire = Some(d);
                        }
                    }
                }
                LineKind::Tunnel => {
                    for p in line.points.iter().map(|p| at(*p)) {
                        let flat = DVec3::new(p.x - ear.x, p.y - ear.y, 0.0).length();
                        if flat < 7.0 && (p.z - ear.z).abs() < 7.0 {
                            self.place.tunnel = true;
                        }
                    }
                }
            }
        }
    }

    /// A road lane: the far roar of what drives on it, the faster the louder (tyre noise
    /// rises some 30 dB per tenfold speed, so with the speed cubed), over its distance (a
    /// line source falls off with the distance itself, not its square).
    fn road(&mut self, line: &omsi_geometry::SoundLine, at: &impl Fn([f32; 3]) -> DVec3) {
        let ear = self.ear;
        let v = (line.speed.max(30.0) / 100.0) as f64;
        for w in line.points.windows(2) {
            let p = at(w[0]);
            let d = (p - ear).length();
            if self.place.road.is_none_or(|best| (d as f32) < best) {
                self.place.road = Some(d as f32);
            }
            if !(ROAD_NEAR..ROAD_REACH).contains(&d) {
                continue;
            }
            let seg = (at(w[1]) - p).length();
            // (a motorway lane 300 m off: 0.25 per 100 m of it seen)
            let w = seg / 100.0 * v.powi(3) * 300.0 / d * 0.25;
            if line.speed >= 70.0 {
                self.fast += w;
                self.fast_dir += (p - ear).normalize_or_zero() * w;
            } else {
                self.far += w * 0.5;
            }
        }
    }

    pub fn finish(mut self) -> Place {
        let mut p = std::mem::take(&mut self.place);
        p.urban = (1.0 - (-self.houses / 25.0).exp()) as f32;
        // (a cell is some 20 m square: a lake of a few hectares is near 1)
        p.water_amount = (1.0 - (-self.water_cells / 40.0).exp()) as f32;
        p.height = if self.houses > 0.0 {
            (self.height_sum / self.houses) as f32
        } else {
            0.0
        };
        p.deciduous = (1.0 - (-self.deciduous).exp()) as f32;
        p.conifer = (1.0 - (-self.conifer).exp()) as f32;
        p.open = ((1.0 - p.urban) * (1.0 - p.deciduous.max(p.conifer))).clamp(0.0, 1.0);
        // (four lanes at 120 km/h along a kilometre 300 m off sum to some 17, a country road
        // of two lanes at 100 km/h to some 5)
        p.fast_roads = (1.0 - (-self.fast / 12.0).exp()) as f32;
        p.fast_roads_from = self.fast_dir.normalize_or_zero();
        p.far_roads = (1.0 - (-self.far / 25.0).exp()) as f32;
        p.tracks = self.tracks.len() as u32;
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_geometry::{GroundSound, SoundLine, SoundSpot};

    #[test]
    fn a_town_a_wood_a_field() {
        let ear = DVec3::new(150.0, 150.0, 0.0);
        let mut town = GroundSound::default();
        for i in 0..40 {
            town.buildings.push([
                100.0 + (i % 8) as f32 * 15.0,
                100.0 + (i / 8) as f32 * 20.0,
                18.0,
            ]);
        }
        let mut g = Gather::new(ear);
        g.tile(&town, DVec3::ZERO);
        let p = g.finish();
        assert!(p.urban > 0.6 && p.height > 15.0 && p.open < 0.4, "{p:?}");

        let mut wood = GroundSound::default();
        for i in 0..60 {
            wood.trees.push([
                120.0 + (i % 8) as f32 * 8.0,
                120.0 + (i / 8) as f32 * 8.0,
                20.0,
                1.0,
            ]);
        }
        let mut g = Gather::new(ear);
        g.tile(&wood, DVec3::ZERO);
        let p = g.finish();
        assert!(
            p.conifer > 0.9 && p.deciduous == 0.0 && p.open < 0.1,
            "{p:?}"
        );

        let p = Gather::new(ear).finish();
        assert_eq!(p.open, 1.0);
    }

    #[test]
    fn a_motorway_is_heard_from_its_side_and_a_church_from_afar() {
        let ear = DVec3::new(0.0, 0.0, 0.0);
        let mut g = GroundSound::default();
        // two lanes each way 300 m to the east, 1 km long
        for _ in 0..4 {
            g.lines.push(SoundLine {
                kind: LineKind::Road,
                speed: 120.0,
                points: (0..=100)
                    .map(|k| [300.0, -500.0 + k as f32 * 10.0, 0.0])
                    .collect(),
            });
        }
        g.spots.push(SoundSpot {
            kind: Spot::ChurchCatholic as u16,
            pos: [-600.0, 0.0, 0.0],
            id: 7,
        });
        let mut gather = Gather::new(ear);
        gather.tile(&g, DVec3::ZERO);
        let p = gather.finish();
        assert!(p.fast_roads > 0.5, "{}", p.fast_roads);
        assert!(
            p.fast_roads_from.x > 0.9,
            "from the east: {:?}",
            p.fast_roads_from
        );
        assert_eq!(p.near(Spot::ChurchCatholic).count(), 1);
    }
}
