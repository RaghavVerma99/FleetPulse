param(
  [string]$DatabaseUrl = $env:DATABASE_URL
)
if (-not $DatabaseUrl) { throw "DATABASE_URL is required" }
$psql = Get-Command psql -ErrorAction SilentlyContinue
if (-not $psql) { throw "psql not found; install Postgres client tools or run seed.sql manually" }
psql "$DatabaseUrl" -f "$PSScriptRoot/seed.sql"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "Seeded demo device 00000000-0000-0000-0000-000000000001"