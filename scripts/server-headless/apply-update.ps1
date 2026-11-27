# Offline update of omsi-server.exe only. Stop the server before running.
# The downloaded ZIP and expected digest were staged by /update download.
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$stage = Join-Path $here ".headless-updates"
$ready = Join-Path $stage "ready.txt"
if (-not (Test-Path -LiteralPath $ready)) {
    throw "No verified update staged; run /update download first."
}
$meta = @(Get-Content -LiteralPath $ready)
if ($meta.Count -ne 3) { throw "Invalid staged metadata" }
$name, $expected, $version = $meta
if ($name -notmatch '^openOMSI-[0-9]+\.[0-9]+\.[0-9]+-headless-test-windows-x64\.zip$' -or
    $name -ne "openOMSI-$version-headless-test-windows-x64.zip" -or
    $expected -notmatch '^[0-9a-f]{64}$') {
    throw "Unexpected update package, version or checksum."
}
$archive = Join-Path $stage $name
$target = Join-Path $here "omsi-server.exe"
$backup = Join-Path $here "omsi-server.exe.previous"
if (-not (Test-Path -LiteralPath $archive) -or -not (Test-Path -LiteralPath $target)) {
    throw "Package or installed server executable is missing."
}
if (Test-Path -LiteralPath $backup) {
    throw "Previous backup already exists at $backup; archive it before another update."
}
$actual = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "Update SHA-256 mismatch; installation refused." }
$temp = Join-Path $stage "extracted-headless"
if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
New-Item -ItemType Directory -Path $temp | Out-Null
try {
    Expand-Archive -LiteralPath $archive -DestinationPath $temp
    $candidate = Join-Path $temp "omsi-server.exe"
    if (-not (Test-Path -LiteralPath $candidate)) { throw "Archive has no omsi-server.exe" }
    $newVersion = ((& $candidate --version) | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $newVersion -ne $version) {
        throw "New server binary has an unexpected version."
    }
    & $candidate --self-test
    if ($LASTEXITCODE -ne 0) { throw "New server failed self-test" }
    # A still-running executable cannot be replaced on Windows. If the
    # rename fails, leave everything intact.
    Move-Item -LiteralPath $target -Destination $backup -ErrorAction Stop
    try {
        Copy-Item -LiteralPath $candidate -Destination $target -ErrorAction Stop
        & $target --self-test
        if ($LASTEXITCODE -ne 0) { throw "New server failed installed self-test" }
    } catch {
        if (Test-Path -LiteralPath $target) { Remove-Item -LiteralPath $target -Force }
        Move-Item -LiteralPath $backup -Destination $target
        throw
    }
    Remove-Item -LiteralPath $ready
    Write-Host "Updated server to $version; backup: $backup"
    Write-Host "Start it with the same OMSI root and server.cfg as before."
} finally {
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
}
