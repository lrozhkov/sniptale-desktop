param(
  [Parameter(Mandatory = $true)][string] $Msi,
  [Parameter(Mandatory = $true)][string] $MsiSha256,
  [Parameter(Mandatory = $true)][string] $PreviousMsi,
  [Parameter(Mandatory = $true)][string] $PreviousMsiSha256,
  [Parameter(Mandatory = $true)][string] $PreviousExeSha256,
  [Parameter(Mandatory = $true)][string] $ExpectedExeSha256,
  [Parameter(Mandatory = $true)][string] $ExpectedNativeHostManifestSha256,
  [Parameter(Mandatory = $true)][string] $IdentityVerifier,
  [Parameter(Mandatory = $true)][string] $IdentityVerifierSha256,
  [Parameter(Mandatory = $true)][string] $BrowserProbe,
  [Parameter(Mandatory = $true)][string] $BrowserProbeSha256,
  [Parameter(Mandatory = $true)][string] $ProbeNonce,
  [string] $Output = "e2e-out",
  [string] $Cases = ""
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
$outputCandidate = if ([IO.Path]::IsPathRooted($Output)) { $Output } else { Join-Path $PWD $Output }
$outputPath = [IO.Path]::GetFullPath($outputCandidate)
Remove-Item -LiteralPath $outputPath -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $outputPath, (Join-Path $outputPath "screenshots"), (Join-Path $outputPath "recordings"), (Join-Path $outputPath "logs") | Out-Null
$progressPath = Join-Path $outputPath "logs\progress.log"
$selectedTests = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
if ($Cases) {
  foreach ($caseName in $Cases.Split(',')) {
    if ($caseName -notmatch '^[a-z0-9][a-z0-9-]{0,127}$') { throw "invalid selected E2E case" }
    [void]$selectedTests.Add($caseName)
  }
}
$tests = [Collections.Generic.List[object]]::new()
$installed = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent\sniptale-native-agent.exe"
$settingsTitle = "Sniptale $([char]0x2014) Manual settings"
$agent = $null
$script:manualProfileTemplate = $null
$script:browserProbeAttempt = 0

function Write-ProgressEvent(
  [string] $Scenario,
  [string] $Status,
  [int] $Attempt = 0,
  [int] $MaxAttempts = 0,
  [int] $DeadlineSeconds = 0
) {
  $value = [ordered]@{
    schemaVersion = 1
    scenario = $Scenario
    status = $Status
    attempt = $Attempt
    maxAttempts = $MaxAttempts
    deadlineSeconds = $DeadlineSeconds
    epoch = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
  }
  [IO.File]::AppendAllText(
    $progressPath,
    (($value | ConvertTo-Json -Compress) + [Environment]::NewLine),
    [Text.UTF8Encoding]::new($false)
  )
}

function Wait-InteractiveShell([int] $Seconds = 30) {
  Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class SniptaleShellProbe {
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string className, string windowName);
}
'@
  $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
  $explorer = Get-Process explorer -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $sessionId } | Select-Object -First 1
  if (-not $explorer) { $explorer = Start-Process explorer.exe -PassThru }
  $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
  while ([DateTime]::UtcNow -lt $deadline) {
    if ([SniptaleShellProbe]::FindWindow("Shell_TrayWnd", $null) -ne [IntPtr]::Zero) { return }
    Start-Sleep -Milliseconds 500
  }
  throw "interactive Explorer notification area is unavailable in session $sessionId"
}

Wait-InteractiveShell
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $Msi).Hash.ToLowerInvariant() -ne $MsiSha256.ToLowerInvariant()) {
  throw "MSI SHA-256 mismatch after guest transfer"
}
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $PreviousMsi).Hash.ToLowerInvariant() -ne $PreviousMsiSha256.ToLowerInvariant()) {
  throw "previous MSI SHA-256 mismatch after guest transfer"
}
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $IdentityVerifier).Hash.ToLowerInvariant() -ne $IdentityVerifierSha256.ToLowerInvariant()) {
  throw "MSI identity verifier SHA-256 mismatch after guest transfer"
}
& $IdentityVerifier -Msi $PreviousMsi -ExpectedProductVersion "0.0.1"
& $IdentityVerifier -Msi $Msi -ExpectedProductVersion "0.1.0"
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $BrowserProbe).Hash.ToLowerInvariant() -ne $BrowserProbeSha256.ToLowerInvariant()) {
  throw "browser probe SHA-256 mismatch after guest transfer"
}
if ($ProbeNonce -notmatch '^[0-9a-f]{64}$') { throw "browser probe nonce is invalid" }

function Add-Test([string] $Name, [scriptblock] $Body) {
  if ($selectedTests.Count -gt 0 -and -not $selectedTests.Contains($Name)) { return }
  $watch = [Diagnostics.Stopwatch]::StartNew()
  Write-ProgressEvent $Name "started"
  try {
    $detail = & $Body
    $tests.Add([ordered]@{ name = $Name; status = "pass"; duration_ms = [int]$watch.ElapsedMilliseconds; detail = "$detail" })
    Write-ProgressEvent $Name "pass"
  } catch {
    $tests.Add([ordered]@{ name = $Name; status = "fail"; duration_ms = [int]$watch.ElapsedMilliseconds; detail = $_.Exception.Message })
    Write-ProgressEvent $Name "fail"
  }
}

function Invoke-Msi([string[]] $Arguments, [string] $LogName) {
  $arguments = @($Arguments + @("/qn", "/norestart", "/l*v", (Join-Path $outputPath "logs\$LogName.log")))
  $process = Start-Process msiexec.exe -ArgumentList $arguments -Wait -PassThru
  if ($process.ExitCode -notin @(0, 1641, 3010)) { throw "msiexec exit=$($process.ExitCode)" }
}

function ConvertTo-LowerHex([byte[]] $Bytes) {
  -join ($Bytes | ForEach-Object { $_.ToString("x2") })
}

function Provision-DevRuntimeTrust {
  Add-Type -AssemblyName System.Security
  $directory = [IO.Path]::GetDirectoryName($installed)
  $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
  $secretBytes = [byte[]]::new(32)
  $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
  try { $rng.GetBytes($secretBytes) } finally { $rng.Dispose() }
  $plain = [Text.Encoding]::UTF8.GetBytes((ConvertTo-LowerHex $secretBytes))
  $protected = [Security.Cryptography.ProtectedData]::Protect(
    $plain,
    $null,
    [Security.Cryptography.DataProtectionScope]::CurrentUser
  )
  $secret = [ordered]@{
    installId = "install-dev"
    currentUserSid = $sid
    authAlgorithm = "pipe-auth-v2-hmac-sha256"
    protection = "current-user-dpapi"
    protectedSecretHex = ConvertTo-LowerHex $protected
  }
  [IO.File]::WriteAllText(
    (Join-Path $directory "pipe-secret.json"),
    ($secret | ConvertTo-Json -Depth 5 -Compress),
    [Text.UTF8Encoding]::new($false)
  )
  $marker = [ordered]@{
    kind = "sniptale-dev-local-browser-trust"
    enabled = $true
    scope = "current-user-browser-parent"
    executableSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $installed).Hash.ToLowerInvariant()
  }
  [IO.File]::WriteAllText(
    (Join-Path $directory "dev-local-browser-trust.json"),
    ($marker | ConvertTo-Json -Depth 4 -Compress),
    [Text.UTF8Encoding]::new($false)
  )
}

function Write-UpgradeAuthorizationFixture {
  Provision-DevRuntimeTrust
  $directory = [IO.Path]::GetDirectoryName($installed)
  $trusted = [ordered]@{ formatVersion = 1; trusted = @() }
  [IO.File]::WriteAllText(
    (Join-Path $directory "trusted-connections.json"),
    ($trusted | ConvertTo-Json -Depth 4 -Compress),
    [Text.UTF8Encoding]::new($false)
  )
  $script:upgradeAuthorizationHashes = @{}
  foreach ($name in @("pipe-secret.json", "dev-local-browser-trust.json", "trusted-connections.json")) {
    $script:upgradeAuthorizationHashes[$name] = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $directory $name)).Hash
  }
}

function Save-Desktop([string] $Name) {
  Add-Type -AssemblyName System.Drawing
  Add-Type -AssemblyName System.Windows.Forms
  $bounds = [Windows.Forms.SystemInformation]::VirtualScreen
  $bitmap = [Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
  $graphics = [Drawing.Graphics]::FromImage($bitmap)
  try { $graphics.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $bounds.Size); $bitmap.Save((Join-Path $outputPath "screenshots\$Name.png"), [Drawing.Imaging.ImageFormat]::Png) }
  finally { $graphics.Dispose(); $bitmap.Dispose() }
}

function Save-UiWindow([object] $Element, [string] $Name) {
  Add-Type -AssemblyName System.Drawing
  $bounds = $Element.Current.BoundingRectangle
  if ($bounds.Width -lt 100 -or $bounds.Height -lt 100) { throw "UI window bounds are invalid" }
  $bitmap = [Drawing.Bitmap]::new([int]$bounds.Width, [int]$bounds.Height)
  $graphics = [Drawing.Graphics]::FromImage($bitmap)
  try {
    $graphics.CopyFromScreen([int]$bounds.Left, [int]$bounds.Top, 0, 0, $bitmap.Size)
    $bitmap.Save((Join-Path $outputPath "screenshots\$Name.png"), [Drawing.Imaging.ImageFormat]::Png)
  } finally {
    $graphics.Dispose()
    $bitmap.Dispose()
  }
}

function Test-BinaryMarker([string] $Path, [string] $Marker) {
  $needle = [Text.Encoding]::ASCII.GetBytes($Marker)
  $stream = [IO.File]::OpenRead($Path)
  try {
    $buffer = [byte[]]::new(1048576 + $needle.Length)
    $carry = 0
    while (($read = $stream.Read($buffer, $carry, 1048576)) -gt 0) {
      $length = $carry + $read
      $text = [Text.Encoding]::ASCII.GetString($buffer, 0, $length)
      if ($text.Contains($Marker)) { return $true }
      $carry = [Math]::Min($needle.Length - 1, $length)
      [Array]::Copy($buffer, $length - $carry, $buffer, 0, $carry)
    }
    return $false
  } finally {
    $stream.Dispose()
  }
}

function Find-Ui([string] $Name, [int] $Seconds = 20) {
  Add-Type -AssemblyName UIAutomationClient
  $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
  $condition = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, $Name)
  while ([DateTime]::UtcNow -lt $deadline) {
    $item = [Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
    if ($item) { return $item }
    Start-Sleep -Milliseconds 500
  }
  throw "UI element not found: $Name"
}

