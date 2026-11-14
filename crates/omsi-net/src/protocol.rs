/// Version of the message format. Raised whenever a message changes shape.
/// 3: binary vehicle states, INFO, the host's world and clock, chat.
/// 4: the host's traffic, people and light programs (`world`), a nonce in the hello (a
/// client may try several addresses of the host at once), codes with several addresses.
/// 5: the tour a player drives in `INFO` (the host's timetable leaves it out), riders of
/// the players' buses in the world frames (`world::PLAYER_BUS`), and the players' people
/// passed on to the other players.
/// 6: up to 63 sound and moving-part values in a state (a 6-bit count: the AA-FR Agora's
/// sound variables alone filled the 31 there was room for).
/// 7: owner-authoritative RGBA script textures with acknowledged initialization and deltas.
/// Older peers cannot render remote script textures after local drawing is disabled.
/// 8: canonical full variable tables and versioned, fragmented strings.
/// 9: paged authoritative AI timetables and system script context.
/// 10: recoverable variable-name manifests and model/sound compatibility fallback.
/// 11: reliable, ordered owner-authored T.L and T.F sound events.
/// 12: timestamped complete model animation samples on the owner's pose clock.
/// 13: upstream 0.2.47 admission proofs and signed host identity for shared mods.
/// 14: session-bound variable/schema/sound packets and acknowledged long strings.
/// 15: the AI traffic as a table of rows under ids (`traffic_table`: `TRAF` datagrams the
/// client acknowledges with `TACK`), no longer cars in `WORLD` or `DESC|c|…`.
/// 16: `REALM`: the hardware identifier is made for each host owner, not the same everywhere.
/// 17: the timetable buses' fleet number, plate and depot file in `TRAF` (`meta numbers`): a
/// game of 16 drew the map's buses without numbers and with another depot's signs.
/// 18: vehicle transfer policy and descriptor preflight for multiplayer content.
pub const PROTOCOL: u32 = 18;
