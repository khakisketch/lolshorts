[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$InputCsv,

    [string]$OutputPath = ""
)

$ErrorActionPreference = "Stop"

function Is-Yes {
    param([object]$Value)
    return ([string]$Value).Trim().ToLowerInvariant() -in @("yes", "y", "true", "1")
}

$rows = @(Import-Csv -LiteralPath $InputCsv)
$labeled = @($rows | Where-Object { -not [string]::IsNullOrWhiteSpace($_.keepWorthy) })
if ($labeled.Count -lt 30) {
    throw "At least 30 labeled clips are required before tuning; found $($labeled.Count)."
}

$kept = @($labeled | Where-Object { Is-Yes $_.keepWorthy }).Count
$duplicateCount = 0
$duplicateGroups = @($labeled | Where-Object { -not [string]::IsNullOrWhiteSpace($_.duplicateGroup) } | Group-Object duplicateGroup)
foreach ($group in $duplicateGroups) {
    $duplicateCount += [math]::Max(0, $group.Count - 1)
}
$trimProblems = @($labeled | Where-Object { (Is-Yes $_.missingLeadIn) -or (Is-Yes $_.excessiveTail) }).Count

$issueDefinitions = [ordered]@{
    missingLeadIn = "Missing lead-in"
    excessiveTail = "Excessive tail"
    eventMisclassified = "Event misclassification"
    videoIssue = "Video issue"
    audioIssue = "Audio issue"
}
$repeatedIssues = New-Object System.Collections.Generic.List[string]
foreach ($entry in $issueDefinitions.GetEnumerator()) {
    $propertyName = [string]$entry.Key
    $count = @($labeled | Where-Object {
        Is-Yes $_.PSObject.Properties[$propertyName].Value
    }).Count
    if ($count -ge 3) { $repeatedIssues.Add("- $($entry.Value): $count clips") | Out-Null }
}

$keepRate = [math]::Round(($kept / $labeled.Count) * 100.0, 1)
$duplicateRate = [math]::Round(($duplicateCount / $labeled.Count) * 100.0, 1)
$trimRate = [math]::Round(($trimProblems / $labeled.Count) * 100.0, 1)
$qualityPassed = $keepRate -ge 70 -and $duplicateRate -le 10 -and $trimRate -le 5
$report = @(
    "# Highlight Quality Label Summary",
    "",
    "- Overall quality gate: $(if ($qualityPassed) { '**PASS**' } else { '**FAIL**' })",
    "- Labeled clips: $($labeled.Count)",
    "- Keep-worthy rate: $keepRate% (target >= 70%)",
    "- Duplicate rate: $duplicateRate% (target <= 10%)",
    "- Lead/tail trim problem rate: $trimRate% (target <= 5%)",
    "",
    "## Repeated issues eligible for tuning (minimum 3 clips)",
    ""
)
if ($repeatedIssues.Count -eq 0) {
    $report += "- None. Do not change highlight score, merge windows, or pre/post timing."
} else {
    $report += $repeatedIssues
}

if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = [System.IO.Path]::ChangeExtension((Resolve-Path -LiteralPath $InputCsv).Path, ".summary.md")
}
Set-Content -LiteralPath $OutputPath -Value $report -Encoding utf8
Write-Host "Highlight label summary written to: $OutputPath"
if (-not $qualityPassed) { exit 1 }
