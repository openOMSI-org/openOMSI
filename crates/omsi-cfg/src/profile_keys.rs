//! What OMSI_PROFILE times, by key: the frame's stages and their parts (`traffic.tick.ai`),
//! and the offscreen run's stages. A key is a variant, its time a slot of an array: the frame
//! adds to it twenty-odd times without looking a string up (a map keyed by strings did, every
//! frame). The names are the ones the logs, OMSI_PROFILE_JSON and the performance overlay
//! always printed.

/// A profile key (see the module's notes). The variants are in the order of their names (a
/// test pins it): the profile lists its keys in that order, as the map it replaced did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProfKey {
    Acquire,
    Clock,
    Drive,
    Duty,
    Gpu,
    GroundGap,
    Hud,
    HudNavigator,
    Humans,
    Integrations,
    Lan,
    Lighting,
    LightsAndRain,
    LightsAmbience,
    LightsAtlas,
    LightsCollect,
    LightsRain,
    LightsSnow,
    LightsSoundscape,
    LightsSpray,
    Limiter,
    Mirrors,
    OnFoot,
    Photo,
    Placement,
    Player,
    PlayerGlass,
    PlayerHover,
    Present,
    Record,
    Render,
    Scripted,
    ScriptedApply,
    ScriptedBoards,
    ScriptedInputs,
    ScriptedLamps,
    ScriptedLock,
    ScriptedScripts,
    ScriptedTextures,
    Server,
    Service,
    Shell,
    SimThreadStep,
    SimThreadWait,
    Snapshot,
    Startup,
    Streaming,
    Surroundings,
    Traffic,
    TrafficAudio,
    TrafficPopulate,
    TrafficSchedule,
    TrafficSync,
    TrafficTick,
    TrafficTickAi,
    TrafficTickLanes,
    TrafficTickPlan,
    TrafficTickSignals,
    ViewKeys,
    Weather,
    WheelSurface,
}

impl ProfKey {
    /// Every key, in the order of their names.
    pub const ALL: [ProfKey; 61] = {
        use ProfKey::*;
        [
            Acquire, Clock, Drive, Duty, Gpu, GroundGap, Hud, HudNavigator, Humans, Integrations, Lan, Lighting,
            LightsAndRain, LightsAmbience, LightsAtlas, LightsCollect, LightsRain, LightsSnow, LightsSoundscape, LightsSpray,
            Limiter, Mirrors, OnFoot, Photo, Placement, Player, PlayerGlass, PlayerHover, Present, Record, Render, Scripted,
            ScriptedApply, ScriptedBoards, ScriptedInputs, ScriptedLamps, ScriptedLock, ScriptedScripts,
            ScriptedTextures, Server, Service, Shell, SimThreadStep, SimThreadWait, Snapshot, Startup, Streaming, Surroundings, Traffic,
            TrafficAudio, TrafficPopulate, TrafficSchedule, TrafficSync, TrafficTick, TrafficTickAi,
            TrafficTickLanes, TrafficTickPlan, TrafficTickSignals, ViewKeys, Weather, WheelSurface,
        ]
    };

    /// The key as the logs print it; a dot marks a stage's own part (`traffic.tick` of `traffic`).
    pub const fn name(self) -> &'static str {
        use ProfKey::*;
        match self {
            Acquire => "acquire",
            Clock => "clock",
            Drive => "drive",
            Duty => "duty",
            Gpu => "gpu",
            GroundGap => "ground_gap",
            Hud => "hud",
            HudNavigator => "hud.navigator",
            Humans => "humans",
            Integrations => "integrations",
            Lan => "lan",
            Lighting => "lighting",
            LightsAndRain => "lights+rain",
            LightsAmbience => "lights.ambience",
            LightsAtlas => "lights.atlas",
            LightsCollect => "lights.collect",
            LightsRain => "lights.rain",
            LightsSnow => "lights.snow",
            LightsSoundscape => "lights.soundscape",
            LightsSpray => "lights.spray",
            Limiter => "limiter",
            Mirrors => "mirrors",
            OnFoot => "on_foot",
            Photo => "photo",
            Placement => "placement",
            Player => "player",
            PlayerGlass => "player.glass",
            PlayerHover => "player.hover",
            Present => "present",
            Record => "record",
            Render => "render",
            Scripted => "scripted",
            ScriptedApply => "scripted.apply",
            ScriptedBoards => "scripted.boards",
            ScriptedInputs => "scripted.inputs",
            ScriptedLamps => "scripted.lamps",
            ScriptedLock => "scripted.lock",
            ScriptedScripts => "scripted.scripts",
            ScriptedTextures => "scripted.textures",
            Server => "server",
            Service => "service",
            Shell => "shell",
            SimThreadStep => "sim_thread.step",
            SimThreadWait => "sim_thread.wait",
            Snapshot => "snapshot",
            Startup => "startup",
            Streaming => "streaming",
            Surroundings => "surroundings",
            Traffic => "traffic",
            TrafficAudio => "traffic.audio",
            TrafficPopulate => "traffic.populate",
            TrafficSchedule => "traffic.schedule",
            TrafficSync => "traffic.sync",
            TrafficTick => "traffic.tick",
            TrafficTickAi => "traffic.tick.ai",
            TrafficTickLanes => "traffic.tick.lanes",
            TrafficTickPlan => "traffic.tick.plan",
            TrafficTickSignals => "traffic.tick.signals",
            ViewKeys => "view_keys",
            Weather => "weather",
            WheelSurface => "wheel_surface",
        }
    }

    /// The key of a name (the tests' profiles, written by name).
    #[doc(hidden)]
    pub fn of(name: &str) -> ProfKey {
        *Self::ALL.iter().find(|k| k.name() == name).unwrap_or_else(|| panic!("no profile key {name:?}"))
    }
}

