# Virtual reality (Windows)

openOMSI can render through an OpenXR headset on Windows. VR is off by default. With VR off,
the normal desktop controls and rendering stay available.

## Requirements

- Windows and an OpenXR headset with an active OpenXR runtime.
- A DirectX 12 capable graphics adapter.

No runtime debug tool setting is needed. The headset runtime controls its own reprojection
settings.

## Start and settings

1. Connect the headset.
2. Start its OpenXR runtime before you launch a drive.
3. In the launcher, open **Settings → Camera → Virtual reality**.
4. Enable **Use OpenXR headset**.
5. Start a drive. The game starts in the headset when the session opens.

| Setting | Values | Default | What it does |
| --- | --- | --- | --- |
| Eye resolution | 50%, 65%, 80%, 100% | 65% | Scales the runtime's recommended resolution for each eye. Lower values reduce GPU work. |
| Head tracking smoothing | Off, 5, 10, 20, 30 ms | Off | Smooths the headset pose. Off uses raw tracking. |
| Bus mirror refresh | Off, 8, 16, 24, 32, 48, 60, 90, 120, 180, 240, 360/s, Every frame | 16/s | Total redraw budget shared by all bus mirrors. Off freezes their picture. Every frame redraws every mirror once per game frame. |
| Show headset picture on monitor | On, Off | On | Copies the left eye to the desktop window. |

These settings affect VR only. The other graphics settings still apply; lower them on
demanding maps if needed.

### Bus mirror refresh

- The budget is a total for all mirrors. With four mirrors, 120/s gives about 30 updates/s
  per mirror, and 240/s about 60 per mirror.
- Each mirror updates at most once per game frame.
- **Every frame** removes the budget: mirrors follow the game's actual frame rate. Headset
  reprojection does not add mirror updates.
- More frequent mirror rendering can lower game FPS, especially on buses with many mirrors.
- For testing, `OMSI_OPENXR_MIRROR_RATE` overrides the saved budget: `-1` for Every frame,
  `0` to freeze, or a positive total redraw rate.

## Controls

Edit the VR key bindings under **Controls → Keyboard** (search for `VR`).

| Control | Action |
| --- | --- |
| Ctrl+Shift+R | Reset the VR view |
| F7 | Toggle the monitor preview |
| F8 | Switch between VR and desktop |
| Ctrl+Shift+N | Show or hide the VR navigator |
| Shift+N | Cycle the navigator: map, map with stop list, off |
| Ctrl+Shift+M | Position the VR navigator |
| Esc | Open the menu in front of the headset |
| Mouse | Point at cockpit controls |
| Left-click | Use the cockpit control under the pointer |
| Right-click | Toggle a smooth zoom (VR only). The mouse stays usable while zoomed. |
| Right-drag (outside VR) | Normal camera control |

- The pointer fades after ten seconds without mouse movement. It comes back in the centre
  when you move the mouse.
- When mouse steering is active, the cockpit pointer is hidden so the mouse can steer the bus.

### Seat position

Adjust the seat while driving under **Esc → Options → Camera**: **Seat forward and back**,
**Seat height**, **Seat left and right** and **Head pitch**. **Reset the seat position** puts
the seat back where the bus has it.

The launcher has the same sliders under **Settings → Camera**. Those changes apply when the
next drive starts.

## VR navigator

- The navigator is off by default for buses without saved settings.
- It stays attached to the bus as you drive and look around.
- It is an overlay: cockpit geometry does not hide it, so place it in a clear spot.
- The navigator controls and the placement menu are available only in VR.

To place it, press **Ctrl+Shift+M** or open **Esc → Options → VR → Move and rotate with the
mouse...**. Opening placement mode also shows the navigator.

| Control (placement mode) | Action |
| --- | --- |
| Hold left mouse and move | Move the display |
| Hold right mouse and move | Turn and tilt the display |
| Shift + right mouse drag | Roll the display sideways |
| Mouse wheel | Move closer or farther away |
| Ctrl + mouse wheel | Make the display larger or smaller |
| R | Reset placement and size, keeping visibility unchanged |
| Esc or Enter | Save and finish |

