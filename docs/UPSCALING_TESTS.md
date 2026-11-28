# Testing DLSS, FSR 3 and MetalFX

This is the test plan a tester runs before NVIDIA DLSS, AMD FSR 3 and Apple MetalFX (upscaling and frame generation) are called working. For what these upscalers are and how players use them, see [Graphics](GRAPHICS.md).

**Summary**

| Area | Status |
|---|---|
| DLSS / FSR 3, runs 1–5 | Done on an RTX 5080 |
| DLSS, tester reports | RTX 5070 Ti, RTX 3060 Laptop, RTX 2070 |
| FSR 3, tester reports | RX 580, GTX 1050 |
| Intel cards | Not tried yet |
| MetalFX upscaling, run 1 | Done on an Apple M2 (macOS 27.2) |

Every further result counts, a failure as much as a success.

## Windows: DLSS and FSR 3

### Before you start

- The Windows x64 build under test (the `windows-x64` artifact of its Release run). Beside `openomsi.exe` it holds the four AMD files: `amd_fidelityfx_loader_dx12.dll`, `amd_fidelityfx_upscaler_dx12.dll`, `amd_fidelityfx_loader_vk.dll`, `amd_fidelityfx_upscaler_vk.dll`.
- For DLSS: the NVIDIA files `sl.interposer.dll`, `sl.common.dll`, `sl.dlss.dll` and `nvngx_dlss.dll`. They are not in the release ZIP. Install them in the launcher with **Settings → Graphics → Install NVIDIA DLSS** (after accepting NVIDIA's terms); see [NVIDIA DLL license scope](NVIDIA_DLL_LICENSE_SCOPE.md).
- An up-to-date graphics driver. Note the card, the driver version and the Windows version.
- One map and one bus for every run, at the same time of day, so the pictures compare.

### Steps for each run

1. In **Settings → Graphics**, set **Upscaling**, **Upscaling quality** and **Graphics API** as the run's row says.
2. Start the game anew.
3. Drive for a minute: at a stop, past other traffic, through rain if possible.
4. Take a screenshot.
5. Quit and keep `game.log`.

### Runs

| # | Card | Graphics API | Upscaler | Quality | `game.log` must say |
|---|---|---|---|---|---|
| 1 | NVIDIA RTX | DirectX 12 | DLSS | Quality | `DLSS: Streamline 2.14.1 ... (DirectX 12)`, `dlss upscaling (quality) running` |
| 2 | NVIDIA RTX | DirectX 12 | DLSS | Native (DLAA) | As 1, the window's size drawn |
| 3 | NVIDIA RTX | Vulkan | DLSS | Quality | `DLSS on Vulkan: the device is made with ...`, `(Vulkan)` |
| 4 | NVIDIA RTX | DirectX 12 | FSR 3 | Balanced | `FSR 3 upscaling: AMD FidelityFX for Dx12 loaded` |
| 5 | NVIDIA RTX | Vulkan | FSR 3 | Balanced | `FSR 3 upscaling: AMD FidelityFX for Vulkan loaded` |
| 6 | AMD or Intel | DirectX 12 | FSR 3 | Quality | As 4; DLSS is not in the list at all |
| 7 | AMD or Intel | Vulkan | FSR 3 | Performance | As 5 |
| 8 | NVIDIA GTX (no RTX) | any | – | – | DLSS is not in the list; FSR 3 is |

Run 3 may end with `dlss upscaling is off: ...` instead. That line (Streamline asking for a Vulkan feature, queue or extension the game's device does not have) is the result to report, not a failure of the test.

### What to look at

- The picture is sharp at the window's size. The bus's displays and the cab's gauges are readable. The HUD and the menus are at full size on top.
- No ghosting behind moving buses and cars, no smearing on the windows in the rain, no flickering on fences and trees.
- The frame rate, against the same scene with **Upscaling: Standard** (Settings: show FPS, or the performance overlay).
- The screenshot shows the upscaled picture.
- Resize the window, go full screen and back, Alt+Tab: no black picture, no crash.
- On failure the game goes on without the upscaler. The log says `upscaling is off` or `upscaling stopped working`, and why.

### Report

Per run, send:

- the card and the driver version;
- the run's number;
- works / fails;
- the frame rates with and without the upscaler;
- the screenshot;
- `game.log`, or at least its lines with `DLSS`, `Streamline`, `FSR 3`, `FidelityFX`, `upscaling` and `opening graphics device`.

## macOS: MetalFX upscaling

MetalFX is the Mac's temporal upscaler (`upscaler=metalfx`). It needs Apple silicon, macOS 13 or newer and Metal. No extra files are needed. Note the Mac's chip and the macOS version.

Follow the same steps, things to look at and report as for Windows. The log lines to send are those with `MetalFX`, `metalfx`, `upscaling` and `opening graphics device`. Frame rate and frame times can also be read off Metal's own overlay: start the game with `MTL_HUD_ENABLED=1`.

| # | Mac | Upscaler | Quality | `game.log` must say |
|---|---|---|---|---|
| 1 | Apple silicon, macOS 14.4 or newer | MetalFX | Quality | `MetalFX upscaling: Apple's temporal scaler, up to ...x on this Mac`, `metalfx upscaling (quality) running` |
| 2 | Apple silicon | MetalFX | Native | As 1, the window's size drawn (`MetalFX upscaling: WxH to WxH`) |
| 3 | Apple silicon | MetalFX | Ultra performance | As 1, plus `this Mac's MetalFX does ...x at most; drawing at ...` where the chip scales up less than three times |
| 4 | Apple silicon, macOS 13.0 to 14.3 | MetalFX | Quality | As 1, with `without the mask` |
| 5 | Intel Mac, or macOS 11 or 12 | – | – | MetalFX is not in the list |

On a Mac with a graphics card, this runs the upscaler's tests with MetalFX, including a still picture that must settle on the full-sized one (`metalfx_settles_on_the_full_sized_picture`):

```bash
cargo test -p omsi-render --lib -- --ignored fsr::
```

## macOS: frame generation

Apple's MetalFX frame interpolator (`frame_generation=metalfx`) needs Apple silicon, macOS 26 or newer, and MetalFX upscaling on. Start the game with `MTL_HUD_ENABLED=1` and the frame rate shown.

| # | Mac | Settings | Log and screen must say |
|---|---|---|---|
| 6 | Apple silicon, macOS 26 or newer | MetalFX Quality, Frame generation on, V-sync off | `frame generation: Apple MetalFX for WxH, the scene drawn at WxH`; the corner shows `N fps (M + FG)` with N about twice M; Metal HUD's frame rate about N |
| 7 | As 6 | As 6, V-sync on | Metal HUD's frame-interval graph flat or a regular pattern; its histogram at most two buckets |
| 8 | As 6 | Switched off and on in the game's menu | `frame generation: off`, then 6's line again, without a restart |
| 9 | As 6 | Upscaling changed to Standard while on | No generated frames (no `+ FG` in the corner) |
| 10 | macOS 25 or older | – | Frame generation is not in the list |
| 11 | As 6 | A heavy place, or full screen at full resolution, where fewer than 30 frames a second are drawn | After about 10 s: `frame generation off to keep up` in the log, "The graphics card cannot keep up: frame generation off for this drive" on screen, no `+ FG` in the corner. Off and on in the menu tries again. |

What to look at:

- The HUD, the cab's displays and the menus over generated frames: steady, not doubled or wobbling.
- Fast turns and camera cuts: no smear from the frame before.
- `OMSI_FG_SHOW=generated` shows only the generated frames, `OMSI_FG_SHOW=drawn` only the drawn ones, to look at one kind alone.

This checks that a generated frame lies between its two drawn ones and that the HUD stays put:

```bash
cargo test -p omsi-render --lib -- --ignored metalfx_makes_the_frame_between_two
```
