<#
  End-to-end test of the players' buses by content, against a running dedicated server
  (docker/server: `docker compose up -d`). Two games without a window join it:

    A drives the stock MAN SD77 - but its .bus is a copy with one line more (a line OMSI
      passes over): a bus the server has only in part. The server takes the files it has
      itself from its own installation and is sent the one that differs.
    B drives a stock MAN SD80 and joins through the server's web gateway (a WebSocket, as
      the launcher does): its mods connection goes through the gateway's tunnel. It lacks
      A's version of the SD77: it fetches it by its fingerprint - the one file that
      differs; the rest it has itself. The server has every file of B's SD80 (its own
      installation's), and keeps that bus by its fingerprint all the same.
    A has B's SD80 file for file: it draws its own copy, fetching nothing.

  Usage (PowerShell, from the repository):
    scripts\e2e-bus-sync.ps1 -OmsiDir "D:\SteamLibrary\steamapps\common\OMSI 2"
  The logs stay in the folder it prints.
#>
param(
    [Parameter(Mandatory = $true)][string]$OmsiDir,
    [string]$Exe = "",
    [string]$ServerHost = "127.0.0.1",
    [int]$Port = 27015,
    [int]$WebPort = 27025,
    [int]$Seconds = 150,
    [string]$Container = "openomsi-server"
)
$ErrorActionPreference = "Stop"
# (Windows PowerShell 5.1 has no $PSScriptRoot in a parameter's default)
if (-not $Exe) { $Exe = Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) "..\target\release\openomsi.exe" }
$Exe = (Resolve-Path $Exe).Path
$work = Join-Path ([IO.Path]::GetTempPath()) ("openomsi-e2e-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
New-Item -ItemType Directory -Force $work | Out-Null
Write-Host "Work folder: $work"

# the server answers
try {
    $status = Invoke-RestMethod -Uri "http://${ServerHost}:${WebPort}/status" -TimeoutSec 10
    Write-Host ("Server: {0} - map {1}, version {2}" -f $status.name, $status.map, $status.version)
} catch {
    throw "No server at http://${ServerHost}:${WebPort}/status - start it first (docker/server: docker compose up -d)."
}
$since = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")

# A's content folder: its own copy of the SD77's .bus, one line more
$contentA = Join-Path $work "contentA"
$contentB = Join-Path $work "contentB"
New-Item -ItemType Directory -Force (Join-Path $contentA "Vehicles\MAN_SD200"), $contentB | Out-Null
$stockBus = Join-Path $OmsiDir "Vehicles\MAN_SD200\MAN_SD77.bus"
$copy = [IO.File]::ReadAllBytes($stockBus) + [Text.Encoding]::ASCII.GetBytes("`r`nThis copy differs from the stock one by this line (openOMSI end-to-end test, $(Get-Date -Format o)).`r`n")
[IO.File]::WriteAllBytes((Join-Path $contentA "Vehicles\MAN_SD200\MAN_SD77.bus"), $copy)

function Start-Game($name, $bus, $content, $drive, $join) {
    $log = Join-Path $work "$name.log"
    $env:OMSI_CONTENT = $content
    $env:RUST_LOG = "info"
    $argv = @("--root", "`"$OmsiDir`"", "--map", "maps/Grundorf/global.cfg", "--bus", $bus,
              "--lan-join", $join, "--lan-name", $name,
              "--offscreen", "`"$(Join-Path $work "$name.png")`"", "--drive", "$drive")
    $p = Start-Process -FilePath $Exe -ArgumentList $argv -RedirectStandardError $log -RedirectStandardOutput (Join-Path $work "$name.out") -PassThru -WindowStyle Hidden
    Remove-Item Env:OMSI_CONTENT
    Write-Host "$name started (pid $($p.Id)): $bus"
    return $p
}

$a = Start-Game "A" "Vehicles/MAN_SD200/MAN_SD77.bus" $contentA $Seconds "${ServerHost}:${Port}"
Start-Sleep -Seconds 25
$b = Start-Game "B" "Vehicles/MAN_SD200/MAN_SD80.bus" $contentB ($Seconds - 25) "ws://${ServerHost}:${WebPort}"
Write-Host "Waiting for both games to end (up to $($Seconds + 240) s)..."
$a | Wait-Process -Timeout ($Seconds + 240) -ErrorAction SilentlyContinue
$b | Wait-Process -Timeout 240 -ErrorAction SilentlyContinue
foreach ($p in @($a, $b)) { if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force } }

# (docker writes its log to stderr: through cmd, so PowerShell 5.1 does not take the lines for errors)
$server = (cmd /c "docker logs --since $since $Container 2>&1") -join "`n"
[IO.File]::WriteAllText((Join-Path $work "server.log"), $server)
$logA = Get-Content -Raw (Join-Path $work "A.log")
$logB = Get-Content -Raw (Join-Path $work "B.log")

$checks = @(
    # (fingerprints are v2 - the bus file's path hashed too)
    @("A worked out its bus's fingerprint", $logA, "bus identity: .*MAN_SD77.* is v2-"),
    @("the server was offered A's SD77", $server, "uploads: a player offers .*MAN_SD77"),
    # (the files it has itself - its installation's, or kept from an earlier run - are not wanted)
    @("of A's SD77 the server wanted only the file that differs", $server, "uploads: a player offers .*MAN_SD77.* \(\d+ files\): 1 wanted"),
    @("the server draws A's SD77 as A has it", $server, "vehicles: .*MAN_SD77.* fetched by its content"),
    @("the server kept A's SD77, sent only what differs", $server, "uploads: .*MAN_SD77.* is kept as v2-[0-9a-f]+ \(1 files came\)"),
    @("B fetched A's SD77 by its content", $logB, "vehicles: .*MAN_SD77.* fetched by its content"),
    @("A drew B's SD80 from its own copy (the same, file for file)", $logA, "vehicles: .*MAN_SD80.* is the same here"),
    @("B offered its stock SD80 and the server wanted no file of it", $server, "uploads: a player offers .*MAN_SD80.* \(\d+ files\): 0 wanted"),
    @("the server keeps B's SD80 by its fingerprint though it had every file", $server, "uploads: .*MAN_SD80.* is kept (already )?as v2-"),
    @("B joined through the web gateway and reached the mods through its tunnel", $logB, "through ws://.*/tcp")
)
$failed = 0
foreach ($c in $checks) {
    $ok = $c[1] -match $c[2]
    if (-not $ok) { $failed++ }
    Write-Host ("[{0}] {1}" -f ($(if ($ok) { "OK  " } else { "FAIL" })), $c[0])
}
Write-Host "Logs: $work"
if ($failed -gt 0) { Write-Host "$failed of $($checks.Count) checks failed."; exit 1 }
Write-Host "All $($checks.Count) checks passed."
