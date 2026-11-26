//! The turn an AI car indicates ahead of it.

use super::*;

impl AiState {
    /// The turn to indicate now: the first one on the way within `within` metres, but not
    /// while a turn to the same side that the car does not take still lies before it -
    /// indicating there told everybody the car was about to turn into that one.
    pub fn turn_to_signal(&self, net: &Network, within: f32) -> Option<i32> {
        let (turn, dist) = self.turn_ahead(net, within)?;
        if dist <= 0.0 {
            return Some(turn);
        }
        let (mut from, mut d) = (self.lane, net.lanes[self.lane].length() - self.s);
        for l in self.upcoming() {
            if d >= dist - 0.5 {
                break;
            }
            if net.lanes[from]
                .next
                .iter()
                .any(|&n| n != l && net.lanes[n].turn == turn)
            {
                return None;
            }
            from = l;
            d += net.lanes[l].length();
        }
        Some(turn)
    }
}
