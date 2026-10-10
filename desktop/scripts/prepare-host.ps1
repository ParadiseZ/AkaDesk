param([ValidateSet('debug', 'release')][string]$Profile = 'release')
$ErrorActionPreference = 'Stop'
$desktopRoot = Split-Path $PSScriptRoot -Parent
$projectRoot = Split-Path $desktopRoot -Parent
$runtimeDir = Join-Path $desktopRoot 'src-tauri/resources/host'
$sdkBin = Join-Path $projectRoot 'third_party/ffmpe-win64-gpl-shared/bin'
if (!(Test-Path -LiteralPath $sdkBin)) { throw "FFmpeg SDK missing: $sdkBin" }
Push-Location (Join-Path $desktopRoot 'host')
try {
    if ($Profile -eq 'release') { cargo build --locked --release -p host-runtime }
    else { cargo build --locked -p host-runtime }
    if ($LASTEXITCODE -ne 0) { throw 'Host runtime build failed' }
} finally { Pop-Location }
New-Item -ItemType Directory -Path $runtimeDir -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $desktopRoot "host/target/$Profile/host-runtime.exe") -Destination $runtimeDir -Force
Get-ChildItem -LiteralPath $sdkBin -Filter '*.dll' -File | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination $runtimeDir -Force
}
Write-Output "Host runtime and FFmpeg DLLs prepared in $runtimeDir"
