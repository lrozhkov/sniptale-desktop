param(
  [string] $Repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..")).Path,
  [ValidateSet("stable", "beta", "dev")]
  [string] $Channel = "stable",
  [string] $ExtensionOrigin = "chrome-extension://pcegkgbagkiagakmfjchgfgheepkllda/",
  [string] $ExtensionVersion = "0.1.0",
  [string] $ProductVersion = "",
  [string] $Wix = "wix",
  [ValidateRange(1, 10)]
  [int] $ProbeCount = 3,
  [int] $TimeoutMs = 30000,
  [switch] $SkipCheckpoint,
  [switch] $DevLocalTrust,
  [switch] $KeepLogs
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
if ([string]::IsNullOrWhiteSpace($env:CARGO_INCREMENTAL)) {
  $env:CARGO_INCREMENTAL = "0"
}

$script:SmokeEvidence = @()

function Invoke-Step($Name, [scriptblock] $Body) {
  Write-Host "-> $Name"
  & $Body
  if ($LASTEXITCODE -ne $null -and $LASTEXITCODE -ne 0) {
    throw "$Name failed with exit code $LASTEXITCODE"
  }
  Write-Host "OK $Name"
}

function Test-Command($Name) {
  [bool] (Get-Command $Name -ErrorAction SilentlyContinue)
}

function Ensure-WixUiExtension($WixCommand) {
  $versionOutput = (& $WixCommand --version 2>&1 | Out-String).Trim()
  $versionMatch = [regex]::Match($versionOutput, '^(\d+\.\d+\.\d+)')
  if (-not $versionMatch.Success) {
    throw "Could not determine WiX version from: $versionOutput"
  }
  $wixVersion = $versionMatch.Groups[1].Value
  $extensionPath = Join-Path $env:USERPROFILE ".wix\extensions\WixToolset.UI.wixext\$wixVersion\wixext4\WixToolset.UI.wixext.dll"
  if (Test-Path -LiteralPath $extensionPath) {
    return $extensionPath
  }

  $extensions = & $WixCommand extension list 2>&1
  if ($LASTEXITCODE -eq 0 -and (($extensions | Out-String) -match "WixToolset\.UI\.wixext\s+$([regex]::Escape($wixVersion))")) {
    if (Test-Path -LiteralPath $extensionPath) {
      return $extensionPath
    }
  }

  $extensionPackage = "WixToolset.UI.wixext/$wixVersion"
  Write-Host "Installing WiX extension: $extensionPackage"
  & $WixCommand extension add -g $extensionPackage
  if ($LASTEXITCODE -ne 0) {
    throw "WiX UI extension install failed. Run '$WixCommand extension add -g $extensionPackage' and retry."
  }
  if (-not (Test-Path -LiteralPath $extensionPath)) {
    throw "WiX UI extension installed but expected dll is missing: $extensionPath"
  }
  return $extensionPath
}

function Push-RepoLocation($Path) {
  if ($Path -like "\\*") {
    $driveName = "SX" + ([Guid]::NewGuid().ToString("N").Substring(0, 8))
    New-PSDrive -Name $driveName -PSProvider FileSystem -Root $Path -Scope Script | Out-Null
    Push-Location "${driveName}:\"
    return $driveName
  }

  Push-Location $Path
  return $null
}

function Get-NativePath($Path) {
  $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Path)
}

function ConvertTo-WslPath($Path) {
  if ($Path -match '^\\\\wsl(?:\.localhost|\$)\\([^\\]+)\\(.+)$') {
    return @{
      Distro = $matches[1]
      Path = "/" + ($matches[2] -replace "\\", "/")
    }
  }
  return $null
}

function Invoke-Checkpoint($RepoPath) {
  $wslPath = ConvertTo-WslPath $RepoPath
  if (-not $wslPath) {
    throw "Checkpoint from Windows requires a WSL repo path or -SkipCheckpoint: $RepoPath"
  }
  if (-not (Test-Command "wsl.exe")) {
    throw "wsl.exe not found; run with -SkipCheckpoint only for local smoke diagnostics"
  }
  & wsl.exe -d $wslPath.Distro --cd $wslPath.Path bash -lc "tooling/qa/wrappers/checkpoint.sh"
  if ($LASTEXITCODE -ne 0) {
    throw "checkpoint failed with exit code $LASTEXITCODE"
  }
}

function Get-VisualStudioInstallPath {
  $vswhereCandidates = @(
    "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe",
    "${env:ProgramFiles}\Microsoft Visual Studio\Installer\vswhere.exe"
  )
  $vswhere = $vswhereCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
  if ($vswhere) {
    $installPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 Microsoft.VisualStudio.Component.Windows10SDK -property installationPath
    if ($installPath) {
      return $installPath
    }
  }
  return $null
}

