# Dedicated openOMSI server — regression acceptance matrix

This document defines **release gates**, not a claim that all gates have passed.
The new core must not replace `openomsi --server` until every required scenario
below has been checked on a clean Linux machine without a display or GPU.

## Automated gates

| Gate | Verification | Required result |
|---|---|---|
| Rust workspace | `cargo test --locked --workspace --no-fail-fast` | No Rust test regressions |
| Dedicated server | `cargo test --locked -p omsi-server --all-targets` | Two-client relay, no phantom host bus, tick/profiler, CPU cache, mirrored lanes |
| Standalone CPU loaders | `cargo build --locked -p omsi-server --bins` | `worldcheck` and `simcheck` link without `omsi-render`, `omsi-scene`, `wgpu`, `winit` or `omsi-audio` |
| Runtime dependency graph | `cargo tree --locked -p omsi-server --edges normal` | No graphical or audio runtime dependencies |
| Style and architecture | Existing PR checks | Formatting, file limits, crate-layering and app-growth guards pass |
| Safety | Existing audit, CodeQL and secret scanning | All checks pass |

The network test spins up a real UDP/TCP host on an ephemeral port and two
ordinary openOMSI client sessions. It asserts the clients' vehicles propagate,
they receive the same map/date/weather and the server itself owns no bus. It
does **not** replace two separate game clients driving a licensed map.

## Isolated experimental CPU AI, passengers and schedules

The **separate** `--experimental-simulation` switch activates the shared
engine's AI road-car physics and scripts, pedestrian population, existing
`TrafficTable` and `WORLD` / `DESC` serializers, and the native
`ScheduleSim` dispatcher for simple road-bus schedules. See
`scripts/server-headless/server-simulation.cfg`. The normal
`--experimental-free-roam` path remains traffic/passenger/timetable-free.

The native server keeps voice and unauthorized teleport disabled. Public
listing, tunnel, map sharing and bus uploads are independent operator opt-ins:
test each combination for reachability and authorization. Map/vehicle transfers
must have explicit configuration consent and pass the authenticated LAN
mod-service and private-content policy. The configurable maximum is 512
ordinary road cars, 2,000 humans and 12 separate scheduled road buses. These
ceilings are not a benchmark or a guarantee of simultaneous active entities.

The CPU-simulation path begins with an entrypoint-oriented tile set and
**additively streams** player-interest tiles without replacing existing lane
indices. The broader route network, entity-aware unloading and Chrono
date-specific map-data migration remain incomplete. It does **not** establish authoritative
player-driven bus input/physics or whole-map, train, articulated consist,
real-time performance, timetable and complete collision parity.

Required external tests: a licensed installed map with usable AI vehicle and
human definitions, actual Chrono timetable content, multiple normal graphical
clients, coordinated stop boarding/CLAIMs, AI near junctions and signals, TPS
under load, late joining, reconnection, server shutdown, and client behavior
outside the initial CPU area. Do **not** mark production-ready if the CI
smoke test alone passes: a synthetic test without installed AI types cannot
prove a populated real-world scene.

## Integration status before Phase 2 review

Phase 1 connects CPU AI spawning, the timetable dispatcher, pedestrian
population, authenticated traffic table, rotating scheduled-bus script
context, world-frame pedestrian/signal snapshots and the local `/reload`
console. The native simulation remains explicitly opt-in and keeps the
legacy graphical dedicated host intact.

The Phase 2 review must include two real graphical clients and a licensed
map with installed AI scripts, road-bus HOF and Chrono data. In particular,
verify actual script context on both clients, ticketing and passenger claims,
time acceleration, a midnight/day rollover, live-reloaded traffic targets
(0 to positive and back), late-join AI ID reuse, reconnects, and duplicate
passenger prevention. The experimental native host now validates and forwards
bounded client-owned `WORLDUP`/`DESCUP` pedestrian updates using isolated IDs.
The synthetic two-client UDP regression covers their relay and removal, but
does not establish full boarding, ticketing or graphical-client parity.
Never mark physics, scripts, or passenger parity complete from synthetic
process tests alone.

## Native headless release updater regression gates

