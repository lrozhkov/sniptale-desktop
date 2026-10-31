param(
  [Parameter(Mandatory = $true)][string] $SourceBundle,
  [Parameter(Mandatory = $true)][string] $SourceSha256,
  [ValidateSet("dev", "release")][Alias("Profile")][string] $BuildProfile = "release",
  [string] $Output = "out",
  [ValidateSet("", "warm")][string] $WorkspaceMode = "",
  [string] $VendorBundle = "",
  [string] $VendorSha256 = "",
  [string] $RustcSha256 = "",
  [string] $CargoSha256 = ""
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

if ((Get-FileHash -Algorithm SHA256 -LiteralPath $SourceBundle).Hash.ToLowerInvariant() -ne $SourceSha256.ToLowerInvariant()) {
  throw "source bundle SHA-256 mismatch"
}
$work = "C:\SniptaleLab\work"
$outputPath = [IO.Path]::GetFullPath((Join-Path $PWD $Output))
if ($WorkspaceMode -ne "warm") { Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue }
Remove-Item -LiteralPath $outputPath -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $work, $outputPath | Out-Null
$extract = if ($WorkspaceMode -eq "warm") { "C:\SniptaleLab\source-stage" } else { $work }
if ($WorkspaceMode -eq "warm") {
  Remove-Item -LiteralPath $extract -Recurse -Force -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force -Path $extract | Out-Null
}
& tar.exe -xzf $SourceBundle -C $extract
if ($LASTEXITCODE -ne 0) { throw "source extraction failed" }
if ($WorkspaceMode -eq "warm") {
  # Deterministic archives use epoch mtimes. Copy changed bytes with a fresh mtime so Cargo
  # cannot mistake a changed Rust source for an older, already compiled input.
  $incoming = @{}
  foreach ($file in Get-ChildItem -LiteralPath $extract -Recurse -File) {
    if ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "source contains reparse point" }
    $relative = $file.FullName.Substring($extract.Length + 1)
    $incoming[$relative] = $true
    $destination = Join-Path $work $relative
    if (-not (Test-Path -LiteralPath $destination) -or (Get-FileHash -LiteralPath $destination).Hash -ne (Get-FileHash -LiteralPath $file.FullName).Hash) {
      New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($destination)) | Out-Null
      Copy-Item -LiteralPath $file.FullName -Destination $destination -Force
      (Get-Item -LiteralPath $destination).LastWriteTimeUtc = [DateTime]::UtcNow
    }
  }
  Get-ChildItem -LiteralPath $work -Recurse -File | Where-Object {
    -not $incoming.ContainsKey($_.FullName.Substring($work.Length + 1))
  } | Remove-Item -Force
  Remove-Item -LiteralPath $extract -Recurse -Force
  $env:CARGO_TARGET_DIR = "C:\SniptaleLab\warm-target"
  if ($VendorBundle) {
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $VendorBundle).Hash.ToLowerInvariant() -ne $VendorSha256) { throw "vendor bundle hash mismatch" }
    Remove-Item -LiteralPath "C:\SniptaleLab\vendor" -Recurse -Force -ErrorAction SilentlyContinue
    & tar.exe -xzf $VendorBundle -C "C:\SniptaleLab"
    if ($LASTEXITCODE -ne 0) { throw "vendor extraction failed" }
  }
  New-Item -ItemType Directory -Force -Path (Join-Path $work ".cargo") | Out-Null
  [IO.File]::WriteAllText((Join-Path $work ".cargo\config.toml"), @'
[source.crates-io]
replace-with = "lab-vendor"
[source.lab-vendor]
directory = "C:/SniptaleLab/vendor"
[net]
offline = true
'@, [Text.UTF8Encoding]::new($false))
}

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
$environment = & cmd.exe /d /c "call `"$vsdev`" -arch=amd64 -host_arch=amd64 && set"
if ($LASTEXITCODE -ne 0) { throw "MSVC environment activation failed" }
foreach ($line in $environment) {
  if ($line -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($Matches[1], $Matches[2], "Process") }
}
$toolsTimer = [Diagnostics.Stopwatch]::StartNew()
. (Join-Path $work "tooling/release/windows/toolchain.ps1")
$windowsTools = Initialize-SniptaleWindowsTool $work "C:\SniptaleLab\build-tools" -Offline:($WorkspaceMode -eq "warm")
$toolsTimer.Stop()
Write-Output "[build-progress] windows-tools status=ready"
$cargoProfile = if ($BuildProfile -eq "release") { "--release" } else { "" }
$pinnedRust = (Get-Content (Join-Path $work "tooling/configs/qa/tool-versions.json") -Raw | ConvertFrom-Json).rust.channel
if ($WorkspaceMode -eq "warm") {
  $compilerRoot = "C:\SniptaleLab\rust-toolchain"
  if ((Get-FileHash (Join-Path $compilerRoot "bin\rustc.exe")).Hash.ToLowerInvariant() -ne $RustcSha256 -or (Get-FileHash (Join-Path $compilerRoot "bin\cargo.exe")).Hash.ToLowerInvariant() -ne $CargoSha256) { throw "offline compiler hash mismatch" }
  & rustup toolchain link sniptale-lab $compilerRoot
  if ($LASTEXITCODE -ne 0) { throw "offline toolchain registration failed" }
  $env:RUSTUP_TOOLCHAIN = "sniptale-lab"
  $compilerVersion = & rustc --version
  if ($LASTEXITCODE -ne 0 -or $compilerVersion -notmatch "^rustc $([regex]::Escape($pinnedRust)) ") { throw "offline compiler version mismatch" }
}
if ($WorkspaceMode -ne "warm") {
  $installedRust = & rustup run stable rustc --version 2>$null
  if ($LASTEXITCODE -eq 0 -and $installedRust -match "^rustc $([regex]::Escape($pinnedRust)) ") {
    $env:RUSTUP_TOOLCHAIN = "stable"
  }
}
$env:SNIPTALE_INSTALL_CHANNEL = "dev"
$env:SNIPTALE_ALLOWED_EXTENSION_ORIGIN = "chrome-extension://elhndangpenapadjimfoodlaenpcndpo/"
$env:SNIPTALE_NATIVE_HOST_NAME = "com.sniptale.dev.native_host"
$env:SNIPTALE_WINDOWS_RESOURCES = "require"
$env:SNIPTALE_DEV_LOCAL_BROWSER_TRUST = "1"
$env:CARGO_PROFILE_RELEASE_DEBUG = "1"
$buildCommand = "cd /d `"$work`" && cargo build --locked --target x86_64-pc-windows-msvc --all-features $cargoProfile"
Write-Output "[build-progress] compile status=started"
$compileTimer = [Diagnostics.Stopwatch]::StartNew()
& cmd.exe /d /s /c $buildCommand
$compileTimer.Stop()
if ($LASTEXITCODE -ne 0) { throw "Windows cargo build failed" }

