//! Internet radio: a station's stream read over HTTP(S) and decoded (MP3, AAC in ADTS, Ogg
//! Vorbis) on a thread of its own into a `StreamBuf` the mixer plays. Nothing is downloaded
//! ahead: it is live, like a car radio. Playlist addresses (.m3u, .pls) are followed, the
//! song titles a Shoutcast/Icecast server sends in between (ICY metadata) are taken out of
//! the sound and kept as the status, and a connection that drops is made again.

use crate::stream::StreamBuf;
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Seconds of sound read ahead at most; a server that sends a burst at the start (many do,
/// to fill a player's buffer) is simply read more slowly.
const AHEAD: f32 = 8.0;

/// Start playing `url` into a new buffer; `close()` on it ends the reader.
pub fn open(url: &str) -> Arc<StreamBuf> {
    let buf = Arc::new(StreamBuf::default());
    let (b, url) = (buf.clone(), url.to_string());
    let spawned = std::thread::Builder::new()
        .name("radio".into())
        .spawn(move || run(&b, &url));
    if let Err(e) = spawned {
        log::warn!("radio: cannot start the radio: {e}");
        buf.set_status("cannot start the radio");
    }
    buf
}

fn run(buf: &StreamBuf, url: &str) {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(15))
        .user_agent(&format!("openOMSI/{}", option_env!("OPENOMSI_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))))
        .build();
    let mut wait = 2u64;
    while !buf.is_closed() {
        buf.set_status("connecting …");
        match play(&agent, buf, url) {
            Ok(()) => wait = 2,
            Err(e) => {
                if buf.is_closed() {
                    return;
                }
                log::warn!("radio {url}: {e}");
                buf.set_status("no signal");
            }
        }
        // the connection ended or failed: try again, less often the longer it fails
        for _ in 0..wait * 10 {
            if buf.is_closed() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        wait = (wait * 2).min(30);
    }
}

/// Where the sound is: `url` itself, or the first address of the playlist it names.
fn resolve(agent: &ureq::Agent, url: &str) -> anyhow::Result<ureq::Response> {
    let mut url = url.to_string();
    for _ in 0..4 {
        let resp = agent.get(&url).set("Icy-MetaData", "1").call()?;
        let ctype = resp.content_type().to_ascii_lowercase();
        let lower = url.to_ascii_lowercase();
        let playlist = ctype.contains("mpegurl")
            || ctype.contains("scpls")
            || ctype.starts_with("text/")
            || lower.ends_with(".m3u")
            || lower.ends_with(".m3u8")
            || lower.ends_with(".pls");
        if !playlist {
            return Ok(resp);
        }
        let mut text = String::new();
        resp.into_reader().take(64 * 1024).read_to_string(&mut text)?;
        // .m3u: the first line that is an address; .pls: File1=address
        let next = text
            .lines()
            .map(|l| l.trim())
            .map(|l| l.split_once('=').filter(|(k, _)| k.to_ascii_lowercase().starts_with("file")).map(|(_, v)| v.trim()).unwrap_or(l))
            .find(|l| l.starts_with("http://") || l.starts_with("https://"))
            .ok_or_else(|| anyhow::anyhow!("the playlist names no stream"))?;
        url = next.to_string();
    }
    anyhow::bail!("playlists nested too deep")
}

fn play(agent: &ureq::Agent, buf: &StreamBuf, url: &str) -> anyhow::Result<()> {
    let resp = resolve(agent, url)?;
    let ctype = resp.content_type().to_ascii_lowercase();
    let metaint = resp.header("icy-metaint").and_then(|v| v.trim().parse::<usize>().ok());
    let name = resp.header("icy-name").map(|s| s.trim().to_string()).unwrap_or_default();
    let reader: Box<dyn Read + Send + Sync> = Box::new(resp.into_reader());
    let title = Arc::new(parking_lot::Mutex::new(String::new()));
    let reader: Box<dyn Read + Send + Sync> = match metaint {
        Some(n) if n > 0 => Box::new(IcyReader { inner: reader, every: n, left: n, title: title.clone() }),
        _ => reader,
    };
    // (a new connection: what the last one said of the station counts no longer)
    buf.set_meta(&name, "");
    let mss = MediaSourceStream::new(Box::new(ReadOnlySource::new(reader)), Default::default());
    let mut hint = Hint::new();
    if ctype.contains("mpeg") || ctype.contains("mp3") {
        hint.with_extension("mp3");
    } else if ctype.contains("aac") {
        hint.with_extension("aac");
    } else if ctype.contains("ogg") {
        hint.with_extension("ogg");
    }
    // AAC is read as ADTS straight away: probed, its frame headers pass for MPEG audio and
    // the MP3 reader takes it
    let mut format: Box<dyn FormatReader> = if ctype.contains("aac") {
        Box::new(symphonia::default::formats::AdtsReader::try_new(mss, &FormatOptions::default())?)
    } else {
        symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())?
            .format
    };
    let track = format
        .default_track()
        .ok_or_else(|| anyhow::anyhow!("no audio in the stream"))?
        .clone();
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;
    let mut samples: Option<SampleBuffer<f32>> = None;
    let mut shown = String::new();
    loop {
        if buf.is_closed() {
            return Ok(());
        }
        while buf.buffered() > AHEAD && !buf.is_closed() {
            std::thread::sleep(Duration::from_millis(100));
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != track.id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // a damaged frame (a stream joined mid-frame, a glitch): skip it
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        };
        let spec = *decoded.spec();
        let ch = spec.channels.count().max(1);
        let sb = match &mut samples {
            Some(s) if s.capacity() >= decoded.capacity() * ch => s,
            _ => samples.insert(SampleBuffer::new(decoded.capacity() as u64, spec)),
        };
        sb.copy_interleaved_ref(decoded);
        let data = sb.samples();
        buf.push(
            spec.rate,
            data.chunks_exact(ch).map(|f| if ch >= 2 { [f[0], f[1]] } else { [f[0], f[0]] }),
        );
        let now = {
            let t = title.lock();
            buf.set_meta(&name, &t);
            if t.is_empty() { name.clone() } else { t.clone() }
        };
        let status = if buf.is_playing() { now } else { "buffering …".to_string() };
        if status != shown {
            buf.set_status(status.clone());
            shown = status;
        }
    }
}

/// The sound of a stream with ICY metadata: every `every` bytes of sound the server puts
/// one length byte (×16) and that many bytes of `StreamTitle='…';` text.
struct IcyReader {
    inner: Box<dyn Read + Send + Sync>,
    every: usize,
    left: usize,
    title: Arc<parking_lot::Mutex<String>>,
}

impl Read for IcyReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.left == 0 {
            let mut len = [0u8; 1];
            self.inner.read_exact(&mut len)?;
            let n = len[0] as usize * 16;
            if n > 0 {
                let mut meta = vec![0u8; n];
                self.inner.read_exact(&mut meta)?;
                let text = String::from_utf8_lossy(&meta);
                if let Some(start) = text.find("StreamTitle='") {
                    let rest = &text[start + 13..];
                    let end = rest.find("';").unwrap_or(rest.len());
                    *self.title.lock() = rest[..end].trim().to_string();
                }
            }
            self.left = self.every;
        }
        let want = out.len().min(self.left);
        let got = self.inner.read(&mut out[..want])?;
        self.left -= got;
        Ok(got)
    }
}
