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

/// Start trimming low-priority textures before running into the driver's hard budget.
/// A small reserve lets mip downsizing allocate a replacement before the old mip chain
/// is freed, and leaves room for mandatory UI targets and the driver's own allocations.
/// The video-memory setting stays the hard upper bound; this is an earlier trigger.
pub(super) fn card_soft_over(video_limit: u64, card: omsi_render::GpuMemory, step: u64) -> u64 {
    let cap = card.cap(video_limit);
    if cap == 0 || step == 0 {
        return 0;
    }
    // Cap the reserve on large GPUs while still protecting low-VRAM devices.
    let reserve = (cap / 8).clamp(128 << 20, 512 << 20).min(cap / 4);
    let soft_cap = cap.saturating_sub(reserve);
    card.total().saturating_sub(soft_cap) / step * step
}

/// Keep the original 2 ms work slice until the card is near its hard cap;
/// only then spend up to 4 ms on texture shrinking to free allocation room.
pub(super) fn budget_pass_duration(forced: bool, urgent: bool) -> std::time::Duration {
    if forced {
        std::time::Duration::from_secs(3600)
    } else {
        std::time::Duration::from_millis(if urgent { 4 } else { 2 })
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

#[cfg(test)]
mod card_pressure_tests {
    use super::*;

    fn card(total: u64, budget: u64) -> omsi_render::GpuMemory {
        omsi_render::GpuMemory {
            textures: total,
            meshes: 0,
            other: 0,
            budget: Some(budget),
            measured: true,
        }
    }

    #[test]
    fn small_card_trims_before_reaching_hard_limit() {
        let cap = 3_000_000_000;
        let before_pressure = card(2_000_000_000, 3_464_000_000);
        let under_pressure = card(2_850_000_000, 3_464_000_000);
        assert_eq!(card_soft_over(cap, before_pressure, 32 << 20), 0);
        assert!(card_soft_over(cap, under_pressure, 32 << 20) > 0);
    }

    #[test]
    fn unknown_budget_without_limit_does_not_force_shrinks() {
        let unknown = omsi_render::GpuMemory {
            textures: 2_000_000_000,
            meshes: 0,
            other: 0,
            budget: None,
            measured: false,
        };
        assert_eq!(card_soft_over(0, unknown, 32 << 20), 0);
    }

    #[test]
    fn tiny_cap_cannot_reserve_more_than_a_quarter() {
        let small = card(80 << 20, 2_000_000_000);
        assert_eq!(card_soft_over(100 << 20, small, 1 << 20), 5 << 20);
    }
}

/// Rank scenery pictures by their nearest tile while preserving shared texture users.
pub(super) fn texture_budget_entries(
    states: &HashMap<(i32, i32), TileState>,
    gpu: &GpuCache,
    centers: &[DVec3],
) -> (Vec<(f64, PathBuf)>, hashbrown::HashSet<PathBuf>) {
    let ts = tile_size();
    let tile_distance = |k: &(i32, i32)| -> f64 {
        centers
            .iter()
            .map(|c| {
                let (x0, y0) = (k.0 as f64 * ts, k.1 as f64 * ts);
                let dx = (x0 - c.x).max(c.x - (x0 + ts)).max(0.0);
                let dy = (y0 - c.y).max(c.y - (y0 + ts)).max(0.0);
                (dx * dx + dy * dy).sqrt()
            })
            .fold(f64::MAX, f64::min)
    };
    // how near each scenery texture is: the nearest tile that uses it
    let mut near: hashbrown::HashMap<TextureId, f64> = hashbrown::HashMap::new();
    let mut spline_textures = hashbrown::HashSet::new();
    let (mut types, mut splines): (
        hashbrown::HashMap<usize, f64>,
        hashbrown::HashMap<usize, f64>,
    ) = Default::default();
    let (mut trees, mut shared): (
        hashbrown::HashMap<&str, f64>,
        hashbrown::HashMap<&PathBuf, f64>,
    ) = Default::default();
    for (key, st) in states.iter() {
        let d = tile_distance(key);
        let nearer = |e: &mut f64| *e = e.min(d);
        st.gpu
            .types
            .iter()
            .for_each(|t| nearer(types.entry(*t).or_insert(f64::MAX)));
        st.gpu
            .spline_types
            .iter()
            .for_each(|t| nearer(splines.entry(*t).or_insert(f64::MAX)));
        st.gpu
            .trees
            .iter()
            .for_each(|t| nearer(trees.entry(t.as_str()).or_insert(f64::MAX)));
        st.gpu
            .shared_textures
            .iter()
            .for_each(|p| nearer(shared.entry(p).or_insert(f64::MAX)));
    }
    {
        let g = &*gpu;
        let mut see = |p: &PathBuf, d: f64| {
            if let Some(e) = g.textures.get(p) {
                let n = near.entry(e.id).or_insert(f64::MAX);
                *n = n.min(d);
            }
        };
        for (t, d) in &types {
            g.types
                .get(t)
                .into_iter()
                .flat_map(|t| t.textures.iter())
                .for_each(|p| see(p, *d));
        }
        for (t, d) in &splines {
            for p in g.splines.get(t).into_iter().flat_map(|s| s.textures.iter()) {
                see(p, *d);
                spline_textures.insert(p.clone());
            }
        }
        for (t, d) in &trees {
            g.trees
                .get(*t)
                .and_then(|t| t.texture.as_ref())
                .into_iter()
                .for_each(|p| see(p, *d));
        }
        for (p, d) in &shared {
            see(p, *d);
        }
    }
    ground_is_near(&gpu, &mut near);
    let entries: Vec<(f64, PathBuf)> = gpu
        .textures
        .iter()
        .map(|(p, e)| (near.get(&e.id).copied().unwrap_or(f64::MAX), p.clone()))
        .collect();
    (entries, spline_textures)
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
