# Samples omsi-server CPU and real process RSS without modifying the host.
# Run from a second PowerShell window with the game server already running.
param(
    [int]$ProcessId = 0,
    [ValidateRange(2,7200)][int]$DurationSeconds = 600,
    [ValidateRange(1,60)][int]$IntervalSeconds = 2,
    [string]$Output = 'omsi-process-metrics.csv'
)
$ErrorActionPreference = 'Stop'
if ($ProcessId -le 0) {
    $candidates = @(Get-Process -Name 'omsi-server' -ErrorAction SilentlyContinue)
    if ($candidates.Count -ne 1) { throw 'Provide -ProcessId: expected exactly one omsi-server process.' }
    $ProcessId = $candidates[0].Id
}
$proc = Get-Process -Id $ProcessId
if ($proc.ProcessName -ne 'omsi-server') { throw 'Selected PID is not omsi-server.' }
$logicalCores = [Environment]::ProcessorCount
$csv = [System.IO.Path]::GetFullPath($Output)
$utf8 = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($csv, 'utc,elapsed_seconds,cpu_one_core_percent,cpu_system_percent,rss_mib' + "`n", $utf8)
$watch = [System.Diagnostics.Stopwatch]::StartNew()
$lastTime = $watch.Elapsed.TotalSeconds
$lastCpu = $proc.CPU
while ($watch.Elapsed.TotalSeconds -lt $DurationSeconds) {
    Start-Sleep -Seconds $IntervalSeconds
    try { $proc = Get-Process -Id $ProcessId -ErrorAction Stop } catch { break }
    if ($proc.ProcessName -ne 'omsi-server') { break }
    $elapsed = $watch.Elapsed.TotalSeconds
    $seconds = $elapsed - $lastTime
    if ($seconds -le 0) { continue }
    $percentOneCore = [Math]::Max(0, 100.0 * ($proc.CPU - $lastCpu) / $seconds)
    $percentSystem = $percentOneCore / [Math]::Max(1, $logicalCores)
    $rss = $proc.WorkingSet64 / 1MB
    $line = ('{0},{1:F3},{2:F3},{3:F3},{4:F3}' -f ((Get-Date).ToUniversalTime().ToString('o')), $elapsed, $percentOneCore, $percentSystem, $rss)
    [System.IO.File]::AppendAllText($csv, $line + "`n", $utf8)
    $lastTime = $elapsed
    $lastCpu = $proc.CPU
}
Write-Host "Saved process samples to $csv"
