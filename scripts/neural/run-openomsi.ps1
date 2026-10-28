param(
    [Parameter(Mandatory)][string] $GameExe,
    [string[]] $GameArguments = @(),
    [ValidateSet('Capture','Neural')][string] $Mode = 'Capture',
    [ValidateRange(1,16)][int] $CaptureFrames = 4,
    [string] $NeuralHost = '',
    [string] $Runtime = '',
    [string] $Project = '',
    [ValidateSet('auto','streamline','ngx')][string] $Backend = 'auto',
    [switch] $AcceptRuntimeLicenseAndExperimentalLdr,
    [switch] $AcceptExperimentalNgx18
)
. "$PSScriptRoot/common.ps1"
Assert-WindowsX64
Assert-NeuralFile $GameExe
$logs = New-NeuralLogDirectory 'game'
$names = @('OPENOMSI_NEURAL_CAPTURE_DIR','OPENOMSI_NEURAL_CAPTURE_FRAMES','OPENOMSI_NEURAL_SESSION','OPENOMSI_NEURAL_CONSENT','OPENOMSI_NEURAL_TIMEOUT_MS')
$previous = @{}; foreach ($name in $names) { $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process'); [Environment]::SetEnvironmentVariable($name, $null, 'Process') }
$job = $null
try {
    if ($Mode -eq 'Capture') {
        $env:OPENOMSI_NEURAL_CAPTURE_DIR = Join-Path $logs 'capture'
        $env:OPENOMSI_NEURAL_CAPTURE_FRAMES = "$CaptureFrames"
        Write-Host "Input capture: $env:OPENOMSI_NEURAL_CAPTURE_DIR (main desktop view, 100% render scale, temporal debug off)."
    } else {
        Assert-NeuralFile $NeuralHost
        if (-not $AcceptRuntimeLicenseAndExperimentalLdr -or -not $Project -or -not $Runtime) { throw 'Neural mode requires Runtime, Project and explicit license/LDR acknowledgement. Validate a replay first.' }
        $session = Join-Path $logs 'session'; New-Item -ItemType Directory $session | Out-Null
        $acl = Get-Acl $session; $acl.SetAccessRuleProtection($true, $false)
        $identity = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
        $rule = [System.Security.AccessControl.FileSystemAccessRule]::new($identity, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
        $acl.AddAccessRule($rule); Set-Acl $session $acl
        $nativeArgs = @('--serve','--session',$session,'--runtime',$Runtime,'--project',$Project,'--backend',$Backend,'--output',(Join-Path $logs 'host-output'),'--accept-runtime-license-and-experimental-ldr')
        if ($AcceptExperimentalNgx18) { $nativeArgs += '--accept-experimental-ngx18' }
        $config = @{ Executable=(Resolve-Path $NeuralHost).Path; Arguments=$nativeArgs; Log=(Join-Path $logs 'host.log') }
        $job = Start-Job -ArgumentList $config -ScriptBlock { param($config) $nativeArgs = $config.Arguments; & $config.Executable @nativeArgs *> $config.Log; if ($LASTEXITCODE -ne 0) { throw "Host exited $LASTEXITCODE; see $($config.Log)" } }
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        while (-not (Test-Path (Join-Path $session 'host.ready'))) {
            if ($job.State -ne 'Running') { Receive-Job $job; throw 'Host initialization failed.' }
            if ([DateTime]::UtcNow -gt $deadline) { throw 'Host ready timeout. See host.log.' }
            Start-Sleep -Milliseconds 100
        }
        $env:OPENOMSI_NEURAL_SESSION = $session; $env:OPENOMSI_NEURAL_CONSENT = '1'; $env:OPENOMSI_NEURAL_TIMEOUT_MS = '30000'
    }
    Invoke-NeuralLogged $GameExe $GameArguments (Join-Path $logs 'game.log')
} finally {
    if ($job) {
        New-Item -ItemType File (Join-Path $logs 'session/host.stop') -ErrorAction SilentlyContinue | Out-Null
        Wait-Job $job -Timeout 35 | Out-Null
        if ($job.State -eq 'Running') { Stop-Job $job }
        Receive-Job $job -ErrorAction Continue; Remove-Job $job
    }
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
    Write-Host "Logs: $logs. Launch normally without these environment variables to restore ordinary rendering."
}
