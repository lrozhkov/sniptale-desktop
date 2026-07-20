param(
  [Parameter(Mandatory = $true)][string] $SourceBundle,
  [Parameter(Mandatory = $true)][string] $SourceSha256,
  [ValidateSet("dev", "release")][string] $Profile = "release",
  [string] $Output = "out"
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

if ((Get-FileHash -Algorithm SHA256 -LiteralPath $SourceBundle).Hash.ToLowerInvariant() -ne $SourceSha256.ToLowerInvariant()) {
  throw "source bundle SHA-256 mismatch"
}
$work = "C:\SniptaleLab\work"
$outputPath = [IO.Path]::GetFullPath((Join-Path $PWD $Output))
Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $outputPath -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $work, $outputPath | Out-Null
& tar.exe -xzf $SourceBundle -C $work
if ($LASTEXITCODE -ne 0) { throw "source extraction failed" }

$powerShellFiles = @(Get-ChildItem -LiteralPath (Join-Path $work "tooling") -Recurse -File -Filter "*.ps1")
$powerShellParseErrors = @()
foreach ($powerShellFile in $powerShellFiles) {
  $tokens = $null
  $fileErrors = $null
  [System.Management.Automation.Language.Parser]::ParseFile(
    $powerShellFile.FullName,
    [ref]$tokens,
    [ref]$fileErrors
  ) | Out-Null
  if ($fileErrors) { $powerShellParseErrors += $fileErrors }
}
if ($powerShellParseErrors.Count -gt 0) {
  $summary = ($powerShellParseErrors | ForEach-Object { $_.Message } | Sort-Object -Unique) -join "; "
  throw "PowerShell tooling parse verification failed: $summary"
}
Write-Output ("[build-progress] powershell-parse status=pass files={0}" -f $powerShellFiles.Count)

$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$install = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $install) { throw "MSVC Build Tools not found" }
$vsdev = Join-Path $install "Common7\Tools\VsDevCmd.bat"
$cargoProfile = if ($Profile -eq "release") { "--release" } else { "" }
$env:SNIPTALE_INSTALL_CHANNEL = "dev"
$env:SNIPTALE_ALLOWED_EXTENSION_ORIGIN = "chrome-extension://elhndangpenapadjimfoodlaenpcndpo/"
$env:SNIPTALE_NATIVE_HOST_NAME = "com.sniptale.dev.native_host"
$env:SNIPTALE_WINDOWS_RESOURCES = "require"
$env:SNIPTALE_DEV_LOCAL_BROWSER_TRUST = "1"
$env:CARGO_PROFILE_RELEASE_DEBUG = "1"
$buildCommand = "call `"$vsdev`" -arch=amd64 -host_arch=amd64 && cd /d `"$work`" && cargo build --locked --target x86_64-pc-windows-msvc --all-features $cargoProfile"
& cmd.exe /d /s /c $buildCommand
if ($LASTEXITCODE -ne 0) { throw "Windows cargo build failed" }

$profileDir = if ($Profile -eq "release") { "release" } else { "debug" }
$exe = Join-Path $work "target\x86_64-pc-windows-msvc\$profileDir\sniptale-native-agent.exe"
if (-not (Test-Path -LiteralPath $exe)) { throw "Windows executable is missing" }
Copy-Item -LiteralPath $exe -Destination (Join-Path $outputPath "sniptale-native-agent-$Profile.exe")
$pdb = Get-ChildItem -LiteralPath (Join-Path $work "target\x86_64-pc-windows-msvc\$profileDir") `
  -File -Filter "*sniptale*.pdb" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($pdb) {
  Copy-Item -LiteralPath $pdb.FullName -Destination (Join-Path $outputPath "sniptale-native-agent-$Profile.pdb")
}
$baselineExe = Join-Path $outputPath "sniptale-native-agent-baseline-$Profile.exe"
Copy-Item -LiteralPath $exe -Destination $baselineExe
$baselineMarker = [Text.Encoding]::ASCII.GetBytes("SNIPTALE-E2E-BASELINE-0.0.1")
$baselineStream = [IO.File]::Open($baselineExe, [IO.FileMode]::Append, [IO.FileAccess]::Write, [IO.FileShare]::None)
try { $baselineStream.Write($baselineMarker, 0, $baselineMarker.Length) } finally { $baselineStream.Dispose() }

$manifest = Get-Content -Raw -LiteralPath (Join-Path $work "installer\native-host\dev.json.template")
$manifest = $manifest.Replace("{{SNIPTALE_NATIVE_AGENT_EXE}}", "sniptale-native-agent.exe")
$manifest = $manifest.Replace("{{SNIPTALE_DEV_EXTENSION_ORIGIN}}", "chrome-extension://elhndangpenapadjimfoodlaenpcndpo/")
$manifestPath = Join-Path $outputPath "native-host.json"
[IO.File]::WriteAllText($manifestPath, $manifest, [Text.UTF8Encoding]::new($false))
$wix = Join-Path $env:USERPROFILE ".dotnet\tools\wix.exe"
if (-not (Test-Path -LiteralPath $wix)) { $wix = "wix.exe" }
$wixVersion = ((& $wix --version) -replace '^([0-9]+\.[0-9]+\.[0-9]+).*$', '$1').Trim()
if ($wixVersion -notmatch '^\d+\.\d+\.\d+$') { throw "could not determine WiX version" }
& $wix extension add -g "WixToolset.UI.wixext/$wixVersion" 2>$null
$currentWixPath = Join-Path $work "installer\wix\SniptaleNativeAgent.wxs"
$identityVerifier = Join-Path $work "tooling\release\windows\verify_msi_identity.ps1"
foreach ($productVersion in @("0.0.1", "0.1.0")) {
  $msi = Join-Path $outputPath "SniptaleNativeAgent-dev-$productVersion-$Profile.msi"
  $payloadExe = if ($productVersion -eq "0.0.1") { $baselineExe } else { $exe }
  & $wix build $currentWixPath `
    -ext WixToolset.UI.wixext `
    -d "NativeAgentExe=$payloadExe" `
    -d "NativeHostManifest=$manifestPath" `
    -d "ProjectLicense=$(Join-Path $work 'LICENSE')" `
    -d "ProjectNotice=$(Join-Path $work 'NOTICE')" `
    -d "ThirdPartyNotices=$(Join-Path $work 'THIRD_PARTY_NOTICES.md')" `
    -d "ThirdPartyDependencies=$(Join-Path $work 'THIRD_PARTY_DEPENDENCIES.json')" `
    -d "AppIcon=$(Join-Path $work 'resources\windows\icons\sniptale.ico')" `
    -d "NativeHostName=com.sniptale.dev.native_host" `
    -d "ProductVersion=$productVersion" `
    -o $msi
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $msi)) { throw "WiX MSI build failed for $productVersion" }
  & $identityVerifier -Msi $msi -ExpectedProductVersion $productVersion
  if ($LASTEXITCODE -ne 0) { throw "MSI identity verification failed for $productVersion" }
}

$entries = Get-ChildItem -LiteralPath $outputPath -File | ForEach-Object {
  [ordered]@{ name = $_.Name; bytes = $_.Length; sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant() }
}
$value = [ordered]@{ schema_version = 1; profile = $Profile; artifacts = @($entries) }
[IO.File]::WriteAllText((Join-Path $outputPath "build-manifest.json"), ($value | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
