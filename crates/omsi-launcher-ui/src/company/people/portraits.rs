//! The staff's portraits (Luc's 300 pictures, `assets/company/portraits`, 192 pixels square):
//! built into the launcher and decoded only when a face is first shown - a few a frame, so a
//! list of many people does not hold the window up - then kept with the launcher's other
//! pictures (`Launcher::icons`). Which face is whose is the company's (`staff::free_portraits`:
//! no two people share one).

use super::super::super::theme::*;
use super::super::super::ui::Ui;
use super::super::super::Launcher;
use omsi_ui::paint::Align;
use omsi_ui::{Rect, Weight};
use std::collections::HashMap;

/// The pictures, `portrait-001.jpg` first.
static FILES: [&[u8]; 300] = [
    include_bytes!("../../../../../assets/company/portraits/portrait-001.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-002.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-003.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-004.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-005.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-006.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-007.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-008.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-009.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-010.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-011.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-012.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-013.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-014.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-015.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-016.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-017.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-018.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-019.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-020.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-021.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-022.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-023.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-024.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-025.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-026.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-027.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-028.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-029.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-030.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-031.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-032.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-033.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-034.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-035.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-036.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-037.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-038.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-039.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-040.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-041.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-042.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-043.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-044.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-045.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-046.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-047.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-048.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-049.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-050.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-051.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-052.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-053.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-054.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-055.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-056.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-057.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-058.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-059.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-060.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-061.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-062.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-063.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-064.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-065.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-066.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-067.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-068.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-069.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-070.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-071.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-072.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-073.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-074.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-075.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-076.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-077.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-078.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-079.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-080.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-081.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-082.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-083.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-084.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-085.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-086.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-087.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-088.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-089.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-090.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-091.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-092.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-093.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-094.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-095.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-096.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-097.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-098.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-099.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-100.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-101.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-102.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-103.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-104.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-105.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-106.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-107.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-108.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-109.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-110.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-111.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-112.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-113.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-114.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-115.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-116.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-117.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-118.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-119.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-120.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-121.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-122.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-123.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-124.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-125.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-126.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-127.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-128.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-129.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-130.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-131.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-132.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-133.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-134.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-135.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-136.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-137.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-138.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-139.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-140.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-141.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-142.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-143.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-144.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-145.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-146.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-147.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-148.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-149.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-150.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-151.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-152.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-153.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-154.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-155.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-156.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-157.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-158.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-159.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-160.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-161.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-162.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-163.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-164.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-165.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-166.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-167.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-168.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-169.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-170.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-171.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-172.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-173.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-174.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-175.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-176.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-177.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-178.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-179.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-180.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-181.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-182.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-183.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-184.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-185.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-186.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-187.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-188.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-189.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-190.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-191.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-192.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-193.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-194.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-195.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-196.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-197.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-198.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-199.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-200.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-201.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-202.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-203.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-204.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-205.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-206.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-207.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-208.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-209.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-210.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-211.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-212.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-213.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-214.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-215.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-216.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-217.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-218.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-219.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-220.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-221.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-222.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-223.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-224.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-225.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-226.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-227.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-228.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-229.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-230.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-231.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-232.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-233.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-234.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-235.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-236.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-237.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-238.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-239.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-240.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-241.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-242.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-243.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-244.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-245.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-246.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-247.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-248.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-249.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-250.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-251.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-252.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-253.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-254.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-255.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-256.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-257.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-258.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-259.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-260.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-261.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-262.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-263.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-264.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-265.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-266.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-267.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-268.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-269.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-270.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-271.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-272.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-273.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-274.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-275.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-276.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-277.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-278.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-279.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-280.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-281.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-282.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-283.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-284.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-285.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-286.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-287.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-288.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-289.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-290.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-291.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-292.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-293.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-294.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-295.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-296.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-297.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-298.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-299.jpg"),
    include_bytes!("../../../../../assets/company/portraits/portrait-300.jpg"),
];

/// How many faces are decoded in a frame at most.
const PER_FRAME: usize = 6;

thread_local! {
    /// Faces that could not be decoded (not tried again every frame).
    static UNREAD: std::cell::RefCell<std::collections::HashSet<u32>> = Default::default();
}

/// The texture of face `n` (1 to 300): None until it is decoded and uploaded, a frame or two
/// after it is first asked for.
pub(super) fn texture(l: &mut Launcher, n: u32) -> Option<usize> {
    let key = format!("portrait:{n}");
    if let Some(t) = l.icons.get(&key) {
        return Some(*t);
    }
    let bytes = *FILES.get((n as usize).checked_sub(1)?)?;
    if l.icons_pending.iter().any(|p| p.0 == key) || UNREAD.with(|u| u.borrow().contains(&n)) {
        return None;
    }
    // (a few a frame: the next frame takes the next few)
    l.ui.keep_moving();
    if l.icons_pending.iter().filter(|p| p.0.starts_with("portrait:")).count() >= PER_FRAME {
        return None;
    }
    match image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg) {
        Ok(img) => l.icons_pending.push((key, img.to_rgba8())),
        Err(e) => {
            log::warn!("staff portrait {n}: {e}");
            UNREAD.with(|u| {
                u.borrow_mut().insert(n);
            });
        }
    }
    None
}

/// The textures of the faces `wanted` that are there (the others asked for).
pub(super) fn textures(l: &mut Launcher, wanted: impl IntoIterator<Item = u32>) -> HashMap<u32, usize> {
    let mut out = HashMap::new();
    for n in wanted {
        if let Some(t) = texture(l, n) {
            out.insert(n, t);
        }
    }
    out
}

/// A face in `r`: the picture once it is there, until then a calm square with the initials.
pub(super) fn draw(ui: &mut Ui, r: Rect, tex: Option<usize>, name: &str) {
    let radius = (r.w * 0.12).min(RADIUS);
    ui.p().rounded(r, radius, FIELD);
    match tex {
        Some(t) => ui.image(r, t, radius),
        None => {
            let initials: String = name.split_whitespace().filter_map(|w| w.chars().next()).filter(|c| c.is_uppercase()).take(2).collect();
            ui.text_in(&initials, r, r.h * 0.32, Weight::Bold, TEXT_DIM, Align::Center);
        }
    }
    ui.p().rounded_border(r, radius, 1.0, EDGE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_face_is_a_picture_of_its_own() {
        assert_eq!(FILES.len() as u32, omsi_launcher_lib::company::model::PORTRAITS);
        for (k, bytes) in FILES.iter().enumerate() {
            let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg).unwrap_or_else(|e| panic!("portrait {}: {e}", k + 1));
            assert!(img.width() >= 64 && img.width() == img.height(), "portrait {}", k + 1);
        }
    }
}
