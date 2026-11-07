use crate::lock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const FRESH: Duration = Duration::from_secs(3);
const MAX_SIDE: u32 = 4096;
const QUALITY: u8 = 72;

#[derive(Default)]
pub(crate) struct Pictures {
    wants: Mutex<HashMap<u64, (u32, u32, Instant)>>,
    last: Mutex<Option<Arc<Vec<u8>>>>,
    seq: AtomicU64,
}

impl Pictures {
    pub(crate) fn want(&self, session: u64, w: u32, h: u32) {
        let mut wants = lock(&self.wants);
        match (w, h) {
            (1..=MAX_SIDE, 1..=MAX_SIDE) => wants.insert(session, (w, h, Instant::now())),
            _ => wants.remove(&session),
        };
    }

    pub(crate) fn forget(&self, session: u64) {
        lock(&self.wants).remove(&session);
    }

    pub(crate) fn wants(&self, session: u64) -> bool {
        lock(&self.wants).get(&session).is_some_and(|w| w.2.elapsed() < FRESH)
    }

    pub(crate) fn largest(&self) -> Option<(u32, u32)> {
        lock(&self.wants).values().filter(|w| w.2.elapsed() < FRESH).map(|w| (w.0, w.1)).max_by_key(|(w, h)| u64::from(*w) * u64::from(*h))
    }

    pub(crate) fn seq(&self) -> u64 {
        self.seq.load(Ordering::Acquire)
    }

    pub(crate) fn last(&self) -> Option<Arc<Vec<u8>>> {
        lock(&self.last).clone()
    }

    fn put(&self, jpeg: Vec<u8>) {
        *lock(&self.last) = Some(Arc::new(jpeg));
        self.seq.fetch_add(1, Ordering::Release);
    }
}

pub(crate) fn rgb_rows(data: &[u8], row: u32, w: u32, h: u32, bgra: bool) -> Option<Vec<u8>> {
    let (row, w, h) = (row as usize, w as usize, h as usize);
    if row < w * 4 || data.len() < row * h.saturating_sub(1) + w * 4 {
        return None;
    }
    let mut out = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for p in data[y * row..][..w * 4].chunks_exact(4) {
            let (r, b) = if bgra { (p[2], p[0]) } else { (p[0], p[2]) };
            out.extend_from_slice(&[r, p[1], b]);
        }
    }
    Some(out)
}

pub(crate) fn encode(w: u32, h: u32, rgb: &[u8]) -> Option<Vec<u8>> {
    if rgb.len() != (w as usize) * (h as usize) * 3 {
        return None;
    }
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, QUALITY).encode(rgb, w, h, image::ExtendedColorType::Rgb8).ok()?;
    Some(out)
}

pub(crate) struct Raw {
    pub w: u32,
    pub h: u32,
    pub row: u32,
    pub bgra: bool,
    pub data: Vec<u8>,
}

pub(crate) fn encoder(pictures: Arc<Pictures>) -> Option<SyncSender<Raw>> {
    let (tx, rx) = sync_channel::<Raw>(1);
    std::thread::Builder::new()
        .name("remote cockpit map".into())
        .spawn(move || {
            for r in rx {
                if let Some(jpeg) = rgb_rows(&r.data, r.row, r.w, r.h, r.bgra).and_then(|rgb| encode(r.w, r.h, &rgb)) {
                    pictures.put(jpeg);
                }
            }
        })
        .ok()?;
    Some(tx)
}
