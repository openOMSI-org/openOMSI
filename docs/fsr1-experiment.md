# FSR 1 upscaling: notes and measurements

Developer notes on openOMSI's AMD FidelityFX Super Resolution 1 (spatial upscaling with EASU + RCAS). It records how it works, how to measure it, the measurements so far and what is still open. For the player's view of upscaling, see [Graphics](GRAPHICS.md).

**Summary**

- FSR 1 uses shaders derived from AMD's MIT-licensed reference. It needs no driver DLLs. It is spatial only: no temporal upscaling or frame generation.
- It is off by default. The launcher lists it as **AMD FSR 1** under **Settings → Graphics → Upscaling**.
- On the one card measured (an R7 240 class GCN 1 card), FSR 1 costs more GPU time than the standard upscaler at the same internal size and buys no frame rate over simply lowering the standard render scale. Its picture is cleaner (no stair-steps or halos).
- Sharpening: the slider's 100 % is 0.8 of AMD's RCAS strength; the default is 80 %.
- No performance gain is claimed until measured on more hardware.

## Enable and compare

1. Open **Settings → Graphics → Upscaling** in the launcher, or **Esc → Options → Graphics** in the game.
2. Choose **AMD FSR 1**.
3. Choose an **FSR quality** preset and an **FSR sharpness** (0–100 %).

Both are saved in `settings.cfg` and included in graphics profiles. In the game the change shows at once.

| FSR quality | Internal scale |
|---|---|
| Ultra Quality | 77 % |
| Quality | 67 % |
| Balanced | ~59 % |
| Performance | 50 % (the renderer clamps fixed scales to 0.5) |
| Automatic (keeps the frame rate) | Left to the frame-rate governor |
| Use render scale | The **Render scale** setting |

Behaviour:

- FSR 1's passes are compiled the first time it is chosen; nothing is compiled for players who never do.
- If the graphics driver refuses them, the game says so once and keeps the standard upscaler for the rest of the run.
- The FSR quality row shows the size the 3D picture is drawn at and the window's size (`1286×723 … 1920×1080`).
- **Automatic** uses render scale *Auto*: the picture is drawn smaller while the card cannot keep the frame rate (85, 70, 55 %) and FSR 1 scales it up. At full size nothing is upscaled.
- A **Render scale** below the preset's still caps it. The optimiser's steps down and the lighter graphics after a lost graphics device (75 %) lower the render scale; the picture is drawn at the smaller of the two (`fsr1_presets_keep_the_render_scale_caps`).
- For a quick 1080p test, choose **Quality** (0.67 internal scale).
- The `OMSI_FSR1=1` override still works for developer testing; then **Render scale** sets the internal resolution.

### Where FSR 1 is used

| Picture | Upscaler |
|---|---|
| The window | FSR 1 |
| Each panel of a triple screen | FSR 1 |
| VR eyes | FSR 1 (see below) |
| Mirrors, offscreen pictures | Standard (Catmull-Rom) |
| Anything when FSR is off or nothing is scaled | Standard |

- **Mirrors** keep their own size. They are small (256 by default), and FSR would add two passes per mirror picture for little. Their GPU time shows as `mirrors: mirror` in the measurements, which decides whether that is worth trying.
- **VR:** without FSR, the headset image itself is made at the eye resolution (`vr_scale`, 65 % by default) and the runtime scales it. With FSR on when VR starts, the image is full-sized and each eye is drawn at the eye resolution and scaled up by FSR 1. That is sharper at the same drawing cost, plus two FSR passes per eye and a larger image for the runtime. Turned off during the VR session, the eyes keep that size and the standard upscaler takes over. `OMSI_OPENXR_SCALE` takes the place of the eye resolution once. **Not tried on a headset yet**; the renderer's part is tested offscreen (`a_vr_eye_with_fsr1_is_drawn_smaller_and_scaled_up`).

### Sharpening

- The FSR sharpness slider controls RCAS. Default: **80 %**.
- The slider's whole range maps onto 0–0.8 of AMD's RCAS strength (`fsr1_rcas_strength`). Measured below, AMD's strongest (1.0) was overdone: grain on the gauges, dark outlines. 0.8 was as sharp as the standard upscaler and clean, so that is the slider's 100 %. 30 % of the old scale was too soft.
- A `settings.cfg` saved with the old scale keeps its number, now 80 % as strong. 0.3 there was too soft already, so raise it.
- At 0 %, RCAS would return its input unchanged, so it is skipped. EASU then draws the window's picture itself: one full-screen pass and its full-size intermediate fewer (`fsr1_without_sharpening_skips_rcas`). Only `FSR1 EASU` then shows in the pass times.
- A picture FSR does not draw (a mirror, vanilla FXAA, an offscreen view) keeps the standard upscaler's own sharpening. The slider is RCAS's only.

## Measuring it

Send numbers, not impressions.

