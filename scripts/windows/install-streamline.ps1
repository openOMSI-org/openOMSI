# User-initiated installation of NVIDIA Streamline 2.14.1 for an existing Windows game.
# No NVIDIA binary is stored in the openOMSI release or source tree.
param(
    [Parameter(Mandatory = $true)][string] $GameDirectory,
    [Parameter(Mandatory = $true)][string] $PackagingScript
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$ReleaseUrl = 'https://github.com/NVIDIA-RTX/Streamline/releases/download/v2.14.1/streamline-sdk-v2.14.1.zip'
$Files = @(
    'sl.interposer.dll', 'sl.common.dll', 'sl.dlss.dll', 'nvngx_dlss.dll',
    'NVIDIA-Streamline-LICENSE.txt', 'nvngx_dlss.license.txt',
    'NVIDIA-Streamline-NOTICE.txt'
)
$ResolvedGame = (Resolve-Path -LiteralPath $GameDirectory).Path
if (-not (Test-Path -LiteralPath (Join-Path $ResolvedGame 'openomsi.exe') -PathType Leaf)) {
    throw 'The selected installation directory does not contain openomsi.exe'
}
if (-not (Test-Path -LiteralPath $PackagingScript -PathType Leaf)) {
    throw 'The bundled NVIDIA verification script is missing'
}
# (the launcher's own bus preview runs as `openomsi --launcher-service` and loads no NVIDIA file;
# a game in another folder does not use these files)
$GameExe = Join-Path $ResolvedGame 'openomsi.exe'
$RunningGame = @(Get-CimInstance Win32_Process -Filter "Name = 'openomsi.exe'" | Where-Object {
    $_.CommandLine -notmatch '--launcher-service' -and (-not $_.ExecutablePath -or $_.ExecutablePath -ieq $GameExe)
})
if ($RunningGame.Count -gt 0) {
    throw 'Close openOMSI before installing NVIDIA DLSS files'
}
if (((Get-Item -LiteralPath $ResolvedGame -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw 'For safety, installation through a reparse-point game directory is disabled'
}

# Two independent temporary directories: the downloaded SDK in system temp, and a
# transaction directory on the same volume as the game for atomic replacements.
$DownloadDir = Join-Path ([IO.Path]::GetTempPath()) ('openomsi-nvidia-' + [guid]::NewGuid().ToString('N'))
$TxnDir = Join-Path $ResolvedGame ('.openomsi-nvidia-install-' + [guid]::NewGuid().ToString('N'))
$MovedBackups = [System.Collections.Generic.List[string]]::new()
$Installed = [System.Collections.Generic.List[string]]::new()
$Finished = $false
try {
    New-Item -ItemType Directory -Path $DownloadDir -ErrorAction Stop | Out-Null
    $Stage = Join-Path $DownloadDir 'stage'
    $Archive = Join-Path $DownloadDir 'streamline-sdk-v2.14.1.zip'
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri $ReleaseUrl -OutFile $Archive -UseBasicParsing -MaximumRedirection 5 -TimeoutSec 300 -Headers @{ 'User-Agent' = 'openOMSI-Launcher' }

    # This pinned source-controlled script verifies the full SDK archive SHA256,
    # extracts ONLY the four allowlisted production DLLs, checks every NVIDIA
    # Authenticode signature, and copies the vendor's own licenses and notice.
    & $PackagingScript -Archive $Archive -Destination $Stage

    foreach ($Name in $Files) {
        $Source = Join-Path $Stage $Name
        if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) {
            throw ('Missing expected NVIDIA file in verified archive: ' + $Name)
        }
    }

    New-Item -ItemType Directory -Path $TxnDir -ErrorAction Stop | Out-Null
    $Pending = Join-Path $TxnDir 'pending'
    $Backup = Join-Path $TxnDir 'backup'
    New-Item -ItemType Directory -Path $Pending | Out-Null
    New-Item -ItemType Directory -Path $Backup | Out-Null
    foreach ($Name in $Files) {
        Copy-Item -LiteralPath (Join-Path $Stage $Name) -Destination (Join-Path $Pending $Name) -ErrorAction Stop
    }

    foreach ($Name in $Files) {
        $Target = Join-Path $ResolvedGame $Name
        if (Test-Path -LiteralPath $Target) {
            $Existing = Get-Item -LiteralPath $Target -Force
            if (($Existing.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or $Existing.PSIsContainer) {
                throw ('Refusing to replace a link or directory: ' + $Name)
            }
            Move-Item -LiteralPath $Target -Destination (Join-Path $Backup $Name) -ErrorAction Stop
            $MovedBackups.Add($Name)
        }
        Move-Item -LiteralPath (Join-Path $Pending $Name) -Destination $Target -ErrorAction Stop
        $Installed.Add($Name)
    }
    $Finished = $true
    Write-Output 'NVIDIA Streamline DLSS 2.14.1 installed from the official verified SDK.'
}
catch {
    $OriginalError = $_
    foreach ($Name in $Installed) {
        $Target = Join-Path $ResolvedGame $Name
        if (Test-Path -LiteralPath $Target -PathType Leaf) {
            Remove-Item -LiteralPath $Target -Force -ErrorAction SilentlyContinue
        }
    }
    foreach ($Name in $MovedBackups) {
        $BackupFile = Join-Path (Join-Path $TxnDir 'backup') $Name
        if (Test-Path -LiteralPath $BackupFile -PathType Leaf) {
            Move-Item -LiteralPath $BackupFile -Destination (Join-Path $ResolvedGame $Name) -ErrorAction SilentlyContinue
        }
    }
    throw $OriginalError
}
finally {
    if (Test-Path -LiteralPath $DownloadDir) {
        Remove-Item -LiteralPath $DownloadDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $TxnDir) {
        # (all in place: the replaced files are not kept beside the game; a lock on one of them
        # must not undo a finished install)
        if ($Finished) {
            Remove-Item -LiteralPath (Join-Path $TxnDir 'backup') -Recurse -Force -ErrorAction SilentlyContinue
        }
        # Do not delete a backup directory if rollback could not restore a file.
        $Leftovers = @(Get-ChildItem -LiteralPath (Join-Path $TxnDir 'backup') -ErrorAction SilentlyContinue)
        if ($Leftovers.Count -eq 0) {
            Remove-Item -LiteralPath $TxnDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