function Restart-AgentWithMode([ValidateSet("manual", "extension-managed")][string] $Mode) {
  if ($script:agent -and -not $script:agent.HasExited) {
    Stop-Process -Id $script:agent.Id -Force -ErrorAction SilentlyContinue
    $null = $script:agent.WaitForExit(5000)
  }
  $ownedProcesses = @(Get-Process "sniptale-native-agent" -ErrorAction SilentlyContinue | Where-Object {
    try { [string]::Equals($_.Path, $installed, [StringComparison]::OrdinalIgnoreCase) } catch { $false }
  })
  foreach ($process in $ownedProcesses) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
  $deadline = [DateTime]::UtcNow.AddSeconds(15)
  do {
    $remaining = @(Get-Process "sniptale-native-agent" -ErrorAction SilentlyContinue | Where-Object {
      try { [string]::Equals($_.Path, $installed, [StringComparison]::OrdinalIgnoreCase) } catch { $false }
    })
    if ($remaining.Count -eq 0) { break }
    Start-Sleep -Milliseconds 100
  } while ([DateTime]::UtcNow -lt $deadline)
  if ($remaining.Count -ne 0) { throw "resident process did not stop before mode restart" }
  $profilePath = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent\local-profile.json"
  $profile = Get-Content -Raw -LiteralPath $profilePath | ConvertFrom-Json
  $profile.revision = [uint64]$profile.revision + 1
  $profile.operatingMode = $Mode
  if ($Mode -eq "manual" -and $null -eq $profile.manualProfile) {
    $profile.manualProfile = $script:manualProfileTemplate
  }
  [IO.File]::WriteAllText($profilePath, ($profile | ConvertTo-Json -Depth 8 -Compress), [Text.UTF8Encoding]::new($false))
  $script:agent = Start-Process -FilePath $installed -PassThru
  Start-Sleep -Seconds 4
  if ($script:agent.HasExited) {
    # MSI autostart can observe the forced stop and win the singleton race against Start-Process.
    # Accept that installed owner only when it is live and path-bound to the exact tested binary.
    $replacement = @(Get-Process "sniptale-native-agent" -ErrorAction SilentlyContinue | Where-Object {
      try { -not $_.HasExited -and [string]::Equals($_.Path, $installed, [StringComparison]::OrdinalIgnoreCase) } catch { $false }
    } | Sort-Object StartTime -Descending | Select-Object -First 1)
    if ($replacement.Count -ne 1) { throw "resident failed to start in $Mode mode" }
    $script:agent = $replacement[0]
  }
}

function Start-ProbeListener([string] $Nonce) {
  if (-not $script:probeListenerScript) {
    $script:probeListenerScript = Join-Path $env:TEMP "sniptale-e2e-probe-listener.ps1"
    $listenerSource = @'
param(
  [Parameter(Mandatory = $true)][string] $ExpectedNonce,
  [Parameter(Mandatory = $true)][string] $ReadyPath,
  [Parameter(Mandatory = $true)][string] $ResultPath,
  [Parameter(Mandatory = $true)][string] $ErrorPath
)
$ErrorActionPreference = "Stop"
$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 18765)
$client = $null
$reader = $null
try {
  $listener.Start()
  [IO.File]::WriteAllText($ReadyPath, "ready", [Text.ASCIIEncoding]::new())
  $client = $listener.AcceptTcpClient()
  $stream = $client.GetStream()
  $reader = [IO.StreamReader]::new($stream, [Text.Encoding]::ASCII, $false, 1024, $true)
  $length = 0
  while ($true) {
    $line = $reader.ReadLine()
    if ($null -eq $line -or $line.Length -eq 0) { break }
    if ($line -match '^Content-Length:\s*(\d+)$') { $length = [int]$Matches[1] }
  }
  if ($length -le 0 -or $length -gt 65536) { throw "invalid probe content length" }
  $buffer = [char[]]::new($length)
  $offset = 0
  while ($offset -lt $length) {
    $count = $reader.Read($buffer, $offset, $length - $offset)
    if ($count -le 0) { throw "probe request body ended early" }
    $offset += $count
  }
  $body = -join $buffer
  $value = $body | ConvertFrom-Json
  if ($value.nonce -ne $ExpectedNonce) { throw "browser probe nonce mismatch" }
  [IO.File]::WriteAllText($ResultPath, $body, [Text.UTF8Encoding]::new($false))
  $response = [Text.Encoding]::ASCII.GetBytes("HTTP/1.1 200 OK`r`nContent-Length: 2`r`nConnection: close`r`n`r`nOK")
  $stream.Write($response, 0, $response.Length)
  $stream.Flush()
} catch {
  [IO.File]::WriteAllText($ErrorPath, $_.Exception.Message, [Text.UTF8Encoding]::new($false))
  exit 1
} finally {
  if ($reader) { $reader.Dispose() }
  if ($client) { $client.Dispose() }
  $listener.Stop()
}
'@
    [IO.File]::WriteAllText($script:probeListenerScript, $listenerSource, [Text.UTF8Encoding]::new($false))
  }
  $id = [Guid]::NewGuid().ToString("N")
  $readyPath = Join-Path $outputPath "logs\probe-$id.ready"
  $resultPath = Join-Path $outputPath "logs\probe-$id.result"
  $errorPath = Join-Path $outputPath "logs\probe-$id.error"
  $arguments = @(
    "-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
    "-File", $script:probeListenerScript,
    "-ExpectedNonce", $Nonce,
    "-ReadyPath", $readyPath,
    "-ResultPath", $resultPath,
    "-ErrorPath", $errorPath
  )
  $process = Start-Process powershell.exe -ArgumentList $arguments -WindowStyle Hidden -PassThru
  [pscustomobject]@{ Process = $process; ReadyPath = $readyPath; ResultPath = $resultPath; ErrorPath = $errorPath }
}

function Install-UnpackedBrowserExtension([string] $Name, [IntPtr] $Window, [string] $ExtensionPath, [object] $DeveloperMode) {
  Add-Type -AssemblyName System.Windows.Forms
  Add-Type -AssemblyName UIAutomationClient
  if ($Name -eq "edge") {
    $edgeBounds = [SniptaleBrowserFocus+Rect]::new()
    if (-not [SniptaleBrowserFocus]::GetWindowRect($Window, [ref]$edgeBounds)) { throw "edge developer switch bounds are unavailable" }
    $developerBounds = $DeveloperMode.Current.BoundingRectangle
    $edgeToggleY = [int]($developerBounds.Top + ($developerBounds.Height / 2))
    [SniptaleBrowserFocus]::SetCursorPos($edgeBounds.Left + 352, $edgeToggleY) | Out-Null
    [SniptaleBrowserFocus]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
    [SniptaleBrowserFocus]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Seconds 2
    Save-Desktop "edge-developer-mode-enabled"
    Open-EdgeExtensionsMenu $Window
  } else {
    $toggleOffset = [int]($DeveloperMode.Current.BoundingRectangle.Width / 2) + 38
    Click-UiElement $DeveloperMode $toggleOffset
  }
  Start-Sleep -Seconds 1
  try {
    $loadUnpacked = Find-Ui "Load unpacked" 15
  } catch {
    if ($Name -eq "edge") {
      throw
    } else {
      Click-UiElement $DeveloperMode $toggleOffset
    }
    $loadUnpacked = Find-Ui "Load unpacked" 15
  }
  Click-UiElement $loadUnpacked
  Start-Sleep -Seconds 1
  $dialog = Find-Ui "Select the extension directory." 30
  $dialogWindow = [IntPtr]$dialog.Current.NativeWindowHandle
  [SniptaleBrowserFocus]::ShowWindowAsync($dialogWindow, 9) | Out-Null
  if (-not [SniptaleBrowserFocus]::SetForegroundWindow($dialogWindow)) { throw "extension directory dialog could not be focused" }
  Start-Sleep -Milliseconds 300
  $dialogBounds = [SniptaleBrowserFocus+Rect]::new()
  if (-not [SniptaleBrowserFocus]::GetWindowRect($dialogWindow, [ref]$dialogBounds)) {
    throw "extension directory dialog bounds are unavailable"
  }
  $folderX = $dialogBounds.Left + [int](($dialogBounds.Right - $dialogBounds.Left) * 0.62)
  $folderY = $dialogBounds.Bottom - 75
  [SniptaleBrowserFocus]::SetCursorPos($folderX, $folderY) | Out-Null
  [SniptaleBrowserFocus]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
  [SniptaleBrowserFocus]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 300
  [Windows.Forms.SendKeys]::SendWait("^a")
  [Windows.Forms.SendKeys]::SendWait($ExtensionPath)
  Start-Sleep -Milliseconds 500
  $selectFolder = Find-Ui "Select Folder" 10
  Click-UiElement $selectFolder
  Start-Sleep -Seconds 2
}