The dedicated server's `/update`, `/update status` and
`/update download` are **local-console-only** operations. They use
a worker thread for the official stable GitHub API and download only
the platform's separate `headless-test` ZIP, never the graphical game
or legacy `--server` package. Require the expected official asset
address, announced size and SHA-256 digest; reject unsupported OS,
missing digest, new releases that are not actually newer, PR builds
and tampered metadata. A failed download deletes its temporary file
without replacing `omsi-server`.

The staged package is activated **offline**, with the host stopped, by
`apply-update.ps1` (Windows) or `apply-update.sh` (Linux), which
recheck SHA-256, run the new binary's `--version` and `--self-test`
before replacing only that binary, keep a rollback backup and do not
touch `server.cfg`, maps, uploaded buses or private user files.
Unlike the in-session `/restart`, this requires a full process restart
and reconnect. The update and test build artifacts remain isolated
from the normal graphical and legacy server releases.

**Required manual acceptance:** successful signed official ZIP,
tampered SHA, incorrect platform, no archive, repeated check/download,
while-server-running refusal, rollback after a broken extracted
executable, and a real client reconnect to the updated host.

## Live configuration reload regression gates

The renderer-free dedicated host accepts `/reload` from the **local
terminal only**; the stdin reader sets a one-shot flag, and the simulation
thread reads the original config filename and validates every change before
applying anything. Automated checks require:

- Valid hot changes to `traffic`, `time_speed`, `passenger_density`,
  `ai_max_scheduled` and `speed_limit_kmh` are accepted.
- Invalid numbers, deleted config, changes to map/ports/auth/transfer/public
  settings, and attempts to enable AI through free-roam are all rejected.
- Rejected reloads do not mutate the live config, traffic/scheduled limits,
  connection endpoints or policies. No user-controlled remote `reload`
  action is available.
- Lowering `traffic` retires excess unscheduled AI cars; currently active
  timetable buses may finish their runs. The session and existing clients
  remain connected.
- A real interactive-console test must verify the command produces an
  applied/unchanged/rejected status in the logs.

## In-session CPU restart regression gates

The local `/restart` consumes a one-shot console request on the tick thread,
strictly validates the candidate settings, and builds a fresh map, simulation,
timetable and streaming worker before replacing any active world state.
It must keep the existing UDP LAN session, host key, join code, authenticated
TCP manifest listener, gateway/tunnel and already connected clients.

The automated separate-process fixture connects two LAN clients, changes
restart-only simulation settings, sends `/restart` to stdin, and verifies the
host stays alive, continues pose relay and serves the same authenticated
manifest. Also verify that a rejected configuration leaves the live state
untouched and repeated restarts do not multiply CPU background workers.

Two **graphical** clients must still validate that old AI and passenger IDs
retire correctly, no ghost entities remain, scheduled buses/timetable data
are usable, and boarding/CLAIM survives or recovers. Configured map and
authentication/transfer-policy changes require a complete process restart.
A restart reopens date-sensitive Chrono data with the current simulated date.
The native host now has an **experimental staged midnight Chrono migration**:
the simulation detects an active Chrono-folder selection change, prepares
a fresh map index, AI definitions, road/wheel geometry, collision set,
stop/timetable data and tile worker off the LAN tick, then swaps the
complete candidate while retaining LAN sockets, join code, connected peers,
publisher history, and the currently streamed player neighborhoods. A failed
build retains the old world and is retried after a bounded delay; a stale
build is never installed after a newer date transition or manual restart.
No rebuild is required when the active Chrono selection is unchanged.

**Still blocking acceptance:** on a licensed Chrono map, verify with two
graphical clients that midnight switches correct active AI lists, tile
patches and timetables without ghost cars, duplicate passengers, broken
boarding/CLAIM, stale client scripts, disconnects or large TPS stalls.
Also check an accelerated clock and an in-session manual restart. A synthetic
Chrono.cfg date-selection unit test does not prove multiplayer continuity.

## Licensed-map CPU smoke and load measurements

```sh
cargo run --locked -p omsi-server --bin simcheck -- /path/to/OMSI2 Maps/Grundorf/global.cfg 19890501 600 16
```

