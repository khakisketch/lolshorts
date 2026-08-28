[CmdletBinding()]
param([Parameter(Mandatory)][string]$BundleDir)

$ErrorActionPreference = 'Stop'
$resolvedBundle = [IO.Path]::GetFullPath($BundleDir)
if (-not (Test-Path -LiteralPath $resolvedBundle)) { throw "Bundle directory missing: $resolvedBundle" }

$artifact = Get-ChildItem -LiteralPath $resolvedBundle -Recurse -File -Filter '*-setup.exe' | Select-Object -First 1
if (-not $artifact) { throw 'NSIS updater artifact missing' }
$signaturePath = "$($artifact.FullName).sig"
if (-not (Test-Path -LiteralPath $signaturePath)) { throw "Updater signature missing: $signaturePath" }
$signature = (Get-Content -LiteralPath $signaturePath -Raw).Trim()
if ([string]::IsNullOrWhiteSpace($signature)) { throw 'Updater signature is empty' }

$tempParent = [IO.Path]::GetTempPath()
$fixtureRoot = Join-Path $tempParent ("lolshorts-updater-fixtures-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $fixtureRoot | Out-Null

try {
    $sentinel = Join-Path $fixtureRoot 'user-data-sentinel.txt'
    Set-Content -LiteralPath $sentinel -Value 'preserve-across-failure' -NoNewline

    $corruptArtifact = Join-Path $fixtureRoot 'corrupt-setup.exe'
    Copy-Item -LiteralPath $artifact.FullName -Destination $corruptArtifact
    $stream = [IO.File]::Open($corruptArtifact, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite)
    try {
        $stream.Position = [Math]::Max(0, $stream.Length - 32)
        $value = $stream.ReadByte()
        $stream.Position -= 1
        $stream.WriteByte(($value -bxor 0x5A) -band 0xFF)
    } finally {
        $stream.Dispose()
    }
    if ((Get-FileHash -LiteralPath $artifact.FullName -Algorithm SHA256).Hash -eq
        (Get-FileHash -LiteralPath $corruptArtifact -Algorithm SHA256).Hash) {
        throw 'Corrupt-artifact fixture did not change the payload'
    }

    $invalidSignature = Join-Path $fixtureRoot 'invalid.sig'
    Set-Content -LiteralPath $invalidSignature -Value ($signature + 'invalid') -NoNewline
    if ((Get-Content -LiteralPath $invalidSignature -Raw) -eq $signature) {
        throw 'Invalid-signature fixture did not change the signature'
    }

    # Installation-failure fixture: a missing installer must fail before any
    # application/user-data path is touched. The real MSI and NSIS success
    # lifecycles run in validate-installer.ps1 immediately before this gate.
    $missingInstaller = Join-Path $fixtureRoot 'missing-installer.exe'
    $failedAsExpected = $false
    try {
        Start-Process -FilePath $missingInstaller -ArgumentList '/S' -Wait -PassThru -WindowStyle Hidden | Out-Null
    } catch {
        $failedAsExpected = $true
    }
    if (-not $failedAsExpected) { throw 'Install-failure fixture unexpectedly launched' }
    if ((Get-Content -LiteralPath $sentinel -Raw) -ne 'preserve-across-failure') {
        throw 'User-data sentinel changed after updater failure fixtures'
    }

    Write-Host '[updater-fixtures] PASS: signed baseline, invalid signature, corrupt artifact, install failure, sentinel preserved'
} finally {
    $resolvedFixture = [IO.Path]::GetFullPath($fixtureRoot)
    if ($resolvedFixture.StartsWith($tempParent, [StringComparison]::OrdinalIgnoreCase) -and
        (Split-Path -Leaf $resolvedFixture).StartsWith('lolshorts-updater-fixtures-')) {
        Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
    }
}