function Focus-BrowserWindow([string] $Name, [int] $Seconds = 20) {
  if (-not ("SniptaleBrowserFocus" -as [type])) {
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class SniptaleBrowserFocus {
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left; public int Top; public int Right; public int Bottom; }
  public delegate bool EnumWindowsProc(IntPtr window, IntPtr parameter);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr parameter);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string className, string windowName);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, System.Text.StringBuilder className, int capacity);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
  [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr window, int command);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extraInfo);
  [DllImport("user32.dll")] static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam);
  public static IntPtr FindBrowserWindow() {
    IntPtr found = IntPtr.Zero;
    EnumWindows((window, parameter) => {
      var className = new System.Text.StringBuilder(256);
      if (IsWindowVisible(window) && GetClassName(window, className, className.Capacity) > 0 &&
          className.ToString().StartsWith("Chrome_WidgetWin_", StringComparison.Ordinal)) {
        found = window;
        return false;
      }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
'@
  }
  $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
  $window = [IntPtr]::Zero
  while ($window -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline) {
    $window = [SniptaleBrowserFocus]::FindBrowserWindow()
    if ($window -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 200 }
  }
  if ($window -eq [IntPtr]::Zero) { throw "$Name browser window is unavailable" }
  [SniptaleBrowserFocus]::ShowWindowAsync($window, 9) | Out-Null
  if (-not [SniptaleBrowserFocus]::SetForegroundWindow($window)) { throw "$Name browser window could not be focused" }
  Start-Sleep -Milliseconds 500
  return $window
}

function Set-BrowserLocation([IntPtr] $Window, [string] $Url) {
  Add-Type -AssemblyName UIAutomationClient
  Add-Type -AssemblyName System.Windows.Forms
  $root = [Windows.Automation.AutomationElement]::FromHandle($Window)
  $nameCondition = [Windows.Automation.PropertyCondition]::new(
    [Windows.Automation.AutomationElement]::NameProperty,
    "Address and search bar"
  )
  $address = $root.FindFirst([Windows.Automation.TreeScope]::Descendants, $nameCondition)
  $editCondition = [Windows.Automation.PropertyCondition]::new(
    [Windows.Automation.AutomationElement]::ControlTypeProperty,
    [Windows.Automation.ControlType]::Edit
  )
  if (-not $address) { $address = $root.FindFirst([Windows.Automation.TreeScope]::Descendants, $editCondition) }
  if ($address) {
    try {
      $address.SetFocus()
      $value = $address.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
      $value.SetValue($Url)
    } catch {
      $address = $null
    }
  }
  if (-not $address) {
    [SniptaleBrowserFocus]::ShowWindowAsync($Window, 9) | Out-Null
    if (-not [SniptaleBrowserFocus]::SetForegroundWindow($Window)) { throw "browser location fallback could not focus the window" }
    [Windows.Forms.SendKeys]::SendWait("^l")
    [Windows.Forms.SendKeys]::SendWait($Url)
  }
  [Windows.Forms.SendKeys]::SendWait("{ENTER}")
}

function Open-EdgeExtensionsMenu([IntPtr] $Window) {
  $bounds = [SniptaleBrowserFocus+Rect]::new()
  if (-not [SniptaleBrowserFocus]::GetWindowRect($Window, [ref]$bounds)) { throw "edge extensions window bounds are unavailable" }
  [SniptaleBrowserFocus]::SetCursorPos($bounds.Left + 42, $bounds.Top + 148) | Out-Null
  [SniptaleBrowserFocus]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
  [SniptaleBrowserFocus]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Seconds 1
}

function Click-UiElement([object] $Element, [int] $OffsetX = 0) {
  $bounds = $Element.Current.BoundingRectangle
  if ($bounds.Width -lt 4 -or $bounds.Height -lt 4) { throw "browser UI element bounds are invalid" }
  [SniptaleBrowserFocus]::SetCursorPos([int]($bounds.Left + ($bounds.Width / 2) + $OffsetX), [int]($bounds.Top + ($bounds.Height / 2))) | Out-Null
  [SniptaleBrowserFocus]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
  [SniptaleBrowserFocus]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 300
}

function Stop-ProbeBrowsers {
  $deadline = [DateTime]::UtcNow.AddSeconds(30)
  do {
    $processes = @(Get-Process chrome, msedge -ErrorAction SilentlyContinue)
    if ($processes.Count -eq 0) { return }
    if ("SniptaleBrowserFocus" -as [type]) {
      $window = [SniptaleBrowserFocus]::FindBrowserWindow()
      if ($window -ne [IntPtr]::Zero) {
        [SniptaleBrowserFocus]::PostMessage($window, 0x0010, [UIntPtr]::Zero, [IntPtr]::Zero) | Out-Null
        Start-Sleep -Milliseconds 500
      }
    }
    & cmd.exe /d /c "taskkill.exe /F /T /IM chrome.exe >NUL 2>&1" | Out-Null
    & cmd.exe /d /c "taskkill.exe /F /T /IM msedge.exe >NUL 2>&1" | Out-Null
    $processes | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 200
  } while ([DateTime]::UtcNow -lt $deadline)
  if (@(Get-Process chrome, msedge -ErrorAction SilentlyContinue).Count -ne 0) {
    throw "probe browser processes did not stop within their bound"
  }
}

function Add-BrowserConnectionTrust([string] $ProcessName) {
  $storePath = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent\trusted-connections.json"
  $store = if (Test-Path -LiteralPath $storePath) {
    Get-Content -Raw -LiteralPath $storePath | ConvertFrom-Json
  } else {
    [pscustomobject]@{ formatVersion = 1; trusted = @() }
  }
  $trusted = @($store.trusted)
  $origin = "chrome-extension://elhndangpenapadjimfoodlaenpcndpo/"
  $hostHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $installed).Hash.ToLowerInvariant()
  $paths = @($env:ComSpec)
  $paths += @(Get-Process $ProcessName -ErrorAction SilentlyContinue | ForEach-Object {
    try { $_.Path } catch { $null }
  })
  $paths = @($paths | Where-Object { $_ } | Sort-Object -Unique)
  if ($paths.Count -eq 0) { throw "$ProcessName browser process path is unavailable for connection trust" }
  foreach ($path in $paths) {
    $normalized = $path.Replace('/', '\').TrimEnd('\').ToLowerInvariant()
    $label = [IO.Path]::GetFileName($normalized)
    $category = if ($label -in @("cmd.exe", "powershell.exe", "pwsh.exe", "conhost.exe")) {
      "shell-launcher-parent"
    } else {
      "recognized-browser-parent"
    }
    $value = "$category|$label|$normalized"
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
      $fingerprint = ConvertTo-LowerHex ($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($value)))
    } finally {
      $sha.Dispose()
    }
    if (-not @($trusted | Where-Object {
      $_.extensionOrigin -eq $origin -and $_.hostExeSha256 -eq $hostHash -and $_.parentFingerprint -eq $fingerprint
    })) {
      $trusted += [ordered]@{
        extensionOrigin = $origin
        hostExeSha256 = $hostHash
        parentFingerprint = $fingerprint
      }
    }
  }
  $payload = [ordered]@{ formatVersion = 1; trusted = @($trusted) }
  [IO.File]::WriteAllText($storePath, ($payload | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
}

function Approve-ConnectionConsent([object] $Listener, [int] $Seconds = 20) {
  Add-Type -AssemblyName UIAutomationClient
  $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
  $windowCondition = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, "Sniptale Native Agent")
  while ([DateTime]::UtcNow -lt $deadline) {
    if ($Listener.Process.HasExited) { return $false }
    $window = [Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Descendants, $windowCondition)
    if ($window) {
      $yesCondition = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, "Yes")
      $yes = $window.FindFirst([Windows.Automation.TreeScope]::Descendants, $yesCondition)
      if (-not $yes) { throw "native connection consent Yes button missing" }
      $windowHandle = [IntPtr]$window.Current.NativeWindowHandle
      [SniptaleBrowserFocus]::ShowWindowAsync($windowHandle, 9) | Out-Null
      if (-not [SniptaleBrowserFocus]::SetForegroundWindow($windowHandle)) { throw "native connection consent could not be focused" }
      Click-UiElement $yes
      return $true
    }
    Start-Sleep -Milliseconds 250
  }
  return $false
}

function Invoke-BrowserNativeProbe(
  [string] $Name,
  [string] $Executable,
  [string] $ExtensionPath,
  [ValidateSet("pass", "fail")][string] $ExpectedStatus = "pass"
) {
  if (-not (Test-Path -LiteralPath $Executable)) { throw "$Name executable missing" }
  $script:browserProbeAttempt++
  $attemptName = "$Name-attempt-$($script:browserProbeAttempt)"
  Stop-ProbeBrowsers
  $profile = $null
  $listener = $null
  $browser = $null
  try {
    $listenerDetail = "listener process exited before ready"
    $listenerReady = $false
    foreach ($listenerAttempt in 1..3) {
      $listener = Start-ProbeListener $ProbeNonce
      $readyDeadline = [DateTime]::UtcNow.AddSeconds(20)
      while (-not (Test-Path -LiteralPath $listener.ReadyPath) -and -not $listener.Process.HasExited -and [DateTime]::UtcNow -lt $readyDeadline) {
        Start-Sleep -Milliseconds 100
      }
      if (Test-Path -LiteralPath $listener.ReadyPath) {
        $listenerReady = $true
        break
      }
      if (Test-Path -LiteralPath $listener.ErrorPath) { $listenerDetail = Get-Content -Raw -LiteralPath $listener.ErrorPath }
      if (-not $listener.Process.HasExited) {
        Stop-Process -Id $listener.Process.Id -Force -ErrorAction SilentlyContinue
        $null = $listener.Process.WaitForExit(5000)
      }
      Remove-Item -LiteralPath $listener.ReadyPath, $listener.ResultPath, $listener.ErrorPath -Force -ErrorAction SilentlyContinue
      $listener = $null
      Start-Sleep -Milliseconds 500
    }
    if (-not $listenerReady) {
      throw "$Name loopback listener failed to start: $listenerDetail"
    }
    $browserWindow = [IntPtr]::Zero
    foreach ($browserAttempt in 1..3) {
      $profile = Join-Path $env:TEMP "sniptale-e2e-browser-profile-$Name-$([Guid]::NewGuid().ToString('N'))"
      $arguments = @(
        "--user-data-dir=$profile",
        "--force-renderer-accessibility",
        "--disable-background-mode",
        "--no-first-run",
        "--no-default-browser-check",
        "--new-window",
        "about:blank"
      )
      $browser = Start-Process -FilePath $Executable -ArgumentList $arguments -PassThru
      Start-Sleep -Seconds 2
      try {
        $browserWindow = Focus-BrowserWindow $Name 20
        break
      } catch {
        $browserExit = if ($browser.HasExited) { "exit code $($browser.ExitCode)" } else { "no visible window" }
        if (-not $browser.HasExited) { Stop-Process -Id $browser.Id -Force -ErrorAction SilentlyContinue }
        Stop-ProbeBrowsers
        Remove-Item -LiteralPath $profile -Recurse -Force -ErrorAction SilentlyContinue
        $profile = $null
        if ($browserAttempt -eq 3) { throw "$Name browser startup failed after 3 attempts ($browserExit)" }
        Start-Sleep -Seconds 1
      }
    }
    try {
      Add-Type -AssemblyName System.Windows.Forms
      $internalUrl = if ($Name -eq "edge") { "edge://extensions" } else { "chrome://extensions" }
      $developerMode = $null
      foreach ($navigationAttempt in 1..2) {
        Set-BrowserLocation $browserWindow $internalUrl
        Start-Sleep -Seconds 3
        if ($Name -eq "edge") { Open-EdgeExtensionsMenu $browserWindow }
        try {
          $developerMode = Find-Ui "Developer mode" 10
          break
        } catch {
          if ($navigationAttempt -eq 2) { throw }
        }
      }
      Install-UnpackedBrowserExtension $Name $browserWindow $ExtensionPath $developerMode
      $null = Find-Ui "Sniptale Native E2E Probe" 15
      Save-Desktop "browser-extension-$attemptName-loaded"
      $browserProcessName = if ($Name -eq "edge") { "msedge" } else { "chrome" }
      Add-BrowserConnectionTrust $browserProcessName
      $probePage = $null
      foreach ($probeNavigationAttempt in 1..2) {
        Set-BrowserLocation $browserWindow "chrome-extension://elhndangpenapadjimfoodlaenpcndpo/probe.html"
        try {
          $probePage = Find-Ui "Sniptale E2E Probe Running" 15
          break
        } catch {
          if ($probeNavigationAttempt -eq 2) { throw }
        }
      }
      Save-Desktop "browser-extension-$attemptName-running"
    } catch {
      Save-Desktop "browser-extension-$attemptName-error"
      throw
    }
    $null = Approve-ConnectionConsent $listener 3
    $resultDeadline = [DateTime]::UtcNow.AddSeconds(60)
    $title = "sniptale-e2e-result-$ProbeNonce-$Name-$ExpectedStatus"
    $titleCondition = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, $title)
    $titleFound = $false
    while (-not (Test-Path -LiteralPath $listener.ResultPath) -and -not $listener.Process.HasExited -and [DateTime]::UtcNow -lt $resultDeadline) {
      $titleFound = $null -ne [Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Descendants, $titleCondition)
      if ($titleFound) { break }
      Start-Sleep -Milliseconds 100
    }
    if (Test-Path -LiteralPath $listener.ResultPath) {
      $payload = Get-Content -Raw -LiteralPath $listener.ResultPath
      $result = $payload | ConvertFrom-Json
    } else {
      if ($titleFound) {
        $result = [pscustomobject]@{ browser = $Name; status = $ExpectedStatus; detail = "nonce-bound extension page result" }
      } else {
        $detail = if (Test-Path -LiteralPath $listener.ErrorPath) { Get-Content -Raw -LiteralPath $listener.ErrorPath } else { "no browser result" }
        throw "$Name native probe timed out: $detail"
      }
    }
    if ($result.browser -ne $Name -or $result.status -ne $ExpectedStatus) {
      throw "$Name native probe expected $ExpectedStatus, got $($result.status): $($result.detail)"
    }
    if ($ExpectedStatus -eq "pass") { "$Name connectNative received app.hello" }
    else { "$Name connectNative was rejected while Manual authority was active" }
  } finally {
    if ($browser -and -not $browser.HasExited) { Stop-Process -Id $browser.Id -Force -ErrorAction SilentlyContinue }
    try { Stop-ProbeBrowsers } catch { }
    if ($listener -and -not $listener.Process.HasExited) {
      Stop-Process -Id $listener.Process.Id -Force -ErrorAction SilentlyContinue
      $null = $listener.Process.WaitForExit(5000)
    }
    if ($listener) { Remove-Item -LiteralPath $listener.ReadyPath, $listener.ResultPath, $listener.ErrorPath -Force -ErrorAction SilentlyContinue }
    Remove-Item -LiteralPath $profile -Recurse -Force -ErrorAction SilentlyContinue
  }
}