This bounded, standalone command reads a real map's initial tiles, loads
`ailists.cfg` AI vehicle types and road path metadata, then advances the
production traffic and passenger simulation for 600 fixed ticks. It reports
cold-load duration, sampled tick latency and **unpaced offline throughput**.
No GPU, audio or multiplayer socket is used. The selected first tiles may not
represent a populated route and traffic target is deliberately zero while
road collision and schedules are still being migrated. This is **not** a
claim of 60 live TPS, passenger correctness or playable multiplayer.

## Partial CPU static collision and traffic light migration

The headless map loader can publish static `[boundingbox]` and decoded `[collision_mesh]` obstacles into
`WorldData::lists.collision`, using the map's world placement, while
avoiding oversized road-through boxes that would create invisible walls. Exact
collision meshes use the existing O3D loader and simulation MeshShape, with a
bounded cache and rejection of corrupt/oversized files.
`simcheck` reports the remaining collision meshes, attached objects and
missing definitions. **It is not yet complete collision parity**: tilted mesh placements,
road/crossing surfaces, attached/parked objects
and deformable terrain still require the dedicated CPU placement path.

The CPU lane loader can now assign `[use_traffic_light]` indices to
crossing-owned `TrafficLightController` programs, retaining the original
stop/jump phase rules and stable object IDs. The diagnostic assembles these
controllers into `TrafficSim`. Script-driven signal lamps, parented objects,
rail interlocks and timetable integration remain required acceptance tests.

These are **simulation foundations**, not authorization to call
`HostTransport::ready()` for a real map. Missing or unsupported collision
and signal records must stay visible as unresolved, not be silently converted
into boxes, ignored signals or a false-ready server.

## Partial CPU spline road/wheel surfaces

The renderer-free host now builds the production spline triangle meshes and
uses the same `TileSurface` raster and `DriveGrid` wheel-probe logic for its
active tiles. The loader includes splines in the neighboring eight tiles so
a road crossing a tile boundary does not end at the `.map` boundary. It
preserves the `OMSI_HEIGHTPROFILE_GROUND` switch, alpha-only overlays and
the overhead-spline exclusion rules. Tests exercise triangle-height sampling
and road geometry overlapping two adjacent tiles.

Static scenery `[surface]` objects can contribute their first CPU
O3D mesh to the road raster and wheel-probe grid, with proper world rotation
and a bounded mesh-definition cache. The experimental CPU loader now applies
authored `[crossing_heightdeformation]` fields to static crossing road meshes
and object AI lanes, including pitch/bank, and installs explicit
`[terrainhole]` cutters (also on non-drivable tunnel/crossing objects).
Absolute-height objects preserve their authored Z. Tests cover tilted lanes,
barycentric field sampling, the deformed road mesh, raster hole coverage and
rejection of a cutter buried far below the terrain. Animated, attached,
missing, corrupt and oversized surfaces remain unsupported or reported as
incomplete. This is **partial** parity: neither an automated raster test nor
a green headless build establishes visible wheel contact on a real map.

**Not yet parity:** the native executable starts with up to 64
entrypoint-oriented tiles. Restricted free-roam now chooses additional tiles
around connected players (up to 192 active tiles) and rebuilds CPU road lanes,
surfaces, collisions, stops and traffic-light programs on a background map
worker. The UDP networking loop continues ticking while this work runs.
Replaced regions release inactive road rasters and terrain outside the active
neighborhood; current signal clocks are carried across the lane-network swap.

This **free-roam** streaming path keeps autonomous traffic and pedestrian
population disabled. The separate `--experimental-simulation` mode now
**additively** extends the CPU network around actual connected players,
keeping existing lane IDs stable for active AI and timed road buses.
Both modes still lack verified entity-aware unloading, complete crossing
deformation and terrain-hole parity across all map content, graphical-client
passenger boarding and authoritative **player-driven** bus scripts. Rejected
scenery surfaces, invalid splines and missing definitions remain visible
as unresolved counts; the road mesh is not proof of full collision parity.

## AI road height — graphical-client regression gate

This gate addresses the reported issue where AI cars sank into some road
sections but not others. **Use matching client/server builds from this PR**
with a licensed map. At least two players should observe the same AI vehicle:

1. Compare a plain road spline, a sloped crossing (.sco object paths),
   a junction using `[crossing_heightdeformation]`, an elevated bridge
   with `[absheight]`, a tunnel mouth with `[terrainhole]`, and a
   tile-boundary crossing. Include one known bad section from the previous
   build and a known working reference section.
