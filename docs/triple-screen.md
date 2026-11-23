# Native triple screen

Enable **Three screen projections** in the launcher's **Camera** settings, or
**Triple screen** in the game's **Display** settings. Geometry changes take effect
in game immediately; changing the spanning window requires restarting the game.
OpenXR takes priority when requested.

The renderer draws three independent, monoscopic camera views from the same
driver eye, with asymmetric frusta, per-panel visibility, depth, SSAO and
antialiasing. Exposure and the sun shadow atlas are shared. The HUD, navigator,
city map, menus and mirror overlays default to the centre screen, at its full
resolution. **HUD on centre screen** in the launcher and the game's Display
settings switches them to the whole window. Hitboxes move with the pictures;
world-space player name tags still follow their vehicles on any screen.
Mouse picking, cockpit controls and vehicle placement use the
projection of the panel under the pointer. Bus mirrors refresh on side panels
as well. Screenshots made with `--offscreen` use the same three projections.

## Calibration

Use three panels with the same visible dimensions and resolution, in a horizontal
row. Measure the following rather than tuning an artificially wide FOV:

| Setting | Meaning | Default |
| --- | --- | --- |
| Visible panel width | One panel's active width, excluding the frame | 600 mm |
| Eye distance | Perpendicular distance from the eye to the centre panel | 650 mm |
| Frame width at each join | Total hidden width of both adjacent frames | 0 mm |
| Left/right screen angle | Each side panel's inward angle from a flat row | 45° |
| Eye above screen centre | Eye height relative to the physical panel centre | 0 mm |

Panel height is derived from panel width and the viewport's pixel aspect ratio.
The side panels hinge beside the centre panel; separate left and right angles
allow an asymmetric rig. All panels have square pixels. A zero angle supports
a flat row; bezels hide the corresponding part of the scene rather than stretching
the visible picture.

The **Field of view** slider in the launcher's Camera tab or the Escape menu's
Camera page controls the centre screen's vertical FOV. **Default** uses the
physical measurements. An explicit FOV adjusts the common virtual eye distance
for all three panels, keeping their joins aligned. Changing the measured eye
distance resets this override. Single-screen FOV is stored separately. Temporary
mouse zoom applies to all three projections and to picking without overwriting
the saved calibration.

**Span three monitors at startup** finds three equal, adjacent OS monitors with
the same vertical position and creates a borderless window across them. It does
not require Surround/Eyefinity. With Surround/Eyefinity exposing one wide monitor,
use fullscreen on that monitor. Alternatively disable automatic spanning and
set the total window resolution yourself, e.g. `5760x1080` for three 1920x1080
panels. Mixed panel dimensions/resolutions and separate output windows are not
supported by this rig.

## Saved settings

The launcher and game share `~/.openbus/settings.cfg`:

```ini
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

Three views increase rendering cost. Render scale and the existing graphics
settings apply to each panel. Rain films refract the current panel's scene;
views share scratch targets, with each panel submitted before the next uses them.

## Verification

```text
cargo check --locked -p omsi-app -p omsi-launcher-core
cargo test --locked -p omsi-render triple::tests
cargo test --locked -p omsi-app -p omsi-launcher-core triple --lib
cargo test --locked -p omsi-app every_setting_is_on_exactly_one_tab --lib
cargo test --locked -p omsi-render gpu_renders_each_panel -- --ignored
```

The geometric tests check seams with camera pitch and roll, flat and angled rigs,
bezel gaps, vertical eye offset, reversed depth and odd window widths. The GPU
test checks different scene markers in each panel, centre/spanning/cleared HUD,
MSAA 1/4, render scale, current-panel rain refraction, odd window widths and
both vanilla and enhanced graphics. Physical monitor alignment and fullscreen
placement still need a check on the actual three-monitor setup.