function Invoke-BrowserNativeProbeWithRetry(
  [string] $Name,
  [string] $Executable,
  [string] $ExtensionPath,
  [ValidateSet("pass", "fail")][string] $ExpectedStatus = "pass",
  [string] $ProgressScenario = "native-messaging"
) {
  $lastError = $null
  foreach ($protocolAttempt in 1..4) {
    Write-ProgressEvent "$ProgressScenario-$Name" "retry" $protocolAttempt 4 120
    try {
      return Invoke-BrowserNativeProbe $Name $Executable $ExtensionPath $ExpectedStatus
    } catch {
      $lastError = $_
      if ($protocolAttempt -lt 4) {
        if ($ExpectedStatus -eq "pass") { Restart-AgentWithMode "extension-managed" }
        Start-Sleep -Seconds 2
      }
    }
  }
  throw $lastError
}

Add-Test "network-isolation" {
  $client = [Net.Sockets.TcpClient]::new()
  try {
    $attempt = $client.ConnectAsync("1.1.1.1", 443)
    try { $null = $attempt.Wait(3000) } catch [AggregateException] { }
    if ($client.Connected) { throw "runtime guest reached an external TCP endpoint" }
  } finally {
    $client.Dispose()
  }
  "QEMU runtime policy denied guest egress"
}

Add-Test "install" {
  Invoke-Msi @("/i", $PreviousMsi) "install-baseline"
  if (-not (Test-Path -LiteralPath $installed)) { throw "installed executable missing" }
  if ((Get-FileHash -Algorithm SHA256 -LiteralPath $installed).Hash.ToLowerInvariant() -ne $PreviousExeSha256.ToLowerInvariant()) {
    throw "baseline install did not contain the distinct 0.0.1 executable"
  }
  Write-UpgradeAuthorizationFixture
  "distinct baseline MSI and executable installed with authorization-state preservation fixture"
}
Add-Test "upgrade" {
  Invoke-Msi @("/i", $Msi) "upgrade"
  if ((Get-FileHash -Algorithm SHA256 -LiteralPath $installed).Hash.ToLowerInvariant() -ne $ExpectedExeSha256.ToLowerInvariant()) {
    throw "major upgrade did not replace the baseline executable"
  }
  $installedManifest = Join-Path ([IO.Path]::GetDirectoryName($installed)) "native-host.json"
  if ((Get-FileHash -Algorithm SHA256 -LiteralPath $installedManifest).Hash.ToLowerInvariant() -ne $ExpectedNativeHostManifestSha256.ToLowerInvariant()) {
    throw "installed Native Messaging manifest differs from the guest-built manifest"
  }
  $installDirectory = [IO.Path]::GetDirectoryName($installed)
  foreach ($name in @("LICENSE.txt", "NOTICE.txt", "THIRD_PARTY_NOTICES.md", "THIRD_PARTY_DEPENDENCIES.json")) {
    if (-not (Test-Path -LiteralPath (Join-Path $installDirectory $name))) { throw "major upgrade missing legal payload: $name" }
  }
  foreach ($name in @("pipe-secret.json", "dev-local-browser-trust.json", "trusted-connections.json")) {
    $path = Join-Path $installDirectory $name
    if (-not (Test-Path -LiteralPath $path)) { throw "major upgrade removed authorization state: $name" }
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash -ne $script:upgradeAuthorizationHashes[$name]) {
      throw "major upgrade changed authorization state: $name"
    }
  }
  Provision-DevRuntimeTrust
  $registration = @(
    "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*",
    "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*",
    "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*"
  ) | ForEach-Object { Get-ItemProperty -Path $_ -ErrorAction SilentlyContinue } |
    Where-Object { $_.DisplayName -eq "Sniptale Desktop" -and $_.DisplayVersion -eq "0.1.0" } |
    Select-Object -First 1
  if (-not $registration) { throw "MSI major upgrade did not register current version" }
  "MSI major upgrade completed from 0.0.1 to 0.1.0 while preserving authorization state"
}
Add-Test "repair" {
  Remove-Item -LiteralPath $installed -Force
  if (Test-Path -LiteralPath $installed) { throw "repair fixture could not remove the owned executable" }
  Invoke-Msi @("/fa", $Msi) "repair"
  if (-not (Test-Path -LiteralPath $installed) -or (Get-FileHash -Algorithm SHA256 -LiteralPath $installed).Hash.ToLowerInvariant() -ne $ExpectedExeSha256.ToLowerInvariant()) {
    throw "MSI repair did not restore the exact current executable"
  }
  $installedManifest = Join-Path ([IO.Path]::GetDirectoryName($installed)) "native-host.json"
  if ((Get-FileHash -Algorithm SHA256 -LiteralPath $installedManifest).Hash.ToLowerInvariant() -ne $ExpectedNativeHostManifestSha256.ToLowerInvariant()) {
    throw "MSI repair did not preserve the exact Native Messaging manifest"
  }
  "MSI repair restored the deliberately removed executable and manifest binding"
}

Add-Test "resident" {
  $media = Join-Path $outputPath "product-media"
  New-Item -ItemType Directory -Force -Path $media | Out-Null
  $profileDir = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent"
  New-Item -ItemType Directory -Force -Path $profileDir | Out-Null
  $script:manualProfileTemplate = [ordered]@{
    destination = [ordered]@{ id = "synthetic-destination"; path = $media }
    screenshot = [ordered]@{ includeCursor = $true }
    video = [ordered]@{ quality = "high"; fps = "fps30"; bitrateOverrideMbps = $null; hardwarePreference = "prefer"; includeCursor = $true; maxDurationSeconds = 120 }
    audio = [ordered]@{ sourceMode = "system"; microphoneDeviceId = $null; bitrateKbps = 128 }
    hotkeys = [ordered]@{ "screenshot-screen" = "Ctrl+Alt+Shift+F5"; "screenshot-region" = "Ctrl+Alt+Shift+F6"; "recording-start-screen" = "Ctrl+Alt+Shift+F7"; "recording-stop" = "Ctrl+Alt+Shift+F8" }
  }
  $profile = [ordered]@{
    schemaVersion = 1; revision = 1; operatingMode = "extension-managed"
    manualProfile = $null
  }
  [IO.File]::WriteAllText((Join-Path $profileDir "local-profile.json"), ($profile | ConvertTo-Json -Depth 8 -Compress), [Text.UTF8Encoding]::new($false))
  # MSI autostart may already own the singleton after install/upgrade/repair. Starting a second
  # process and checking that process' exit status mistakes a healthy singleton handoff for a
  # product crash. The shared restart helper stops every installed owner, applies the requested
  # profile, and proves that the newly-owned resident stays alive.
  Restart-AgentWithMode "extension-managed"
  Restart-AgentWithMode "manual"
  $version = & $installed --version-json | ConvertFrom-Json
  $doctor = & $installed --doctor-json | ConvertFrom-Json
  if (-not $version.platformKind -or -not $doctor.checks) { throw "support JSON incomplete" }
  "resident and support modes operational"
}

function Initialize-SniptaleTrayProbe {
  if ("SniptaleTrayProbe" -as [type]) { return }
  Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class SniptaleTrayProbe {
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left; public int Top; public int Right; public int Bottom; }
  public delegate bool EnumWindowsProc(IntPtr window, IntPtr parameter);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string className, string windowName);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowsProc callback, IntPtr parameter);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, System.Text.StringBuilder className, int capacity);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extraInfo);
  [DllImport("user32.dll")] static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll", SetLastError=true)] public static extern bool PostMessage(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam);
  public static IntPtr FindRightmostPopup() {
    IntPtr found = IntPtr.Zero;
    int right = int.MinValue;
    EnumWindows((window, parameter) => {
      var name = new System.Text.StringBuilder(64);
      Rect rect;
      if (GetClassName(window, name, name.Capacity) > 0 && name.ToString() == "#32768" && GetWindowRect(window, out rect) && rect.Right > right) {
        found = window;
        right = rect.Right;
      }
      return true;
    }, IntPtr.Zero);
    return found;
  }
  public static void PressKey(byte key) {
    keybd_event(key, 0, 0, UIntPtr.Zero);
    keybd_event(key, 0, 2, UIntPtr.Zero);
  }
}
'@
}

function Initialize-SniptaleHotkeyInput {
  if ("SniptaleHotkeyInput" -as [type]) { return }
  Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Threading;
public static class SniptaleHotkeyInput {
  [DllImport("user32.dll")] static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
  [DllImport("winmm.dll")] static extern uint waveOutSetVolume(IntPtr output, uint volume);
  static void Key(byte key, bool up) { keybd_event(key, 0, up ? 2u : 0u, UIntPtr.Zero); }
  public static void Press(byte functionKey) {
    Key(0x11, false); Key(0x12, false); Key(0x10, false); Key(functionKey, false);
    Thread.Sleep(150);
    Key(functionKey, true); Key(0x10, true); Key(0x12, true); Key(0x11, true);
    Thread.Sleep(150);
  }
  public static void EnsurePlaybackAudible() {
    waveOutSetVolume(IntPtr.Zero, 0xFFFFFFFFu);
    for (int i = 0; i < 20; i++) {
      Key(0xAF, false); Key(0xAF, true); Thread.Sleep(25);
    }
  }
}
'@
}

