# Bounded system-RAM texture cache (optional)

The openOMSI GPU budget may reduce mip levels to keep a large OMSI 2 map within
video memory. When a high-resolution picture becomes important again, the
texture worker prepares it for reupload. This change adds a **secondary RAM
cache of previously prepared compressed source data**. It avoids repeated
file decoding / BC compression when an eligible texture is requested again,
while the existing VRAM governor decides which mip levels the renderer can
actually allocate.

This is not RAM-backed live GPU texture sampling or an alternative to the
graphics driver's VRAM allocator. A texture still has to be uploaded to VRAM
to draw. The cache cannot make an over-budget full-resolution image fit.

## Settings

The setting is read once on first use in the current process; restart the game to change it.

| Platform | Default | Override |
| --- | ---: | --- |
| Windows, Linux, macOS | 128 MiB | `OMSI_TEXTURE_RAM_CACHE_MB=256` |
| Android, iOS | Off (0 MiB) | `OMSI_TEXTURE_RAM_CACHE_MB=32` |
| All | 0 disables | `OMSI_TEXTURE_RAM_CACHE_MB=0` |

- Hard maximum **512 MiB**; malformed values revert to the platform default.
- Each admitted texture must fit in one quarter of the configured cache and
  be no larger than 16 MiB. This prevents a single large atlas evicting
  everything.
- Cached data is already block-compressed (BC format); arbitrary uncompressed
  RGBA and script-generated GPU textures are not held.
- Least-recently-used entries are evicted to stay within the limit.
- Runtime-generated RGBA script textures are not cached. File-backed textures
  modified during play are invalidated when their filesystem metadata changes;
  sealed/virtual private resources and sources without a stable timestamp are
  never admitted. The existing separate live freetex upload path is unchanged.
- File size, modification time, content generation and GPU compression
  settings are validated on reuse; an outdated entry is discarded.
- Entries are process-wide and bounded; they remain available between maps
  until evicted, invalidated or the game exits.
- Cache hits avoid decompression and block recompression. The cache checks
  filesystem metadata; direct initial uploads may still run on the render
  thread, so frame-time profiling is required to verify no regression.

## Observability and testing

The cache has internal hit, miss and usage counters for tests; these are not yet
presented in the game's HUD or standard status output. A cold map initially
populates the cache; improvements are expected on repeated loads and restores.

Compare the same route before/after with:

1. `OMSI_TEXTURE_RAM_CACHE_MB=0` (control).
2. `OMSI_TEXTURE_RAM_CACHE_MB=128` (default desktop).
3. `OMSI_TEXTURE_RAM_CACHE_MB=256` on a PC with ample free RAM.

Record RAM usage, VRAM usage, GPU allocations, number of cache hits, texture
quality after 45–60 minutes, and frame-time spikes. Measure on an affected
map/graphics card (for example Delpest 5.2, Ahlheim V5, Stadtedreieck21).
Do not claim better frame rate merely because the cache hits increase.

The restoration scheduling fix in fork PR
[**#398**](https://github.com/isaacsa2/openOMSI/pull/398) is complementary:
it permits a nearby texture to regain intermediate detail before its entire
full-resolution upload would fit. This PR makes *preparation* of that source
less expensive, but does not modify scheduling or the graphics memory limit.

Relevant upstream issue:
[#3888](https://github.com/openOMSI-org/openOMSI/issues/3888).

## Limitations

- The cache covers calls to `vehicle_materials::load_texture_key` with
  compression enabled, including background texture restoration and some
  direct uploads. Fast uncompressed uploads and bump textures bypass it.
- Textures may still appear blurry if GPU VRAM remains full. Driver-reported
  allocated memory can include pools the application cannot force-reclaim.
- No cross-session RAM persistence, no network asset exchange, and no
  changes to OMSI 2 resources.
- Requires build, GPU memory profile, and long-drive testing on hardware
  before claiming an actual improvement.
