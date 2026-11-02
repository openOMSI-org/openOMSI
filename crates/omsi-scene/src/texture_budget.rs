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

pub(super) fn card_restore_room(video_limit: u64, card: omsi_render::GpuMemory) -> u64 {
    match card.cap(video_limit) {
        0 => u64::MAX,
        cap => (cap - cap / 10).saturating_sub(card.total()),
    }
}

pub(super) fn texture_restore_room(
    limit: u64,
    usage: u64,
    video_limit: u64,
    card: omsi_render::GpuMemory,
) -> u64 {
    let room = if limit == 0 {
        u64::MAX
    } else {
        (limit - limit / 100 * 15).saturating_sub(usage)
    };
    room.min(card_restore_room(video_limit, card))
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
    mut card_room: u64,
    t0: std::time::Instant,
) -> usize {
    let mut queued = 0;
    for (d, p) in entries {
        if *d >= near || queued >= 24 || pending.contains(p) || gpu.wants_restore.contains(p) {
            continue;
        }
        let Some(e) = gpu.textures.get_mut(p) else { continue };
        if e.dropped == 0 || e.squeezed || e.retry_after.is_some_and(|t| t > t0) {
            continue;
        }
        let needed = texture_restore_cost(e);
        if needed > card_room {
            continue;
        }
        card_room -= needed;
        e.retry_after = None;
        e.near_restore = true;
        gpu.wants_restore.push(p.clone());
        queued += 1;
    }
    queued
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
}