function Import-VsDevEnvironment {
  $installPath = Get-VisualStudioInstallPath
  if (-not $installPath) {
    throw "Visual Studio Build Tools with C++ tools and Windows SDK were not found."
  }

  $vsDevCmd = Join-Path $installPath "Common7\Tools\VsDevCmd.bat"
  if (-not (Test-Path $vsDevCmd)) {
    throw "VsDevCmd.bat not found: $vsDevCmd"
  }

  Write-Host "Importing MSVC environment from $vsDevCmd"
  $previousLocation = Get-Location
  try {
    Set-Location $env:SystemRoot
    $envOutput = & cmd.exe /s /c "`"$vsDevCmd`" -arch=x64 -host_arch=x64 >nul && set"
  }
  finally {
    Set-Location $previousLocation
  }
  foreach ($line in $envOutput) {
    if ($line -match '^([^=]+)=(.*)$') {
      [Environment]::SetEnvironmentVariable($matches[1], $matches[2], "Process")
    }
  }

  if (-not (Test-Command "link.exe")) {
    throw "MSVC link.exe is still unavailable after importing VsDevCmd environment."
  }
}

function ConvertTo-Hex($Bytes) {
  -join ($Bytes | ForEach-Object { $_.ToString("x2") })
}

function Get-InstallId($ChannelName) {
  switch ($ChannelName) {
    "stable" { return "install-stable" }
    "beta" { return "install-beta" }
    "dev" { return "install-dev" }
  }
}

function Ensure-DevCodeSigningCertificate {
  $subject = "CN=Sniptale Local Dev Code Signing"
  $cert = Get-ChildItem Cert:\CurrentUser\My |
    Where-Object {
      $_.Subject -eq $subject -and
      $_.HasPrivateKey -and
      $_.NotAfter -gt (Get-Date)
    } |
    Sort-Object NotAfter -Descending |
    Select-Object -First 1

  if (-not $cert) {
    Write-Host "Creating local dev code-signing certificate"
    $cert = New-SelfSignedCertificate `
      -Type CodeSigningCert `
      -Subject $subject `
      -CertStoreLocation Cert:\CurrentUser\My `
      -KeyAlgorithm RSA `
      -KeyLength 2048 `
      -HashAlgorithm SHA256 `
      -NotAfter (Get-Date).AddYears(2)
  }

  $cerPath = Join-Path $env:TEMP "sniptale-local-dev-code-signing.cer"
  Export-Certificate -Cert $cert -FilePath $cerPath | Out-Null
  foreach ($store in @("Cert:\CurrentUser\Root", "Cert:\CurrentUser\TrustedPublisher")) {
    $trusted = Get-ChildItem $store |
      Where-Object { $_.Thumbprint -eq $cert.Thumbprint } |
      Select-Object -First 1
    if (-not $trusted) {
      Write-Host "Trusting local dev code-signing certificate in $store"
      Import-Certificate -FilePath $cerPath -CertStoreLocation $store | Out-Null
    }
  }

  return $cert
}

function Sign-DevExecutable($Path) {
  $cert = Ensure-DevCodeSigningCertificate
  Write-Host "Signing dev executable: $Path"
  $signature = Set-AuthenticodeSignature -FilePath $Path -Certificate $cert -HashAlgorithm SHA256
  if ($signature.Status -ne "Valid") {
    throw "Dev signing failed for ${Path}: $($signature.Status) $($signature.StatusMessage)"
  }
}

