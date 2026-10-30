function Expand-SniptaleBuildPackage([string] $Package, [string] $Destination, [string] $Sha256) {
  $stamp = Join-Path $Destination ".package-sha256"
  if (-not (Test-Path $stamp) -or (Get-Content -Raw $stamp) -ne $Sha256) {
    Remove-Item $Destination -Recurse -Force -ErrorAction SilentlyContinue
    $null = New-Item -ItemType Directory -Force $Destination
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($Package)
    try {
      $total = [long]0
      foreach ($entry in $archive.Entries) {
        $target = [IO.Path]::GetFullPath((Join-Path $Destination $entry.FullName))
        if (-not $target.StartsWith($Destination + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw "build package path escaped cache" }
        $total += $entry.Length
        if ($total -gt 2GB -or $entry.Length -gt 256MB -or $archive.Entries.Count -gt 30000) { throw "build package extraction budget exceeded" }
      }
      [IO.Compression.ZipFileExtensions]::ExtractToDirectory($archive, $Destination)
    } finally { $archive.Dispose() }
    [IO.File]::WriteAllText($stamp, $Sha256)
  }
}

# Shared, hash-verified Windows build inputs for CI and isolated development guests.
function Initialize-SniptaleWindowsTools([string] $Root, [string] $Cache, [switch] $Offline) {
  $versions = Get-Content -Raw (Join-Path $Root "tooling/configs/qa/tool-versions.json") | ConvertFrom-Json
  $lock = Get-Content -Raw (Join-Path $Root "tooling/configs/qa/toolchain-lock.json") | ConvertFrom-Json
  $null = New-Item -ItemType Directory -Force $Cache
  $packages = Join-Path $Cache "packages"
  $null = New-Item -ItemType Directory -Force $packages
  foreach ($property in $lock.nuget.PSObject.Properties) {
    $name = $property.Name
    $item = $property.Value
    if ($name -notin @("wix", "wixtoolset.ui.wixext", "microsoft.windows.sdk.cpp", "microsoft.windows.sdk.cpp.x64") -or $item.sha256 -notmatch '^[0-9a-f]{64}$') { throw "invalid build package identity" }
    $version = if ($name -like "microsoft.windows.sdk.*") { $versions.windows.sdkPackage } else { $versions.windows.wix }
    $expectedUrl = "https://api.nuget.org/v3-flatcontainer/$name/$version/$name.$version.nupkg"
    if ($item.version -ne $version -or $item.url -ne $expectedUrl) { throw "stale build package lock" }
    $package = Join-Path $packages "$name.$version.nupkg"
    if (-not (Test-Path -LiteralPath $package)) {
      if ($Offline) { throw "offline build package missing: $name" }
      Invoke-WebRequest -Uri $expectedUrl -OutFile $package -UseBasicParsing
    }
    if ((Get-FileHash -LiteralPath $package).Hash.ToLowerInvariant() -ne $item.sha256) { throw "build package checksum mismatch: $name" }
    Expand-SniptaleBuildPackage $package (Join-Path $Cache "$name-$version") $item.sha256
  }
  $wixRoot = Join-Path $Cache "wix-$($versions.windows.wix)"
  $env:DOTNET_ROLL_FORWARD = "Major"
  $wixCandidates = @(Get-ChildItem (Join-Path $wixRoot "tools") -Recurse -File -Filter "wix.exe" | Where-Object { $_.FullName -match '\\tools\\net[0-9.]+\\any\\wix\.exe$' })
  if ($wixCandidates.Count -ne 1) { throw "verified WiX executable layout unavailable" }
  $wix = $wixCandidates[0].FullName
  $actual = ((& $wix --version) -split '\+')[0].Trim()
  if ($actual -ne $versions.windows.wix) { throw "WiX version differs from policy" }
  $ui = $lock.nuget.'wixtoolset.ui.wixext'
  $extension = Join-Path (Join-Path $Cache "wixtoolset.ui.wixext-$($versions.windows.wix)") $ui.dll_path
  if ((Get-FileHash $extension).Hash.ToLowerInvariant() -ne $ui.dll_sha256) { throw "WiX extension checksum mismatch" }
  $sdk = Join-Path $Cache "microsoft.windows.sdk.cpp-$($versions.windows.sdkPackage)/c"
  $sdkLib = Join-Path $Cache "microsoft.windows.sdk.cpp.x64-$($versions.windows.sdkPackage)/c"
  $kit = $versions.windows.sdk
  foreach ($path in @("$sdk/Include/$kit/um/Windows.h", "$sdk/bin/$kit/x64/rc.exe", "$sdkLib/um/x64/kernel32.lib")) {
    if (-not (Test-Path -LiteralPath $path)) { throw "pinned Windows SDK layout unavailable" }
  }
  $env:WindowsSdkDir = "$sdk\"
  $env:WindowsSDKVersion = "$kit\"
  $env:INCLUDE = "$sdk\Include\$kit\ucrt;$sdk\Include\$kit\shared;$sdk\Include\$kit\um;$sdk\Include\$kit\winrt;$env:INCLUDE"
  $env:LIB = "$sdkLib\ucrt\x64;$sdkLib\um\x64;$env:LIB"
  $env:Path = "$sdk\bin\$kit\x64;$env:Path"
  return @{ Wix = $wix; WixExtension = $extension; Sdk = $kit; SdkPackage = $versions.windows.sdkPackage }
}

function Initialize-SniptaleWindowsPython([string] $Root, [string] $Cache, [switch] $Offline) {
  $versions = Get-Content -Raw (Join-Path $Root "tooling/configs/qa/tool-versions.json") | ConvertFrom-Json
  $lock = Get-Content -Raw (Join-Path $Root "tooling/configs/qa/toolchain-lock.json") | ConvertFrom-Json
  $version = $versions.python.version
  $item = $lock.python.windows
  if ($version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+$' -or $item.sha256 -notmatch '^[0-9a-f]{64}$' -or $item.sha256 -ne $versions.python.windowsEmbeddableSha256) { throw "invalid Windows Python identity" }
  $expectedUrl = "https://www.python.org/ftp/python/$version/python-$version-embed-amd64.zip"
  if ($item.url -ne $expectedUrl) { throw "invalid Windows Python provenance" }
  $packages = Join-Path $Cache "packages"
  $null = New-Item -ItemType Directory -Force $packages
  $package = Join-Path $packages "python-$version-embed-amd64.zip"
  if (-not (Test-Path -LiteralPath $package)) {
    if ($Offline) { throw "offline Windows Python package missing" }
    Invoke-WebRequest -Uri $expectedUrl -OutFile $package -UseBasicParsing -TimeoutSec 120
  }
  if ((Get-FileHash -LiteralPath $package).Hash.ToLowerInvariant() -ne $item.sha256) { throw "Windows Python package checksum mismatch" }
  $destination = Join-Path $Cache "python-$version"
  Expand-SniptaleBuildPackage $package $destination $item.sha256
  $python = Join-Path $destination "python.exe"
  $actual = & $python --version
  if ($LASTEXITCODE -ne 0 -or $actual.Trim() -ne "Python $version") { throw "Windows Python version differs from policy" }
  $env:Path = "$destination;$env:Path"
  return $python
}
