//! When the enhanced sky cube and the reflection probe are redrawn: a slice of a face a
//! frame instead of a whole face every fourth, and the probe by the clock.

use crate::*;

/// Slices (bands of rows) a sky cube face is drawn in, one a frame.
pub(crate) const SKY_CUBE_SLICES: u32 = 4;
/// The most slices drawn in one frame (owed ones catch up two at a time).
const SLICES_PER_FRAME: u32 = 2;
/// The reflection probe is redrawn this often (seconds): every 30 frames it was, twice a
/// second at 60 fps and eight times at 240, for a sky that changes over minutes.
pub(crate) const PROBE_EVERY: f32 = 0.5;
/// A slice of the sky cube is drawn this often (seconds): 24 a second, a face every sixth
/// of a second, the whole cube in a second.
pub(crate) const SLICE_EVERY: f32 = 1.0 / 24.0;

/// Rows a slice of a face covers (the last one takes what is left).
pub(crate) fn slice_rows(first: u32, count: u32) -> (u32, u32) {
    let h = SKY_CUBE_SIZE.div_ceil(SKY_CUBE_SLICES);
    let y0 = (first * h).min(SKY_CUBE_SIZE);
    let y1 = ((first + count) * h).min(SKY_CUBE_SIZE);
    (y0, y1 - y0)
}

/// Slices of one face drawn in one pass, `count` slices from `first`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SkySlice {
    pub face: u32,
    pub first: u32,
    pub count: u32,
}

/// The sky cube's and probe's turns (see `Probe`): the cube is drawn a quarter of a face a
/// frame, the same face rate as a whole face every fourth frame without the one long frame
/// in four.
pub(crate) struct SkySched {
    /// Slices owed: one more each window frame, at most `1 / SLICE_EVERY` a second.
    pub owed: u32,
    /// The next slice of the face being redrawn (`Probe::cube_next`).
    pub slice: u32,
    /// Seconds since the probe was drawn.
    pub probe_since: f32,
    /// A new sky came in: the probe is redrawn on the next frame (not on the one taking
    /// the new sky's table in, which has that upload already).
    pub sky_new: bool,
    pub since_slice: f32,
}

impl Default for SkySched {
    fn default() -> Self {
        Self {
            owed: 0,
            slice: 0,
            probe_since: f32::INFINITY,
            sky_new: false,
            since_slice: SLICE_EVERY,
        }
    }
}

impl SkySched {
    /// This window frame's slices (one or two passes). `face` and `round` are the probe's
    /// `cube_next` and `cube_round`, moved on as faces are completed.
    pub(crate) fn plan(
        &mut self,
        face: &mut u32,
        round: &mut u32,
        dt: f32,
    ) -> [Option<SkySlice>; 2] {
        self.since_slice += dt.max(0.0);
        if self.since_slice >= SLICE_EVERY {
            self.since_slice = (self.since_slice - SLICE_EVERY).clamp(0.0, SLICE_EVERY);
            self.owed = (self.owed + 1).min(2 * SKY_CUBE_SLICES);
        }
        let n = self.owed.min(SLICES_PER_FRAME);
        self.owed -= n;
        let mut out: [Option<SkySlice>; 2] = [None; 2];
        let mut k = 0;
        for _ in 0..n {
            let s = SkySlice {
                face: *face,
                first: self.slice,
                count: 1,
            };
            self.slice += 1;
            if self.slice == SKY_CUBE_SLICES {
                self.slice = 0;
                *face = (*face + 1) % 6;
                *round = round.wrapping_add(1);
            }
            // (the next band of the same face goes into the same pass)
            let joins = k > 0
                && out[k - 1].is_some_and(|p| p.face == s.face && p.first + p.count == s.first);
            if joins {
                if let Some(p) = out[k - 1].as_mut() {
                    p.count += 1;
                }
            } else {
                out[k] = Some(s);
                k += 1;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A slice every frame, every face as often as before (one in four frames); owed
    /// slices (a frame that drew none) catch up two at a time, in one pass.
    #[test]
    fn a_slice_a_frame() {
        let mut s = SkySched::default();
        let (mut face, mut round) = (0, 0);
        for n in 0..96u32 {
            let out = s.plan(&mut face, &mut round, 0.05);
            assert_eq!(
                out.iter().flatten().map(|p| p.count).sum::<u32>(),
                1,
                "frame {n}"
            );
        }
        assert_eq!((face, round), (0, 24));
        s.owed += 1;
        let out = s.plan(&mut face, &mut round, 0.05);
        assert_eq!(out[0].map(|p| p.count), Some(2));
        assert!(out[1].is_none(), "one pass: {out:?}");
    }

    #[test]
    fn at_most_twenty_four_slices_a_second() {
        for fps in [30.0f32, 60.0, 59.0, 120.0, 144.0, 240.0] {
            let mut s = SkySched::default();
            let (mut face, mut round) = (0, 0);
            let frames = (fps * 10.0) as u32;
            let mut slices = 0;
            for _ in 0..frames {
                slices += s
                    .plan(&mut face, &mut round, 1.0 / fps)
                    .iter()
                    .flatten()
                    .map(|p| p.count)
                    .sum::<u32>();
            }
            let want = frames.min(240);
            assert!(
                slices.abs_diff(want) <= 2,
                "{fps} fps: {slices} slices in 10 s, {want} wanted"
            );
        }
    }

    #[test]
    fn slices_cover_the_face() {
        let mut rows = 0;
        for k in 0..SKY_CUBE_SLICES {
            let (y0, h) = slice_rows(k, 1);
            assert_eq!(y0, rows);
            rows += h;
        }
        assert_eq!(rows, SKY_CUBE_SIZE);
        assert_eq!(slice_rows(1, 2), (SKY_CUBE_SIZE / 4, SKY_CUBE_SIZE / 2));
    }
}
