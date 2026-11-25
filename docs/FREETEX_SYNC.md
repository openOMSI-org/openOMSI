# Free textures in multiplayer

How a bus's `[matl_freetex]` pictures reach the other players. This is a technical reference for bus modders and developers.

When two players have the same bus, each sees the owner's free-texture pictures exactly as the owner's game shows them.
This includes pictures that a program outside the game regenerates under the same file name.
The receiving player does not need those files.

## What players see

- The owner's current picture, on every player's copy of that bus, rear sections included.
- Until the first picture arrives, the bus's original material.
- An empty slot on the owner's side restores the original material on the others.
- An unrelated local picture with the same path is never used in its place.

Requirements:

- Both players and the host or server need a version with the free-texture stream. Older builds ignore its packet type.
- Both need the same bus. The same bus and variable checks apply as for script textures.
- Stand-ins and fallback vehicles get no pictures.

The pose and script-texture streams are not affected.

## Content matching

Before sending pixels, the owner sends a small SHA-256 content reference. It covers the source file's SHA-256, the image size and the displayed RGBA pixels.

- A name alone never allows reuse. The displayed pixels also catch a file rewritten while the GPU still shows the older version, or different texture settings.
- The receiver checks the images it displays now, and the local image named by the owner's synchronized freetex variable, even if its own bus does not show that file.
- If the content matches, the picture is resolved locally and no image bytes travel.
- If it is missing or different, the receiver asks for pixels, never for the original file.
- A short grace period lets names and variables arrive after a content reference. Lost references and replies are retried.
- File reads, hashing and decoding run on a worker thread.

## Where pictures come from and go

**On the owner's side.** The stream takes the texture data already uploaded for the selected freetex slots, including updates from the reload worker and texture upgrades.
A slot is identified by the bus path, the section number and the freetex variable. Absolute folders and generated file names are not part of it.

**On the receiver's side.**

- Each owner and slot gets its own GPU texture. It never enters the shared texture cache.
- Received pictures stay in memory. They are never saved into the content folders and never overwrite the receiver's images.
- Remote names cannot make the lookup open files outside the local content root.
- Textures are released with the remote vehicle.
- A replacement vehicle or a reconnect starts a new subscription, so stale packets cannot change the new bus.

## Work and bandwidth

- Snapshots are taken at most ten times a second.
- Block decoding, RGBA copies, delta generation, compression and packet assembly run on one worker. The frame uses non-blocking, bounded channels.
- At most one complete image is uploaded per frame. Same-size updates reuse their GPU allocation; a size change rebinds the surface.

The stream reuses the acknowledged script-texture protocol on packet type `0xBD`:

1. The first picture goes as lossless zlib-compressed pixels.
2. Later ones send only the changed pixel runs against each subscriber's acknowledged image.
3. Unchanged images are not resent. Partial, reordered or lost transfers are retried and never shown half-done.

| Limit | Value |
|---|---|
| Data packet | 1200-byte payload, at most 1245 bytes in all |
| Send rate | 8 data packets per 50 ms per publisher, shared round-robin by subscribers: about 195 KiB/s at most, plus small subscription and acknowledgement packets |
| Slots | 64 |
| One image | 16 MiB of RGBA |
| Local decoded images or received baselines | 64 MiB |
| Source snapshots | 64 MiB |
| Remote GPU surfaces | 64 MiB |
| Local lookup cache | 64 MiB, at most 64 candidates |
| File hashing | 64 MiB per source |

The send rate is a ceiling; simple display changes use much less.
Session authentication, sender-address checks and the texture rate limiter also apply when the host relays between players.

- A source file too large or unreadable disables reference reuse. Valid captured pixels can still be sent.
- Images beyond a pixel or device budget are not uploaded.
- Local candidates are checked at most twice a second, one lookup per worker pass. Idle unchanged images are not rehashed or resent.
