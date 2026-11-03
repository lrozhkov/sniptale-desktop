param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^chrome-extension://[a-p]{32}/$')][string] $ExtensionOrigin,
  [Parameter(Mandatory = $true)][string] $Output
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
$root = (Resolve-Path (Join-Path $PSScriptRoot "../../..")).Path
Set-Location $root
$out = [IO.Path]::GetFullPath($Output)
if (Test-Path -LiteralPath $out) { throw "Dev output must be a new directory" }
if (-not (Test-Path -LiteralPath "ui/dist/index.html")) { throw "Build the shared UI before packaging" }
$versionMatch = [regex]::Match((Get-Content Cargo.toml -Raw), '(?m)^version = "([^"]+)"')
if (-not $versionMatch.Success) { throw "Cargo package version is missing" }
$version = $versionMatch.Groups[1].Value
Remove-Item Env:SNIPTALE_RELEASE_FLAVOR -ErrorAction SilentlyContinue
& (Join-Path $PSScriptRoot "smoke/windows-smoke.ps1") -Repo $root -Channel dev `
  -ExtensionOrigin $ExtensionOrigin -DevLocalTrust -UnsignedDev -BuildProfile dev -SkipCheckpoint
if ($LASTEXITCODE -ne 0) { throw "Unsigned dev Windows smoke failed" }
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "target" }
$compiled = Join-Path $target "x86_64-pc-windows-msvc/debug/sniptale-native-agent.exe"
if ((Get-AuthenticodeSignature -LiteralPath $compiled).Status -ne "NotSigned") {
  throw "Expected an unsigned dev executable"
}
New-Item -ItemType Directory -Path $out | Out-Null
Copy-Item -LiteralPath $compiled -Destination (Join-Path $out "sniptale-native-agent.exe")
Copy-Item LICENSE (Join-Path $out "LICENSE.txt")
Copy-Item NOTICE (Join-Path $out "NOTICE.txt")
Copy-Item THIRD_PARTY_NOTICES.md, THIRD_PARTY_DEPENDENCIES.json $out
Copy-Item LICENSES (Join-Path $out "LICENSES") -Recurse
Copy-Item (Join-Path $PSScriptRoot "install-dev.ps1") $out
Copy-Item (Join-Path $root ".tmp/release/windows-dev/native-host.json") $out
$msi = Join-Path $out "SniptaleNativeAgent-dev-$version.msi"
Copy-Item (Join-Path $root ".tmp/release/windows-dev/SniptaleNativeAgent-dev-$version.msi") $msi
if ((Get-AuthenticodeSignature -LiteralPath $msi).Status -ne "NotSigned") { throw "Expected an unsigned dev MSI" }
$entries = Get-ChildItem -LiteralPath $out -Recurse -File | ForEach-Object {
  [ordered]@{ path = $_.FullName.Substring($out.Length + 1).Replace('\', '/'); sha256 = (Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant() }
}
$proof = [ordered]@{
  kind = "unsigned-dev-local"; releaseAdmissible = $false; version = $version
  host = "com.sniptale.dev.native_host"; extensionOrigin = $ExtensionOrigin
  executableSignature = "NotSigned"; files = @($entries)
}
[IO.File]::WriteAllText((Join-Path $out "dev-bundle.json"), ($proof | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
Write-Output "Unsigned dev bundle: $out"
