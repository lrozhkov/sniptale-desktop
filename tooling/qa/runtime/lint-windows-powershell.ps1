param(
    [Parameter(Mandatory = $true)][string]$Repository,
    [Parameter(Mandatory = $true)][string]$Inventory
)
$ErrorActionPreference = 'Stop'
if ($PSVersionTable.PSVersion.Major -ne 5 -or $PSVersionTable.PSVersion.Minor -ne 1) {
    throw 'Compatibility proof requires Windows PowerShell 5.1'
}
$files = Get-Content -LiteralPath $Inventory | ConvertFrom-Json
$count = 0
foreach ($relative in $files) {
    $tokens = $null
    $parseErrors = $null
    $file = Join-Path -Path $Repository -ChildPath $relative
    $null = [System.Management.Automation.Language.Parser]::ParseFile($file, [ref]$tokens, [ref]$parseErrors)
    if ($parseErrors.Count -gt 0) { throw 'Windows PowerShell 5.1 rejected script syntax' }
    $count++
}
# These are the explicit Add-Type assemblies used by the guest scripts.
Add-Type -AssemblyName System.Security, UIAutomationClient, UIAutomationTypes, System.Windows.Forms, System.Drawing
$addon = Get-Content -Raw -LiteralPath (Join-Path -Path $Repository -ChildPath 'tooling/configs/qa/windows51-type-profile.json') | ConvertFrom-Json
$typeCount = 0
foreach ($property in $addon.types.PSObject.Properties) {
    $type = ([System.Management.Automation.PSTypeName]$property.Name).Type
    if ($null -eq $type) { throw 'A collected framework type is unavailable in Windows PowerShell 5.1' }
    foreach ($scope in @('Static', 'Instance')) {
        $flags = [System.Reflection.BindingFlags]('Public,FlattenHierarchy,' + $scope)
        $members = @($type.GetMembers($flags) | ForEach-Object { $_.Name })
        foreach ($category in @('Methods', 'Fields', 'Properties', 'Events')) {
            $group = $property.Value.data.$scope.$category
            if ($null -eq $group) { continue }
            foreach ($member in $group.PSObject.Properties) {
                if ($members -notcontains $member.Name) { throw 'A collected framework member is unavailable in Windows PowerShell 5.1' }
            }
        }
    }
    $typeCount++
}
Write-Output ('Windows PowerShell {0}: {1} scripts parsed; {2} collected framework types and their members verified' -f $PSVersionTable.PSVersion, $count, $typeCount)
