# Multiplayer

Drive on the same map with friends: host a session from your own game and send them a code, or join a dedicated server.
This page is for players. To run a server that is always on, see [Dedicated server](SERVER.md).

## Ways to play together

| You want to | Do this |
|---|---|
| Play with friends, one of you hosts | The host picks **Invite friends** on the **Drive** page. The others use **Join friends** with the session code. |
| Join a server someone runs | **Servers** page: add its address, or pick it from **Public servers**. |
| Join the official server | It is in the **Servers** list as *openOMSI \| Official Server* (address `openomsi`). |
| Switch rooms without leaving the game | In the game: `Esc` → **Multiplayer...** |
| Start from a script or shortcut | `--lan-host` and `--lan-join` on the command line ([below](#command-line)). |

Windows, macOS and Linux players play together. Every player needs the same version of openOMSI: a game of another network protocol version is turned away with a message saying so.

## Host a session

The host's game runs the world for everybody: the traffic, the timetable buses, the people and the clock.

1. Open the **Drive** page and choose the map, bus and duty as usual.
2. Under **Play mode**, select **Invite friends**.
   The launcher starts preparing the internet connection in the background.
3. Optional: in **Buses of this session**, list the buses your players may drive ([see below](#limit-the-buses)).
4. Press **Host and drive**.
5. When the game has loaded, the **session code** appears on the **Drive** page and on **Running games**.
   Press **Copy code** and send it to your friends.
   In the game, `Esc` → **Copy server code** copies it too.

A session code looks like `OMSI-7Q4K-2M9X-HD3P-R8TZ-KC5W-NB6E`. It has no look-alike characters, so it can be read out loud.
While the host is reachable only on the local network, the page says *Getting ready for friends on the internet...*.
Once the tunnel is up it says *Friends can join from anywhere*.

> **Note:** As the host, your game receives your players' driver names, network addresses, chat and voice. You decide what you keep of it, such as logs and bans.

### Limit the buses

**Buses of this session** takes a list separated by `;`:

- a bus folder, such as `MAN_SD200` or `Vehicles/MAN_SD200` (every bus in that folder), or
- a bus file, such as `Vehicles/MAN_SD200/MAN_SD77.bus`.

Empty means every bus. A player who drives a bus that is not on the list is sent away and told which buses the session has. They can join again with one of them.

### Running the session

Open `Esc` → **Player list...** to see who is there. As the host you can act on every player: see [Player list and moderation](#player-list-and-moderation).
`Esc` → **Administration...** sets the shared world: the clock and its speed, the weather and the AI traffic.

When the host quits, the session ends for everybody.

## Join friends

1. Get the session code from the host.
2. On the **Drive** page, press **Join friends**.
3. Paste the whole code and press **Confirm session**.
   The launcher checks the code and asks the host's game for the session details.
4. Choose your bus on the **Drive** page.
   The map, date, time and weather are the host's. The Drive bar shows them.
5. Press **Join and drive**.

If the code is valid but the session details cannot be read, the dialog offers **Choose a bus without session details**. The game can still connect over UDP.
If the session is full, ask the host to make room and confirm again.

To go back to driving alone, press **Cancel** in the session notice on the **Drive** page. Your previous map and bus choice come back.

## Join a server

Dedicated servers are listed on the **Servers** page. It has two tabs: **Saved servers** and **Public servers**.

### Add a server by its address

1. Open **Servers** and press **Use a server address**.
2. Type the address the server owner gave you. It can be:
   - `host` or `host:port` (UDP, port 27015 by default),
   - a web address such as `https://name.trycloudflare.com` (WebSocket),
   - `openomsi` for the official server.
3. Optional: give it a name, and pick the **Connection protocol** ([see below](#udp-or-websocket)).
4. Press **Select server** to join now, or **Save to list** to keep it for later.

Each saved server shows its icon, map, players, time, weather and message of the day. The list is asked again every 15 seconds while it is open.
**Refresh** asks one server now. The bin icon removes it from the list.

To join a saved server:

1. Press **Select server** on its card.
2. Choose your bus on the **Drive** page. Only the buses the server allows are offered.
3. Press **Join and drive**.

### UDP or WebSocket

A server can be reached two ways. The **Connection protocol** chooses:

| Protocol | When |
|---|---|
| **Auto** (default) | UDP for a plain `host:port`, WebSocket for a web address. If UDP gets no answer, the game tries the server's web port. |
| **UDP** | Fastest. Needs the server's UDP port to be reachable. |
| **WebSocket** | Goes through the server's web port or tunnel. Works where only HTTPS gets through. |

### Public servers

1. Open **Servers** → **Public servers**.
2. Press **Show the public list**.
3. Press **Select server** to join one, or the **+** button to add it to your saved servers.

Only servers whose owners chose to list themselves appear here. Your launcher checks every name, message and picture before it shows it: a flagged text is covered.

### Buses from the server

A server can offer buses you do not have installed. In the bus picker they show a notice: *Server-only bus: required files will be verified and fetched for this session when you start.*

When you press **Join and drive** with such a bus, the launcher asks **Download server vehicle**.
Matching local files are reused. Other files are checked and cached for this session. Your installed mods are never replaced.
Press **Download and join** to go on.

### Teleporting on a server

On a dedicated server, teleporting your own bus (F4 map click, the navigator's city map, a start point) is a request to the server.
Whether ordinary players may do it is the server's setting. Admins and moderators always may.

## Switch rooms in the game

`Esc` → **Multiplayer...** opens the multiplayer panel without leaving the game.

- **Join room**: paste a session code or a server address (**Paste connection code**), then **Join this room**.
  The game connects to the new room first while the current one stays open. You confirm before it switches.
- **Reconnect to current room**: join the same host again after a lost connection.
- **Leave room and play offline**: go on driving alone.

The panel warns you when the new room uses a different map (the world reloads and your drive ends) or does not allow your bus.
If you are the host, leaving or switching closes your room for all other players.

## Chat

| Key | Does |
|---|---|
| `/` | Open the chat line. If your keyboard file uses `/` for something else (OMSI 2's manual gearbox does), the chat opens on the key left of `1`, or on `\`. |
| `Enter` | Send the line. |
| `Esc` | Drop the line. |
| **V** | Show or hide the chat. |

Both keys can be changed in **Controls** (*Multiplayer: write in the chat*, *Multiplayer: show / hide the chat*).
Joining and leaving are announced in the chat. Bad words are starred out.

Chat commands:

| Type | Does |
|---|---|
| `/reconnect` | Join the host again after the connection was lost, without restarting the game. |
| `/admin <password>` | Become an admin or moderator of a dedicated server. The password never goes over the network. See [Dedicated server](SERVER.md#administration). |

The chat grows with the interface size. It also has a size of its own: `Ctrl` + mouse wheel over the chat, or **Settings → General → Chat size** (50 to 300 %).
**Settings → General → Chat in online games** turns the chat off.
**Other players' names above their buses** turns the name tags on and off.

## Voice chat

Hold a key to talk: **V** for players nearby, **B** for a radio channel the whole session hears.
Voice chat is off until you switch it on in **Settings → Sound → Voice chat**.
Everything about it is in [Voice chat](VOICE.md).

> **Note:** By default **V** is both the chat's show/hide key and the nearby talk key. If you use voice chat, give one of them another key in **Controls**.

## Player list and moderation

`Esc` → **Player list...** shows every player by name and number, what they drive or do, and their role.
Pick a player to open their page:

- **Voice chat: audible. Mute them** / **Voice chat: muted. Hear them again** mutes that player for you only. Everybody can do this.

The host of a session, and an admin or moderator of a server, also get:

| Action | Does |
|---|---|
| **go to** | Puts your bus beside theirs. |
| **bring here** | Puts their bus beside yours. |
| **repair and refuel their bus** | Repairs and refuels it. |
| **put their bus back on its wheels** | Rights a bus that tipped over or got stuck. |
| **Silence their voice chat for everybody** / **Allow their voice chat again** | Mutes them for the whole session. |
| **Send away...** | Kicks them, with an optional reason. They can join again. |
| **Ban this machine...** | Bans their computer: this session or all your sessions, for a day, a week or for good. Or **Only for this run** (by name and address). |

**Banned machines** at the end of the list shows the bans and lifts them.
A ban holds the machine, not the name: changing a name does not lift it. How machine bans work is in [Dedicated server](SERVER.md#bans).

The reasons offered are: none, *Reckless driving or blocking others*, *Insulting or spamming in the chat*, *Cheating or exploiting the game*, *Against the server's rules*.

## What is shared

The host (or the server) owns the world. Each player owns their own bus.

| Shared from the host | Your own |
|---|---|
| Map, date, time of day, weather, season | Your bus and how you drive it |
| AI traffic, timetable buses, traffic lights | Your money |
| People on the pavements and at the stops | The timetable of your own duty |
| The clock and its speed | Your settings, graphics and sound |

How it works in play:

- **One world.** A joining game simulates no traffic and no people. It draws the host's world 160 ms in the past, so it moves smoothly between the host's updates.
- **Passengers.** When your bus stands at a stop with a door open, your game asks the host for the people waiting there. Nobody is ever on two buses.
  Everybody sees the passengers in everybody's bus.
- **Timetable tours.** The host's timetable leaves the tour you drive to you. Its AI bus comes back from the next departure when you leave.
- **Other players' buses** are drawn and heard where they stand, and the AI traffic treats them as obstacles.
  Their doors, wipers, needles, displays, script sounds and script variables follow their owner's game.
- **A bus you do not have.** If the server accepts players' buses, you get the real bus ([see below](#other-players-buses)). Otherwise a stand-in of your own type is drawn.

A player on foot can ride in another player's bus. To ask for a door, stand within 3.2 m of an entry point outside a bus (or an exit point inside the bus you ride) and press `Ctrl+Alt+S`. The request goes to the bus's owner, or to the host for an AI bus. The key is **Passenger: request entry / exit** in **Controls**.

### The host's mods

What the host's session uses beyond the OMSI 2 folder itself is offered to joining players: its map, bus, objects, splines, AI vehicles and people from the content folder or archives.

- A joining game downloads what it lacks before it loads the map, and keeps it for next time.
- Each file is checked against a SHA-256 hash.
- Maps installed directly into the OMSI 2 folder are not passed on: every player needs them installed.
- A big add-on map takes the host a while to list after it starts. A joining game waits for it.
- If the map is not installed and not offered, the HUD says which map is missing.

The downloads travel encrypted and are stored sealed in `~/.openomsi/lan-store`. They are meant for playing the session, not for keeping. Copying them out of the game's folders gives unreadable files.
This is obfuscation, not a lock: the decryption runs on your own computer.

To see how much space multiplayer downloads take, or to clear them, open **Troubleshooting** → **Disk cache and multiplayer downloads**.

### Other players' buses

A server with `accept_uploads = 1` (the default) takes the bus of a player who joins with one it lacks, and passes it on to everybody.
A session a player hosts does the same.

- Only what your game actually loaded for that bus is sent: the variant and paint you drive, the shared folders it points into, the fonts of its displays.
- Stock OMSI 2 content is never sent. Programs, libraries and plugins are refused.
- You send it under the loading screen (*Getting your bus to the server*). After three minutes the rest goes on in the background.
- Other players fetch it when your bus first comes into view (*Downloading another player's bus: 40%*). Meanwhile they see a grey box of its size.
- Nothing is written into anybody's `Vehicles` folder.

## Connections and firewall

The session code carries up to three addresses of the host: a VPN address first (Hamachi `25.x.x.x`, Radmin VPN `26.x.x.x`, ZeroTier, Tailscale `100.64.0.0/10`), then the LAN address.
The joining game tries all of them at once and keeps the one that answers.

Over the internet, no VPN and no port forwarding is needed:

1. The host asks its router to forward the port (UPnP).
2. Both games learn their public address (STUN) and meet through a public message relay that passes only the addresses (UDP hole punching).
3. If that fails, the joining game uses the host's free Cloudflare quick tunnel. The game downloads `cloudflared` itself the first time it hosts.

If nothing answers within 10 seconds, the joining game gives up and says why that may be.

| Port | Protocol | Used for |
|---|---|---|
| 27015 | UDP | The session. A host on a busy machine takes the next free port up to 27024. |
| 27015 | TCP | The host's mods and the players' buses. |
| 27025 | TCP | A dedicated server's web port (status, WebSocket players). |

The host's firewall must let the game receive UDP on its port. On Windows, allow it for public networks too: Windows treats a Hamachi network as public.

## Command line

| Flag | Does |
|---|---|
| `--lan-host <port>` | Host a session on this UDP port. `0` takes 27015, or the next free port up to 27024. |
| `--lan-join <where>` | Join a session. `<where>` is a session code, `ip`, `ip:port`, a host name, a bare port (a host on this machine), a web address (`https://…`, WebSocket), `openomsi` (the official server) or `auto` (search the local network). |
| `--lan-name <name>` | Your name as the others see it. The launcher passes your profile name. |
| `--session-buses "<bus>;<bus>"` | The buses a session you host allows ([as above](#limit-the-buses)). |

The full list of flags is in [Command line](COMMAND_LINE.md).

## Common problems

**"no answer from … within 10 s"**
- Check that the session still runs and that the whole code was copied.
- The host's firewall must allow UDP 27015-27024. On Windows, allow it for public networks too.
- Some mobile and carrier networks change the port for every connection. Two such networks cannot be connected directly. The host can forward UDP 27015 on the router, both players can use a VPN, or you can play on a dedicated server.

**"both players need the same version"**
The host and the joining game have different network protocol versions. Update both to the same openOMSI version.

**"the session is full"**
The host or server has reached its player limit.

**"you are banned from this session"**
The host or a server admin banned your machine. Only they can lift it.

**"this session needs the game's identifier for bans"**
The server turns away players who send no machine identifier. Turn on **Settings → General → Send servers an identifier for bans**.

**"only these buses can be driven here (…)"**
The session has a bus list. Join again with one of the buses named.

**The map is missing**
The HUD names the map. Install it, or ask the server owner whether the server shares its map.

**"the host has not answered for … s - the session is over"**
The connection was lost. Type `/reconnect` in the chat, or use `Esc` → **Multiplayer...** → **Reconnect to current room**.

**The session code holds the wrong LAN address**
On a machine with several network adapters the game can pick the wrong one. Set `OMSI_LAN_IP=a.b.c.d` before starting the host to offer that address first ([Environment variables](DEBUG_FLAGS.md)).

**The game could not open a session**
Another program may use the port, or a firewall or antivirus blocks it. Close the game and start the duty again. The launcher's **Technical details** show the exact error.

## Details

- In multiplayer, the time speed is the host's or the server's. The optional passenger connections between lines are off.
- Each game sends its bus's state up to 20 times a second (5 while nothing changes), about 60 bytes. Everything else is text. The host checks everything it receives and limits how much each player may send.
- Each game also sends all script variables of its bus (floats and strings up to 1 MiB): the changed ones 10 times a second, the rest in turn. A player who joins late gets the whole state.
  When two players have different versions of a bus, variables both versions have are still passed by name. `OMSI_NO_VAR_SYNC=1` turns this off.
- Matrix displays and other script textures come from the bus's owner. Both games need the same bus with the same texture slots. Free textures are covered in [Freetex images](FREETEX_SYNC.md).
- `(T.L.name)` and `(T.F.name)` script sounds come from the bus's owner (the host for AI vehicles), once each and in order. Your game plays its own installed sound file. Sound files are never transferred.
- AI vehicles' scripts read the game date, time, weather and sun through `L.S.*` from their initialization. A joining game's copies of the AI buses get the host's timetable state before their scripts start.
- How the AI traffic travels to joining games is described in [The traffic table](TRAFFIC_TABLE.md).
- **Settings → General → Discord: multiplayer and the number of players** shows your session in Discord.
