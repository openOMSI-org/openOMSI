//! The bus radio as live internet radio. OMSI itself plays no music: the buses' radios
//! only set variables that plugins turned into sound - `Snd_Radio` (the cassette player of
//! the stock buses and many mods: 1 while it plays) and the Sound Extension radios'
//! `SndExt_Radio` (the station button pressed, 0 = off) with `SndVol_Radio` (the volume
//! knob). Here those variables tune in a list of internet stations, streamed while they
//! play (see `omsi_audio::radio`); Shift+R steps through the list.
//!
//! The stations are kept in `~/.openomsi/radio.cfg`, one `name = address` per line
//! (MP3, AAC or Ogg streams, or .m3u/.pls playlists), with `volume = 0..1`; the file is
//! written with a default list the first time. A map can bring stations of its own: a
//! `radio.cfg` of the same kind beside its global.cfg, whose stations come before the
//! player's (its `volume` is not read).

use glam::Vec3;
use omsi_audio::stream::StreamBuf;
use omsi_audio::{AudioEngine, VoiceId, VoiceParams};
use omsi_cfg::radio::{config_path, default_text, DEFAULT_STATIONS};
use std::sync::Arc;

/// The setting of the radio's effects named `key` (`radio_speaker`, `radio_hiss`,
/// `radio_dropouts`) in `fx`: its key as written to the settings, and its amount.
pub fn fx_setting<'a>(fx: &'a mut omsi_audio::radio_fx::RadioFxParams, key: &str) -> (&'static str, &'a mut f32) {
    match key {
        "radio_speaker" => ("radio_speaker", &mut fx.speaker),
        "radio_hiss" => ("radio_hiss", &mut fx.hiss),
        _ => ("radio_dropouts", &mut fx.dropouts),
    }
}

/// The options' slider `key` of the radio's effects set to `v` (in steps of a hundredth):
/// the key and value kept for the next game.
pub fn set_fx_setting(fx: &mut omsi_audio::radio_fx::RadioFxParams, key: &str, v: f32) -> (&'static str, String) {
    let (key, at) = fx_setting(fx, key);
    *at = ((v * 100.0).round() / 100.0).clamp(0.0, 1.0);
    (key, at.to_string())
}

/// The stream addresses an OMSI radio plugin keeps in its text files under `plugins`
/// (SuperRadio's `.opl` and its lists; whatever the layout, a line with an http(s) address is
/// a station, named by the text before the address or else by its host): "openOMSI does not
/// load the stations I defined in the .opl".
fn plugin_stations(dir: &std::path::Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut files = Vec::new();
    let mut walk = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = walk.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && depth < 2 {
                walk.push((p, depth + 1));
            } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| matches!(x.to_ascii_lowercase().as_str(), "opl" | "cfg" | "ini" | "txt" | "m3u" | "pls")) {
                files.push(p);
            }
        }
    }
    files.sort();
    for f in files {
        let Ok(bytes) = std::fs::read(&f) else { continue };
        if bytes.len() > 1 << 20 {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        for line in text.lines() {
            let Some(at) = line.find("http://").or_else(|| line.find("https://")) else { continue };
            let url: String = line[at..].chars().take_while(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | ';' | ',' | '|')).collect();
            let before = line[..at].trim().trim_end_matches(['=', ':', '|', ',', ';', '"', '\'', '\t']).trim();
            let before = before.trim_start_matches(|c: char| c.is_ascii_digit() || matches!(c, '.' | ')' | '-' | ' '));
            let name = if before.is_empty() || before.len() > 60 {
                url.split('/').nth(2).unwrap_or(&url).to_string()
            } else {
                before.to_string()
            };
            if url.len() > 12 && !out.iter().any(|(_, u): &(String, String)| u == &url) {
                out.push((name, url));
            }
        }
    }
    out
}

/// A frequency a station is on (as a radio's display writes it: `94.6`), and where: a
/// place on the map in the game's own metres - x east, y north, as the game's log gives a
/// bus's position - near which that frequency is the one on air. Without a place it is
/// the station's frequency everywhere.
#[derive(Clone, Debug, PartialEq)]
struct Frequency {
    mhz: String,
    at: Option<(f64, f64)>,
}