2. On each client record whether the **same AI car** starts sunk, enters
   the pavement as it moves, floats above the asphalt, or has correct
   tire contact. Report if only one client sees the error; that distinguishes
   a client rendering/synchronization problem from shared CPU placement.
3. Compare the same location against the ordinary graphical host, keeping
   the map, date, traffic content and AI vehicle type consistent. Check
   unscheduled cars and scheduled buses separately.
4. During the test, inspect the server's reported skipped/unsupported
   ground geometry and missing definitions. A rejected corrupted mesh must
   not crash the server, silently become a flat road, or authorize
   redistributing any missing mod. Recheck the location after a full
   server-process restart using a build containing the latest commit.
5. Keep at least two clients connected and monitor `tps` and
   `timings report` while crossing these sections. A green geometry
   comparison does not validate a loaded multiplayer server's performance.

**Pass condition:** no new sinking, floating or tire-gap regression at
these locations, and both clients agree on the vehicle's placement.
**Not yet established:** real-map graphical confirmation, dynamic/attached
surface parity, complete spline-cut/deformed terrain parity or
authoritative player-bus physics. Do not merge based only on unit tests.

## Native boot smoke fixture

A new all-targets integration test creates a small synthetic OMSI map with a
single CPU spline and launches the actual native server executable for three
fixed ticks. It deliberately removes DISPLAY/WAYLAND_DISPLAY, uses no real
OMSI map or texture files and checks clean shutdown. This catches missing
binary packaging and bootstrap dependencies missed by library tests, but
**does not replace** a licensed real-world map and two game clients.

An additional cross-process regression launches that same native executable
without a display and connects two independent production-protocol LAN
sessions through its actual UDP port. It verifies both clients receive each
other's vehicle pose, the dedicated host adds no local bus, and a departed
client frees its slot for a replacement. This is synthetic wire-level
coverage, **not** a substitute for two graphical game clients or complete
server-side vehicle authority.

## Experimental independent host process

The Release CI creates **separate** headless-test ZIPs for Windows x64 and
Linux. These contain `omsi-server`, both `server-test.cfg` and
`server-simulation.cfg`, separate `run-test` and `run-simulation`
launchers, and `README.md`. The existing `server-windows` and `server-linux` ZIPs
remain the original `openomsi --server` build. The native test package is
not promoted as a supported production host.


The first **native renderer-free** `omsi-server` executable now exists. It
has a 60 Hz loop, actual network relay, restricted admin/console, CPU-only map
and road data, collision-box checks, traffic-light initialization, and CPU
simulation ticks. **This does not constitute full OMSI server parity**:
player-driven vehicle script ownership, full collisions, client-owned rider
replication and timetable/AI parity are still incomplete. The native
`TrafficTable` plus rotating AI-context pages send scripted line,
destination, HOF stop and delay data using the same LAN encoder as the
graphical host; this requires real-client acceptance testing.

For the **isolated free-roam relay only**, configure `server.cfg` with:
`date = 1989-05-30`, `traffic = 0`, `timetable = 0`,
`passengers = 0`, `share_maps = 0`, `accept_uploads = 0`,
`parked_buses = 0`, `public_server = 0`, `tunnel = 0`, `public_address = off`,
`real_time = 0` and `metar_sync = 0`. Other values fail closed before
listening. The server never executes a remote shell and does not advertise
itself publicly.

```sh
cargo run --locked -p omsi-server --bin omsi-server -- /path/to/OMSI2 server.cfg --experimental-free-roam
```

The `--ticks 600` option bounds a diagnostic run to 600 ticks.
Without `--experimental-free-roam`, the native binary deliberately
**refuses multiplayer admission**, keeping production with `openomsi --server`.
Users must not treat a successful relay connection as proof of vehicle,
map, timetable, or passenger authority.

## Player-bus occupancy and same-line passenger division

The LAN `STATE` wire format already includes an 8-bit player-reported
`Pose.passengers` count (0–255). The headless runtime now retains a
per-connection, per-vehicle ledger of current occupancy and positive/negative
state deltas, separately from the host's AI and walking population. These
deltas are **observed**, not authoritative passenger events: missing
intermediate updates or a replaced vehicle can prevent exact counting.
`GRANT` means only that a waiting person was transferred to the client.

