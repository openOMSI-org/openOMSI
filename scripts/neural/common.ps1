Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
function Assert-WindowsX64 {
    if (-not [Environment]::Is64BitProcess -or [Environment]::OSVersion.Platform -ne 'Win32NT') {
        throw 'Run in native Windows x64 PowerShell.'
    }
}
function New-NeuralLogDirectory([string] $Prefix) {
    $root = Join-Path $PSScriptRoot '../../target/neural-logs'
    New-Item -ItemType Directory -Force $root | Out-Null
    $directory = Join-Path $root ($Prefix + '-' + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-fff') + '-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory $directory | Out-Null
    return (Resolve-Path $directory).Path
}
function Invoke-NeuralLogged([string] $Executable, [string[]] $Arguments, [string] $Log) {
    if (Test-Path $Log) { throw "Refusing to overwrite log: $Log" }
    & $Executable @Arguments 2>&1 | Tee-Object -FilePath $Log | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "$Executable failed with exit $LASTEXITCODE. See $Log" }
}
function Assert-NeuralFile([string] $Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Missing file: $Path" }
}