/// `94.6`, `94.6 MHz` or `94.6 @ 25400, 990`.
fn parse_frequency(text: &str) -> Option<Frequency> {
    let (mhz, at) = match text.split_once('@') {
        Some((m, p)) => (m, Some(p)),
        None => (text, None),
    };
    let mhz = mhz.trim().to_ascii_lowercase();
    let mhz: f32 = mhz.trim_end_matches("mhz").trim().replace(',', ".").parse().ok().filter(|v: &f32| *v > 0.0 && v.is_finite())?;
    let at = match at {
        Some(p) => {
            let (x, y) = p.split_once(',')?;
            Some((x.trim().parse::<f64>().ok()?, y.trim().parse::<f64>().ok()?))
        }
        None => None,
    };
    Some(Frequency { mhz: format!("{mhz:.1}"), at })
}

/// The frequencies of the stations that name any, by address (lower case).
type Frequencies = std::collections::HashMap<String, Vec<Frequency>>;

/// Which of a station's frequencies is on air at (x, y): that of the nearest place, and
/// the one without a place where no place is given.
fn frequency_at(on: &[Frequency], x: f64, y: f64) -> Option<&str> {
    let near = on
        .iter()
        .filter_map(|f| f.at.map(|(fx, fy)| ((fx - x).powi(2) + (fy - y).powi(2), f)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, f)| f);
    near.or_else(|| on.iter().find(|f| f.at.is_none())).map(|f| f.mhz.as_str())
}

/// What a radio.cfg says: its stations, its volume where one is set, and the frequencies
/// of the stations that name any (by address, lower case). A line is `name = address`,
/// and behind the address `| frequency` or `| frequency @ x, y` as often as the station
/// has frequencies along the map.
fn parse_stations(text: &str) -> (Vec<(String, String)>, Option<f32>, Frequencies) {
    let (mut stations, mut volume, mut frequencies) = (Vec::new(), None, Frequencies::new());
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else { continue };
        let (name, value) = (name.trim(), value.trim());
        if name.eq_ignore_ascii_case("volume") {
            volume = value.parse::<f32>().ok().map(|v| v.clamp(0.0, 1.0)).or(volume);
            continue;
        }
        let mut parts = value.split('|');
        let url = parts.next().unwrap_or("").trim();
        if url.is_empty() {
            continue;
        }
        let on: Vec<Frequency> = parts.filter_map(parse_frequency).collect();
        if !on.is_empty() {
            frequencies.insert(url.to_ascii_lowercase(), on);
        }
        stations.push((name.to_string(), url.to_string()));
    }
    (stations, volume, frequencies)
}

/// A map's own stations: `radio.cfg` beside its global.cfg, written like the player's -
/// the stations a bus hears where the map plays, and the frequencies they are on there.
/// Its volume line counts for nothing: how loud the radio is stays the player's business.
fn map_stations(map_cfg: &std::path::Path) -> (Vec<(String, String)>, Frequencies) {
    let Some(dir) = map_cfg.parent() else { return Default::default() };
    let Ok(bytes) = omsi_cfg::vfs::read(&dir.join("radio.cfg")) else { return Default::default() };
    let (stations, _, frequencies) = parse_stations(&omsi_cfg::codepage::decode(&bytes));
    (stations, frequencies)
}

pub struct Radio {
    /// What the buttons play: the map's stations, then the player's.
    stations: Vec<(String, String)>,
    /// The player's own (radio.cfg and the radio plugins' lists).
    own: Vec<(String, String)>,
    /// The frequencies the stations are on: the player's file's, and the map's over them.
    frequencies: Frequencies,
    own_frequencies: Frequencies,
    /// The map the list was made for, and whether the list changed while a station played.
    map: String,
    relisted: bool,
    volume: f32,
    /// Shift+R: how far the list is turned from the bus's own station numbers.
    offset: usize,
    playing: Option<Playing>,
    /// The radio's own sound (the settings' `radio_speaker`, `radio_hiss`, `radio_dropouts`).
    pub fx: omsi_audio::radio_fx::RadioFxParams,
}

