param(
    [Parameter(Mandatory = $true)][string]$Repository,
    [Parameter(Mandatory = $true)][string]$ModulePath,
    [Parameter(Mandatory = $true)][string]$Inventory
)
$ErrorActionPreference = 'Stop'
Import-Module -Name $ModulePath -ErrorAction Stop
$settings = Join-Path -Path $Repository -ChildPath 'tooling/configs/qa/PSScriptAnalyzer.psd1'
$files = Get-Content -LiteralPath $Inventory | ConvertFrom-Json
$failed = $false
foreach ($relative in $files) {
    $file = Join-Path -Path $Repository -ChildPath $relative
    $tokens = $null
    $parseErrors = $null
    $null = [System.Management.Automation.Language.Parser]::ParseFile($file, [ref]$tokens, [ref]$parseErrors)
    if ($parseErrors.Count -gt 0) {
        $parseErrors | Format-Table -AutoSize
        $failed = $true
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
