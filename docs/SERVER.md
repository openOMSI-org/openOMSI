# Dedicated server

A dedicated server hosts an openOMSI multiplayer session that stays on, with no player at the machine.
This page is for server owners: setup, every `server.cfg` key, administration, bans and the public server list.
To join a server as a player, see [Playing together](MULTIPLAYER.md).

The server is the game itself (`openomsi`) started with `--server`. It has no window, no sound and needs no graphics card.
It simulates the world, the AI traffic, the timetable buses and the people exactly as a hosting player's game does, and draws nothing.

## Quick start

### 1. Download

Every release has ready server packages:

| Package | System | Start script |
|---|---|---|
| `openOMSI-<version>-server-linux-x64.zip` | Linux x86-64 | `start.sh`, `run-server.sh` |
| `openOMSI-<version>-server-linux-arm64.zip` | Linux ARM64 | `start.sh`, `run-server.sh` |
| `openOMSI-<version>-server-windows-x64.zip` | Windows 10/11, Server 2016 or later | `start.cmd`, `run-server.cmd` |
| `openOMSI-<version>-server-windows-arm64.zip` | Windows on ARM | `start.cmd`, `run-server.cmd` |

Unpack it into a folder of its own. You also need a **complete original OMSI 2 installation** on that machine, the same one the players have.
Put the server's mods into the OMSI 2 folder's `Vehicles`, `maps`, `Sceneryobjects` … as in the game's content folder.

To build the same package yourself on Linux: `scripts/build-server.sh [folder]` (into `dist/server` by default).

### 2. Run the guided start script

The easiest way is the script that asks:

- Windows: double-click `run-server.cmd`.
- Linux: run `./run-server.sh`.

1. The first time, it asks for the OMSI 2 folder and checks that it is a complete installation.
2. It asks for the main settings: name, maximum players, UDP port, web port, the tunnel and the admin password (typed hidden, at least 12 characters).
   The current value is shown in brackets. `Enter` keeps it.
3. Every time, it lists the installed maps by number. The last one is marked; `Enter` takes it.
4. It starts the server on that map. Output goes to the window and to `server.log`.
5. Once per map it offers to create `start-<map>.cmd` (or `.sh`), which starts that map again with no questions.

| Option | Does |
|---|---|
| `run-server.cmd -Setup` / `run-server.sh --setup` | Ask for the folder and settings again. |
| `run-server.cmd -Map maps/<folder>/global.cfg` / `run-server.sh --map …` | Start this map with no questions. |

The script remembers the folder, the last map and its shortcuts in `run-server.conf`.
It does not change `map` in `server.cfg`. It speaks Portuguese on a Portuguese system and English otherwise.
It needs no administrator rights. On Windows it runs in the built-in Windows PowerShell.
When the server stops on an error, the script says what the exit code means.

### 3. Or start it directly

```bash
./start.sh /path/to/OMSI2 [server.cfg]
```

```text
start.cmd C:\path\to\OMSI2 [server.cfg]
```

Both run:

```bash
openomsi --root /path/to/OMSI2 --server /path/to/server.cfg
```

A missing `server.cfg` is written with its defaults and a comment for every key on the first start.
`start.sh` writes the output to `server.log` too. `start.cmd` shows it in its window only.

### 4. Minimal configuration

Edit `server.cfg` beside the script. For a first server, these keys are enough:

```text
name = My openOMSI server
motd = Welcome! Drive safely.
map = maps/Grundorf/global.cfg
max_players = 16
admin_password = a-long-password-here
```

Restart the server after changing `server.cfg`.

### 5. Connect

The server prints its addresses when it starts. Players join in the launcher on **Servers** → **Use a server address**:

- `<server address>:27015` (UDP), or
- the `https://….trycloudflare.com` address the server printed (with `tunnel = 1`, the default).

On the same machine, use `127.0.0.1:27015`.

### 6. Open the firewall

| Port | Protocol | Used for |
|---|---|---|
| 27015 (`port`) | UDP | The session |
| 27015 (`port`) | TCP | The server's mods and the players' buses |
| 27025 (`web_port`) | TCP | Status, icon, WebSocket players, web panel |

On a server on the internet, open all three. With the tunnel, players can also come in through it with no ports open.

## Configuration reference

`server.cfg` is `key = value`, one per line. Lines starting with `#` are comments. Flags take `1`/`0` (also `true`, `yes`, `on`).
Two defaults are given when they differ: the value in a newly written file, and what a missing key means (for a `server.cfg` from an older version).

### Name and map

| Key | Default | What it sets |
|---|---|---|
| `name` | `openOMSI server` | Shown in the players' server list and when they join. |
| `motd` | `Welcome! Drive safely.` (missing: empty) | Message of the day. |
| `map` | `maps/Berlin-Spandau/global.cfg` | The map, relative to the OMSI 2 folder. Maps in the content folder are found too. |
| `date` | empty | Start date `YYYY-MM-DD`. Empty: today. |
| `time` | `08:00` | Start time of day. |
| `weather` | empty | A `.owt` file of the OMSI 2 folder. Empty: the map's default. `cycle`: one weather after another through the day, as the month allows. |
| `radius` | `0` | Tiles around the map's first entry point kept loaded. `0`: the whole map. |

`server-icon.png` beside `server.cfg` is the icon in the players' list: a PNG, 64x64, under 256 KB.

