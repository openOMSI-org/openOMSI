param(
    [Parameter(Mandatory)][string] $NeuralHost,
    [Parameter(Mandatory)][string] $Runtime,
    [Parameter(Mandatory)][string] $Capture,
    [Parameter(Mandatory)][string] $Project,
    [ValidateSet('auto','streamline','ngx')][string] $Backend = 'auto',
    [switch] $AcceptRuntimeLicenseAndExperimentalLdr,
    [switch] $AcceptExperimentalNgx18
)
. "$PSScriptRoot/common.ps1"
Assert-WindowsX64
Assert-NeuralFile $NeuralHost
if (-not $AcceptRuntimeLicenseAndExperimentalLdr) { throw 'Review runtime terms and pass -AcceptRuntimeLicenseAndExperimentalLdr.' }
$logs = New-NeuralLogDirectory 'replay'
Invoke-NeuralLogged $NeuralHost @('--self-test') (Join-Path $logs 'self-test.log')
Invoke-NeuralLogged $NeuralHost @('--inspect-frame', $Capture) (Join-Path $logs 'inputs.log')
Invoke-NeuralLogged $NeuralHost @('--diagnose', '--runtime', $Runtime) (Join-Path $logs 'diagnose.log')
$output = Join-Path $logs 'output'
$nativeArgs = @('--replay-frame', $Capture, '--runtime', $Runtime, '--backend', $Backend, '--project', $Project, '--output', $output, '--accept-runtime-license-and-experimental-ldr')
if ($AcceptExperimentalNgx18) { $nativeArgs += '--accept-experimental-ngx18' }
Invoke-NeuralLogged $NeuralHost $nativeArgs (Join-Path $logs 'replay.log')
$report = Get-Content -Raw (Join-Path $output 'report.json') | ConvertFrom-Json
foreach ($name in @('RUNTIME FOUND','RUNTIME VERIFIED','RUNTIME LOADED','GPU SUPPORTED','FEATURE INITIALIZED','RESOURCES ALLOCATED','EVALUATION SUBMITTED','GPU COMPLETED','OUTPUT READ','OUTPUT VALIDATED')) {
    if ($report.stages.$name -ne $true) { throw "Neural execution unproven: $name" }
}
Write-Host "Runtime evaluation completed. Inspect raw RGBA, BMP and per-frame reports in $output. Image quality and in-game presentation remain unqualified."
