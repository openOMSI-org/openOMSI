param(
    [Parameter(Mandatory)][string] $NeuralHost,
    [Parameter(Mandatory)][string] $Runtime,
    [ValidateSet('auto','streamline','ngx')][string] $Backend = 'auto',
    [switch] $Initialize,
    [switch] $VerifyOnly,
    [string] $Project = '',
    [switch] $AcceptRuntimeLicenseAndExperimentalLdr,
    [switch] $AcceptExperimentalNgx18
)
. "$PSScriptRoot/common.ps1"
Assert-WindowsX64
Assert-NeuralFile $NeuralHost
$logs = New-NeuralLogDirectory 'diagnose'
Invoke-NeuralLogged $NeuralHost @('--self-test') (Join-Path $logs 'self-test.log')
Invoke-NeuralLogged $NeuralHost @('--diagnose', '--runtime', $Runtime) (Join-Path $logs 'diagnose.log')
if ($VerifyOnly) {
    Invoke-NeuralLogged $NeuralHost @('--verify-runtime', '--runtime', $Runtime, '--backend', $Backend, '--output', (Join-Path $logs 'integrity')) (Join-Path $logs 'integrity.log')
}
if ($Initialize) {
    if (-not $AcceptRuntimeLicenseAndExperimentalLdr -or -not $Project) { throw 'Initialization requires -Project and -AcceptRuntimeLicenseAndExperimentalLdr.' }
    $nativeArgs = @('--validate-runtime', '--runtime', $Runtime, '--backend', $Backend, '--project', $Project, '--output', (Join-Path $logs 'initialization'), '--accept-runtime-license-and-experimental-ldr')
    if ($AcceptExperimentalNgx18) { $nativeArgs += '--accept-experimental-ngx18' }
    Invoke-NeuralLogged $NeuralHost $nativeArgs (Join-Path $logs 'initialize.log')
}
Write-Host "Logs: $logs"
