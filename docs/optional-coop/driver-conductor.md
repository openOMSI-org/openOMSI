# Optional co-op roles: driver handover and conductor (preparation)

Status: **authorization policy only, not gameplay-ready**. No UI, handshake, or
network command invokes these permissions yet; both roles remain OFF.

## Source reference and credit

The feature design was informed by the older
[Mayconrib808/OpenOmsi-BBS](https://github.com/Mayconrib808/OpenOmsi-BBS)
branch `source/openomsi-0.3.45-coop15-2.2.1.8`:

- `crates/omsi-app/src/coop_drive.rs`: visitor authority, vehicle snapshots,
  freeze/unfreeze, return and recovery
- `crates/omsi-app/src/coop_view.rs`: passenger view, picking rays and
  stop-bell permissions
- `crates/omsi-lan/src/coop.rs`: fare desk IDs, conductor actions and grants
- `crates/omsi-lan/src/vehicle_transfer.rs`: live vehicle snapshot/message format

The present codebase has changed substantially since 0.3.45. Do **not** copy
or enable the old transport or snapshot format without a security and
compatibility review.

## Proposed activation

Two independent **disabled by default** options, initially in the LAN host
and local player preferences:

1. `coop_driver_handover = false`
2. `coop_conductor = false`

No feature is active merely because a newer client connects. Both host and
vehicle owner must allow each action, and the visitor must explicitly accept
the role. Old clients must ignore an unknown opt-in extension, not be dropped.

## Driver handover

- Host authenticates the real sender, both peers and the target bus.
- Require a bus speed <= 0.5 km/h, a guest genuinely aboard, consent from
  both players, and matching content installed on both peers.
- Transfer **authority**, not vehicle files. Respect the existing private-mod,
  `share_maps` and `allow_file_transfer` policies; never send a private
  `.bus`, texture, or map simply to support a handover.
- There must be only one script/physics/control authority at a time.
- Reject stale tokens, wrong owner IDs and replays. On disconnect, timeout,
  game end or rejected transfer, restore the previous owner safely.
- Do not import the old raw vehicle snapshot wholesale: it contains script
  state, program memory and potentially large script textures. Define bounded,
  versioned fields and apply only what the current engine can validate.

## Conductor and tickets

- The role is separate from the driver; the host authenticates the
  designated actor, the bus footprint, owner consent and a per-role epoch.
- Only the current cash desk's allowed ticket/change commands may be
  performed. Reject replayed sequence numbers, stale passenger IDs and
  clicks from outside the bus.
- No implicit engine, steering, arbitrary script variable or door authority.
  Stop-bell actions remain independently permitted by the existing passenger
  rules.
- Ticket money, fare and current passenger remain authoritative with the
  owning simulation. Sending a UI click must not invent revenue.

## Next integration stages

1. **Driver:** introduce a small versioned LAN capability and authenticated
   request/approve/revoke messages; test old/new client negotiation.
2. **Driver:** add takeover/return state machine in `omsi-app`, with crash,
   ownership, sync and disconnect tests; support multiple vehicle types and
   trailer articulation without duplicating simulation.
3. **Conductor:** add bounded fare transaction messages and host-side action
   validation; hook into the existing passenger and ticket systems.
4. **UI:** optional driver handover and conductor approval controls, with
   the passenger camera ray mapped to the vehicle pose used for rendering.
5. **Tests:** unit, LAN two-client integration, dropped packets, replay,
   wrong-bus access, offline/single-player unchanged, old client compatibility,
   performance and anti-leak checks.

## Policy component in this PR

`omsi_lan::coop_permissions` defines small pure approval functions that do
not change game state: `may_handover_driver` and `FareGrant::authorize`.
Unit tests cover disabled defaults, driver stop/consent/content prerequisites,
and conductor role scope, passenger identity, desk ID and replay protection.

**This is a preparatory change, not a complete driver swap or ticketing implementation.**
