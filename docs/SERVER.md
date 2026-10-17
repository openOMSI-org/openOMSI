# Dedicated server

## How it runs today

```
openomsi --root /path/to/OMSI2 --server /opt/openomsi-server/server.cfg
```

(`scripts/build-server.sh` builds it on a Linux machine into `dist/server`, and `start.sh`
there starts it; every release also has ready `openOMSI-<version>-server-linux-x64.zip` and
`-server-linux-arm64.zip`, and for Windows (10/11 and Windows Server 2016 or later)
`-server-windows-x64.zip` and `-server-windows-arm64.zip`, started with
`start.cmd C:\path\to\OMSI2`.) The server is the game binary hosting a session with no window, no sound and no
graphics card: the renderer runs on wgpu's no-op device, so the world, the AI traffic, the
timetable buses and the people are simulated exactly as a hosting player's game does, and
nothing is drawn. `server.cfg` is written with commented defaults on the first start (name,
motd, map, date, time, weather, traffic, timetable, passengers, port, web_port,
max_players, tunnel, radius, the voice chat); `server-icon.png` beside it (64x64, like Minecraft's) is the
icon the players' list shows.

**The start script that asks** (`run-server.cmd` on Windows, a double click; `run-server.sh`
on Linux, both beside `openomsi` in the server's zip): the first time it asks for the OMSI 2
folder (and checks it is a complete installation before going on), then for the main
settings of `server.cfg` - name, players at most, the UDP and web ports, the tunnel, the admin
password (typed hidden; 12 characters at least) - with what the file has in brackets, kept
with Enter. Every time, it lists the installed maps by number (the last one marked, taken
with Enter) and starts the server on the one picked, without changing `map` in `server.cfg`;
the output goes to the window and to `server.log`, and when the server stops on an error it
says what the exit code means. Asked once per map, it makes `start-<map>.cmd` (or `.sh`)
beside it: that starts the same map again with no question. `run-server.cmd -Setup`
(`run-server.sh --setup`) asks for the folder and the settings again; `-Map maps/<folder>/global.cfg`
(`--map …`) starts a map with no question. What it remembers is in `run-server.conf` (the
folder, the last map, the maps of the `start-` scripts). It speaks Portuguese on a system in
Portuguese, English otherwise. It needs no rights of an administrator; on Windows it runs in
the Windows PowerShell every Windows has. `start.cmd` / `start.sh` stay as they were, for a
service or another launcher.

For a launcher of its own, the program has the two pieces the script uses:
`openomsi --root <folder> --list-maps` prints the installed maps (one a line, as `server.cfg`
names them; the content folder's too) and ends - with code 1 and what is missing when the
folder is not a complete OMSI 2, never another installation found instead; with
`--server <server.cfg>` it also writes a missing `server.cfg` with its defaults, nothing more
of a server's start. `--server-map maps/<folder>/global.cfg` starts the server on that map
instead of `server.cfg`'s.

**When it does not start**, it says why on its output and ends with a code a launcher can
tell apart:

| exit code | cause |
|---|---|
| 1 | the original OMSI 2 was not found at `--root` (the files missing are listed: a wrong folder, or an incomplete installation), or another error, printed as `Error: …` |
| 2 | `server.cfg` cannot be read or written, or its `map` is not installed (the maps there are listed, as `server.cfg` names them) |
| 3 | its UDP `port` is taken (another server, or another program): stop it, or choose another port |

A `web_port` that is taken is no reason to stop: the gateway takes another port and the log
says which (the status page, the WebSocket players and the tunnel are there). The log goes to
the standard error output: a launcher keeps it with the rest (`start.sh` writes it to
`server.log` too; `start.cmd` shows it in its window), and does not take a line there for a
failure - informational lines go there as well; the exit code says whether it failed. Paths
with spaces or accents are passed as they are, quoted. A map is named as in the game, relative
to the OMSI 2 folder (`maps/<folder>/global.cfg`), and found in the content folder (mods) too.

Players reach it two ways:

* UDP at `host:port` (a server with a public address, or on the LAN);
* over a **WebSocket** at its web port (`omsi-net::ws`): every datagram is one binary
  message, the server's gateway gives each WebSocket a UDP socket of its own on 127.0.0.1.
  The session sees such a player there (`joined from 127.0.0.1:<port>`); the log says who
  that is (`gateway: player WebSocket from <address> on 127.0.0.1:<port>`), with the
  address a reverse proxy or tunnel on the machine forwards: the last `X-Forwarded-For`,
  else `CF-Connecting-IP`. With Cloudflare's proxy (the orange cloud) in front of a proxy of
  this machine, the last `X-Forwarded-For` is a Cloudflare edge, and the players would share a
  few addresses (16 connections each at most): `gateway_proxy_header = cf-connecting-ip`
  then believes Cloudflare's `CF-Connecting-IP` first - only with Cloudflare in front, which
  writes it over a client's own.
  That is what a free Cloudflare quick tunnel carries (`tunnel = 1` starts `cloudflared
  tunnel --url http://127.0.0.1:<web_port>` and prints the `https://….trycloudflare.com`
  address). The same port answers `GET /status` (JSON: name, motd, map, players,
  max_players, time, weather, version, protocol, vehicles, and on a dedicated server `world`:
  its AI cars, buses, cars asleep, parked cars, people walking, waiting and aboard, the
  traffic density, whether it has passengers and the passenger density it applies now) and `GET /icon.png`, which the launcher's Multiplayer → Servers list
  shows. `vehicles` is the server's `vehicles` list (else every bus it has): a joining
  player's launcher offers only those, and so do the game menu's *Place a vehicle* and *Swap*
  once the game has joined. With an `admin_password` it also takes `POST /admin`
  from the machine itself: one admin command a line (`clock 30600`, `weather set
  Weather/#CAVOK.owt`, `say …`, `kick 3`, as the Administration menu sends them), the
  password in `X-Admin-Password`; five wrong ones in two minutes close it for a while. A
  tool beside the server (a dispatch page, a script) administers it that way without
  joining. A reverse proxy on the same machine connects from 127.0.0.1 as well: do not let
  it pass `/admin` on. With `share_positions = 1` it also answers
  `GET /players`: a JSON array of the players (`id`, `name`, `bus`, `line`, `destination`,
  `tour`, `x`/`y` in world metres east/north, `heading` in degrees clockwise from north,
  `speed_kmh`, `on_foot` and `aboard` - a player on foot is where it walks (even with its
  own bus parked nearby), one sitting in another player's bus is with that bus (its id) - and `lat`/`lon` on a `[worldcoordinates]`
  map such as Berlin-Spandau, `null` elsewhere), refreshed every second - what a live map of
  the server on a website needs.
  It is off by default: the players' names and positions are then nobody's business.

**Passengers.** `passengers = 1/0` switches the people at the stops and on the pavements on
or off for the whole run (a number there is not a count: `passengers = 50` reads as off).
How many there are is `passenger_density`, a factor on the map's own demand: the hourly
`[trafficdensity_passenger]` of its `global.cfg` and each stop's own numbers, so the map's
busy and quiet hours and stops stay as they are. `1` is what the map says, `0.5` half, `2`
twice, from `0` to `3`; it is the same factor as the game's own passengers setting
(`pax_density` in `settings.cfg`), which a server without the key goes on using as before.
It is not a cap on how many people there are (that is `ai_max_humans` of `settings.cfg`), and
it changes neither `traffic` nor the timetable's buses. `0` brings no new people while the
passengers stay on; whoever already waits, walks or rides stays. A value that is not a number
from 0 to 3 (negative, `NaN`, `10`) is refused with the reason in the log, and the server
runs as if the key were not there. An admin changes it while the server runs with
`passengers <factor>` (the Administration menu's command, also through `POST /admin`): the
stops get that many people from then on - nobody already there is taken away, the boarded
passengers' trips go on, and the players' games only mirror the server's people, so nobody
appears twice. `GET /status` shows the density applied and the people waiting, walking and
aboard. On a server with `passengers = 0` the command is refused (the log says so).

With `voice_channel` set, the players **talk** to each other through
[GreenTeaSpeak](https://greenteaspeak.de) as SaltyChat lets FiveM players do: a player is
heard from where they stand or sit, up to `voice_range` metres (20 by default), and through
the bodywork - quieter - when one of the two is in a bus and the other is not. Each player
runs GreenTeaSpeak 2 with the openOMSI plugin (`tools/greenteaspeak-plugin`, its README says
how to install it) and is connected to the voice server whose unique id is
`voice_server_uid` (the id in GreenTeaSpeak's server info panel; it is needed: without it
there is no voice chat, so that a server cannot move its players about on whatever voice
server they happen to be on). A
joining game asks the server for these settings (the command `voice?`), the plugin moves
the player into the channel `voice_channel` (its id or its name, with
`voice_channel_password`) and renames them `<name> #<player id>` - the name every other
game of the session gives them there - and the game tells it ten times a second where the
camera is and where everybody else is. A player who is speaking has "speaking" under their
name tag. With `voice_radio` on (the default), a driver is put on a map-wide **bus radio**
automatically: hold the bindable *Multiplayer: bus radio* key (Controls; X by default) to
transmit to every other driver on the map while proximity voice still reaches people next
to the bus; their name tag shows "radio". A player hosting by code names the voice server in
`~/.openomsi/voice.cfg` with the same keys. Settings → General → *Voice chat through
GreenTeaSpeak* switches it off. `voice_channel_password` is sent to every player who joins
(their game needs it to enter the channel): it keeps strangers on the voice server out of
the channel, not the server's own players. The settings together must fit a chat command
(160 characters after encoding), or the server says so in its log and has no voice chat.

The same gateway and tunnel open for a player hosting by **code** (Connect by Code): its
address goes to the rendezvous topic, and a joining game that gets no answer from the
addresses in the code (routers that cannot be punched through) joins through the tunnel.

## Players' buses (`accept_uploads`)

With `accept_uploads = 1` (the default) a player may drive a bus the server does not have:
the player's game sends the server the files that bus uses, the server keeps them and passes
them on to every other player, so nobody sees a stand-in.

* **What goes:** only what the player's game actually read for that bus - the variant and the
  paint driven, the shared folders its files point into (`..\..\Anzeigen`, shared sounds) and
  the fonts its displays write with - not the whole pack. Stock OMSI 2 content and files that
  came from the server never go. Programs, libraries and plugins are refused by name and by
  content, as for the server's own mods.
* **When:** a joining player sends it under the loading screen ("Getting your bus to the
  server", with the progress), so the others see the real bus from its first metre; after
  three minutes the loading screen lets go and the rest goes on in the background, shown as a
  card over the HUD. A bus or paint changed in the game is sent the same way.
* **The others:** every player's game says which bus it drives by its fingerprint (the
  hash over the list of its files). A game that draws that player uses its own copy when it
  is the same bus file for file, and else asks the server for that bus by its fingerprint
  ("Downloading another player's bus: 40%") - never another version that has the same path
  here. Joining downloads the map and the server's own mods only; the buses of the players
  who are there come this way, the moment their players are drawn (a bus still on its way to
  the server is waited for: the server answers the moment it has come). At most three buses
  download at once.
* **How it goes:** both ways in pieces of 1 MiB (a small file is one piece), each compressed
  when that makes it smaller (uncompressed `.tga`/`.bmp` textures go at a few percent of their
  size; `.dds`/`.png` hardly shrink), over four connections at once. Every piece is kept
  sealed the moment it comes, so a transfer that breaks off goes on where it stopped, whatever
  the size of the file, and loses at most the pieces on their way. A file counts only once
  all its pieces are here and its SHA-256 is the one its list names, and it has passed the
  checks of what it is; a bus is drawn only once every file of its list is here. Anything
  else - a piece changed on the way, a file that is not what its list says - is thrown away
  and fetched again, never drawn. A game or server of a version before the pieces sends and
  takes whole files, as before.
* **On the server:** taken without asking and with no size limit, and kept by its content:
  every file sealed once in `~/.openomsi/lan-store` by its SHA-256 (a file several buses share
  is kept and sent once), every bus as its list of files in `lan-store/vehicles/<fingerprint>.json`
  - the fingerprint the server works out itself from what it checked. Two players with two
  versions of the same mod are two buses, both kept, neither in the place of the other; the
  server draws each player's bus in the version that player has. Still there after a restart.
  A bus nobody drives is kept `uploads_keep_days` (30 by default) after it was last used; when
  the disk runs short for a file being sent, the buses nobody drives now go first, the least
  lately used first, until it fits - no bus is ever refused for its size, only when even then
  the disk cannot hold it. A bus being sent, and every file of it, is never let go meanwhile.
  To forget a bus by hand, stop the server and delete its `.json`.
* With `vehicles` empty the server then offers any bus (the launcher no longer limits the
  choice to the server's own buses); with a `vehicles` list only those buses are taken.
* `accept_uploads = 0` switches it off. A player's game hosting a session (`--lan-host`) takes
  the joining players' buses the same way, and keeps its own bus in its store for them.

## Administration

A server administers itself with passwords of `server.cfg`, at least 12 characters each (a
shorter one lets nobody in, and the log says so - a server set up before with 8 to 11
characters needs a longer one now). The game's login goes over the session unencrypted: its
challenge, salt and rounds can be overheard, and a short password could then be guessed
offline, hence the length.

| key | role | may |
|---|---|---|
| `admin_password` | admin | everything below, and the world: the clock and its speed, the weather, the AI traffic, the passengers; bringing players to their bus (`bring`, `bringall`) |
| `moderator_password` | moderator | see the server and its players, send a player away (not an admin), the chat, a word or a notice to one player or to all (`tell`, `notify`), duties, services, putting a player's bus back on its wheels (`unstick`), going to a player (`goto`, which moves the moderator's own bus) |

The role comes from the password that was given and is decided by the server alone: nothing a
game or a page sends names or raises it, and every operation is checked on the server, whether
a menu showed it or not. A `moderator_password` equal to the `admin_password` gives the admin's
role. No password: no administration.

**In the game.** A player says `/admin <password>` in the chat; the password never goes over the
network (the game asks `auth?`, the server answers with a fresh challenge, its salt and rounds,
and the game proves it knows the password: PBKDF2 and an HMAC of the challenge). Five wrong
answers in two minutes from one source (the player's address - the one behind the web gateway
for a player who came by WebSocket - or their number on this machine) lock it for that source
for a while; thirty from all sources together lock it for everybody. The web panel counts its
own wrong passwords apart (see below): a lock of the one is no lock of the other. The server answers `admin-ok admin`
or `admin-ok moderator` (a game before the roles reads the first word), and the Esc menu's
*Administration* sends every line to the server as `admin <action>`. The rights end when the
player leaves or talks from another address.

A game from before the roles, given the moderator's password, shows the admin's whole old menu:
the server still refuses the world's actions, but that game does not read the answer
(`admin-result`), so nothing happens on its screen. Moderators should play a current game.

**The Administration menu, from afar.** A player who administers a dedicated server gets
the server's own menu under Esc → *Administration*: who they are (admin or moderator), the
clock and the weather; *Players...* - every player the server lists, by name and number, with
what they do and drive, searchable with the menu's search line; a player's page and, before
sending them away (now, or for the rest of the run), a question naming them, their number and
their connection; the clock, its speed, the next weather, the traffic and the passengers'
density with their present values - or why they cannot be changed (a moderator's role, the real
time, METAR, no passengers); and what became of the last operations. The menu asks the server
over the session itself - the server may be anywhere; it never looks for it at an address of
its own - every 3 seconds at most while it is open, one question at a time, and says so when
no answer comes (a server older than the game: *Full list of commands (older menu)...* still
has every line of the older menu). The line picked stays on what it does when new answers
come; a player picked who left or came back is not acted on. *Stop administering* ends the
rights. It is a list of the game menu like the others, worked the same way (the search line
needs a keyboard; the list works without it). The host of a game hosted by code keeps the menu it had.

**One way of doing it.** Whatever way an operation comes - a player's menu, the machine's own
tool (`POST /admin`, see above: the admin's role), the web panel - it is checked against the
role, done by the server's own loop on its tick (never by a web request's thread), and
recorded with what became of it: `applied` (done: a player sent away is gone from the session,
the clock reads the new time), `pending` (the server did its part, the rest is a player's game:
a word or a duty sent to it), `rejected` with the reason (no such player, not the player picked
any more, not allowed for the role, the clock follows the real time, the weather follows the
METAR report, a weather that is not installed, a number out of range), or `queued` while the
loop has not come to it. A player's game that asked is told `admin-result <record> <status>
<message>`. A request taken is not a request done.

**Picking a player.** A player is named by number *and* connection: `kick 3#17 reason` (the
connection is the session's own number for that join, which the admins' list gives). A player
who left, or left and came back - the same number, a new connection - or somebody else who has
the number now is not the one picked: the operation is refused and nobody else is touched. The
older menus' `kick 3` still works and names the number alone. The players' list for an admin
(`admin-list [search]` → an `admin-p <id> <conn> <state> <role> <aboard> <online s> <vehicle>
<name>` line each, then `admin-end <found> <total>`) needs no `share_positions`: it gives no
position and no address. A search finds every name that holds the text, whatever the case
(`pichau` finds `Pichau` and `pichau_fan`, the exact name first), as text - no pattern, no
command - and never picks one by itself: the admin picks. `admin-state` answers `admin-s` with
the clock, its speed, the weather, the traffic, the passengers' density and counts, the players
and the locks (`rt=1`: real time, `metar=`: the METAR station).

**Sending away.** A player sent away (`kick`) reads the reason given, or "sent away by an admin",
and may join again. `ban` keeps them out for the rest of this server run, by their game's random
nonce and the address they came from. That is not an identity: a player name is whatever the
game says, an address changes (and many players can share one), and the session knows nothing
that outlives a run and could not be made up. So there is no permanent ban - it would be a ban
of a name or of an address, neither of which says who somebody is.

**Bringing players, going to them.** `bring`, `bringall` and `goto` stay in the game's menu for
an admin who drives on the server: the player's game puts its bus beside the admin's (with its
trailers; a player on foot is put back at the wheel first, so walking is not kept). What
becomes of other players sitting in a bus that is moved is not checked. They need the admin's
bus, and are not safe for every case, so the web panel and the new panels offer none of them.

**The web API.** The web gateway answers the administration under `/api/admin/` (and the
panel under `/admin/`), on the server's web port; nothing else is needed. All JSON, the token
of a login in `Authorization: Bearer <token>` - never in a cookie or in the address. A request
that changes something is `POST` with `Content-Type: application/json`.

| request | body / query | answer |
|---|---|---|
| `POST /api/admin/login` | `{"password": "…"}` | `200 {"token", "session"}`; `401` wrong password; `429 {"retry_after"}` too many lately; `404` no password set |
| `POST /api/admin/logout` | `{}` | `200`: the session ends |
| `GET /api/admin/session` | | `200 {"session"}` |
| `GET /api/admin/state` | | `200 {"session", "server", "age_ms"}` |
| `GET /api/admin/players` | `?q=<search>` | `200 {"players", "total", "age_ms"}` |
| `POST /api/admin/ops` | an operation, below | `202 {"op"}` queued; `400` not one (with the reason); `403` not for this role; `503` too many waiting |
| `GET /api/admin/ops/<id>` | | `200 {"op"}`; `404` not kept any more (the audit log has it) |
| `GET /api/admin/events` | `?audit_after=<seq>` | `200 {"ops": the last 50 records, "audit": the audit entries after seq}` |

`session` is `{"role": "admin"|"moderator", "permissions": ["view", "moderate", "world"], "actor":
"web#2 (address)", "expires_in": seconds}`. `server` is `{"name", "map", "version", "players",
"max_players", "time": "HH:MM", "clock": seconds of the day, "clock_speed", "real_time", "weather",
"metar": station or null, "traffic", "traffic_max", "timetable", "passengers",
"passenger_density", "passenger_density_max", "waiting", "walking", "aboard", "cars",
"buses"}`, as the server's loop published it about a second ago (`age_ms`; a server that has
stopped publishing shows as old). A player is `{"id", "conn", "name", "state": "joining" |
"driving" | "on_foot" | "aboard", "vehicle", "line", "tour", "aboard": the driver's id or null,
"online_secs", "admin": role or null}` - no position, no address. An operation's record is
`{"id", "actor", "op", "summary", "target": {"id", "conn"} or null, "target_name", "status":
"queued" | "applied" | "pending" | "rejected", "message", "at"}`.

The operations (`POST /api/admin/ops`), with what they need:

| body | role |
|---|---|
| `{"op": "kick", "target": {"id": 3, "conn": 17}, "reason": "…"}` (reason optional, 120 characters) | moderator |
| `{"op": "ban", "target": {…}, "reason": "…"}` - for this server run | moderator |
| `{"op": "say", "text": "…"}` / `{"op": "tell", "target": {…}, "text": "…"}` (160 characters) | moderator |
| `{"op": "clock", "seconds": 30600}` (0 .. 86400) / `{"op": "shift", "seconds": -3600}` / `{"op": "speed", "factor": 2}` (1 .. 30) | admin |
| `{"op": "weather", "next": true}` / `{"op": "weather", "file": "Weather/#CAVOK.owt"}` | admin |
| `{"op": "traffic", "density": 40}` (0 .. the game's top) / `{"op": "traffic", "clear": true}` | admin |
| `{"op": "passenger_density", "value": 1.5}` (0 .. 3) | admin |

A request is answered `queued` at once; the server's loop does it on its next step and the
record says what became of it (`GET /api/admin/ops/<id>`): the web's threads never touch the
session or the world. A request the loop has not come to in 30 s is refused. The texts are
values, never commands: a line break in a reason is a space, markup stays text (the panel
shows it as text).

**The web panel.** `http://127.0.0.1:<web_port>/admin/` (with the slash) on the server's
machine - or through your HTTPS proxy with `admin_web = remote` - is a page of the server
itself: no other service, nothing to install, the same API and the same checks as above. Log
in with the admin's or the moderator's password (it is sent to the server only and kept
nowhere; the session's token lives in that browser tab until it is closed or you log out). It
shows the server (map, players, clock and its speed, weather, traffic, the passengers' density
and how many wait, walk and ride, with "real time" and "METAR" when they hold the clock or the
weather), the players with a search (by name or `#number`), the actions on the player picked
- send away, for this run, a word - each confirmed with the name, number and connection; the
announcement; the world's controls with their present values (disabled with the reason for a
moderator, on the real time or under METAR, without passengers); and what became of each
operation (queued, applied, sent, rejected and why). It asks the server every 3 seconds while
the tab is shown (not while it is hidden), more slowly when the server does not answer, and
says so; a player who left or came back while picked is not acted on. English and Portuguese,
for a desktop and a phone. `crates/omsi-lan/examples/admin_demo.rs` runs a session with a few
players and the panel without OMSI 2, to try it (`crates/omsi-admin/panel/test/` drives it in
a browser).

**Who reaches the web panel and the API** (`server.cfg`):

* `admin_web = local` (what a new server's file says): from the server's own machine only, directly - a request
  that came through a tunnel or a reverse proxy (it carries `X-Forwarded-For`, `CF-Connecting-IP`
  …) is refused, the server's own Cloudflare tunnel (`tunnel = 1`) included. Starting the tunnel
  does not put the administration on the internet.
* `admin_web = remote`: also through the proxies listed in `admin_trusted_proxies` (addresses
  separated by `;`; this machine by default), and only for requests they say came over HTTPS
  (`X-Forwarded-Proto: https`, or Cloudflare's `CF-Visitor` with `admin_proxy_header =
  cf-connecting-ip` only - behind another proxy it would be the client's own). The address they say a request came
  from is believed of them alone, and only in the header they write: `admin_proxy_header`, the
  last address of `X-Forwarded-For` by default (Caddy, nginx, Cloudflare), or `x-real-ip`, or
  `cf-connecting-ip` - the other headers may be the client's own, passed on. That address counts
  its wrong passwords and names it in the records; a request the proxy names nobody for, or
  names this machine for, is refused. Which header to choose:
  * Caddy: the default (it adds the client to `X-Forwarded-For`).
  * nginx: either `proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;` with the
    default, or `proxy_set_header X-Real-IP $remote_addr;` with `admin_proxy_header =
    x-real-ip` - nginx passes the client's own `X-Forwarded-For` on unchanged otherwise.
  * Cloudflare's proxy (the orange cloud) in front of your own proxy: `cf-connecting-ip` - the
    last `X-Forwarded-For` address is then Cloudflare's, and every client would share a few
    addresses for the locks. A plain HTTP request from another machine is refused. Mind
  that with `tunnel = 1` the local cloudflared is such a proxy: the panel is then on the public
  tunnel address, guarded by the passwords alone. A proxy of your own (Caddy, nginx with a
  certificate) in front of `127.0.0.1:<web_port>` is the safer way; it must not pass `/admin`
  (the machine's own door) on.
* On this machine the panel answers to its own names only (`127.0.0.1`, `localhost`, `[::1]`): a
  page of another site whose name was turned to this machine (DNS rebinding) is refused.
* A reverse proxy on this machine must say it is one: it has to add `X-Forwarded-For` (or the
  header of `admin_proxy_header`) to what it passes on. One that adds none hands every request
  from the internet over as this machine's own - the server cannot tell them apart. nginx's
  plain `proxy_pass http://127.0.0.1:<web_port>;` is such a one (it sends `Host: 127.0.0.1:…`
  and no address): add `proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;`.
* Mind that a tunnel working below HTTP (`ssh -R`, frp, playit, `ngrok tcp`) hands every client
  over as this machine, with no header to tell: with one in front of the web port, anybody who
  reaches it meets the login as if they were here (the passwords and their locks still hold).
  Point such a tunnel at the game's port only, or keep `admin_web = off`.
* `admin_web = off`: none (the machine's own `POST /admin` stays as it is). A `server.cfg`
  without the key - one from before the web panel - or with a wrong word is off: an update
  opens no panel the operator did not ask for. Add `admin_web = local` to have it.
* `admin_web_origins`: other `https://` sites whose pages may send requests (a dispatch page
  elsewhere); a request whose browser names another origin is refused. No answer allows
  another site's page to read it (no `Access-Control-Allow-Origin`).

A session ends after 30 minutes unused, after 12 hours, at logout, or when the server stops;
at most 16 at once (a moderator's login makes room among the moderators' sessions only, never an
admin's). Five wrong passwords in two minutes lock a source for a while, thirty from anywhere
lock the door for everybody but this machine itself (a flood from afar cannot keep the operator
out of their own panel). A wrong password is noted in the audit log, a lock once - not each
knock on it. The server's `POST /admin` for a tool on its machine stays
as it was (one command a line, `X-Admin-Password`, the loopback only, `202` when taken): its
commands go the same way as the others and are in the audit log, with what became of them.

**The audit log.** Every login, refused password, operation and what became of it is written to
`admin-audit.log` beside `server.cfg` (a JSON object a line: when, who - `player 3 'Name'`,
`local tool (POST /admin)`, `web#2 (address)` -, the role, the action, the player, the result),
put aside as `admin-audit.log.1` past 1 MB, so it never takes more than about 2 MB. No password
or token is ever written to it; names, reasons and chat lines are kept as cleaned text.

## What is still to come

A world without the renderer's data structures (the server keeps the tiles' meshes in memory
for nothing), and tiles streamed by the players' interest (today `radius = 0` loads the whole
map). The section below is the plan the server was first written from: what it says about
`omsi-server`, `password`, `admins` and `public` is not what runs today - the server is
`openomsi --server`, and its keys are those of the `server.cfg` it writes.

# Dedicated server - the original concept (a plan, not the current server)

Today a session is hosted by a player's game: the host simulates the shared world (AI
traffic, timetable buses, people, traffic lights) around every player and streams it to the
clients (`omsi-net::world`, `omsi-app::lan_world`). A dedicated server is the same host role
without a player, a window or a sound card, running on a machine that is always on - like a
Minecraft server. This document fixes how it is to be built, so that the pieces written now
fit it.

## What already fits

* **The protocol.** Clients do not care whether the host is a game or a server: `HELLO` /
  `WELCOME` / states / world frames / claims are the same (`omsi-net`, `PROTOCOL`).
* **Authority.** The host already owns the shared world; a client simulates only its own
  bus and the people boarding it, and claims waiting passengers from the host
  (`lan_world`). A server keeps exactly that split.
* **Finding it.** A session code carries the session id; the rendezvous (`omsi-net::bridge`:
  STUN, UPnP, the relay topic named after the id) finds the host behind a home router. A
  server with a public address is simply reachable at `address:port`; it posts to the relay
  as a host does, so a code works for it too.
* **Areas of interest.** Traffic and people are already populated around *every* player
  (`Traffic::lan_centers`, `Humans::lan_centers`) and sent per client only near its bus.

## What has to move

1. **A world without a renderer.** `scene::World` loads tiles for drawing and for the
   simulation at once, and `Traffic`/`Humans` create render instances as they spawn. The
   simulation part goes into its own crate (`omsi-world`: tiles' lanes, stops, objects'
   collision and light programs, the timetable, `Traffic`, `Humans`) behind a small trait
   the game implements to show things:

   ```rust
   pub trait Presenter {
       fn vehicle_added(&mut self, id: u32, ty: &VehicleType, scheme: Option<usize>);
       fn vehicle_removed(&mut self, id: u32);
       fn person_added(&mut self, id: u32, hum: &Path);
       fn person_removed(&mut self, id: u32);
       // poses are read by the presenter from the world after each tick
   }
   ```

   The game's presenter makes render instances (today's code in `Traffic::sync`,
   `Humans::tick`); the server's does nothing. Vehicle scripts run without their meshes
   (`VehicleInstance` already works headless: the `bus_audit` example does it).
2. **Tiles by interest, not by camera.** The tile streamer follows the camera; the server
   streams the tiles around every player's bus (the union of their radii) and lets go of
   the rest.
3. **A player without a bus of its own on the server.** The server is player 0 with no pose;
   `LanSession` already tolerates a host whose pose has no vehicle (`Pose::has_vehicle`).

## The server program

`omsi-server` (a binary of its own, no winit/wgpu/cpal):

```
omsi-server --root "/path/to/OMSI 2" --config server.cfg
```

`server.cfg` (key = value, like `settings.cfg`):

| key | meaning |
|---|---|
| `map` | `maps/Berlin-Spandau/global.cfg` |
| `date`, `time`, `time_scale` | the world clock (the server's clock is the session's) |
| `weather` | a `.owt`, or `auto` (a cycle) |
| `traffic`, `passengers`, `timetable` | density and switches, as the launcher's |
| `port` | UDP port (27015) |
| `max_players`, `password` | who may join (`REJECT` with a reason otherwise) |
| `name`, `motd` | shown in the launcher's list and on joining |
| `public` | post to the relay's lobby topic, so the launcher can list the server |
| `admins` | player names allowed to run `/kick`, `/time`, `/weather` in the chat |

It needs the original OMSI 2 files like the game (same `missing_original_essentials`
check) and the same mods as the players: the server lists its content in `WELCOME`
(vehicle and map files with sizes), and a client missing something is told what.

## Order of work

1. Extract `omsi-world` with the `Presenter` trait; the game keeps working through it.
2. Headless world tick in a test (`Traffic` and `Humans` on Grundorf for 10 minutes without a
   renderer, compared with the game's positions for the same seed).
3. `omsi-server` hosting a session from `server.cfg`; the launcher's Join accepts
   `host:port` and codes as today.
4. Lobby: servers post `name/map/players/code` to the relay's lobby topic; the launcher
   shows the list.
5. Admin commands and a small status page (players, tick time, bandwidth).
