//! The MileZero integration (settings `milezero`): the station's own stream with the quality
//! picked by the connection, what plays now, and song requests. The navigator's card that
//! shows it is `Ui::draw_milezero`. Streams and API: https://milezero.live/blog/resoource-api-stream

use omsi_audio::stream::StreamBuf;
use omsi_audio::{AudioEngine, VoiceId, VoiceParams};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const HUB: &str = "https://hub.milezero.live";

/// The three streams, best first: AAC, 320 kbit/s MP3, 128 kbit/s MP3.
const STREAMS: [(&str, &str); 3] = [
    ("AAC", "https://radio.milezero.live/radio"),
    ("MP3 320", "https://radio.milezero.live/radio-320.mp3"),
    ("MP3 128", "https://radio.milezero.live/radio-128.mp3"),
];

/// Seconds of a bad connection (an empty buffer) before the next worse stream is taken.
const DOWN_AFTER: f32 = 4.0;
/// Seconds of a full buffer before a better stream is tried (doubled after each fall back).
const UP_AFTER: f32 = 75.0;

#[derive(Clone, Default)]
pub(crate) struct Song {
    pub title: String,
    pub artist: String,
    pub dj: String,
    /// The cover, decoded on the poller's thread, with the address it came from.
    pub cover: Option<(String, Arc<omsi_texture::Image>)>,
    /// Unix seconds the song started (from the station's history), when known.
    pub started: Option<i64>,
    /// The song's length in seconds (looked up by name), when found.
    pub duration: Option<f32>,
}

#[derive(Default)]
struct Shared {
    song: Option<Song>,
    /// The result of the last request, with when it was made.
    request: Option<(String, Instant)>,
}

struct Playing {
    buf: Arc<StreamBuf>,
    voice: VoiceId,
    level: usize,
    since: Instant,
    bad: f32,
    good: f32,
}

pub(crate) struct Milezero {
    shared: Arc<Mutex<Shared>>,
    /// The running poller's own token: every poller gets a new one, so an old poller that has
    /// not yet noticed its stop is never started again by a quick off and on.
    stop: Arc<AtomicBool>,
    polling: bool,
    /// The player pressed play (it is always paused when the game starts).
    pub want_play: bool,
    playing: Option<Playing>,
    /// The stream level the connection settled on, and how long a better one is not tried.
    level: usize,
    up_after: f32,
    /// The request being typed, while the box is open.
    pub typing: Option<String>,
    last: Instant,
}

impl Default for Milezero {
    fn default() -> Self {
        Self { shared: Default::default(), stop: Arc::new(AtomicBool::new(false)), polling: false, want_play: false, playing: None, level: 0, up_after: UP_AFTER, typing: None, last: Instant::now() }
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(6)).timeout_read(Duration::from_secs(10)).user_agent("openOMSI").build()
}

fn get_json(agent: &ureq::Agent, url: &str) -> Option<serde_json::Value> {
    serde_json::from_reader(agent.get(url).call().ok()?.into_reader().take(1 << 20)).ok()
}

use std::io::Read;

/// The length of a song, from the public iTunes search (the station's own API has none).
fn itunes_length(agent: &ureq::Agent, artist: &str, title: &str) -> Option<f32> {
    let enc = |t: &str| t.bytes().map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") }).collect::<String>();
    let first = artist.split(',').next().unwrap_or(artist);
    let v = get_json(agent, &format!("https://itunes.apple.com/search?entity=song&limit=1&term={}", enc(&format!("{first} {title}"))))?;
    v["results"][0]["trackTimeMillis"].as_f64().map(|ms| ms as f32 / 1000.0)
}

