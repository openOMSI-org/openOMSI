param(
    [Parameter(Mandatory)][string] $StreamlineSdk,
    [string] $NgxSdk = '',
    [string] $Generator = 'Visual Studio 17 2022',
    [string] $BuildDirectory = '',
    [switch] $StreamlineOnly,
    [switch] $AcceptSdkLicense,
    [switch] $CheckRust,
    [switch] $Offline
)
. "$PSScriptRoot/common.ps1"
Assert-WindowsX64
if (-not $AcceptSdkLicense) { throw 'Review the local NVIDIA SDK license and pass -AcceptSdkLicense before compiling against it.' }
Get-Command cmake -ErrorAction Stop | Out-Null
$sdk = (Resolve-Path $StreamlineSdk).Path
Assert-NeuralFile (Join-Path $sdk 'include/sl.h')
if (-not $NgxSdk) { $NgxSdk = Join-Path $sdk 'external/ngx-sdk' }
$root = (Resolve-Path "$PSScriptRoot/../..").Path
$build = if ($BuildDirectory) { [IO.Path]::GetFullPath($BuildDirectory) } else { Join-Path $root 'target/neural-windows' }
$logs = New-NeuralLogDirectory 'build'
$ngx = if ($StreamlineOnly) { 'OFF' } else { 'ON' }
if (-not $StreamlineOnly) {
    Assert-NeuralFile (Join-Path $NgxSdk 'include/nvsdk_ngx.h')
    $libraries = @((Join-Path $NgxSdk 'lib/Windows_x86_64/nvsdk_ngx_d.lib'), (Join-Path $NgxSdk 'lib/Windows_x86_64/x64/nvsdk_ngx_d.lib'))
    if (-not ($libraries | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf })) { throw "Missing official NGX MSVC library under $NgxSdk" }
}
Invoke-NeuralLogged cmake @('-S', (Join-Path $root 'tools/neural-host'), '-B', $build, '-G', $Generator, '-A', 'x64', "-DSTREAMLINE_SDK=$sdk", "-DNGX_SDK=$NgxSdk", "-DOPENOMSI_ENABLE_NGX=$ngx") (Join-Path $logs 'configure.log')
Invoke-NeuralLogged cmake @('--build', $build, '--config', 'Release', '--parallel', '2') (Join-Path $logs 'build.log')
Invoke-NeuralLogged ctest @('--test-dir', $build, '-C', 'Release', '--output-on-failure') (Join-Path $logs 'tests.log')
$nativeHost = Join-Path $build 'Release/openomsi-neural-host.exe'
$infoText = & $nativeHost --build-info
if ($LASTEXITCODE -ne 0) { throw 'Host build-info failed.' }
$infoText | Set-Content -Encoding utf8 (Join-Path $logs 'host-build-info.json')
$info = $infoText | ConvertFrom-Json
if ($info.msvc -le 0 -or -not $info.windows_x64 -or (-not $StreamlineOnly -and -not $info.ngx_compiled)) { throw 'Build did not produce the requested native MSVC/x64/NGX host.' }
if ($CheckRust) {
    Get-Command cargo -ErrorAction Stop | Out-Null
    $cargoArgs = @('check', '--manifest-path', (Join-Path $root 'Cargo.toml'), '--workspace', '--target', 'x86_64-pc-windows-msvc', '--features', 'omsi-app/neural-external', '--locked')
    if ($Offline) { $cargoArgs += '--offline' }
    Invoke-NeuralLogged cargo $cargoArgs (Join-Path $logs 'rust.log')
}
Write-Host "Host: $(Join-Path $build 'Release/openomsi-neural-host.exe')"
Write-Host "Logs: $logs"
