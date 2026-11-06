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
