//! Admission and resolution choices shared by texture streaming and recovery.
use super::*;

pub(super) fn fit_upgrade(data: &Arc<TextureData>, allowed: u64) -> Option<Arc<TextureData>> {
    let mut upload = data.clone();
    while upload.gpu_bytes() > allowed {
        let side = upload.width.max(upload.height);
        if side <= 1 {
            return None;
        }
        let mut small = omsi_render::fit_texture(&upload, side / 2)?;
        if small.format.is_compressed() && (small.width % 4 != 0 || small.height % 4 != 0) {
            let rgba =
                small
                    .format
                    .decode_level(small.levels.first()?, small.width, small.height, true);
            small.format = omsi_texture::PixelFormat::Rgba8;
            small.levels = vec![rgba];
            small.gpu_mips = true;
        }
        upload = Arc::new(small);
    }
    Some(upload)
}

/// Avoid allocating another scenery texture unless it adds detail or saves storage.
/// A compression upgrade at the same resolution is still worthwhile.
pub(super) fn texture_upgrade_worthwhile(
    (width, height): (u32, u32),
    before: u64,
    upload: &TextureData,
) -> bool {
    upload.width >= width
        && upload.height >= height
        && (upload.width > width || upload.height > height || upload.gpu_bytes() < before)
}

impl World {
    /// A worker can finish after the budget has changed. Keep the resident image and
    /// retry later rather than repeatedly allocating the same or a smaller image.
    pub(super) fn defer_texture_restore(&self, path: &Path) {
        if let Some(e) = self.gpu.lock().textures.get_mut(path) {
            e.near_restore = false;
            if e.dropped > 0 {
                e.retry_after = Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
            }
        }
    }
}

pub(super) fn card_restore_room(video_limit: u64, card: omsi_render::GpuMemory) -> u64 {
    match card.cap(video_limit) {
        0 => u64::MAX,
        cap => (cap - cap / 10).saturating_sub(card.total()),
    }
}

pub(super) fn card_near_room(video_limit: u64, card: omsi_render::GpuMemory) -> u64 {
    match card.cap(video_limit) {
        0 => u64::MAX,
        cap => cap.saturating_sub(card.total()),
    }
}

pub(super) fn card_hard_over(
    card: omsi_render::GpuMemory,
    video_limit: u64,
    card_over: u64,
    step: u64,
) -> u64 {
    match (card.budget, video_limit) {
        (None, _) => card_over,
        (Some(budget), 0) => (card.total().saturating_sub(budget) / step * step).min(card_over),
        (Some(budget), limit) => {
            (card.total().saturating_sub(budget.min(limit)) / step * step).min(card_over)
        }
    }
}

pub(super) fn texture_room(limit: u64, usage: u64) -> u64 {
    if limit == 0 {
        u64::MAX
    } else {
        (limit - limit / 100 * 15).saturating_sub(usage)
    }
}

pub(super) fn texture_restore_room(
    limit: u64,
    usage: u64,
    video_limit: u64,
    card: omsi_render::GpuMemory,
) -> u64 {
    texture_room(limit, usage).min(card_restore_room(video_limit, card))
}

pub(super) fn texture_restore_cost(e: &TexEntry) -> u64 {
    if e.bytes == 0 {
        e.full_bytes.min(128 * 128 * 4 * 4 / 3)
    } else {
        e.full_bytes.saturating_sub(e.bytes)
    }
}

pub(super) fn reserved_restore_bytes(gpu: &GpuCache, pending: &hashbrown::HashSet<PathBuf>) -> u64 {
    gpu.textures
        .iter()
        .filter(|(p, _)| pending.contains(*p) || gpu.wants_restore.contains(*p))
        .map(|(_, e)| texture_restore_cost(e))
        .sum()
}

