param(
  [Parameter(Mandatory = $true)][string] $QaProof,
  [Parameter(Mandatory = $true)][string] $Tag,
  [string] $Output = ".tmp/release/alpha"
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "../../.."))
Set-Location -LiteralPath $root
foreach ($source in Get-ChildItem tooling -Recurse -File -Filter *.ps1) {
  $tokens = $null
  $parseErrors = $null
  [System.Management.Automation.Language.Parser]::ParseFile($source.FullName, [ref]$tokens, [ref]$parseErrors) | Out-Null
  if ($parseErrors) { throw "PowerShell source parse verification failed" }
}
$commit = (& git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw "Git source identity unavailable" }
& python tooling/release/windows/alpha_artifacts.py verify-proof $QaProof --commit $commit
if ($LASTEXITCODE -ne 0) { throw "Linux release proof rejected" }
if ($Tag -notmatch '^v(0\.\d+\.\d+)-alpha\.[1-9][0-9]*$') { throw "invalid alpha tag" }
$version = $Matches[1]
$cargoText = Get-Content Cargo.toml -Raw
if ($cargoText -notmatch '(?m)^version = "([^"]+)"' -or $Matches[1] -ne $version) {
  throw "alpha tag version must match Cargo.toml"
}
if (Test-Path -LiteralPath $Output) { throw "alpha output must be a new directory" }
$out = [IO.Path]::GetFullPath((Join-Path $root $Output))
New-Item -ItemType Directory -Path $out | Out-Null
$qa = Join-Path $out "qa"
Copy-Item -LiteralPath $QaProof -Destination $qa -Recurse