$profileDir = if ($BuildProfile -eq "release") { "release" } else { "debug" }
$targetRoot = if ($WorkspaceMode -eq "warm") { $env:CARGO_TARGET_DIR } else { Join-Path $work "target" }
$exe = Join-Path $targetRoot "x86_64-pc-windows-msvc\$profileDir\sniptale-native-agent.exe"
if (-not (Test-Path -LiteralPath $exe)) { throw "Windows executable is missing" }
Copy-Item -LiteralPath $exe -Destination (Join-Path $outputPath "sniptale-native-agent-$BuildProfile.exe")
$pdb = Get-ChildItem -LiteralPath (Join-Path $targetRoot "x86_64-pc-windows-msvc\$profileDir") `
  -File -Filter "*sniptale*.pdb" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($pdb) {
  Copy-Item -LiteralPath $pdb.FullName -Destination (Join-Path $outputPath "sniptale-native-agent-$BuildProfile.pdb")
}
$exe = Join-Path $outputPath "sniptale-native-agent-$BuildProfile.exe"
$baselineExe = Join-Path $outputPath "sniptale-native-agent-baseline-$BuildProfile.exe"
Copy-Item -LiteralPath $exe -Destination $baselineExe
$baselineMarker = [Text.Encoding]::ASCII.GetBytes("SNIPTALE-E2E-BASELINE-0.0.1")
$baselineStream = [IO.File]::Open($baselineExe, [IO.FileMode]::Append, [IO.FileAccess]::Write, [IO.FileShare]::None)
try { $baselineStream.Write($baselineMarker, 0, $baselineMarker.Length) } finally { $baselineStream.Dispose() }

. (Join-Path $work "tooling/desktop/e2e_lab/guest/windows/lab-signing.ps1")
$labCertificate = New-LabSigningIdentity
$labCertificatePath = Join-Path $outputPath "lab-signing.cer"
[IO.File]::WriteAllBytes($labCertificatePath, $labCertificate.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Cert))
Sign-LabArtifact $exe $labCertificate
Sign-LabArtifact $baselineExe $labCertificate

$manifest = Get-Content -Raw -LiteralPath (Join-Path $work "installer\native-host\dev.json.template")
$manifest = $manifest.Replace("{{SNIPTALE_NATIVE_AGENT_EXE}}", "sniptale-native-agent.exe")
$manifest = $manifest.Replace("{{SNIPTALE_DEV_EXTENSION_ORIGIN}}", "chrome-extension://elhndangpenapadjimfoodlaenpcndpo/")
$manifestPath = Join-Path $outputPath "native-host.json"
[IO.File]::WriteAllText($manifestPath, $manifest, [Text.UTF8Encoding]::new($false))
$wix = $windowsTools.Wix
$wixVersion = ((& $wix --version) -split '\+')[0].Trim()
$wixExtension = $windowsTools.WixExtension
$currentWixPath = Join-Path $work "installer\wix\SniptaleNativeAgent.wxs"
$identityVerifier = Join-Path $work "tooling\release\windows\verify_msi_identity.ps1"
$packageTimer = [Diagnostics.Stopwatch]::StartNew()
foreach ($productVersion in @("0.0.1", "0.1.0")) {
  $msi = Join-Path $outputPath "SniptaleNativeAgent-dev-$productVersion-$BuildProfile.msi"
  $payloadExe = if ($productVersion -eq "0.0.1") { $baselineExe } else { $exe }
  & $wix build $currentWixPath `
    -ext $wixExtension `
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
  Sign-LabArtifact $msi $labCertificate
  & $identityVerifier -Msi $msi -ExpectedProductVersion $productVersion
  if ($LASTEXITCODE -ne 0) { throw "MSI identity verification failed for $productVersion" }
}

$packageTimer.Stop()
$entries = Get-ChildItem -LiteralPath $outputPath -File | ForEach-Object {
  [ordered]@{ name = $_.Name; bytes = $_.Length; sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant() }
}
$os = Get-CimInstance Win32_OperatingSystem
$value = [ordered]@{
  schema_version = 1; profile = $BuildProfile; artifacts = @($entries)
  timings_ms = [ordered]@{ tools = $toolsTimer.ElapsedMilliseconds; compile = $compileTimer.ElapsedMilliseconds; package = $packageTimer.ElapsedMilliseconds }
  environment = [ordered]@{
    os_version = $os.Version; os_build = $os.BuildNumber
    rustc = (& rustc --version); cargo = (& cargo --version); wix = $wixVersion
    windows_sdk = $windowsTools.Sdk; windows_sdk_package = $windowsTools.SdkPackage
    signing_mode = "isolated-laboratory-self-signed"
    laboratory_certificate_sha256 = (Get-FileHash -LiteralPath $labCertificatePath).Hash.ToLowerInvariant()
    source_sha256 = $SourceSha256; workspace_mode = $WorkspaceMode
    rustc_sha256 = $RustcSha256; cargo_sha256 = $CargoSha256
  }
}
[IO.File]::WriteAllText((Join-Path $outputPath "build-manifest.json"), ($value | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
