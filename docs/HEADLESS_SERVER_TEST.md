# openOMSI — native dedicated server test package

**EXPERIMENTAL CPU-ONLY host. Not a replacement for `openomsi --server`.**
The default test launcher uses the restricted free-roam mode. A separate
simulation launcher enables bounded AI, passengers and schedules for private
tests, as documented below. This binary is built from PR #405 and does not initialize the graphical
renderer, DirectX, Vulkan, or audio. It uses the normal openOMSI UDP session
transport with a fixed CPU-side tick and a local server console.

## First test on Windows (including a Windows VPS)

1. Extract this ZIP to a separate directory. Keep your existing server
   installation untouched.
2. Install or locate your own licensed OMSI 2 content and verify
   `Maps/Grundorf/global.cfg` exists. With default settings every client already needs the map
   and matching buses. For private tests you may explicitly enable the
   existing authenticated map and bus-mod transfer service (below).
3. Double-click `run-test.cmd` (or supply the OMSI 2 directory as its first
   argument). Enter the complete OMSI 2 folder, for example
   `C:\Program Files (x86)\Steam\steamapps\common\OMSI 2`.
4. Look for the console's UDP port, server status and connection code. Use the
   matching openOMSI PR/release protocol on two normal clients. Connect by the
   code or a reachable direct address; if on another network, the VPS/router
   must permit UDP **27015**, and for WebSocket access TCP **27025**.
5. Use `list`, `tps`, `status`, `timings on`, `timings report` and
   `stop` in the server console. Profile data is collected only after
   `timings on`; it now separates `network`, `interest`, `simulation` and
   `world_sync` execution times. These are stage wall-clock times, not
   OS process CPU percentages.

## Linux test / headless VPS

Extract the native Linux ZIP, then run:

```sh
chmod +x ./omsi-server ./run-test.sh
./run-test.sh /path/to/OMSI2
```

The process does not require an X11/Wayland display or GPU runtime. All
required content remains local. To run from a source checkout instead:

```sh
cargo run --locked --release -p omsi-server --bin omsi-server -- /path/to/OMSI2 scripts/server-headless/server-test.cfg --experimental-free-roam
```

The sample configuration starts with `traffic=0`, `passengers=0`,
`timetable=0`, `accept_uploads=0` and `share_maps=0`.
Public listing, tunnels, verified parked buses and redistribution-authorized
content sharing are independent **explicit opt-ins**, disabled by default
in this example. Full process restart is required to change those network,
fleet and transfer policies. To use another installed map, change only the
`map` entry initially and ensure it contains usable paths.

**Important:** `public_address=off` prevents automatic address advertisement;
it is **not a firewall**. Restrict your server's UDP/TCP ports to trusted
testers using your host/VPS firewall. Do not post the join code publicly.

For a local web admin/TPS panel, set a unique `admin_password` of at least
12 characters in your private `server-test.cfg` and leave
`admin_web=local`. Never publish or commit this password. Open the local
admin URL `http://127.0.0.1:27025/admin/` on the server itself. With a blank
password, web admin is intentionally disabled.

## Optional AI, passenger and timetable simulation (experimental)

The original `--experimental-free-roam` mode remains unchanged, with
`traffic=0`, `passengers=0`, and `timetable=0`. The **separate**
`--experimental-simulation` flag uses the real CPU `TrafficSim`,
`PeopleSim` and `ScheduleSim` from the shared engine:

```sh
cargo run --locked --release -p omsi-server --bin omsi-server -- /path/to/OMSI2 scripts/server-headless/server-simulation.cfg --experimental-simulation
```

The headless-test ZIP now includes `server-simulation.cfg` and
`run-simulation.cmd` (Windows) or `run-simulation.sh` (Linux), alongside
the unchanged free-roam launchers. Run the simulation launcher with your
OMSI 2 installation folder, just as with `run-test`. Alternatively invoke
`omsi-server` directly with the flag and configuration above.

Change the example `map` only to a map actually installed on the test
machine. The native simulation reads its `ailists.cfg`, timetable, road
paths, Chrono data, stop locations and installed AI vehicle scripts.
It runs a bounded number of road vehicles, their existing AI physics and
script callbacks, and pedestrians around connected players. It publishes
cars through the acknowledged LAN traffic table and people through existing
`WORLD` and `DESC` packets. Chrono-based road-bus departures and lines are
experimental; not all consist, train and timetable edge cases are supported.

