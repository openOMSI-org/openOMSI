# Dedicated server

```
openomsi --root /path/to/OMSI2 --server /opt/openomsi-server/server.cfg
```

The dedicated server is the game binary hosting a session with no window, no sound and no
graphics card. The renderer runs on wgpu's no-op device. The world, AI traffic, timetable buses
and people are simulated exactly as in a hosting player's game; nothing is drawn.

## Get the server

Every release has server packages:

| Package | Platform | Start with |
|---|---|---|
| `openOMSI-<version>-server-linux-x64.zip`, `-server-linux-arm64.zip` | Linux | `start.sh /path/to/OMSI2 [server.cfg]` |
| `-server-windows-x64.zip`, `-server-windows-arm64.zip` | Windows 10/11, Windows Server 2016 or later | `start.cmd C:\path\to\OMSI2 [server.cfg]` |

`scripts/build-server.sh [folder]` builds the same on Linux into `dist/server`.

Give the server a complete original OMSI 2 installation, as the players have it. Put the
server's mods in its `Vehicles`, `maps`, `Sceneryobjects` … folders, as in the game's content
folder. The first start writes `server.cfg`, with a comment for every key.

## Start the server

### The start script that asks

Run `run-server.cmd` (Windows, double click) or `run-server.sh` (Linux), beside `openomsi` in
the server zip.

* First run: it asks for the OMSI 2 folder and checks it is complete. Then it asks for name,
  max players, UDP and web ports, tunnel and admin password (hidden, 12 characters at least).
  The current value is in brackets; Enter keeps it.
* Every run: it lists the installed maps by number (the last one marked, picked with Enter)
  and starts on the map picked. It does not change `map` in `server.cfg`.
