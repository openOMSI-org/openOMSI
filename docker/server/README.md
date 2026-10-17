# openOMSI dedicated server (Linux, Docker)

The server is `openomsi` itself started with `--server`: no window, no sound, no graphics
card. This container builds it for Linux and runs it with your OMSI 2 installation mounted
**read only**. It serves both for a local test (Docker Desktop on Windows) and for a Linux
server.

## Requirements

- Docker with Compose (Docker Desktop on Windows; `docker` + `docker compose` on Linux).
- A **complete** OMSI 2 installation (the one the players have: maps, stock buses). On Linux,
  copy the OMSI 2 folder to the server (e.g. `/srv/omsi2`).

## Install and start

In this folder (`docker/server`):

```sh
cp .env.example .env              # Windows: copy .env.example .env
#   edit .env: OMSI_DIR = your OMSI 2 folder (the one with Omsi.exe, maps and Vehicles)
mkdir config
cp server.cfg.example config/server.cfg   # optional: without it the server writes one with the defaults
docker compose up -d --build      # the first build takes several minutes
docker compose logs -f            # watch: "server '...': map ..." and then the map loaded
```

The server is ready when `http://<address>:27025/status` answers (Docker shows `healthy` in
`docker ps` too).

### Ports

| Port | Protocol | What for |
|---|---|---|
| 27015 | UDP | the session (positions, events) |
| 27015 | TCP | the map's mods and the **players' buses** (sent and fetched by content) |
| 27025 | TCP | web: `/status`, icon, players over WebSocket |

On a server on the internet, open all three in the firewall.

## Joining (players)

In the launcher: **Multiplayer → Join**, address `<server address>:27015` (locally
`127.0.0.1:27015`). Or from the command line:
`openomsi.exe --lan-join 127.0.0.1:27015 --bus Vehicles/... --map maps/Grundorf/global.cfg`.

Every player needs a game with the players' buses by content to send and receive them.

## Configuring the server (map, name, traffic)

Name, message, traffic and the number of players are keys of `config/server.cfg`; the
server takes them at its next start (`docker compose restart`).

To put a map on the server with everything it uses (scenery objects, splines, traffic cars,
people, ground textures), pack it into the `content` folder beside this file and set `map`
in `server.cfg`:

```sh
openomsi.exe --root "<OMSI 2 folder>" --pack-map "<map name>" --pack-to content
```

On the same disk as OMSI the files go in as hard links, taking no space again.

The server **does not hand out maps** to the players (`share_maps = 0` in `server.cfg`, the
default): a big map is several GB and would eat the server's bandwidth. Every player installs
the map themselves; whoever joins without it is told which map is missing. The players'
buses are still passed on by the server.

Files the map names that do not exist on your machine are listed in
`content\openomsi-map-pack-<map>.txt`: neither the server nor the players will have them.

For a server on another machine, make a `.zip` and unpack it into that machine's content
folder: `openomsi.exe --root "<OMSI 2 folder>" --pack-map "<map name>" --pack-to map.zip`.

## Players' buses

- A player who joins with a bus the server lacks **sends** it under the loading screen
  ("Sending your bus to the server", with its progress). Only what the server lacks goes:
  files it has in its own installation, in the same version, are not sent.
- The other players see the bus **exactly as its driver has it** (same files, same paint,
  same `.cti`): from their own copy when it is identical, else fetched from the server.
  Meanwhile, a **grey box of the bus's size** with "loading the bus: 40%".
- Nothing is written into anybody's `Vehicles` folders; every player's `.cti` and paints
  stay their own.
- The server keeps the buses in the `openomsi-data` volume (`/root/.openomsi/lan-store`),
  every version. Buses nobody has used for `uploads_keep_days` days (30) go; when the disk
  runs short, the least used go first. No bus is refused for its size.

## Automatic end-to-end test (Windows)

With the server running and the game built (`cargo build --release -p omsi-app`):

```powershell
scripts\e2e-bus-sync.ps1 -OmsiDir "D:\SteamLibrary\steamapps\common\OMSI 2"
```

Two games without a window join the server: A with a modified copy of the SD77, B with a
stock SD80. The script checks in the logs that the server received only the file that
differs, that B fetched A's bus by its content and that A recognised B's SD80 as identical.

## Useful commands

```sh
docker compose ps                 # state (healthy once /status answers)
docker compose logs -f            # the server's log
docker compose restart            # restart (the kept buses stay)
docker compose down               # stop (the volume with the buses stays)
docker compose down -v            # stop and DELETE the kept buses
curl http://127.0.0.1:27025/status
```
