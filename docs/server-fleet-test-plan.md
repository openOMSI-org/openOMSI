# Server-owned fleet: regression test plan (#370)

The test plan for server-owned fleets: buses a multiplayer server publishes and players download on join. It lists the offline checks, the live end-to-end gate to pass before merging, and related regression checks.

**Summary**

- Branch: `test/server-fleet-auto-sync` in `isaacsa2/openOMSI`.
- The automated tests simulate real launcher selection and a **local authenticated TCP mods server**. They do not render a bus, contact a live VPS or prove performance.
- Not yet proven: recursive dependencies of complicated `.cti [item]` repaints, live `FLEETINFO`, renderer performance. Keep the PR in Draft until the live gate passes.

## 1. Fast offline checks

Run from the repository root:

```bash
node --experimental-vm-modules --test launcher/tests/server-fleet.test.mjs launcher/tests/mobile-fleet-consent.test.mjs
node --test launcher/tests/server-fleet-translations.test.mjs
cargo test -p omsi-lan selected_bus_request_is_rejected_without_an_authorized_fleet
node launcher/tests/drive-check.mjs
```

- The PR-checks workflow picks up both new `.test.mjs` files through `launcher/tests/*.test.mjs`.
- The Drive test needs the headless Chrome described in `launcher/README.md`.

What the automated tests cover:

| Area | Checked behaviour |
|---|---|
| Server-only vehicles | Appear without local content, but never get a fake 3D preview |
| Paints | Server paint names are authoritative; missing repaint previews are blocked |
| Withdrawn models | A whitelisted model without a loadable fleet entry is not offered |
| Catalogue refresh | Refreshing the VPS catalogue repairs stale selections, paints and an open picker |
| Consent | Fleet v1 downloads need explicit consent; consent cannot authorize a bus outside the published fleet |
| Serialized duty | The UI-only consent marker is stripped; only `allow_server_fleet` is passed |
| Stale queries | An earlier server query cannot overwrite a newer session |
| Legacy servers | Sessions without fleet v1 still join with `LIST`, including with a non-empty vehicle list |
| Server auth | A loopback authenticated server rejects unauthorized `LISTBUS` and keeps ordinary `LIST` available |
| Translations | The eight fleet messages exist in all 29 maintained translations with matching placeholders |

## 2. Live end-to-end gate (before merge)

**Setup**

1. Use the **same PR commit** on the VPS, the launcher and the game.
2. Configure a non-empty `vehicles` list of loadable `Vehicles/.../*.bus` paths.
3. Keep `accept_uploads = 0`, to test server-owned content apart from player uploads.
4. Test `share_maps = 0` and `share_maps = 1` separately.
5. Back up client settings. Do not overwrite the OMSI 2 installation.

See [Dedicated server](SERVER.md) for the server settings.

**Tests**

| # | Test | Steps | Expected |
|---|---|---|---|
| 1 | Absent bus | Offer an S21 with the default look and several `.cti` repaints. Select it, click **Join and drive**, approve. | No preview before; `LISTBUS` completes; hashes verified; all advertised paints render and drive |
| 2 | Missing repaint | Install only the base S21 locally. | The missing server paint has no local preview, is downloaded on approved join; original local files unchanged |
| 3 | Mismatched version | Change one byte of a local texture or script. | The exact server version is mounted for the session; the incompatible local version is not silently accepted |
| 4 | Two players | Player A uses an official repaint; player B views it with uploads disabled. Then B drives the same approved bus. | `FLEETINFO` fingerprint matches the advertised content and shows the real bus; B reuses verified remote-store blobs |
| 5 | Security | Try non-whitelisted buses, invalid paths, withdrawn repaints, bad sizes/hashes, manifests changed mid-transfer, declined consent. | Every case fails closed; variable/animation-table mismatches are never ignored |
| 6 | Stability and performance | Interrupt a download and reconnect. Stress up to 64 concurrent joins with realistic assets. | Record CPU/RAM, transfer rate, dropped sockets and frame-time percentiles, especially after heavy paint loads |
| 7 | Local-only compatibility | Install the exact server model and every advertised repaint. Forward UDP and the status endpoint; block the TCP mods port; disable the WS relay. | The local model matches the status catalogue's `content` fingerprint and joining works |
| 7a | | Change one dependency and repeat. | Join requires the verified server download and refuses when it is unavailable |
| 7b | | Repeat with `OMSI_NO_LAN_MODS`. | The server advertises fleet version 0 |
| 8 | Platforms | Check picker layouts and languages on Windows, Linux, macOS and Android as applicable. | Native clients without a consent UI never authorize selected fleet downloads |

**Expected log clues**

- `--allow-server-fleet` appears only after explicit consent.
- Selected requests use `LISTBUS`.
- Remote official vehicles use `FLEETINFO` when fingerprints match.
- Any refused required file aborts the selected join.
- `table A, ours B` is a runtime variable/animation compatibility warning, **not** a SHA-256 file comparison.

## 3. Related regression checks

| Area | Check |
|---|---|
| Storage budgets | Budgets stay soft: 1 GB local cache, 4 GB client LAN downloads, textures 700 MB by default within the shared local budget. Active games and fresh partial transfers may exceed them; idle maintenance brings eligible content back toward the limits. |
| Cleanup lease | Start a game from the CLI, open **Troubleshooting**: manual and automatic cleanup are blocked until it exits. A crash must release its process-held lease. |
| Retained blobs | Host a game, close it, then clear client downloads. Retained server blobs survive; unrelated downloads are reclaimed. An empty upload directory must not disable cleanup. |
| Graphics confirmation | Edit graphics, change a gameplay option, cancel: only graphics revert. Close or crash without confirming: unconfirmed graphics were never saved. Test Back and Esc, and leave the confirmation open for 20 seconds. |
| Join bursts | Burst joins from an older client get an explicit retry rejection; new clients negotiate `WAIT`. One IP must not fill the whole waiting queue. |

## Limitations

Automation does not replace testing with real OMSI assets and a live multiplayer VPS. Recursive dependencies of complicated `.cti [item]` repaints, live `FLEETINFO` and renderer performance are not yet proven. Keep the PR in Draft until verified.
