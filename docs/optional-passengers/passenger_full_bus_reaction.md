# Full-bus reaction (experimental)

MIT source reference: Mayconrib808/OpenOmsi-BBS, 0.3.45-coop.15-2.2.1.8.

- OFF by default and independent of opt-in hailing; original OMSI animation untouched.
- Counts usable seated AND standing [passpos] locations, excluding script-disabled places; no false full state for a zero-capacity/unknown model.
- Uses a short 1.4-second visual reaction, then suppresses repetition for that bus until it has capacity again (a bounded cache remembers eight recent buses). Temporary loss of the visual target does not restart it.
- Only affects procedural animation; boarding decisions and route tasks remain authoritative and unchanged.
- Multiplayer visuals, Windows gameplay captures and low-end GPU/frame-time measurements still require review. Real-map visual and performance validation still pending.

The original MIT copyright and permission notice are retained in [MAYCON-LICENSE.txt](MAYCON-LICENSE.txt). Source: https://github.com/Mayconrib808/OpenOmsi-BBS.

Review: synthetic tests cover seated/standing capacity, disabled places, unknown capacity, repeated reactions and native/launcher settings persistence. With the option off, no capacity/occupancy scan or temporary per-person buffer is built, and the pose path skips gesture work. Runtime FPS and memory measurements are not available; no performance claim is made.

The full-bus response is a brief glance towards the approaching bus, with arms lowered; it no longer resembles a request to stop. It is limited to passengers still waiting, not those walking to the door. This remains experimental pending gameplay captures.

Integration: stacked on the latest #3427 code, reusing the existing hailing helpers instead of duplicating them. With neither option enabled bus matching is skipped. Hailing alone raises the arm only for buses with room, reaction alone gives a short glance at a full bus with arms down, and both together smoothly lower a raised arm at capacity. The per-snapshot capacity cache is reused, and the expensive occupancy scan only runs for actually targeted buses whose reservations do not already prove fullness. Settings and simulated inputs test all four combinations.