Add-Test "tray" {
  Initialize-SniptaleTrayProbe
  $trayWindow = [SniptaleTrayProbe]::FindWindow("SniptaleTrayCommandSource", $null)
  if ($trayWindow -eq [IntPtr]::Zero) { throw "tray callback window is unavailable" }
  [SniptaleTrayProbe]::SetCursorPos(640, 400) | Out-Null
  $contextCallback = [IntPtr]((1 -shl 16) -bor 0x007b)
  if (-not [SniptaleTrayProbe]::PostMessage($trayWindow, 0x8001, [UIntPtr]::Zero, $contextCallback)) {
    throw "tray context callback could not be posted"
  }
  Start-Sleep -Seconds 1
  Save-Desktop "tray-context-menu"
  # UI Automation does not expose native #32768 menu items consistently on
  # current Windows 11 builds. Click the fixed Manual-menu Settings row inside
  # the actual popup HWND after validating its geometry.
  $menuWindow = [SniptaleTrayProbe]::FindWindow("#32768", $null)
  if ($menuWindow -eq [IntPtr]::Zero) { throw "native tray popup window is unavailable" }
  $menuBounds = [SniptaleTrayProbe+Rect]::new()
  if (-not [SniptaleTrayProbe]::GetWindowRect($menuWindow, [ref]$menuBounds)) { throw "native tray popup bounds are unavailable" }
  $menuWidth = $menuBounds.Right - $menuBounds.Left
  $menuHeight = $menuBounds.Bottom - $menuBounds.Top
  if ($menuWidth -lt 100 -or $menuHeight -lt 150) { throw "native tray popup bounds are invalid" }
  [SniptaleTrayProbe]::SetCursorPos($menuBounds.Left + [int]($menuWidth / 2), $menuBounds.Top + [int]($menuHeight * 0.69)) | Out-Null
  [SniptaleTrayProbe]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
  [SniptaleTrayProbe]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
  $null = Find-Ui $settingsTitle 10
  "tray icon and context menu invoked"
}
Add-Test "settings" {
  $window = Find-Ui $settingsTitle 20
  $transform = $window.GetCurrentPattern([Windows.Automation.TransformPattern]::Pattern)
  if (-not $transform.Current.CanResize) { throw "settings window is not resizable" }
  $beforeBounds = $window.Current.BoundingRectangle
  $transform.Resize(1000, 720)
  Start-Sleep -Seconds 2
  $window = Find-Ui $settingsTitle 5
  $afterBounds = $window.Current.BoundingRectangle
  if ([Math]::Abs($afterBounds.Width - $beforeBounds.Width) -lt 50 -or [Math]::Abs($afterBounds.Height - $beforeBounds.Height) -lt 50) {
    throw "settings window bounds did not change after resize"
  }
  Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class SniptaleDpi {
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint message, UIntPtr wparam, IntPtr lparam);
}
'@
  $dpi = [SniptaleDpi]::GetDpiForWindow([IntPtr]$window.Current.NativeWindowHandle)
  if ($dpi -lt 120) { throw "settings window did not run at the provisioned 125 percent DPI" }
  Save-UiWindow $window "settings-resized"
  $lightHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $outputPath "screenshots\settings-resized.png")).Hash
  $themePath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"
  New-Item -Path $themePath -Force | Out-Null
  New-ItemProperty -Path $themePath -Name AppsUseLightTheme -PropertyType DWord -Value 0 -Force | Out-Null
  & rundll32.exe user32.dll,UpdatePerUserSystemParameters
  [SniptaleDpi]::PostMessage([IntPtr]$window.Current.NativeWindowHandle, 0x031a, [UIntPtr]::Zero, [IntPtr]::Zero) | Out-Null
  Start-Sleep -Seconds 3
  $window = Find-Ui $settingsTitle 5
  Save-UiWindow $window "settings-dark-theme"
  $darkHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $outputPath "screenshots\settings-dark-theme.png")).Hash
  if ($darkHash -eq $lightHash) { throw "settings pixels did not change after the dark-theme transition" }
  New-ItemProperty -Path $themePath -Name AppsUseLightTheme -PropertyType DWord -Value 1 -Force | Out-Null
  & rundll32.exe user32.dll,UpdatePerUserSystemParameters
  [SniptaleDpi]::PostMessage([IntPtr]$window.Current.NativeWindowHandle, 0x031a, [UIntPtr]::Zero, [IntPtr]::Zero) | Out-Null
  $window.GetCurrentPattern([Windows.Automation.WindowPattern]::Pattern).Close()
  Start-Sleep -Seconds 1
  "settings HWND bounds changed at dpi=$dpi and light/dark pixels differ"
}
Add-Test "native-messaging" {
  $hostName = "com.sniptale.dev.native_host"
  foreach ($browser in @("HKCU:\Software\Google\Chrome\NativeMessagingHosts", "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts")) {
    $key = Join-Path $browser $hostName
    if (-not (Test-Path $key)) { throw "native host registration missing" }
    $manifest = (Get-ItemProperty -LiteralPath $key).'(default)'
    if (-not (Test-Path -LiteralPath $manifest)) { throw "native host manifest missing" }
    $manifestValue = Get-Content -Raw -LiteralPath $manifest | ConvertFrom-Json
    if ($manifestValue.name -ne $hostName -or $manifestValue.type -ne "stdio") { throw "native host manifest identity is invalid" }
    if (@($manifestValue.allowed_origins).Count -ne 1 -or $manifestValue.allowed_origins[0] -ne "chrome-extension://elhndangpenapadjimfoodlaenpcndpo/") {
      throw "native host manifest origin is invalid"
    }
    if ($manifestValue.path -notin @("sniptale-native-agent.exe", $installed)) { throw "native host manifest executable path is invalid" }
  }
  $pipeSecretPath = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent\pipe-secret.json"
  if (-not (Test-Path -LiteralPath $pipeSecretPath)) { throw "authenticated pipe secret is missing" }
  $pipeSecret = Get-Content -Raw -LiteralPath $pipeSecretPath | ConvertFrom-Json
  $currentSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
  if ($pipeSecret.authAlgorithm -ne "pipe-auth-v2-hmac-sha256" -or $pipeSecret.protection -ne "current-user-dpapi") {
    throw "pipe secret authentication or protection contract is invalid"
  }
  if ($pipeSecret.currentUserSid -ne $currentSid -or $pipeSecret.installId -notmatch '^[A-Za-z0-9_-]{8,64}$') {
    throw "pipe secret is not bound to the current user/install"
  }
  $hex = [string]$pipeSecret.protectedSecretHex
  if ($hex.Length % 2 -ne 0 -or $hex -notmatch '^[0-9A-Fa-f]+$') { throw "DPAPI pipe secret encoding is invalid" }
  $protected = [byte[]]::new($hex.Length / 2)
  for ($i = 0; $i -lt $protected.Length; $i++) { $protected[$i] = [Convert]::ToByte($hex.Substring($i * 2, 2), 16) }
  Add-Type -AssemblyName System.Security
  $plain = [Security.Cryptography.ProtectedData]::Unprotect($protected, $null, [Security.Cryptography.DataProtectionScope]::CurrentUser)
  try { if ($plain.Length -lt 32) { throw "DPAPI pipe secret is too short" } } finally { [Array]::Clear($plain, 0, $plain.Length) }
  if (-not (Test-Path -LiteralPath $BrowserProbe)) { throw "browser probe archive missing" }
  $script:extensionPath = Join-Path $outputPath "browser-extension"
  Expand-Archive -LiteralPath $BrowserProbe -DestinationPath $extensionPath -Force
  $script:chrome = @(
    (Join-Path $env:ProgramFiles "Google\Chrome\Application\chrome.exe"),
    (Join-Path ${env:ProgramFiles(x86)} "Google\Chrome\Application\chrome.exe")
  ) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
  $script:edge = @(
    (Join-Path ${env:ProgramFiles(x86)} "Microsoft\Edge\Application\msedge.exe"),
    (Join-Path $env:ProgramFiles "Microsoft\Edge\Application\msedge.exe")
  ) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
  $manualResult = Invoke-BrowserNativeProbeWithRetry "chrome" $chrome $extensionPath "fail"
  try {
    Restart-AgentWithMode "extension-managed"
    $chromeResult = Invoke-BrowserNativeProbeWithRetry "chrome" $chrome $extensionPath "pass" "native-messaging"
    $edgeResult = Invoke-BrowserNativeProbeWithRetry "edge" $edge $extensionPath "pass" "native-messaging"
    "$manualResult; $chromeResult; $edgeResult"
  } finally {
    Restart-AgentWithMode "manual"
  }
}
Add-Test "capture" {
  Add-Type -AssemblyName System.Drawing
  Add-Type -AssemblyName System.Windows.Forms
  Restart-AgentWithMode "manual"
  Initialize-SniptaleTrayProbe
  Initialize-SniptaleHotkeyInput
  Stop-ProbeBrowsers
  $media = Join-Path $outputPath "product-media"
  $before = @(Get-ChildItem -LiteralPath $media -Filter *.png -ErrorAction SilentlyContinue).Count
  $screen = @()
  foreach ($attempt in 1..5) {
    [Windows.Forms.SendKeys]::SendWait("{ESC}")
    [SniptaleHotkeyInput]::Press(0x74)
    Start-Sleep -Seconds 10
    $screen = @(Get-ChildItem -LiteralPath $media -Filter *.png -ErrorAction SilentlyContinue | Sort-Object LastWriteTime)
    if ($screen.Count -gt $before) { break }
  }
  if ($screen.Count -le $before) { throw "Sniptale screen capture produced no PNG" }
  $screenImage = [Drawing.Image]::FromFile($screen[-1].FullName)
  try { if ($screenImage.Width -lt 100 -or $screenImage.Height -lt 100) { throw "Sniptale screen PNG dimensions are invalid" } } finally { $screenImage.Dispose() }
  Copy-Item -LiteralPath $screen[-1].FullName -Destination (Join-Path $outputPath "screenshots\sniptale-screen.png")
  Add-Type @'
using System.Runtime.InteropServices;
using System.Threading;
public static class SniptaleDrag {
  [StructLayout(LayoutKind.Sequential)] struct Rect { public int Left; public int Top; public int Right; public int Bottom; }
  [DllImport("user32.dll")] static extern bool GetWindowRect(System.IntPtr window, out Rect rect);
  [DllImport("user32.dll")] static extern bool SetForegroundWindow(System.IntPtr window);
  [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] static extern void mouse_event(uint flags, uint x, uint y, uint data, System.UIntPtr extra);
  public static void Region(System.IntPtr window) {
    Rect rect;
    if (!GetWindowRect(window, out rect)) throw new System.InvalidOperationException("overlay bounds unavailable");
    int startX = rect.Left + 100;
    int startY = rect.Top + 100;
    int endX = System.Math.Max(startX + 100, rect.Right - 100);
    int endY = System.Math.Max(startY + 100, rect.Bottom - 100);
    SetForegroundWindow(window);
    SetCursorPos(startX, startY);
    Thread.Sleep(750);
    mouse_event(2, 0, 0, 0, System.UIntPtr.Zero);
    Thread.Sleep(500);
    SetCursorPos(endX, endY);
    Thread.Sleep(500);
    mouse_event(4, 0, 0, 0, System.UIntPtr.Zero);
  }
}
'@
  $after = $screen
  $regionAttemptOutcomes = [Collections.Generic.List[string]]::new()
  $regionStarted = [DateTime]::UtcNow
  foreach ($attempt in 1..5) {
    Write-ProgressEvent "capture" "retry" $attempt 5 25
    [Windows.Forms.SendKeys]::SendWait("{ESC}")
    [SniptaleHotkeyInput]::Press(0x75)
    $overlayDeadline = [DateTime]::UtcNow.AddSeconds(10)
    $overlayWindow = [IntPtr]::Zero
    while ([DateTime]::UtcNow -lt $overlayDeadline) {
      $overlayWindow = [SniptaleTrayProbe]::FindWindow("SniptaleRegionOverlay", $null)
      if ($overlayWindow -ne [IntPtr]::Zero) { break }
      Start-Sleep -Milliseconds 250
    }
    if ($overlayWindow -eq [IntPtr]::Zero) {
      $null = $regionAttemptOutcomes.Add("attempt-${attempt}:overlay-not-observed")
      continue
    }
    Start-Sleep -Seconds 1
    [SniptaleDrag]::Region($overlayWindow)
    Start-Sleep -Seconds 10
    $after = @(Get-ChildItem -LiteralPath $media -Filter *.png -ErrorAction SilentlyContinue | Sort-Object LastWriteTime)
    if ($after.Count -gt $screen.Count) {
      $null = $regionAttemptOutcomes.Add("attempt-${attempt}:pass")
      break
    }
    $agentAlive = @(Get-Process -Name sniptale-native-agent -ErrorAction SilentlyContinue).Count -gt 0
    $reason = if ($agentAlive) { "no-finalized-png" } else { "agent-exited" }
    $null = $regionAttemptOutcomes.Add("attempt-${attempt}:$reason")
    if (-not $agentAlive) {
      $applicationError = $null
      $eventDeadline = [DateTime]::UtcNow.AddSeconds(10)
      while (-not $applicationError -and [DateTime]::UtcNow -lt $eventDeadline) {
        $applicationError = Get-WinEvent -FilterHashtable @{
          LogName = "Application"
          Id = 1000
          StartTime = $regionStarted
        } -ErrorAction SilentlyContinue |
          Where-Object { $_.Message -match "sniptale-native-agent" } |
          Select-Object -First 1
        if (-not $applicationError) { Start-Sleep -Milliseconds 500 }
      }
      if ($applicationError) {
        [IO.File]::WriteAllText(
          (Join-Path $outputPath "logs\capture-application-error.log"),
          $applicationError.Message,
          [Text.UTF8Encoding]::new($false)
        )
      }
      break
    }
  }
  if ($after.Count -le $screen.Count) {
    throw "Sniptale region capture produced no PNG after bounded retry ($($regionAttemptOutcomes -join ', '))"
  }
  $regionImage = [Drawing.Image]::FromFile($after[-1].FullName)
  try { if ($regionImage.Width -lt 100 -or $regionImage.Height -lt 100) { throw "Sniptale region PNG dimensions are invalid" } } finally { $regionImage.Dispose() }
  Copy-Item -LiteralPath $after[-1].FullName -Destination (Join-Path $outputPath "screenshots\sniptale-region.png")
  "Sniptale screen and region PNGs produced"
}
function Set-RecordingAudioMode([ValidateSet("system", "microphone")][string] $SourceMode) {
  $profilePath = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent\local-profile.json"
  $profile = Get-Content -Raw -LiteralPath $profilePath | ConvertFrom-Json
  $profile.revision = [uint64]$profile.revision + 1
  $profile.manualProfile.audio.sourceMode = $SourceMode
  $profile.manualProfile.audio.microphoneDeviceId = $null
  # This scenario proves audio capture and MP4 finalization, not the high-bitrate preset. Keep the
  # encoder below the disposable VM's software-H264 saturation point so wall time and media time
  # remain comparable and the ten-second PCM acceptance threshold is meaningful.
  $profile.manualProfile.video.quality = "low"
  $profile.manualProfile.video.fps = "fps24"
  $profile.manualProfile.video.bitrateOverrideMbps = $null
  [IO.File]::WriteAllText($profilePath, ($profile | ConvertTo-Json -Depth 8 -Compress), [Text.UTF8Encoding]::new($false))
  Restart-AgentWithMode "manual"
}

