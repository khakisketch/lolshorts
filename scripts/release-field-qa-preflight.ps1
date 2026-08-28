<#
LoLShorts release and field QA preflight.

This script collects local automated evidence for a candidate build and writes a
Markdown report. It does not claim field readiness by itself; real LoL, LCU,
YouTube, installer, updater, GPU, audio, and support evidence must still be
recorded by a human tester.
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $false)]
    [string]$OutputDir = "",

    [Parameter(Mandatory = $false)]
    [switch]$SkipCommandChecks,

    [Parameter(Mandatory = $false)]
    [switch]$RunInstallerValidation,

    [Parameter(Mandatory = $false)]
    [string]$InstallerPath = ""
)

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $OutputDir = Join-Path $ProjectRoot "qa-evidence"
}

$Timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$RunDir = Join-Path $OutputDir "release-preflight-$Timestamp"
$ReportPath = Join-Path $RunDir "release-preflight.md"
$Results = New-Object System.Collections.Generic.List[object]

function New-SafeFileName {
    param([string]$Value)

    return ($Value -replace '[^A-Za-z0-9_.-]+', '-').Trim('-')
}

function Add-Result {
    param(
        [string]$Area,
        [string]$Check,
        [ValidateSet("PASS", "FAIL", "WARN", "SKIP")]
        [string]$Status,
        [string]$Evidence,
        [string]$NextAction = ""
    )

    $Results.Add([pscustomobject]@{
        Area       = $Area
        Check      = $Check
        Status     = $Status
        Evidence   = $Evidence
        NextAction = $NextAction
    }) | Out-Null
}

function Invoke-LoggedCommand {
    param(
        [string]$Area,
        [string]$Check,
        [string]$Command,
        [string[]]$Arguments
    )

    if ($SkipCommandChecks) {
        Add-Result $Area $Check "SKIP" "Skipped by -SkipCommandChecks." "Run without -SkipCommandChecks before release sign-off."
        return
    }

    $safeName = New-SafeFileName "$Area-$Check"
    $logPath = Join-Path $RunDir "$safeName.log"
    $stdoutPath = Join-Path $RunDir "$safeName.stdout.tmp"
    $stderrPath = Join-Path $RunDir "$safeName.stderr.tmp"
    Push-Location $ProjectRoot
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        & $Command @Arguments > $stdoutPath 2> $stderrPath
        $exitCode = if ($null -eq $LASTEXITCODE) { 0 } else { $LASTEXITCODE }
        $stdout = if (Test-Path $stdoutPath) { Get-Content -Raw -Path $stdoutPath } else { "" }
        $stderr = if (Test-Path $stderrPath) { Get-Content -Raw -Path $stderrPath } else { "" }
        Set-Content -Path $logPath -Value @(
            "Command: $Command $($Arguments -join ' ')",
            "Exit code: $exitCode",
            "",
            "STDOUT:",
            $stdout,
            "",
            "STDERR:",
            $stderr
        )

        if ($exitCode -eq 0) {
            Add-Result $Area $Check "PASS" "Exit code 0. Log: $logPath"
        } else {
            Add-Result $Area $Check "FAIL" "Exit code $exitCode. Log: $logPath" "Fix the failing command before promoting the build."
        }
    } catch {
        $message = $_.Exception.Message
        Set-Content -Path $logPath -Value $message
        Add-Result $Area $Check "FAIL" "Command failed to start: $message. Log: $logPath" "Fix local toolchain or command wiring."
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
        Pop-Location
        Remove-Item -Path $stdoutPath, $stderrPath -Force -ErrorAction SilentlyContinue
    }
}

