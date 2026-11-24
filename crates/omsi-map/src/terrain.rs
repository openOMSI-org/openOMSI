//! `tile.map.terrain` height field (unit `mc_terrain_2`) and `.water`.

use crate::TERRAIN_SAMPLES;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Terrain {
    /// Grid resolution stored in the file (60 = 60 cells, 61 samples).
    pub cells: usize,
    /// Row-major heights, `samples × samples`, metres.
    pub heights: Vec<f32>,
}

#[derive(Debug, thiserror::Error)]
pub enum TerrainError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("terrain file too short: {0} bytes")]
    Short(usize),
}

impl Terrain {
    pub fn flat() -> Terrain {
        Terrain {
            cells: TERRAIN_SAMPLES - 1,
            heights: vec![0.0; TERRAIN_SAMPLES * TERRAIN_SAMPLES],
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<Terrain, TerrainError> {
        if bytes.len() < 4 {
            return Err(TerrainError::Short(bytes.len()));
        }
        let cells = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        // (no cells: nothing to sample between; `sample` clamps into 0..cells)
        if cells == 0 {
            return Err(TerrainError::Short(bytes.len()));
        }
        let samples = cells + 1;
        // a damaged header must not overflow the size it asks for
        let need = samples
            .checked_mul(samples)
            .and_then(|n| n.checked_mul(4))
            .and_then(|n| n.checked_add(4));
        let Some(need) = need.filter(|n| bytes.len() >= *n) else {
            return Err(TerrainError::Short(bytes.len()));
        };
        let heights = bytes[4..need]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        Ok(Terrain { cells, heights })
    }

    /// The file as OMSI writes it: the cell count, then the heights row by row.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + self.heights.len() * 4);
        out.extend_from_slice(&(self.cells as u32).to_le_bytes());
        for h in &self.heights {
            out.extend_from_slice(&h.to_le_bytes());
        }
        out
    }

    pub fn load(path: &Path) -> Result<Terrain, TerrainError> {
        Ok(Self::parse(&omsi_cfg::vfs::read(path)?)?)
    }

    pub fn samples(&self) -> usize {
        self.cells + 1
    }

    #[inline]
    pub fn height_at(&self, ix: usize, iy: usize) -> f32 {
        self.heights[iy * self.samples() + ix]
    }

    /// Height at tile-local coordinates (metres, 0..300) on the ground as drawn: each cell
    /// is two triangles split along the diagonal from its (i, j) to its (i+1, j+1) corner,
    /// as OMSI splits them (its `.map.terrain_0.rdy` vertex cache). Objects, people and
    /// wheels stand on that; a bilinear patch differs from it by centimetres on a curved
    /// slope and by half the step beside a kerb.
    pub fn sample(&self, x: f32, y: f32) -> f32 {
        let n = self.cells as f32;
        let fx = (x / crate::tile_size() as f32 * n).clamp(0.0, n - 1e-4);
        let fy = (y / crate::tile_size() as f32 * n).clamp(0.0, n - 1e-4);
        let (ix, iy) = (fx.floor() as usize, fy.floor() as usize);
        let (u, v) = (fx - ix as f32, fy - iy as f32);
        let h00 = self.height_at(ix, iy);
        let h10 = self.height_at(ix + 1, iy);
        let h01 = self.height_at(ix, iy + 1);
        let h11 = self.height_at(ix + 1, iy + 1);
        if u >= v {
            h00 + (h10 - h00) * u + (h11 - h10) * v
        } else {
            h00 + (h11 - h01) * u + (h01 - h00) * v
        }
    }

    /// Bilinear height at tile-local coordinates (metres, 0..300).
    pub fn sample_bilinear(&self, x: f32, y: f32) -> f32 {
        let n = self.cells as f32;
        let fx = (x / crate::tile_size() as f32 * n).clamp(0.0, n - 1e-4);
        let fy = (y / crate::tile_size() as f32 * n).clamp(0.0, n - 1e-4);
        let ix = fx.floor() as usize;
        let iy = fy.floor() as usize;
        let tx = fx - ix as f32;
        let ty = fy - iy as f32;
        let h00 = self.height_at(ix, iy);
        let h10 = self.height_at(ix + 1, iy);
        let h01 = self.height_at(ix, iy + 1);
        let h11 = self.height_at(ix + 1, iy + 1);
        (h00 * (1.0 - tx) + h10 * tx) * (1.0 - ty) + (h01 * (1.0 - tx) + h11 * tx) * ty
    }
}

/// `tile.map.water`: a count followed by four floats (water height per quadrant/edge).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Water {
    pub count: u32,
    pub values: Vec<f32>,
}

impl Water {
    pub fn parse(bytes: &[u8]) -> Water {
        if bytes.len() < 4 {
            return Water::default();
        }
        let count = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let values = bytes[4..]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        Water { count, values }
    }
}

/// `tile.map.prt`: precache list - scenery object file followed by min id, max id and -1.
pub fn parse_prt(text: &str) -> Vec<(String, i64, i64)> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim_end()).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 < lines.len() + 1 && i + 2 < lines.len() {
        let file = lines[i].to_string();
        if file.trim().is_empty() {
            i += 1;
            continue;
        }
        let a = omsi_cfg::parse_i64(lines[i + 1]);
        let b = omsi_cfg::parse_i64(lines[i + 2]);
        out.push((file, a, b));
        i += 4;
    }
    out
}

#[cfg(test)]
mod damaged_tests {
    #[test]
    fn a_terrain_of_no_cells_is_refused() {
        assert!(super::Terrain::parse(&[0, 0, 0, 0, 0, 0, 0, 0]).is_err());
    }
}
