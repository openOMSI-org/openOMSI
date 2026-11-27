# Adaptive texture residency for openOMSI

Status: **design and validation plan; full visibility-based eviction is not implemented**.

This document records the next engine steps after the small, independently reviewed
changes listed below. It is intentionally not a performance claim.

## Problem

The engine already streams map tiles, compresses texture data, evicts unused vehicle
sets and shrinks distant scenery mip levels when under GPU-memory pressure. These
systems are not a complete texture residency manager:

- Tile streaming loads the textures of all prepared objects in a loaded tile,
  even if the camera does not show their materials.
- Rendering culls individual instances, but the material's GPU texture resource
  can remain allocated after the instance has left the camera.
- Some file-backed vehicle textures are downscaled by the global budget, and
  the former restoration logic concentrated on scenery; the driven vehicle must
  be protected from persistent quality loss.
- A texture-memory or total GPU-memory limit is a target for reductions, not a
  strict bound while meshes, targets or protected textures occupy the remainder.
- Shader, mirror, shadow, LAN, material variations and GPU command lifetimes all
  constrain when a texture can actually be released.

Related upstream reports:

- [#3206](https://github.com/openOMSI-org/openOMSI/issues/3206): a 4 GB
  setting reportedly does not prevent 8 GB usage on Vulkan after a long drive.
- [#2903](https://github.com/openOMSI-org/openOMSI/issues/2903): exhaustion
  and repeated stutters on a 4 GB GPU even with a lower configured limit.
- [#2295](https://github.com/openOMSI-org/openOMSI/issues/2295): prolonged FPS
  collapse associated with GPU memory pressure.

The OMSI 2 manual documents a reduced-resolution option with an exception for
the player's bus. Its precise texture timeout is **not verified**. A 30-second
idle timeout below is an **initial tuning proposal**, not an asserted fact about
OMSI 2. Similarly, 2 GB is not an inherent universal VRAM cap.

## Preparatory isolated PRs on the fork

- [#362](https://github.com/isaacsa2/openOMSI/pull/362):
  protect player vehicle texture paths and restore previously reduced vehicle mips.
- [#363](https://github.com/isaacsa2/openOMSI/pull/363):
  report current file-backed texture allocation categories and upgrade queues.
- [#364](https://github.com/isaacsa2/openOMSI/pull/364):
  small bounded compressed-vehicle RAM cache.
- [#365](https://github.com/isaacsa2/openOMSI/pull/365):
  prefer shrinking off-camera scenery mips, restoring visible texture detail
  when headroom allows.

These are **not** prerequisites merged into main, and none implements complete
GPU texture eviction. Review and benchmark each independently.

## Architecture

### Separate identity, ownership, and residence

A texture's stable logical ID and file identity must outlive its current GPU
allocation. Its residency state must be explicit:

| State | GPU allocation | RAM/disk | What a material samples |
| --- | --- | --- | --- |
| Resident | Full/appropriate mips | Optional | Ready GPU texture |
| Reduced | Limited mip range | Optional | Smaller usable image |
| WarmRAM | Not guaranteed | Compressed prepared image | Safe fallback/placeholder |
| DiskOnly | Not guaranteed | Existing disk/source | Safe fallback/placeholder |
| Loading | Previous allocation or placeholder | Async worker | Stable current binding |
| Failed | None or previous | Retry/backoff | Safe fallback, never uninitialized |

File-backed resource ownership remains refcounted by tiles, vehicle sets and
dynamic material holders. Residency may be reduced while ownership remains;
only a final owner release may permanently delete the logical resource.

### Priority

1. **P0 / player**: cab, body, displays, active repaint, connected trailers,
   material variations and resources needed by the player camera.
2. **P1 / drawn**: materials visible in the main, XR, triple-screen, mirror and
   reflection views; include alpha/night/bump/light/transmap dependencies.
3. **P2 / predicted**: moving camera's probable next view, imminent tiles and
   near traffic; perform bounded asynchronous prefetch.
4. **P3 / idle**: materials whose last meaningful draw was long ago and whose
   mip levels can be safely discarded first.

A single shared texture used by player and AI must keep the highest required
priority; separating AI-specific low-resolution variants needs explicit
resource keys rather than mutating a shared texture globally.

### Aging, eviction and budget

- Start with **30 s idle grace** only as a tuning baseline; expose an optional
  debug parameter, and shorten the grace under real pressure.
- Use hysteresis for re-admission and eviction to prevent thrashing if the
  player repeatedly turns the camera or oscillates around a view-distance
  threshold.
- Prefer dropping unused highest mip levels before destroying the whole GPU
  resource. If fully evicted, atomically rebind all affected materials to a
  valid small fallback with the right alpha/material semantics.
- Reserve headroom for GPU render targets, meshes, shadow maps, driver allocator
  blocks and in-flight uploads. The absolute limit can be **unachievable** in
  some scenes; report the constrained bytes instead of claiming enforcement.
- Maintain **separate** RAM warm-cache, disk-cache and GPU budgets; do not
  treat the existing 2 GiB disk-cache allowance as a VRAM cap.
- Never stall the render thread on disk I/O, PNG decoding, BC compression or
  waiting for a GPU resource to become idle.
- Budget texture uploads and rebinds per frame, with cancellation/generation
  checks for stale tile, repaint, chronology and season changes.
- On memory pressure, prefer stable reduced detail to a device-loss crash, but
  never allow permanent blur on player-critical textures.

## Implementation increments

1. **Audit hard-cap behavior:** compare settings to actual driver-reported
   usage, Vulkan/DXGI/GL behavior and allocator fragmentation. Distinguish
   reclaimable textures from pinned/player, meshes, targets and in-flight.
2. **Residency registry:** track every file-backed texture's stable owner,
   GPU generation, bytes, mip range, refs, CPU/disk recovery metadata and
   last use. No eviction until safety checks are proven.
3. **Per-view material feedback:** augment visible instances with their active
   materials, handling mirrors, triple-screen, VR, shadows and script changes.
   Start with conservative visibility to avoid missing texture dependencies.
4. **Adaptive mip target:** compute minimum useful mip from projected pixel
   density and scene settings. Apply age/pressure rules with a grace period.
5. **Full eviction and asynchronous restore:** only once stable placeholders,
   resource lifetime, and bind-group rebinding work across all material paths;
   keep old resource alive until in-flight GPU commands retire.
6. **Prefetch and thrash prevention:** predict camera turns, use prioritized
   worker queues, bounded RAM cache and disk cache. Back off on failed loads.
7. **Gameplay/graphics UI:** user options for automatic texture streaming,
   player-first quality (default on) and advanced diagnostics; sensible mobile
   defaults. Add translations for all supported locales via existing process.

Keep one PR per reason, avoid changing simulation logic, and do not bypass
size-budget or architecture checks.

## Telemetry and validation

Record current/peak driver VRAM, GPU resource bytes by class, resident/reduced/
evicted textures, CPU warm RAM, disk cache hit rate, texture reload latency,
upload bytes and rebind milliseconds per frame, number of visible-mip misses,
eviction-to-reload cycles, and OOM events.

Acceptance tests must cover:

- 1/2/4/8 GB budgets where supported; DX12, Vulkan, GL/ANGLE, mobile GPUs,
  and discrete vs shared-memory adapters.
- 15/30/60-minute drives on dense maps with 50+ AI vehicles, passengers,
  mirrors, rain, post-processing and dynamic displays.
- Instant yaw turns, external/cab/VR/triple-screen, camera teleport and
  rapid back-and-forth movement: no flashes or permanently blurred bus.
- Taking over a downscaled AI bus, changing repaints, coupling trailers,
  material variants, seasonal and night textures, and LAN remote vehicles.
- `scripts/bench-window.sh`, `scripts/compare-performance.py`,
  `scripts/golden.sh`, GPU memory telemetry and actual scene captures,
  with pre/post medians and p95/p99 comparisons.

No specific FPS uplift or VRAM reduction should be promised until A/B
measurements confirm it.
