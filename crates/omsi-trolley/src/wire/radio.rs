use super::*;

impl WireNet {
    fn radio_matches(&self, switch: &SwitchDef, pos: DVec3, channel: u32) -> bool {
        switch.mode == SwitchMode::Radio
            && switch.radio_range > 0.0
            && !switch.channel.is_some_and(|wanted| wanted != channel)
            && horizontal(self.nodes[switch.node as usize].pos - pos).length_squared() <= (switch.radio_range as f64).powi(2)
    }

    /// Send a radio request now to every matching receiver in horizontal range; the number moved.
    pub fn receive_radio(&self, pos: DVec3, request: &SwitchRequest) -> usize {
        let mut dynamic = self.dynamic.lock().unwrap();
        let mut changed = 0;
        for (index, switch) in self.switches.iter().enumerate() {
            if !self.radio_matches(switch, pos, request.channel) {
                continue;
            }
            let want = if request.radio == 2 {
                !dynamic.branch[index]
            } else {
                let Some(want) = switch.wanted(request) else { continue };
                want
            };
            if dynamic.move_switch(index, want, self.nodes[switch.node as usize].pos) {
                changed += 1;
            }
        }
        changed
    }

    /// Send route `code` to the receivers whose range a vehicle entered between `from` and `to`;
    /// the number moved. Direction requests are never sent this way.
    pub(crate) fn receive_route_on_entry(&self, from: DVec3, to: DVec3, code: u32, channel: u32) -> usize {
        let request = SwitchRequest { code, channel, ..Default::default() };
        let mut dynamic = None;
        let mut changed = 0;
        for (index, switch) in self.switches.iter().enumerate() {
            if !self.radio_matches(switch, to, channel) || self.radio_matches(switch, from, channel) {
                continue;
            }
            let Some(want) = switch.wanted(&request) else { continue };
            let dynamic = dynamic.get_or_insert_with(|| self.dynamic.lock().unwrap());
            if dynamic.move_switch(index, want, self.nodes[switch.node as usize].pos) {
                changed += 1;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_inverts_actual_position_and_ignores_route_preselection() {
        let net = super::super::tests::radio_net("range 20\nchannel 3\nroute 158 right\n");
        let pos = DVec3::new(50.0, 0.0, 5.5);
        let request = SwitchRequest { radio: 2, channel: 3, code: 158, ..Default::default() };
        assert_eq!(net.receive_radio(pos, &SwitchRequest { channel: 2, ..request }), 0);
        assert!(!net.switch_branch(0));
        assert_eq!(net.receive_radio(pos, &request), 1);
        assert!(net.switch_branch(0));
        assert_eq!(net.receive_radio(pos, &request), 1);
        assert!(!net.switch_branch(0));
        net.set_switch(0, true);
        assert_eq!(net.receive_radio(pos, &request), 1);
        assert!(!net.switch_branch(0));
    }

    #[test]
    fn toggle_obeys_horizontal_range_and_receiver_mode() {
        let mut net = super::super::tests::radio_net("range 20\nchannel 3\n");
        let pos = DVec3::new(50.0, 20.0, 100.0);
        let request = SwitchRequest { radio: 2, channel: 3, ..Default::default() };
        assert_eq!(net.receive_radio(DVec3::new(50.0, 20.01, 5.5), &request), 0);
        assert!(!net.switch_branch(0));
        assert_eq!(net.receive_radio(pos, &request), 1);
        assert!(net.switch_branch(0));
        for mode in [SwitchMode::Power, SwitchMode::Indicator, SwitchMode::Fixed] {
            net.switches[0].mode = mode;
            assert_eq!(net.receive_radio(pos, &request), 0);
            assert!(net.switch_branch(0));
        }
    }
}
