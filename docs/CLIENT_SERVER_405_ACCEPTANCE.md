# Unified client + native server acceptance — #405

This is an **experimental test branch**, not a production release. It combines the native dedicated server (#405), the client fixes (#408), and official upstream including #4111. PR #412 was used to resolve shared-code changes. Do not treat automated tests as equivalent to two real graphical clients.

## Automated gates

- [ ] Current-head PR checks: rustfmt, file/function size budgets, crate layering, settings/translation validation
- [ ] Current-head security: CodeQL, secret scan, audit
- [ ] Current-head headless regression suite, including private transfer policy and passenger CLAIM/BOARD/RETURN
- [ ] Current-head Release: Windows workspace tests and x64 executable, Linux x64, macOS arm64, Android build
- [ ] Investigate any failure on the exact current commit; do not soften CI thresholds or silence regression tests
- [ ] Ensure fork `main` remains aligned to official upstream before release approval

## Licensed map and two-client acceptance

1. Use **two graphical clients from the exact same build** and the native CPU-only server with a licensed local OMSI map. Confirm joining, driving, interior view, doors, route/IBIS/HOF, AI and timetable, and disconnect/reconnect.
2. Verify independent buses on the **same line, direction and stop**: separate passenger CLAIMs, physical boarding, seat occupancy, PAXBOARD acknowledgement, PAXRETURN on skipped/abandoned boardings, and no ghost or duplicate passengers. Repeat with one bus full, one bus arriving late, and both buses leaving a stop.
3. Set `pax_board_when_stopped=0`: boarding must retain original behavior. Set it to `1`: a moving bus or a bus with every entry door shut must not draw waiting passengers toward it; a stopped bus with an open entry door must. Repeat on a bus with left-side entry doors and on a modded bus.
4. On the **other** client's bus, ensure remote seated passengers maintain seated poses in original/enhanced animations; no repeated transitions after rejoining.
5. Independently toggle `pax_bad_driving_alight` and ambient passenger conversations. The original behavior must remain the default where specified; avoid duplicated voice playback on LAN mirrors.
6. Select two HOF/depot entries with overlapping names: an exact match wins, while fuzzy fallback and custom destination text remain functional.
7. Test native AI, people and scheduled journeys with a player driving. Observe stable vehicle identity, position, doors, passenger counts, spawn/despawn and route continuity across distance.
8. Test /reload, /restart, /speed, /traffic, /passengers and client reconnection using the documented supported modes. A command must not claim success without changing simulation state.
9. With a date-specific Chrono folder and `ailists` change, cross 23:59 -> 00:00. Verify the new date's tile patches, AI lists and timetable load, while the LAN session and players remain connected. Repeat after a date jump and a failed reload.
10. Explicitly disable private-content transfer (`accept_uploads = 0`, `share_maps = 0`, `allow_file_transfer = 0`, where supported). Confirm a second client cannot receive proprietary maps/vehicles/scripts via the test server. Test permission denial and reconnect.
11. Test a player-hosted LAN session separately. Its protocol and graphical host behavior must not depend on the dedicated server's headless rendering path.

## Experimental conductor cabin controls and stable passenger visibility

- [ ] On the owner's bus, Ctrl+Alt+K selects/revokes the **sole authenticated player currently aboard** as interim assistant, not every passenger. If two or more players are aboard, no assistant is selected. Permissions are off by default and revoked on the owner's disconnection/reconnect.
- [ ] From another client's bus, use the appointed assistant to click model doors, windows, lights, wipers, climate, IBIS and fare controls. Doors and boarding aids require a stopped bus. Grab and release a compatible window or dial to apply one bounded adjustment. Test missing and mismatched model events and normal passenger stop requests.
- [ ] In manual boarding mode, verify a waiting fare on the owner's bus. Try a supported ticket printer, or press **T** to issue the requested ticket, **Ctrl+T** to give change to an empty change tray, and **Shift+T** to collect the passenger's payment. The owner's existing fare simulation determines the ticket and amount. Verify a second sale can start, and that ordinary passengers and moving-bus shortcut requests are rejected.
- [ ] As the assistant aboard the owner's bus, press **Ctrl+Alt+J** to turn on **cabin keyboard mode**. Movement keys now operate the owner's bus bindings instead of walking. Press Ctrl+Alt+J again (or Escape) to return to walking. The owner must first appoint this player with Ctrl+Alt+K.
- [ ] Verify the owner's `Inputs/keyboard.cfg` determines the vehicle actions, **not** the visitor's differently mapped key bindings. Test a regular saloon-light, wiper, indicator, custom cockpit switch, door button and a ticket-machine binding.
- [ ] Try the same keys without consent, with another rider, outside the bus and after revocation or reconnect. Every cabin request must be rejected. Doors, boarding aids and fares must be rejected while moving; light, climate and display controls may continue at speed.
- [ ] Confirm that neither mouse controls nor remote keyboard events can touch steering, throttle, brakes, gears, ignition or engine controls. Only the driver's client runs authorized model actions; requests contain bounded key codes rather than peer-chosen script trigger names.
- [ ] With the driver's explicit grant and the bus fully stopped, have the conductor press **Ctrl+Alt+B** while aboard. The driver must hear a short two-tone departure bell and see the request to check clearance and close the doors. The conductor should hear the confirmed chime too. The bell must not physically close any door or use the vehicle horn.
- [ ] Repeat **Ctrl+Alt+B** with a moving bus, an unassigned player, a revoked role, while outside the bus, and immediately twice within 1.5 seconds. Verify rejection/cooldown without playing a sound. Check that a bus with no model-defined gong still gets the generated chime.
- [ ] The conductor operates the fare keys and model ticket machine remotely, but the sale, current passenger, banknotes, coins, physical ticket and desk visuals remain authoritative on the owner's client. Check that no local assistant-side sale is fabricated.
- [ ] Confirm that the assistant's controls leave steering, throttle, brakes, gears and engine controls with the driver.
- [ ] In a busy district with over 96 pedestrians within the observer's interest radius, verify the closest people remain visible when internal population order changes, with no repeated GONE/spawn flicker for stationary people. Legitimate distance-based GONE is still required.
- [ ] Reorder and duplicate network WORLD packets so an older pedestrian GONE arrives after a newer position. Verify it is ignored on both native and relayed-client mirrors.
- [ ] Check a busy same-line passenger stop using two separate buses: GRANT, physical BOARD/RETURN, escrow, and on-board figures should not disappear or transfer between buses without a real event.

## Real-time clock, METAR and multiplayer Weather UI (2026-11-21)

- [ ] Use a **full process restart**, not `/reload`, to change `real_time`, `metar_sync` or `metar_station` on the native server. These are restart-only settings; `/reload` must preserve existing settings and explicitly reject them. With `metar_sync=1`, specify a four-letter `metar_station`; do not guess the nearest airport in headless mode.
- [ ] Confirm `real_time=1` matches the server machine's local date/time, including crossing midnight and an operator clock offset, without independent client clocks overriding the session.
- [ ] Confirm `metar_sync=1` reports a non-blocking, host-authoritative METAR that is refreshed and broadcast to every client. During a service outage retain the last valid weather; do not freeze the simulation tick or silently replace weather with clear skies.
- [ ] On a LAN **client**, World > Weather and Time show the host's current read-only conditions and clock. Visual precipitation, sky, wet roads and temperature should track the same weather state after the network update. Only the host/admin may edit session weather.
- [ ] Test both features OFF, each ON independently, and both ON; reconnect a late client and verify new snapshots without protocol changes.
- [ ] Remote physical *click* routing and single-assistant ID checks are integrated but still require live testing; explicit two-party acceptance, continuous held-switch gestures, mutual driver handover and a remote-server GPS countdown remain unfinished. No passenger privilege is granted merely by boarding.

## Exclusive co-op and GPS functionality required for completion

- [ ] Confirm the new ray/picking path matches real `[mouseevent]` bus switches and only emits supported cabin controls and typed fare requests. Compare click and bounded drag behavior on several bus models.
- [ ] An authenticated driver explicitly appoints **exactly one consenting assistant/conductor** to the bus. Ordinary passengers retain only normal passenger actions (e.g. a stop request); no cabin access just for being inside. Revoke on exit, switch, reconnect, disconnect and timeout.
- [ ] Allow driver and assistant to exchange roles only when stopped, with a two-party confirmation, single vehicle-authority epoch/token and recovery on connection loss; no private vehicle/mod transfer.
- [ ] Verify the new GPS 'Green in Ns' indicator using local `TrafficSim` for a deterministic red signal on a single unambiguous lane, and that it stays absent for actuated signals, divergent junctions and missing light state. Extend the dedicated server's authoritative light phases to joined clients safely before claiming full multiplayer support.

## GPU/memory and native load evidence

- Drive a large map for an extended session, change views/settings, return to unloaded areas and repeat on a low-VRAM GPU. Measure missing textures, mip restoration, load time, VRAM/RAM, peak frame times and unusual repeated uploads.
- Compare `OMSI_TEXTURE_RAM_CACHE_MB=0`, `128` and `256` (desktop only). Report actual resident RAM, VRAM, p95/p99 frame time and client crashes.
- Use the native timings scripts in `scripts/server-headless/` and `docs/DEDICATED_SERVER_TIMINGS.md`. Capture true TPS, stage p95/p99, server CPU, RSS and network data with reproducible client/player counts and identical map configuration.
- Test escalating traffic and people counts. The 512 AI-car / 2,000-person configuration values are ceilings, **not verified sustained loads**. Never report simulated or historical maxima as measurements from this branch.
- Compare with the latest upstream baseline, with the same licensed map and settings, and capture the commit SHA and system information.

## Known limits and decision

- Co-op **driver handover** and mutual consent remain open. Cabin switches and delegated fare actions need validation with two real clients. Cash and ticket desk visuals are still local to the driver. Continuous held-switch gestures remain open.
- Native authority over player-bus physics/scripts, full passenger capacity, modded bus boarding, rollback-safe streaming, and mixed-version client compatibility still need independent sign-off.
- Complete real-client and security acceptance before proposing merge to the fork or official main; screenshots only when they materially clarify a visual regression.