struct Playing {
    station: usize,
    buf: Arc<StreamBuf>,
    voice: VoiceId,
    /// The status last shown on screen (the song, "no signal" ...).
    shown: String,
    /// The line a text display runs through (see `Radio::display_text`), and since when.
    line: (String, std::time::Instant),
    /// Where the speaker is heard from in the cab: its place relative to the listener in
    /// the bus's frame, as it follows a camera that moves (see `cab_place`), and when it
    /// was last moved.
    heard_at: Option<(Vec3, std::time::Instant)>,
    /// The effects the voice was last given.
    fx: omsi_audio::radio_fx::RadioFxParams,
}

/// How far the radio's sound is from the listener in the cab, of the true distance: the
/// bus's speakers are spread along it, so the sound comes from the driver's corner but does
/// not fade away towards the back as one loudspeaker there would.
const SPREAD: f32 = 0.5;

/// Seconds the radio's place takes to follow a camera that jumped (a view changed): it
/// glides over instead of jumping from one ear to the other.
const GLIDE: f32 = 0.25;

/// Where the radio sits in a bus (its frame: x right, y forward, z up): a little ahead of
/// and under the driver's eyes, in the dashboard or the panel above the windscreen.
fn radio_point(v: &omsi_sim::VehicleInstance) -> Vec3 {
    let def = &v.ty.def;
    let eye = def
        .cameras_driver
        .get(def.camera_std.min(def.cameras_driver.len().saturating_sub(1)))
        .map(|c| Vec3::from(c.pos))
        .unwrap_or(Vec3::new(-0.5, 0.0, 1.8));
    eye + Vec3::new(0.25, 0.5, -0.25)
}

/// The place in the bus's frame, relative to the listener, the radio's sound comes from
/// now: `want` (its true place, nearer by `SPREAD`) reached over `GLIDE` seconds from
/// `was`, `dt` seconds after it.
fn cab_place(was: Option<Vec3>, want: Vec3, dt: f32) -> Vec3 {
    let want = want * SPREAD;
    match was {
        Some(w) => w + (want - w) * (1.0 - (-dt.max(0.0) / GLIDE).exp()),
        None => want,
    }
}

/// Characters in a line of a radio's text display (the "Magnitola" radio of P3ta's SOR
/// buses and its kin: two lines of ten).
const DISPLAY_WIDTH: usize = 10;

/// A station's name and its song as one line a simple text display can show: plain Latin
/// letters (its fonts have little else), no `@` (the display's line break).
fn display_line(name: &str, status: &str) -> String {
    let waiting = status.is_empty() || status.ends_with('…') || status.eq_ignore_ascii_case(name);
    let text = if waiting { name.to_string() } else { format!("{name} - {status}") };
    let mut out = String::new();
    for c in text.chars() {
        let plain: &str = match c {
            '@' => " ",
            c if c.is_ascii() && !c.is_ascii_control() => {
                out.push(c);
                continue;
            }
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ą' => "a",
            'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' | 'Ą' => "A",
            'č' | 'ç' | 'ć' => "c",
            'Č' | 'Ç' | 'Ć' => "C",
            'ď' => "d",
            'Ď' => "D",
            'é' | 'ě' | 'è' | 'ê' | 'ë' | 'ę' => "e",
            'É' | 'Ě' | 'È' | 'Ê' | 'Ë' | 'Ę' => "E",
            'í' | 'ì' | 'î' | 'ï' => "i",
            'Í' | 'Ì' | 'Î' | 'Ï' => "I",
            'ľ' | 'ĺ' | 'ł' => "l",
            'Ľ' | 'Ĺ' | 'Ł' => "L",
            'ň' | 'ñ' | 'ń' => "n",
            'Ň' | 'Ñ' | 'Ń' => "N",
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ő' | 'ø' => "o",
            'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' | 'Ő' | 'Ø' => "O",
            'ř' | 'ŕ' => "r",
            'Ř' | 'Ŕ' => "R",
            'š' | 'ś' => "s",
            'Š' | 'Ś' => "S",
            'ß' => "ss",
            'ť' => "t",
            'Ť' => "T",
            'ú' | 'ù' | 'û' | 'ü' | 'ů' | 'ű' => "u",
            'Ú' | 'Ù' | 'Û' | 'Ü' | 'Ů' | 'Ű' => "U",
            'ý' | 'ÿ' => "y",
            'Ý' => "Y",
            'ž' | 'ź' | 'ż' => "z",
            'Ž' | 'Ź' | 'Ż' => "Z",
            '–' | '—' => "-",
            '’' | '‘' => "'",
            '…' => "...",
            _ => "",
        };
        out.push_str(plain);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `width` characters of `text` at `seconds` after it came up: all of a text that fits,
/// else the text running through from right to left, four characters a second after a
/// moment at its beginning, with a gap before it comes round again.
fn marquee(text: &str, width: usize, seconds: f32) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        return format!("{text:<width$}");
    }
    let round = chars.len() + 3;
    let at = ((seconds - 1.5).max(0.0) * 4.0) as usize % round;
    (0..width).map(|i| chars.get((at + i) % round).copied().unwrap_or(' ')).collect()
}

