# Virtual reality (Windows)

openOMSI can show the game in a PC VR headset through OpenXR. This page covers setting it up, the VR settings and keys, and placing the navigator in the headset. VR is off by default and is available on Windows only.

## Requirements

- Windows.
- A headset with an active OpenXR runtime (for example the one of SteamVR, Meta or Windows Mixed Reality).
- A graphics card with DirectX 12. With VR on, the game always uses DirectX 12, whatever **Graphics API** says.

You drive with the keyboard, mouse, a wheel or a game controller. Headset motion controllers are not used.

## Start a drive in VR

1. Connect the headset and start its OpenXR runtime.
2. In the launcher, open **Settings → Camera → Virtual reality** and switch on **Use OpenXR headset**.
3. Start a drive. The game opens in the headset when the session starts.

If the headset cannot be opened, the game runs on the monitor and `game.log` says `OpenXR could not start` and why. Press **F8** to try again; the screen then says `Could not start VR (...)` if it still fails.

The headset runtime controls its own reprojection. No runtime debug setting is needed.

## VR settings

These are in **Settings → Camera → Virtual reality** in the launcher, and in **Esc → Options → Camera** in the game (they take effect at the next start).

| Setting | Choices | What it does |
|---|---|---|
| Use OpenXR headset | On, Off | Starts drives in the headset. Default: Off. |
| Eye resolution | 50 %, 65 %, 80 %, 100 % | Share of the runtime's recommended resolution per eye. Lower is less GPU work. Default: 65 %. |
| Head tracking smoothing | Off, 5, 10, 20, 30 ms | Smooths the headset pose. Off uses raw tracking. Default: Off. |
| Bus mirror refresh | Off, 8 to 360/s, Every frame | Total redraws per second shared by all bus mirrors. Default: 16/s. |
| Show headset picture on monitor | On, Off | Copies the left eye to the desktop window. Default: On. |

The game's other graphics settings still apply. On demanding maps you may need to lower them.