**Hold fixed:** the place (standing still), MSAA, graphics backend and window size. Turn the frame limit off (`max fps` 0 or high). A limit below the card's rate hides every difference, and below 50 it also stops the governor.

### Frame rate, in one run

1. Start with `OMSI_PROFILE=1`. Once a second `game.log` says `profile interval: 30.3 fps over 1.01 s`.
2. Switch the upscaler or the FSR quality in **Esc → Options → Graphics** (no restart needed).
3. Wait a minute between switches. Compare the averages of each stretch.

To compare FSR 1 with the standard upscaler *at the same internal resolution*, set the standard one's **Render scale** to the FSR quality's (67 % for Quality).

### GPU time per pass, one run per setting

1. Find the `command line:` line in `game.log`.
2. Start `openomsi.exe` with that command line plus `--exit-after 120`, with `OMSI_PROFILE=1` and `OMSI_GPU_TIMERS=1`. In PowerShell:
   ```powershell
   $env:OMSI_PROFILE='1'; $env:OMSI_GPU_TIMERS='1'; .\openomsi.exe <that command line> --exit-after 120
   ```
3. Repeat once with each setting.
4. When the game ends by itself, the log has `profile gpu pass FSR1 EASU: … ms`, `FSR1 RCAS` (or `upscale` for the standard upscaler), and `profile: window 1161x653, scene drawn at 778x437`.

With `OMSI_PROFILE_JSON=<file>` the same summary goes to a file. `scripts/compare-performance.py a.json b.json` compares two.

## Results so far (one machine)

**Setup**

| Item | Value |
|---|---|
| Card | AMD Radeon R7 240 class (GCN 1, Vulkan, 4 GB) |
| CPU, OS | 20-thread CPU, Windows |
| Window | 1161x653 |
| Settings | MSAA 2x, SSAO on, frame limit off |
| Scene | Ponte Rasa, standing in the cab, same start every run |
| Run | 150 s each (`--exit-after 150`); frame rate averaged over the ~85 s after loading |
| Pass times | `OMSI_GPU_TIMERS` |

The game waits on the graphics card (`profile acquire` 16.8–18.6 ms of a ~27 ms frame), so GPU time saved shows as frame rate. Between two runs of the same setting the average moved by up to 0.8 fps: differences under ~1 fps are a tie.

| Setting | Scene | fps | GPU (timed passes) | upscale / EASU / RCAS (ms) |
|---|---|---|---|---|
| Standard 67 % | 778x438 | 36.8–37.6 | 4.5–5.6 ms | 1.62–1.66 / – / – |
| Standard 59 % | 685x385 | 40.4–41.0 | 5.0–5.2 ms | 1.62–1.66 / – / – |
| FSR 67 % (Quality), sharpening 30 % | 778x438 | 34.5–34.8 | 9.0 ms | – / 2.75 / 0.93 |
| FSR 67 %, sharpening 0 (RCAS skipped) | 778x438 | 35.2 | 7.6 ms | – / 2.85 / – |
| FSR 59 % (Balanced), sharpening 0 | 685x385 | 38.8–38.9 | 7.3–7.5 ms | – / 2.81 / – |
| FSR 59 %, sharpening 30 / 80 / 100 % | 685x385 | 37.3–37.8 | 8.7–9.0 ms | – / 2.70 / 0.93 |
| FSR 50 % (Performance), sharpening 0 | 581x327 | 42.5–42.6 | 7.0 ms | – / 2.80 / – |
| FSR 50 %, sharpening 30 / 80 / 100 % | 581x327 | 40.4–41.2 | 8.2–8.5 ms | – / 2.70 / 0.93 |

The "timed passes" figure leaves out the first main-pass part on frames where it is not timed. Compare it between rows, not with the frame time.

**Findings**

