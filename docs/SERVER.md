# Dedicated server

```
openomsi --root /path/to/OMSI2 --server /opt/openomsi-server/server.cfg
```

Every release has ready server packages: `openOMSI-<version>-server-linux-x64.zip` and
`-server-linux-arm64.zip` (started with `start.sh /path/to/OMSI2 [server.cfg]`), and for
Windows 10/11 and Windows Server 2016 or later `-server-windows-x64.zip` and
`-server-windows-arm64.zip` (started with `start.cmd C:\path\to\OMSI2 [server.cfg]`).
`scripts/build-server.sh [folder]` builds the same on a Linux machine into `dist/server`.
The OMSI 2 folder given is a complete original installation, as the players have it; the
server's mods go into its `Vehicles`, `maps`, `Sceneryobjects` … as in the game's content
folder.

The server is the game binary hosting a session with no window, no sound and no
graphics card: the renderer runs on wgpu's no-op device, so the world, the AI traffic, the
timetable buses and the people are simulated exactly as a hosting player's game does, and
nothing is drawn. `server.cfg` is written with a comment for every key on the first start:

| Keys | What they set |
|---|---|
| `name`, `motd` | shown in the players' server list and when they join |
| `map`, `date`, `time`, `weather` | the map, the start date (empty: today), the time of day, the weather (a `.owt`, empty: the map's default, `cycle`: one after another through the day) |
| `time_speed`, `real_time` | how fast the clock runs (1 to 30), or the machine's real date and time |
| `metar_sync`, `metar_station` | the weather follows an airport's METAR report (ICAO code; empty: the one nearest the map), downloaded every ten minutes and passed on to the players |
| `traffic`, `timetable`, `passengers` | AI cars, timetable buses, waiting passengers |
| `port`, `web_port`, `max_players` | UDP port of the session (27015), TCP port of the web gateway (27025), player limit (16) |
| `tunnel` | start a free Cloudflare quick tunnel (needs `cloudflared`) |
| `radius` | how many tiles round the map's first entry point are kept loaded (0: the whole map) |
| `admin_password` | players who say `/admin <password>` in the chat get the Administration menu (Esc); at least 8 characters, empty: nobody |
| `vehicles` | the buses players may drive, separated by `;` (empty: every bus on the server) |
| `share_positions` | `GET /players` on the web port (below) |
| `accept_uploads`, `uploads_keep_days` | players' buses (below) |
| `share_maps` | a player who joins without the server's map gets it from the server (the map and everything it uses, gigabytes for a big map); off (the written default), players install it themselves and are told which one is missing |
| `voice_*` | voice chat (below) |

`server-icon.png` beside `server.cfg` (64x64, a PNG under 256 KB) is the icon the players'
list shows.

Players reach it two ways:

* UDP at `host:port` (a server with a public address, or on the LAN);
* over a **WebSocket** at its web port (`omsi-net::ws`): every datagram is one binary
  message, the server's gateway gives each WebSocket a UDP socket of its own on 127.0.0.1.
  The session sees such a player there (`joined from 127.0.0.1:<port>`); the log says who
  that is (`gateway: player WebSocket from <address> on 127.0.0.1:<port>`), with the
  address a reverse proxy or tunnel on the machine forwards (`X-Forwarded-For`,
  `X-Real-IP`, `CF-Connecting-IP`).
  That is what a free Cloudflare quick tunnel carries (`tunnel = 1` starts `cloudflared
  tunnel --url http://127.0.0.1:<web_port>` and prints the `https://….trycloudflare.com`
  address). The same port answers `GET /status` (JSON: name, motd, map, players,
  max_players, time, weather, version, protocol, vehicles, and on a dedicated server `world`:
  its AI cars, buses, cars asleep, parked cars, people walking, waiting and aboard, the
  traffic density) and `GET /icon.png`, which the launcher's Multiplayer → Servers list
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
`~/.openomsi/voice.cfg` with the same keys. A player switches it off in the launcher under
Settings → General → *Voice chat through GreenTeaSpeak (multiplayer)*. `voice_channel_password` is sent to every player who joins
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
  here. Joining downloads only the server's own mods (and its map, with `share_maps`); the buses of the players
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
  and fetched again, never drawn.
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
* With `vehicles` empty the players may choose any bus they have, not only the server's own;
  with a `vehicles` list only those buses are taken.
* `accept_uploads = 0` switches it off. A player's game hosting a session (`--lan-host`) takes
  the joining players' buses the same way, and keeps its own bus in its store for them.