function Test-SupabaseSqlGuardrails {
    $migrationDir = Join-Path $ProjectRoot "supabase\migrations"
    $files = @(
        @(Get-ChildItem -LiteralPath $migrationDir -Filter "*.sql" -File -ErrorAction Stop |
            Sort-Object Name |
            Select-Object -ExpandProperty FullName)
        (Join-Path $ProjectRoot "supabase\schema.sql")
    )

    foreach ($file in $files) {
        if (-not (Test-Path $file)) {
            Add-Result "Supabase" "SQL guardrails: $(Split-Path -Leaf $file)" "FAIL" "Missing file: $file" "Restore the expected schema or migration file."
            continue
        }

        $content = Get-Content -Raw -Path $file
        $fileName = Split-Path -Leaf $file
        # Comments often explain forbidden examples (for example SECURITY
        # DEFINER) and must not be counted as executable SQL by this static
        # preflight. This is a conservative lexical pass, not a SQL parser.
        $sqlForChecks = [regex]::Replace($content, '(?s)/\*.*?\*/', '')
        $sqlForChecks = [regex]::Replace($sqlForChecks, '(?m)--.*$', '')

        if ($sqlForChecks -match '(?is)CREATE\s+ROLE\s+[^;]*\bPASSWORD\b') {
            Add-Result "Supabase" "No passworded bootstrap roles: $fileName" "FAIL" "Found CREATE ROLE with PASSWORD." "Move local bootstrap secrets out of production-facing SQL."
        } else {
            Add-Result "Supabase" "No passworded bootstrap roles: $fileName" "PASS" "No CREATE ROLE ... PASSWORD statements found."
        }

        if ($sqlForChecks -match '(?is)GRANT\s+ALL(?:\s+PRIVILEGES)?\s+ON\s+[^;]+\s+TO\s+(anon|authenticated|public)\b') {
            Add-Result "Supabase" "No broad exposed-role grants: $fileName" "FAIL" "Found broad GRANT ALL to anon/authenticated/public." "Replace broad grants with least-privilege grants and matching RLS policies."
        } else {
            Add-Result "Supabase" "No broad exposed-role grants: $fileName" "PASS" "No broad GRANT ALL to anon/authenticated/public found."
        }

        $securityDefinerCount = ([regex]::Matches($sqlForChecks, '(?is)SECURITY\s+DEFINER')).Count
        $searchPathCount = ([regex]::Matches($sqlForChecks, '(?is)SET\s+search_path')).Count
        if ($securityDefinerCount -gt 0 -and $searchPathCount -lt $securityDefinerCount) {
            Add-Result "Supabase" "SECURITY DEFINER search_path: $fileName" "WARN" "Found $securityDefinerCount SECURITY DEFINER entries and $searchPathCount SET search_path entries." "Review every SECURITY DEFINER function before production migration."
        } else {
            Add-Result "Supabase" "SECURITY DEFINER search_path: $fileName" "PASS" "SECURITY DEFINER entries have matching or surplus SET search_path guardrails."
        }

        $createsPublicTable = $sqlForChecks -match '(?is)CREATE\s+TABLE(?:\s+IF\s+NOT\s+EXISTS)?\s+(?:public\.)?\w+'
        if ($sqlForChecks -match '(?is)ENABLE\s+ROW\s+LEVEL\s+SECURITY') {
            Add-Result "Supabase" "RLS statements present: $fileName" "PASS" "ENABLE ROW LEVEL SECURITY statements found."
        } elseif ($createsPublicTable) {
            Add-Result "Supabase" "RLS statements present: $fileName" "WARN" "No ENABLE ROW LEVEL SECURITY statement found." "Confirm this file is not intended to define exposed public tables."
        } else {
            Add-Result "Supabase" "RLS statements present: $fileName" "PASS" "Migration does not create a public table; no new RLS enablement is required."
        }
    }
}

