# Server-owned fleet: reproducible regression plan (#370)

Branch: `test/server-fleet-auto-sync` in `isaacsa2/openOMSI`.

The unit tests simulate real launcher selection and a **local authenticated TCP mods server**. They do not claim to render a bus, contact a live VPS or prove performance.

## Fast offline checks

Run from the repository root:

```bash
node --experimental-vm-modules --test launcher/tests/server-fleet.test.mjs launcher/tests/mobile-fleet-consent.test.mjs
node --test launcher/tests/server-fleet-translations.test.mjs
cargo test -p omsi-lan selected_bus_request_is_rejected_without_an_authorized_fleet
node launcher/tests/drive-check.mjs
```

The existing PR-checks workflow picks up both new `.test.mjs` files via `launcher/tests/*.test.mjs`. The Drive test needs the headless Chrome described in `launcher/README.md`.

Automated coverage:
- Server-only vehicles appear without local content, but never get a fake 3D preview.
- Server paint names are authoritative; missing repaint previews are blocked.
- A model that is whitelisted but no longer has a loadable fleet entry is not offered.
- Refreshing the VPS catalogue repairs stale selections, paints and an open picker.
- Server-fleet v1 downloads require explicit consent; consent cannot authorize a bus outside its published fleet.
- The UI-only consent marker is stripped from serialized duty; only `allow_server_fleet` is passed.
- Previous server queries cannot overwrite a newer session.
- Legacy multiplayer sessions without fleet v1 still join using `LIST`, including with a nonempty vehicle list.
- A loopback authenticated server rejects unauthorized `LISTBUS` and leaves ordinary `LIST` available.
- The eight fleet messages have all 29 maintained translations and matching placeholders.

## Live end-to-end gate (before merge)

Use the **same PR commit** on VPS, launcher and game. Configure a nonempty `vehicles` list of loadable `Vehicles/.../*.bus` paths. Keep `accept_uploads = 0` to validate server-owned content independently of player uploads; test `share_maps = 0` and `1` separately. Back up client settings and do not overwrite the OMSI 2 installation.

1. **Absent bus:** provide an S21 with default look and multiple `.cti` repaints; verify no preview, select it, click *Join and drive*, explicitly approve, complete `LISTBUS`, verify hashes, render and drive all advertised paints.
2. **Missing repaint:** install only the base S21 locally; verify missing server paint has no local preview, gets downloaded on approved join, and original local files are unchanged.
3. **Mismatched versions:** change one local texture/script byte; verify the exact server version is mounted in the session rather than silently accepting an incompatible local version.
4. **Two players:** player A uses an official repaint and player B views it with uploads disabled. Verify `FLEETINFO` fingerprint matches the advertised player content and shows the real bus. Have B drive the same approved bus and confirm reuse of verified remote-store blobs.
5. **Security:** try non-whitelisted buses, invalid paths, withdrawn repaints, bad sizes/hashes, changed manifests mid-transfer and declined consent. Expect fail-closed outcomes; never ignore variable/animation-table mismatches.
6. **Stability/performance:** interrupt a download and reconnect; stress up to 64 concurrent joins with realistic assets. Record CPU/RAM, transfer rate, dropped sockets, and frame-time percentiles, especially after heavy paint loads.
7. **Local-only compatibility:** install the exact server model and every advertised repaint. Forward UDP and the status endpoint, but block the TCP mods port and disable the WS relay. Confirm that the local model matches the status catalogue's `content` fingerprint and joining still works. Change one dependency and repeat: it must require the verified server download and refuse when that download is unavailable. Repeat with `OMSI_NO_LAN_MODS` and verify the server advertises fleet version 0.
8. **Platforms:** verify Windows/Linux/macOS/Android picker layouts and languages as applicable. Native clients without a consent UI must not authorize selected fleet downloads.

Expected log clues: `--allow-server-fleet` only after explicit consent, selected requests use `LISTBUS`, remote official vehicles use `FLEETINFO` when fingerprints match, and any refused required file aborts the selected join. `table A, ours B` is a runtime variable/animation compatibility warning, **not** a SHA-256 file comparison.

## Related regression checks

- Storage budgets remain soft: 1 GB of local cache and 4 GB of client LAN downloads. Active games and fresh partial transfers may exceed them; idle maintenance brings eligible content back toward the limits. Textures default to 700 MB within the shared local budget.
- Start a game from the CLI, open Troubleshooting, and verify both manual and automatic cleanup are blocked until it exits. A crash must release its process-held lease.
- Host a game, close it, then clear client downloads. Retained server blobs must survive while unrelated downloads are reclaimed. An empty upload directory must not disable cleanup.
- Edit graphics, change a gameplay option, cancel, and verify only graphics revert. Close/crash without confirmation and verify unconfirmed graphics were never saved. Test both Back and Esc, and leave the confirmation open for 20 seconds.
- Burst joins from an older client must get an explicit retry rejection; new clients negotiate WAIT. One IP must not occupy the full waiting queue.

## Limitations

Automation does not replace testing real OMSI assets or a live multiplayer VPS. Recursive dependencies of complicated `.cti [item]` repaints, live `FLEETINFO` and renderer performance are not yet proven. Keep the PR in Draft until verified.
