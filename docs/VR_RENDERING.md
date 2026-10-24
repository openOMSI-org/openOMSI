# Stereo frame preparation

`Renderer::render_xr_stereo` draws both OpenXR eyes from one scene snapshot.
It applies pending instance/bounds uploads once, then traverses the scene's
bounding blocks once to collect candidates for both asymmetric eye frusta.
Object origins and radii are projected once per object per eye, rather than
once per mesh. Smaller culling leaves retain the same instance budget for
tightening dirty bounds; growing bounds remain conservative immediately.

The shared data lives only for this call. Moving or reshaping a mesh, recycling
an instance, or streaming a tile cannot leave a candidate cache from a previous
frame. Widely separated views that would require a render-origin change between
eyes use the existing independent preparation path.

Each eye retains its own visibility bits, object-size/LOD hysteresis and cached
render bundles. Final culling, draw-list offsets, camera uniforms and transparent
sorting remain independent. The desktop's history is restored after each eye.
The existing shared shadow atlas and D3D12/OpenXR queue ordering are preserved.
The first eye's reported CPU duration includes shared preparation.

## Traffic mesh detail

Shared AI vehicle instances opt into a one-pixel mesh cutoff in headset views.
The projected bounding sphere uses its nearest point and the eye's actual
projection and render height. A previously visible mesh has a 15% deadband.
This removes subpixel fittings without changing the authored whole-vehicle LOD
selection, player vehicle detail, simulation, desktop rendering or mirror detail.
Unlit, emissive and night-mapped meshes remain visible for their light and glow.
Pooled instances receive the policy after slot acquisition, including an explicit
reset for player instances. This is not a general AI model simplifier. Upstream's authored vehicle LOD loader and bands remain unchanged; both near and far instances receive the headset mesh policy, and final LOD selection stays per eye.

## Mirror refresh

VR refresh selection uses both last-submitted eye projections. Asymmetric FOVs
are widened conservatively by 15 degrees and mirror bounds by 35 cm to allow for
the age of the pose. Fixed cameras continue refreshing because their monitors
need not be positioned at the camera itself. Coupled vehicle cameras participate
in the same numbering and budget as their textures.

All mirror images initialize on a bus or XR session change. Newly visible
mirrors refresh immediately, even if the normal budget has not accrued a turn.
Regular visible turns and at most one aggregate hidden refresh per second use
the selected total redraw budget. A zero rate freezes existing images; the
separate Off setting retains its existing behavior. Desktop mirror scheduling
is unchanged. This uses a previous-frame pose, not late-latched visibility.

## Regression coverage

The renderer's explicit GPU tests compare paired rendering with independent
eye calls, including per-eye image readback, LOD/distance histories, scene and
origin changes, bundle ownership and AI mesh detail. These run without a headset.
Pure tests cover asymmetric mirror visibility, initialization, newly visible
mirrors, aggregate background refresh, redraw limits and zero-rate behavior.

The changes reduce repeated preparation and unnecessary work. They do not imply
a particular headset frame rate or establish the cause of a compositor error.

## GPU diagnostics and offline reproduction

`OMSI_GPU_TIMERS=1` now separates physical mirrors, desktop, left eye and right
eye into independent query/readback slots. Existing pass figures partition the
view's GPU timeline and include intervening untimed work. Additional `(interval)`
figures measure each pass's own begin/end timestamps. They can overlap on tile
GPUs and must not be summed as a frame time. `(all passes)` spans only the timed
scene passes; it excludes OpenXR UI, the compositor and runtime pacing. Eye draw
counts report visible meshes, AI meshes, batches and triangles separately.
As in the desktop counters, "blended draws" also includes meshes that must follow
the vehicle's authored draw order, even when some of their slots are opaque.
Physical mirror time per frame remains a sampling estimate; its denominator now
includes stereo pairs rather than relying on desktop renders that VR may omit.
Normal free-drive shutdown also writes the ready GPU summaries and draw counts
to the game log when GPU timers are enabled, without requiring a timed exit or
waiting for pending timestamp readbacks.

For repeatable local comparisons without an HMD, combine `--offscreen scene.png`
with `OMSI_BENCH=360`, `OMSI_BENCH_STEREO=1`, `OMSI_GPU_TIMERS=1` and
`OMSI_PROFILE=1`. `--size` is per eye. The synthetic pair has a symmetric 100-degree
vertical FOV and 64 mm IPD; it is not a reproduction of the runtime's asymmetric
projection, head motion or compositor scheduling. Both targets are rendered via
the production stereo entry point. Completion waits belong only to this existing
offline benchmark; the live OpenXR submission path gains no waits.