For overlapping player `CLAIM` at the same stop, the host now groups nearby
stationary boarding buses only if their displayed line, destination and
heading agree. It divides server-owned waiting-person grants according to
the buses' initial occupancies and their existing granted shares. The
claim handler updates pending person IDs immediately within a LAN tick so
two drivers cannot receive the same person. Claimants must be stationary
(5 km/h or less) and have open/unknown doors; moving buses and buses with
known closed doors cannot pre-claim a stop they skip. Any departing or
closed-door bus loses its unused stop share, so people who are **still
waiting on the server** remain available to the following vehicle. Tests
cover equal and unequal onboard loads, different/opposite services, distinct
grants, a bypassing bus and a departing bus.

An accepted GRANT is now held in a native server escrow until explicit
client confirmation. `PAXBOARD` commits ownership only once the original
person is inside the player's bus; `PAXRETURN` restores that same person
to its original stop and ID if the client abandons boarding while still
on the ground. `PAXACK` terminates reliable UDP retry for either path.
The host validates the sender and isolates retained people by the actual
LAN connection identity, not just the numeric player ID.

**Acceptance remains blocking:** prove cancellation during active boarding
with two real graphical clients, both lost return/ACK packets, Chrono
reload during an escrow, passenger model/loading failure and reconnect
after leaving. A disconnected or replaced session cannot safely assume
that the rider did not board: current escrow entries are discarded rather
than resurrecting a passenger who might already be inside the offline
player's bus. The graphical LAN-host path still uses the legacy
non-escrow handover and needs a compatibility test.

**Still blocking:** a truthful duty/route graph, destination-aware rider
choice, bus-specific capacity, reconciliation after a GRANT never boards
and physical boarding/alighting continuity on two graphical clients.
Client-reported displays and occupancy alone cannot certify these.


## Player-first headless proximity optimizations (experimental)

The graphical LAN path remains unchanged. The headless host keeps the same
authoritative AI and pedestrian states, original distance cutoffs, per-client
entity caps, ordered selection and wire formats. There is **no new LAN protocol
requirement**. The normal game's established traffic mirror still uses the
host's poses and locally handles rendering/animation; its player bus continues
to run immediately in the client.

- **AI context:** `HeadlessContextSync` builds one short-lived spatial index
  per eligible context update instead of scanning all AI cars separately for
  every player. The stable original list order and 650-metre selection radius
  are preserved before the existing per-peer bandwidth and context budgets.
- **Pedestrians:** `HeadlessPeopleSync` reuses the same index across viewers.
  Each XY broad-phase result still passes the original exact 3D person-range
  filter and original `Place`/ownership restrictions in `PeopleSim`; the
  graphical LAN host's `lan_people` API retains its old behavior.
- **Graphics:** timetable-driver cleanup in the normal renderer uses a set to
  test retained IDs; no driver creation, visibility or animation policy
  changes. The shared `omsi_sim::spatial::SpatialIndex` has unit coverage
  comparing ordered results with a reference linear scan across 512 objects
  and many positions/radii.

**Gates:** compare server p95/p99, CPU and bandwidth at 64/320/512 road cars,
500/2000 people and at least 2/8 connected clients. Confirm per-client
visible entity IDs, DESC/WORLD and AI context ordering are unchanged, and
the legacy graphical LAN host continues working. A data-structure speedup is
plausible, but **not measured or guaranteed** until these benchmarks pass.

**Deferred:** tile unloading with stable lane/route references, frequency-
adaptive AI simulation, player-vehicle server authority, route/capacity
reconciliation and reconnect persistence. None may be called complete or
turned on by default merely because proximity queries are faster.

## Additional native CPU correctness and load checks

**Terrain and AI wheel contact.** The CPU AI ground-height fallback now
respects explicit `[terrainhole]` cutters. If the road raster does not cover
a tunnel cutout, it returns *no ground* rather than silently using the
invisible terrain. Continue comparisons with two graphical clients at
sloped/deformed crossings and tunnels; do not claim animated or attached
surface parity. The unit test uses a rasterized cutter and real flat terrain.

