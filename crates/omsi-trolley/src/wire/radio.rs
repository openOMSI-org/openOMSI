use super::*;

impl WireNet {
    fn radio_matches(&self, switch: &SwitchDef, pos: DVec3, channel: u32) -> bool {
        switch.mode == SwitchMode::Radio
            && switch.radio_range > 0.0
            && !switch.channel.is_some_and(|wanted| wanted != channel)
            && horizontal(self.nodes[switch.node as usize].pos - pos).length_squared() <= (switch.radio_range as f64).powi(2)
    }

    pub fn receive_radio(&self, pos: DVec3, request: &SwitchRequest) -> usize {
        self.receive_radio_since(pos, request, None)
    }

    pub(crate) fn receive_radio_since(&self, pos: DVec3, request: &SwitchRequest, previous: Option<DVec3>) -> usize {
        let mut dynamic = self.dynamic.lock().unwrap();
        let mut changed = 0;
        for (index, switch) in self.switches.iter().enumerate() {
            if !self.radio_matches(switch, pos, request.channel) {
                continue;
            }
            if previous.is_some_and(|previous| horizontal(self.nodes[switch.node as usize].pos - previous).length_squared() <= (switch.radio_range as f64).powi(2)) {
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