/// A texture shrunk while its tiles were far that is near now comes back whole at once, room
/// or not: the far ones give way for it in the seconds after. (Waiting for room left the
/// buildings right in front of the bus blurred for good on a map that filled the budget -
/// they had lost their levels on the way in.)
pub(super) fn queue_near_restores(
    gpu: &mut GpuCache,
    pending: &hashbrown::HashSet<PathBuf>,
    entries: &[(f64, PathBuf)],
    near: f64,
    (mut card_room, mut squeezed_room): (u64, u64),
    t0: std::time::Instant,
) -> (usize, bool) {
    let mut queued = 0;
    let mut blocked = false;
    for (d, p) in entries {
        if *d >= near || queued >= 24 || pending.contains(p) || gpu.wants_restore.contains(p) {
            continue;
        }
        let Some(e) = gpu.textures.get_mut(p) else {
            continue;
        };
        if e.dropped == 0 || e.retry_after.is_some_and(|t| t > t0) {
            continue;
        }
        let needed = texture_restore_cost(e);
        let peak = e.full_bytes.max(needed);
        if peak > card_room || (e.squeezed && needed > squeezed_room) {
            blocked = true;
            continue;
        }
        card_room -= peak;
        if e.squeezed {
            squeezed_room -= needed;
        }
        e.retry_after = None;
        e.near_restore = true;
        gpu.wants_restore.push(p.clone());
        queued += 1;
    }
    // A nearby texture may not fit back at full resolution although a smaller
    // GPU upload could fit. This fallback offers the closest reduced texture to
    // the existing fit_upgrade path when no full restore has been queued.
    //
    // This is an admission estimate, not a one-mip upload cap: fit_upgrade may
    // restore multiple levels if the upload-time GPU budget permits them.
    // replace_texture briefly holds both allocations. Limit the fallback to
    // one candidate per pass to avoid bursts near the memory limit.
    if queued == 0 {
        let partial = entries
            .iter()
            .filter(|(d, p)| *d < near && !pending.contains(p) && !gpu.wants_restore.contains(p))
            .filter_map(|(d, p)| {
                let e = gpu.textures.get(p)?;
                if e.dropped == 0 || e.retry_after.is_some_and(|t| t > t0) {
                    return None;
                }
                // One additional mip roughly quadruples the stored area. Use a
                // conservative bound (with room for mip/block rounding); the upload size is checked again by
                // fit_upgrade and the renderer before replacing any resource.
                let peak = if e.bytes == 0 {
                    e.full_bytes.min(128 * 128 * 4 * 4 / 3)
                } else {
                    e.full_bytes
                        .min(e.bytes.saturating_mul(4).saturating_add(4096))
                };
                (peak > e.bytes
                    && peak <= card_room
                    && (!e.squeezed || peak.saturating_sub(e.bytes) <= squeezed_room))
                    .then_some((*d, p))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, p)) = partial {
            if let Some(e) = gpu.textures.get_mut(p) {
                e.retry_after = None;
                e.near_restore = true;
                gpu.wants_restore.push(p.clone());
                queued = 1;
            }
        }
    }
    (queued, blocked)
}

pub(super) fn ground_is_near(gpu: &GpuCache, near: &mut hashbrown::HashMap<TextureId, f64>) {
    if let Some(g) = &gpu.ground {
        for id in g.ground_id.into_iter().chain(g.ground_detail.map(|d| d.0)) {
            near.insert(id, 0.0);
        }
    }
}

pub(super) fn budget_level_drop(side: u32, levels: u32, floor: u32, bytes: u64, over: u64) -> u32 {
    let mut n = 0;
    while n + 1 < levels && side.checked_shr(n + 1).unwrap_or(0) >= floor {
        n += 1;
        if bytes.saturating_sub(bytes >> (2 * n).min(63)) >= over {
            break;
        }
    }
    n
}

impl World {
    /// OMSI's `[texmemlimit]`: the scenery and vehicle textures may take `bytes` on the GPU
    /// (0 = no limit), see [`World::update_texture_budget`].
    pub fn set_texture_budget(&self, bytes: u64) {
        self.texture_limit
            .store(bytes, std::sync::atomic::Ordering::Relaxed);
    }

