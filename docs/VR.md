# Virtual reality (Windows)

openOMSI can render through an OpenXR headset on Windows. An active OpenXR runtime
and a DirectX 12 capable graphics adapter are required. VR is off by default.
The normal desktop controls and rendering remain available when VR is off.

## Start and settings

Connect the headset and start its OpenXR runtime before launching a drive. In the
launcher, open **Settings → Camera → Virtual reality** and enable **Use OpenXR headset**.
The game starts in the headset when the session opens.

| Setting | Choices | Effect |
| --- | --- | --- |
| Eye resolution | 50%, 65%, 80%, 100% | Scale of the runtime's recommended resolution for each eye. Lower values reduce GPU work. |
| Head tracking smoothing | Off, 5, 10, 20, 30 ms | Smooth the headset pose; Off uses raw tracking. |
| Bus mirror refresh | Off, 8, 16, 24, 32, 48, 60, 90, 120, 180, 240, 360/s, Every frame | Total redraw budget shared by all bus mirrors. Off freezes their picture; Every frame redraws every mirror once per game frame. |
| Show headset picture on monitor | On or off | Copy the left eye to the desktop window. |

These settings affect VR only. The game's other graphics settings still apply
and may need adjusting on demanding maps.

The default mirror budget is 16 redraws/s in total. With four mirrors, 120/s
targets about 30 updates/s per mirror, while 240/s targets about 60 updates/s
per mirror. Each mirror can update at most once per game frame. **Every frame**
removes the redraw budget, so mirrors follow the game's actual frame rate;
headset reprojection does not generate additional mirror updates. More frequent
mirror rendering can reduce game FPS, especially on buses with many mirrors.

For testing, `OMSI_OPENXR_MIRROR_RATE` overrides the saved VR mirror budget.
Set it to `-1` for Every frame, `0` to freeze, or a positive total redraw rate.

## Controls

The VR key bindings are editable under **Controls → Keyboard**; search for `VR`.
The default bindings are:

| Action | Default key |
| --- | --- |
| Reset the VR view | Ctrl+Shift+R |
| Toggle the monitor preview | F7 |
| Switch between VR and desktop | F8 |
| Show or hide the VR navigator | Ctrl+Shift+N |
| Position the VR navigator | Ctrl+Shift+M |

Adjust your seating position while driving under **Esc → Options → Camera** with
**Seat forward and back**, **Seat height**, **Seat left and right** and **Head pitch**.
**Reset the seat position** puts the seat back where the bus has it. The launcher
has the same sliders under **Settings → Camera**; those changes apply when the next
drive starts.

**Esc** opens the menu in front of the headset. Move the mouse to point at cockpit
controls and left-click to use them. The pointer fades after ten seconds without
mouse movement and returns in the centre when moved again. When mouse steering is
active, the cockpit pointer is hidden so the mouse can steer the bus.

In VR, **right-click** toggles a smooth zoom; the next right-click returns to the
normal view. The mouse remains usable while zoomed. Outside VR, right-drag retains
its normal camera control.

The headset runtime controls its own reprojection settings. No runtime debug tool
setting is needed to enable openOMSI's VR mode.

## VR navigator

The navigator is off by default for buses without saved settings. **Ctrl+Shift+N**
shows or hides it. **Shift+N** cycles between the map, map with stop list, and off.
The display stays attached to the bus as you drive and look around.

Press **Ctrl+Shift+M** to position it with the mouse, or open
**Esc → Options → VR → Move and rotate with the mouse...**.
Opening placement mode also enables the navigator. These controls and the placement
menu are available only in VR.

| Input in placement mode | Action |
| --- | --- |
| Hold left mouse and move | Move the display |
| Hold right mouse and move | Turn and tilt the display |
| Shift + right mouse drag | Roll the display sideways |
| Mouse wheel | Move closer or farther away |
| Ctrl + mouse wheel | Make the display larger or smaller |
| R | Reset placement and size, keeping visibility unchanged |
| Esc or Enter | Save and finish |

The placement menu also offers numeric position, width, rotation, tilt, roll and
background opacity settings. Placement and visibility are saved separately for
each bus.

Single-player pauses during placement; multiplayer continues running. Mouse input
adjusts the navigator instead of operating cockpit controls. Losing window focus
saves and ends placement. The display is an overlay, so place it in a clear spot:
cockpit geometry does not hide it.