This test mode permits configuring up to **512 ordinary AI cars**,
**2,000 humans** and **12 scheduled road buses**, without renderer or audio.
The example uses much lower values; these are configurable ceilings, not
guaranteed simultaneous active counts or tested performance targets.
Scheduled buses have a separate budget and are not removed by proximity
recycling. The CPU road network starts with up to **64 entrypoint-oriented
tiles** and supports **additive player-interest tile streaming** while
preserving existing lane indices. Routing across unfinished or untested map
regions may still fail, so test buses and walkers near map edges.

Start with a private LAN/VPS firewall allowing trusted testers. Public
listing and a tunnel are now separate, **explicit operator opt-ins** for
experimental tests; they do not make this a supported production server.
Content sharing stays disabled by default; enabling it intentionally sends
selected files to connected players, so do not use private or paid content
without permission. Neither mode provides authoritative
**player-driven bus** physics/scripts; those remain client-controlled and
validated by the existing network relay. Do not use this experimental mode
for production or publish it as full multiplayer parity.

For a useful manual test, use two matching graphical clients, wait for
AI cars to spawn out of sight, follow them across crossings, approach a
stop with waiting passengers, board a passenger using the existing CLAIM
handshake, then compare traffic, passengers and timetable lines on both
clients. Record CPU/TPS and disconnect/reconnect behavior. Distinguish a
config with no installed AI/human/timetable content from a network failure.


## Public multiplayer testing

To test public discovery through the regular moderated server list, explicitly
enable it in server.cfg and restart the process once:

```ini
public_server = 1
tunnel = 1
public_address = off
share_positions = 0
accept_uploads = 0
share_maps = 0
require_hardware_id = 1
```

The tunnel is optional; with `tunnel=0` you must arrange external reachability
for game UDP and TCP gateway/mod service. Public listing can reject the
server's name, icon or MOTD and requires a reachable gateway. Use moderation,
firewall restrictions and redistribution-authorized assets. Public discovery
is experimental, not proof of complete multiplayer parity.

## Optional private mod-sharing test (server restart required)

The native server reuses the ordinary openOMSI encrypted/authenticated LAN
file service; it does **not** create a new unauthenticated HTTP download
endpoint. For a test where every participant has permission to receive the
content, set these keys explicitly in the active server configuration:

```ini
# Send the map and its non-stock dependencies to joining game clients.
share_maps = 1
# Accept a player's bus, livery and required dependencies for other clients.
accept_uploads = 1
# Optional: verified stored parked buses (up to 32) with uploads enabled.
parked_buses = 0
# Optional: permit downloadable server-owned buses via LISTBUS.
# Only list assets you are explicitly authorized to distribute.
# vehicles = Vehicles/YourBus/YourBus.bus
```

These switches are **independent**: use only `share_maps=1` for map
sharing, only `accept_uploads=1` for client-bus uploads, or both.
With both off (the sample defaults), clients must install their own maps
and vehicles. Missing keys in older configurations do NOT count as
consent: this experimental server refuses to start if the underlying
legacy defaults would enable sharing without an explicit setting.

The file service authenticates to the LAN room, applies trusted-content-root
checks, file type/quarantine limits, hashes and sandboxed downloads. This
does not provide DRM: someone using a modified client can still extract
delivered content. Share only files you own and are allowed to redistribute.
Player-uploaded buses may be retained in the server's LAN store even after
the player leaves, according to `uploads_keep_days`. Test with
public-domain or otherwise redistribution-authorized files first.

**Optional parked buses:** set `accept_uploads=1` and `parked_buses=1..32`
before restarting the server. This reuses the existing verified uploaded-bus
store, claim/hand-over and proximity publication. Parked player buses are
not AI timetable buses and do not gain authoritative player-bus physics.
Keep disabled for content you lack permission to redistribute.

Changes to `share_maps`, `accept_uploads`, `vehicles`,
`uploads_keep_days` or `parked_buses` are restart-only. Running
`/reload` after changing them intentionally rejects the entire reload
without changing the active policy. Stop the server and start it again
using the same `server.cfg`. Never open the mod TCP port to strangers.
The TCP mod service uses the **same numeric port** as LAN UDP (27015
by default); allow TCP 27015 for trusted clients as well as UDP 27015.

