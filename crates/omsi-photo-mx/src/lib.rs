//! The photo mode's camera and darkroom, apart from the game: `lens` turns the photo camera's
//! settings into the many pictures one photo is made of, and `develop` adds them up in linear
//! light and develops the sum into the picture shown and saved. The app drives them (the
//! panel, the renderer's pictures, saving); nothing here knows the simulation or the window.

pub mod develop;
pub mod exposure;
pub mod lens;
pub mod optics;

/// `px` (`w` x `h`) stretched to `tw` x `th` (the nearest pixel), for the input script's
/// pictures of the window.
pub fn stretch(w: u32, h: u32, px: &[u8], tw: u32, th: u32) -> Vec<u8> {
    let mut out = vec![0u8; (tw * th * 4) as usize];
    for y in 0..th {
        let sy = (y as u64 * h as u64 / th.max(1) as u64) as u32;
        for x in 0..tw {
            let sx = (x as u64 * w as u64 / tw.max(1) as u64) as u32;
            let a = ((sy * w + sx) * 4) as usize;
            let b = ((y * tw + x) * 4) as usize;
            out[b..b + 4].copy_from_slice(&px[a..a + 4]);
        }
    }
    out
}

/// Keep bus instances first for wheel animation and avoid moving shared instances twice.
pub fn motion_parts(bus: &[usize], trailers: &[usize], aboard: &[usize]) -> Vec<usize> {
    let mut seen = std::collections::HashSet::new();
    bus.iter().chain(trailers).chain(aboard).copied().filter(|i| seen.insert(*i)).collect()
}

#[cfg(test)]
mod tests {
    use super::motion_parts;

    #[test]
    fn a_rolling_shot_moves_the_bus_its_trailers_and_the_riders_once_each_in_that_order() {
        assert_eq!(motion_parts(&[3, 4, 5], &[9, 10], &[20, 21, 22]), [3, 4, 5, 9, 10, 20, 21, 22]);
        assert_eq!(motion_parts(&[3, 4], &[4, 9], &[9, 20, 3]), [3, 4, 9, 20]);
        assert_eq!(motion_parts(&[1, 2], &[], &[]), [1, 2]);
        assert!(motion_parts(&[], &[], &[]).is_empty());
        let parts = motion_parts(&[7, 8, 9], &[], &[1, 2]);
        assert_eq!(&parts[..3], &[7, 8, 9]);
    }
}