* Output goes to the window and to `server.log`, begun anew at every start (see [What the
  server keeps about its players](#what-the-server-keeps-about-its-players)). On an error exit
  it explains the code.
* Asked once per map, it writes `start-<map>.cmd` (or `.sh`), which starts that map with no
  question.

| Option (Windows / Linux) | Effect |
|---|---|
| `run-server.cmd -Setup` / `run-server.sh --setup` | ask for the folder and settings again |
| `-Map maps/<folder>/global.cfg` / `--map …` | start that map with no question |

It remembers the folder, the last map and the `start-` scripts' maps in `run-server.conf`. It
speaks Portuguese on a Portuguese system, English otherwise. It needs no administrator rights;
on Windows it runs in the built-in Windows PowerShell. `start.cmd` / `start.sh` are unchanged,
for a service or another launcher.

### From your own launcher

```
openomsi --root <folder> --list-maps
openomsi --root <folder> --server <server.cfg> --server-map maps/<folder>/global.cfg
```

| Flag | Effect |
|---|---|
| `--root <folder>` | the OMSI 2 folder |
| `--server <server.cfg>` | run as a dedicated server with this file |
| `--list-maps` | print the installed maps (one per line, as `server.cfg` names them, content folder included) and exit. Not a complete OMSI 2: exit code 1 and what is missing; it never uses another installation instead. With `--server`, it also writes a missing `server.cfg` with defaults, nothing more. |
| `--server-map maps/<folder>/global.cfg` | use this map instead of `server.cfg`'s |

* Name a map as in the game, relative to the OMSI 2 folder. Maps in the content folder (mods)
  are found too.
* Pass paths with spaces or accents as they are, quoted.
* The log goes to standard error, informational lines included. Do not treat a line there as a
  failure; the exit code says. `start.sh` also writes it to `server.log`; `start.cmd` shows it in
  its window.

### When it does not start

It prints why and exits with:

| Exit code | Cause |
|---|---|
| 1 | original OMSI 2 not found at `--root` (missing files listed: wrong folder or incomplete installation), or another error, printed as `Error: …` |
| 2 | `server.cfg` cannot be read or written, or its `map` is not installed (installed maps listed, as `server.cfg` names them) |
| 3 | UDP `port` taken by another server or program: stop it or choose another port |

A taken `web_port` is not fatal: the gateway takes another port and the log says which. The
status page, WebSocket players and tunnel move there.

## How players connect

* **UDP** at `host:port`: a server with a public address, or on the LAN.
* **WebSocket** at the web port (`omsi-net::ws`): each datagram is one binary message. The
  gateway gives each WebSocket its own UDP socket on 127.0.0.1.
* **Cloudflare quick tunnel** (free, needs `cloudflared`): `tunnel = 1` runs `cloudflared
  tunnel --url http://127.0.0.1:<web_port>` and prints the `https://….trycloudflare.com`
  address. It carries WebSocket players.
* **Connect by Code**: a player hosting by code opens the same gateway and tunnel. Its address
  goes to the rendezvous topic. A joiner that gets no answer from the code's addresses (routers
  that cannot be punched through) joins through the tunnel.

The session sees a WebSocket player as `joined from 127.0.0.1:<port>`. The log names the real
address: `gateway: player WebSocket from <address> on 127.0.0.1:<port>`. That is the last
`X-Forwarded-For` from a reverse proxy or tunnel on the machine, else `CF-Connecting-IP`.

With Cloudflare's proxy (orange cloud) in front of a proxy on this machine, the last
`X-Forwarded-For` is a Cloudflare edge, so players would share a few addresses (16 connections
each at most). Set `gateway_proxy_header = cf-connecting-ip` to believe `CF-Connecting-IP`
first. Only do this with Cloudflare in front: it overwrites a client's own header.

## server.cfg reference

Default is the value a new `server.cfg` is written with. A key missing from the file (an older `server.cfg`) takes the same value, except where the table says *without the key*.

**World**

| Key | Values | Default | What it does |
|---|---|---|---|
| `name`, `motd` | text | `openOMSI server`, `Welcome! Drive safely.` (`motd` without the key: empty) | shown in the players' server list and on join |
| `map` | `maps/<folder>/global.cfg` | `maps/Berlin-Spandau/global.cfg` | the map |
| `date` | `YYYY-MM-DD` | empty: today | start date |
| `time` | time of day | `08:00` | start time |
| `weather` | a `.owt`, or `cycle` | empty: map's default | `cycle`: one after another through the day |
| `time_speed` | 1 to 30 | `1` | clock speed |
| `real_time` | `0` / `1` | `0` | use the machine's real date and time |
| `metar_sync` | `0` / `1` | `0` | weather follows an airport's METAR, downloaded every ten minutes and passed to players |
| `metar_station` | ICAO code | empty: nearest the map | |
| `radius` | tiles; `0`: whole map | `0` | tiles kept loaded round the map's first entry point |
| `vehicles` | buses separated by `;` | empty: every bus on the server | buses players may drive |

**Traffic and players**

| Key | Values | Default | What it does |
|---|---|---|---|
| `traffic` | number of cars, up to the game's highest Traffic step | `30` | AI cars |
| `timetable` | `0` / `1` | `1` | timetable buses |
| `passengers` | `1` / `0` | `1` | people at stops and on pavements ([Passengers](#passengers)) |
| `passenger_density` | `0` to `3` | `1`; without the key: `pax_density` of `settings.cfg` | how many ([Passengers](#passengers)) |
| `ai_drive_through_parked` | `0` / `1` | `0` | `1`: AI drives through a parked car in its lane, as OMSI 2 does, instead of stopping and going round when it can. For maps with no way round. |
| `ai_unsched_factor` | `0` to `300` | empty: `settings.cfg` | random traffic, percent of the map's density, for the whole server |
| `ai_max_scheduled` | `0` or more | empty: `settings.cfg` | timetable vehicles at once (`0` = no limit) |
| `ai_max_parked` | `-1` or more | empty: `settings.cfg` | parked cars (`0` = every space, `-1` = none) |
| `ai_max_humans` | `1` to `4096` | empty: `settings.cfg` | people at once, for the whole server (not per player) |
| `speed_limit_kmh` | `0`, `60`, `75`, `80` | `0` (off) | RP speed limit ([governor](#optional-rp-speed-governor)) |
| `allow_player_teleport` | `0` / `1` | `0` | `1`: every player may relocate their own bus ([Teleport](#teleport-and-moving-players)) |

**Network and sharing**

| Key | Values | Default | What it does |
|---|---|---|---|
| `port` | UDP port, 1 to 65535 | `27015` | session port |
| `web_port` | TCP port, 1 to 65535 | `27025` | web gateway port |
| `max_players` | 1 to 64 | `16` | player limit |
| `gateway_proxy_header` | `x-forwarded-for`, `cf-connecting-ip` | `x-forwarded-for` | header a proxy on this machine names a WebSocket player's address in |
| `tunnel` | `0` / `1` | `1` | start a free Cloudflare quick tunnel (needs `cloudflared`) |
| `public_address` | empty, an address (`1.2.3.4:27025`, a name), `off` | empty | where players reach the web port without the tunnel; a game joining through the tunnel tries it first. Empty: found by itself (the machine's own internet address, else the one the internet sees if the server reaches itself there, never behind a carrier's NAT) |
| `share_positions` | `0` / `1` | `0` | answer `GET /players` |
| `share_maps` | `0` / `1` | `0`; without the key: `1`, so an older `server.cfg` shares its map | a joiner without the map gets it, with everything it uses, from the server (gigabytes for a big map). Off: players install it and are told which is missing. |
| `accept_uploads` | `0` / `1` | `1` | [players' buses](#players-buses-accept_uploads) |
| `uploads_keep_days` | 1 to 3650 days | `30` | keep an unused player bus this long |
| `public_server` | `0` / `1` | `0` | list in the [public server list](#public-server-list) |
| `public_list_url` | URL | not in a new file; empty or missing: openOMSI's own | optional: list service address |

**Voice** ([Voice chat](#voice-chat))

| Key | Values | Default | What it does |
|---|---|---|---|
| `voice` | `0` / `1` | `1`; without the key: `0` | voice chat at all |
| `voice_radio` | `0` / `1` | `1`; without the key: `0` | session-wide radio channel; `0`: nearby channel only |
| `voice_range` | 5 to 1000 m | `75` | nearby channel reach |

**Administration and bans**

| Key | Values | Default | What it does |
|---|---|---|---|
| `admin_password`, `moderator_password` | 12+ characters | empty: nobody | game, web panel and `POST /admin` login ([Administration](#administration)) |
| `admin_web` | `local`, `remote`, `off` | `local` in a new file; `off` without the key | who reaches the web panel (`/admin/` on the web port), see [below](#who-can-reach-the-web-panel-and-the-api). `remote` needs 16+ character passwords, else acts as `local`. |
| `admin_web_origins` | `https://` sites | empty | other sites whose pages may send requests |
| `admin_trusted_proxies` | addresses separated by `;` | this machine | proxies trusted with `remote` |
| `admin_proxy_header` | `x-forwarded-for`, `x-real-ip`, `cf-connecting-ip` | `x-forwarded-for` (its last address); also for a wrong word | header a trusted proxy names the client in |
| `owner_bans` | file | empty: `~/.openomsi/bans-owner.json` | owner's ban list shared by all your sessions ([Machine bans](#machine-bans)) |
| `require_hardware_id` | `0` / `1` | `0` | turn away games that send no machine identifier |

**Logs** ([What the server keeps](#what-the-server-keeps-about-its-players))

| Key | Values | Default | What it does |
|---|---|---|---|
| `log_max_mb` | MB; `0`: no limit | `20` | new `server.log` past this size |
| `log_keep` | count up to 1000; `0`: none | `5` | earlier logs kept |
| `log_addresses` | `0` / `1` | `0` | write players' addresses whole (for an abuse report) |

`log_max_mb` and `log_keep` are read by `run-server.cmd`, `run-server.sh` and `start.sh`, which write `server.log`. `openomsi --server` run another way, `start.cmd` and Docker ([docker/server/README.md](../docker/server/README.md)) ignore them.

`server-icon.png` beside `server.cfg` (64x64 PNG, under 256 KB) is the icon in the players'
list.

### Passengers

`passengers = 1/0` switches people at stops and on pavements on or off for the whole run. It is
not a count: `passengers = 50` reads as off.

`passenger_density` multiplies the map's own demand (the hourly `[trafficdensity_passenger]` of
its `global.cfg` and each stop's numbers), so busy and quiet hours and stops keep their shape.

* `1` is the map's value, `0.5` half, `2` twice; range `0` to `3`.
* Same factor as the game's `pax_density` in `settings.cfg`, which a server without the key
  keeps using.
* Not a cap on people (that is `ai_max_humans` in `settings.cfg`). It does not change `traffic`
  or timetable buses.
* `0` brings no new people; those already waiting, walking or riding stay.
* Anything else (negative, `NaN`, `10`) is refused with the reason in the log, as if the key
  were absent.

Admins change it live with `passengers <factor>` (Administration menu or `POST /admin`). New
people follow the factor; nobody is removed, boarded trips go on, and players' games mirror the
server, so nobody appears twice. Refused on a server with `passengers = 0` (logged).
`GET /status` shows the density and the people waiting, walking and aboard.

### Voice chat

See [VOICE.md](VOICE.md). A talk key sends the player's voice as small Opus frames. The server
relays them without decoding, needs no audio device and stores nothing.

* *Nearby* channel: players within `voice_range` metres (75 by default), quieter with distance.
* *Radio* channel: everybody.
* Players must also turn voice on in their settings (off by default). Set `voice = 0` to avoid
  the traffic.
* Limits per player: 60 frames a second (talking sends 50), 400 bytes a frame. Frames over
  the rate or the size are dropped (the rate with a log line); the player stays connected.
* `vmute <id>` / `vunmute <id>` (admin or moderator, from the player's page in the
  Administration menu or the Player list). The mute ends when the player leaves.
* Old keys `voice_server_uid`, `voice_channel`, `voice_channel_password` are ignored, with a log
  line. A `voice_range` under 5 m is raised to 5.

## Web port endpoints

| Request | Needs | Answer |
|---|---|---|
| `GET /status` | | JSON: name, motd, map, players, max_players, time, weather, version, protocol, vehicles. A dedicated server adds `world`: AI cars, buses, cars asleep, parked cars, people walking, waiting and aboard, traffic density, whether it has passengers, current passenger density. |
| `GET /icon.png` | | server icon for the launcher's Multiplayer → Servers list |
| `GET /players` | `share_positions = 1` | player positions, refreshed every second |
| `POST /admin` | `admin_password`, loopback only | admin commands from a local tool |
| `/admin/`, `/api/admin/` | `admin_web` | [web panel](#the-web-panel) and [web API](#web-api) |

`vehicles` in `/status` is the `vehicles` list, else every bus. A joiner's launcher offers only
those, as do the game's *Place a vehicle* and *Swap* after joining.

**`GET /players`** is off by default, so names and positions stay private. It returns a JSON
array, enough for a live map on a website. Fields: `id`, `name`, `bus`, `line`, `destination`,
`tour`, `x`/`y` (world metres east/north), `heading` (degrees clockwise from north),
`speed_kmh`, `on_foot`, `aboard`, `lat`/`lon` (on a `[worldcoordinates]` map such as
Berlin-Spandau, `null` elsewhere). A player on foot is where they walk, even with their bus
parked nearby. A player in another player's bus is with that bus (its id).

**`POST /admin`** lets a local tool (dispatch page, script) administer without joining:

* One command per line, as the Administration menu sends them: `clock 30600`,
  `weather set Weather/#CAVOK.owt`, `say …`, `kick 3`.
* Password in `X-Admin-Password`; admin role. Answers `202` when taken.
* Commands run like all others and go to the audit log with their result.
* A reverse proxy on the same machine also connects from 127.0.0.1: do not let it pass `/admin`.

## Players' buses (`accept_uploads`)

With `accept_uploads = 1` (default) a player may drive a bus the server lacks. Their game sends
the bus files to the server, which keeps them and passes them on, so nobody sees a stand-in.

* **What is sent:** only files the game read for that bus: the variant and paint driven, shared
  folders its files point into (`..\..\Anzeigen`, shared sounds), and its display fonts. Never
  the whole pack, stock OMSI 2 content, or files from the server. Programs, libraries and
  plugins are refused by name and content, as for the server's mods.
* **When:** under the joiner's loading screen ("Getting your bus to the server", with
  progress), so others see the real bus from the first metre. After three minutes the loading
  screen lets go; the rest continues in the background, shown as a card over the HUD. A bus or
  paint changed in game is sent the same way.
* **Other players:** each game announces its bus by fingerprint (hash of its file list). A game
  uses its own copy if identical file for file; otherwise it fetches that fingerprint from the
  server ("Downloading another player's bus: 40%"), never another version at the same path.
  Joining downloads only the server's mods (and map, with `share_maps`); players' buses come
  when those players are drawn. A bus still uploading is waited for. At most three download at
  once.
* **Transfer:** 1 MiB pieces both ways (a small file is one piece), four connections at once,
  each piece compressed when smaller (uncompressed `.tga`/`.bmp` drop to a few percent;
  `.dds`/`.png` hardly shrink). Pieces are sealed on arrival, so a broken transfer resumes,
  losing at most pieces in flight, whatever the file size. A file counts once all pieces are in,
  its SHA-256 matches its list and it passes the content checks. A bus is drawn once every file
  is in. A tampered piece or wrong file is discarded and fetched again, never drawn.
* **Storage:** taken without asking, no size limit, kept across restarts. Each file is stored
  once in `~/.openomsi/lan-store` by SHA-256 (shared files once). Each bus is a file list in
  `lan-store/vehicles/<fingerprint>.json`; the server computes the fingerprint from what it
  checked. Two versions of a mod are two buses, both kept; each player's bus is drawn in that
  player's version.
* **Clean-up:** an undriven bus is kept `uploads_keep_days` (30) after last use. When the disk
  runs short, undriven buses go first, least recently used first, until the file fits. A bus is
  refused only if the disk still cannot hold it. A bus being sent, with its files, is never
  removed. To forget a bus, stop the server and delete its `.json`.
* Empty `vehicles`: players may bring any bus. With a `vehicles` list, only those are taken.
* `accept_uploads = 0` turns it off. A game hosting a session (`--lan-host`) takes joiners'
  buses the same way and keeps its own bus in its store for them.

## Optional RP speed governor

`speed_limit_kmh` advertises `60`, `75` or `80` km/h to players. `0` (default) keeps normal
controls. An unknown preset logs a warning and disables the limiter.

* Clients ask the host periodically, including after reconnect.
* Throttle drops progressively over the last 5 km/h before the limit.
* Above it, a small braking assist counters downhill acceleration. Steering, reverse and normal
  braking still work. Velocity is never abruptly clamped.

**Limitation:** the policy is an authenticated host-sourced LAN command; older clients ignore
it. It is **cooperative client-side RP behavior**, not anti-cheat: modified clients can ignore
it, and the host does not yet reject overspeed poses. For competitive enforcement, verify client
versions; overspeed detection is follow-up work. Restart the server after changing
`server.cfg`.

## Administration

### Passwords and roles

| Key | Role | May |
|---|---|---|
| `moderator_password` | moderator | see the server and players; send a player away (not an admin); chat; a word or notice to one or all (`tell`, `notify`); duties; services; right a player's bus (`unstick`); go to a player (`goto`, moves own bus) |
| `admin_password` | admin | all of the above, plus the world (clock and speed, weather, AI traffic, passengers) and bringing players to their bus (`bring`, `bringall`) |

* Passwords need 12+ characters. A shorter one lets nobody in (logged), so a server set up with
  8 to 11 needs a longer one. Reason: the game login is unencrypted, so its challenge, salt and
  rounds can be overheard and a short password guessed offline.
* The server alone decides the role from the password. Nothing a game or page sends can name or
  raise it. Every operation is checked server-side, shown in a menu or not.
* A `moderator_password` equal to `admin_password` gives the admin role. No password: no
  administration.

**Login locks.** In the game and on the web panel, five wrong passwords in two minutes from one
source lock that source for a while. Thirty from all sources together lock everybody (on the
web, everybody except this machine, so a flood from afar cannot keep the operator out). The game
and the web panel count separately: a lock on one is no lock on the other. In the game, the
source is the player's address (behind the gateway, for a WebSocket player) or their number on
this machine. `POST /admin` closes for a while after five wrong passwords in two minutes.

### Log in from the game

Type `/admin <password>` in the chat. The password never crosses the network: the game asks
`auth?`, the server sends a fresh challenge, salt and rounds, and the game proves it knows the
password (PBKDF2 and an HMAC of the challenge).

The server answers `admin-ok admin` or `admin-ok moderator` (pre-role games read the first
word). Esc → *Administration* then sends each line as `admin <action>`. Rights end when the
player leaves or talks from another address.

A pre-role game given the moderator password shows the old full admin menu. The server still
refuses world actions, but that game ignores the `admin-result` answer, so nothing visible
happens. Moderators should run a current game.

### The Administration menu

On a dedicated server, Esc → *Administration* shows the server's own menu:

* your role, the clock, the weather;
* *Players...*: every player by name and number, with activity and vehicle, searchable;
* a player's page; sending away (now, or for the run) asks for confirmation naming the player,
  number and connection;
* clock, speed, next weather, traffic and passenger density with current values, or why they
  are locked (moderator role, real time, METAR, no passengers);
* results of the last operations.

The menu talks to the server over the session, so the server may be anywhere; it never uses a
separate address. It polls every 3 seconds at most while open, one request at a time, and says
when no answer comes. Against an older server, *Full list of commands (older menu)...* keeps the
old lines. The selected line keeps its action when answers update. A selected player who left
or came back is not acted on. *Stop administering* ends the rights. The search line needs a
keyboard; the list does not. A game hosted by code keeps its old menu.

### Teleport and moving players

**Own bus.** On a dedicated server, an admin or moderator can relocate their own bus without reconnecting: Esc →
Service → Teleport to a start point (e.g. Bauhof to Kraken), F4 map click, or the navigator/city
map teleport. The host validates the sender and returns a `teleport` command; the client applies
the usual host-teleport reset.

`allow_player_teleport = 0` (default) limits this to logged-in moderators and admins. `1` lets
every player relocate **only their own bus** the same way. It never allows moving others, the
clock or map editing. Old clients do not support the request.

**Other players.** Moderators have `goto` and `unstick`; admins also `bring` and `bringall`, in
the Players menu.

* `bring`, `bringall` and `goto` are for an admin driving on the server: the player's bus is put
  beside the admin's, with trailers. A player on foot is put back at the wheel first.
* What happens to other players sitting in a moved bus is not checked.
* They need the admin's bus and are not safe in every case, so the web panel and new panels omit
  them.

### How operations are applied

Every operation (game menu, `POST /admin` as admin, web panel) is checked against the role, run
by the server loop on its tick (never a web thread) and recorded:

| Status | Meaning |
|---|---|
| `queued` | not run yet |
| `applied` | done: the kicked player is gone, the clock shows the new time |
| `pending` | the server did its part; the rest is up to a player's game (a word, a duty) |
| `rejected` | with reason: no such player, no longer the player picked, role not allowed, clock on real time, weather on METAR, weather not installed, number out of range |

A game that asked gets `admin-result <record> <status> <message>`. Taken is not done.

### Commands

Name a player by number **and** connection: `kick 3#17 reason`. The connection is the session's
number for that join, shown in the admin list. If the player left, rejoined (same number, new
connection) or someone else has the number, the operation is refused and nobody is touched.
Older menus' `kick 3` (number only) still works.

| Command | Role | What it does |
|---|---|---|
| `kick 3#17 reason` | moderator (not on an admin) | send away; they read the reason (or "sent away by an admin") and may rejoin |
| `ban 3#17 reason` | moderator (not on an admin) | keep out for this run, by the game's random nonce and source address |
| `hwban 3#17 session\|all [12h\|3d] reason` | moderator for `session`, admin for `all` | [machine ban](#machine-bans) |
| `unban <id>` | admin | lift a machine ban |
| `say …` | moderator | message to all |
| `tell 3#17 text` | moderator | private word to one player |
| `notify all\|3#17 <notice-id> <seconds> info\|warn\|alert text` | moderator | notice on the screen of one player or all |
| `duty 3#17 <line> <tour> <trip> <stop>` | moderator | give a player a duty (`%20` for a space in a name; trip and stop by number) |
| `duty-off 3#17` | moderator | take a player's duty back |
| `service repair\|refuel\|wash [all\|3#17]` | moderator | repair, refuel or wash one player's bus, or all (the default) |
| `vmute <id>`, `vunmute <id>` | moderator (may not mute an admin) | mute or unmute voice |
| `unstick 3#17` | moderator | right a player's bus |
| `goto 3#17` | moderator | move own bus to a player |
| `bring 3#17`, `bringall` | admin | bring one player, or every player, to own bus |
| `clock 30600` | admin | set the clock (seconds after midnight) |
| `time 3600` | admin | move the clock on (or back with a minus), at most a day |
| `speed 2` | admin | clock speed, 1 to 30 |
| `weather set Weather/#CAVOK.owt` | admin | set the weather |
| `weather next`, `weather cycle` | admin | next installed weather (a server has no cycle of its own: `cycle` does the same) |
| `traffic 40`, `traffic clear` | admin | AI traffic density (up to the game's highest Traffic step), or every AI car off the road (timetable buses stay) |
| `passengers <factor>` | admin | set passenger density |
| `admin-list [search]` | moderator | list players |
| `admin-state` | moderator | server state |

`ban` is not an identity: names are whatever the game says, addresses change and are shared,
and the session knows nothing that outlives a run and could not be made up. So `ban` lasts the run only; for a lasting ban, use the
[machine ban](#machine-bans).

`admin-list` answers one `admin-p <id> <conn> <state> <role> <aboard> <online s> <vehicle>
<name>` line per player, then `admin-end <found> <total>`. No position or address, so no
`share_positions` needed. Search is case-insensitive substring match on names (`pichau` finds
`Pichau` and `pichau_fan`, exact name first), plain text, never a pattern or command, and never
picks for you.

`admin-state` answers `admin-s` with clock, speed, weather, traffic, passenger density and
counts, players and locks (`rt=1`: real time, `metar=`: METAR station).

### The web panel

Open `http://127.0.0.1:<web_port>/admin/` (with the slash) on the server's machine, or through
your HTTPS proxy with `admin_web = remote`. It is part of the server: nothing to install, same
API and checks.

Log in with the admin or moderator password. It goes to the server only and is stored nowhere;
the token lives in that browser tab until you close it or log out.

It shows the server (map, players, clock and speed, weather, traffic, passenger density, people
waiting, walking and riding; "real time" and "METAR" when they drive clock or weather), players
with search (name or `#number`), actions on the selected player (send away, for this run, a
word; each confirmed with name, number and connection), an announcement, world controls with
current values (disabled with reason for moderators, real time, METAR, no passengers), and each
operation's result (queued, applied, sent, rejected and why).

It polls every 3 seconds while the tab is visible, slower when the server does not answer (and
says so). A selected player who left or rejoined is not acted on. English and Portuguese,
desktop and phone.

Try it without OMSI 2: `crates/omsi-lan/examples/admin_demo.rs` runs a session with a few
players and the panel; `crates/omsi-admin/panel/test/` drives it in a browser.

### Who can reach the web panel and the API

| `admin_web` | Access |
|---|---|
| `local` (new files) | this machine only, directly. Requests via a tunnel or reverse proxy (with `X-Forwarded-For`, `CF-Connecting-IP` …) are refused, the server's own tunnel included. `tunnel = 1` does not expose administration. |
| `remote` | also via proxies in `admin_trusted_proxies` (addresses separated by `;`; this machine by default), HTTPS only |
| `off` | no panel or API; `POST /admin` unchanged. A missing key (pre-panel file) or wrong word means off, so an update opens nothing unasked. Add `admin_web = local` to enable. |

**With `remote`:**

* HTTPS means `X-Forwarded-Proto: https`, or Cloudflare's `CF-Visitor` with
  `admin_proxy_header = cf-connecting-ip` only (behind another proxy it could be the client's
  own). Plain HTTP from another machine is refused.
* The client address is believed only from trusted proxies, and only from `admin_proxy_header`:
  last `X-Forwarded-For` address by default (Caddy, nginx, Cloudflare), `x-real-ip` or
  `cf-connecting-ip`. Other headers may be the client's own. That address counts wrong passwords
  and appears in records. A request naming no client, or this machine, is refused.

| Proxy | Setting |
|---|---|
| Caddy | default (it adds the client to `X-Forwarded-For`) |
| nginx | `proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;` with the default, or `proxy_set_header X-Real-IP $remote_addr;` with `admin_proxy_header = x-real-ip`. Otherwise nginx passes the client's `X-Forwarded-For` unchanged. |
| Cloudflare proxy (orange cloud) before your proxy | `cf-connecting-ip` (the last `X-Forwarded-For` is Cloudflare's, so clients would share a few addresses for the locks) |

**Security warnings:**

* With `tunnel = 1`, the local cloudflared is a proxy on this machine, trusted by default: under `remote` the panel is on the
  public tunnel address, guarded only by passwords. Your own proxy (Caddy, nginx with a
  certificate) in front of `127.0.0.1:<web_port>` is safer. It must not pass `/admin` (the local
  tool's door).
* A reverse proxy on this machine must add `X-Forwarded-For` (or the `admin_proxy_header`
  header). Without it, internet requests look local and cannot be told apart. nginx's plain
  `proxy_pass http://127.0.0.1:<web_port>;` does this (sends `Host: 127.0.0.1:…`, no address):
  add `proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;`.
* A tunnel below HTTP (`ssh -R`, frp, playit, `ngrok tcp`) makes every client look local, with no
  header. Anyone reaching the web port meets the login as if local (passwords and locks still
  apply). Point such tunnels at the game port only, or keep `admin_web = off`.
* Locally the panel answers only to `127.0.0.1`, `localhost` and `[::1]`, which blocks DNS
  rebinding.
* `admin_web_origins` lists other `https://` sites allowed to send requests (e.g. a dispatch
  page). Other browser origins are refused. No response allows cross-site reads (no
  `Access-Control-Allow-Origin`).

**Sessions:** end after 30 minutes idle, after 12 hours, at logout or when the server stops. At
most 16 at once; a moderator login only displaces moderator sessions, never an admin's. A wrong
password is logged in the audit log; a lock is logged once, not each attempt.

### Web API

Served under `/api/admin/` (panel under `/admin/`) on the web port. All JSON. Token in
`Authorization: Bearer <token>`, never a cookie or URL. Changes are `POST` with
`Content-Type: application/json`.

| Request | Body / query | Answer |
|---|---|---|
| `POST /api/admin/login` | `{"password": "…"}` | `200 {"token", "session"}`; `401` wrong password; `429 {"retry_after"}` too many lately; `503` a moderator login while every session is an admin's; `404` no password set |
| `POST /api/admin/logout` | `{}` | `200`: the session ends |
| `GET /api/admin/session` | | `200 {"session"}` |
| `GET /api/admin/state` | | `200 {"session", "server", "age_ms"}` |
| `GET /api/admin/players` | `?q=<search>` | `200 {"players", "total", "age_ms"}` |
| `POST /api/admin/ops` | an operation, below | `202 {"op"}` queued; `400` not one (with reason); `403` not for this role; `503` too many waiting |
| `GET /api/admin/ops/<id>` | | `200 {"op"}`; `404` not kept any more (the audit log has it) |
| `GET /api/admin/bans` | | `200 {"bans": [{"id", "name", "reason", "by", "at", "until", "scope"}], "age_ms"}`, no identifiers |
| `GET /api/admin/events` | `?audit_after=<seq>` | `200 {"ops": the last 50 records, "audit": the audit entries after seq}` |

| Object | Shape |
|---|---|
| `session` | `{"role": "admin"\|"moderator", "permissions": ["view", "moderate", "world"], "actor": "web#2 (address)", "expires_in": seconds}` |
| `server` | `{"name", "map", "version", "players", "max_players", "time": "HH:MM", "clock": seconds of the day, "clock_speed", "real_time", "weather", "metar": station or null, "traffic", "traffic_max", "timetable", "passengers", "passenger_density", "passenger_density_max", "waiting", "walking", "aboard", "cars", "buses"}`, as published by the loop about a second ago (`age_ms`; stale if the server stopped publishing) |
| player | `{"id", "conn", "name", "state": "joining" \| "driving" \| "on_foot" \| "aboard", "vehicle", "line", "tour", "aboard": the driver's id or null, "online_secs", "admin": role or null}`, no position or address |
| operation | `{"id", "actor", "op", "summary", "target": {"id", "conn"} or null, "target_name", "status": "queued" \| "applied" \| "pending" \| "rejected", "message", "at"}` |

Operations (`POST /api/admin/ops`):

| Body | Role |
|---|---|
| `{"op": "kick", "target": {"id": 3, "conn": 17}, "reason": "…"}` (reason optional, 120 characters) | moderator |
| `{"op": "ban", "target": {…}, "reason": "…"}` (this run) | moderator |
| `{"op": "hwban", "target": {…}, "reason": "…", "scope": "session"\|"all", "hours": 24}` (`scope` default `session`; `hours` 1 .. 8784, none: for good) | moderator for `session`, admin for `all` |
| `{"op": "unban", "id": "0badf00d"}` | admin |
| `{"op": "say", "text": "…"}` / `{"op": "tell", "target": {…}, "text": "…"}` (160 characters) | moderator |
| `{"op": "clock", "seconds": 30600}` (0 .. 86400) / `{"op": "shift", "seconds": -3600}` / `{"op": "speed", "factor": 2}` (1 .. 30) | admin |
| `{"op": "weather", "next": true}` / `{"op": "weather", "file": "Weather/#CAVOK.owt"}` | admin |
| `{"op": "traffic", "density": 40}` (0 .. the game's top) / `{"op": "traffic", "clear": true}` | admin |
| `{"op": "passenger_density", "value": 1.5}` (0 .. 3) | admin |

Requests return `queued` at once; the loop runs them on its next step and the record shows the
result (`GET /api/admin/ops/<id>`). Web threads never touch the session or world. Requests not
run within 30 s are refused. Texts are values, never commands: a line break in a reason becomes
a space, markup stays text.

### The audit log

`admin-audit.log` beside `server.cfg` records every login, refused password, operation and
result.

* One JSON object per line: time, actor (`player 3 'Name'`, `local tool (POST /admin)`,
  `web#2 (address)`), role, action, player, result.
* Past 1 MB it moves to `admin-audit.log.1`; the previous one becomes `admin-audit.log.2`
  (about 3 MB total).
* Web logins and refused passwords keep **whole source addresses**, regardless of
  `log_addresses` (which only affects the general game/server log).
* No password or token is written. Names, reasons and chat lines are cleaned text.

## Machine bans

Names and addresses do not identify anyone, so lasting bans target the **machine**.

**The identifier.** On join, the game sends a *hardware identifier* specific to this host's
owner. The host answers the first hello with its *realm* (a random key in the owner's list). The
game sends SHA-256 over a fixed text, the machine id and the realm, cut to 128 bits. The machine
id is Windows `MachineGuid`, Linux `/etc/machine-id` or macOS `IOPlatformUUID`; without one, a
random number kept in `~/.openomsi/hardware-id`. It never leaves the machine and cannot be
recovered from the hash. No disk or board serial is read.

* Sessions sharing an owner list see the same identifier per machine; another owner sees
  another.
* The host pins it to the connection at the first hello; a later hello cannot change it.
* It is shown to nobody: panels list bans by name, reason and time only.
* A banned machine gets `you are banned from this session: <reason>`.

**Who bans.** The host of a game started by code, and a server's admins and moderators, via the
game's *Player list* -> a player -> *Ban this machine...* (reason; a day, a week or for good),
the web panel or `POST /api/admin/ops`. Moderators ban for the session and cannot ban an admin;
the owner's list and unbanning are admin-only. The player is sent away at once and the ban is
audited. A host cannot ban its own machine.

**Scope:**

* *This session*: `bans.json` beside `server.cfg` for a dedicated server (survives restarts);
  for a game hosting by code, only while it runs.
* *All my sessions*: the owner's list, `~/.openomsi/bans-owner.json` of the host's account. Every
  game and server from that account on that machine reads it; running ones pick up changes
  within a second. To share across servers or machines, point `owner_bans` at a synced file.
* A ban may expire (`hours`), else it holds until lifted. Unbanning (*Player list* -> *Banned
  machines*, web panel, or `unban`) removes it from its list, for the other sessions too.

**Files:** JSON, written whole to a temp file then moved over the old one. An unreadable file
gives no bans (logged, `.bak` copy kept) and is never silently emptied.

**Limits:**

* The identifier is the game's claim, not proof. A modified game can send anything. A
  reinstalled system, new machine id, virtual machine or other computer is a new machine.
* Older games, and players who opt out (launcher: *Send servers an identifier for bans*, or
  `OMSI_NO_HARDWARE_ID=1`), send none and cannot be machine-banned. `require_hardware_id = 1`
  turns them away; `ban` still works for the run.
* Hosts with different owner lists cannot compare joiners unless they deliberately share the
  list (`owner_bans`).
* Bans from before LAN protocol 16 name identifiers no game sends now (logged at start). Ban
  those players again.

**Wire:** `HELLO` ends with `|<hardware id>`: empty until the host's `REALM|<realm>` arrives,
`-` when the player sends none. A host with a ban list answers an empty one with `REALM` and
admits the player only after the next hello.

## Public server list

`public_server = 1` lists the server in the launcher's *Multiplayer -> Servers -> Public
servers*.

* It registers with `public_list_url` (default: openOMSI's own, a Cloudflare Worker in
  `services/serverlist`).
* It is listed once the service reads its `GET /status` and finds the challenge it issued. The
  web port or tunnel must be reachable from the internet; a tunnel is the sure way.
* It pings every minute and is dropped three minutes after the last ping.
* Listed: name, message of the day, map, players, maximum, version, join address. Nothing about
  players.

**Moderation:**

* `name`, `motd` and `server-icon.png` are checked before the first ping. A flagged text
  (profanity and filter evasion, addresses, hidden characters) blocks listing, and the log names
  the key to change. A flagged icon blocks it too.
* Each launcher rechecks every listing: flagged text is covered, and a custom icon shows only if
  the viewer's own image check cleared it.
* The service filters too (no addresses or invites, per-source limits), but no client trusts it.
* Text is checked by [`rustrict`](https://crates.io/crates/rustrict) (MIT, also the chat filter)
  and our rules, behind a small trait (`omsi-serverlist`, `Moderator`).
* **Icons are not cleared at all yet.** No image classifier is bundled, so every custom icon is
  "unchecked" and the list shows the default. An open-licence classifier (a small Apache-2.0 or
  MIT model run by `tract`, downloaded to the content folder with a checksum, not committed)
  plugs in with `set_image_classifier`, nothing else changing.

**Why not OpenLB** (ImgCleaner / TxtCleaner), the library first planned:

* Its licence is MIT plus one line: the software "may not be used for anything but its original
  purpose". This project's AGPL does not allow that use limit in a part of it; crates.io files it
  as "non-standard".
* Its models (99 MB text, 10 to 47 MB image) have no licence, and training data that "cannot be
  released".
* It needs a native ONNX runtime (`ort`, pinned to a release candidate, loaded dynamically) for
  every platform, which the dedicated server, Android and iOS builds would each carry.

## What the server keeps about its players

Tell your players what is kept (the motd is a good place). The end of `server.cfg` lists it too.

| What | Where | How long |
|---|---|---|
| Joins and leaves, names, **every chat message**, admin actions, errors | `server.log` beside `server.cfg` (`run-server` and `start.sh`; Docker: `docker compose logs`, see [docker/server/README.md](../docker/server/README.md)) | New file at every start and past `log_max_mb` (20 MB; 0: no limit). `log_keep` (5) older ones as `server.1.log` (newest) … `server.5.log`; `log_keep = 0` keeps none. Docker: five files of 20 MB (`docker-compose.yml`) |
| Players' internet addresses | same log | Public addresses keep only the network (`203.0.113.x`, `2001:db8:85a3::x`); LAN and local addresses are written whole. `log_addresses = 1` writes all whole (for abuse reports) |
| Logins, refused passwords, admin and moderator operations; **whole source addresses** for web logins and refused passwords, regardless of `log_addresses` | `admin-audit.log` | Rotated past 1 MB, keeping `admin-audit.log.1` and `.2`: about 3 MB total |
| Banned machines (hardware identifier) | `bans.json`, `owner_bans` | Until lifted |
| Players' buses (`accept_uploads = 1`) | `~/.openomsi/lan-store` | `uploads_keep_days` (30) after last use |
| Name, icon, map, players, motd (no player names) | public list, with `public_server = 1` | Dropped three minutes after the server stops |
| Names, buses, lines, positions | anyone asking `GET /players`, with `share_positions = 1` | Live only |

Voice is relayed, never recorded or decoded. The address cut lives in the game's logger, so a
game hosting from the launcher (`game.log`, rewritten each start) cuts addresses the same way.
Before a crash report becomes a GitHub issue, the launcher removes the account folder and name
(`~`, `<user>`) and the driver name (`Drivers/<driver>.odr`).

## License

openOMSI is released under the GNU Affero General Public License, version 3 only (`LICENSE`).
Its launcher preview encodes pictures with [jpeg-encoder](https://crates.io/crates/jpeg-encoder):
this software is based in part on the work of the Independent JPEG Group.
