# Dedicated server: from a game host to a headless process

This is an implementation plan, not a claim that a standalone headless server already ships.
Keep the existing `openomsi --server server.cfg` command, packages and Docker images
working while the host is extracted. No protocol bump is required by the first stage.

## Current coupling

The server is a multiplayer host without a local driver. It runs the
`omsi-app::offscreen` path and creates a wgpu no-op renderer and a Scene.
`offscreen::setup` loads the world, AI traffic, timetable, people, weather,
remote vehicles and administration; `offscreen::step` advances them.

The `omsi-sim` crate has no rendering/network dependency, but the adapters that
populate traffic, create remote vehicles and synchronize their view are still in
`omsi-app`. `omsi-lan::game` already uses a `LanHost` trait for part of this
boundary. Splitting the executable without relocating these adapters would only
rename a graphical application, not create a dedicated server.

## Migration stages

1. **Distinct host tick (this PR).** A dedicated session takes `SERVER_STEP`
   rather than the offscreen capture/test stage list. It keeps the server
   housekeeping, clock, traffic, ground diagnostics, people, LAN and environmental
   simulation hooks, but no longer executes the local-driver/duty, screenshots or
   film-recording stages. The recorder is not constructed in server mode, and
   renderer-only road-snow tracks are not updated. The renderer and scene are
   still allocated: this step does **not** remove wgpu.
2. **Simulation-first world loading.** Extract a host-world loader with no
   `Renderer`, `Scene`, font atlas, texture upload or camera requirement.
   Move AI population, timetable departures and passenger simulation into
   renderer-independent host adapters. Make scenery and bus scripts that need
   text/image hooks use explicit optional host-side backends.
3. **Host multiplayer adapter.** Move server-side network tick and remote
   vehicle handling behind a `DedicatedLanHost` implementation in the network
   / simulation layer. Avoid calling `view_sync`, mesh placement, sound engines
   or any render-side state from this implementation. Preserve the existing
   host-authoritative clock, AI snapshots, positions, chat, content policy,
   vehicle fingerprints, voice relay and admin authorization.
4. **Standalone executable.** Add an `omsi-server` crate/binary whose runtime
   depends on `omsi-sim`, world/content, `omsi-net`, `omsi-lan`, admin and
   platform services, but not `omsi-app`, `omsi-render`, `wgpu`, `winit`,
   `omsi-scene`, `omsi-audio` or graphics/audio native libraries. The process
   owns its fixed timestep, wall-clock pacing, graceful shutdown and state
   persistence. Only after it runs a real map and accepts existing clients
   should Docker/release packaging switch to it.
5. **Compatibility and operations.** Keep the same `server.cfg` keys and
   default security policy. Maintain dedicated-server log/status endpoints,
   admin panel and authentication, public listing, ban/audit storage, tunnels,
   map/vehicle transfer controls and restore after restart. Retain
   `openomsi --server` as a compatibility entry point until the standalone
   binary has passed real-world tests.

## Non-negotiable invariants

- One authority for world time, traffic, passenger state and vehicle ownership;
  clients must not create additional AI/passengers on top of host snapshots.
- The host never needs a GPU, display server, audio device or a dummy GPU device.
- Old clients remain compatible unless an intentional wire-format change is
  documented and coordinated through `omsi-net::PROTOCOL`.
- Never weaken existing upload authorization, private-asset checks, file
  fingerprinting, limits, ban policy, or `share_maps` default.
- Shutdown stops accepting joins, flushes records, releases sockets and exits
  cleanly on SIGINT/SIGTERM; a crashed run does not corrupt persisted state.
- Server metrics distinguish connected players, simulated AI, network traffic,
  memory and tick latency. Player addresses and positions remain subject to
  their current privacy settings.

## Resource utilization and performance objectives

The current wgpu no-op renderer uses no physical GPU, but it still leaves
renderer/scene, visual vehicle instances and presentation-side bookkeeping in
the host process. Removing those can save allocations, CPU work, RAM and package
size; a measured gain is **not guaranteed** and depends on the map, population
and workload. Avoid treating GPU usage as the only source of overhead.

- **CPU:** profile the tick stages and prioritize simulation and packet work.
  At 60 TPS the nominal tick budget is 16.67 ms (including the network tick),
  though elapsed wall time and catch-up behavior must be observed separately.
  Parallelize only independent workloads (visibility/interest updates, asset
  metadata I/O and population planning). Entity ownership, script execution
  ordering and authoritative snapshots must stay deterministic.