function Invoke-VideoTrayAction([ValidateSet("start", "stop")][string] $Action) {
  $trayWindow = [IntPtr]::Zero
  $trayDeadline = [DateTime]::UtcNow.AddSeconds(15)
  while ([DateTime]::UtcNow -lt $trayDeadline) {
    $trayWindow = [SniptaleTrayProbe]::FindWindow("SniptaleTrayCommandSource", $null)
    if ($trayWindow -ne [IntPtr]::Zero) { break }
    Start-Sleep -Milliseconds 500
  }
  if ($trayWindow -eq [IntPtr]::Zero) { throw "tray callback window is unavailable for recording control" }
  [SniptaleTrayProbe]::SetCursorPos(640, 400) | Out-Null
  $contextCallback = [IntPtr]((1 -shl 16) -bor 0x007b)
  $main = [IntPtr]::Zero
  [SniptaleTrayProbe]::PressKey(0x1B)
  [SniptaleTrayProbe]::PressKey(0x1B)
  foreach ($menuAttempt in 1..5) {
    if (-not [SniptaleTrayProbe]::PostMessage($trayWindow, 0x8001, [UIntPtr]::Zero, $contextCallback)) {
      throw "recording tray context callback could not be posted"
    }
    Start-Sleep -Seconds 1
    $main = [SniptaleTrayProbe]::FindWindow("#32768", $null)
    if ($main -ne [IntPtr]::Zero) { break }
    [SniptaleTrayProbe]::PressKey(0x1B)
  }
  $mainBounds = [SniptaleTrayProbe+Rect]::new()
  if ($main -eq [IntPtr]::Zero -or -not [SniptaleTrayProbe]::GetWindowRect($main, [ref]$mainBounds)) { throw "recording main tray menu is unavailable" }
  $mainWidth = $mainBounds.Right - $mainBounds.Left
  $mainHeight = $mainBounds.Bottom - $mainBounds.Top
  [SniptaleTrayProbe]::SetCursorPos($mainBounds.Left + [int]($mainWidth / 2), $mainBounds.Top + [int]($mainHeight * 0.43)) | Out-Null
  [SniptaleTrayProbe]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
  [SniptaleTrayProbe]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Seconds 1
  Save-Desktop "recording-$Action-main-menu"
  $submenu = [SniptaleTrayProbe]::FindRightmostPopup()
  $submenuBounds = [SniptaleTrayProbe+Rect]::new()
  if ($submenu -eq [IntPtr]::Zero -or $submenu -eq $main -or -not [SniptaleTrayProbe]::GetWindowRect($submenu, [ref]$submenuBounds)) { throw "recording Video submenu is unavailable" }
  Save-Desktop "recording-$Action-video-submenu"
  $submenuWidth = $submenuBounds.Right - $submenuBounds.Left
  $submenuHeight = $submenuBounds.Bottom - $submenuBounds.Top
  $row = if ($Action -eq "start") { 0 } else { 5 }
  $targetY = $submenuBounds.Top + [int](($row + 0.5) * ($submenuHeight / 7.0))
  [SniptaleTrayProbe]::SetCursorPos($submenuBounds.Left + [int]($submenuWidth / 2), $targetY) | Out-Null
  [SniptaleTrayProbe]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
  [SniptaleTrayProbe]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
}

function New-SyntheticToneWave([string] $Path, [string] $Ffmpeg) {
  # A long one-shot source avoids SoundPlayer.PlayLooping races that previously yielded only a
  # few hundred milliseconds of loopback audio while the video continued for twenty seconds.
  $null = & $Ffmpeg -nostdin -v error -f lavfi -i "sine=frequency=880:sample_rate=48000:duration=90" -ac 2 -c:a pcm_s16le -y $Path 2>&1
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $Path) -or (Get-Item -LiteralPath $Path).Length -lt 1048576) {
    throw "synthetic system-audio fixture generation failed"
  }
}

function Test-NonSilentPcm([string] $Path, [int64] $MinimumBytes) {
  if (-not (Test-Path -LiteralPath $Path)) { return $false }
  $item = Get-Item -LiteralPath $Path
  if ($item.Length -lt $MinimumBytes) { return $false }
  $stream = [IO.File]::OpenRead($Path)
  try {
    $buffer = [byte[]]::new(65536)
    $remaining = [Math]::Min($item.Length, 1048576)
    $nonZero = 0
    while ($remaining -gt 0) {
      $read = $stream.Read($buffer, 0, [Math]::Min($buffer.Length, [int]$remaining))
      if ($read -le 0) { break }
      for ($index = 0; $index -lt $read; $index++) {
        if ($buffer[$index] -ne 0) {
          $nonZero++
          if ($nonZero -ge 64) { return $true }
        }
      }
      $remaining -= $read
    }
    return $false
  } finally {
    $stream.Dispose()
  }
}

