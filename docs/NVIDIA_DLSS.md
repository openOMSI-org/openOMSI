# NVIDIA DLSS and AMD FSR 3

Temporal upscaling (`crates/omsi-render/src/temporal`, based on openOMSI-org/openOMSI#3681)
is opt-in: Settings → Graphics → Upscaling, `upscaler=dlss` or `fsr3` plus
`upscale_quality`. A change takes effect on the next start.

- **DLSS** is offered only with an NVIDIA RTX adapter on Windows, subject to the
  Streamline runtime's compatibility checks.
- **FSR 3** can be offered on other vendors' GPUs, subject to runtime support.
- **DirectX 12 or Vulkan** is selected according to the graphics setting and
  the relevant DLLs; failure disables the upscaler and is logged in `game.log`.
- The source integration does not contain the vendor's production DLLs.

## Windows x64 packaging: AMD and NVIDIA runtimes included

`.github/workflows/release.yml` packages the **four specific FidelityFX
SDK 2.4.0 signed DLLs** on every Windows x64 release. The SDK's pinned
`Kits/FidelityFX/docs/license.md` explicitly lists those four filenames
as **MIT-licensed exceptions** (see its `These source files ... are subject
to the following license` section). The workflow checks that the four are
listed and includes the full license and third-party notices.

The Windows x64 packaging step also includes the four unmodified, signed
NVIDIA Streamline 2.14.1/DLSS files from a SHA-256-pinned vendor ZIP:
`sl.interposer.dll`, `sl.common.dll`, `sl.dlss.dll`, and
`nvngx_dlss.dll`. The original vendor license texts are packaged with them.
The NVIDIA runtime has **separate terms**: the MIT license for the open
Streamline components must not be assumed to cover the proprietary NGX DLL.
The game source stays AGPLv3; no NVIDIA DLL is relicensed under AGPL.

**Legal caveat:** mere separate notices do not settle whether the combined
AGPL application and proprietary DLSS runtime can lawfully be distributed
together. This is a maintainer-directed release choice, **not** an AGPLv3
section 7 exception granted by every openOMSI contributor or NVIDIA's
approval of the combination. See
[`NVIDIA_DLL_LICENSE_SCOPE.md`](NVIDIA_DLL_LICENSE_SCOPE.md) and
[`UPSCALER_THIRD_PARTY_LICENSING.md`](UPSCALER_THIRD_PARTY_LICENSING.md).

Shipping the four AMD MIT-listed binaries does not mean every SDK file is
MIT-licensed. None of the production DLLs is committed to the source tree.

## Not verified yet

DLSS and FSR 3 still require GPU validation on DX12 and Vulkan. Build/CI status
and runtime functionality are separate from legal redistribution approval.

The runs a tester goes through, and what to send back, are in `UPSCALING_TESTS.md`.
