# Optional onward passenger connections — experimental

MIT-licensed source reference: Mayconrib808/OpenOmsi-BBS 0.3.45-coop.15-2.2.1.8 (credit retained).

Implementation:
- `passenger_connections=0` by default; `passenger_transfer_percent=20` is inert until enabled.
- Uses the original passenger id and changes the task to walk to an onward stop (no new passenger spawn).
- Up to two transfers per person, 80m candidate radius, original bus excluded from immediate re-boarding.
- Only single-player is enabled. **LAN clients and servers are explicitly held at 0%.** This avoids Maycon's documented missing client-owned passenger authority handback. Multiplayer must be reviewed and implemented separately.
- Candidate walks require loaded, continuous ground sampled at up to 2 m intervals (maximum 40 samples), bounded height changes and a clear collision ray. A known due service or a compatible ready bus is required; absent service data fails closed. This is deliberately conservative, not a complete pavement route planner.
- The four new native/launcher interface strings are translated into the complete 29-language `app.yml` catalog, including the 0–100% connection chance label and help.
- Eligible destinations and services are checked before ground samples or collision rays. Routine connection outcomes log at debug level; a synthetic regression checks that stops without onward service cause no ground samples.
- Tests cover 0%, 20% and 100% chance, disabled RNG preservation, passenger identity, the two-transfer cap and the previous-vehicle exclusion. Real-map CPU and frame-time measurements remain required.
- Synthetic tests cover identity/type retention, two transfers, immediate reboarding exclusion, missing service/ground and the disabled path leaving RNG untouched. Native/launcher settings and host/join/server gates are tested separately.
- Before final review: test real maps (including overpasses, barriers and streamed tiles), Windows gameplay captures, low-end frame times/memory and all supported platforms. Multiplayer remains disabled; authority handback and synchronization are outside this PR.

The eligibility scan checks destinations and available services before it samples walkable terrain or performs collision tests, reducing terrain work at busy interchanges. Ordinary connection diagnostics run at debug level. No measured FPS, p99 frame-time or memory benchmark is available yet; a real-map benchmark at 100% is still required.\n\nThe original MIT copyright and permission notice are retained in [MAYCON-LICENSE.txt](MAYCON-LICENSE.txt). Source: https://github.com/Mayconrib808/OpenOmsi-BBS.
