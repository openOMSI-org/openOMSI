//! Reset the shared-world mirror before a different room may reuse its entity ids.

use super::*;

impl LanWorld {
    pub(crate) fn leave(
        &mut self,
        world: &World,
        renderer: &Renderer,
        scene: &mut Scene,
        traffic: Option<&mut Traffic>,
        humans: Option<&mut Humans>,
        view: &mut SimView,
    ) {
        if let Some(traffic) = traffic {
            traffic.set_mirror(false);
            traffic.lan_centers.clear();
            traffic.others.clear();
            crate::view_sync::apply_traffic(traffic, &mut view.traffic, world, renderer, scene);
        }
        if let Some(humans) = humans {
            humans.set_mirror(false);
            humans.lan_centers.clear();
            crate::view_sync::apply_people(humans, &mut view.people, world, renderer, scene);
        }
        *self = Default::default();
    }
}