    /// What the game may hold on the graphics card all together (bytes; 0 = no more than
    /// the driver's budget): over it, the textures give way as over their own budget (see
    /// [`World::update_texture_budget`]), since they are what can.
    pub fn set_video_memory_limit(&self, bytes: u64) {
        self.video_memory_limit
            .store(bytes, std::sync::atomic::Ordering::Relaxed);
    }

    /// The textures' budget now (bytes, 0 = none).
    pub fn texture_budget_bytes(&self) -> u64 {
        self.texture_limit
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Whether the textures were over their budget, or the card over the video memory, at the
    /// budget's last look (see [`World::update_texture_budget`]): the time to let go of what
    /// nobody draws rather than make the rest smaller.
    pub fn textures_over_budget(&self) -> bool {
        let limit = self.texture_budget_bytes();
        let in_use = self.budget_checked.lock().in_use;
        limit > 0 && in_use > limit
    }
}

/// Count cached pictures, dropped levels and due retries without allocating a new list.
pub(super) fn texture_budget_snapshot(
    gpu: &GpuCache,
    t0: std::time::Instant,
    last: Option<std::time::Instant>,
) -> (u64, bool, bool) {
    gpu.textures
        .values()
        .fold((0u64, false, false), |(b, d, r), e| {
            let due = e
                .retry_after
                .is_some_and(|t| t <= t0 && last.is_none_or(|c| t > c));
            (b + e.bytes, d || e.dropped > 0, r || due)
        })
}

// Includes both workers and completed pictures awaiting the render thread.
const MAX_PENDING_TEXTURE_UPGRADES: usize = 32;

fn take_front<T>(queue: &mut Vec<T>, count: usize) -> Vec<T> {
    let count = count.min(queue.len());
    queue.drain(..count).collect()
}

/// Reserve part of the batch for restores without starving compression jobs.
pub(super) fn texture_upgrade_batch(
    regular: &mut Vec<PathBuf>,
    freetex: &mut Vec<PathBuf>,
    restores: &mut Vec<PathBuf>,
    room: usize,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut r = take_front(restores, room.div_ceil(2));
    let mut w = take_front(regular, room - r.len());
    w.extend(take_front(freetex, room - r.len() - w.len()));
    r.extend(take_front(restores, room - r.len() - w.len()));
    w.extend(r.iter().cloned());
    (w, r)
}

impl World {
    /// Backpressure: never decode an entire map's queued upgrades at once.
    pub(super) fn take_texture_upgrade_batch(&self) -> (Vec<PathBuf>, Vec<PathBuf>) {
        let room = MAX_PENDING_TEXTURE_UPGRADES.saturating_sub(self.upgrades_pending.lock().len());
        if room == 0 {
            return (Vec::new(), Vec::new());
        }
        let mut gpu = self.gpu.lock();
        let GpuCache {
            wants_upgrade,
            wants_restore,
            ..
        } = &mut *gpu;
        texture_upgrade_batch(
            wants_upgrade,
            &mut self.freetex_upgrades.lock(),
            wants_restore,
            room,
        )
    }

    /// Count actual decoded payload bytes, not the estimated GPU footprint.
    pub(super) fn texture_upgrade_summary(&self) -> String {
        let pending = self.upgrades_pending.lock().len();
        let done = self.upgrades_done.lock();
        let ready = done.len();
        let bytes: usize = done
            .iter()
            .flat_map(|(_, data, _)| data.levels.iter())
            .map(|level| level.len())
            .sum();
        drop(done);
        let gpu = self.gpu.lock();
        let queued = gpu.wants_upgrade.len() + gpu.wants_restore.len();
        let queued = queued + self.freetex_upgrades.lock().len();
        format!(
            "texture upgrades: {queued} queued, {pending} pending, {ready} ready ({:.1} MB decoded)",
            bytes as f64 / 1e6,
        )
    }
}
