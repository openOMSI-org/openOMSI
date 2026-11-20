# NVIDIA DLSS and AMD FSR 3

Temporal upscaling (`crates/omsi-render/src/temporal`) is opt-in: Settings → Graphics → Upscaling, `upscaler=dlss` or `fsr3` plus
`upscale_quality`. A change takes effect on the next start.

- **DLSS** is offered only with an NVIDIA RTX adapter on Windows, subject to the
  Streamline runtime's compatibility checks.
- **FSR 3** can be offered on other vendors' GPUs, subject to runtime support.
- **DirectX 12 or Vulkan** is selected according to the graphics setting and
  the relevant DLLs; failure disables the upscaler and is logged in `game.log`.
- The source integration does not contain the vendor's production DLLs.

## Frame generation

`frame_generation=fsr3`, `dlss2x`, `dlss3x` or `dlss4x` (Settings → Graphics → Frame generation;
DirectX 12 only, with DLSS or FSR 3 upscaling on) makes the window's swap chain the vendor's own,
which shows generated frames between the drawn ones. Applied in the game's graphics menu, the
window's swap chain is made again for the new choice, without a restart:

- **AMD FSR 3** (`temporal/framegen.rs`): AMD's frame-interpolation swap chain, with any card. It
  takes the depth, the motion and the camera of the upscaler's frame, and the picture before
  the HUD, drawn for it beside the window's.
- **NVIDIA DLSS** (`temporal/dlssg.rs`): Streamline's swap chain with DLSS-G, Reflex and PCL,
  with DLSS upscaling on a GeForce RTX 40 or 50 (3x and 4x on an RTX 50; asked for more than
  the card makes, it makes what it can). DLSS tags the same inputs, valid until the present.
  Reflex runs in low-latency mode because DLSS-G requires it; its markers are set around the
  upscaler, so it does not shorten the input lag.

Without frame generation the window keeps wgpu's surface, as before. A frame drawn without the
upscaler (a menu over no scene, the loading screen) goes out without generated frames. Where
frame generation cannot start, `game.log` says why and the window gets wgpu's surface. The
frame rate's corner shows the frames on screen, the drawn ones and the upscaler, e.g.
`240 fps (120 + FG) · DLSS`.

## Windows x64 packaging: AMD bundled, NVIDIA user-installed

`.github/workflows/release.yml` packages the **five specific FidelityFX
SDK 2.4.0 signed DLLs** on every Windows x64 release (FSR 3 upscaling on
DirectX 12 and Vulkan, and FSR 3 frame generation). The SDK's pinned
`Kits/FidelityFX/docs/license.md` explicitly lists those five filenames
as **MIT-licensed exceptions** (see its `These source files ... are subject
to the following license` section). The workflow checks that the five are
listed and includes the full license and third-party notices.

**NVIDIA DLSS DLLs are NOT part of the openOMSI release archive.** On
Windows with an RTX GPU, the launcher offers an **Install NVIDIA DLSS** button.
It remains disabled until the player explicitly acknowledges the linked
NVIDIA Streamline/RTX/NGX/Reflex terms. After the player's click, the launcher
downloads the official Streamline SDK 2.14.1 archive directly from NVIDIA's
GitHub release, checks the pinned SHA-256 and each DLL's NVIDIA digital
signature, and installs only `sl.interposer.dll`, `sl.common.dll`,
`sl.dlss.dll`, `nvngx_dlss.dll` (DLSS), `sl.dlss_g.dll`, `nvngx_dlssg.dll`,
`sl.reflex.dll`, `sl.pcl.dll` (DLSS frame generation) and the accompanying
vendor licenses/notices (`reflex.license.txt` among them). An install from
before frame generation shows as DLSS without it, and installing again adds it.
It does not redistribute the full SDK, and it does not run unattended.
The NVIDIA runtime has **separate terms**: the MIT license for the open
Streamline components must not be assumed to cover the proprietary NGX DLL.
The **entire original openOMSI source integration**, including DLSS and
FSR code, remains AGPLv3. NVIDIA's SDK binaries retain their **own NVIDIA
licenses**, and AMD's binaries retain their applicable **AMD licenses**.
Neither NVIDIA nor AMD vendor binaries are relicensed by the openOMSI AGPLv3
or its historical MIT notice.

**Legal caveat:** mere separate notices do not settle whether the combined
AGPL application and proprietary DLSS runtime can lawfully be distributed
together. User-initiated installation removes NVIDIA's proprietary binaries from
openOMSI's distributed release ZIP but does **not** transfer copyright,
change NVIDIA's license, or guarantee that every form of combined use is
compatible with AGPLv3. The user receives only rights granted by NVIDIA. See
[`NVIDIA_DLL_LICENSE_SCOPE.md`](NVIDIA_DLL_LICENSE_SCOPE.md) and
[`UPSCALER_THIRD_PARTY_LICENSING.md`](UPSCALER_THIRD_PARTY_LICENSING.md).

Shipping the five AMD MIT-listed binaries does not mean every SDK file is
MIT-licensed. None of the production DLLs is committed to the source tree.

## Not verified yet

DLSS and FSR 3 still require GPU validation on DX12 and Vulkan. Build/CI status
and runtime functionality are separate from legal redistribution approval.

The runs a tester goes through, and what to send back, are in `UPSCALING_TESTS.md`.
