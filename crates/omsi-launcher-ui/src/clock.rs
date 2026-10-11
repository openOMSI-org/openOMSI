use super::state::State;

impl State {
    pub fn pick_time(&mut self, time: i32) {
        self.choice.time = time;
        if stop_following(&mut self.settings, "use_real_time") {
            self.settings_dirty = 0.3;
        }
        self.touched();
    }

    pub fn pick_date(&mut self, date: String) {
        self.choice.season = "auto".into();
        self.choice.own_date = None;
        self.set_date(date);
        self.touched();
    }

    pub fn set_date(&mut self, date: String) {
        self.choice.date = date;
        if stop_following(&mut self.settings, "use_real_date") {
            self.settings_dirty = 0.3;
        }
        self.load_lines();
    }
}

fn stop_following(settings: &mut serde_json::Value, key: &str) -> bool {
    let on = settings.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
    if on {
        settings[key] = serde_json::json!(false);
    }
    on
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_date_or_time_picked_by_hand_stops_following_the_computers_clock() {
        let mut s = serde_json::json!({"use_real_time": true, "use_real_date": true, "use_real_year": true});
        assert!(super::stop_following(&mut s, "use_real_date"));
        assert_eq!(s["use_real_date"], false);
        assert_eq!(s["use_real_time"], true);
        assert_eq!(s["use_real_year"], true);
        assert!(!super::stop_following(&mut s, "use_real_date"));
        assert!(super::stop_following(&mut s, "use_real_time"));
        assert_eq!(s["use_real_time"], false);
        let mut fresh = serde_json::json!({});
        assert!(!super::stop_following(&mut fresh, "use_real_date"));
        assert_eq!(fresh, serde_json::json!({}));
    }
}