function Get-Sha256Hex($Path) {
  if (-not (Test-Path -LiteralPath $Path)) {
    throw "Cannot hash missing file: $Path"
  }
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Provision-PipeSecret($InstallDir, $ChannelName) {
  Add-Type -AssemblyName System.Security
  $installId = Get-InstallId $ChannelName
  $sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
  $secretBytes = New-Object byte[] 32
  $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
  try {
    $rng.GetBytes($secretBytes)
  }
  finally {
    $rng.Dispose()
  }
  $plain = [System.Text.Encoding]::UTF8.GetBytes((ConvertTo-Hex $secretBytes))
  $protected = [System.Security.Cryptography.ProtectedData]::Protect(
    $plain,
    $null,
    [System.Security.Cryptography.DataProtectionScope]::CurrentUser
  )
  $payload = [ordered]@{
    installId = $installId
    currentUserSid = $sid
    authAlgorithm = "pipe-auth-v2-hmac-sha256"
    protection = "current-user-dpapi"
    protectedSecretHex = ConvertTo-Hex $protected
  } | ConvertTo-Json -Depth 5
  [System.IO.File]::WriteAllText(
    (Join-Path $InstallDir "pipe-secret.json"),
    $payload + [Environment]::NewLine,
    [System.Text.UTF8Encoding]::new($false)
  )
  Assert-PipeSecretProvisioned $InstallDir $installId
}

function Assert-PipeSecretProvisioned($InstallDir, $ExpectedInstallId) {
  $path = Join-Path $InstallDir "pipe-secret.json"
  if (-not (Test-Path -LiteralPath $path)) {
    throw "Missing pipe secret: $path"
  }
  $secret = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
  if ($secret.installId -ne $ExpectedInstallId) {
    throw "Pipe secret installId mismatch: expected $ExpectedInstallId, got $($secret.installId)"
  }
  if ($secret.authAlgorithm -ne "pipe-auth-v2-hmac-sha256") {
    throw "Pipe secret authAlgorithm mismatch: expected pipe-auth-v2-hmac-sha256, got $($secret.authAlgorithm)"
  }
  if ($secret.protection -ne "current-user-dpapi") {
    throw "Pipe secret protection mismatch: expected current-user-dpapi, got $($secret.protection)"
  }
  if ([string]::IsNullOrWhiteSpace($secret.protectedSecretHex)) {
    throw "Pipe secret protectedSecretHex is empty"
  }
}

function Assert-SameFileHash($ActualPath, $ExpectedPath, $Description) {
  if (-not (Test-Path -LiteralPath $ActualPath)) {
    throw "Missing ${Description}: $ActualPath"
  }
  if (-not (Test-Path -LiteralPath $ExpectedPath)) {
    throw "Missing expected ${Description}: $ExpectedPath"
  }
  $actual = Get-FileHash -LiteralPath $ActualPath -Algorithm SHA256
  $expected = Get-FileHash -LiteralPath $ExpectedPath -Algorithm SHA256
  if ($actual.Hash -ne $expected.Hash) {
    throw "${Description} hash mismatch: expected $($expected.Hash.ToLowerInvariant()) from $ExpectedPath, got $($actual.Hash.ToLowerInvariant()) at $ActualPath"
  }
}

function Write-DevLocalBrowserTrust($InstallDir, $ExePath) {
  $payload = [ordered]@{
    kind = "sniptale-dev-local-browser-trust"
    enabled = $true
    scope = "current-user-browser-parent"
    executableSha256 = Get-Sha256Hex $ExePath
  } | ConvertTo-Json -Depth 5
  [System.IO.File]::WriteAllText(
    (Join-Path $InstallDir "dev-local-browser-trust.json"),
    $payload + [Environment]::NewLine,
    [System.Text.UTF8Encoding]::new($false)
  )
}

function Remove-DevLocalBrowserTrust($InstallDir) {
  $path = Join-Path $InstallDir "dev-local-browser-trust.json"
  if (Test-Path -LiteralPath $path) {
    Remove-Item -LiteralPath $path -Force
  }
}

function Assert-RegistryDefaultValue($Path, $ExpectedValue) {
  if (-not (Test-Path $Path)) {
    throw "Missing registry key: $Path"
  }
  $value = (Get-Item $Path).GetValue("")
  if ($value -ne $ExpectedValue) {
    throw "Registry default value mismatch for ${Path}: expected '$ExpectedValue', got '$value'"
  }
}

function Stop-NativeAgentProcesses {
  Get-Process sniptale-native-agent -ErrorAction SilentlyContinue | Stop-Process -Force
}

function Archive-Logs($InstallDir) {
  $logDir = Join-Path $InstallDir "logs"
  if (-not (Test-Path -LiteralPath $logDir)) {
    return
  }
  if ($KeepLogs) {
    return
  }
  $archiveRoot = Join-Path (Get-NativePath ".tmp") "windows-smoke-logs"
  New-Item -ItemType Directory -Force $archiveRoot | Out-Null
  $archiveDir = Join-Path $archiveRoot (Get-Date -Format "yyyyMMdd-HHmmss")
  New-Item -ItemType Directory -Force $archiveDir | Out-Null
  Get-ChildItem -LiteralPath $logDir -Filter "*.jsonl" -ErrorAction SilentlyContinue |
    Move-Item -Destination $archiveDir -Force
  Write-Host "archived old logs: $archiveDir"
}

function Show-SmokeDiagnostics($InstallDir, $Exe) {
  Write-Host "diagnostics:"
  if (Test-Path -LiteralPath $Exe) {
    $item = Get-Item -LiteralPath $Exe
    $hash = Get-FileHash -LiteralPath $Exe -Algorithm SHA256
    Write-Host "exe=$($item.FullName)"
    Write-Host "exeLastWriteUtc=$($item.LastWriteTimeUtc.ToString('o'))"
    Write-Host "exeBytes=$($item.Length)"
    Write-Host "exeSha256=$($hash.Hash.ToLowerInvariant())"
  }
  Get-Process sniptale-native-agent -ErrorAction SilentlyContinue |
    Select-Object Id, ProcessName, StartTime, Path |
    Format-Table -AutoSize |
    Out-String |
    Write-Host

  $logDir = Join-Path $InstallDir "logs"
  if (Test-Path -LiteralPath $logDir) {
    Get-ChildItem -LiteralPath $logDir -Filter "*.jsonl" |
      Sort-Object LastWriteTimeUtc -Descending |
      Select-Object -First 8 |
      ForEach-Object {
        Write-Host "log=$($_.FullName) lastWriteUtc=$($_.LastWriteTimeUtc.ToString('o')) bytes=$($_.Length)"
        Get-Content -LiteralPath $_.FullName -Tail 12 | ForEach-Object { Write-Host $_ }
      }
  }
}

function Assert-ContainsNumber($Values, $Expected, $Description) {
  foreach ($value in @($Values)) {
    if ([int] $value -eq [int] $Expected) {
      return
    }
  }
  throw "$Description must include $Expected"
}

function Add-SmokeEvidence($Category, $Probe, $Evidence) {
  $script:SmokeEvidence += [pscustomobject] [ordered]@{
      category = $Category
      probe = $Probe
      evidence = $Evidence
    }
}

function Write-SmokeEvidenceArtifact($OutDir) {
  New-Item -ItemType Directory -Force $OutDir | Out-Null
  $path = Join-Path $OutDir "windows-smoke-evidence.json"
  $payload = [pscustomobject] [ordered]@{
    generatedAtUtc = (Get-Date).ToUniversalTime().ToString("o")
    categories = @($script:SmokeEvidence)
  }
  [System.IO.File]::WriteAllText(
    (Get-NativePath $path),
    ($payload | ConvertTo-Json -Depth 10) + [Environment]::NewLine,
    [System.Text.UTF8Encoding]::new($false)
  )
  return (Resolve-Path $path).Path
}

function Invoke-CargoExactTest($Category, $TestName) {
  Write-Host "probe[$Category]: $TestName"
  $previousIncremental = [Environment]::GetEnvironmentVariable("CARGO_INCREMENTAL", "Process")
  [Environment]::SetEnvironmentVariable("CARGO_INCREMENTAL", "0", "Process")
  try {
    cargo test $TestName -- --exact
    if ($LASTEXITCODE -ne 0) {
      throw "cargo smoke probe failed for ${Category}: $TestName"
    }
  }
  finally {
    [Environment]::SetEnvironmentVariable("CARGO_INCREMENTAL", $previousIncremental, "Process")
  }
  Add-SmokeEvidence $Category "cargo test --exact" $TestName
}

function Invoke-TopologyFailureProbes {
  $probes = @(
    @{
      Category = "worker-startup-shutdown"
      Test = "app::tests::resident_bridge::resident_agent_runtime_starts_workers_and_stops_in_reverse_order"
    },
    @{
      Category = "browser-eof-active-operation"
      Test = "app::tests::resident_bridge::resident_agent_pipe_session_writes_hello_before_browser_eof"
    },
    @{
      Category = "browser-eof-active-operation"
      Test = "app::tests::resident_bridge::duplex_pipe_server_stop_cancels_active_session_read"
    },
    @{
      Category = "resident-restart-pending-transfer"
      Test = "app::tests::native_session::controller_acquire_reports_pending_recordings_after_reconnect"
    },
    @{
      Category = "resident-restart-pending-transfer"
      Test = "app::tests::native_session::controller_acquire_reports_pending_screenshots_after_reconnect"
    },
    @{
      Category = "stale-challenge-replay"
      Test = "ipc::tests::pipe_auth::replayed_or_expired_challenge_is_rejected"
    },
    @{
      Category = "queue-backpressure-category"
      Test = "app::tests::command_flow::native_host_reports_worker_queue_backpressure"
    },
    @{
      Category = "queue-backpressure-category"
      Test = "ipc::tests::pipe_auth::session_limit_and_queue_backpressure_are_bounded"
    },
    @{
      Category = "malformed-authenticated-frame"
      Test = "app::tests::resident_bridge::resident_agent_pipe_session_rejects_malformed_forwarded_payload_after_auth"
    },
    @{
      Category = "malformed-authenticated-frame"
      Test = "app::tests::resident_bridge::pipe_server_continues_after_malformed_authenticated_session"
    }
  )
  foreach ($probe in $probes) {
    Invoke-CargoExactTest $probe.Category $probe.Test
  }
}

function Invoke-ProcessCapture($Exe, [string[]] $Arguments, [byte[]] $InputBytes, $TimeoutMs) {
  $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
  $startInfo.FileName = $Exe
  $startInfo.Arguments = ($Arguments | ForEach-Object { '"' + ($_.Replace('"', '\"')) + '"' }) -join " "
  $startInfo.UseShellExecute = $false
  $startInfo.RedirectStandardOutput = $true
  $startInfo.RedirectStandardError = $true
  $startInfo.RedirectStandardInput = $null -ne $InputBytes
  $startInfo.CreateNoWindow = $true
  $process = [System.Diagnostics.Process]::new()
  $process.StartInfo = $startInfo
  if (-not $process.Start()) {
    throw "Failed to start process: $Exe"
  }
  if ($null -ne $InputBytes) {
    $process.StandardInput.BaseStream.Write($InputBytes, 0, $InputBytes.Length)
    $process.StandardInput.Close()
  }
  if (-not $process.WaitForExit($TimeoutMs)) {
    try {
      $process.Kill()
    }
    catch {
    }
    throw "Timed out waiting for process: $Exe $($Arguments -join ' ')"
  }
  return [ordered]@{
    exitCode = $process.ExitCode
    stdout = $process.StandardOutput.ReadToEnd()
    stderr = $process.StandardError.ReadToEnd()
  }
}

function Assert-NativeHostOriginGate($Exe, $TimeoutMs) {
  $wrongOrigin = "chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/"
  $result = Invoke-ProcessCapture $Exe @($wrongOrigin) $null $TimeoutMs
  if ($result.exitCode -ne 64) {
    throw "native-host wrong-origin probe expected exit 64, got $($result.exitCode)"
  }
  if ($result.stderr -notmatch "unsupported argument") {
    throw "native-host wrong-origin probe did not report unsupported argument: $($result.stderr)"
  }
  if ($result.stdout.Trim().Length -ne 0) {
    throw "native-host wrong-origin probe wrote stdout unexpectedly: $($result.stdout)"
  }
  Add-SmokeEvidence "native-host-origin-gate" "installed executable wrong-origin probe" "unsupported argument before bridge startup"
}

function New-NativeMessageFrame($Message) {
  $payload = [System.Text.Encoding]::UTF8.GetBytes(($Message | ConvertTo-Json -Compress -Depth 10))
  if ($payload.Length -gt 1048576) {
    throw "Native Messaging smoke payload exceeds framing limit"
  }
  $stream = [System.IO.MemoryStream]::new()
  try {
    $length = [BitConverter]::GetBytes([uint32] $payload.Length)
    $stream.Write($length, 0, $length.Length)
    $stream.Write($payload, 0, $payload.Length)
    return $stream.ToArray()
  }
  finally {
    $stream.Dispose()
  }
}

function Read-NativeMessageFrames([byte[]] $Payload) {
  $frames = [System.Collections.Generic.List[object]]::new()
  $offset = 0
  while ($offset -lt $Payload.Length) {
    if (($Payload.Length - $offset) -lt 4) {
      throw "Native Messaging probe returned a truncated frame header"
    }
    $length = [BitConverter]::ToUInt32($Payload, $offset)
    $offset += 4
    if ($length -eq 0 -or $length -gt 1048576 -or $length -gt ($Payload.Length - $offset)) {
      throw "Native Messaging probe returned an invalid frame length"
    }
    try {
      $frames.Add(([System.Text.Encoding]::UTF8.GetString($Payload, $offset, [int] $length) | ConvertFrom-Json))
    }
    catch {
      throw "Native Messaging probe returned invalid JSON"
    }
    $offset += [int] $length
  }
  return $frames.ToArray()
}

function Invoke-NativeHostProbe($Exe, [string[]] $Arguments, [byte[]] $InputBytes, $TimeoutMs) {
  $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
  $startInfo.FileName = $Exe
  $startInfo.Arguments = ($Arguments | ForEach-Object { '"' + ($_.Replace('"', '\"')) + '"' }) -join " "
  $startInfo.UseShellExecute = $false
  $startInfo.RedirectStandardOutput = $true
  $startInfo.RedirectStandardError = $true
  $startInfo.RedirectStandardInput = $true
  $startInfo.CreateNoWindow = $true
  $process = [System.Diagnostics.Process]::new()
  $process.StartInfo = $startInfo
  if (-not $process.Start()) {
    throw "Failed to start native-host probe"
  }
  $process.StandardInput.BaseStream.Write($InputBytes, 0, $InputBytes.Length)
  $process.StandardInput.Close()
  if (-not $process.WaitForExit($TimeoutMs)) {
    try {
      $process.Kill()
    }
    catch {
    }
    throw "Timed out waiting for native-host probe"
  }
  $stdout = [System.IO.MemoryStream]::new()
  try {
    $process.StandardOutput.BaseStream.CopyTo($stdout)
    $stderr = $process.StandardError.ReadToEnd()
    return [ordered]@{
      exitCode = $process.ExitCode
      stdout = $stdout.ToArray()
      stderr = $stderr
    }
  }
  finally {
    $stdout.Dispose()
    $process.Dispose()
  }
}

function Assert-NativeHostSessionProbes($Exe, $ExtensionOrigin, $ExtensionVersion, $Count, $TimeoutMs) {
  $originMatch = [regex]::Match($ExtensionOrigin, '^chrome-extension://([a-p]{32})/$')
  if (-not $originMatch.Success) {
    throw "Native-host session probe requires a validated extension origin"
  }
  $extensionId = $originMatch.Groups[1].Value
  for ($index = 1; $index -le $Count; $index += 1) {
    $connectionId = "smoke-connection-$index"
    $hello = [ordered]@{
      type = "extension.hello"
      protocolVersion = 1
      settingsSchemaVersion = 1
      extensionVersion = $ExtensionVersion
      extensionId = $extensionId
      minAppVersion = "0.1.0"
      supportedProtocolVersions = @(1)
      supportedSettingsSchemaVersions = @(1)
    }
    $acquire = [ordered]@{
      type = "extension.controller.acquire"
      protocolVersion = 1
      connectionId = $connectionId
      browserFamily = "chrome"
      profileKey = "smoke-profile"
      documentId = "smoke-document-$index"
      requestedAtEpochMs = 1000 + $index
      reason = "initial-connect"
    }
    $input = [System.IO.MemoryStream]::new()
    try {
      foreach ($message in @($hello, $acquire)) {
        $frame = New-NativeMessageFrame $message
        $input.Write($frame, 0, $frame.Length)
      }
      $result = Invoke-NativeHostProbe $Exe @("--native-host", $ExtensionOrigin) $input.ToArray() $TimeoutMs
    }
    finally {
      $input.Dispose()
    }
    if ($result.exitCode -ne 0 -or -not [string]::IsNullOrWhiteSpace($result.stderr)) {
      throw "native-host probe $index failed without a clean stdio session"
    }
    $frames = @(Read-NativeMessageFrames $result.stdout)
    if (@($frames | Where-Object { $_.type -eq "app.hello" }).Count -ne 1) {
      throw "native-host probe $index did not return exactly one app.hello"
    }
    $leases = @($frames | Where-Object { $_.type -eq "app.controller.lease" })
    if ($leases.Count -ne 1 -or [string]::IsNullOrWhiteSpace([string] $leases[0].controllerLeaseId)) {
      throw "native-host probe $index did not return a controller lease"
    }
    Add-SmokeEvidence "native-host-session" "installed app.hello/controller probe $index" "framed session completed"
  }
}

function Invoke-NativeAgentJsonMode($Exe, $Flag, $ExpectedMode, $TimeoutMs, $Environment = @{}) {
  $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
  $startInfo.FileName = $Exe
  $startInfo.Arguments = $Flag
  $startInfo.UseShellExecute = $false
  $startInfo.RedirectStandardOutput = $true
  $startInfo.RedirectStandardError = $true
  $startInfo.CreateNoWindow = $true
  foreach ($key in $Environment.Keys) {
    $startInfo.Environment[$key] = $Environment[$key]
  }

  $process = [System.Diagnostics.Process]::new()
  $process.StartInfo = $startInfo
  if (-not $process.Start()) {
    throw "Failed to start native agent support mode: $Flag"
  }
  # Start draining redirected streams before waiting for process exit. Doctor/diagnostic JSON can
  # exceed the Windows anonymous-pipe buffer; waiting first would deadlock a healthy child in its
  # stdout write and report a false timeout.
  $stdoutTask = $process.StandardOutput.ReadToEndAsync()
  $stderrTask = $process.StandardError.ReadToEndAsync()
  if (-not $process.WaitForExit($TimeoutMs)) {
    try {
      $process.Kill()
    }
    catch {
    }
    throw "Timed out waiting for native agent support mode: $Flag"
  }

  $stdout = $stdoutTask.GetAwaiter().GetResult()
  $stderr = $stderrTask.GetAwaiter().GetResult()
  if ($process.ExitCode -ne 0) {
    throw "Native agent support mode $Flag exited with $($process.ExitCode): $stderr"
  }
  if ($stderr.Trim().Length -ne 0) {
    throw "Native agent support mode $Flag wrote stderr: $stderr"
  }
  try {
    $payload = $stdout | ConvertFrom-Json
  }
  catch {
    throw "Native agent support mode $Flag returned invalid JSON: $stdout"
  }
  if ($payload.processMode -ne $ExpectedMode) {
    throw "Native agent support mode $Flag processMode mismatch: expected $ExpectedMode, got $($payload.processMode)"
  }
  return $payload
}

function Assert-CollectDiagnosticsBundle($Exe, $TimeoutMs) {
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $bundle = Invoke-NativeAgentJsonMode $Exe "--collect-diagnostics" "collect-diagnostics" $TimeoutMs
  if ($bundle.status -ne "bundle-written") {
    throw "collect-diagnostics status mismatch: expected bundle-written, got $($bundle.status)"
  }
  if ($bundle.bundlePathCategory -ne "temp-diagnostics") {
    throw "collect-diagnostics bundlePathCategory mismatch: $($bundle.bundlePathCategory)"
  }
  if ($bundle.entriesWritten -lt 6) {
    throw "collect-diagnostics entriesWritten is too small: $($bundle.entriesWritten)"
  }
  if ([string]::IsNullOrWhiteSpace($bundle.bundleFileName) -or -not $bundle.bundleFileName.EndsWith(".zip")) {
    throw "collect-diagnostics bundleFileName is invalid: $($bundle.bundleFileName)"
  }
  $bundlePath = Join-Path (Join-Path (Join-Path $env:TEMP "sniptale-native-agent") "diagnostics") $bundle.bundleFileName
  if (-not (Test-Path -LiteralPath $bundlePath)) {
    throw "collect-diagnostics bundle is missing: $bundlePath"
  }
  $archive = [System.IO.Compression.ZipFile]::OpenRead($bundlePath)
  try {
    $entryNames = @($archive.Entries | ForEach-Object { $_.FullName })
  }
  finally {
    $archive.Dispose()
  }
  foreach ($required in @("manifest.json", "version.json", "doctor.json", "capability-snapshot.json", "recent-errors.json", "observability.json")) {
    if ($entryNames -notcontains $required) {
      throw "collect-diagnostics bundle missing entry: $required"
    }
  }
  Add-SmokeEvidence "support-bundle-recent-errors" "installed --collect-diagnostics" "bundle contains recent-errors.json and observability.json"
}

function Assert-SupportModeOutputs($Exe, $TimeoutMs) {
  $version = Invoke-NativeAgentJsonMode $Exe "--version-json" "version-json" $TimeoutMs
  if ($version.platformKind -ne "windows") {
    throw "version-json platformKind mismatch: expected windows, got $($version.platformKind)"
  }
  Assert-ContainsNumber @($version.protocolVersion) 1 "version-json protocolVersion"
  Assert-ContainsNumber @($version.settingsSchemaVersion) 1 "version-json settingsSchemaVersion"
  Assert-ContainsNumber @($version.appCacheSchemaVersion) 1 "version-json appCacheSchemaVersion"
  if (-not $version.appVersion) {
    throw "version-json appVersion is missing"
  }
  if (-not $version.installerVersion) {
    throw "version-json installerVersion is missing"
  }
  if (-not $version.nativeHostManifestVersion) {
    throw "version-json nativeHostManifestVersion is missing"
  }

  $doctor = Invoke-NativeAgentJsonMode $Exe "--doctor-json" "doctor-json" $TimeoutMs
  if ($doctor.status -notin @("pass", "fail")) {
    throw "doctor-json status mismatch: $($doctor.status)"
  }
  if ($null -eq $doctor.repairRequired) {
    throw "doctor-json repairRequired is missing"
  }
  if ($null -eq $doctor.checks -or $doctor.checks.Count -lt 1) {
    throw "doctor-json checks are missing"
  }
  $runtimeAccess = @($doctor.checks | Where-Object { $_.name -eq "runtime-directory-access" })
  if ($runtimeAccess.Count -ne 1) {
    throw "doctor-json runtime-directory-access check is missing"
  }
  if ($null -eq $runtimeAccess[0].categories -or $runtimeAccess[0].categories.Count -lt 4) {
    throw "doctor-json runtime-directory-access categories are missing"
  }
  $tempMedia = @($runtimeAccess[0].categories | Where-Object { $_.category -eq "temp-media" })
  if ($tempMedia.Count -ne 1) {
    throw "doctor-json runtime-directory-access temp-media category is missing"
  }
  foreach ($category in $runtimeAccess[0].categories) {
    if ($category.category -match '[\\/]') {
      throw "doctor-json runtime-directory-access category leaks a path: $($category.category)"
    }
    if ($category.status -notin @("protected", "broad-read-access", "broad-write-access", "unknown", "unsupported-platform")) {
      throw "doctor-json runtime-directory-access category status is unknown: $($category.status)"
    }
  }
  Assert-CollectDiagnosticsBundle $Exe $TimeoutMs
}

function Assert-RuntimeDirectoryAccessFailClosed($Exe, $TimeoutMs) {
  Assert-TempMediaAclStatus $Exe $TimeoutMs "M" "broad-write-access"
  Assert-TempMediaAclStatus $Exe $TimeoutMs "RX" "broad-read-access"
}

function Set-TempMediaBroadAcl($Path, $Grant) {
  $currentUserSid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
  & icacls.exe $Path /inheritance:r | Out-Null
  if ($LASTEXITCODE -ne 0) {
    throw "icacls failed while disabling temp-media ACL inheritance"
  }
  & icacls.exe `
    $Path `
    /grant:r `
    "*${currentUserSid}:(OI)(CI)(F)" `
    "*S-1-5-18:(OI)(CI)(F)" `
    "*S-1-5-32-544:(OI)(CI)(F)" `
    "*S-1-5-32-545:(OI)(CI)($Grant)" | Out-Null
  if ($LASTEXITCODE -ne 0) {
    throw "icacls failed while preparing broad temp-media ACL"
  }
}

function Assert-TempMediaAclStatus($Exe, $TimeoutMs, $Grant, $ExpectedStatus) {
  $runtimeRoot = Join-Path $env:TEMP ("sniptale-native-agent-acl-" + [Guid]::NewGuid().ToString("N"))
  $localAppData = Join-Path $runtimeRoot "LocalAppData"
  $tempMedia = Join-Path $localAppData "Sniptale\NativeAgent\temp-media"
  try {
    New-Item -ItemType Directory -Force $tempMedia | Out-Null
    Set-TempMediaBroadAcl $tempMedia $Grant

    $doctor = Invoke-NativeAgentJsonMode `
      $Exe `
      "--doctor-json" `
      "doctor-json" `
      $TimeoutMs `
      @{ LOCALAPPDATA = $localAppData }
    if ($doctor.repairRequired -ne $true) {
      throw "doctor-json must require repair for broad temp-media ACL"
    }
    $runtimeAccess = @($doctor.checks | Where-Object { $_.name -eq "runtime-directory-access" })
    if ($runtimeAccess.Count -ne 1 -or $runtimeAccess[0].status -ne "fail") {
      throw "doctor-json runtime-directory-access must fail for broad temp-media ACL"
    }
    $tempMediaStatus = @($runtimeAccess[0].categories | Where-Object { $_.category -eq "temp-media" })
    if ($tempMediaStatus.Count -ne 1 -or $tempMediaStatus[0].status -ne $ExpectedStatus) {
      $actual = if ($tempMediaStatus.Count -eq 1) { $tempMediaStatus[0].status } else { "<missing>" }
      throw "doctor-json temp-media category must report $ExpectedStatus for broad ACL, got $actual"
    }
  }
  finally {
    Remove-Item -LiteralPath $runtimeRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
}


if ($ExtensionOrigin -notmatch '^chrome-extension://([a-p]{32})/$') {
  throw "Extension origin must be exact chrome-extension://<32-letter-id>/: $ExtensionOrigin"
}

$hostNames = @{
  stable = "com.sniptale.native_host"
  beta = "com.sniptale.beta.native_host"
  dev = "com.sniptale.dev.native_host"
}

$originTokens = @{
  stable = "{{SNIPTALE_STABLE_EXTENSION_ORIGIN}}"
  beta = "{{SNIPTALE_BETA_EXTENSION_ORIGIN}}"
  dev = "{{SNIPTALE_DEV_EXTENSION_ORIGIN}}"
}

$mappedDrive = Push-RepoLocation $Repo
$installDir = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent"
$installedExe = Join-Path $installDir "sniptale-native-agent.exe"
$installedManifest = Join-Path $installDir "native-host.json"
$installedLicense = Join-Path $installDir "LICENSE.txt"
$installedNotice = Join-Path $installDir "NOTICE.txt"
$installedThirdPartyNotices = Join-Path $installDir "THIRD_PARTY_NOTICES.md"
$installedThirdPartyDependencies = Join-Path $installDir "THIRD_PARTY_DEPENDENCIES.json"
try {
  if (-not $SkipCheckpoint) {
    Invoke-Step "checkpoint in WSL" {
      Invoke-Checkpoint $Repo
    }
  }

  if (-not (Test-Command "cargo")) {
    throw "cargo not found in PATH"
  }
  if (-not (Test-Command "python")) {
    throw "python not found in PATH"
  }
  if (-not (Test-Command $Wix)) {
    throw "WiX v4 CLI not found in PATH: $Wix"
  }
  $wixUiExtension = Ensure-WixUiExtension $Wix
  Import-VsDevEnvironment

  if (-not $ProductVersion) {
    $cargoToml = Get-Content "Cargo.toml" -Raw
    $versionMatch = [regex]::Match($cargoToml, '(?m)^version = "([^"]+)"')
    if (-not $versionMatch.Success) {
      throw "Could not determine package version from Cargo.toml"
    }
    $ProductVersion = $versionMatch.Groups[1].Value
  }

  Invoke-Step "stop native agent processes" {
    Stop-NativeAgentProcesses
  }
  Archive-Logs $installDir

  $env:SNIPTALE_INSTALL_CHANNEL = $Channel
  $env:SNIPTALE_ALLOWED_EXTENSION_ORIGIN = $ExtensionOrigin
  if ($DevLocalTrust) {
    $env:SNIPTALE_DEV_LOCAL_BROWSER_TRUST = "1"
  }
  else {
    Remove-Item Env:\SNIPTALE_DEV_LOCAL_BROWSER_TRUST -ErrorAction SilentlyContinue
  }
  Invoke-Step "verify Windows resource inputs" {
    python tooling/release/windows/verify_windows_resources.py
  }
  Invoke-Step "build windows-msvc release executable" {
    $env:SNIPTALE_WINDOWS_RESOURCES = "require"
    cargo build --release --target x86_64-pc-windows-msvc --bin sniptale-native-agent --all-features
  }

  $outDir = ".tmp\release\windows-$Channel"
  New-Item -ItemType Directory -Force $outDir | Out-Null
  Invoke-Step "windows topology failure probes" {
    Invoke-TopologyFailureProbes
  }
  $manifestPath = Join-Path $outDir "native-host.json"
  $manifest = Get-Content "installer\native-host\$Channel.json.template" -Raw
  $manifest = $manifest.Replace("{{SNIPTALE_NATIVE_AGENT_EXE}}", "sniptale-native-agent.exe")
  $manifest = $manifest.Replace($originTokens[$Channel], $ExtensionOrigin)
  $manifestJson = $manifest | ConvertFrom-Json | ConvertTo-Json -Depth 10
  [System.IO.File]::WriteAllText(
    (Get-NativePath $manifestPath),
    $manifestJson + [Environment]::NewLine,
    [System.Text.UTF8Encoding]::new($false)
  )

  $exePath = "target\x86_64-pc-windows-msvc\release\sniptale-native-agent.exe"
  Invoke-Step "verify Windows executable resources" {
    python tooling/release/windows/verify_windows_pe_resources.py $exePath
  }
  $msiPath = Join-Path $outDir "SniptaleNativeAgent-$Channel-$ProductVersion.msi"
  $stageRoot = Join-Path $env:TEMP ("sniptale-native-smoke-" + [Guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Force $stageRoot | Out-Null
  $stageWxs = Join-Path $stageRoot "SniptaleNativeAgent.wxs"
  $stageExe = Join-Path $stageRoot "sniptale-native-agent.exe"
  $stageManifest = Join-Path $stageRoot "native-host.json"
  $stageLicense = Join-Path $stageRoot "LICENSE.txt"
  $stageNotice = Join-Path $stageRoot "NOTICE.txt"
  $stageThirdPartyNotices = Join-Path $stageRoot "THIRD_PARTY_NOTICES.md"
  $stageThirdPartyDependencies = Join-Path $stageRoot "THIRD_PARTY_DEPENDENCIES.json"
  $stageMsi = Join-Path $stageRoot "SniptaleNativeAgent-$Channel-$ProductVersion.msi"
  Copy-Item "installer\wix\SniptaleNativeAgent.wxs" $stageWxs -Force
  Copy-Item $exePath $stageExe -Force
  Copy-Item $manifestPath $stageManifest -Force
  Copy-Item "LICENSE" $stageLicense -Force
  Copy-Item "NOTICE" $stageNotice -Force
  Copy-Item "THIRD_PARTY_NOTICES.md" $stageThirdPartyNotices -Force
  Copy-Item "THIRD_PARTY_DEPENDENCIES.json" $stageThirdPartyDependencies -Force
  if ($DevLocalTrust) {
    Sign-DevExecutable $stageExe
  }

  Invoke-Step "build windows msi" {
    & $Wix build $stageWxs `
      -ext "$wixUiExtension" `
      -d "NativeAgentExe=$stageExe" `
      -d "NativeHostManifest=$stageManifest" `
      -d "ProjectLicense=$stageLicense" `
      -d "ProjectNotice=$stageNotice" `
      -d "ThirdPartyNotices=$stageThirdPartyNotices" `
      -d "ThirdPartyDependencies=$stageThirdPartyDependencies" `
      -d "AppIcon=resources\windows\icons\sniptale.ico" `
      -d "NativeHostName=$($hostNames[$Channel])" `
      -d "ProductVersion=$ProductVersion" `
      -o "$stageMsi"
  }
  Invoke-Step "verify Windows MSI identity" {
    & "tooling\release\windows\verify_msi_identity.ps1" -Msi $stageMsi -ExpectedProductVersion $ProductVersion
  }
  Copy-Item $stageMsi $msiPath -Force

  $installLog = Join-Path $env:TEMP "sniptale-native-agent-smoke-install-$Channel.log"
  Invoke-Step "install windows msi" {
    $process = Start-Process msiexec.exe `
      -ArgumentList @("/i", $stageMsi, "ADDLOCAL=ALL", "/qn", "REBOOT=ReallySuppress", "/L*V", $installLog) `
      -Wait `
      -NoNewWindow `
      -PassThru
    if ($process.ExitCode -ne 0) {
      throw "msiexec failed with exit code $($process.ExitCode). Log: $installLog"
    }
  }

  Assert-SameFileHash $installedExe $stageExe "installed executable"
  Assert-SameFileHash $installedManifest $stageManifest "installed native-host manifest"
  Assert-SameFileHash $installedLicense $stageLicense "installed project license"
  Assert-SameFileHash $installedNotice $stageNotice "installed project notice"
  Assert-SameFileHash $installedThirdPartyNotices $stageThirdPartyNotices "installed third-party notices"
  Assert-SameFileHash $installedThirdPartyDependencies $stageThirdPartyDependencies "installed third-party dependency manifest"
  Invoke-Step "verify installed executable resources" {
    python tooling/release/windows/verify_windows_pe_resources.py $installedExe
  }
  if ($DevLocalTrust) {
    Provision-PipeSecret $installDir $Channel
    Write-DevLocalBrowserTrust $installDir $installedExe
  }
  else {
    Remove-DevLocalBrowserTrust $installDir
  }

  $installed = Get-Content $installedManifest -Raw | ConvertFrom-Json
  if ($installed.name -ne $hostNames[$Channel]) {
    throw "Installed manifest host mismatch: expected $($hostNames[$Channel]), got $($installed.name)"
  }
  if ($installed.allowed_origins.Count -ne 1 -or $installed.allowed_origins[0] -ne $ExtensionOrigin) {
    throw "Installed manifest origin mismatch: expected $ExtensionOrigin, got $($installed.allowed_origins -join ', ')"
  }
  Assert-RegistryDefaultValue "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$($hostNames[$Channel])" $installedManifest
  Assert-RegistryDefaultValue "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\$($hostNames[$Channel])" $installedManifest
  Assert-RegistryDefaultValue "HKCU:\Software\Chromium\NativeMessagingHosts\$($hostNames[$Channel])" $installedManifest

  Invoke-Step "installed support mode probes" {
    Assert-SupportModeOutputs $installedExe $TimeoutMs
    Assert-RuntimeDirectoryAccessFailClosed $installedExe $TimeoutMs
  }

  Invoke-Step "installed native-host origin gate probe" {
    Assert-NativeHostOriginGate $installedExe $TimeoutMs
  }

  Invoke-Step "launch resident agent" {
    Start-Process -FilePath $installedExe -WorkingDirectory $installDir
    Start-Sleep -Milliseconds 500
  }

  Invoke-Step "installed repeated native-host session probes" {
    Assert-NativeHostSessionProbes $installedExe $ExtensionOrigin $ExtensionVersion $ProbeCount $TimeoutMs
  }

  $evidencePath = Write-SmokeEvidenceArtifact $outDir

  Write-Host ""
  Write-Host "browser Native Messaging UI acceptance remains manual for consent prompt coverage."
  Write-Host "Automated Windows smoke evidence covers worker/control-plane failure categories."
  Write-Host "Expected origin: $ExtensionOrigin"

  Write-Host ""
  Write-Host "windows install/resident smoke OK"
  Write-Host "executable: $installedExe"
  Write-Host "manifest:   $installedManifest"
  Write-Host "msi:        $((Resolve-Path $msiPath).Path)"
  Write-Host "installLog: $installLog"
  Write-Host "evidence:   $evidencePath"
}
catch {
  Show-SmokeDiagnostics $installDir $installedExe
  throw
}
finally {
  Pop-Location
  if ($mappedDrive) {
    Remove-PSDrive -Name $mappedDrive -Scope Script -ErrorAction SilentlyContinue
  }
}