function Get-AgentHotkeyEventCount {
  $logDirectory = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent\logs"
  if (-not (Test-Path -LiteralPath $logDirectory)) { return 0 }
  $count = 0
  Get-ChildItem -LiteralPath $logDirectory -Filter *.jsonl -File -ErrorAction SilentlyContinue |
    ForEach-Object {
      $count += @(Select-String -LiteralPath $_.FullName -SimpleMatch `
        '"target":"sniptale_native_agent::hotkeys"' -ErrorAction SilentlyContinue |
        Where-Object { $_.Line.Contains('"phase":"event-received"') }).Count
    }
  return $count
}

function Get-AgentRecordingFailure {
  if (-not $script:agent) { return $null }
  $logDirectory = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent\logs"
  $pattern = "sniptale-native-*-$($script:agent.Id)-*.jsonl"
  $records = [Collections.Generic.List[object]]::new()
  Get-ChildItem -LiteralPath $logDirectory -Filter $pattern -File -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime |
    ForEach-Object {
      Get-Content -LiteralPath $_.FullName -ErrorAction SilentlyContinue | ForEach-Object {
        try {
          $record = $_ | ConvertFrom-Json -ErrorAction Stop
          if ($record.target -eq "sniptale_native_agent::recording" -and $record.error.code) {
            $records.Add($record)
          }
        } catch { }
      }
    }
  if ($records.Count -eq 0) { return $null }
  $latest = $records[$records.Count - 1]
  $native = if ($latest.message -match 'nativeCode=(0x[0-9a-fA-F]{8})') { "-$($Matches[1].ToLowerInvariant())" } else { "" }
  return "$($latest.error.code)$native"
}

function Write-AgentRecordingDiagnostics([string] $SourceMode, [int] $Attempt) {
  if (-not $script:agent) { return }
  $logDirectory = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent\logs"
  $pattern = "sniptale-native-*-$($script:agent.Id)-*.jsonl"
  $target = Join-Path $outputPath "logs\agent-recording-$SourceMode-attempt-$Attempt.log"
  $safeLines = [Collections.Generic.List[string]]::new()
  Get-ChildItem -LiteralPath $logDirectory -Filter $pattern -File -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime | ForEach-Object {
      Get-Content -LiteralPath $_.FullName -ErrorAction SilentlyContinue | ForEach-Object {
        try {
          $record = $_ | ConvertFrom-Json -ErrorAction Stop
          if ($record.target -eq "sniptale_native_agent::recording") {
            $native = if ($record.message -match 'nativeCode=(0x[0-9a-fA-F]{8})') { "; nativeCode=$($Matches[1].ToLowerInvariant())" } else { "" }
            $code = if ($record.error.code) { "; error.code=$($record.error.code)" } else { "" }
            $safeLines.Add("$($record.timestamp) target=$($record.target); phase=$($record.phase)$code$native")
          }
        } catch { }
      }
    }
  if ($safeLines.Count -gt 0) {
    [IO.File]::WriteAllLines($target, $safeLines, [Text.UTF8Encoding]::new($false))
  }
}

function Reset-RecordingFixtureState([string] $Media, [string] $TempMedia) {
  # System and microphone are independent acceptance cases. Do not carry the previous case's
  # pending transfer metadata into the next recording runtime: it can keep finalization occupied
  # even though the prior MP4 has already been copied into receipt-bound evidence.
  if ($script:agent -and -not $script:agent.HasExited) {
    Stop-Process -Id $script:agent.Id -Force -ErrorAction SilentlyContinue
    $null = $script:agent.WaitForExit(5000)
  }
  Remove-Item -LiteralPath $TempMedia -Recurse -Force -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force -Path $TempMedia | Out-Null
  Get-ChildItem -LiteralPath $Media -Filter *.mp4 -File -ErrorAction SilentlyContinue |
    Remove-Item -Force -ErrorAction Stop
}

function Invoke-SyntheticRecording([ValidateSet("system", "microphone")][string] $SourceMode) {
  Add-Type -AssemblyName System.Windows.Forms
  Initialize-SniptaleHotkeyInput
  if ($SourceMode -eq "microphone") {
    $consentPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone"
    New-Item -Path $consentPath -Force | Out-Null
    New-ItemProperty -Path $consentPath -Name Value -PropertyType String -Value "Allow" -Force | Out-Null
  }
  $media = Join-Path $outputPath "product-media"
  $tempMedia = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent\temp-media"
  if ($SourceMode -eq "microphone") {
    Reset-RecordingFixtureState $media $tempMedia
  }
  Set-RecordingAudioMode $SourceMode
  $fixturePath = Join-Path $env:TEMP "sniptale-e2e-motion.ps1"
  $fixtureSource = @'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
$form = [Windows.Forms.Form]::new()
$form.Text = "Sniptale Synthetic Motion Fixture"
$form.Size = [Drawing.Size]::new(900, 600)
$form.StartPosition = "CenterScreen"
$form.TopMost = $true
$label = [Windows.Forms.Label]::new()
$label.Dock = "Fill"
$label.TextAlign = "MiddleCenter"
$label.Font = [Drawing.Font]::new("Segoe UI", 32)
$form.Controls.Add($label)
$colors = @([Drawing.Color]::Blue, [Drawing.Color]::Lime, [Drawing.Color]::Red, [Drawing.Color]::Yellow)
$step = 0
$deadline = [DateTime]::UtcNow.AddSeconds(180)
$timer = [Windows.Forms.Timer]::new()
$timer.Interval = 250
$timer.Add_Tick({
  $script:step++
  $form.BackColor = $colors[$script:step % $colors.Count]
  $label.Text = "SYNTHETIC FRAME $script:step"
  if ([DateTime]::UtcNow -ge $deadline) { $form.Close() }
})
$timer.Start()
[Windows.Forms.Application]::Run($form)
'@
  [IO.File]::WriteAllText($fixturePath, $fixtureSource, [Text.UTF8Encoding]::new($false))
  $fixture = Start-Process powershell.exe -ArgumentList @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$fixturePath`"") -PassThru
  $acceptedRecording = $null
  $attemptOutcomes = [Collections.Generic.List[string]]::new()
  $ffmpeg = (Get-Command ffmpeg.exe -ErrorAction Stop).Source
  $tonePath = Join-Path $env:TEMP "sniptale-e2e-system-audio.wav"
  $toneRunnerPath = Join-Path $env:TEMP "sniptale-e2e-system-audio.ps1"
  $toneRunnerSource = @'
param([Parameter(Mandatory = $true)][string] $Wave)
$ErrorActionPreference = "Stop"
$player = [Media.SoundPlayer]::new($Wave)
$player.Load()
$player.PlaySync()
'@
  $toneProcess = $null
  if ($SourceMode -eq "system") {
    [SniptaleHotkeyInput]::EnsurePlaybackAudible()
    Start-Sleep -Seconds 2
    New-SyntheticToneWave $tonePath $ffmpeg
    [IO.File]::WriteAllText($toneRunnerPath, $toneRunnerSource, [Text.UTF8Encoding]::new($false))
  }
  try {
    foreach ($attempt in 1..2) {
      $scenario = if ($SourceMode -eq "system") { "recording-system-audio" } else { "recording-microphone" }
      Write-ProgressEvent $scenario "retry" $attempt 2 55
      if ($attempt -gt 1) { Restart-AgentWithMode "manual" }
      Start-Sleep -Seconds 2
      $existingRecordings = @{}
      Get-ChildItem -LiteralPath $media -Filter *.mp4 -ErrorAction SilentlyContinue | ForEach-Object {
        $existingRecordings[$_.FullName] = $true
      }
      $existingTempRecords = @{}
      Get-ChildItem -LiteralPath $tempMedia -Filter *.json -ErrorAction SilentlyContinue | ForEach-Object {
        $existingTempRecords[$_.FullName] = $true
      }
      if ($SourceMode -eq "system") {
        if ($toneProcess -and -not $toneProcess.HasExited) {
          Stop-Process -Id $toneProcess.Id -Force -ErrorAction SilentlyContinue
        }
        $toneProcess = Start-Process powershell.exe -ArgumentList @(
          "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
          "-File", "`"$toneRunnerPath`"", "-Wave", "`"$tonePath`""
        ) -PassThru
        Start-Sleep -Seconds 2
        if ($toneProcess.HasExited) { throw "synthetic system-audio playback exited early" }
      }
      # Capture/tray behavior is proved by their own cases. Use the configured Manual hotkey here
      # so the recording case has one deterministic admission path, then wait for the durable
      # writing record created only after the media runtime has started successfully.
      [SniptaleHotkeyInput]::Press(0x76)
      $recordingReady = $false
      $startDeadline = [DateTime]::UtcNow.AddSeconds(30)
      while ([DateTime]::UtcNow -lt $startDeadline) {
        $activeRecord = Get-ChildItem -LiteralPath $tempMedia -Filter *.json -ErrorAction SilentlyContinue |
          Where-Object { -not $existingTempRecords.ContainsKey($_.FullName) } |
          Select-Object -First 1
        if ($activeRecord) {
          $recordingReady = $true
          break
        }
        Start-Sleep -Milliseconds 250
      }
      if (-not $recordingReady) {
        $null = $attemptOutcomes.Add("attempt-${attempt}:start-not-observed")
        [SniptaleHotkeyInput]::Press(0x77)
        if ($toneProcess -and -not $toneProcess.HasExited) {
          Stop-Process -Id $toneProcess.Id -Force -ErrorAction SilentlyContinue
        }
        continue
      }
      # Keep the render endpoint producing samples after the worker has bound WASAPI loopback.
      if ($SourceMode -eq "system") {
        Start-Sleep -Seconds 20
      } else {
        Start-Sleep -Seconds 12
      }
      # RegisterHotKey input is asynchronous. A single synthetic F8 can be lost by the VM input
      # stack even though the agent remains healthy. Prove delivery from the redacted process log
      # and retry only the input gesture; once observed, never send a second stop command.
      $hotkeyEventsBeforeStop = Get-AgentHotkeyEventCount
      $stopObserved = $false
      foreach ($stopAttempt in 1..3) {
        Write-ProgressEvent $scenario "stop-delivery" $stopAttempt 3 3
        [SniptaleHotkeyInput]::Press(0x77)
        $stopDeliveryDeadline = [DateTime]::UtcNow.AddSeconds(3)
        while ([DateTime]::UtcNow -lt $stopDeliveryDeadline) {
          if ((Get-AgentHotkeyEventCount) -gt $hotkeyEventsBeforeStop) {
            $stopObserved = $true
            break
          }
          Start-Sleep -Milliseconds 200
        }
        if ($stopObserved) { break }
      }
      if (-not $stopObserved) {
        $null = $attemptOutcomes.Add("attempt-${attempt}:stop-hotkey-not-observed")
        Write-AgentRecordingDiagnostics $SourceMode $attempt
        if ($toneProcess -and -not $toneProcess.HasExited) {
          Stop-Process -Id $toneProcess.Id -Force -ErrorAction SilentlyContinue
        }
        $toneProcess = $null
        continue
      }
      Write-ProgressEvent $scenario "finalizing" $attempt 2 20
      $finalizeDeadline = [DateTime]::UtcNow.AddSeconds(20)
      $candidate = $null
      $discardObservedAt = $null
      $attemptFailure = $null
      $writingStalledAt = [DateTime]::UtcNow
      $lastWritingBytes = -1L
      while ([DateTime]::UtcNow -lt $finalizeDeadline) {
        $recordingFailure = Get-AgentRecordingFailure
        if ($recordingFailure) {
          $attemptFailure = "runtime-failure-$recordingFailure"
          break
        }
        $candidate = Get-ChildItem -LiteralPath $media -Filter *.mp4 -ErrorAction SilentlyContinue |
          Where-Object { -not $existingRecordings.ContainsKey($_.FullName) } |
          Sort-Object LastWriteTime | Select-Object -Last 1
        if ($candidate -and $candidate.Length -ge 4096) {
          $candidateLength = $candidate.Length
          Start-Sleep -Seconds 5
          $candidate.Refresh()
          if ($candidate.Length -eq $candidateLength) {
            $validationPcm = Join-Path $env:TEMP "sniptale-e2e-validation-$SourceMode.pcm"
            Remove-Item -LiteralPath $validationPcm -Force -ErrorAction SilentlyContinue
            $null = & $ffmpeg -nostdin -v error -i $candidate.FullName -map 0:a:0 -f s16le -ac 1 -ar 48000 -y $validationPcm 2>&1
            # Mono s16le at 48 kHz is 96,000 bytes/second. Require ten seconds and actual
            # non-zero samples so a decodable MP4 with a token audio packet cannot pass.
            $minimumPcmBytes = if ($SourceMode -eq "system") { 960000 } else { 192000 }
            $decodeSucceeded = $LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $validationPcm)
            $pcmBytes = if ($decodeSucceeded) { (Get-Item -LiteralPath $validationPcm).Length } else { 0 }
            $validPcm = $decodeSucceeded -and (Test-NonSilentPcm $validationPcm $minimumPcmBytes)
            if ($validPcm) {
              $acceptedRecording = $candidate
              $null = $attemptOutcomes.Add("attempt-${attempt}:pass")
              Remove-Item -LiteralPath $validationPcm -Force -ErrorAction SilentlyContinue
              break
            }
            $reason = if (-not (Test-BinaryMarker $candidate.FullName "vide")) {
              "missing-video-track"
            } elseif (-not (Test-BinaryMarker $candidate.FullName "soun")) {
              "missing-audio-track"
            } elseif (-not $decodeSucceeded) {
              "audio-decode-failed"
            } elseif ($pcmBytes -lt $minimumPcmBytes) {
              "audio-too-short-${pcmBytes}-bytes"
            } else {
              "audio-silent"
            }
            $null = $attemptOutcomes.Add("attempt-${attempt}:$reason")
            if ($candidate.Length -le 16777216) {
              Copy-Item -LiteralPath $candidate.FullName -Destination (Join-Path $outputPath "recordings\rejected-$SourceMode-attempt-$attempt.mp4") -Force
            }
            Remove-Item -LiteralPath $validationPcm -Force -ErrorAction SilentlyContinue
            # The candidate is stable and finalization has completed. Retrying the
            # same rejected file until the deadline cannot change its contents.
            break
          }
        }
        # A stop failure deletes the Writing record before the Manual asset can be finalized.
        # Once that deletion remains stable for three seconds, more polling cannot
        # produce a published MP4. End this attempt with an exact reason and preserve the bounded
        # retry budget for genuine transient startup failures.
        if (-not (Test-Path -LiteralPath $activeRecord.FullName)) {
          if ($null -eq $discardObservedAt) {
            $discardObservedAt = [DateTime]::UtcNow
          } elseif (([DateTime]::UtcNow - $discardObservedAt).TotalSeconds -ge 3) {
            $attemptFailure = "runtime-discarded"
            break
          }
        } else {
          $discardObservedAt = $null
          $activeMedia = [IO.Path]::ChangeExtension($activeRecord.FullName, ".mp4")
          $writingBytes = if (Test-Path -LiteralPath $activeMedia) { (Get-Item -LiteralPath $activeMedia).Length } else { 0L }
          if ($writingBytes -ne $lastWritingBytes) {
            $lastWritingBytes = $writingBytes
            $writingStalledAt = [DateTime]::UtcNow
          } elseif (([DateTime]::UtcNow - $writingStalledAt).TotalSeconds -ge 10) {
            $attemptFailure = "finalization-stalled-${writingBytes}-bytes"
            break
          }
        }
        Start-Sleep -Seconds 1
      }
      # Keep the render endpoint active until Media Foundation has closed the MP4. Killing the
      # SoundPlayer process immediately after the stop hotkey races WASAPI loopback teardown with
      # encoder finalization and intermittently strands the recording in Writing state.
      if ($toneProcess -and -not $toneProcess.HasExited) {
        Stop-Process -Id $toneProcess.Id -Force -ErrorAction SilentlyContinue
      }
      $toneProcess = $null
      Write-AgentRecordingDiagnostics $SourceMode $attempt
      if ($attemptFailure) {
        $null = $attemptOutcomes.Add("attempt-${attempt}:$attemptFailure")
      } elseif (-not $acceptedRecording -and -not $candidate) {
        $null = $attemptOutcomes.Add("attempt-${attempt}:no-finalized-mp4")
      }
      if ($acceptedRecording) { break }
    }
  } finally {
    if ($toneProcess -and -not $toneProcess.HasExited) {
      Stop-Process -Id $toneProcess.Id -Force -ErrorAction SilentlyContinue
    }
    if ($fixture -and -not $fixture.HasExited) { Stop-Process -Id $fixture.Id -Force -ErrorAction SilentlyContinue }
    Remove-Item -LiteralPath $fixturePath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $tonePath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $toneRunnerPath -Force -ErrorAction SilentlyContinue
  }
  if (-not $acceptedRecording) {
    throw "Sniptale $SourceMode recording failed after bounded retries: $($attemptOutcomes -join ',')"
  }
  if (-not (Test-BinaryMarker $acceptedRecording.FullName "vide")) { throw "Sniptale MP4 has no video track marker" }
  if (-not (Test-BinaryMarker $acceptedRecording.FullName "soun")) { throw "Sniptale MP4 has no audio track marker" }
  $target = Join-Path $outputPath "recordings\sniptale-synthetic-$SourceMode-audio.mp4"
  Copy-Item -LiteralPath $acceptedRecording.FullName -Destination $target
  $pcm = Join-Path $env:TEMP "sniptale-e2e-$SourceMode.pcm"
  & $ffmpeg -nostdin -v error -i $target -map 0:a:0 -f s16le -ac 1 -ar 48000 -y $pcm
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $pcm) -or (Get-Item -LiteralPath $pcm).Length -lt 4096) {
    throw "ffmpeg could not decode the $SourceMode audio track"
  }
  $stream = [IO.File]::OpenRead($pcm)
  try {
    $buffer = [byte[]]::new(1048576)
    $read = $stream.Read($buffer, 0, $buffer.Length)
    $nonzero = @($buffer[0..($read - 1)] | Where-Object { $_ -ne 0 }).Count
    if ($nonzero -lt 100) { throw "$SourceMode PCM is silent" }
  } finally {
    $stream.Dispose()
    Remove-Item -LiteralPath $pcm -Force -ErrorAction SilentlyContinue
  }
  if ($SourceMode -eq "system") {
    "Sniptale MP4 captured synthetic motion and non-silent system audio"
  } else {
    "Sniptale MP4 captured non-silent microphone audio"
  }
}