$versions = Get-Content tooling/configs/qa/tool-versions.json -Raw | ConvertFrom-Json
$rust = $versions.rust.channel
$installedRust = & rustup run stable rustc --version 2>$null
if ($LASTEXITCODE -eq 0 -and $installedRust -match "^rustc $([regex]::Escape($rust)) ") {
  $env:RUSTUP_TOOLCHAIN = "stable"
} else {
  & rustup toolchain install $rust --profile minimal --no-self-update
  if ($LASTEXITCODE -ne 0) { throw "pinned Rust installation failed" }
  $env:RUSTUP_TOOLCHAIN = $rust
}
& rustup target add --toolchain $env:RUSTUP_TOOLCHAIN x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { throw "MSVC target installation failed" }
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw "install Visual Studio Build Tools with C++ and Windows SDK" }
$devcmd = Join-Path $vs "Common7/Tools/VsDevCmd.bat"
$environment = & cmd.exe /d /c "call `"$devcmd`" -arch=amd64 -host_arch=amd64 && set"
if ($LASTEXITCODE -ne 0) { throw "MSVC environment activation failed" }
foreach ($line in $environment) {
  if ($line -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($Matches[1], $Matches[2], "Process") }
}
. (Join-Path $PSScriptRoot "toolchain.ps1")
$windowsTools = Initialize-SniptaleWindowsTool $root (Join-Path $root ".tmp/alpha-tools")
$wix = $windowsTools.Wix
$wixVersion = $versions.windows.wix
Remove-Item Env:SNIPTALE_DEV_LOCAL_BROWSER_TRUST -ErrorAction SilentlyContinue
$env:SNIPTALE_INSTALL_CHANNEL = "dev"
$env:SNIPTALE_RELEASE_FLAVOR = "alpha"
$env:SNIPTALE_ALLOWED_EXTENSION_ORIGIN = "chrome-extension://pcegkgbagkiagakmfjchgfgheepkllda/"
$env:SNIPTALE_WINDOWS_RESOURCES = "require"
$env:CARGO_INCREMENTAL = "0"
Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
$env:RUSTFLAGS = "-C target-feature=+crt-static"
# Recompile project code and build scripts from this checkout; retain only dependency work.
& cargo clean --locked --package sniptale-native-agent --release --target x86_64-pc-windows-msvc --target-dir target
if ($LASTEXITCODE -ne 0) { throw "project artifact invalidation failed" }
$compileWatch = [Diagnostics.Stopwatch]::StartNew()
& cargo build --locked --release --target x86_64-pc-windows-msvc --bin sniptale-native-agent --all-features --timings --target-dir target
if ($LASTEXITCODE -ne 0) { throw "Windows release compilation failed" }
$compileWatch.Stop()
Write-Output "Windows release compilation elapsed seconds: $($compileWatch.Elapsed.TotalSeconds)"
$exe = Join-Path $out "sniptale-native-agent.exe"
Copy-Item target/x86_64-pc-windows-msvc/release/sniptale-native-agent.exe $exe
& python tooling/release/windows/verify_windows_pe_resources.py $exe
if ($LASTEXITCODE -ne 0) { throw "Windows resource verification failed" }
& python tooling/release/windows/verify_windows_dependencies.py $exe
if ($LASTEXITCODE -ne 0) { throw "standalone Windows dependency verification failed" }
$manifest = (Get-Content installer/native-host/dev.json.template -Raw).
  Replace("{{SNIPTALE_NATIVE_AGENT_EXE}}", "sniptale-native-agent.exe").
  Replace("{{SNIPTALE_DEV_EXTENSION_ORIGIN}}", $env:SNIPTALE_ALLOWED_EXTENSION_ORIGIN)
[IO.File]::WriteAllText((Join-Path $out "native-host.json"), $manifest, [Text.UTF8Encoding]::new($false))
Copy-Item LICENSE (Join-Path $out "LICENSE.txt")
Copy-Item NOTICE (Join-Path $out "NOTICE.txt")
Copy-Item THIRD_PARTY_NOTICES.md, THIRD_PARTY_DEPENDENCIES.json $out
Copy-Item LICENSES (Join-Path $out "LICENSES") -Recurse
$msi = Join-Path $out "SniptaleNativeAgent-dev-$version.msi"
& $wix build installer/wix/SniptaleNativeAgent.wxs -ext $windowsTools.WixExtension `
  -d "NativeAgentExe=$exe" -d "NativeHostManifest=$(Join-Path $out 'native-host.json')" `
  -d "ProjectLicense=$(Join-Path $out 'LICENSE.txt')" -d "ProjectNotice=$(Join-Path $out 'NOTICE.txt')" `
  -d "ThirdPartyNotices=$(Join-Path $out 'THIRD_PARTY_NOTICES.md')" `
  -d "ThirdPartyDependencies=$(Join-Path $out 'THIRD_PARTY_DEPENDENCIES.json')" `
  -d "AppIcon=$root/resources/windows/icons/sniptale.ico" -d "NativeHostName=com.sniptale.dev.native_host" `
  -d "ProductVersion=$version" -o $msi
if ($LASTEXITCODE -ne 0) { throw "alpha MSI packaging failed" }
Remove-Item (Join-Path $out "*.wixpdb") -ErrorAction SilentlyContinue
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File tooling/release/windows/verify_msi_identity.ps1 -Msi $msi -ExpectedProductVersion $version
if ($LASTEXITCODE -ne 0) { throw "MSI identity rejected" }
$facts = [ordered]@{
  rustc = (& rustc --version); cargo = (& cargo --version); wix = $wixVersion
  windows_sdk = $windowsTools.Sdk; windows_sdk_package = $windowsTools.SdkPackage
  rustflags = $env:RUSTFLAGS
  windowsSdk = $env:WindowsSDKVersion; msvc = $env:VCToolsVersion
  dotnet = (& dotnet --version); python = (& python --version)
  runnerImage = $env:ImageVersion; osBuild = [Environment]::OSVersion.Version.ToString()
}
[IO.File]::WriteAllText((Join-Path $out "build-environment.json"), ($facts | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
& python tooling/release/windows/alpha_artifacts.py seal $out --commit $commit --tag $Tag
if ($LASTEXITCODE -ne 0) { throw "alpha artifact sealing failed" }
Write-Output "alpha package complete"
