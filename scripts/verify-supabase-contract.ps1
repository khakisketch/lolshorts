[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$SupabaseDir = Join-Path $ProjectRoot "supabase"
$MigrationDir = Join-Path $SupabaseDir "migrations"
$LegacySchema = Join-Path $SupabaseDir "schema.sql"
$ConfigPath = Join-Path $SupabaseDir "config.toml"
$DatabaseTests = Join-Path $SupabaseDir "tests\database"
$Failures = [System.Collections.Generic.List[string]]::new()

function Assert-Contract {
    param(
        [Parameter(Mandatory = $true)][bool]$Condition,
        [Parameter(Mandatory = $true)][string]$Message
    )

    if ($Condition) {
        Write-Host "PASS: $Message"
    } else {
        Write-Host "FAIL: $Message" -ForegroundColor Red
        $Failures.Add($Message)
    }
}

Assert-Contract (-not (Test-Path -LiteralPath $LegacySchema)) "legacy supabase/schema.sql is absent"
Assert-Contract (Test-Path -LiteralPath $MigrationDir -PathType Container) "authoritative migration directory exists"

$Migrations = @(Get-ChildItem -LiteralPath $MigrationDir -Filter "*.sql" -File | Sort-Object Name)
Assert-Contract ($Migrations.Count -ge 6) "ordered migration chain is present"

$MigrationSql = ($Migrations | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw }) -join "`n"
$ExecutableSql = [regex]::Replace($MigrationSql, '(?s)/\*.*?\*/', '')
$ExecutableSql = [regex]::Replace($ExecutableSql, '(?m)--.*$', '')

$ProtectedTables = @(
    "user_profiles",
    "license_tiers",
    "user_licenses",
    "subscriptions",
    "payments",
    "games",
    "clips",
    "auto_edit_results",
    "youtube_uploads",
    "quota_usage",
    "billing_events",
    "auto_edit_usage",
    "auto_edit_quota_consumptions"
)

foreach ($Table in $ProtectedTables) {
    $escapedTable = [regex]::Escape($Table)
    Assert-Contract (
        $ExecutableSql -match "(?is)ALTER\s+TABLE\s+(?:public\.)?$escapedTable\s+ENABLE\s+ROW\s+LEVEL\s+SECURITY"
    ) "RLS is enabled for public.$Table"
}

Assert-Contract (
    $ExecutableSql -notmatch '(?is)GRANT\s+ALL(?:\s+PRIVILEGES)?\s+ON\s+[^;]+\s+TO\s+(anon|authenticated|public)\b'
) "no broad GRANT ALL exists for exposed client roles"

$QuotaSignature = 'public\.consume_auto_edit_quota\s*\(\s*UUID\s*,\s*TEXT\s*,\s*INTEGER\s*,\s*TEXT\s*\)'
Assert-Contract (
    $ExecutableSql -match "(?is)REVOKE\s+ALL\s+ON\s+FUNCTION\s+$QuotaSignature\s+FROM\s+PUBLIC"
) "quota RPC execute is revoked from PUBLIC"
Assert-Contract (
    $ExecutableSql -match "(?is)REVOKE\s+ALL\s+ON\s+FUNCTION\s+$QuotaSignature\s+FROM\s+authenticated"
) "quota RPC execute is revoked from authenticated"
Assert-Contract (
    $ExecutableSql -match "(?is)GRANT\s+EXECUTE\s+ON\s+FUNCTION\s+$QuotaSignature\s+TO\s+service_role"
) "quota RPC execute is granted only to service_role"

$Config = Get-Content -LiteralPath $ConfigPath -Raw
Assert-Contract ($Config -match '(?ms)^\[functions\.billing\]\s*.*?^enabled\s*=\s*false\s*$') "deferred billing function is disabled"
Assert-Contract ($Config -match '(?ms)^\[functions\.quota\]\s*.*?^enabled\s*=\s*true\s*$') "quota function is enabled"
Assert-Contract ($Config -match '(?ms)^\[functions\.quota\]\s*.*?^verify_jwt\s*=\s*true\s*$') "quota function requires a user JWT at the platform boundary"

$PgTapTests = @(Get-ChildItem -LiteralPath $DatabaseTests -Filter "*.sql" -File -ErrorAction SilentlyContinue)
Assert-Contract ($PgTapTests.Count -ge 2) "pgTAP schema and RLS/quota tests are checked in"

if ($Failures.Count -gt 0) {
    throw "Supabase contract verification failed with $($Failures.Count) issue(s)."
}

Write-Host "Supabase static contract verification passed. Run 'supabase test db' for database-level proof."