- **Cost.** EASU alone (2.7–2.85 ms) costs more than the whole standard upscaler (1.62–1.66 ms, its sharpening included). RCAS adds ~0.93 ms whatever its strength. At the same internal size FSR is 2–2.5 fps slower than the standard upscaler on this card.
- **As an optimisation.** FSR at 50 % with useful sharpening (40.4–41.2 fps) ties the standard upscaler at 59 % (40.4–41.0 fps). On this card it buys no frame rate over lowering the standard render scale, which at 59 % looks nearly the same as at 67 %.
- **Picture.** Screenshots from the same spot via `OMSI_INPUT="t=140 shot <file.png>"`, cropped and enlarged; brightness identical across all runs within 1/255.
  - Standard: sharper, readable gauge numbers and bus lettering. But stair-steps on diagonals (the viaduct's underside) and dark/light halos round foliage from its strong CAS (0.66 at 67 %, 0.8 at 59 %).
  - FSR: clean diagonals, no stair-steps or CAS halos. With 0–30 % sharpening clearly softer (gauge numbers and lettering blur). At 80 % close to the standard's sharpness and clean. At 100 % overdone: grain on the gauges, dark outlines round the bus and trees.
- **Sharpening scale.** RCAS's strength grows steeply near the top (it divides by `1 + 4 * lobe`). On the strongest edges the centre is weighted ~1.3x at 30 %, ~2.5x at 80 %, ~4x at 100 %.

## Changes since these measurements (to be measured again)

| Change | What it does | Effect checked |
|---|---|---|
| Flat areas | Where the four texels round an output pixel differ by less than 1/1024 in every channel (sky, road, walls), EASU's clamped result can only be within that of their bilinear mix. So the bilinear mix is drawn, and the other eight taps (24 square roots) and the edge analysis are skipped. | Test scene picture identical to the byte with and without it |
| RCAS's input | EASU writes the intermediate in perceptual values in `Rgb10a2Unorm` (32 bits a pixel, as before). RCAS reads it back without its 15 square roots a pixel. | Against the old sRGB intermediate, 99.8 % of the test picture's bytes are the same and the rest one step apart (eight bits rounded twice: a step off on a third of the pixels) |
| Sharpening slider | New scale and 80 % default (above) | – |
| Mip bias | Drawn smaller, the scene used its textures' mips for the smaller picture: letters, gauges and signs blurred before FSR saw them. With FSR 1 scaling up, textures are now read with a mip bias of log2 of the scale (−0.58 at 67 %, −1 at 50 %), AMD's advice. Shadow and rain maps, drawn at their own size, keep their mips. The standard upscaler is unchanged. `OMSI_NO_FSR_MIP_BIAS=1` leaves the bias out, to compare in the game. | See below |
| `OMSI_FSR_RAW_INPUT=1` | Measurement switch, off by default, not on OpenGL. EASU reads the scene through a non-sRGB view, values as stored (no square roots for its twelve taps); the exact sRGB curve is applied once a pixel on the way out. | Not tried on a GPU yet (the test adapter here is OpenGL). If `FSR1 EASU`'s time falls and the picture is the same, it becomes the default. |

**Mip bias test** (`fsr1_textures_keep_their_detail_with_the_mip_bias`): a checkered wall square on to the camera at 50 %, texels as wide as high.

| Case | Contrast of the checks |
|---|---|
| With the bias | 147 986 |
| Without (the mips' grey) | 14 064 |
| Drawn at full size | 708 212 |

On a wall striped one way only, Metal's anisotropic filtering took the top mip with the bias or without it, and the test saw no difference.

**No anti-aliasing before FSR in Vanilla.** Vanilla's FXAA runs only at full size. At a render scale, FSR gets an aliased picture and enlarges its stair-steps. Until a pass for it exists, use MSAA 2x with FSR (the measurements above did). Enhanced already runs its FXAA on the smaller picture.

## Open questions

- The default: 80 % on the new scale, to be confirmed by a test at 60 / 80 / 100 %.
- A non-sRGB view of the scene for EASU's twelve taps (no square roots at all) needs the scaled scene made with that view allowed, which OpenGL cannot do. Left until a measurement says the remaining square roots matter.
- Measurements on a newer card. FSR's cost is fixed per output pixel, the scene's grows with resolution; the trade may differ there.

## Technical design

1. The source picture is tone mapped.
2. **EASU** writes into a cached full-sized target (`Rgb10a2Unorm`, perceptual values), or straight into the window when RCAS is skipped.
3. **RCAS** reads the cached target and writes the final window image.
4. HUD and overlay quads are drawn after RCAS at native resolution.

Details:

- Both passes work on a perceptual picture, as FSR 1 is designed for. On an sRGB window format the texels read back linear, so they are taken to gamma 2.0 on the way in and back to linear on the way out (as AMD's sample does). On a non-sRGB format the stored values are used as they are. Without this, EASU's edge test and RCAS's limits ran on linear light: more halos in the darks, uneven sharpening.
- Uses WGSL `textureLoad` (no `textureGather` requirement). No changes to the scene shaders, the wgpu fork, multiplayer or game asset loading.
- The temporary image is cached by window resolution, not allocated per frame. All FSR shader compilation is skipped unless FSR 1 is chosen.
- Existing anti-aliasing and render scale settings stay in charge. For best image quality test with MSAA and avoid noisy inputs.

## Remaining validation

Before FSR 1 is called finished:

1. `cargo fmt --check`, `cargo test -p omsi-render --lib` and an application build pass.
2. WGSL pipeline construction is exercised on DX12, Vulkan, ANGLE and OpenGL, including an older GPU.
3. Visual A/B comparisons cover routes, distant text, wires, foliage, bus destination displays and night lighting.
4. GPU timings and memory use are compared at the **same internal resolution** against the old scaler, with MSAA and backend held fixed.
5. Resize, minimised/restored windows, and the runtime fallback when a driver rejects FSR are checked. The fallback is covered on the software adapter by `fsr1_turned_on_while_running_upscales_the_picture`.
