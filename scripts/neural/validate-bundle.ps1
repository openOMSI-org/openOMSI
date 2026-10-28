param(
    [Parameter(Mandatory)][string] $Runtime,
    [string] $NeuralHost = '',
    [ValidateSet('256','512','720p')][string] $Resolution = '256',
    [switch] $Replay,
    [string] $Project = '',
    [switch] $AcceptRuntimeLicenseAndExperimentalLdr,
    [switch] $AcceptExperimentalNgx18
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path "$PSScriptRoot/../..").Path
$config = Get-Content -Raw (Join-Path $root 'validation.json') | ConvertFrom-Json
if (-not $NeuralHost) { $NeuralHost = Join-Path $root $config.host }
# Integrity inspection is independent of whether this host was linked with NGX.
& "$PSScriptRoot/diagnose-windows.ps1" -NeuralHost $neuralHost -Runtime $Runtime -Backend ngx -VerifyOnly
if ($Replay) {
    $actualInfo = & $NeuralHost --build-info | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or -not $actualInfo.ngx_compiled -or $actualInfo.msvc -le 0) { throw 'This host is diagnostic-only. Build the included C++ sources with native MSVC/NGX, then select the resulting executable with -NeuralHost.' }
    if (-not $Project -or -not $AcceptRuntimeLicenseAndExperimentalLdr -or -not $AcceptExperimentalNgx18) { throw 'Replay requires Project and both explicit runtime/profile acknowledgements.' }
    $capture = Join-Path $root $config.captures.$Resolution
    & "$PSScriptRoot/test-runtime.ps1" -NeuralHost $neuralHost -Runtime $Runtime -Capture $capture -Project $Project -Backend ngx -AcceptRuntimeLicenseAndExperimentalLdr -AcceptExperimentalNgx18
}
