//! Pixel-only freetex transport. No filenames, filesystem access or image work on the frame.
use omsi_net::scripttex::{Image, Incoming, Sync};
use omsi_texture::TextureData;
use std::{
    collections::BTreeMap,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

#[path = "freetex_cache.rs"]
mod cache;
pub use cache::Candidate;

pub const MAX_SLOTS: usize = 64;
pub const MAX_PIXELS_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Default)]
pub struct Frame {
    pub table: u32,
    pub images: Vec<Option<Arc<TextureData>>>,
    /// Local-only source paths, never serialized or accepted from network packets.
    pub paths: Vec<Option<std::path::PathBuf>>,
}

#[derive(Clone, Default)]
pub struct State {
    pub id: u32,
    pub local: Frame,
    /// Owner -> (layout identity, incarnation of that remote render).
    pub subscriptions: BTreeMap<u32, (u32, u64)>,
    pub peers: Vec<u32>,
    pub candidates: Vec<Candidate>,
}

pub struct Received {
    pub generation: u64,
    pub incoming: Incoming,
    pub data: Option<Arc<TextureData>>,
}

#[derive(Default)]
pub struct Output {
    pub packets: Vec<Vec<u8>>,
    pub images: Vec<Received>,
}

/// One worker per live multiplayer view. Bounded channels coalesce frames; a busy worker
/// never makes the game wait. Dropping it disconnects channels without joining a thread.
pub struct Worker {
    state: mpsc::SyncSender<State>,
    packets: mpsc::SyncSender<Vec<u8>>,
    output: mpsc::Receiver<Output>,
}

impl Worker {
    pub fn new() -> std::io::Result<Self> {
        let (state, states) = mpsc::sync_channel(1);
        let (packets, input) = mpsc::sync_channel(128);
        let (done, output) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("LAN freetex pixels".into())
            .spawn(move || run(states, input, done))?;
        Ok(Self {
            state,
            packets,
            output,
        })
    }

    pub fn pump(&self, state: State, packets: Vec<Vec<u8>>) -> Output {
        let _ = self.state.try_send(state);
        for mut p in packets.into_iter().take(128) {
            if p.first() != Some(&omsi_net::freetex::MAGIC) {
                continue;
            }
            p[0] = omsi_net::scripttex::MAGIC;
            let _ = self.packets.try_send(p);
        }
        self.output.try_recv().unwrap_or_default()
    }
}

fn cleared() -> Image {
    // All real freetex pictures have mipmaps; false encodes restoration of the base slot.
    Image {
        width: 1,
        height: 1,
        mipmaps: false,
        rgba: Arc::new(vec![0; 4]),
    }
}

fn pixels(data: &TextureData) -> Option<Image> {
    let n = (data.width as usize)
        .checked_mul(data.height as usize)?
        .checked_mul(4)?;
    if n == 0 || n > MAX_PIXELS_BYTES {
        return None;
    }
    let level = data.levels.first()?;
    if level.len() < data.format.level_bytes(data.width, data.height) {
        return None;
    }
    let rgba = data
        .format
        .decode_level(level, data.width, data.height, data.has_alpha);
    (rgba.len() == n).then(|| Image {
        width: data.width,
        height: data.height,
        mipmaps: true,
        rgba: Arc::new(rgba),
    })
}

