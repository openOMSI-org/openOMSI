//! Pure pointer movement and hit-reach geometry used by the application.

pub fn look_deg_per_px(fov_deg: f32) -> f32 {
    fov_deg / 78.75
}

pub fn drag_heading(
    press: [f32; 2],
    release: [f32; 2],
    from: glam::DVec2,
    to: Option<glam::DVec2>,
) -> Option<f64> {
    let d = to.filter(|_| (release[0] - press[0]).hypot(release[1] - press[1]) > 12.0)? - from;
    Some(d.x.atan2(d.y).to_degrees())
}

pub fn part_in_reach(
    eye: glam::DVec3,
    at: glam::DVec3,
    heading: f64,
    bb: Option<[f32; 6]>,
) -> bool {
    let bb = bb.unwrap_or([2.5, 12.0, 3.0, 0.0, 0.0, 1.5]);
    let h = heading.to_radians();
    let (fwd, right) = (
        glam::DVec2::new(h.sin(), h.cos()),
        glam::DVec2::new(h.cos(), -h.sin()),
    );
    let centre = at + (right * bb[3] as f64 + fwd * bb[4] as f64).extend(bb[5] as f64);
    let reach = (bb[0].max(bb[1]) as f64) * 0.5 + 3.0;
    (eye - centre).length() < reach
}