**Passenger ownership.** Every inbound `CLAIM` must originate from a
connected player who currently owns a vehicle and must reference a waiting
person within the server's generous boarding locality (80 m horizontally,
10 m vertically). Cap each claimant to 64 candidate IDs and deny claims
from walkers, invalid poses or distant stops. Reconnecting and receiving a
new ID must never permit a claim on another player's stop. The server's
`WORLDUP`/`DESCUP` relay keeps client-owned IDs isolated; an on-foot
observer receives pedestrians near the *walker's* coordinates, not the last
vehicle coordinates. Synthetic tests are not a substitute for real boarding,
ticketing, door action and disembarkation checks with two graphical clients.

**Streaming.** Populated CPU simulation extends lanes additively while
keeping existing route references stable, but now caps the accumulated
loaded set at 512 tiles. This prevents indefinite map-wide RAM growth; it
does **not** implement safe unloading of lanes held by live AI, and travel
beyond that limit can remain unserved. Test in a large map with separated
players, then review retained-route correctness after travelling and
reconnecting. Free-roam's independent active tile window remains 192.

**Performance.** The live host's previously reported steady 59–60 TPS is
encouraging but is not a 512-car/2,000-person load benchmark. The console
already supports `timings start`, `timings report` and `timings stop`
with p95/p99 and max tick samples. An additional once-per-3600-tick
`native load:` log reports observed AI/people counts and approximate
terrain/raster/wheel data heap. Compare that with OS RSS/peak RSS,
per-core CPU, client network traffic and 10-minute p95/p99 at identical
map/player/AI settings; never label surface heap as total process RAM.

**Player-bus authority is still a separate major milestone.** Current
vehicle traffic states remain client-owned and relayed. Before enabling
server-side physics/scripts, introduce explicit client input/control
messages, authenticated vehicle ownership, deterministic fixed-tick
script/vehicle stepping with locally licensed model definitions, reconciled
state snapshots and a backwards-compatible negotiated protocol. Validate
a two-client door/steering/braking test and an induced-latency test before
replacing the existing relay. Do not silently reinterpret ordinary `Pose`
packets as trustworthy control input or activate an incomplete authority
mode on public servers.

## Manual end-to-end acceptance gates — blocking

1. **No GPU/no display:** on a clean Linux VPS without X11, Wayland, Vulkan,
   DirectX or an audio service, launch the final native `omsi-server` process.
   Confirm zero graphics context initialization. `worldcheck` alone does not
   satisfy this acceptance gate.
2. **Map correctness:** test Grundorf, Berlin-Spandau, a Chrono map and a third
   party map. Compare world coordinates, terrain, crossing/road collision,
   traffic lights, depots, stops and timetable routes with the baseline host.
3. **Vehicles:** two genuine clients enter different buses, drive, change
   liveries, operate doors, scripts and destination displays; confirm state
   agrees at both clients and a new joiner receives complete snapshots.
4. **AI:** compare random/scheduled traffic counts and movement, one-way and
   mirrored roads, intersection right-of-way, rail signals and pedestrian
   crossings. Unloaded distant tiles must not corrupt timetables or AI state.
5. **Passengers:** loading/waiting/boarding/alighting and connection timing
   must match the current server and remain correct across client joins.
6. **Security:** verify kicks, bans, hardware bans, authentication, moderator
   rights, `share_maps = 0`, upload restrictions, protected/private bus files
   and no unauthorized transfers. Never expose a command shell or RCON.
7. **Reliability:** restart and rejoin, client crash/reconnect, bad packets,
   invalid files, host clock change, METAR timeout and headless shutdown.
8. **Performance:** for equal map, script count, AI, weather and number of
   players, record 10 minutes of warm and steady-state TPS, p95/p99 tick
   latency, busiest core CPU%, process RSS/peak RSS and disk I/O. Compare to
   old `openomsi --server`. No speedup claim without measured data.
9. **Distribution:** compare Linux/Windows headless runtime binary and
   dependency size. Keep only permitted OMSI 2 assets and required map/script
   files; never redistribute private vehicles, textures or paid maps.

## Release policy

Any incomplete gate means the PR remains **Draft**. In particular, a
renderer-free `worldcheck` diagnostic, a CPU lane parser, or a connected host
without authoritative bus physics and passenger simulation is **not a
finished dedicated server**. Do not point Docker/production at a diagnostic
binary. Keep the old host working until the new runtime passes all gates.
