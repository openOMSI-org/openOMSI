# Verify and extract the official NVIDIA Streamline 2.14.1 PRODUCTION DLLs
# into a staging folder for an optional user-initiated installation.
# The openOMSI public release does not bundle these proprietary binaries.
#
# Binary source: https://github.com/NVIDIA-RTX/Streamline/releases/tag/v2.14.1
# Do not take files from third-party modifications or development/debug builds.
# The release ZIP SHA-256 is pinned to NVIDIA's own GitHub release asset.
param(
    [Parameter(Mandatory = $true)][string] $Archive,
    [string] $Destination = "dist/windows"
)
$ErrorActionPreference = "Stop"
$ExpectedSha256 = "92c4d954631a1710da86ca3fa8d5034f2b9503838c95fc4ae977ae149319781b"
$ActualSha256 = (Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($ActualSha256 -ne $ExpectedSha256) {
    throw "Official Streamline 2.14.1 ZIP SHA-256 mismatch: got $ActualSha256"
}
New-Item -ItemType Directory -Path $Destination -Force | Out-Null
$OutDir = (Resolve-Path -LiteralPath $Destination).Path
Add-Type -AssemblyName System.IO.Compression
# (ZipFile lives in System.IO.Compression.FileSystem on Windows PowerShell 5.1, which the
# launcher runs as powershell.exe; PowerShell 7 has it in System.IO.Compression already)
Add-Type -AssemblyName System.IO.Compression.FileSystem
$Zip = [System.IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $Archive).Path)
try {
    function Get-SdkEntry {
        param([string] $Filename, [string] $Pattern)
        $Entries = @($Zip.Entries | Where-Object {
            $_.FullName.Replace("\", "/") -match $Pattern
        })
        if ($Entries.Count -ne 1) {
            throw "Streamline SDK archive expected exactly one production '$Filename' ($Pattern), found $($Entries.Count)"
        }
        return $Entries[0]
    }

    function Copy-Entry {
        param($Entry, [string] $Filename)
        $Path = Join-Path $OutDir $Filename
        $InStream = $Entry.Open()
        try {
            $OutStream = [System.IO.File]::Create($Path)
            try { $InStream.CopyTo($OutStream) }
            finally { $OutStream.Dispose() }
        } finally { $InStream.Dispose() }
    }

    # No DLSS Frame Generation, Reflex, debug or development variants.
    foreach ($Dll in @("sl.interposer.dll", "sl.common.dll", "sl.dlss.dll", "nvngx_dlss.dll")) {
        $Escaped = [regex]::Escape($Dll)
        $Entry = Get-SdkEntry $Dll "(?i)(^|/)bin/x64/$Escaped$"
        Copy-Entry $Entry $Dll
        # All shipping modules must be signed by NVIDIA. Reject unsigned and
        # altered DLLs even when the downloaded archive itself hashes correctly.
        $Sig = Get-AuthenticodeSignature -LiteralPath (Join-Path $OutDir $Dll)
        if ($Sig.Status -ne "Valid" -or $Sig.SignerCertificate.Subject -notmatch "(^|, )O=NVIDIA Corporation(,|$)") {
            throw "Invalid NVIDIA production signature: $Dll ($($Sig.Status))"
        }
        Write-Host "Official NVIDIA signed runtime: $Dll"
    }

    # The SDK may carry identical NGX notices under bin/x64 and features.
    # Prefer the production bin/x64 copy without failing on this benign duplicate.
    $NgxNotices = @($Zip.Entries | Where-Object {
        $_.FullName.Replace("\", "/") -match "(?i)(^|/)(bin/x64|features)/nvngx_dlss\.license\.txt$"
    })
    if ($NgxNotices.Count -eq 0) {
        throw "Official NVIDIA NGX redistribution license not found in SDK"
    }
    $NgxLicense = $NgxNotices | Sort-Object {
        if ($_.FullName.Replace("\", "/") -match "(?i)/bin/x64/") { 0 } else { 1 }
    } | Select-Object -First 1
    Copy-Entry $NgxLicense "nvngx_dlss.license.txt"

    $LicenseOptions = @($Zip.Entries | Where-Object {
        $_.FullName.Replace("\", "/") -match "(?i)(^|/)license\.txt$" -and
        $_.FullName.Replace("\", "/") -notmatch "(?i)/(external|tools|source|plugins|features|bin)/"
    } | Sort-Object { $_.FullName.Length })
    if ($LicenseOptions.Count -eq 0) {
        throw "Official Streamline license.txt missing from NVIDIA SDK package"
    }
    Copy-Entry $LicenseOptions[0] "NVIDIA-Streamline-LICENSE.txt"

    $Notice = @"
NVIDIA Streamline SDK v2.14.1 (PRODUCTION)
Original download: https://github.com/NVIDIA-RTX/Streamline/releases/tag/v2.14.1
Official archive SHA-256: $ExpectedSha256

This optional user-installed component set contains NVIDIA's unmodified, signed binaries:
  sl.interposer.dll, sl.common.dll, sl.dlss.dll, nvngx_dlss.dll

These components are installed as unmodified NVIDIA-distributed runtime
libraries. The source code of openOMSI remains licensed under GNU AGPLv3.
All original openOMSI integration source code remains GNU AGPLv3.
These separate, unmodified NVIDIA vendor DLLs are NOT relicensed under
the openOMSI AGPLv3 or its retained historical MIT license.
Each NVIDIA file retains its original applicable vendor license:
  - NVIDIA-Streamline-LICENSE.txt: MIT-covered Streamline components
  - nvngx_dlss.license.txt: proprietary NVIDIA DLSS/NGX runtime
All NVIDIA license terms and intellectual property rights remain in force.
This software contains source code provided by NVIDIA Corporation.
No NVIDIA endorsement of openOMSI is implied. No openOMSI source
code is removed from AGPLv3 and no linking exception is granted here.
See docs/NVIDIA_DLL_LICENSE_SCOPE.md in the project source repository.
Only DLSS Super Resolution / DLAA is included, not DLSS Frame Generation.
The game works without DLSS on unsupported hardware; DLSS is opt-in.
"@
    Set-Content -LiteralPath (Join-Path $OutDir "NVIDIA-Streamline-NOTICE.txt") -Value $Notice -Encoding UTF8
}
finally { $Zip.Dispose() }