impl Radio {
    pub fn load(omsi_root: &std::path::Path) -> Radio {
        let mut stations: Vec<(String, String)>;
        let mut volume = 0.7f32;
        let mut frequencies = Frequencies::new();
        let path = config_path();
        match path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()) {
            Some(text) => {
                let (list, vol, on) = parse_stations(&text);
                stations = list;
                volume = vol.unwrap_or(volume);
                frequencies = on;
            }
            None => {
                stations = DEFAULT_STATIONS.iter().map(|(n, u)| (n.to_string(), u.to_string())).collect();
                if let Some(p) = &path {
                    let _ = std::fs::create_dir_all(p.parent().unwrap_or(p));
                    let _ = std::fs::write(p, default_text());
                }
            }
        }
        // the stations of OMSI's radio plugins (SuperRadio and the like), after the own ones
        let own = stations.len();
        for (name, url) in plugin_stations(&omsi_root.join("plugins")) {
            if !stations.iter().any(|(_, u)| u.eq_ignore_ascii_case(&url)) {
                stations.push((name, url));
            }
        }
        if stations.len() > own {
            log::info!("radio: {} stations of the OMSI radio plugins added", stations.len() - own);
        }
        if !stations.is_empty() {
            log::info!("radio: {} stations", stations.len());
        }
        Radio { stations: stations.clone(), own: stations, frequencies: frequencies.clone(), own_frequencies: frequencies, map: String::new(), relisted: false, volume, offset: 0, playing: None, fx: Default::default() }
    }

    /// The frequency the station that plays is on where the bus is (`94.6 MHz`), for a
    /// radio's display: of the places the station's frequencies are given for, the nearest
    /// to (x, y) counts; a frequency without a place is the station's everywhere else.
    /// None for a station without frequencies - the display then keeps what its script
    /// writes.
    pub fn frequency(&self, x: f64, y: f64) -> Option<String> {
        let p = self.playing.as_ref()?;
        let on = self.frequencies.get(&self.stations.get(p.station)?.1.to_ascii_lowercase())?;
        Some(format!("{} MHz", frequency_at(on, x, y)?))
    }

    /// The map that plays (its global.cfg under `root`): its radio.cfg's stations come
    /// first, on the first station buttons, and the player's own follow - less those the
    /// map names too.
    pub fn set_map(&mut self, root: &std::path::Path, map: &str) {
        if self.map == map {
            return;
        }
        self.map = map.to_string();
        let (mut list, on) = map_stations(&omsi_cfg::resolve_path(root, map));
        if !list.is_empty() {
            log::info!("radio: {} stations of the map, {} with their frequencies", list.len(), on.len());
        }
        self.frequencies = self.own_frequencies.clone();
        self.frequencies.extend(on);
        for s in &self.own {
            if !list.iter().any(|(_, u)| u.eq_ignore_ascii_case(&s.1)) {
                list.push(s.clone());
            }
        }
        if list != self.stations {
            self.stations = list;
            self.offset = 0;
            self.relisted = true;
        }
    }

    /// The station the bus's radio is tuned to, None while it is off.
    fn wanted(&self, v: &omsi_sim::VehicleInstance) -> Option<usize> {
        if self.stations.is_empty() {
            return None;
        }
        let button = match v.var("SndExt_Radio") {
            Some(x) if x >= 0.5 => Some(x.round() as usize - 1),
            _ => None,
        };
        let button = button.or_else(|| v.var("Snd_Radio").filter(|x| *x >= 0.5).map(|_| 0));
        button.map(|b| (b + self.offset) % self.stations.len())
    }

    /// Follow the player's bus radio; a text for the screen when the station or its song
    /// changes.
    pub fn update(&mut self, audio: &AudioEngine, v: &omsi_sim::VehicleInstance, inside: bool) -> Option<String> {
        let wanted = self.wanted(v);
        // (another map's list: the same button is another station now)
        if std::mem::take(&mut self.relisted) {
            self.stop(audio);
        }
        if self.playing.as_ref().map(|p| p.station) != wanted {
            self.stop(audio);
            if let Some(station) = wanted {
                let (name, url) = &self.stations[station];
                log::info!("radio: station {} {name} ({url})", station + 1);
                let buf = omsi_audio::radio::open(url);
                let voice = audio.play_stream(buf.clone(), VoiceParams { gain: 0.0, ..Default::default() });
                self.playing = Some(Playing { station, buf, voice, shown: String::new(), line: (String::new(), std::time::Instant::now()), heard_at: None, fx: Default::default() });
            }
        }
        let p = self.playing.as_mut()?;
        // the volume knob, where the radio has one (0..1, some go to 2)
        let knob = v.var("SndVol_Radio").map(|x| x.clamp(0.0, 2.0)).unwrap_or(1.0);
        let gain = self.volume * knob;
        let params = if inside {
            // from the driver's corner of the cab, half as far as it is, and following the
            // camera along the bus's own axes rather than jumping with it
            let xf = v.world_transform();
            let ear = audio.listener_position();
            let want = radio_point(v) - xf.inverse().transform_point3(ear);
            let now = std::time::Instant::now();
            let rel = cab_place(p.heard_at.map(|h| h.0), want, p.heard_at.map_or(0.0, |h| (now - h.1).as_secs_f32()));
            p.heard_at = Some((rel, now));
            VoiceParams { gain, position: Some(ear + xf.transform_vector3(rel)), range: 3.0, doppler: false, ..Default::default() }
        } else {
            // heard through the bodywork from outside
            p.heard_at = None;
            let pos = v.position.as_vec3() + Vec3::Z * 1.5;
            VoiceParams { gain, position: Some(pos), range: 4.0, lowpass_hz: 700.0, ..Default::default() }
        };
        audio.set_params(p.voice, params);
        if p.fx != self.fx {
            p.fx = self.fx;
            audio.set_radio_fx(p.voice, self.fx);
        }
        let status = p.buf.status();
        if status != p.shown && !status.is_empty() && status != "connecting …" {
            p.shown = status.clone();
            let line = format!("Radio {}: {} - {status}", p.station + 1, self.stations[p.station].0);
            log::info!("{line}");
            return Some(line);
        }
        None
    }

    /// What a radio with a text display shows now (`VehicleInstance::radio_text`): the
    /// station and its song, running through the line where they do not fit. Empty while
    /// the radio is off; None without stations (the scripts' own texts then stay).
    pub fn display_text(&mut self) -> Option<String> {
        if self.stations.is_empty() {
            return None;
        }
        let Some(p) = self.playing.as_mut() else { return Some(String::new()) };
        let line = display_line(&self.stations[p.station].0, &p.buf.status());
        if p.line.0 != line {
            p.line = (line, std::time::Instant::now());
        }
        Some(marquee(&p.line.0, DISPLAY_WIDTH, p.line.1.elapsed().as_secs_f32()))
    }

    /// What plays, for the scripts' `GetRadioName` and `GetRadioSong`: the station's name
    /// (as it gives it, else as the list has it) and the title it gives; both empty while
    /// the radio is off.
    pub fn now_playing(&self) -> (String, String) {
        let Some(p) = self.playing.as_ref() else { return Default::default() };
        let name = p.buf.station_name();
        let name = if name.is_empty() { self.stations.get(p.station).map(|s| s.0.clone()).unwrap_or_default() } else { name };
        (name, p.buf.title())
    }

    /// Shift+R: the next station of the list on every button.
    pub fn next_station(&mut self) -> String {
        if self.stations.is_empty() {
            return "No radio stations (see ~/.openomsi/radio.cfg)".into();
        }
        self.offset = (self.offset + 1) % self.stations.len();
        match &self.playing {
            Some(p) => {
                let next = (p.station + 1) % self.stations.len();
                format!("Radio {}: {}", next + 1, self.stations[next].0)
            }
            None => "The bus radio is off (switch it on in the cockpit)".into(),
        }
    }

    pub fn stop(&mut self, audio: &AudioEngine) {
        if let Some(p) = self.playing.take() {
            p.buf.close();
            audio.stop(p.voice);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_cfg::radio::{station_lines, with_stations};

    #[test]
    fn a_station_list_edited_keeps_the_files_comments_volume_and_frequencies() {
        let text = "# my stations\nvolume = 0.4\nOne = https://a.example/one.mp3 | 94.6 @ 100, 200\n# a note\nTwo = https://b.example/two\n";
        let list = station_lines(text);
        assert_eq!(list, vec![("One".to_string(), "https://a.example/one.mp3 | 94.6 @ 100, 200".to_string()), ("Two".to_string(), "https://b.example/two".to_string())]);
        // the second removed, two added (one of them still without an address)
        let edited = vec![list[0].clone(), ("#Jazz = FM".to_string(), "https://c.example/jazz.m3u".to_string()), ("Later".to_string(), String::new())];
        let out = with_stations(text, &edited);
        assert_eq!(out, "# my stations\nvolume = 0.4\nOne = https://a.example/one.mp3 | 94.6 @ 100, 200\nJazz   FM = https://c.example/jazz.m3u\n# a note\n");
        let (stations, volume, frequencies) = parse_stations(&out);
        assert_eq!(stations.iter().map(|s| s.0.as_str()).collect::<Vec<_>>(), ["One", "Jazz   FM"]);
        assert_eq!(volume, Some(0.4));
        assert_eq!(frequencies.len(), 1);
        // a file without stations gets them after what it has
        assert_eq!(with_stations("volume = 1\n", &edited[..1]), "volume = 1\nOne = https://a.example/one.mp3 | 94.6 @ 100, 200\n");
        assert_eq!(station_lines(&default_text()).len(), DEFAULT_STATIONS.len());
    }

    #[test]
    fn a_text_display_gets_plain_letters_and_no_line_break() {
        assert_eq!(display_line("Evropa 2", ""), "Evropa 2");
        assert_eq!(display_line("Evropa 2", "buffering …"), "Evropa 2");
        assert_eq!(display_line("Evropa 2", "evropa 2"), "Evropa 2");
        assert_eq!(display_line("Český rozhlas", "Dvořák – Žalm č. 23"), "Cesky rozhlas - Dvorak - Zalm c. 23");
        assert_eq!(display_line("me@radio", "Наше Радио"), "me radio -");
    }

    #[test]
    fn a_maps_stations_come_first_and_its_volume_counts_for_nothing() {
        let dir = std::env::temp_dir().join(format!("omsi_map_radio_{}", std::process::id()));
        let map = dir.join("maps").join("Mesto");
        std::fs::create_dir_all(&map).unwrap();
        std::fs::write(map.join("radio.cfg"), "# the town's stations\nvolume = 0.1\nMestske radio = http://example.org/mesto.mp3\nSecond = http://example.org/own.mp3\n").unwrap();
        let own = vec![("Mine".to_string(), "http://example.org/mine.mp3".to_string()), ("Own".to_string(), "http://EXAMPLE.org/own.mp3".to_string())];
        let mut r = Radio { stations: own.clone(), own, frequencies: Default::default(), own_frequencies: Default::default(), map: String::new(), relisted: false, volume: 0.7, offset: 2, playing: None, fx: Default::default() };
        r.set_map(&dir, "maps/Mesto/global.cfg");
        let names: Vec<&str> = r.stations.iter().map(|s| s.0.as_str()).collect();
        assert_eq!(names, ["Mestske radio", "Second", "Mine"]);
        assert_eq!((r.volume, r.offset, r.relisted), (0.7, 0, true));
        // a map without a radio.cfg: the player's own again
        r.set_map(&dir, "maps/Jinde/global.cfg");
        assert_eq!(r.stations.iter().map(|s| s.0.as_str()).collect::<Vec<_>>(), ["Mine", "Own"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stations_frequencies_and_their_places() {
        let (stations, volume, on) = parse_stations(
            "volume = 0.5\nZurnal = http://example.org/Z.mp3 | 94.6 @ 25400, 990 | 90.9 MHz @ 2300,-720\nKiss = http://example.org/k.mp3 | 98,1\nPlain = http://example.org/p.mp3\nBad = http://example.org/b.mp3 | here @ 1,2 | 0\n",
        );
        assert_eq!(stations.iter().map(|s| s.1.as_str()).collect::<Vec<_>>(), ["http://example.org/Z.mp3", "http://example.org/k.mp3", "http://example.org/p.mp3", "http://example.org/b.mp3"]);
        assert_eq!(volume, Some(0.5));
        assert_eq!(on["http://example.org/z.mp3"], [Frequency { mhz: "94.6".into(), at: Some((25400.0, 990.0)) }, Frequency { mhz: "90.9".into(), at: Some((2300.0, -720.0)) }]);
        assert_eq!(on["http://example.org/k.mp3"], [Frequency { mhz: "98.1".into(), at: None }]);
        assert!(!on.contains_key("http://example.org/p.mp3") && !on.contains_key("http://example.org/b.mp3"));
        // the nearest place counts
        assert_eq!(frequency_at(&on["http://example.org/z.mp3"], 24000.0, 0.0), Some("94.6"));
        assert_eq!(frequency_at(&on["http://example.org/z.mp3"], 5000.0, 0.0), Some("90.9"));
        assert_eq!(frequency_at(&on["http://example.org/k.mp3"], 5000.0, 0.0), Some("98.1"));
    }

    #[test]
    fn the_cab_radio_comes_from_half_as_far_and_glides_after_a_jump() {
        let want = Vec3::new(2.0, 8.0, -0.4);
        assert_eq!(cab_place(None, want, 0.0), want * SPREAD);
        // a camera that jumped to the back: a quarter second later most of the way, not all
        let was = Vec3::new(0.2, 0.3, 0.0);
        let half = cab_place(Some(was), want, GLIDE);
        let k = (half - was).length() / (want * SPREAD - was).length();
        assert!((k - (1.0 - (-1.0f32).exp())).abs() < 1e-4, "{k}");
        assert!((cab_place(Some(was), want, 5.0) - want * SPREAD).length() < 1e-3);
        assert_eq!(cab_place(Some(was), want, 0.0), was);
    }

    #[test]
    fn a_long_line_runs_through_ten_characters() {
        assert_eq!(marquee("KISS", 10, 5.0), "KISS      ");
        let line = "Evropa 2 - Song";
        assert_eq!(marquee(line, 10, 0.0), "Evropa 2 -");
        assert_eq!(marquee(line, 10, 1.5), "Evropa 2 -");
        assert_eq!(marquee(line, 10, 2.0), "ropa 2 - S");
        // round again after the text and its gap: 18 characters, four a second
        assert_eq!(marquee(line, 10, 1.5 + 14.0 / 4.0), "g   Evropa");
        assert_eq!(marquee(line, 10, 1.5 + 18.0 / 4.0), "Evropa 2 -");
    }
}