fn poll(shared: Arc<Mutex<Shared>>, stop: Arc<AtomicBool>) {
    let agent = agent();
    let mut last_title = String::new();
    while !stop.load(Ordering::Relaxed) {
        if let Some(v) = get_json(&agent, &format!("{HUB}/api/public/nowplaying")) {
            let np = &v["nowPlaying"];
            let text = |k: &str| np[k].as_str().unwrap_or("").to_string();
            let key = format!("{} - {}", text("artist"), text("title"));
            if np.is_object() && key != last_title {
                last_title = key.clone();
                let started = get_json(&agent, &format!("{HUB}/api/public/history?limit=1")).and_then(|h| h[0]["played_at"].as_i64());
                // (downloaded and decoded in memory here, never on the frame and with no file to share)
                let cover = np["art"].as_str().filter(|u| u.starts_with("https://")).and_then(|u| {
                    let mut bytes = Vec::new();
                    agent.get(u).call().ok()?.into_reader().take(4 << 20).read_to_end(&mut bytes).ok()?;
                    let img = omsi_texture::decode_bytes(&bytes, std::path::Path::new("cover.jpg")).ok();
                    img.map(|i| (u.to_string(), Arc::new(i)))
                });
                let duration = itunes_length(&agent, &text("artist"), &text("title"));
                let song = Song { title: text("title"), artist: text("artist"), dj: np["dj"]["display_name"].as_str().unwrap_or("").to_string(), cover, started, duration };
                // (a poller that was stopped while it downloaded publishes nothing)
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                shared.lock().unwrap().song = Some(song);
            }
        }
        for _ in 0..60 {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

impl Milezero {
    pub fn song(&self) -> Option<Song> {
        self.shared.lock().unwrap().song.clone()
    }

    /// The seconds the song has played, when the station says when it started.
    pub fn elapsed(&self) -> Option<f32> {
        let started = self.song()?.started?;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
        Some((now - started).max(0) as f32)
    }

    pub fn quality(&self) -> &'static str {
        STREAMS[self.playing.as_ref().map_or(self.level, |p| p.level)].0
    }

    pub fn is_playing(&self) -> bool {
        self.playing.is_some()
    }

    pub fn loading(&self) -> bool {
        self.playing.as_ref().is_some_and(|p| !p.buf.is_playing())
    }

    pub fn request_status(&self) -> Option<String> {
        self.shared.lock().unwrap().request.as_ref().filter(|(_, t)| t.elapsed() < Duration::from_secs(6)).map(|(s, _)| s.clone())
    }

    /// Sends the typed request (a song, an artist ... as the station reads it).
    pub fn submit(&mut self) {
        let Some(text) = self.typing.take().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) else { return };
        let shared = self.shared.clone();
        shared.lock().unwrap().request = Some(("Sending the request ...".into(), Instant::now()));
        let _ = std::thread::Builder::new().name("milezero-request".into()).spawn(move || {
            let body = serde_json::json!({ "username": "openOMSI", "message": text.chars().take(2000).collect::<String>(), "type": "song" });
            let res = agent().post(&format!("{HUB}/api/submit-request")).set("Content-Type", "application/json").set("Accept", "application/json").send_string(&body.to_string());
            let msg = match res {
                Ok(_) => "Request sent".to_string(),
                Err(ureq::Error::Status(403, _)) => "You have been blocked from sending requests".to_string(),
                Err(_) => "The request could not be sent".to_string(),
            };
            shared.lock().unwrap().request = Some((msg, Instant::now()));
        });
    }

    /// Every frame: starts and stops the poller and the stream with the setting and the
    /// play button, and moves between the streams as the connection allows.
    pub fn update(&mut self, audio: &AudioEngine, enabled: bool) {
        let dt = self.last.elapsed().as_secs_f32().min(0.5);
        self.last = Instant::now();
        if enabled && !self.polling {
            self.start_poller();
        } else if !enabled && self.polling {
            self.stop_poller();
            self.want_play = false;
            self.typing = None;
        }
        if !enabled || !self.want_play {
            if let Some(p) = self.playing.take() {
                p.buf.close();
                audio.stop(p.voice);
            }
            return;
        }
        if self.playing.is_none() {
            self.start(audio, self.level);
        }
        let Some(p) = self.playing.as_mut() else { return };
        audio.set_params(p.voice, VoiceParams { gain: 1.0, ..Default::default() });
        // a connection that cannot keep the buffer filled takes the next worse stream; one that
        // keeps it full for a long time tries the next better one
        let warm = p.since.elapsed().as_secs_f32() > 6.0;
        let buffered = p.buf.buffered();
        let failing = p.buf.status().starts_with("no signal");
        if warm && (failing || buffered < 0.4) {
            p.bad += dt;
            p.good = 0.0;
        } else if buffered > 6.0 {
            p.good += dt;
            p.bad = 0.0;
        } else {
            p.bad = (p.bad - dt).max(0.0);
        }
        let level = p.level;
        if p.bad > DOWN_AFTER && level + 1 < STREAMS.len() {
            self.up_after = (self.up_after * 2.0).min(600.0);
            self.level = level + 1;
            self.restart(audio);
        } else if p.good > self.up_after && level > 0 {
            self.level = level - 1;
            self.restart(audio);
        }
    }

    fn start_poller(&mut self) {
        self.polling = true;
        self.stop = Arc::new(AtomicBool::new(false));
        let (s, st) = (self.shared.clone(), self.stop.clone());
        let _ = std::thread::Builder::new().name("milezero".into()).spawn(move || poll(s, st));
    }

    fn stop_poller(&mut self) {
        self.polling = false;
        self.stop.store(true, Ordering::Relaxed);
    }

    fn restart(&mut self, audio: &AudioEngine) {
        if let Some(p) = self.playing.take() {
            p.buf.close();
            audio.stop(p.voice);
        }
        self.start(audio, self.level);
    }

    fn start(&mut self, audio: &AudioEngine, level: usize) {
        log::info!("milezero: stream {}", STREAMS[level].0);
        let buf = omsi_audio::radio::open(STREAMS[level].1);
        let voice = audio.play_stream(buf.clone(), VoiceParams { gain: 1.0, ..Default::default() });
        self.playing = Some(Playing { buf, voice, level, since: Instant::now(), bad: 0.0, good: 0.0 });
    }
}

impl Drop for Milezero {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A quick off and on leaves the first poller stopped (its own token) and the second running.
    #[test]
    fn a_quick_toggle_stops_the_old_poller_and_starts_a_new_one() {
        let mut m = Milezero::default();
        m.start_poller();
        let first = m.stop.clone();
        m.stop_poller();
        m.start_poller();
        assert!(first.load(Ordering::Relaxed), "the first poller must stay stopped");
        assert!(!m.stop.load(Ordering::Relaxed), "the second one runs");
        assert!(!Arc::ptr_eq(&first, &m.stop));
        m.stop_poller();
    }
}
