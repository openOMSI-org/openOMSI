//! Mouse adjustment within the headset image, independent of world or bus movement.

use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct HeadPointer {
    offset: (f32, f32),
    previous: Option<(f32, f32)>,
    menu_open: bool,
    last_moved: Option<Instant>,
}

impl HeadPointer {
    /// Include movement received since the last rendered frame, so clicks and wheel
    /// events pick the same location that the next frame will draw.
    pub(super) fn position(&self, mouse: (f32, f32), size: (u32, u32)) -> (f32, f32) {
        let delta = self
            .previous
            .filter(|_| !self.menu_open)
            .map_or((0.0, 0.0), |last| (mouse.0 - last.0, mouse.1 - last.1));
        let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
        (
            (w * 0.5 + self.offset.0 + delta.0).clamp(0.0, w),
            (h * 0.5 + self.offset.1 + delta.1).clamp(0.0, h),
        )
    }

    pub(super) fn update(&mut self, mouse: (f32, f32), size: (u32, u32), menu_open: bool) {
        self.update_at(mouse, size, menu_open, Instant::now());
    }

    fn update_at(&mut self, mouse: (f32, f32), size: (u32, u32), menu_open: bool, now: Instant) {
        if self
            .previous
            .is_none_or(|p| (p.0 - mouse.0).abs() > 0.5 || (p.1 - mouse.1).abs() > 0.5)
            || self.menu_open != menu_open
        {
            self.last_moved = Some(now);
        }
        if !menu_open {
            let position = self.position(mouse, size);
            self.offset = (
                position.0 - size.0.max(1) as f32 * 0.5,
                position.1 - size.1.max(1) as f32 * 0.5,
            );
        }
        self.previous = Some(mouse);
        self.menu_open = menu_open;
    }

    pub(super) fn visible(&self, enabled: bool, now: Instant) -> bool {
        enabled
            && self
                .last_moved
                .is_some_and(|t| now.saturating_duration_since(t) < Duration::from_secs(10))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{eye_projection, eye_ray};
    use super::*;
    use glam::DVec3;
    use omsi_render::Camera;

    #[test]
    fn cursor_hides_after_idle_time_until_mouse_movement_or_menu_transition() {
        let mut pointer = HeadPointer::default();
        let now = Instant::now();
        let size = (1000, 800);
        pointer.update_at((20.0, 30.0), size, false, now);
        assert!(pointer.visible(true, now + Duration::from_secs(9)));
        pointer.update_at((20.0, 30.0), size, false, now + Duration::from_secs(10));
        assert!(!pointer.visible(true, now + Duration::from_secs(10)));
        assert!(!pointer.visible(false, now));
        // Drawing new head poses with a stationary mouse does not refresh activity.
        pointer.update_at((20.0, 30.0), size, false, now + Duration::from_secs(30));
        assert!(!pointer.visible(true, now + Duration::from_secs(30)));
        pointer.update_at((21.0, 30.0), size, false, now + Duration::from_secs(31));
        assert!(pointer.visible(true, now + Duration::from_secs(31)));
        pointer.update_at((21.0, 30.0), size, true, now + Duration::from_secs(50));
        assert!(pointer.visible(true, now + Duration::from_secs(50)));
        assert!(!pointer.visible(true, now + Duration::from_secs(60)));
    }

    #[test]
    fn mouse_fine_adjustment_survives_menu_and_screen_edges() {
        let mut pointer = HeadPointer::default();
        let size = (1000, 800);
        pointer.update((30.0, 40.0), size, false);
        assert_eq!(pointer.position((30.0, 40.0), size), (500.0, 400.0));
        // Input before a render is immediately available to picking.
        assert_eq!(pointer.position((45.0, 32.0), size), (515.0, 392.0));
        pointer.update((45.0, 32.0), size, false);
        pointer.update((900.0, 700.0), size, true);
        pointer.update((200.0, 100.0), size, true);
        pointer.update((200.0, 100.0), size, false);
        assert_eq!(pointer.position((200.0, 100.0), size), (515.0, 392.0));
        pointer.update((2000.0, 100.0), size, false);
        assert_eq!(pointer.position((1999.0, 100.0), size).0, 999.0);
    }

    #[test]
    fn pointer_ray_follows_head_and_vehicle_without_mouse_movement() {
        let mut pointer = HeadPointer::default();
        let size = (1000, 800);
        pointer.update((20.0, 30.0), size, false);
        pointer.update((120.0, 10.0), size, false);
        let at = pointer.position((120.0, 10.0), size);
        let camera = Camera {
            position: DVec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            fov_deg: 90.0,
            near: 0.1,
            far: 1000.0,
        };
        let projection = eye_projection(
            openxr::Fovf {
                angle_left: -0.7,
                angle_right: 0.9,
                angle_up: 0.8,
                angle_down: -0.8,
            },
            0.1,
            1000.0,
        );
        let ray = |c: &Camera| eye_ray(c, projection, at.0, at.1, size, (1.0, 1.0));
        let moved = Camera {
            position: DVec3::new(1000.0, -800.0, 20.0),
            yaw: 90.0,
            ..camera
        };
        let a = ray(&camera);
        let b = ray(&moved);
        // The relative horizontal/vertical aim is unchanged, while world aim turns.
        assert!((a.dot(camera.forward()) - b.dot(moved.forward())).abs() < 1e-5);
        assert!((a.dot(camera.right()) - b.dot(moved.right())).abs() < 1e-5);
        assert!(a.dot(b) < 0.5);
        assert_eq!(pointer.position((120.0, 10.0), size), at);
    }
}