**Upscaling in VR.** NVIDIA DLSS, AMD FSR 3 and Apple MetalFX are not used in VR. AMD FSR 1 works: see [FSR 1 in VR](#fsr-1-in-vr). For upscaling in general see [Graphics](GRAPHICS.md).

### Bus mirror refresh

The rate is a budget shared by all mirrors:

| Rate | With four mirrors |
|---|---|
| 16/s (default) | About 4 updates/s per mirror |
| 120/s | About 30 updates/s per mirror |
| 240/s | About 60 updates/s per mirror |
| Off | Mirrors show a frozen picture |
| Every frame | Every mirror is redrawn once per game frame |

Each mirror updates at most once per game frame. **Every frame** removes the budget, so mirrors follow the game's real frame rate. Headset reprojection does not add mirror updates. More mirror updates can lower the frame rate, especially on buses with many mirrors.

## Keys

The VR keys are at the top of the game list on the **Controls** page (search for `VR`). **Settings → Camera → Change the VR keys** opens it. They act only while a headset runs.

| Action | Default key |
|---|---|
| VR: Reset view | `Ctrl+Shift+R` |
| VR: Monitor preview | `F7` |
| VR: Switch VR / desktop | `F8` |
| VR: Toggle navigator | `Ctrl+Shift+N` |
| VR: Position navigator | `Ctrl+Shift+M` |

Outside VR, `Ctrl+Shift+M` stays the mirror panel editor's shortcut.

## Seat position

Adjust where you sit while driving, in **Esc → Options → Camera**:

- **Seat forward and back**
- **Seat height**
- **Seat left and right**
- **Head pitch**
- **Reset the seat position** puts the seat back where the bus has it.

The launcher has the same sliders under **Settings → Camera → Seat position**. Changes made there apply when the next drive starts.

## Pointer and menus

- **Esc** opens the menu in front of you in the headset.
- Move the mouse to point at cockpit controls. Left-click to use them.
- The pointer fades after ten seconds without mouse movement. It comes back in the centre when you move the mouse.
- While mouse steering is on, the cockpit pointer is hidden so the mouse can steer.
- **Right-click** toggles a smooth zoom; the next right-click returns to the normal view. The mouse still works while zoomed. (Outside VR, right-drag keeps its normal camera use.)
- The line-and-tour menu shows its details pane in VR too: time arrows, stops and **Start trip**.

## The VR navigator

The navigator can float in the headset, attached to the bus. It stays in place as you drive and look around.

- It is off by default for a bus without saved settings.
- **Ctrl+Shift+N** shows or hides it.
- **Shift+N** cycles through map, map with stop list, and off.

The navigator is an overlay: cockpit geometry does not hide it. Place it in a clear spot.

### Place it with the mouse

1. Press **Ctrl+Shift+M**, or open **Esc → Options → VR → Move and rotate with the mouse...**. This also shows the navigator.
2. Move and turn it:

   | Input | Action |
   |---|---|
   | Hold left mouse and move | Move |
   | Hold right mouse and move | Turn and tilt |
   | Shift + right mouse drag | Roll sideways |
   | Mouse wheel | Closer or farther |
   | Ctrl + mouse wheel | Larger or smaller |
   | **R** | Reset placement and size (visibility unchanged) |

3. Press **Esc** or **Enter** to save and finish.

While placing:

- Single-player pauses; multiplayer keeps running.
- The mouse moves the navigator instead of operating cockpit controls.
- Switching away from the window saves and ends placement.

### Place it with numbers

The **VR** page of **Esc → Options** (shown only in VR, during a drive) has:

| Setting | Range |
|---|---|
| Navigator | On, Off |
| Position right / left, forward / back, up / down | In cm |
| Display width | 12 to 65 cm |
| Display rotation, Display roll | −180° to 180° |
| Display tilt | −80° to 80° |
| Interface opacity | 30 % to 100 % (background) |
| Reset navigator position | Back to the default |

Placement and visibility are saved separately for each bus.

## FSR 1 in VR

Without FSR 1, the headset image itself is made at the **Eye resolution** and the runtime scales it up.

With **Upscaling: AMD FSR 1** on when VR starts:

- The headset image stays full-sized.
- Each eye is drawn at the eye resolution and scaled up by FSR 1.
- The picture is sharper at the same drawing cost, plus two FSR passes per eye and a larger image for the runtime.
- If FSR 1 is turned off during the VR session, the eyes keep that size and the standard upscaler takes over.

This has been tested offscreen, not yet on a headset. See [fsr1-experiment.md](fsr1-experiment.md).

## For developers

### Test switches

| Variable | What it does |
|---|---|
| `OMSI_OPENXR=1` | Starts in VR without the setting |
| `OMSI_OPENXR_MIRROR_RATE` | Overrides the saved mirror budget: `-1` every frame, `0` frozen, a positive total rate |
| `OMSI_OPENXR_SCALE` | Takes the place of the eye resolution once (with FSR 1: the share the eyes are drawn at) |

See [Environment variables](DEBUG_FLAGS.md) for the rest.

### Headset-relative cabin input

- The VR pointer keeps a mouse offset within the headset image. Drawing and picking use the same cropped eye ray, including pending mouse input. Surface hits supply depth only.
- Head movement does not turn a held dial or reset the mouse idle time. Cursor-warp events add no movement.
- The pointer hides after ten idle seconds and returns with mouse movement or a menu transition.
- A VR interruption releases held controls at their current values and stops automatic motion with its authored stop event. Desktop release, Escape, missed clicks and wheel notches keep the existing automatic endpoint behaviour.
- VR hover refresh also follows head motion; it may refresh the cabin surface ray every six frames.
- The line-and-tour menu's details pane uses the same 760-pixel width threshold as on the desktop.

Headset-free regressions cover ray alignment, pending input, idle visibility, precise cancellation, desktop auto movement and Escape. Mod-specific scripts and live headset or multiplayer behaviour are outside those tests.

### Enhanced+ eye render targets

- With Enhanced+ (ray tracing) on, headset eyes keep HDR colour and screen masks but leave out the unused desktop ray-reflection surfaces.
- The extra eye pipelines compile once when a VR session starts, not at desktop startup. Later VR sessions reuse them.
- Desktop views keep their own surfaces and upgrade cached targets when needed. Rain films use the eye attachment layout even after such an upgrade.
- Without Enhanced+, this changes neither pipelines nor rendering.

Native GPU regressions compare both eye images at 1x/4x MSAA and check rainy same-sized desktop/eye transitions. Per-eye history and stereo preparation remain separate changes. The FSR 1 eye scale and mip-bias path is kept; a native regression covers both eyes at 75 % scale with 1x/4x MSAA.

### GPU timers

With `OMSI_GPU_TIMERS=1`, physical mirrors, desktop, left eye and right eye have separate query and readback slots.

- The pass figures divide the view's GPU timeline and include untimed work in between.
- The `(interval)` figures measure each pass's own begin and end. They can overlap on tile GPUs; do not add them up as a frame time.
- `(all passes)` spans only the timed scene passes. It leaves out the OpenXR UI, the compositor and runtime pacing.
- Eye draw counts report visible meshes, batches and triangles separately. As on the desktop, "blended draws" also counts meshes that must follow the vehicle's authored draw order, even when some of their slots are opaque.
- Physical mirror time per frame is a sampling estimate. Its denominator includes stereo pairs, not desktop renders that VR may skip.
- A normal free-drive shutdown also writes the ready GPU summaries and draw counts to `game.log`. It needs no timed exit and does not wait for pending readbacks.

### Reproducing without a headset

For repeatable local comparisons, combine `--offscreen scene.png` with `OMSI_BENCH=360`, `OMSI_BENCH_STEREO=1`, `OMSI_GPU_TIMERS=1` and `OMSI_PROFILE=1`. `--size` is per eye.

- The synthetic pair has a symmetric 100° vertical field of view and a 64 mm IPD. It does not reproduce the runtime's asymmetric projection, head motion or compositor scheduling.
- Both targets use the production per-eye entry point. The benchmark does not need the separate shared-stereo/mirror change.
- Completion waits belong only to this offline benchmark. The live OpenXR submission path has no added waits.