impl std::fmt::Display for ProfKey {
    /// The key's name (padded as asked: `{k:10}`).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.pad(self.name())
    }
}

const N: usize = ProfKey::ALL.len();

/// The seconds each key has taken so far. A key is listed once something was added to it
/// (even nothing: a stage that took no time is still listed, as the map's entries were).
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileTimes {
    secs: [f64; N],
    seen: [bool; N],
}

impl Default for ProfileTimes {
    fn default() -> Self {
        ProfileTimes { secs: [0.0; N], seen: [false; N] }
    }
}

impl ProfileTimes {
    #[inline]
    pub fn add(&mut self, key: ProfKey, secs: f64) {
        self.secs[key as usize] += secs;
        self.seen[key as usize] = true;
    }

    /// The key's seconds (0 when nothing was added).
    pub fn get(&self, key: ProfKey) -> f64 {
        self.secs[key as usize]
    }

    /// The keys added to, in the order of their names, with their seconds.
    pub fn iter(&self) -> impl Iterator<Item = (ProfKey, f64)> + '_ {
        ProfKey::ALL.iter().filter(|k| self.seen[**k as usize]).map(|k| (*k, self.secs[*k as usize]))
    }

    /// A profile of named keys (the tests').
    #[doc(hidden)]
    pub fn of(entries: &[(&str, f64)]) -> Self {
        let mut p = Self::default();
        for (k, v) in entries {
            p.add(ProfKey::of(k), *v);
        }
        p
    }
}

impl<'a> IntoIterator for &'a ProfileTimes {
    type Item = (ProfKey, f64);
    type IntoIter = Box<dyn Iterator<Item = (ProfKey, f64)> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_in_the_order_of_their_names_and_each_once() {
        for (i, k) in ProfKey::ALL.iter().enumerate() {
            assert_eq!(*k as usize, i, "{k:?} out of place in ALL");
        }
        for w in ProfKey::ALL.windows(2) {
            assert!(w[0].name() < w[1].name(), "{} before {}", w[0].name(), w[1].name());
        }
    }

    #[test]
    fn lists_what_was_added_in_name_order_like_the_map_did() {
        let mut p = ProfileTimes::default();
        let mut m = std::collections::BTreeMap::<&'static str, f64>::new();
        for (k, v) in [(ProfKey::TrafficTick, 1.0), (ProfKey::Acquire, 0.5), (ProfKey::Traffic, 0.0), (ProfKey::TrafficTick, 2.0), (ProfKey::LightsAndRain, 0.25), (ProfKey::LightsAtlas, 0.125)] {
            p.add(k, v);
            *m.entry(k.name()).or_default() += v;
        }
        assert_eq!(p.iter().map(|(k, v)| (k.name(), v)).collect::<Vec<_>>(), m.into_iter().collect::<Vec<_>>());
        // (a key added nothing to is listed; one never added to is not)
        assert_eq!(p.get(ProfKey::Present), 0.0);
        assert!(p.iter().any(|(k, _)| k == ProfKey::Traffic) && !p.iter().any(|(k, _)| k == ProfKey::Present));
    }
}
