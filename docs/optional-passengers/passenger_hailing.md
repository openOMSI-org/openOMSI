# Visual passenger hailing (experimental)

MIT source credit: Mayconrib808/OpenOmsi-BBS, 0.3.45-coop.15-2.2.1.8.

- OFF by default, no hardcoded Brazil-only map list.
- Both new app.yml labels include all 29 upstream language keys; Portuguese variants retain their existing wording.
- Detects a matching approaching bus within 60 m; does not change actual boarding or AI decisions.
- Enhanced/procedural human rig only, with look and wrist/arm IK; original OMSI animation is untouched.
- No new network packets or assets; default-OFF path exits without scanning buses for passengers.
- Known limitation: mirrored remote people do not get a host-transmitted service intent, so hailing is local-only until networking is reviewed. Windows/macOS/Android gameplay, before/after captures, and low-end GPU/frame-time measurements remain required before integration.

The original MIT copyright and permission notice are retained in [MAYCON-LICENSE.txt](MAYCON-LICENSE.txt). Source: https://github.com/Mayconrib808/OpenOmsi-BBS.

Review: synthetic simulation tests cover approach filtering, out-of-service buses, unchanged boarding intent, procedural IK, finite poses and wrist timing. Settings are covered across native/launcher writers and launcher restart. With the option off, snapshots skip the bus search and the pose path skips gesture work. With the option on, the snapshot reuses its frame buffers, culls beyond 300 m, and does not search a second time for the chosen bus. This is source-level performance evidence, not an FPS benchmark.

The gesture is limited to passengers still waiting at their stop, not those walking to the bus door.
