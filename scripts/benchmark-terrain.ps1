# Run from a checkout containing the terrain review fixes. Requires git and Rust.
param(
    [Parameter(Mandatory=$true)][string]$OmsiRoot,
    [Parameter(Mandatory=$true)][string]$GlobalCfg,
    [string]$Tile = 'auto',
    [string]$Date = '20261001',
    [string]$Csv = 'terrain-benchmark.csv',
    [string]$Base = 'caf95e8a199e30d5a399bde9afe1c58006a07ea2'
)
$ErrorActionPreference = 'Stop'
$repo = (git rev-parse --show-toplevel).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Run inside the repository' }
git -C $repo diff --quiet HEAD -- Cargo.toml Cargo.lock crates
if ($LASTEXITCODE -ne 0) { throw 'Commit or stash source changes before comparing revisions' }
$baseDir = Join-Path ([IO.Path]::GetTempPath()) ('openomsi-terrain-base-' + [guid]::NewGuid())
$csvPath = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Csv)
$rootPath = (Resolve-Path $OmsiRoot).Path
$globalPath = (Resolve-Path $GlobalCfg).Path
$names = @('TERRAIN_BENCH_ROOT','TERRAIN_BENCH_GLOBAL','TERRAIN_BENCH_TILE','TERRAIN_BENCH_DATE','RAYON_NUM_THREADS','CARGO_TARGET_DIR')
$previous = @{}
foreach ($name in $names) { $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$added = $false
try {
    git worktree add --detach $baseDir $Base
    if ($LASTEXITCODE -ne 0) { throw 'Cannot create baseline worktree; fetch the base commit first' }
    $added = $true
    Copy-Item (Join-Path $repo 'crates/omsi-scene/src/terrain_benchmark.rs') (Join-Path $baseDir 'crates/omsi-scene/src/terrain_benchmark.rs')
    Add-Content (Join-Path $baseDir 'crates/omsi-scene/src/lib.rs') "`n#[cfg(test)]`nmod terrain_benchmark;"
    $env:TERRAIN_BENCH_ROOT = $rootPath
    $env:TERRAIN_BENCH_GLOBAL = $globalPath
    $env:TERRAIN_BENCH_TILE = $Tile
    $env:TERRAIN_BENCH_DATE = $Date
    $env:RAYON_NUM_THREADS = '1'
    $env:CARGO_TARGET_DIR = Join-Path $repo 'target'
    'revision,x,y,run,prepare_ms,terrain_vertices,terrain_triangles' | Set-Content $csvPath
    foreach ($entry in @(@('before', $baseDir), @('after', $repo))) {
        Push-Location $entry[1]
        try {
            # Windows PowerShell 5 treats redirected native stderr as ErrorRecords.
            # Cargo writes successful build progress to stderr; use its exit code.
            $ErrorActionPreference = 'Continue'
            try {
                $log = @(& cargo test --locked --release -p omsi-scene --lib tile_preparation_benchmark -- --ignored --nocapture --test-threads=1 2>&1)
                $exitCode = $LASTEXITCODE
            } finally { $ErrorActionPreference = 'Stop' }
            $log | ForEach-Object { Write-Host $_ }
            if ($exitCode -ne 0) { throw ('Benchmark failed: ' + $entry[0]) }
            $rows = @($log | ForEach-Object { if ($_ -match '^TERRAIN_BENCH,(.*)$') { $entry[0] + ',' + $Matches[1] } })
            if ($rows.Count -ne 7) { throw 'Expected seven measurement rows' }
            $rows | Add-Content $csvPath
        } finally { Pop-Location }
    }
    foreach ($group in (Import-Csv $csvPath | Group-Object revision)) {
        $times = @($group.Group | ForEach-Object { [double]::Parse($_.prepare_ms, [Globalization.CultureInfo]::InvariantCulture) } | Sort-Object)
        $counts = @($group.Group.terrain_vertices | Select-Object -Unique)
        if ($counts.Count -ne 1) { throw 'Terrain vertex count varied between runs' }
        Write-Host ("{0}: median {1:F3} ms, min {2:F3}, max {3:F3}; {4} terrain vertices" -f $group.Name, $times[3], $times[0], $times[6], $counts[0])
    }
    Write-Host "Before: $Base"
    Write-Host ("After: " + (git -C $repo rev-parse HEAD))
    Write-Host "Results: $csvPath"
} finally {
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
    if ($added) { git -C $repo worktree remove --force $baseDir }
}
