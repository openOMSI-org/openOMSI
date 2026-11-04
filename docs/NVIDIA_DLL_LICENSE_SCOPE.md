# NVIDIA DLL distribution and license scope

This notice applies **only** to the third-party NVIDIA runtime binaries
shipped alongside the Windows x64 build of openOMSI:

- `sl.interposer.dll`
- `sl.common.dll`
- `sl.dlss.dll`
- `nvngx_dlss.dll`

These DLLs come unmodified from NVIDIA Streamline SDK 2.14.1, downloaded
from the original NVIDIA release and checked by a pinned ZIP SHA-256 and
per-DLL Authenticode signature validation. The package also includes
`NVIDIA-Streamline-LICENSE.txt`, `nvngx_dlss.license.txt` and
`NVIDIA-Streamline-NOTICE.txt`.

## Separate vendor licensing; no relicensing

These NVIDIA binaries are **not** licensed under openOMSI's AGPLv3 and are
**not** openOMSI project source code. Streamline's MIT-covered portions and
the separate proprietary NVIDIA DLSS/NGX runtime must retain their original
copyright notices, license terms and limitations. NVIDIA retains the
rights in its SDK and binaries; openOMSI does not grant downstream rights
beyond NVIDIA's license. Distribution does **not** imply NVIDIA sponsorship,
endorsement or an assignment of distributor liability to NVIDIA.

The openOMSI source code remains GNU AGPLv3. This notice does **not**
purport to change the license on third-party contributions to openOMSI,
does **not** grant an AGPLv3 section 7 linking exception on their behalf,
and does **not** create extra rights from NVIDIA.

## Important legal limitation

The NVIDIA RTX SDK/NGX license conditionally permits application-integrated
distribution but requires protection of NVIDIA's rights and prohibits
putting its SDK under an open-source license. Labelling DLLs as separate,
or loading them at runtime, does **not automatically prove** that a combined
AGPLv3 application and the proprietary runtime can be legally conveyed.
A valid additional linking permission would need authorization from the
copyright holders of the affected AGPL-covered code; a maintainer title
alone cannot grant that permission for others' contributions.

The packaging workflow currently bundles these DLLs at the maintainer's
direction. This record is documentation of scope and provenance, **not
a completed legal clearance**. Before a public release, obtain review
and, if appropriate, written clarifications or permissions from NVIDIA
and the relevant AGPL copyright holders.

Official vendor references:
- https://github.com/NVIDIA-RTX/Streamline/releases/tag/v2.14.1
- https://github.com/NVIDIA-RTX/Streamline/blob/v2.14.1/license.txt
- https://github.com/NVIDIA/DLSS/blob/main/LICENSE.txt
- https://docs.nvidia.com/ngx/latest/ngx-eula/index.html

GNU references:
- https://www.gnu.org/licenses/agpl-3.0.html#section7
- https://www.gnu.org/licenses/gpl-faq.html#GPLIncompatibleLibs

No additional permissions for the openOMSI source code are granted by
this notice.