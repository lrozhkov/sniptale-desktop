param([switch] $NoLaunch)
$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$bundle = Get-Content -Raw (Join-Path $root "dev-bundle.json") | ConvertFrom-Json
if ($bundle.kind -ne "unsigned-dev-local" -or $bundle.releaseAdmissible -ne $false -or
    $bundle.host -ne "com.sniptale.dev.native_host") { throw "Expected an explicit unsigned dev bundle" }
if ($bundle.files -isnot [array] -or $bundle.files.Count -lt 1 -or $bundle.files.Count -gt 2000) {
  throw "Dev bundle inventory is missing or invalid"
}
$packages = @(Get-ChildItem -LiteralPath $root -Filter "SniptaleNativeAgent-dev-*.msi" -File)
if ($packages.Count -ne 1) { throw "Expected one dev MSI in the bundle inventory" }
$names = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
foreach ($entry in $bundle.files) {
  if ($entry.path -isnot [string] -or -not $names.Add($entry.path)) { throw "Duplicate or invalid dev bundle inventory entry" }
}
foreach ($required in @("sniptale-native-agent.exe", "native-host.json", "install-dev.ps1", $packages[0].Name)) {
  if (-not $names.Contains($required)) { throw "Dev bundle inventory omits a required artifact" }
}
foreach ($entry in $bundle.files) {
  $path = [IO.Path]::GetFullPath((Join-Path $root $entry.path))
  if (-not $path.StartsWith($root + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
      $entry.sha256 -notmatch '^[0-9a-f]{64}$' -or
      (Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -ne $entry.sha256) {
    throw "Dev bundle file verification failed"
  }
}
$manifest = Get-Content -Raw (Join-Path $root "native-host.json") | ConvertFrom-Json
if ($manifest.name -ne "com.sniptale.dev.native_host" -or
    @($manifest.allowed_origins).Count -ne 1 -or
    $manifest.allowed_origins[0] -notmatch '^chrome-extension://[a-p]{32}/$' -or
    $manifest.allowed_origins[0] -ne $bundle.extensionOrigin) {
  throw "This installer accepts only an exact dev-channel identity"
}
$expected = (Get-FileHash -LiteralPath (Join-Path $root "sniptale-native-agent.exe")).Hash.ToLowerInvariant()
if ((Get-AuthenticodeSignature -LiteralPath (Join-Path $root "sniptale-native-agent.exe")).Status -ne "NotSigned") {
  throw "Expected an unsigned dev executable"
}
$process = Start-Process msiexec.exe -ArgumentList @("/i", ('"' + $packages[0].FullName + '"'), "/qn", "/norestart") -Wait -PassThru
if ($process.ExitCode -notin @(0, 3010)) { throw "Dev MSI installation failed: $($process.ExitCode)" }
$directory = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent"
$executable = Join-Path $directory "sniptale-native-agent.exe"
if ((Get-FileHash -LiteralPath $executable).Hash.ToLowerInvariant() -ne $expected) {
  throw "Installed executable differs from the dev bundle"
}
$installedManifest = Get-Content -Raw (Join-Path $directory "native-host.json") | ConvertFrom-Json
if ($installedManifest.name -ne $manifest.name -or
    @($installedManifest.allowed_origins).Count -ne 1 -or
    $installedManifest.allowed_origins[0] -ne $manifest.allowed_origins[0]) {
  throw "Installed native-host identity differs from the dev bundle"
}
$marker = [ordered]@{
  kind = "sniptale-dev-local-browser-trust"
  enabled = $true
  scope = "current-user-browser-parent"
  executableSha256 = $expected
}
$temporary = Join-Path $directory "dev-local-browser-trust.json.pending"
[IO.File]::WriteAllText($temporary, ($marker | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
Move-Item -LiteralPath $temporary -Destination (Join-Path $directory "dev-local-browser-trust.json") -Force
if (-not $NoLaunch) { Start-Process -FilePath $executable -WorkingDirectory $directory }
Write-Output "Dev package installed with executable-bound local trust. No certificate was installed."
