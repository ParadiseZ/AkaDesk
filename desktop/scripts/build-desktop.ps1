$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'prepare-host.ps1')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
pnpm --dir (Join-Path (Split-Path $PSScriptRoot -Parent) 'ui') build
exit $LASTEXITCODE
