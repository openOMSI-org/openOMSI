# Multiplayer freetex images

Players with the same vehicle see the owner's `[matl_freetex]` pictures, including
externally regenerated pictures whose filename did not change. A receiver does not
need those files. An unrelated local picture at the same path is never substituted.
Both players and the host/server must support the freetex pixel stream. Older builds
ignore its separate packet type. Pose and script-texture streams remain unchanged.
The same bus and approved variable-schema/name-mapping checks as scripttex apply.

## Content matching

Before pixels, the owner sends a small SHA-256 content reference. The identity includes
the SHA-256 of the source file and the dimensions and displayed RGBA pixels. Equal names
alone never authorize reuse. The displayed pixels also protect against the file being
rewritten while the GPU still shows an earlier version, or different texture settings.

The receiver checks its currently displayed images and the local image named by the
owner's synchronized freetex variable, even if its own bus is not displaying that file.
File reads, SHA calculation and decoding run on the worker. Matching references resolve
locally without image bytes on the wire. Missing or different content requests pixels;
it never requests the original file. A brief grace period accommodates names/variables
arriving after a content reference. Lost references and replies are retried.

Received images remain in memory and separate per-owner GPU textures. They are never
saved into the content directories and never overwrite the receiver's images. Remote
names cannot cause the lookup cache to open files outside the local content root.

## Source and destination

The view snapshots texture data already uploaded for the owner's selected freetex
slots, including updates from the reload worker and texture upgrades. Only these captured sources are published. A slot is identified by the canonical bus path,
section number and freetex variable; rear sections are included. Absolute directories
and generated filenames are not part of this identity.

Receivers allocate separate GPU textures per owner and slot. These never enter the
shared filesystem texture cache, never overwrite a content file, and are released
with that remote vehicle. Empty slots explicitly restore the original material.
Until initialization completes, the original material is shown. Stand-ins and
fallback vehicles are not subscribed. A replacement vehicle or reconnect starts a
new subscription, preventing stale packets from changing the replacement bus.

## Work and bandwidth

Snapshots are sampled at most ten times per second. Block decoding, RGBA copies,
delta generation, compression, decompression and packet assembly run on one worker.
The frame uses nonblocking, bounded channels. At most one complete image is uploaded
per frame. Same-size updates reuse their GPU allocation and materials; resize
updates rebind the surface.

The stream reuses the acknowledged script-texture protocol on packet magic `0xBD`:
lossless zlib-compressed initial pixels, then changed pixel runs relative to each
subscriber's acknowledged image. Unchanged images are not resent. Partial, reordered
or lost transactions are retried and never displayed as incomplete images.

Data packets have a 1,200-byte payload and at most 1,245 bytes in total. Eight data
packets per 50 ms are shared round-robin by subscribers: about 195 KiB/s of data
datagrams in total per publisher, plus small subscription/acknowledgement packets.
This is a ceiling; simple display changes normally use much less. Session
authentication, sender-address checks and the existing texture rate limiter also
apply when the host relays between clients.

The stream accepts up to 64 slots, 16 MiB of RGBA per image, and 64 MiB of local
decoded images or received baselines. Source snapshots and remote GPU surfaces have
separate 64 MiB budgets, as does the optional local lookup cache (at most 64 candidates).
File hashing is capped at 64 MiB per source. Oversized/unreadable source files disable
reference reuse; valid captured pixels can still be sent. Images beyond a pixel or
device budget are not uploaded. Local candidates are checked at most twice a second,
one lookup per worker iteration; idle unchanged images are not rehashed or resent.