function Test-YoutubeEnvironment {
    $sampleFiles = @(
        (Join-Path $ProjectRoot ".env.example"),
        (Join-Path $ProjectRoot ".env.production.example")
    ) | Where-Object { Test-Path $_ }

    $expectedVars = @(
        "VITE_SUPABASE_URL",
        "VITE_SUPABASE_ANON_KEY",
        "SUPABASE_URL",
        "SUPABASE_ANON_KEY",
        "YOUTUBE_CLIENT_ID",
        "YOUTUBE_CLIENT_SECRET",
        "YOUTUBE_REDIRECT_URI"
    )

    foreach ($varName in $expectedVars) {
        $presentInSamples = $false
        foreach ($file in $sampleFiles) {
            $content = Get-Content -Raw -Path $file
            if ($content -match "(?m)^\s*$([regex]::Escape($varName))\s*=") {
                $presentInSamples = $true
                break
            }
        }

        if ($presentInSamples) {
            Add-Result "Environment" "Sample env declares $varName" "PASS" "Variable name appears in checked-in env examples; value not printed."
        } else {
            Add-Result "Environment" "Sample env declares $varName" "WARN" "Variable name is not present in checked-in env examples." "Add the non-secret variable name to env examples if it is required for release setup."
        }
    }

    $redirectUri = [Environment]::GetEnvironmentVariable("YOUTUBE_REDIRECT_URI")
    if ([string]::IsNullOrWhiteSpace($redirectUri)) {
        Add-Result "Environment" "Current YouTube redirect URI" "SKIP" "YOUTUBE_REDIRECT_URI is not set in the current process." "Set it for real OAuth field QA."
    } elseif ($redirectUri -match '^http://(localhost|127\.0\.0\.1)(:\d+)?(/|$)') {
        Add-Result "Environment" "Current YouTube redirect URI" "PASS" "YOUTUBE_REDIRECT_URI is loopback HTTP; value not printed."
    } else {
        Add-Result "Environment" "Current YouTube redirect URI" "FAIL" "YOUTUBE_REDIRECT_URI is not an accepted loopback HTTP URI; value not printed." "Use http://localhost:<port>/... or http://127.0.0.1:<port>/..."
    }
}

function Test-FieldPrerequisites {
    if ([System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT) {
        Add-Result "Field QA" "Windows host" "PASS" "Running on Windows $([System.Environment]::OSVersion.Version)."
    } else {
        Add-Result "Field QA" "Windows host" "WARN" "This script is not running on Windows." "Run final field QA on a real Windows machine."
    }

    $leagueProcesses = Get-Process -Name "LeagueClient", "LeagueClientUx", "League of Legends" -ErrorAction SilentlyContinue
    if ($leagueProcesses.Count -gt 0) {
        Add-Result "Field QA" "League client process" "PASS" "Detected League-related process names: $($leagueProcesses.ProcessName -join ', ')."
    } else {
        Add-Result "Field QA" "League client process" "WARN" "No League client process detected." "Run live LCU and recording checks with League Client open."
    }

    $binariesDir = Join-Path $ProjectRoot "src-tauri\binaries"
    $ffmpegCandidates = @()
    $ffprobeCandidates = @()
    if (Test-Path $binariesDir) {
        $ffmpegCandidates = @(Get-ChildItem -Path $binariesDir -File -Filter "ffmpeg*" -ErrorAction SilentlyContinue)
        $ffprobeCandidates = @(Get-ChildItem -Path $binariesDir -File -Filter "ffprobe*" -ErrorAction SilentlyContinue)
    }

    if ($ffmpegCandidates.Count -gt 0 -and $ffprobeCandidates.Count -gt 0) {
        Add-Result "Field QA" "Bundled FFmpeg sidecars" "PASS" "Found bundled FFmpeg and ffprobe sidecar candidate(s); exact path hidden from public release notes."
    } elseif ($ffmpegCandidates.Count -gt 0) {
        Add-Result "Field QA" "Bundled FFmpeg sidecars" "WARN" "Found FFmpeg sidecar candidate but no ffprobe sidecar candidate." "Run src-tauri/build_scripts/prepare_ffmpeg.ps1 before installer field QA."
    } else {
        Add-Result "Field QA" "Bundled FFmpeg sidecars" "WARN" "No bundled FFmpeg sidecar candidate found in src-tauri/binaries." "Run src-tauri/build_scripts/prepare_ffmpeg.ps1 before installer field QA."
    }

    $bundleDir = Join-Path $ProjectRoot "src-tauri\target\release\bundle"
    $msiDir = Join-Path $bundleDir "msi"
    $nsisDir = Join-Path $bundleDir "nsis"
    $hasInstaller = (Test-Path $msiDir) -or (Test-Path $nsisDir) -or (-not [string]::IsNullOrWhiteSpace($InstallerPath))
    if ($hasInstaller) {
        Add-Result "Field QA" "Installer artifact candidate" "PASS" "Installer path or bundle directory exists."
    } else {
        Add-Result "Field QA" "Installer artifact candidate" "WARN" "No release installer artifact found." "Run a release build before installer/updater field QA."
    }
}

function Invoke-InstallerValidation {
    if (-not $RunInstallerValidation) {
        Add-Result "Installer" "Installer validation script" "SKIP" "Skipped by default." "Run with -RunInstallerValidation after release artifacts exist."
        return
    }

    $scriptPath = Join-Path $ProjectRoot "tests\installer\validate-installer.ps1"
    if (-not (Test-Path $scriptPath)) {
        Add-Result "Installer" "Installer validation script" "FAIL" "Missing $scriptPath" "Restore installer validation script."
        return
    }

    $args = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $scriptPath, "-NonInteractive")
    if (-not [string]::IsNullOrWhiteSpace($InstallerPath)) {
        $args += @("-InstallerPath", $InstallerPath)
    }

    Invoke-LoggedCommand "Installer" "Installer validation script" "powershell" $args
}