fn run(
    states: mpsc::Receiver<State>,
    input: mpsc::Receiver<Vec<u8>>,
    done: mpsc::SyncSender<Output>,
) {
    let mut sync = Sync::with_packet_budget(8);
    let mut state = State::default();
    let mut subscriptions = BTreeMap::new();
    let mut source: Vec<Option<Arc<TextureData>>> = Vec::new();
    let mut images = Vec::new();
    let mut retained: BTreeMap<(u32, u16), Received> = BTreeMap::new();
    let mut last = Instant::now();
    let mut cache = cache::Cache::default();
    let mut references = Vec::new();
    loop {
        let next = match states.recv_timeout(Duration::from_millis(10)) {
            Ok(s) => Some(s),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if let Some(next) = next {
            // A reconnect is a new owner identity, not a continuation of old transfers.
            if next.id != state.id {
                sync = Sync::with_packet_budget(8);
                subscriptions.clear();
                source.clear();
                images.clear();
                retained.clear();
                cache = cache::Cache::default();
            }
            if next.local.table != state.local.table {
                source.clear();
                images.clear();
                sync.clear_local();
            }
            state = next;
            let count = state.local.images.len().min(MAX_SLOTS);
            let mut declared = 0u64;
            for image in state.local.images.iter_mut().take(count) {
                let bytes = image.as_ref().and_then(|d| {
                    u64::from(d.width)
                        .checked_mul(u64::from(d.height))?
                        .checked_mul(4)
                });
                if let Some(n) = bytes
                    .filter(|n| *n <= MAX_PIXELS_BYTES as u64 && declared + *n <= 64 * 1024 * 1024)
                {
                    declared += n;
                } else {
                    *image = None;
                }
            }
            images.resize(count, None);
            references.resize(count, None);
            source.resize(count, None);
            let mut total = 0usize;
            for i in 0..count {
                let new = &state.local.images[i];
                let same = match (&source[i], new) {
                    (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                    (None, None) => images[i].is_some(),
                    _ => false,
                };
                if !same {
                    images[i] = Some(new.as_deref().and_then(pixels).unwrap_or_else(cleared));
                    references[i] = new.as_ref().and_then(|_| {
                        cache::identity(state.local.paths.get(i)?.as_deref()?, images[i].as_ref()?)
                    });
                    source[i] = new.clone();
                }
                total += images[i].as_ref().map_or(0, |i| i.rgba.len());
            }
            if total <= 64 * 1024 * 1024 {
                sync.set_local(state.local.table, images.clone());
            } else {
                sync.clear_local();
            }
            cache.update(&state.candidates);
            let mut reusable = cache.images();
            for (key, image) in references.iter().zip(&images) {
                if let (Some(key), Some(image)) = (key, image) {
                    reusable.insert(*key, image.clone());
                }
            }
            sync.set_references(references.clone(), reusable);
            subscriptions.retain(|id, old| {
                if state.subscriptions.get(id) == Some(old) {
                    true
                } else {
                    sync.unsubscribe(*id);
                    false
                }
            });
            for (&id, &identity) in &state.subscriptions {
                if subscriptions.get(&id) != Some(&identity) {
                    sync.subscribe(id, identity.0);
                    subscriptions.insert(id, identity);
                }
            }
            sync.retain_peers(&state.peers);
            retained.retain(|(id, _), r| {
                state
                    .subscriptions
                    .get(id)
                    .is_some_and(|s| s.1 == r.generation)
            });
        }
        let mut packets = Vec::new();
        for p in input.try_iter().take(128) {
            packets.extend(sync.receive(omsi_net::PROTOCOL as u8, state.id, &p));
        }
        let now = Instant::now();
        packets.extend(sync.tick(
            omsi_net::PROTOCOL as u8,
            state.id,
            (now - last).as_secs_f32(),
        ));
        last = now;
        for incoming in sync.take_incoming() {
            if let Some(&(_, generation)) = state.subscriptions.get(&incoming.owner) {
                let reused = images
                    .iter()
                    .flatten()
                    .any(|image| Arc::ptr_eq(&image.rgba, &incoming.image.rgba))
                    || cache.contains(&incoming.image);
                log::debug!(
                    "LAN freetex: owner={} slot={} {}x{} source={}",
                    incoming.owner,
                    incoming.index,
                    incoming.image.width,
                    incoming.image.height,
                    if reused {
                        "local SHA-256 match"
                    } else {
                        "network pixels"
                    }
                );
                let data = incoming.image.mipmaps.then(|| {
                    Arc::new(TextureData::from_image(omsi_texture::Image {
                        width: incoming.image.width,
                        height: incoming.image.height,
                        rgba: incoming.image.rgba.as_ref().clone(),
                        has_alpha: true,
                    }))
                });
                retained.insert(
                    (incoming.owner, incoming.index),
                    Received {
                        generation,
                        incoming,
                        data,
                    },
                );
            }
        }
        for p in &mut packets {
            p[0] = omsi_net::freetex::MAGIC;
        }
        if packets.is_empty() && retained.is_empty() {
            continue;
        }
        let output = Output {
            packets,
            images: std::mem::take(&mut retained).into_values().collect(),
        };
        match done.try_send(output) {
            Ok(()) => {}
            Err(mpsc::TrySendError::Full(output)) => {
                for r in output.images {
                    retained.insert((r.incoming.owner, r.incoming.index), r);
                }
            }
            Err(mpsc::TrySendError::Disconnected(_)) => break,
        }
    }
}
