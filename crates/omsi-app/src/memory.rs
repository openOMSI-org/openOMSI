//! What the machine has and gives back: physical memory, the texture budget, and returning freed memory to the system.

use super::*;
pub(crate) use omsi_platform::memory::physical_memory;
pub use omsi_platform::memory::release_free_memory;


/// The textures' own budget in bytes: `OMSI_TEXTURE_MEMORY` (MB), else an eighth of the
/// machine's memory (2 GB on a 16 GB Mac - Ahlheim's main station needs about 1.5). The
/// setting is the video memory now, everything on the card (see [`video_memory_limit`]):
/// half of it, on a card of its own, when that is more.
pub(crate) fn texture_budget(video: u64) -> u64 {
    let mb = omsi_cfg::flags::OMSI_TEXTURE_MEMORY.parse::<u64>().unwrap_or(0);
    if mb > 0 {
        // (a budget the graphics card cannot hold is taken down to what it can: the
        // settings offer up to 6 GB, and a 2 GB card lost its device at the first frames,
        // #323)
        let card = omsi_render::ADAPTER_TEXTURE_MB.load(std::sync::atomic::Ordering::Relaxed);
        // (a bigger card may take up to half of its own memory when it is set)
        let vram = omsi_render::ADAPTER_VRAM_MB.load(std::sync::atomic::Ordering::Relaxed);
        let limit = if vram > 2560 { card.max(vram / 2) } else { card };
        if limit > 0 && mb > limit * 5 / 4 {
            log::warn!("texture memory {mb} MB is more than the graphics card holds: {} MB", limit * 5 / 4);
            return limit * 5 / 4 * 1_000_000;
        }
        return mb * 1_000_000;
    }
    // automatic: an eighth of the system's memory, but no more than the graphics adapter
    // is taken to hold (a PC with 32 GB and a 4 GB card let the textures grow to 4 GB and
    // the card ran out of memory)
    // (and no more than 2.5 GB: since Windows reads its memory as well - 0.1.237 - a PC
    // with 32 or 64 GB let the textures grow to 4-8 GB, the game took 9 GB at the start and
    // ran at single-digit frame rates, #277; OMSI_TEXTURE_MEMORY still gives more)
    let ram = physical_memory().map(|m| m / 8).unwrap_or(2_000_000_000).min(2_500_000_000);
    let automatic = match omsi_render::ADAPTER_TEXTURE_MB.load(std::sync::atomic::Ordering::Relaxed) {
        0 => ram.min(1_600_000_000),
        g => ram.min(g * 1_000_000),
    };
    let card = omsi_render::ADAPTER_VRAM_MB.load(std::sync::atomic::Ordering::Relaxed);
    if card == 0 || omsi_cfg::flags::OMSI_SAFE_GPU.parse::<u32>().is_some_and(|n| n > 0) {
        return automatic;
    }
    automatic.max(video / 2)
}

/// What the game may hold on the graphics card all together, in bytes (0 = only the driver's
/// budget bounds it): `OMSI_VIDEO_MEMORY` (MB), else the "Video memory" setting - textures,
/// meshes, render targets and the driver's own, not the textures alone as it was - else nine
/// tenths of a discrete card's own memory, or an integrated chip's share (see
/// [`integrated_video_memory`]). The driver's budget, where it tells one, bounds it further
/// at every look (see `World::update_texture_budget`).
pub(crate) fn video_memory_limit(settings: &settings::Settings) -> u64 {
    // (the card's memory is told in MiB)
    let card = omsi_render::ADAPTER_VRAM_MB.load(std::sync::atomic::Ordering::Relaxed) << 20;
    let card = card / 10 * 9;
    let mb = omsi_cfg::flags::OMSI_VIDEO_MEMORY.parse::<u64>()
        .unwrap_or(settings.video_memory as u64);
    let limit = if mb == 0 {
        match omsi_render::ADAPTER_INTEGRATED_MB.load(std::sync::atomic::Ordering::Relaxed) {
            room if card == 0 && room > 0 => integrated_video_memory(room, physical_memory()),
            _ => card,
        }
    } else if card > 0 && mb * 1_000_000 > card {
        // (the "8 GB" choice on an 8 GB card: what it can hold, which is what was meant)
        log::info!("video memory {mb} MB is more than the graphics card holds: {:.0} MB", card as f64 / 1e6);
        card
    } else {
        mb * 1_000_000
    };
    match omsi_cfg::flags::OMSI_SAFE_GPU.parse::<u32>().filter(|n| *n > 0) {
        Some(n) => safe_video_memory(limit, n),
        None => limit,
    }
}