- The placement menu also has numeric position, width, rotation, tilt, roll and background
  opacity settings.
- Placement and visibility are saved separately for each bus.
- Single-player pauses during placement. Multiplayer keeps running.
- During placement the mouse moves the navigator, not cockpit controls.
- Losing window focus saves and ends placement.

## Timetable controls

In VR the line-and-tour menu shows its details pane (time arrows, stops, Start trip). It uses
the same 760-pixel width threshold as the desktop.

## Troubleshooting

- **The game does not start in the headset:** start the OpenXR runtime before the drive, and
  check that **Use OpenXR headset** is on. A DirectX 12 capable graphics adapter is required.
- **Low frame rate:** lower **Eye resolution** or **Bus mirror refresh**, or the game's other
  graphics settings.
- **No pointer:** move the mouse. With mouse steering active, the pointer stays hidden.
- **View off centre:** press **Ctrl+Shift+R**.

## Technical notes

### Headset-relative cabin input

- The VR pointer keeps a mouse offset within the headset image.
- Drawing and picking use the same cropped eye ray, including pending mouse input. Surface
  hits supply depth only.
- Head movement does not turn a held dial or reset mouse idle time.
- The pointer hides after ten idle seconds. It returns with mouse movement or a menu
  transition. Cursor-warp events add no movement.
- A VR interruption releases held controls at their current values. It stops automatic motion
  with its authored stop event.
- Desktop release, Escape, missed clicks and wheel notches keep the existing automatic
  endpoint behaviour.
- VR hover refresh also follows head motion. It may refresh the cabin surface ray every six
  frames.
- Headset-free regression tests cover ray alignment, pending input, idle visibility, precise
  cancellation, desktop auto movement and Escape. They do not cover mod-specific scripts or
  live headset and multiplayer behaviour.

### Enhanced+ headset render targets

- With Enhanced+ (ray tracing) on, headset eyes keep HDR colour and screen masks. They omit
  the unused desktop ray-reflection surfaces.
- The extra eye pipelines compile once when a VR session starts, not at desktop startup.
  Later VR sessions reuse them.
- Desktop views keep their original surfaces and upgrade cached targets when needed. Rain
  films use the eye attachment layout even after such an upgrade.
- Without Enhanced+, this changes neither pipelines nor rendering.
- Native GPU regression tests compare both eye images at 1x/4x MSAA. They also check rainy
  same-sized desktop/eye transitions.
- The upstream FSR 1 eye scale and mip-bias path is kept. A native regression test covers
  both eyes at 75% scale with 1x/4x MSAA.

### GPU diagnostics and offline reproduction

`OMSI_GPU_TIMERS=1` gives physical mirrors, desktop, left eye and right eye their own
query/readback slots.

- Pass figures partition the view's GPU timeline and include untimed work in between.
- `(interval)` figures measure each pass's own begin/end timestamps. They can overlap on tile
  GPUs: do not add them up as a frame time.
- `(all passes)` spans only the timed scene passes. It excludes OpenXR UI, the compositor and
  runtime pacing.
- Eye draw counts report visible meshes, batches and triangles separately.
- As in the desktop counters, "blended draws" also counts meshes that must follow the
  vehicle's authored draw order, even when some of their slots are opaque.
- Physical mirror time per frame is a sampling estimate. Its denominator includes stereo
  pairs, not desktop renders that VR may skip.
- With GPU timers on, a normal free-drive shutdown writes the ready GPU summaries and draw
  counts to the game log. It needs no timed exit and does not wait for pending timestamp
  readbacks.

For repeatable local comparisons without an HMD, combine `--offscreen scene.png` with
`OMSI_BENCH=360`, `OMSI_BENCH_STEREO=1`, `OMSI_GPU_TIMERS=1` and `OMSI_PROFILE=1`.

- `--size` is per eye.
- The synthetic pair has a symmetric 100-degree vertical FOV and 64 mm IPD. It does not
  reproduce the runtime's asymmetric projection, head motion or compositor scheduling.
- Both targets use the production per-eye entry point.
- Completion waits exist only in this offline benchmark. The live OpenXR submission path
  gets no waits.