### Clock and weather

| Key | Default | What it sets |
|---|---|---|
| `time_speed` | `1` | How fast the clock runs, 1 to 30. |
| `real_time` | `0` | `1`: the clock follows the machine's real date and time. `time`, `date`, `time_speed` and the admins' clock are then ignored. |
| `metar_sync` | `0` | `1`: the weather follows an airport's real METAR report. The server downloads it every ten minutes and passes it on, so players need no METAR sync of their own. Admins cannot change the weather then. |
| `metar_station` | empty | The airport's ICAO code, for example `EDDB`. Empty: the one nearest the map. |

### Traffic and passengers

| Key | Default | What it sets |
|---|---|---|
| `traffic` | `30` | Random AI cars. Capped at the game's own maximum. |
| `timetable` | `1` | Timetable buses. |
| `passengers` | `1` | People at the stops and on the pavements, on or off for the whole run. A number is not a count: `passengers = 50` reads as off. |
| `passenger_density` | `1` (missing: the machine's `settings.cfg`) | How many people, as a factor on the map's own demand ([below](#passenger-density)). 0 to 3. |
| `ai_drive_through_parked` | `0` | `1`: the AI drives on through a parked car standing in its lane, as OMSI 2's does. `0` stops behind it and goes round when it can. Use `1` on maps where a parked car blocks a lane with no way round. |
| `speed_limit_kmh` | `0` | Optional roleplay speed limit: `0` (off), `60`, `75` or `80` ([below](#speed-limit)). |

#### AI limits

These override the machine's `settings.cfg` for this server. Empty: `settings.cfg` decides. They are for the whole server, not per player: with players all over the map, raise them (and the CPU) with the players.

| Key | Range | What it sets |
|---|---|---|
| `ai_unsched_factor` | 0 to 300 | Random traffic, percent of the map's density (100 = as the map says). |
| `ai_max_scheduled` | 0 to 4096 | Timetable vehicles on the road at once. `0`: no limit. |
| `ai_max_parked` | -1 to 100000 | Parked cars in the loaded tiles. `0`: every space. `-1`: none. |
| `ai_max_humans` | 1 to 4096 | People at once: waiting, walking and riding. |

A value out of range is ignored with a warning in the log.

#### Passenger density

`passenger_density` multiplies the map's own demand: the hourly `[trafficdensity_passenger]` of its `global.cfg` and each stop's numbers. The map's busy and quiet hours stay as they are.

- `1` is what the map says, `0.5` half, `2` twice.
- `0` brings no new people, but those already waiting, walking or riding stay. `passengers = 0` switches them off.
- It is not a cap on how many people there are (that is `ai_max_humans`). It does not change `traffic` or the timetable.
- It is the same factor as the game's own `pax_density` setting.
- A value that is not a number from 0 to 3 is refused with the reason in the log, and the server runs as if the key were missing.

Admins change it while the server runs with `passengers <factor>`. Nobody already there is removed.

#### Speed limit

`speed_limit_kmh` asks every player's game to hold their bus under 60, 75 or 80 km/h. Any other value logs a warning and turns the limit off.

- The game reduces the throttle progressively in the last 5 km/h. Above the limit, a small braking assist counters downhill speed.
- Steering, reverse and normal braking work as usual. The speed is never clamped abruptly.
- Games ask the server for the limit regularly, also after reconnecting. Older games ignore it.

> **Warning:** This is cooperative roleplay behaviour, not anti-cheat. A modified game can ignore it, and the server does not reject buses that go too fast.

### Network

| Key | Default | What it sets |
|---|---|---|
| `port` | `27015` | UDP port of the session. The same number on TCP carries the mods and players' buses. |
| `web_port` | `27025` | TCP port of the web gateway: status, icon, WebSocket players, web panel. If it is taken, the server picks another and logs which. |
| `max_players` | `16` | Player limit, 1 to 64. |
| `tunnel` | `1` | Start a free Cloudflare quick tunnel in front of the web port and print its `https://….trycloudflare.com` address. The game downloads `cloudflared` if needed. |
| `public_address` | empty | Where players reach the web port without the tunnel ([below](#public-address)). |
| `gateway_proxy_header` | `x-forwarded-for` | How a reverse proxy on this machine names a WebSocket player's address: `x-forwarded-for` or `cf-connecting-ip` ([below](#behind-a-reverse-proxy)). |

#### Public address

A quick tunnel drops its connections now and then. A game joining through the tunnel therefore first tries the server's direct web address.

- Empty: found automatically. This is the machine's own internet address when it has one. Otherwise it is the address the internet sees, if the server can reach itself there (not behind a carrier's NAT).
- `1.2.3.4:27025` or `play.example.org:27025`: your own.
- `off`: told to nobody.

The address is shown to whoever asks the server's status.

### Players and content

| Key | Default | What it sets |
|---|---|---|
| `vehicles` | empty | The buses players may drive, separated by `;` ([below](#allowed-buses)). Empty: every bus on the server (and with `accept_uploads = 1`, any bus a player has). |
| `accept_uploads` | `1` | Take the buses of players who drive one the server lacks, and pass them on ([Uploads](#uploads)). |
| `uploads_keep_days` | `30` | Days an uploaded bus nobody drives is kept, 1 to 3650. |
| `parked_buses` | `0` | How many uploaded buses stand parked in the world for any player to take ([below](#parked-buses)). 0 to 536. Needs `accept_uploads = 1`. |
| `share_maps` | `0` (missing: `1`) | A player who joins without the server's map gets it from the server, with everything it uses ([below](#sharing-the-map)). |
| `share_positions` | `0` | `GET /players` on the web port tells anyone the players' names, buses, lines and positions ([below](#web-endpoints)). |
| `allow_player_teleport` | `0` | `1`: every player may teleport their **own** bus (F4 map, navigator map, start points). `0`: only admins and moderators. It never moves another player. |

#### Allowed buses

`vehicles` takes bus files (`Vehicles/MAN_SD200/MAN_SD77.bus`) or bus folders (`Vehicles/MAN_SD200` or `MAN_SD200`, meaning every bus directly in that folder).
Slashes or backslashes and any case are fine.

- The players' launcher offers only these buses, and so do the game's *Place a vehicle* and *Swap* menus.
- A player who drives another bus is sent away and told the list.
- The server announces these buses with their paint schemes (the first 256). Players who lack one can download it after they confirm ([Playing together](MULTIPLAYER.md#buses-from-the-server)).
  A first start checks every file and can take minutes for a large list. The web port is open meanwhile.

#### Sharing the map

With `share_maps = 1`, a player who joins without the server's map downloads it and everything it uses: objects, splines, vehicles. For a big map that is gigabytes.
With `0`, players install the map themselves and are told which one is missing. The server's bandwidth then goes to the players' buses.

To put a map on the server with everything it uses, pack it into the server's content folder:

```bash
openomsi --root "/path/to/OMSI2" --pack-map "<map name>" --pack-to content
```

On the same disk, files go in as hard links and take no extra space. `--pack-to map.zip` makes a zip for another machine.
Files the map names that do not exist are listed in `openomsi-map-pack-<map>.txt` in the target folder.

#### Parked buses

With `parked_buses` above 0, buses that players uploaded stand in the world when nobody drives them, where their last driver left them (also after a restart).
Any player who walks up to one takes it with the key that takes a seat. At most this many stand, one of each bus.
A game is told only the ones near its player. Players on an older release see them as other players' parked buses and cannot take them.

### Administration keys

| Key | Default | What it sets |
|---|---|---|
| `admin_password` | empty | Password of the admin role. At least 12 characters. Empty: no admin. |
| `moderator_password` | empty | Password of the moderator role. At least 12 characters. Empty: no moderators. One equal to `admin_password` gives the admin role. |
| `admin_web` | `local` (missing: `off`) | Who reaches the web panel: `local`, `remote` or `off` ([Web panel](#web-panel)). |
| `admin_web_origins` | empty | Other `https://` sites whose pages may use the admin API, separated by `;`. |
| `admin_trusted_proxies` | empty (this machine) | Proxies whose word about a request's source is believed, separated by `;`. |
| `admin_proxy_header` | `x-forwarded-for` | The header those proxies name the client in: `x-forwarded-for` (last address), `x-real-ip` or `cf-connecting-ip`. |

A shorter password lets nobody in, and the log says so. With `admin_web = remote`, both passwords need 16 characters, or the panel stays `local`.

### Voice chat

| Key | Default | What it sets |
|---|---|---|
| `voice` | `1` (missing: `0`) | Voice chat at all. Players must also switch it on themselves. |
| `voice_radio` | `1` (missing: `0`) | The session-wide radio channel. `0` leaves only the nearby channel. |
| `voice_range` | `75` | How far the nearby channel carries, 5 to 1000 m. |

The server passes voice on without decoding it and needs no audio device. See [Voice chat](VOICE.md).
Old keys `voice_server_uid`, `voice_channel` and `voice_channel_password` are ignored, and the log says so.

### Public list and bans

| Key | Default | What it sets |
|---|---|---|
| `public_server` | `0` | `1`: list the server in the public server list ([Public server list](#public-server-list)). |
| `public_list_url` | empty | The list service's `https://` address. Empty: openOMSI's own. Not in the written file. |
| `owner_bans` | empty | The owner's ban list file, shared by all your sessions. Empty: `~/.openomsi/bans-owner.json`. |
| `require_hardware_id` | `0` | `1`: turn away players whose game sends no machine identifier. |

### Logs and privacy

| Key | Default | What it sets |
|---|---|---|
| `log_addresses` | `0` | `1`: write the players' internet addresses whole in the log (for an abuse report). `0`: a public address keeps only its network (`203.0.113.x`). |
| `log_keep` | `5` | Earlier `server.log` files kept (`server.1.log` is the newest). `0` keeps none. |
| `log_max_mb` | `20` | A new `server.log` is begun past this size. `0`: no limit. |

`log_keep` and `log_max_mb` are applied by `run-server` and `start.sh`. See [What the server keeps](#what-the-server-keeps).

## Uploads

With `accept_uploads = 1` a player may drive a bus the server does not have. Their game sends the files to the server, which keeps them and passes them on, so nobody sees a stand-in.

**What is sent.** Only what the player's game actually loaded for that bus: the variant and paint driven, the shared folders its files point into (`..\..\Anzeigen`, shared sounds) and the fonts of its displays.
Stock OMSI 2 content and files that came from the server are never sent. Programs, libraries and plugins are refused by name and by content.

**When.** A joining player sends it under the loading screen (*Getting your bus to the server*), so the others see the real bus from the start. After three minutes the loading screen lets go and the rest continues in the background.
A bus or paint changed in the game is sent the same way.

**How others get it.** Each game names the bus by its fingerprint, a hash over its list of files.
A game that already has the same bus file for file uses its own copy. Otherwise it downloads that exact version from the server, never another version at the same path. At most three buses download at once.

**Transfer.** Files go in pieces of 1 MiB, compressed when that helps, over four connections at once.
An interrupted transfer resumes where it stopped. A file counts only when its SHA-256 matches its list and it passes the content checks. A bus is drawn only when all its files are there.

**Storage on the server.** Every file is kept sealed once in `~/.openomsi/lan-store`, by its SHA-256. A file several buses share is kept once.
Every bus is a list of files in `lan-store/vehicles/<fingerprint>.json`. Two versions of the same mod are two buses, both kept.

- Uploads are taken without asking and have no size limit.
- A bus nobody drives is kept `uploads_keep_days` after its last use.
- When the disk runs short, the buses nobody drives now go first, least recently used first. No bus is refused for its size.
- To forget a bus by hand, stop the server and delete its `.json`.

`accept_uploads = 0` turns uploads off. A game hosting a session (not a dedicated server) takes its players' buses the same way.

## Administration

The server has two roles. The role comes from the password given, and the server alone decides it. Every operation is checked on the server.

| Role | May |
|---|---|
| Moderator | See the server and its players. Send players away and ban them for the session (not admins). Chat, private words and notices. Duties, services, `unstick`, `goto`. Mute a player's voice. |
| Admin | Everything a moderator may, plus the world: clock, its speed, weather, traffic, passengers. `bring` and `bringall`. Bans in all your sessions, and lifting bans. |

There are four ways to administer. All go through the same checks and the same audit log:

1. **In the game**: say `/admin <password>` in the chat, then use `Esc` → **Administration...** and **Player list...**.
2. **The web panel** at `http://127.0.0.1:<web_port>/admin/`.
3. **The web API** under `/api/admin/` (what the panel uses).
4. **`POST /admin`** from the server machine itself, for scripts.

### In the game

1. Join the server.
2. Type `/admin <password>` in the chat.
   The password never goes over the network: the server sends a challenge and the game proves it knows the password.
3. The server answers with your role. `Esc` → **Administration...** now shows the server's menu.

The **Administration** menu shows your role, the clock and the weather, and has:

- **Players...**: every player by name and number, with what they drive. Searchable. A player's page offers sending them away (now, or for the rest of this run), with a confirmation naming them.
- The clock, its speed, the next weather, the traffic and the passenger density, with their current values. Or why they cannot be changed: moderator role, real time, METAR, no passengers.
- The results of the last operations.
- **Stop administering** ends your rights.

The menu asks the server every 3 seconds at most while it is open. A server older than the game shows **Full list of commands (older menu)...**.
Rights end when you leave or when you talk from another address.

> **Note:** A game from before the roles, given the moderator password, shows the admin's old menu. The server still refuses the world's actions, but that game does not show the refusal. Moderators should use a current game.

### Admin commands

The menus send these lines. `POST /admin` takes the same lines.
`<player>` is a player number, or `number#connection` to make sure it is still the same join (the admins' list gives the connection). A player who left and came back has a new connection, and the operation is refused.

| Command | Role | Does |
|---|---|---|
| `kick <player> [reason]` | moderator | Send away. They can join again. |
| `ban <player> [reason]` | moderator | Send away and keep out for the rest of this server run (by the game's random nonce and address). |
| `hwban <player> session [12h\|3d] [reason]` | moderator | Ban the machine from this server. Without a duration: until lifted. |
| `hwban <player> all [12h\|3d] [reason]` | admin | Ban the machine from all your sessions. |
| `unban <ban id>` | admin | Lift a machine ban. |
| `say <text>` | moderator | Chat line to everybody. |
| `tell <player> <text>` | moderator | Private word to one player. |
| `notify <all\|player> <id> <seconds> <kind> <text>` | moderator | A notice card. `kind`: `info`, `warn`, `alert`, `action`, `success`. Seconds 2 to 30. |
| `duty <player> <line> <tour> <trip> <stop>` | moderator | Give a duty. A space in a name is `%20`. |
| `duty-off <player>` | moderator | Take a duty back. |
| `service <repair\|refuel\|wash> [all\|<player>]` | moderator | Service buses. |
| `unstick <player>` | moderator | Put a player's bus back on its wheels. |
| `goto <player>` | moderator | Move your own bus beside theirs (from the game only). |
| `vmute <player>` / `vunmute <player>` | moderator | Silence or allow a player's voice for the session. |
| `bring <player>` | admin | Put their bus beside yours (you must drive on the server). |
| `bringall` | admin | Bring everybody, one behind the other. |
| `clock <seconds>` | admin | Set the time of day (seconds since midnight: `30600` is 08:30). |
| `time <seconds>` | admin | Move the clock, at most a day either way. |
| `speed <factor>` | admin | Clock speed, 1 to 30. |
| `weather next` | admin | Next weather. |
| `weather set <file>` | admin | This weather, for example `Weather/#CAVOK.owt`. |
| `traffic <density>` / `traffic clear` | admin | AI traffic density, or clear all AI traffic (a jam). |
| `passengers <factor>` | admin | Passenger density, 0 to 3. Refused when `passengers = 0`. |

Two read-only queries serve the menus: `admin-list [search]` lists players (number, connection, state, role, vehicle, name; no positions or addresses) and `admin-state` gives the clock, speed, weather, traffic, passengers and locks.

A moderator cannot send away, ban or silence an admin. Nobody can ban themselves.
`bring`, `bringall` and `goto` need the admin's own bus on the server, so the web panel does not offer them. A player on foot who is brought is put back at the wheel first.

### Operation results

Every operation, whichever way it comes, is done by the server's own loop and recorded with its result:

| Status | Means |
|---|---|
| `queued` | Taken, not done yet. Refused if not done within 30 s. |
| `applied` | Done: the player is gone, the clock reads the new time. |
| `pending` | The server did its part; the rest is a player's game (a word or duty sent to it). |
| `rejected` | Not done, with the reason: no such player, a different connection, not allowed for the role, real time or METAR in control, weather not installed, number out of range. |

A game that asked gets `admin-result <record> <status> <message>`.
Texts are values, never commands: a line break in a reason becomes a space, and markup stays text.

### Web panel

`http://127.0.0.1:<web_port>/admin/` (with the final slash) on the server machine. It is part of the server: nothing to install.

1. Open the address in a browser on the server machine.
2. Log in with the admin or moderator password. It is sent to the server only and kept nowhere.
3. The session token lives in that browser tab until you close it or log out.

The panel shows the server (map, players, clock and speed, weather, traffic, passenger density, people waiting, walking and riding), the players with a search (by name or `#number`), actions on the player picked (send away, ban for this run, a word, a machine ban), announcements, the world controls, and the result of each operation.
Controls a role may not use are disabled with the reason.
It refreshes every 3 seconds while the tab is visible. It is in English and Portuguese, for desktop and phone.

A login session ends after 30 minutes unused, after 12 hours, at logout or when the server stops. At most 16 run at once; a moderator's login never pushes out an admin's.

#### Who can reach the panel

| `admin_web` | Reachable from |
|---|---|
| `local` | The server machine only, directly. A request that came through a tunnel or proxy is refused, including the server's own Cloudflare tunnel. |
| `remote` | Also through the proxies in `admin_trusted_proxies`, only for requests they say came over HTTPS. Passwords need 16 characters. |
| `off` | Nobody. `POST /admin` from the machine still works. A `server.cfg` without the key, or with a wrong word, is `off`. |

Other protections:

- On this machine the panel answers only to `127.0.0.1`, `localhost` and `[::1]`. Another site's name pointed at this machine (DNS rebinding) is refused.
- A request whose browser names another origin is refused unless it is in `admin_web_origins`. No other site may read the answers.
- Five wrong passwords in two minutes lock that source for a while. Thirty from anywhere lock everybody except the server machine itself.
  The game's login and the panel count wrong passwords separately.

> **Warning:** With `admin_web = remote` and `tunnel = 1`, the local `cloudflared` is a trusted proxy, so the panel is on the public tunnel address, guarded by the passwords alone. A proxy of your own with a certificate (Caddy, nginx) in front of `127.0.0.1:<web_port>` is safer.

#### Behind a reverse proxy

A reverse proxy on the server machine must add the client's address to what it passes on. One that adds none hands every request from the internet over as this machine's own.

| Proxy | Setting |
|---|---|
| Caddy | Default `admin_proxy_header = x-forwarded-for`. |
| nginx | `proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;` with the default, or `proxy_set_header X-Real-IP $remote_addr;` with `admin_proxy_header = x-real-ip`. A plain `proxy_pass` adds no address: add one of these. |
| Cloudflare's proxy (orange cloud) in front of your proxy | `admin_proxy_header = cf-connecting-ip` and `gateway_proxy_header = cf-connecting-ip`. Otherwise every client looks like a few Cloudflare addresses. |

The proxy must not pass `/admin` (the machine's own door) on.
For `remote`, HTTPS is recognised by `X-Forwarded-Proto: https`, or Cloudflare's `CF-Visitor` with `admin_proxy_header = cf-connecting-ip`.

> **Warning:** A tunnel working below HTTP (`ssh -R`, frp, playit, `ngrok tcp`) hands every client over as this machine, with no header to tell. With one in front of the web port, anybody who reaches it meets the login as if they were local. Point such a tunnel at the game port only, or set `admin_web = off`.

#### Web API

All JSON on the web port. The login token goes in `Authorization: Bearer <token>`, never in a cookie or URL. Requests that change something are `POST` with `Content-Type: application/json`.

| Request | Answer |
|---|---|
| `POST /api/admin/login` `{"password": "…"}` | `200 {"token", "session"}`. `401` wrong password, `429 {"retry_after"}` too many, `404` no password set. |
| `POST /api/admin/logout` `{}` | `200`. |
| `GET /api/admin/session` | `200 {"session"}` |
| `GET /api/admin/state` | `200 {"session", "server", "age_ms"}` |
| `GET /api/admin/players?q=<search>` | `200 {"players", "total", "age_ms"}` |
| `POST /api/admin/ops` (an operation) | `202 {"op"}` queued. `400` invalid, `403` not for this role, `503` too many waiting. |
| `GET /api/admin/ops/<id>` | `200 {"op"}`. `404` no longer kept (the audit log has it). |
| `GET /api/admin/bans` | `200 {"bans": [{"id", "name", "reason", "by", "at", "until", "scope"}], "age_ms"}`. No identifiers. |
| `GET /api/admin/events?audit_after=<seq>` | `200 {"ops": the last 50 records, "audit": entries after seq}` |

Objects:

- `session`: `{"role": "admin"|"moderator", "permissions": ["view", "moderate", "world"], "actor": "web#2 (address)", "expires_in": seconds}`.
- `server`: `{"name", "map", "version", "players", "max_players", "time": "HH:MM", "clock", "clock_speed", "real_time", "weather", "metar", "traffic", "traffic_max", "timetable", "passengers", "passenger_density", "passenger_density_max", "waiting", "walking", "aboard", "cars", "buses"}`, as published about a second ago (`age_ms`).
- A player: `{"id", "conn", "name", "state": "joining"|"driving"|"on_foot"|"aboard", "vehicle", "line", "tour", "aboard", "online_secs", "admin"}`. No position, no address.
- An operation record: `{"id", "actor", "op", "summary", "target": {"id", "conn"} or null, "target_name", "status", "message", "at"}`.

Operations for `POST /api/admin/ops`:

| Body | Role |
|---|---|
| `{"op": "kick", "target": {"id": 3, "conn": 17}, "reason": "…"}` (reason optional, 120 characters) | moderator |
| `{"op": "ban", "target": {…}, "reason": "…"}` (this server run) | moderator |
| `{"op": "hwban", "target": {…}, "reason": "…", "scope": "session"\|"all", "hours": 24}` | moderator for `session`, admin for `all` |
| `{"op": "unban", "id": "0badf00d"}` | admin |
| `{"op": "say", "text": "…"}` / `{"op": "tell", "target": {…}, "text": "…"}` (160 characters) | moderator |
| `{"op": "clock", "seconds": 30600}` (0 to 86400) / `{"op": "shift", "seconds": -3600}` / `{"op": "speed", "factor": 2}` (1 to 30) | admin |
| `{"op": "weather", "next": true}` / `{"op": "weather", "file": "Weather/#CAVOK.owt"}` | admin |
| `{"op": "traffic", "density": 40}` / `{"op": "traffic", "clear": true}` | admin |
| `{"op": "passenger_density", "value": 1.5}` (0 to 3) | admin |

`hwban`'s `scope` defaults to `session`. `hours` is 1 to 8784; without it the ban holds until lifted.

### Local tool: POST /admin

A script on the server machine can administer without joining:

```bash
curl -X POST -H "X-Admin-Password: $PASSWORD" \
  --data-binary $'clock 30600\nsay Good morning!' \
  http://127.0.0.1:27025/admin
```

One command per line, with the admin role. It answers `202` when taken. It works only from the loopback address and needs `admin_password`.
Five wrong passwords in two minutes close it for a while. A reverse proxy on the same machine also connects from 127.0.0.1: do not let it pass `/admin` on.

### Audit log

Every login, refused password, operation and its result is written to `admin-audit.log` beside `server.cfg`.

- One JSON object per line: when, who (`player 3 'Name'`, `local tool (POST /admin)`, `web#2 (address)`), role, action, player, result.
- Rotated past 1 MB to `admin-audit.log.1`, keeping `admin-audit.log.2` too: about 3 MB in total.
- Web logins and refused passwords keep their whole source address, whatever `log_addresses` says.
- No password or token is ever written. Names, reasons and chat are cleaned text.
- A lock is logged once, not every knock on it.

## Bans

There are two kinds of ban:

| Ban | Holds | Lasts |
|---|---|---|
| `ban` | The game's random nonce and the address it came from | This server run only |
| Machine ban (`hwban`) | The machine's identifier | Until it ends or is lifted, across restarts |

A name is whatever the game says, and an address changes or is shared. So the lasting ban is the machine ban.

### Ban a machine

1. In the game, open `Esc` → **Player list...** and pick the player.
2. Choose **Ban this machine...**.
3. Pick a reason, then the scope and length: **This session** or **All my sessions** (admin only), each **for a day**, **for a week** or **for good**.

The web panel and `POST /api/admin/ops` do the same. The player is sent away at once, and the ban is in the audit log.
The host of a session started by code can ban too. A host cannot ban its own machine.

To lift a ban: **Player list...** → **Banned machines**, the web panel, or `unban <id>`. Lifting removes it from whichever list holds it.

### Where bans are kept

| Scope | File |
|---|---|
| This session | `bans.json` beside `server.cfg`. For a game hosting by code: only while it runs. |
| All my sessions | The owner's list, `~/.openomsi/bans-owner.json` of the account the server runs as. Every game and server started from that account reads it, running ones within a second. |

To share one list between servers or machines, point `owner_bans` at a file in a synced folder.
The files are JSON, written to a temporary file and moved into place. A file that cannot be read gives no bans (the log says so, and a `.bak` copy is kept). It is never silently emptied.

### How the identifier works

When a game joins, the host answers its first hello with a *realm*, a random key kept in the owner's list.
The game sends SHA-256 over the machine's own ID and that realm, cut to 128 bits. The machine ID is Windows `MachineGuid`, Linux `/etc/machine-id` or macOS `IOPlatformUUID`; on a system with none, a random number kept in `~/.openomsi/hardware-id`.

- The machine ID never leaves the machine and cannot be recovered from the hash. No disk or board serial is read.
- Every session of one owner list gets the same identifier from a machine. Another owner gets a different one, so owners cannot compare who joined them unless they share a list on purpose.
- The host fixes the identifier to the connection at its first hello and shows it to nobody. The panels list bans by name, reason and time only.
- A banned machine is turned away with `you are banned from this session: <reason>`.

### Limits

- The identifier is the game's claim, not a proof. A modified game can send any value. A reinstalled system, a new machine ID, a virtual machine or another computer is another machine.
- Older games, and players who turned it off, send none. They cannot be machine-banned. `require_hardware_id = 1` turns them away; `ban` still holds them for the run.
- Players turn it off in the launcher: **Settings → General → Send servers an identifier for bans**.
- Bans made before network protocol 16 name identifiers no game sends any more. The server log says so at start. Ban those players again.

## Public server list

`public_server = 1` lists the server in the launcher's **Servers → Public servers** tab.

1. The server registers with the list service (`public_list_url`, by default openOMSI's own).
2. The service fetches the server's `GET /status` and looks for a challenge it gave. So the web port or the tunnel must be reachable from the internet. The tunnel is the sure way.
3. Once found, the server is listed. It says it is alive every minute and is dropped three minutes after the last time.

Listed: name, message of the day, map, players, maximum, version and where to join. Nothing about the players is sent.

`name`, `motd` and `server-icon.png` are checked before the first registration. A name or message that is flagged (profanity, ways round a filter, addresses, hidden characters) refuses the listing, and the log says which key to change.
Every launcher checks each listing again before showing it: a flagged text is covered.

> **Note:** Custom icons are not shown in the public list yet. No image classifier is bundled, so every custom icon counts as unchecked and the list shows the default icon.

## Web endpoints

The web port (`web_port`, also behind the tunnel) answers:

| Request | Answer |
|---|---|
| `GET /` | A small page with the name, message, map and players. |
| `GET /status` | JSON: `name`, `motd`, `map`, `players`, `max_players`, `version`, `date`, `time`, `weather`, `protocol`, `vehicles`, the announced fleet, and `world` (AI cars, buses, parked cars, people walking, waiting and aboard, traffic and passenger density). |
| `GET /icon.png` | The server icon. |
| `GET /players` | Only with `share_positions = 1`, else `404`. |
| WebSocket | Players joining over the web ([below](#websocket-players)). |
| `/admin/`, `/api/admin/` | The [web panel](#web-panel) and its API. |
| `POST /admin` | The [local tool](#local-tool-post-admin). |

`GET /players` returns a JSON array refreshed every second, for a live map on a website:

| Field | Meaning |
|---|---|
| `id`, `name` | Player number and name. |
| `bus`, `paint`, `line`, `destination`, `tour` | What they drive. Empty when on foot. |
| `x`, `y` | World metres east and north. |
| `heading` | Degrees clockwise from north. |
| `speed_kmh` | Speed. |
| `on_foot` | Not driving. A player on foot is where they walk, even with their own bus parked nearby. |
| `aboard` | The id of the player whose bus they ride in, or `null`. They are placed with that bus. |
| `parked` | A bus standing free ([parked buses](#parked-buses)). |
| `lat`, `lon` | On a map with `[worldcoordinates]` (such as Berlin-Spandau), else `null`. |

It is off by default: tell your players when you turn it on.

### WebSocket players

Players can join at the web port over a WebSocket instead of UDP. Each datagram is one binary message. This is what the Cloudflare tunnel carries.

The gateway gives each WebSocket player a UDP socket of its own on 127.0.0.1. The session log then says `joined from 127.0.0.1:<port>`, and the gateway logs who that is: `gateway: player WebSocket from <address> on 127.0.0.1:<port>`.
The address is the one a proxy or tunnel on this machine forwards: the last `X-Forwarded-For`, else `CF-Connecting-IP`.

With Cloudflare's proxy in front of a proxy of yours, the last `X-Forwarded-For` is a Cloudflare edge, and players would share a few addresses (16 connections each at most). Set `gateway_proxy_header = cf-connecting-ip` then, and only then.

The same gateway and tunnel also open for a player hosting by code. A joining game that cannot reach the code's addresses joins through the tunnel.

## Running as a service

The start scripts `start.sh` and `start.cmd` are meant for services: they ask nothing.

### Linux (systemd)

1. Create a user for the server, for example `openomsi`, and unpack the server into `/opt/openomsi-server`.
2. Run it once by hand to write `server.cfg`, then edit it.
3. Create `/etc/systemd/system/openomsi-server.service`:

```ini
[Unit]
Description=openOMSI dedicated server
After=network-online.target
Wants=network-online.target

[Service]
User=openomsi
WorkingDirectory=/opt/openomsi-server
ExecStart=/opt/openomsi-server/start.sh /srv/omsi2 /opt/openomsi-server/server.cfg
Restart=on-failure
RestartSec=10
KillSignal=SIGINT
TimeoutStopSec=20

[Install]
WantedBy=multi-user.target
```

4. Start it and enable it at boot:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now openomsi-server
journalctl -u openomsi-server -f
```

The players' buses and the owner's ban list live in the service user's `~/.openomsi`.

### Windows

Use the Task Scheduler, which every Windows has:

1. Open **Task Scheduler** → **Create Task**.
2. **General**: a name, **Run whether user is logged on or not**.
3. **Triggers**: **At startup**.
4. **Actions**: **Start a program**, program `C:\openomsi-server\start.cmd`, arguments `"C:\path\to\OMSI 2"`, start in `C:\openomsi-server`.
5. **Settings**: untick **Stop the task if it runs longer than**. Optionally **If the task fails, restart every** 1 minute.

### Docker (Linux)

The repository has a container setup in `docker/server` (see its `README.md`):

```bash
cd docker/server
cp .env.example .env              # set OMSI_DIR to your OMSI 2 folder
mkdir config
cp server.cfg.example config/server.cfg
docker compose up -d --build
docker compose logs -f
```

The OMSI 2 folder is mounted read-only. `config/` holds `server.cfg`, `content/` the packed maps, and a volume keeps the players' buses.
The server is ready when `http://<address>:27025/status` answers (`docker ps` shows `healthy`).
Docker keeps the log itself: five files of 20 MB.

## What the server keeps

Tell your players what is kept, for example in the `motd`. `server.cfg` lists the same at its end.

| What | Where | How long |
|---|---|---|
| Who joined and left, names, **every chat message**, admin actions, errors | `server.log` beside the start script (`run-server`, `start.sh`; Docker: `docker compose logs`) | New file at every start and past `log_max_mb`. `log_keep` older files. |
| Players' internet addresses | The same log | Public addresses cut to their network unless `log_addresses = 1`. Local network addresses are written whole. |
| Logins, refused passwords, admin and moderator operations, with whole source addresses for web logins | `admin-audit.log` | About 3 MB in total. |
| Banned machines' identifiers | `bans.json`, the owner's list | Until the ban is lifted. |
| Players' buses (`accept_uploads = 1`) | `~/.openomsi/lan-store` | `uploads_keep_days` after last use. |
| Name, icon, map, players count, motd | The public list (`public_server = 1`) | Until three minutes after the server stops. |
| Names, buses, lines, positions | Anyone asking `GET /players` (`share_positions = 1`) | Live only. |

Voice is passed on and never recorded or decoded.
A game hosting from the launcher cuts addresses in `game.log` the same way. Before a crash report becomes a GitHub issue, the launcher also removes the account's folder and name and the driver's name.

The server prints at start that you run it, not openOMSI: you are responsible for the data players send it and what you keep.

## Troubleshooting

The server prints why it stops and ends with an exit code a launcher can tell apart:

| Exit code | Cause | Fix |
|---|---|---|
| 1 | The original OMSI 2 was not found at `--root` (the missing files are listed), or another error printed as `Error: …` | Point `--root` at a complete installation. `run-server … --setup` chooses the folder again. |
| 2 | `server.cfg` cannot be read or written, or its `map` is not installed (the installed maps are listed) | Fix the file's permissions or the `map` key. |
| 3 | The UDP `port` is taken | Stop the other server or program, or choose another port. |

A taken `web_port` does not stop the server: it takes another port and logs which.
The log goes to standard error. Informational lines go there too: the exit code, not a line, says whether it failed.

Other problems:

- **Players cannot connect over UDP.** Open UDP and TCP 27015 and TCP 27025 in the firewall and any cloud security group. Or let players use the tunnel address.
- **"server.cfg: admin_password has fewer than 12 characters"**: the administration stays off until the password is longer. Servers set up with 8 to 11 characters need a longer one now.
- **The web panel does not open.** Check `admin_web` (a `server.cfg` from before the panel has none, which means off). From another machine you need `remote`, an HTTPS proxy and 16-character passwords.
- **The server is not in the public list.** The log says why: a flagged name or message, or the service cannot reach the web port. Keep `tunnel = 1` or open the web port.
- **A map is missing for players.** With `share_maps = 0` players install it themselves. Turn it on, or tell them which map to install.
- **Players are sent away for their bus.** The `vehicles` list does not include it. The kick message names the allowed buses.
- **Slow first start.** With a long `vehicles` list, the first start checks every file of every bus. Later starts are faster.

### Helper flags

For your own launcher or scripts:

| Flag | Does |
|---|---|
| `--root <folder> --list-maps` | Print the installed maps, one per line, as `server.cfg` names them (content folder included), and end. Exit code 1 with the missing files when the folder is not a complete OMSI 2. |
| `--root <folder> --server <server.cfg> --list-maps` | The same, and write a missing `server.cfg` with its defaults. |
| `--server-map maps/<folder>/global.cfg` | Start on this map instead of `server.cfg`'s. |
| `--pack-map <map> --pack-to <folder or .zip>` | Pack a map with everything it uses ([above](#sharing-the-map)). |

Paths with spaces or accents are passed as they are, quoted. Maps are named relative to the OMSI 2 folder.
More flags are in [Command line](COMMAND_LINE.md).

## License

openOMSI is released under the GNU Affero General Public License, version 3 only (`LICENSE`).
Its launcher preview encodes pictures with [jpeg-encoder](https://crates.io/crates/jpeg-encoder): this software is based in part on the work of the Independent JPEG Group.