/// The automatic video memory of a chip that shares the system's memory (bytes): its own
/// memory and a quarter of what it shares (`room` MB, `u64::MAX` when the driver tells
/// neither, see `omsi_render::ADAPTER_INTEGRATED_MB`), but no more than a tenth of the
/// machine's memory, and 1 to 3 GB. Without it the driver's budget was all the
/// bound: 8.3 GB of a 14.7 GB laptop's shared memory, which the game filled until the device
/// was lost two minutes into the drive; at 1.5 GB the same drive went on (#3493).
pub(crate) fn integrated_video_memory(room: u64, ram: Option<u64>) -> u64 {
    let share = ram.map_or(1_500_000_000, |r| r / 10).clamp(1_000_000_000, 3_000_000_000);
    share.min(room.saturating_mul(1_000_000).max(1_000_000_000))
}

/// The video memory (bytes) to start with again after the graphics device was lost `n`
/// times: two thirds of the limit, or of what the card holds when automatic (from a fixed
/// 1200 MB, an 8 GB card started again with 800 MB for everything on it), three quarters of
/// that after a second loss; 1200 MB when the card's size is unknown.
pub(crate) fn safe_video_memory(limit: u64, n: u32) -> u64 {
    let base = if limit == 0 { 1_200_000_000 } else { limit };
    let safe = if n >= 2 { base * 2 / 3 * 3 / 4 } else { base * 2 / 3 };
    safe.max(400_000_000)
}


#[cfg(test)]
mod tests {
    #[test]
    fn the_textures_take_half_the_video_memory_of_a_card_of_its_own() {
        use std::sync::atomic::Ordering::Relaxed;
        let own = super::texture_budget(0);
        assert!(own > 0 && own <= 2_500_000_000);
        assert_eq!(super::texture_budget(10_000_000_000), own);
        omsi_render::ADAPTER_VRAM_MB.store(12_288, Relaxed);
        let (big, small) = (super::texture_budget(10_000_000_000), super::texture_budget(own));
        omsi_render::ADAPTER_VRAM_MB.store(0, Relaxed);
        assert_eq!((big, small), (5_000_000_000, own));
    }

    #[test]
    fn an_integrated_chip_gets_a_tenth_of_the_machine_by_itself() {
        let gb = 1_000_000_000u64;
        // (the laptop of #3493: 2019 MB of its own and 7010 shared, 14.7 GB in all)
        assert_eq!(super::integrated_video_memory(2019 + 7010 / 4, Some(14_702 * 1_000_000)), 1_470_200_000);
        // (a small chip on a big machine: what it is taken to have room for)
        assert_eq!(super::integrated_video_memory(1200, Some(64 * gb)), 1_200_000_000);
        // (8 GB in all: no less than 1 GB; 64 GB: no more than 3)
        assert_eq!(super::integrated_video_memory(u64::MAX, Some(8 * gb)), gb);
        assert_eq!(super::integrated_video_memory(u64::MAX, Some(64 * gb)), 3 * gb);
        assert_eq!(super::integrated_video_memory(256, Some(16 * gb)), gb);
        // (nothing told)
        assert_eq!(super::integrated_video_memory(u64::MAX, None), 1_500_000_000);
    }

    #[test]
    fn video_memory_after_a_lost_device_is_taken_from_the_card() {
        let card = (8192u64 << 20) / 10 * 9;
        assert_eq!(super::safe_video_memory(card, 1), card * 2 / 3);
        assert_eq!(super::safe_video_memory(card, 2), card * 2 / 3 * 3 / 4);
        assert_eq!(super::safe_video_memory(0, 1), 800_000_000);
        assert_eq!(super::safe_video_memory(0, 2), 600_000_000);
        assert_eq!(super::safe_video_memory(500_000_000, 1), 400_000_000);
    }
}