function Write-Report {
    $failCount = @($Results | Where-Object { $_.Status -eq "FAIL" }).Count
    $warnCount = @($Results | Where-Object { $_.Status -eq "WARN" }).Count
    $skipCount = @($Results | Where-Object { $_.Status -eq "SKIP" }).Count

    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add("# LoLShorts Release Field QA Preflight") | Out-Null
    $lines.Add("") | Out-Null
    $lines.Add("- Generated: $(Get-Date -Format o)") | Out-Null
    $lines.Add("- Project root: $ProjectRoot") | Out-Null
    $lines.Add("- Result summary: $failCount fail, $warnCount warn, $skipCount skipped") | Out-Null
    $lines.Add("") | Out-Null
    $lines.Add("This report is local automated evidence only. It does not replace the E5 run in docs/E5_FIELD_QA_PACKET.md or the readiness rows in docs/FIELD_QA_COMMERCIAL_READINESS.md.") | Out-Null
    $lines.Add("") | Out-Null
    $lines.Add("| Area | Check | Status | Evidence | Next action |") | Out-Null
    $lines.Add("| --- | --- | --- | --- | --- |") | Out-Null

    foreach ($result in $Results) {
        $evidence = ($result.Evidence -replace '\|', '\|')
        $nextAction = ($result.NextAction -replace '\|', '\|')
        $lines.Add("| $($result.Area) | $($result.Check) | $($result.Status) | $evidence | $nextAction |") | Out-Null
    }

    Set-Content -Path $ReportPath -Value $lines -Encoding UTF8
    Write-Host "Release preflight report: $ReportPath"

    if ($failCount -gt 0) {
        exit 1
    }
}

New-Item -ItemType Directory -Force -Path $RunDir | Out-Null

Invoke-LoggedCommand "Frontend" "TypeScript typecheck" "npm" @("run", "typecheck")
Invoke-LoggedCommand "Frontend" "ESLint" "npm" @("run", "lint")
Invoke-LoggedCommand "Frontend" "Production build" "npm" @("run", "build")
Invoke-LoggedCommand "Frontend" "Jest unit tests" "npm" @("run", "test:unit")
Invoke-LoggedCommand "Frontend" "Node dependency audit" "npm" @("run", "audit:all")
Invoke-LoggedCommand "Rust" "Formatting" "cargo" @("fmt", "--manifest-path", "src-tauri\Cargo.toml", "--all", "--", "--check")
Invoke-LoggedCommand "Rust" "Clippy" "cargo" @("clippy", "--manifest-path", "src-tauri\Cargo.toml", "--all-targets", "--", "-D", "warnings")
Invoke-LoggedCommand "Rust" "Tests" "cargo" @("test", "--manifest-path", "src-tauri\Cargo.toml")
Invoke-LoggedCommand "Rust" "Dependency audit" "cargo" @("audit", "--file", "Cargo.lock")
Invoke-LoggedCommand "Field QA" "Evidence tool fail-closed tests" "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "scripts/test-field-evidence-tools.ps1")
Invoke-LoggedCommand "Repository" "Diff whitespace check" "git" @("diff", "--check")

Test-SupabaseSqlGuardrails
Test-YoutubeEnvironment
Test-FieldPrerequisites
Invoke-InstallerValidation
Write-Report