## Reload server.cfg without disconnecting players

The running process can re-read the **exact configuration file used at launch**
(`server-test.cfg`, `server-simulation.cfg`, or a custom file). Edit the
file on disk, save it, then enter this command in the **local server terminal**:

```text
/reload
```

The slash is optional (`reload` also works). A successful reload is applied
by the next server simulation tick, without replacing the sockets, game map or
active LAN session. It prints the actual changed setting names to the server
log; an unchanged file prints `no changes`.

The only **hot-reloadable** keys in this experimental native host are:

- `traffic`: random AI population target (up to 512 in CPU simulation).
  Reducing it removes excess unscheduled AI cars; scheduled road buses keep
  their existing trips.
- `passenger_density`: demand factor 0–3, if passengers were enabled at boot.
- `time_speed`: simulation clock speed, 1–30.
- `ai_max_scheduled`: maximum new scheduled road buses (up to 12; 0
  retains the legacy meaning of no configured limit, bounded by the native
  experimental cap of 12).
- `speed_limit_kmh`: 0, 60, 75 or 80.

Changes to **all other** keys (such as `map`, `date`, `time`,
`passengers`, `timetable`, `port`, `web_port`, `max_players`,
`admin_password`, `vehicles`, `accept_uploads`, `share_maps`,
`public_server`, proxy settings and security policy) **require a restart**.
The host rejects the entire reload if it detects any of these changes, even
when the file also includes otherwise hot-reloadable values. It also rejects
missing files, invalid hot values and any attempt to bypass the private-mode
security policy. No changes are applied on a failed reload.

This command is intentionally **terminal-only**, not a Discord/chat command
or remotely invocable web-admin action. Avoid editing a live configuration
while it is half-written; save atomically from your editor if possible. The
initial free-roam mode still requires `traffic=0`,
`passengers=0`, and `timetable=0`; it cannot turn itself into the
simulation mode through `/reload`.

## Update the headless executable from an official release

The native server has its own **local-console-only** updater; the graphical
launcher updater downloads the different game archive, so never use that
installer for a dedicated host. The update check and download run on a
worker thread, not the simulation tick.

```text
/update
/update status
/update download
```

