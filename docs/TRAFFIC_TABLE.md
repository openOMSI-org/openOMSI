# The traffic table

How the host's AI traffic reaches the other players in a multiplayer session. This is a technical reference for developers and for server owners who want to understand the bandwidth.
Players do not need it: see [Playing together](MULTIPLAYER.md).

## Overview

Every AI vehicle of the host's world is one row of a table, under an id of its own.

1. The host's traffic simulation writes the table after each step.
2. The host tells each joining game about the rows near it, ten times a second.
3. Each joining game keeps its own copy of the table and draws its cars from it.

The table code has no game and no GPU, so a dedicated server uses it unchanged.
It came with network protocol 15. Protocol 17 added the timetable buses' fleet number, registration and depot file.

Code: `crates/omsi-net/src/traffic_table/` (table, wire format, tests), `crates/omsi-app/src/traffic/table.rs` (the host's writer), `crates/omsi-app/src/lan_world.rs` (the joining game's side).

## What a row holds

| Part | Contents | Changes |
|---|---|---|
| id | Slot (12 bits) and generation (8 bits, 1 to 255) | Never during a vehicle's life |
| kind | Car, bus, rail, motorcycle, truck, other | Never |
| look | Vehicle file (relative to a content root), repaint index and name. Shared by rows, told once per client. | Never |
| meta | A timetable bus's line, destination (`#<row of the depot file>`), fleet number, registration and depot file | When the bus gets a new trip |
| row | Position (cm, world coordinates), heading (360/4096°), pitch and bank (0.1°), speed (0.05 m/s), front wheel angle (0.5°), indicators, brake lights, headlights, stop state (boarding / leaving) | Every step |

A row is held in the units it travels in. Two rows are equal exactly when they would be drawn the same, and the host's row is bit for bit what a client decodes.
Wheels, doors, lamps and sounds come from the client's own vehicle scripts, fed with these numbers.

### Ids

The table hands out the ids.

- A slot a vehicle leaves rests for 30 s, then comes back with the next generation. A late datagram, or a client that missed the vehicle's end, can never mistake the next vehicle in that slot for the old one.
- When the table is full (4096 rows), a new vehicle is not told.
- The simulation keeps its own car numbers. Where another channel names a car (its script context, its sounds, the riders of a timetable bus in `WORLD`), it uses the table id.

## Who reads what

- **The simulation writes** the table after each step. A client's mirror does not write.
- **Nothing reads the table back** into the simulation. Collisions with the player, the AI's decisions and the timetable buses' passengers stay the host's.
- **The host's clients** are told from the table.
- **A client's table** is what its mirror cars are made and placed from.
- **The host's own pictures and sounds** (and single player) still read the simulation's vehicles directly. Those are the instances whose scripts the table's numbers come from; drawing them from a second copy would run their scripts twice.

The wire and the client's drawing are tested without a game: both ends run in one process over a simulated network that loses, delays and reorders datagrams.

## The wire

Two datagrams, both bit-packed like the other binary ones.

**`TRAF` (`0xBA`), host to client**, ten snapshots a second: new looks, spawns (id, look, kind and the whole row), line and destination changes, removals, and row updates.

- A snapshot is cut into datagrams of at most 1180 bytes, below the MTU of common VPNs. Each datagram stands on its own.
- A row goes whole (110 bits, centimetres from an anchor near the client: 21 bits reach 5 km), or as a delta against a row the client acknowledged.
- A delta takes a few bits when nothing changed, about 6 to 7 bytes for a moving car.

**`TACK` (`0xBB`), client to host**: the newest datagram heard and which of the 32 before it, plus up to 8 problems (a row it cannot use as a delta base, a vehicle it does not know).

### Acknowledged state

What a datagram carried belongs to the client once acknowledged. So:

- A standing vehicle costs nothing once acknowledged, apart from a sign of life every 8 s.
- What is not acknowledged (lost datagram, or lost answer) is told again with the row's current state. Deltas are always against the last acknowledged row, so a lost datagram never makes a later one unreadable.
- Spawns, looks, line changes and removals are repeated every 0.35 s until acknowledged.
  A removal carries the snapshot it was made in, and so does a spawn. A late copy of an old spawn after a removal, or of an old removal after a new spawn, is ignored.
- A client that loses a row, or never had it, says so in its next answer, and the host starts that vehicle over.
- A host that starts a client's table anew changes the epoch byte, and the client starts over too.

### Range, priority, budget

| Rule | Value |
|---|---|
| Cars told to a client | Within 650 m of its vehicle; dropped beyond 700 m (no flicker on the edge) |
| Update rate within 200 m | 10 per second |
| Update rate within 400 m | 5 per second |
| Update rate beyond | 3 per second |
| Budget per client | 40 000 bytes per second |

What does not fit the budget waits, the longest-waiting and nearest first.
`OMSI_DEBUG_LAN=1` logs rows told whole or as changes, rows deferred by the budget, and answers heard ([Environment variables](DEBUG_FLAGS.md)).

### Drawing

- A client draws the cars 160 ms behind the host, blending linearly between two rows (angles the short way round).
- Past the newest row it extrapolates from the speed and the turn for at most 400 ms, and not at all for a standing vehicle.
- A jump of more than 30 m is a teleport: no gliding.
- Indicators, brake lights and stop state switch halfway between the two rows.

## Compatibility

Games of different network protocol versions turn each other away at the handshake with the usual message ("both players need the same version").
Since protocol 15, `WORLD` no longer carries cars and `DESC|c|…` is gone.
The first bytes `0xBA` and `0xBB` are reserved: no text message starts with them.
