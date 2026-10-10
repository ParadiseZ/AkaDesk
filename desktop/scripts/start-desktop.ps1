$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'prepare-host.ps1') -Profile debug
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
pnpm --dir (Join-Path (Split-Path $PSScriptRoot -Parent) 'ui') dev --host 127.0.0.1 --port 1420
exit $LASTEXITCODE
