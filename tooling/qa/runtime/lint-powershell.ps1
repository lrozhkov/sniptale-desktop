param(
    [Parameter(Mandatory = $true)][string]$Repository,
    [Parameter(Mandatory = $true)][string]$ModulePath,
    [Parameter(Mandatory = $true)][string]$Inventory
)
$ErrorActionPreference = 'Stop'
Import-Module -Name $ModulePath -ErrorAction Stop
$settings = Import-PowerShellDataFile -LiteralPath (Join-Path -Path $Repository -ChildPath 'tooling/configs/qa/PSScriptAnalyzer.psd1')
# The Python coordinator merges real Framework APIs into the pinned upstream profile.
$settings.Rules.PSUseCompatibleTypes.TargetProfiles = @((Join-Path -Path $Repository -ChildPath '.tmp/qa-reports/windows51-profile.json'))
$files = Get-Content -LiteralPath $Inventory | ConvertFrom-Json
$failed = $false
foreach ($relative in $files) {
    $file = Join-Path -Path $Repository -ChildPath $relative
    $tokens = $null
    $parseErrors = $null
    $null = [System.Management.Automation.Language.Parser]::ParseFile($file, [ref]$tokens, [ref]$parseErrors)
    if ($parseErrors.Count -gt 0) {
        Write-Output ('PowerShellSyntax: {0} parse errors' -f $parseErrors.Count)
        $parseErrors | Format-Table -AutoSize
        $failed = $true
        continue
    }
    $findings = @(Invoke-ScriptAnalyzer -Path $file -Settings $settings -ErrorAction Stop)
    if ($findings.Count -gt 0) {
        foreach ($finding in $findings) {
            Write-Output ('{0}:{1}: {2}: {3}' -f $relative, $finding.Line, $finding.RuleName, $finding.Message)
        }
        $failed = $true
    }
}
if ($failed) { exit 1 }
