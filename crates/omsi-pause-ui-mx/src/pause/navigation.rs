//! Presentation-only grouping; indices always refer to the original, permission-filtered menu.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Category {
    #[default]
    Home,
    Driving,
    World,
    Saves,
    Capture,
    Settings,
    Online,
    Plugins,
}

impl Category {
    pub const ALL: [Self; 8] = [
        Self::Home,
        Self::Driving,
        Self::World,
        Self::Saves,
        Self::Capture,
        Self::Settings,
        Self::Online,
        Self::Plugins,
    ];
    pub const GROUPS: [(&'static str, &'static [Self]); 4] = [
        ("Simulation", &[Self::Driving, Self::World, Self::Saves]),
        ("Tools", &[Self::Capture]),
        ("Preferences", &[Self::Settings]),
        ("Community", &[Self::Online, Self::Plugins]),
    ];

    pub fn of(id: &str) -> Option<Self> {
        Some(match id {
            "resume" | "quit" => return None,
            "tobus" | "map" | "streets" | "duty" | "skipstop" | "endduty" => Self::Driving,
            "vehicle" | "world" => Self::World,
            "save" | "saveslot" | "load" => Self::Saves,
            "shot" | "photo" => Self::Capture,
            "multiplayer" | "players" | "copycode" | "admin" => Self::Online,
            "plugin" => Self::Plugins,
            _ => Self::Settings,
        })
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Home => "Quick actions",
            Self::Driving => "Driving and routes",
            Self::World => "Vehicle and world",
            Self::Saves => "Saved situations",
            Self::Capture => "Photo tools",
            Self::Settings => "Game settings",
            Self::Online => "Multiplayer",
            Self::Plugins => "Add-on tools",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Driving => "departure_board",
            Self::World => "airport_shuttle",
            Self::Saves => "save",
            Self::Capture => "photo_camera",
            Self::Settings => "settings",
            Self::Online => "group",
            Self::Plugins => "extension",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Home => "Your frequent actions, without the long menu.",
            Self::Driving => "Find your way and manage the current tour.",
            Self::World => "Adjust your vehicle and the simulation around it.",
            Self::Saves => "Save the situation or return to a saved one.",
            Self::Capture => "Capture a screenshot or open photo mode.",
            Self::Settings => "Graphics, sound, controls and the driver's view.",
            Self::Online => "Players, server invitations and administration.",
            Self::Plugins => "The tools registered by your loaded add-ons.",
        }
    }

    pub fn indices(self, ids: &[&str]) -> Vec<usize> {
        ids.iter()
            .enumerate()
            .filter_map(|(i, id)| {
                let included = if self == Self::Home {
                    matches!(
                        *id,
                        "tobus" | "camera" | "map" | "save" | "duty" | "players"
                    )
                } else {
                    Self::of(id) == Some(self)
                };
                included.then_some(i)
            })
            .collect()
    }

    pub fn available(ids: &[&str]) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|c| *c == Self::Home || !c.indices(ids).is_empty())
            .collect()
    }

    #[cfg(test)]
    pub fn focus_indices(self, ids: &[&str]) -> Vec<usize> {
        let mut indices: Vec<usize> = ids
            .iter()
            .position(|id| *id == "resume")
            .into_iter()
            .collect();
        indices.extend(self.indices(ids));
        indices.extend(ids.iter().position(|id| *id == "quit"));
        indices
    }
}

pub fn step(indices: &[usize], selected: usize, forward: bool) -> Option<usize> {
    let n = indices.len();
    if n == 0 {
        return None;
    }
    let at = indices.iter().position(|i| *i == selected);
    let next = match (at, forward) {
        (Some(i), true) => (i + 1) % n,
        (Some(i), false) => (i + n - 1) % n,
        (None, true) => 0,
        (None, false) => n - 1,
    };
    Some(indices[next])
}

#[cfg(test)]
#[path = "navigation_tests.rs"]
mod tests;
