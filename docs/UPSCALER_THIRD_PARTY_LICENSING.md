# Third-party temporal upscaler licensing — decision record

**Status: AMD 2.4.0's four FSR DLLs are expressly MIT-listed and bundled.
NVIDIA runtime DLLs are NOT bundled with openOMSI releases; the Windows
launcher offers an optional, player-initiated direct download from NVIDIA
only after the player acknowledges NVIDIA's terms. This does NOT independently
settle AGPLv3/NGX compatibility.** This is not legal advice, nor a claim of consent
from NVIDIA or every openOMSI rights holder.

## Scope

- All original openOMSI source code (including the DLSS/FSR integration) stays
  AGPLv3-licensed. Its license is not changed or excepted by bundling vendor
  binaries.
- NVIDIA: Streamline 2.14.1; `sl.interposer.dll`, `sl.common.dll`,
  `sl.dlss.dll` and `nvngx_dlss.dll`.
- AMD: FidelityFX SDK 2.4.0, commit
  `7539ae8b6568ad29ed78a753404351daf3b5cca7`:
  `amd_fidelityfx_loader_dx12.dll`, `amd_fidelityfx_upscaler_dx12.dll`,
  `amd_fidelityfx_loader_vk.dll`, `amd_fidelityfx_upscaler_vk.dll`.

## AMD: the four exact binary files have an MIT exception

Read the *whole* pinned `Kits/FidelityFX/docs/license.md`, not just its
opening paragraph. The first section sets a restrictive binary-only license
for files **not** excepted below. Beginning at `These source files`, however,
it lists **all four exact signedbin DLLs** above, then states those listed
files `are subject to the following license`: the MIT grant.

This **explicit exception** means these four signed DLLs can be redistributed
together with the AGPLv3 openOMSI distribution under their MIT terms, with
copyright/permission notice preserved. This conclusion is limited to those
four filenames at the pinned commit — it does not cover every binary, SDK
tool, library or future version.

The Windows x64 workflow checks the MIT exception list, copies only the four
allowlisted DLLs, includes the complete SDK license, a short mapping notice
and the SDK's third-party notices. AMD packaging is on by default.

Source:
https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK/blob/7539ae8b6568ad29ed78a753404351daf3b5cca7/Kits/FidelityFX/docs/license.md

## NVIDIA: MIT Streamline core versus proprietary NGX runtime

The Streamline repository's `license.txt` grants MIT terms to the Streamline
source/core portions. The production `nvngx_dlss.dll` is **separately
licensed** by the NVIDIA RTX/NGX SDK EULA (`nvngx_dlss.license.txt` inside
the official SDK distribution). NVIDIA's official Streamline integration guide
identifies this DLL as required for DLSS.

The NVIDIA RTX SDK license grants conditional permission to distribute
SDK material incorporated in applications (section 1(c)) but imposes specific
distribution obligations (section 2(c)) and prohibits use that would cause
the proprietary SDK to become subject to an open-source license (section
4(e)). The RTX supplement also specifies notification before **commercial**
DLSS/NGX application releases (section 4). These provisions do not
automatically prohibit every open-source game; they do mean that the
interaction with AGPLv3 needs an affirmative, case-specific determination.

The proprietary NGX DLL is *not* generally covered by the Streamline MIT
license. Neither openOMSI's AGPLv3 nor its retained historical MIT notice
changes the licensing of any vendor binary. NVIDIA's binaries keep their
original NVIDIA terms; AMD's four specifically MIT-listed binaries keep
their original AMD MIT terms. No openOMSI source code is being removed from
AGPLv3 by this distribution notice. Because openOMSI's optional DLSS integration dynamically loads this
DLL, merely placing it beside `openomsi.exe` or calling it a separate file
does not prove a GPL/AGPL-compatible aggregate. The GNU GPL FAQ treats
linking with GPL-incompatible libraries as needing an appropriate exception
unless a defined exception applies. A custom linking exception for AGPL code
cannot be granted unilaterally for other authors' contributions.

**Current project configuration:** the Windows x64 release workflow does
**not** bundle NVIDIA DLLs. The launcher, at the player's request only,
downloads the pinned official NVIDIA SDK archive, verifies its checksum and
the four allowed signed DLLs, and installs the vendor's original license
files. This opt-in installer does not itself grant an AGPL linking exception,
establish compatibility, sublicense NVIDIA's DLLs, or shift liability to
NVIDIA. Acceptance is not transfer of ownership. NVIDIA's open-source Streamline core and proprietary
NGX runtime remain subject to different legal grants. The maintainers
should obtain confirmation from NVIDIA and appropriate openOMSI rights
holders before public release; see `NVIDIA_DLL_LICENSE_SCOPE.md`.

Official references:
- https://github.com/NVIDIA-RTX/Streamline/blob/v2.14.1/license.txt
- https://github.com/NVIDIA-RTX/Streamline/blob/v2.14.1/docs/ProgrammingGuide.md
- https://github.com/NVIDIA/DLSS/blob/main/LICENSE.txt
- https://docs.nvidia.com/ngx/latest/ngx-eula/index.html
- https://www.gnu.org/licenses/gpl-faq.html#GPLIncompatibleLibs

## Release and test distinction

Legal ability to redistribute a DLL is separate from compatibility,
successful compilation and GPU validation. This review does not prove
the AMD or NVIDIA upscaler works on DX12/Vulkan, does not certify CI and
does not replace a review of any other SDK dependencies.