Add-Test "recording-system-audio" {
  Invoke-SyntheticRecording "system"
}
Add-Test "recording-microphone" {
  Invoke-SyntheticRecording "microphone"
}
Add-Test "reconnect-recovery" {
  try {
    Restart-AgentWithMode "extension-managed"
    $chromeResult = Invoke-BrowserNativeProbeWithRetry "chrome" $chrome $extensionPath "pass" "reconnect-recovery"
    $edgeResult = Invoke-BrowserNativeProbeWithRetry "edge" $edge $extensionPath "pass" "reconnect-recovery"
    "resident restart recovery completed; $chromeResult; $edgeResult"
  } finally {
    Restart-AgentWithMode "manual"
  }
}
Add-Test "uninstall" {
  Stop-Process -Id $agent.Id -Force -ErrorAction SilentlyContinue
  Invoke-Msi @("/x", $Msi) "uninstall"
  if (Test-Path -LiteralPath $installed) { throw "installed executable remains after uninstall" }
  if (Test-Path -LiteralPath (Join-Path ([IO.Path]::GetDirectoryName($installed)) "native-host.json")) { throw "native host manifest remains after uninstall" }
  foreach ($name in @("LICENSE.txt", "NOTICE.txt", "THIRD_PARTY_NOTICES.md", "THIRD_PARTY_DEPENDENCIES.json")) {
    if (Test-Path -LiteralPath (Join-Path ([IO.Path]::GetDirectoryName($installed)) $name)) { throw "legal payload remains after uninstall: $name" }
  }
  foreach ($browser in @(
    "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.sniptale.dev.native_host",
    "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\com.sniptale.dev.native_host",
    "HKCU:\Software\Chromium\NativeMessagingHosts\com.sniptale.dev.native_host"
  )) {
    if (Test-Path -LiteralPath $browser) { throw "native host registration remains after uninstall" }
  }
  $runValue = (Get-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -ErrorAction SilentlyContinue).SniptaleNativeAgent
  if ($null -ne $runValue) { throw "autostart registration remains after uninstall" }
  foreach ($stateFile in @("pipe-secret.json", "dev-local-browser-trust.json", "trusted-connections.json")) {
    if (Test-Path -LiteralPath (Join-Path ([IO.Path]::GetDirectoryName($installed)) $stateFile)) {
      throw "authorization state remains after uninstall: $stateFile"
    }
  }
  "MSI uninstall removed binaries, manifest, browser registrations, autostart, and authorization state"
}

if ($script:extensionPath) { Remove-Item -LiteralPath $script:extensionPath -Recurse -Force -ErrorAction SilentlyContinue }
if ($script:probeListenerScript) { Remove-Item -LiteralPath $script:probeListenerScript -Force -ErrorAction SilentlyContinue }
Remove-Item -LiteralPath (Join-Path $outputPath "product-media") -Recurse -Force -ErrorAction SilentlyContinue
$agentLogDirectory = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent\logs"
Get-ChildItem -LiteralPath $agentLogDirectory -File -ErrorAction SilentlyContinue | ForEach-Object {
  Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $outputPath "logs\agent-$($_.Name)") -Force
}
Get-ChildItem -LiteralPath (Join-Path $outputPath "logs") -File -ErrorAction SilentlyContinue |
  Where-Object { $_.Extension -ne ".jsonl" } | ForEach-Object {
  $text = Get-Content -Raw -LiteralPath $_.FullName -ErrorAction SilentlyContinue
  if ($null -ne $text) {
    $text = $text -replace '(?i)(?:authorization|cookie|token|password|secret)\s*[:=]\s*[^\s,;]+', '[redacted]'
    $text = $text -replace "(?i)https?://[^\s`"']+", '[redacted-url]'
    $text = $text -replace "(?:[A-Za-z]:\\)[^\r\n`"']+", '[redacted-path]'
    [IO.File]::WriteAllText($_.FullName, $text, [Text.UTF8Encoding]::new($false))
  }
}
foreach ($test in $tests) {
  $detail = [string]$test.detail
  $detail = $detail -replace '(?i)(?:authorization|cookie|token|password|secret)\s*[:=]\s*[^\s,;]+', '[redacted]'
  $detail = $detail -replace "(?i)https?://[^\s`"']+", '[redacted-url]'
  $detail = $detail -replace "(?:[A-Za-z]:\\)[^\r\n`"']+", '[redacted-path]'
  $test.detail = $detail.Substring(0, [Math]::Min($detail.Length, 4096))
}
$artifacts = Get-ChildItem -LiteralPath $outputPath -Recurse -File | ForEach-Object { $_.Name }
$value = [ordered]@{
  schema_version = 1
  lane = "windows-11"
  tests = @($tests)
  environment = [ordered]@{
    display_server = "win32"
    session = "local-console"
    runner_version = "1"
    recording_fixture_quality = "low-fps24"
    microphone_fixture = "qemu-hda-alsa-infinite-synthetic-noise"
    system_audio_fixture = "qemu-hda-wav-duplex-880hz-s16le-48000-stereo"
  }
  artifacts = @($artifacts)
}
[IO.File]::WriteAllText((Join-Path $outputPath "result.json"), ($value | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
if ($tests.Where({ $_.status -ne "pass" }).Count -gt 0) { exit 1 }
