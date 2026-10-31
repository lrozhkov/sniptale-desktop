# Shared isolated guest UI and explicit development authorization fixtures.
function ConvertTo-LowerHex([byte[]] $Bytes) {
  -join ($Bytes | ForEach-Object { $_.ToString("x2") })
}


function Provision-DevRuntimeTrust {
  Add-Type -AssemblyName System.Security
  $directory = [IO.Path]::GetDirectoryName($installed)
  $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
  $secretBytes = [byte[]]::new(32)
  $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
  try { $rng.GetBytes($secretBytes) } finally { $rng.Dispose() }
  $plain = [Text.Encoding]::UTF8.GetBytes((ConvertTo-LowerHex $secretBytes))
  $protected = [System.Security.Cryptography.ProtectedData]::Protect(
    $plain,
    $null,
    [System.Security.Cryptography.DataProtectionScope]::CurrentUser
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


function Save-Desktop([string] $Name) {
  Add-Type -AssemblyName System.Drawing
  Add-Type -AssemblyName System.Windows.Forms
  $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
  $bitmap = [Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
  $graphics = [Drawing.Graphics]::FromImage($bitmap)
  try { $graphics.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $bounds.Size); $bitmap.Save((Join-Path $outputPath "screenshots\$Name.png"), [Drawing.Imaging.ImageFormat]::Png) }
  finally { $graphics.Dispose(); $bitmap.Dispose() }
}


function Find-Ui([string] $Name, [int] $Seconds = 20) {
  Add-Type -AssemblyName UIAutomationClient
  $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
  $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, $Name)
  while ([DateTime]::UtcNow -lt $deadline) {
    $item = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
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
  $browserProfile = Get-Content -Raw -LiteralPath $profilePath | ConvertFrom-Json
  $browserProfile.revision = [uint64]$browserProfile.revision + 1
  $browserProfile.operatingMode = $Mode
  if ($Mode -eq "manual" -and $null -eq $browserProfile.manualProfile) {
    $browserProfile.manualProfile = $script:manualProfileTemplate
  }
  [IO.File]::WriteAllText($profilePath, ($browserProfile | ConvertTo-Json -Depth 8 -Compress), [Text.UTF8Encoding]::new($false))
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


function Get-LabBrowserExecutable([ValidateSet("chrome", "edge")][string] $Name) {
  $relative = if ($Name -eq "chrome") { "Google\Chrome\Application\chrome.exe" } else { "Microsoft\Edge\Application\msedge.exe" }
  $executable = @((Join-Path $env:ProgramFiles $relative), (Join-Path ${env:ProgramFiles(x86)} $relative)) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
  if (-not $executable) { throw "$Name browser is not installed" }
  return $executable
}

function Install-UnpackedBrowserExtension([string] $Name, [IntPtr] $Window, [string] $ExtensionPath, [object] $DeveloperMode) {
  Add-Type -AssemblyName System.Windows.Forms
  Add-Type -AssemblyName UIAutomationClient
  $loadUnpacked = $null
  try { $loadUnpacked = Find-Ui "Load unpacked" 1 } catch { $loadUnpacked = $null }
  if (-not $loadUnpacked) {
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
  [System.Windows.Forms.SendKeys]::SendWait("^a")
  [System.Windows.Forms.SendKeys]::SendWait($ExtensionPath)
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
  $root = [System.Windows.Automation.AutomationElement]::FromHandle($Window)
  $nameCondition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    "Address and search bar"
  )
  $address = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $nameCondition)
  $editCondition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Edit
  )
  if (-not $address) { $address = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $editCondition) }
  if ($address) {
    try {
      $address.SetFocus()
      $value = $address.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
      $value.SetValue($Url)
    } catch {
      $address = $null
    }
  }
  if (-not $address) {
    [SniptaleBrowserFocus]::ShowWindowAsync($Window, 9) | Out-Null
    if (-not [SniptaleBrowserFocus]::SetForegroundWindow($Window)) { throw "browser location fallback could not focus the window" }
    [System.Windows.Forms.SendKeys]::SendWait("^l")
    [System.Windows.Forms.SendKeys]::SendWait($Url)
  }
  [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
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


function Stop-ProbeBrowser {
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


function Add-BrowserConnectionTrust([string] $ProcessName, [string] $Origin = "chrome-extension://elhndangpenapadjimfoodlaenpcndpo/") {
  $storePath = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent\trusted-connections.json"
  $store = if (Test-Path -LiteralPath $storePath) {
    Get-Content -Raw -LiteralPath $storePath | ConvertFrom-Json
  } else {
    [pscustomobject]@{ formatVersion = 1; trusted = @() }
  }
  $trusted = @($store.trusted)
  $origin = $Origin
  $hostHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $installed).Hash.ToLowerInvariant()
  $trusted = @($trusted | Where-Object { $_.hostExeSha256 -eq $hostHash })
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
    $sha = [System.Security.Cryptography.SHA256]::Create()
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

