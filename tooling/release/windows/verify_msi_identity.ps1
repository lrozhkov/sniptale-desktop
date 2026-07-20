param(
  [string] $Msi,
  [string] $ExpectedProductVersion,
  [string] $ExpectedUpgradeCode = "{BC80D869-3328-47B1-BC08-CC5E0FA05961}",
  [switch] $SelfTest
)
$ErrorActionPreference = "Stop"

function Get-RetiredProductRoots {
  @(
    (@("scr", "eenyx") -join ""),
    (@("smart", "capture") -join ""),
    (@("ns", "mp") -join ""),
    (@("s", "cx") -join "")
  )
}

function Assert-CleanIdentityValue([string] $Category, [string] $Value) {
  if ($null -eq $Value) { return }
  $folded = $Value.ToLowerInvariant()
  foreach ($root in Get-RetiredProductRoots) {
    if ($folded.Contains($root)) {
      throw "MSI identity contains a retired product root in category $Category"
    }
  }
}

function Invoke-ComMethod([object] $Target, [string] $Name, [object[]] $Arguments = @()) {
  $Target.GetType().InvokeMember(
    $Name,
    [Reflection.BindingFlags]::InvokeMethod,
    $null,
    $Target,
    $Arguments
  )
}

function Get-ComProperty([object] $Target, [string] $Name, [object[]] $Arguments = @()) {
  $Target.GetType().InvokeMember(
    $Name,
    [Reflection.BindingFlags]::GetProperty,
    $null,
    $Target,
    $Arguments
  )
}

function Get-MsiRows([object] $Database, [string] $Query) {
  $view = Invoke-ComMethod $Database "OpenView" @($Query)
  try {
    $null = Invoke-ComMethod $view "Execute"
    while ($true) {
      $record = Invoke-ComMethod $view "Fetch"
      if ($null -eq $record) { break }
      try {
        $fieldCount = [int](Get-ComProperty $record "FieldCount")
        $values = [Collections.Generic.List[string]]::new()
        foreach ($index in 1..$fieldCount) {
          try {
            $values.Add([string](Get-ComProperty $record "StringData" @($index)))
          } catch {
            $values.Add("")
          }
        }
        [pscustomobject]@{ Values = @($values) }
      } finally {
        if ([Runtime.InteropServices.Marshal]::IsComObject($record)) {
          [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($record)
        }
      }
    }
  } finally {
    try { $null = Invoke-ComMethod $view "Close" } catch { }
    if ([Runtime.InteropServices.Marshal]::IsComObject($view)) {
      [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($view)
    }
  }
}

function Get-MsiProperty([object] $Database, [string] $Name) {
  $query = "SELECT ``Value`` FROM ``Property`` WHERE ``Property`` = '$Name'"
  $rows = @(Get-MsiRows $Database $query)
  if ($rows.Count -ne 1) { throw "MSI Property table requires exactly one $Name row" }
  [string]$rows[0].Values[0]
}

if ($SelfTest) {
  foreach ($root in Get-RetiredProductRoots) {
    $rejected = $false
    try { Assert-CleanIdentityValue "self-test" ("prefix-" + $root.ToUpperInvariant() + "-suffix") } catch { $rejected = $true }
    if (-not $rejected) { throw "MSI identity self-test did not reject a retired root" }
  }
  Assert-CleanIdentityValue "self-test" "SniptaleNativeAgent-0.1.0"
  Write-Output "OK Sniptale MSI identity self-test"
  exit 0
}

if (-not $Msi -or -not $ExpectedProductVersion) {
  throw "Msi and ExpectedProductVersion are required"
}
$resolvedMsi = [IO.Path]::GetFullPath($Msi)
if (-not (Test-Path -LiteralPath $resolvedMsi -PathType Leaf)) { throw "MSI input is missing" }
$leafName = [IO.Path]::GetFileName($resolvedMsi)
$escapedVersion = [Regex]::Escape($ExpectedProductVersion)
if ($leafName -notmatch "^SniptaleNativeAgent-(stable|beta|dev)-$escapedVersion(?:-(dev|release))?\.msi$") {
  throw "MSI filename does not match the Sniptale channel/version contract"
}
Assert-CleanIdentityValue "filename" $leafName

$normalizedExpectedUpgradeCode = $ExpectedUpgradeCode.Trim("{}".ToCharArray()).ToUpperInvariant()
if ($normalizedExpectedUpgradeCode -notmatch '^[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}$') {
  throw "ExpectedUpgradeCode is malformed"
}

$installer = New-Object -ComObject WindowsInstaller.Installer
$database = $null
try {
  $database = Invoke-ComMethod $installer "OpenDatabase" @($resolvedMsi, 0)
  $productName = Get-MsiProperty $database "ProductName"
  $manufacturer = Get-MsiProperty $database "Manufacturer"
  $productVersion = Get-MsiProperty $database "ProductVersion"
  if ($productName -ne "Sniptale Desktop") { throw "MSI ProductName drifted" }
  if ($manufacturer -ne "Sniptale") { throw "MSI Manufacturer drifted" }
  if ($productVersion -ne $ExpectedProductVersion) { throw "MSI ProductVersion drifted" }

  $tableRows = @(Get-MsiRows $database 'SELECT `Name` FROM `_Tables`')
  if ($tableRows.Count -eq 0) { throw "MSI table inventory is empty" }
  foreach ($tableRow in $tableRows) {
    $tableName = [string]$tableRow.Values[0]
    if ($tableName -notmatch '^[A-Za-z_][A-Za-z0-9_]*$') { throw "MSI table name is malformed" }
    Assert-CleanIdentityValue "table-name" $tableName
    $rows = @(Get-MsiRows $database "SELECT * FROM ``$tableName``")
    foreach ($row in $rows) {
      foreach ($value in $row.Values) { Assert-CleanIdentityValue "table-value" ([string]$value) }
    }
  }

  $upgradeRows = @(Get-MsiRows $database 'SELECT `UpgradeCode` FROM `Upgrade`')
  if ($upgradeRows.Count -eq 0) { throw "MSI Upgrade table is empty" }
  $upgradeCodes = @($upgradeRows | ForEach-Object { ([string]$_.Values[0]).Trim("{}".ToCharArray()).ToUpperInvariant() } | Sort-Object -Unique)
  if ($upgradeCodes.Count -ne 1 -or $upgradeCodes[0] -ne $normalizedExpectedUpgradeCode) {
    throw "MSI Upgrade table is not bound exclusively to the Sniptale product family"
  }
  Write-Output ("OK Sniptale MSI identity file={0} version={1} upgradeCode={2} upgradeRows={3} tables={4}" -f $leafName, $productVersion, $ExpectedUpgradeCode, $upgradeRows.Count, $tableRows.Count)
} finally {
  if ($null -ne $database -and [Runtime.InteropServices.Marshal]::IsComObject($database)) {
    [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($database)
  }
  if ([Runtime.InteropServices.Marshal]::IsComObject($installer)) {
    [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($installer)
  }
}