- `/update` (or `/update check`) only checks the latest **stable**
  [official openOMSI release](https://github.com/openOMSI-org/openOMSI/releases).
- `/update status` shows the result of the last check or download.
- `/update download` stages only the matching
  `headless-test-windows-x64.zip` or `headless-test-linux-*.zip` in
  `.headless-updates/` next to the running `omsi-server` binary. A GitHub
  release SHA-256 digest and the exact download URL are **required**: the
  command refuses unsigned/unmatched packages. It does **not** modify the
  running executable, session, map, server configuration or installed mods.

To activate a verified download **after stopping the server**:

**Windows** (PowerShell in the extracted headless server directory):

```powershell
.\apply-update.ps1
```

If local script execution is blocked, use a PowerShell window started with
the script explicitly: `powershell -NoProfile -ExecutionPolicy Bypass -File .\apply-update.ps1`.
**Linux** (terminal in the extracted headless server directory):

```sh
./apply-update.sh
```

Both scripts recheck the downloaded ZIP's SHA-256, extract **only** the
headless executable, check the embedded release version, run `--self-test`,
back up the previous binary (`.previous`) and roll back if replacement
or the new self-test fails. **Neither script runs while the server is
active.** Start the server again with its original launch script after
installation; a full process restart necessarily disconnects clients,
unlike `/restart`, which only rebuilds the CPU world.

The backup is deliberately retained. Before another upgrade, move that
backup to a separate location (or delete it after the new version proves
stable). This update path is for published stable builds, not PR test
artifacts or local Cargo builds; never silently replace a running branch
build with an older official release. No remote Discord, HTTP or LAN
update command is exposed. `OMSI_NO_UPDATE` disables checking.

## Restart the CPU world without dropping the tunnel

After editing the **local** configuration, use `/restart` (or `restart`) in
the server terminal to rebuild the CPU map, AI and pedestrian simulation,
timetable and streaming worker. The process does **not** rebind its LAN
socket/session, regenerate the join code, or recreate the existing web
gateway, tunnel or authenticated TCP manifest listener.

Only `passengers`, `timetable` and `ai_max_humans` are additionally
rebuildable with `/restart`. The `/reload` hot values remain accepted
here too. Changes to `map`, ports, public exposure, mod sharing, fleet
authorization, authentication and other restart-only settings still require
**stopping and launching the entire process**. Validation failure leaves
the current world running; it never partially applies a candidate.

The CPU state is rebuilt, so AI and pedestrian entities can be recreated.
Before relying on this command with players connected, test two real game
clients for synchronized removals, AI/timetable state, passenger boarding,
scripts and reconnects. An automated synthetic LAN process regression does
not prove complete in-game continuity. Date-specific Chrono content is loaded afresh on `/restart` using the
current simulation date. The experimental native host now also **prepares
an automatic Chrono map/AI-list/timetable rebuild** when the active Chrono
folders change at midnight. Preparation happens on a CPU worker, keeps already
streamed player neighborhoods, and applies only a fully prepared snapshot
without replacing LAN sockets, join codes or authenticated mod services.
A failed build keeps the old world and retries after 30 seconds. This does
**not** guarantee continuity of live AI IDs, passenger claims or bus scripts:
test with two graphical clients before treating midnight migration as complete.

To exercise the automatic path, use a licensed map with an authored
`Chrono/<scenario>/Chrono.cfg` date boundary. Configure time shortly before
that boundary, join with two graphical clients, enable experimental
simulation and observe the log's `native Chrono: preparing` and
`native Chrono: installed` messages. Verify the active `ailists`, schedules,
vehicle positions, re-joins, passenger claims and UDP/TCP session continuity
before and after midnight. A map with no Chrono changes does not need
an expensive CPU world rebuild every day.

## Streaming-test addendum: drive beyond the starting area

The native experimental host initially loads 64 tiles around the map's
entrypoints. On a larger map, the interest selector prioritizes player tiles
and their neighborhoods within a 192-tile request window. The populated
simulation retains at most 512 unique tiles across cumulative requests.
A dedicated map worker builds new lanes, road/wheel surfaces,
stop lists, collisions and traffic-light programs **without blocking UDP
keepalives**; the simulation installs the completed region on its next tick.

For a manual test, drive away from the starting area in a compatible client.
Keep another player in the spawn area, then continue across at least several
tile boundaries. Look for `native tile stream:` in the host console and
verify that both players stay connected and their buses continue to update.
Return to the starting area, repeat the journey and reconnect another
client. Note stalls, new invisible walls, missing signals and unexpected
terrain or surface heights. The dedicated CPU road loader now applies
static crossing deformation fields, tilted object AI paths and explicit
terrain-hole cutters, but complex/deformable geometry still needs
side-by-side comparison on licensed maps, especially around crossings,
bridges, tunnel entrances and Chrono content.

The **free-roam relay** can replace its active region (up to 192 tiles)
because it has no server-owned AI. The separate **CPU simulation** mode
extends the road network additively around connected drivers, preserving
lane IDs already held by AI, pedestrians and scheduled buses. Until safe
entity-aware unloading exists, it retains up to **512 unique tiles across
the session**; once full, new distant regions are refused with a warning,
not silently loaded into unbounded RAM. Its map worker operates off the
60 Hz network tick but has not yet demonstrated full lifecycle/unloading
parity. Neither mode authoritatively executes the **player's** bus scripts
or vehicle physics.

## AI cars sinking or floating — ground parity re-test

The original headless multiplayer report described **AI cars partially
sinking at certain road sections** while other sections were normal.
After updating to a server build containing the crossing/ground correction,
test the same positions again. The command `/restart` only rebuilds world
state; it cannot replace an older executable with newer code.

With two graphical clients connected to the **same** native server:

1. Keep the prior problem section and one known-good road section in the
   test plan. Observe an identical AI car on both screens; compare against
   the graphical game's non-dedicated host at that map position.
2. Include sloped intersections, `[crossing_heightdeformation]` junctions,
   `[absheight]` bridges, `[terrainhole]` tunnel mouths and tile seams.
3. Note whether the car is already below the road when spawned, crosses
   into the asphalt while moving, appears to hover, or differs between
   clients. Test both random AI and scheduled road buses.
4. Record `map`, road/landmark, date/time, PR commit and AI model.
   Capture paired screenshots only where the defect is visible. Collect
   a nearby log excerpt and the `tps`/`timings report` output.
5. Do not confuse successful CPU geometry unit tests with full graphical
   wheel-ground parity: animated/attached scenery, some terrain shaping
   and complete player-bus physics remain experimental.

The related checklist is in `docs/DEDICATED_SERVER_REGRESSIONS.md`
under **AI road height — graphical-client regression gate**. This is a
blocking real-world check, not a proven fix on every map.

## Player passenger counts and multiple buses on the same line

The native server now reads the existing `Pose.passengers` field from
each connected player's bus instead of confusing passengers in a **player
bus** with host-owned walking/AI people. Once per minute the console logs
`native player passengers:` for the aggregate and `native player bus #ID:`
for each driver's reported onboard count and changes observed since the
current CPU world was created. A `GRANT` alone does **not** prove someone
boarded: actual onboard changes come from the client's vehicle state. The
wire counter is eight bits and saturates at 255; it cannot distinguish
higher loads or produce an exact event history if state updates are lost.

When two players concurrently serve the same physical stop, advertise the
same line and destination, and face the same direction, the headless host
distributes competing `CLAIM` requests using their initial onboard loads
and the number waiting. A lighter bus can receive a larger share. Players
with unknown/different services are not grouped; a bus that leaves or
disconnects loses its reservation. Each person is handed over only once,
even when multiple CLAIM datagrams arrive in one server tick.

**Skipped stops:** a moving bus (over 5 km/h) or a bus with known closed
doors cannot claim waiting people. A bus that passes or closes its doors
loses its reserved share, leaving *not-yet-granted* people for the next
bus that actually stops and opens doors. With legacy/unknown door telemetry,
a stationary bus may still claim. The native host now keeps each granted person's original state in a bounded
session ownership escrow until the client's physical-boarding confirmation.
The graphical client sends `PAXBOARD` only after the person is actually
inside its own bus. If it moves away from the original stop while a granted
person remains on the ground, it removes the local copy and sends
`PAXRETURN`. The host restores the original person at the same stop and
ID and responds with `PAXACK`; both `PAXBOARD` and `PAXRETURN` are retried
on UDP packet loss, and duplicate requests are idempotent. Authentication
binds each request to the original connected player. A failed client-side
`GRANT` materialization also sends `PAXRETURN`.

**Manual acceptance:** have bus A claim people with its door open, then
pull away before all have entered. Watch native server logs and verify the
remaining people visibly reappear in the original stop's queue; bus B of
the matching line, destination and direction must be able to board them.
Repeat when A drives through without opening doors, A actually boards some
first, and with forced packet loss/reconnect. Nobody already aboard A may
be returned to B. If the client disconnects without returning unboarded
people, the server cannot assume they were never aboard; this still needs
a separate reconnect reconciliation strategy. The ordinary graphical-host
path does not hold the native escrow and needs compatibility validation.

This is a conservative **boarding coordination** layer, not validated
end-to-end route planning. It cannot authenticate a driver's displayed
line, inspect every subsequent stop of the player's current duty, enforce
each custom bus's seat capacity or prove that a granted rider eventually
entered. Test two buses on the same line (equal loads, unequal loads and
opposite headings), doors opening/closing, boarding, disembarkation and
reconnect with real graphical clients before declaring route/CLAIM parity.
Do not change `accept_uploads` or `share_maps` as part of passenger testing:
they are separate explicit server-owner choices.


## TPS, tick latency and memory — practical load test

A steady **59–60 TPS** reported by the live host is a useful baseline;
it is not proof of capacity for 512 AI cars, 2,000 pedestrians, several
players or a complex map. TPS measures completed ticks, whereas the
profiler's p95/p99 reveals short stalls even when the average is 60.

1. Run the server with 2–3 real clients on the same map, date, buses and
   comparable traffic settings as the graphical host. Record the PR commit.
2. Enter `timings start`, drive across several tile boundaries, run
   `timings report` after at least 10 minutes, and then `timings stop`.
3. Monitor the periodic `native load:` log. Every 60 simulated tick-seconds
   it reports measured TPS, active CPU tiles, AI/population counts and an
   **estimated terrain/raster/wheel heap**. This is not total process RAM:
   also record actual RSS and peak RSS externally on the host OS.
4. Repeat at traffic targets of 0, 30, 160, 320 and (only if stable) 512; compare
   passenger demand with and without people. Observe p95/p99/max tick time
   and CPU usage. The 60 Hz tick budget is about **16.67 ms**.
5. If the 512-tile accumulated limit is reached, the host will refuse new
   distant regions rather than accumulate unbounded data. Do **not** treat
   this as complete map streaming: unloaded-region eviction and live
   entity migration are separate work.

The test is a measurement plan; no benchmark numbers beyond those actually
collected from the running system should be claimed.

## Real-client multiplayer test procedure (2–3 drivers)

Use the **same PR #405 client build** on every testing PC, preferably the exact
commit that produced the server package. A stable release may use a different
network protocol or have a different set of player synchronization fields.
The native `headless-test` archive contains **only the server**, not the
graphical clients. Never copy a paid bus or map from one PC to another as a
substitute for a local, licensed installation.

1. Start the server **before** the game clients. Keep its console open. Check
   for `native headless test host ready` and the printed UDP port. If it
   exits during startup, copy the complete console error and stop the test.
2. Start client **A** and join the host by its direct address, or by the
   printed session code if that address is reachable. On a private LAN,
   prefer `<host-local-IP>:27015`. Load the same map installed on the server,
   choose a bus **already present on A**, and move to a known spawn.
3. Start client **B** on a different PC or a separate game process using the
   matching build. Join, load its locally installed bus and park beside A.
   Both clients must independently see **both buses, not a third phantom host
   bus**. Move A, then B; verify each remote bus moves on the other screen.
4. On A change the livery, destination/line and door positions. On B check
   those visible states; repeat B to A. Capture screenshots of the same scene
   if either client disagrees. The server currently **relays** these player
   states; this is not evidence of authoritative server-side vehicle scripts.
5. Close B's session without stopping the server. Verify B disappears for A.
   Rejoin as B or a new client **C** and check that the new join receives the
   surviving bus state. With three simultaneous clients, check each sees
   exactly two remote players. Keep the server's `max_players` above the
   number of test clients, but at or below the documented limit.
6. Run `status`, `list`, `tps` and `timings report` in the host console.
   Record any unexpected disconnects or server errors. Stop the server with
   `stop`, not by terminating its process.

| Scenario | Expected | Result to record |
|---|---|---|
| Join A then B | Both connect to the same world, date and time | Pass / Fail |
| Two moving buses | Each client sees the other's motion, no host bus | Pass / Fail |
| Paint, doors, line, destination | Remote display matches what the sender shows | Pass / Fail, capture mismatch |
| B leaves and C joins | Departed vehicle is removed; slot can be reused | Pass / Fail |
| Three simultaneous clients | Each client sees two remote vehicles | Pass / Fail |
| Security | Default mode transfers nothing; explicit opt-ins transfer only authorized content | Pass / Fail |

When reporting a failure, include OS, PR commit, map and bus identifiers,
whether connection used an IP or code, the **first error message** and the
matching excerpt of the server and client logs. Redact join codes, passwords,
private addresses and identifying player information before posting publicly.

**Current limits:** the free-roam relay still disables AI traffic,
pedestrians and schedules, but can stream distant tiles. The separate
`--experimental-simulation` mode activates bounded AI traffic, passengers,
Chrono road buses and their shared LAN script-context pages on fixed tiles.
Neither has complete server-owned player-bus physics, map/crossing collision,
safe entity-aware tile unloading or verified real-client passenger parity.
Passing this table does **not** make either native mode production-ready.

## Acceptance checklist

- The process loads the licensed map without graphics or sound initialization.
- Two real game clients connect and see each other's bus positions and movement.
- Server `tps` and `timings report` reflect completed CPU simulation ticks.
- `stop` closes the session; reconnects, bans and malformed join attempts
  do not crash the host.
- Default configuration transfers no private map, vehicle or livery. With explicit mod-sharing opt-in, only redistribution-authorized content is served over the authenticated connection.

**Still missing:** authoritative player-bus physics/scripts, complete
attached/animated scenery collision, full deformed-road/train-signal parity,
safe removal of tiles still referenced by active AI and pedestrians, and
graphical-client validation of boarding/CLAIM and handover continuity.
The host already accepts bounded `WORLDUP`/`DESCUP` queues, remaps IDs,
forwards authorized people to nearby peers and validates boarding CLAIM
proximity. These mechanisms still require real-client verification before
the dedicated server can replace the graphical host.