- **Memory:** keep CPU-side world collision, lanes, schedules and required bus
  scripting data, but do not retain vertex buffers, GPU textures, scene
  instances, sky lighting atlases or audio buffers. Measure **RSS and peak
  memory**, not merely reserved virtual address space; expose load progress
  and memory pressure rather than using memory unnecessarily.
- **Disk/storage:** ship no unnecessary DX12/Vulkan/Metal/ANGLE libraries,
  shaders, graphics assets or test recordings in the dedicated package. Keep
  game content required for AI, collision, scripts and map/vehicle licensing.
  Read vehicle files/metadata lazily where safe; never infer that removing
  visuals means private models may be redistributed or shared.
- **I/O:** cache immutable path/timetable metadata with explicit capacity
  limits; keep log rotation and audit history bounded. Do not add an
  unconditional per-tick disk flush or synchronous content scan.
- **Visibility/interest:** prioritize players' nearby traffic and people.
  A host-only distant-simulation policy needs tests for bus timetables,
  spawning, handoffs and shared snapshots before enabling.

### Benchmark contract

For a fixed map, number of AI cars, passengers and clients, compare
`openomsi --server` and the extracted `omsi-server` on the **same machine**.
Record warm-up separately from a 10-minute steady-state run and report:

| Metric | How to compare |
| --- | --- |
| TPS (avg, minimum, p95 tick duration) | Completed simulation ticks / wall-clock seconds; 60 TPS is nominal target |
| CPU utilization | Process CPU time and busiest per-thread usage; include total core count |
| RSS and peak RSS | Real process memory; also report growth after clients disconnect |
| Disk I/O | Bytes read/written during cold start and steady operation |
| Server package size | Uncompressed binary + runtime libraries; exclude licensed OMSI 2 assets |
| Network traffic | Bytes/s at equal client counts and interest ranges |
| Simulation correctness | Clock, AI vehicles, passengers, lines and remote buses agree across clients |

No target improvement percentage should be advertised until measured.

## GPU-free world diagnostics (independent process)

`cargo run -p omsi-server --bin worldcheck -- /path/to/OMSI2 Maps/Spandau/global.cfg 19890501`

This checks the CPU map index, Chrono selections, tile references and
object lookup without constructing a wgpu device, scene, player view or audio
engine. It must work on a VPS without a display. It is intentionally named
**worldcheck** rather than `omsi-server`, because it does **not** yet host
players, vehicle scripts, collision tiles, traffic or passengers. The existing
`openomsi --server` still owns those functions until their headless
equivalents pass multi-client simulation regression tests. Do not distribute
the world-check command as if it were a playable server.

## Validation required before switching release artifacts

- Unit tests for host ticking, time-speed changes, map reload/startup errors,
  player join/leave, admin, transfer policy, AI and passenger lifecycle.
- Two clients on one dedicated server: both see each other's vehicles, paint,
  doors, passengers, traffic and timetable positions with the same world clock.
- A server on a Linux VPS with **no display server and no GPU libraries**,
  plus a Windows service/console run and Docker health check.
- Shutdown/restart with existing `server.cfg` and persistent ban/upload store.
- CPU, resident-memory and p95/p99 tick latency measured against the current
  `--server` on the same map, traffic settings and player count.

## Local checks for this stage

```sh
cargo test --locked -p omsi-app frame_stages::tests
cargo test --locked -p omsi-lan
cargo test --locked -p omsi-net
cargo build --locked -p omsi-app
```

The no-op renderer is still used by the existing entry point. Do not describe
this stage as GPU-independent or as a completed server extraction.


## GPU-free real-map CPU simulation smoke test

```sh
cargo run --locked -p omsi-server --bin simcheck -- /path/to/OMSI2 Maps/Spandau/global.cfg 19890501 600 16
```

Arguments: OMSI 2 root, map, date, tick count, and maximum decoded tiles.
The tool loads the installed map, CPU lane network and AI vehicle metadata,
then advances the production traffic and passenger simulation without a
graphics or audio runtime. It uses zero spawned AI cars and zero active
passengers by default because full collision, signals, schedules and passenger
boarding are not yet installed. It reports unpaced CPU tick duration and
p95/p99, not production multiplayer TPS. Multiplayer admission, asset
transfer, scripts, collision overlays and live timetable parity remain
separate blocking gates.
