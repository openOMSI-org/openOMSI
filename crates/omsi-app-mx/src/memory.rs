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
/// tenths of a discrete card's own memory. The driver's budget, where it tells one, bounds
/// it further at every look (see `World::update_texture_budget`).
pub(crate) fn video_memory_limit(settings: &settings::Settings) -> u64 {
    // (the card's memory is told in MiB)
    let card = omsi_render::ADAPTER_VRAM_MB.load(std::sync::atomic::Ordering::Relaxed) << 20;
    let card = card / 10 * 9;
    let mb = omsi_cfg::flags::OMSI_VIDEO_MEMORY.parse::<u64>()
        .unwrap_or(settings.video_memory as u64);
    let limit = if mb == 0 {
        card
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
    fn video_memory_after_a_lost_device_is_taken_from_the_card() {
        let card = (8192u64 << 20) / 10 * 9;
        assert_eq!(super::safe_video_memory(card, 1), card * 2 / 3);
        assert_eq!(super::safe_video_memory(card, 2), card * 2 / 3 * 3 / 4);
        assert_eq!(super::safe_video_memory(0, 1), 800_000_000);
        assert_eq!(super::safe_video_memory(0, 2), 600_000_000);
        assert_eq!(super::safe_video_memory(500_000_000, 1), 400_000_000);
    }
}
