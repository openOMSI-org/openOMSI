use super::*;

impl Player {
    /// Reach for the driver's physical door button when a bus trigger is activated directly
    /// (by a cockpit mouse event or a key bound to `bus_doorfront0`, etc.).
    pub(crate) fn door_button_for_trigger(&mut self, trigger: &str, mesh: Option<usize>) {
        let index = door_keys(&self.vehicle.ty)
            .iter()
            .position(|group| {
                group.iter().any(|name| {
                    name.split('|')
                        .any(|part| part.eq_ignore_ascii_case(trigger))
                })
            })
            .map(|i| i + 1)
            .or_else(|| {
                self.driver.as_ref().and_then(|driver| {
                    mesh.and_then(|mesh| driver.door_button_group_for_mesh(mesh))
                        .or_else(|| driver.door_button_group_for_trigger(trigger))
                })
            });
        if let (Some(index), Some(driver)) = (index, self.driver.as_mut()) {
            driver.door_button_pressed(index, Some(trigger), mesh);
        }
    }
}
