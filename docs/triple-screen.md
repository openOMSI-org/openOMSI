# Triple screen

openOMSI can draw three correct perspective views for three monitors in a row, like a real cab's windscreen and side windows. This page explains how to switch it on and how to measure your rig so the picture lines up across the screens.

## What it does

- Three separate camera views from the same driver's eye, one per panel, each with its own correct perspective.
- The HUD, navigator, city map, menus and mirror panels sit on the centre screen at its full resolution (or across the whole window, if you prefer).
- Mouse picking, cockpit controls and vehicle placement use the panel under the pointer.
- Bus mirrors refresh on the side panels too. Other players' name tags follow their vehicles on any screen.
- Screenshots made with `--offscreen` use the same three views.

When a VR headset is in use, VR takes priority and triple screen is ignored.

## Requirements

- Three monitors with the **same visible size and resolution**, side by side in a horizontal row.
- Mixed panel sizes or resolutions and separate windows per monitor are not supported.

## Switch it on

1. In the launcher, open **Settings → Camera → Triple screen** and switch on **Three screen projections**.
   In the game, the same switch is **Esc → Options → Display and memory → Triple screen**.
2. Leave **Span three monitors at startup** on. At the next start, the game finds three equal, adjacent monitors at the same height and opens one borderless window across them. NVIDIA Surround or AMD Eyefinity is not needed.
3. Measure your rig and enter the values (next section).
4. Restart the game if you changed the spanning switch. Geometry changes apply in the game at once.

**With Surround or Eyefinity** (one wide monitor for the system): switch off **Span three monitors at startup** and use fullscreen on that monitor.

**Without spanning:** switch it off and set the total window size yourself, for example `5760x1080` for three 1920x1080 panels.

## Calibrate

Measure your setup with a tape measure. Do not try to fix the picture with an extra-wide field of view.

| Launcher label (game label) | What to measure | Range | Default |
|---|---|---|---|
| Visible width of one panel (Visible panel width) | One panel's picture width, without the frame | 200–2000 mm | 600 mm |
| Eye to centre screen (Eye distance) | Straight distance from your eye to the centre panel | 200–3000 mm | 650 mm |
| Both frames at each join (Frame width at each join) | Total hidden width of the two frames where two panels meet | 0–100 mm | 0 mm |
| Left / Right screen inward angle (Left / Right screen angle) | How far each side panel turns in from a flat row | 0–90° | 45° |
| Eye above screen centre | Eye height above the centre of the panels (negative: below) | −500–500 mm | 0 mm |

Notes:

- Panel height comes from the panel width and the window's pixel aspect ratio. All panels have square pixels.
- The side panels hinge beside the centre panel. Separate left and right angles allow an asymmetric rig. An angle of 0° is a flat row.
- Frames hide the matching part of the scene; the visible picture is not stretched.

### Field of view

The **Field of view** slider (launcher: **Settings → Camera**; game: **Esc → Options → Camera**) sets the centre screen's vertical field of view.

- **Default** uses your physical measurements. This is the correct setting for a measured rig.
- A set value moves the virtual eye distance for all three panels, keeping their joins aligned.
- Changing **Eye to centre screen** resets this override to Default.
- The single-screen field of view is stored separately, so switching triple screen off brings yours back.
- Temporary mouse zoom applies to all three views and to picking without changing the saved calibration.

## HUD on centre screen

**HUD on centre screen** (launcher and **Esc → Options → Display and memory**) is on by default. It keeps the HUD, navigator, menus and information on the centre panel. Switch it off to spread them over the whole window.

## Performance

Three views cost more to draw than one.

- Render scale and the other graphics settings apply to each panel.
- AMD FSR 1 works on each panel. NVIDIA DLSS, AMD FSR 3 and Apple MetalFX are not used with triple screen. See [Graphics](GRAPHICS.md).
- Exposure and the sun's shadow map are shared between the panels.

## Saved settings

The launcher and the game share `settings.cfg` in `~/.openomsi` (on Windows in your user folder):

```text
triple_screen=1
triple_span=1
triple_hud_center=1
triple_fov_deg=0
triple_width_mm=600
triple_distance_mm=650
triple_bezel_mm=0
triple_left_angle_deg=45
triple_right_angle_deg=45
triple_eye_height_mm=0
```

`triple_fov_deg=0` means Default (from the measurements).

## For developers

Each panel has its own asymmetric frustum, visibility, depth, SSAO and anti-aliasing. Rain films refract the current panel's scene. The views share scratch targets; each panel is submitted before the next uses them.

Checks:

```text
cargo check --locked -p omsi-app -p omsi-launcher-core
cargo test --locked -p omsi-render triple::tests
cargo test --locked -p omsi-app -p omsi-launcher-core triple --lib
cargo test --locked -p omsi-app every_setting_is_on_exactly_one_tab --lib
cargo test --locked -p omsi-render gpu_renders_each_panel -- --ignored
```

| Test | Covers |
|---|---|
| Geometry tests | Seams with camera pitch and roll, flat and angled rigs, bezel gaps, vertical eye offset, reversed depth, odd window widths |
| GPU test | Different scene markers per panel; centre, spanning and cleared HUD; MSAA 1/4; render scale; current-panel rain refraction; odd window widths; vanilla and enhanced graphics |

Physical monitor alignment and fullscreen placement still need a check on a real three-monitor setup.
