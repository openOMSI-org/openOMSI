//! The relief of a map's package: contour lines of the ground, classes 5 and 6 of the lines of `surface.oms`
//! (`width` is the height above sea level in metres, every fifth line is an index line), and where their heights are
//! written on the city map. Optional: a package without them has no terrain button.

use super::{Package, Rd};

/// A contour line of the ground and an index contour line (every fifth, drawn stronger): `width` is the height.
pub const LINE_CONTOUR: u8 = 5;
pub const LINE_CONTOUR_INDEX: u8 = 6;

impl Package {
    /// Whether a chunk has contour lines (without reading its triangles).
    fn chunk_has_contours(body: &[u8]) -> bool {
        let mut r = Rd { b: body, i: 0 };
        let scan = |r: &mut Rd| -> Option<bool> {
            for _ in 0..r.u32()? {
                r.u8()?;
                let n = r.u32()? as usize;
                r.take(n.checked_mul(24)?)?;
            }
            for _ in 0..r.u32()? {
                let class = r.u8()?;
                r.u8()?;
                r.f32()?;
                r.u32()?;
                let n = r.u32()? as usize;
                if class == LINE_CONTOUR || class == LINE_CONTOUR_INDEX {
                    return Some(true);
                }
                r.take(n.checked_mul(8)?)?;
            }
            Some(false)
        };
        scan(&mut r).unwrap_or(false)
    }

    /// Looks for contour lines, called once by `open`.
    pub(super) fn load_relief(&mut self) {
        self.has_contours = self.index.iter().any(|e| self.data.get(e.at..e.at + e.len).is_some_and(Self::chunk_has_contours));
    }

    /// The package carries contour lines (the city map then has a button for them).
    pub fn has_contours(&self) -> bool {
        self.has_contours
    }

    /// Metres added to the height of every contour line when the game says it (the manifest's `terrain_offset`).
    pub fn terrain_offset(&self) -> f32 {
        self.terrain_offset
    }

    /// Where the heights of the contours are written on the city map: a point (world metres), the height (with the
    /// offset) and whether the line is an index one; about every 700 m along an index line and every 450 m along the
    /// others. Made on first use.
    pub fn contour_marks(&self) -> &[((f64, f64), i32, bool)] {
        self.contour_marks.get_or_init(|| {
            let mut out = Vec::new();
            for e in &self.index {
                let Some(body) = self.data.get(e.at..e.at + e.len) else { continue };
                let mut layers = super::Layers::default();
                let _ = self.walk(body, &mut layers);
                for l in &layers.contours {
                    let index = l.class == LINE_CONTOUR_INDEX;
                    let (gap, mut run) = (if index { 700.0 } else { 450.0 }, 0.0f64);
                    let mut next = if index { 150.0 } else { 100.0 };
                    for s in l.points.windows(2) {
                        run += ((s[1].0 - s[0].0).powi(2) + (s[1].1 - s[0].1).powi(2)).sqrt();
                        if run >= next {
                            out.push((((s[0].0 + s[1].0) * 0.5, (s[0].1 + s[1].1) * 0.5), (l.width + self.terrain_offset).round() as i32, index));
                            next = run + gap;
                        }
                    }
                }
            }
            out
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of one chunk with one contour line of 10 m and one index line of 50 m (each 1 km long), and a river.
    fn with_contours() -> Package {
        let mut body = Vec::new();
        body.extend(0u32.to_le_bytes());
        body.extend(3u32.to_le_bytes());
        for (class, h, y) in [(LINE_CONTOUR, 10.0f32, 0.0f32), (LINE_CONTOUR_INDEX, 50.0, 40.0), (super::super::LINE_RIVER, 3.0, 80.0)] {
            body.extend([class, 0]);
            body.extend(h.to_le_bytes());
            body.extend(u32::MAX.to_le_bytes());
            body.extend(3u32.to_le_bytes());
            for v in [0.0f32, y, 500.0, y, 1000.0, y] {
                body.extend(v.to_le_bytes());
            }
        }
        body.extend(0u32.to_le_bytes());
        let mut f = Vec::new();
        f.extend(super::super::MAGIC);
        f.extend(super::super::VERSION.to_le_bytes());
        f.extend(1000.0f64.to_le_bytes());
        f.extend(2000.0f64.to_le_bytes());
        for v in [0.0f32, 1000.0, 0.0, 1000.0, 2000.0] {
            f.extend(v.to_le_bytes());
        }
        f.extend(1u32.to_le_bytes());
        f.extend(4u16.to_le_bytes());
        f.extend(b"Test");
        f.extend(1u32.to_le_bytes());
        let at = f.len() + 20;
        f.extend(0i32.to_le_bytes());
        f.extend(0i32.to_le_bytes());
        f.extend((at as u64).to_le_bytes());
        f.extend((body.len() as u32).to_le_bytes());
        f.extend(body);
        let mut p = Package::parse(f, "Test".into(), String::new(), super::super::Palette::dark(), super::super::Palette::light(), 300.0).expect("valid");
        p.load_relief();
        p
    }

    #[test]
    fn contours_are_read_apart_from_the_other_lines() {
        let p = with_contours();
        assert!(p.has_contours());
        let l = p.layers(None);
        assert_eq!(l.lines.len(), 1, "the river only");
        assert_eq!(l.contours.iter().map(|c| (c.class, c.width as i32)).collect::<Vec<_>>(), vec![(LINE_CONTOUR, 10), (LINE_CONTOUR_INDEX, 50)]);
    }

    #[test]
    fn heights_are_written_along_the_lines_with_the_offset() {
        let mut p = with_contours();
        p.terrain_offset = 2.0;
        let marks = p.contour_marks();
        // 1 km of an index line: a mark at 150 m, the next 700 m later is past the end; 1 km of a plain one: 100 m, then 450 m later
        assert_eq!(marks.iter().filter(|m| m.2).count(), 1);
        assert!(marks.iter().filter(|m| !m.2).count() >= 2);
        assert!(marks.iter().all(|m| m.1 == if m.2 { 52 } else { 12 }), "{marks:?}");
    }

    #[test]
    fn a_package_without_contours_has_none() {
        assert!(!Package::test_stub().has_contours());
        assert!(Package::test_stub().contour_marks().is_empty());
    }
}
