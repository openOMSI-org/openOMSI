# NVIDIA DLSS and AMD FSR 3

Temporal upscaling (`crates/omsi-render/src/temporal`) is opt-in: Settings → Graphics → Upscaling, `upscaler=dlss` or `fsr3` plus
`upscale_quality`. A change takes effect on the next start.

- **DLSS** is offered only with an NVIDIA RTX adapter on Windows, subject to the
  Streamline runtime's compatibility checks.
- **FSR 3** can be offered on other vendors' GPUs, subject to runtime support.
- **DirectX 12 or Vulkan** is selected according to the graphics setting and
  the relevant DLLs; failure disables the upscaler and is logged in `game.log`.
- The source integration does not contain the vendor's production DLLs.

## Windows x64 packaging: AMD bundled, NVIDIA user-installed

`.github/workflows/release.yml` packages the **four specific FidelityFX
SDK 2.4.0 signed DLLs** on every Windows x64 release. The SDK's pinned
`Kits/FidelityFX/docs/license.md` explicitly lists those four filenames
as **MIT-licensed exceptions** (see its `These source files ... are subject
to the following license` section). The workflow checks that the four are
listed and includes the full license and third-party notices.

**NVIDIA DLSS DLLs are NOT part of the openOMSI release archive.** On
Windows with an RTX GPU, the launcher offers an **Install NVIDIA DLSS** button.
It remains disabled until the player explicitly acknowledges the linked
NVIDIA Streamline/RTX/NGX terms. After the player's click, the launcher
downloads the official Streamline SDK 2.14.1 archive directly from NVIDIA's
GitHub release, checks the pinned SHA-256 and each DLL's NVIDIA digital
signature, and installs only `sl.interposer.dll`, `sl.common.dll`,
`sl.dlss.dll`, `nvngx_dlss.dll` and the accompanying vendor licenses/notices.
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

Shipping the four AMD MIT-listed binaries does not mean every SDK file is
MIT-licensed. None of the production DLLs is committed to the source tree.

## Not verified yet

DLSS and FSR 3 still require GPU validation on DX12 and Vulkan. Build/CI status
and runtime functionality are separate from legal redistribution approval.

The runs a tester goes through, and what to send back, are in `UPSCALING_TESTS.md`.
