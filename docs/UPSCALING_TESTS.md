# Testing DLSS and FSR 3

What a tester runs before DLSS and FSR 3 (`docs/NVIDIA_DLSS.md`) are called working. Runs 1 to 5
have been done on an RTX 5080. Testers also report DLSS on an RTX 5070 Ti, RTX 3060 Laptop and
RTX 2070, and FSR 3 on an RX 580 and a GTX 1050. Intel cards have not been tried yet. Every
further result counts, a failure as much as a success.

## Before

- The Windows x64 build of this branch (the `windows-x64` artifact of its Release run). Its
  folder holds `sl.interposer.dll`, `sl.common.dll`, `sl.dlss.dll`, `nvngx_dlss.dll` and
  `amd_fidelityfx_loader_dx12.dll`, `amd_fidelityfx_upscaler_dx12.dll`,
  `amd_fidelityfx_loader_vk.dll`, `amd_fidelityfx_upscaler_vk.dll` beside `openomsi.exe`.
- An up-to-date graphics driver. Note the card, the driver version and Windows' version.
- One map and one bus for every run, the same time of day, so the pictures compare.

## The runs

Each run: choose it in Settings → Graphics (Upscaling, Upscaling quality, Graphics API), start
the game anew, drive a minute (at a stop, past other traffic, through rain if it can), take a
screenshot, quit, keep `game.log`.

| # | Card | Graphics API | Upscaler | Quality | The log must say |
|---|---|---|---|---|---|
| 1 | NVIDIA RTX | DirectX 12 | DLSS | Quality | `DLSS: Streamline 2.14.1 ... (DirectX 12)`, `dlss upscaling (quality) running` |
| 2 | NVIDIA RTX | DirectX 12 | DLSS | Native (DLAA) | as 1, the window's size drawn |
| 3 | NVIDIA RTX | Vulkan | DLSS | Quality | `DLSS on Vulkan: the device is made with ...`, `(Vulkan)` |
| 4 | NVIDIA RTX | DirectX 12 | FSR 3 | Balanced | `FSR 3 upscaling: AMD FidelityFX for Dx12 loaded` |
| 5 | NVIDIA RTX | Vulkan | FSR 3 | Balanced | `FSR 3 upscaling: AMD FidelityFX for Vulkan loaded` |
| 6 | AMD or Intel | DirectX 12 | FSR 3 | Quality | as 4; DLSS is not in the list at all |
| 7 | AMD or Intel | Vulkan | FSR 3 | Performance | as 5 |
| 8 | NVIDIA GTX (no RTX) | any | - | - | DLSS is not in the list; FSR 3 is |

Run 3 may end with `dlss upscaling is off: ...` instead: that line (Streamline asking for a Vulkan
feature, queue or extension the game's device does not have) is the result to report, not a
failure of the test.

## What to look at

- The picture is sharp at the window's size; the bus's displays and the cab's gauges are
  readable; the HUD and the menus are at full size on top.
- No ghosting behind moving buses and cars, no smearing on the windows in the rain, no
  flickering on fences and trees.
- The frame rate against the same scene with Upscaling: Standard (Settings: show FPS, or the
  performance overlay).
- The screenshot shows the upscaled picture.
- Changing the window's size, full screen and back, Alt+Tab: no black picture, no crash.
- Should it fail, the game goes on without the upscaler: the log says `upscaling is off` or
  `upscaling stopped working` and why.

## Report

Per run: the card, the driver, the run's number, works / fails, the frame rates with and
without, the screenshot, and `game.log` (the lines with `DLSS`, `Streamline`, `FSR 3`,
`FidelityFX`, `upscaling`, `opening graphics device`).

## Apple MetalFX (macOS)

MetalFX is the Mac's temporal upscaler (`upscaler=metalfx`): Apple silicon, macOS 13 or newer,
Metal. Nothing lies beside the game for it. Run 1 has been done on an Apple M2 (macOS 27.2).
Note the Mac's chip and macOS' version.

| # | Mac | Upscaler | Quality | The log must say |
|---|---|---|---|---|
| 1 | Apple silicon, macOS 14.4 or newer | MetalFX | Quality | `MetalFX upscaling: Apple's temporal scaler, up to ...x on this Mac`, `metalfx upscaling (quality) running` |
| 2 | Apple silicon | MetalFX | Native | as 1, the window's size drawn (`MetalFX upscaling: WxH to WxH`) |
| 3 | Apple silicon | MetalFX | Ultra performance | as 1, and `this Mac's MetalFX does ...x at most; drawing at ...` where the chip scales up less than three times |
| 4 | Apple silicon, macOS 13.0 to 14.3 | MetalFX | Quality | as 1, with `without the mask` |
| 5 | Intel Mac, or macOS 11 or 12 | - | - | MetalFX is not in the list |

What to look at and what to report are as above; the log's lines are those with `MetalFX`,
`metalfx`, `upscaling` and `opening graphics device`. The frame rate and the frame times can be
read off Metal's own overlay as well: start the game with `MTL_HUD_ENABLED=1`.

On a Mac with a graphics card, `cargo test -p omsi-render --lib -- --ignored fsr::` runs the
upscaler's tests with MetalFX, among them a still picture that has to settle on the full-sized
one (`metalfx_settles_on_the_full_sized_picture`).

## Frame generation (macOS)

Apple's MetalFX frame interpolator (`frame_generation=metalfx`): Apple silicon, macOS 26 or newer,
with MetalFX upscaling on. Start the game with `MTL_HUD_ENABLED=1` and the frame rate shown.

| # | Mac | Settings | The log and the screen must say |
|---|---|---|---|
| 6 | Apple silicon, macOS 26 or newer | MetalFX Quality, Frame generation on, V-sync off | `frame generation: Apple MetalFX for WxH, the scene drawn at WxH`; the corner `N fps (M + FG)` with N about twice M; Metal HUD's frame rate about N |
| 7 | as 6 | as 6, V-sync on | Metal HUD's frame-interval graph flat or a regular pattern, its histogram at most two buckets |
| 8 | as 6 | switched off and on in the game's menu | `frame generation: off`, then 6's line again, no restart |
| 9 | as 6 | Upscaling changed to Standard while on | no generated frames (the corner without `+ FG`) |
| 10 | macOS 25 or older | - | Frame generation is not in the list |
| 11 | as 6 | a heavy place, or full screen at the Mac's full resolution, where fewer than 30 frames a second are drawn with it | after about 10 s: `frame generation off to keep up` in the log and "The graphics card cannot keep up: frame generation off for this drive" on screen; no `+ FG` in the corner. Off and on in the menu: tried again |

Look at the HUD, the cab's displays and the menus over generated frames (they must stay steady,
not doubled or wobbling), and at fast turns and camera cuts (no smear from the frame before).
`OMSI_FG_SHOW=generated` shows the generated frames only, `drawn` the drawn ones only, to look at
one kind alone. `cargo test -p omsi-render --lib -- --ignored metalfx_makes_the_frame_between_two`
checks that a generated frame lies between its two drawn ones and that the HUD stays put.

