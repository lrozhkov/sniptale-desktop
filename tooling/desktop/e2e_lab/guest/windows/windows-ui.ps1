# Shared isolated guest UI and explicit development authorization fixtures.
function ConvertTo-LowerHex([byte[]] $Bytes) {
  -join ($Bytes | ForEach-Object { $_.ToString("x2") })
}


function Provision-DevRuntimeTrust {
  Add-Type -AssemblyName System.Security
  $directory = [IO.Path]::GetDirectoryName($installed)
  $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
  $secretPath = Join-Path $directory "pipe-secret.json"
  if (Test-Path -LiteralPath $secretPath) {
    $existing = Get-Content -Raw -LiteralPath $secretPath | ConvertFrom-Json
    if ($existing.installId -ne "install-dev" -or $existing.currentUserSid -ne $sid -or $existing.authAlgorithm -ne "pipe-auth-v2-hmac-sha256" -or $existing.protection -ne "current-user-dpapi" -or $existing.protectedSecretHex -notmatch '^(?:[0-9a-fA-F]{2}){32,}$') {
      throw "existing laboratory pipe credential must validate; it must never be silently replaced"
    }
  } else {
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
  }
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

function Find-DesktopSettingsWindow([int] $Seconds = 20) {
  Add-Type -AssemblyName UIAutomationClient
  $settingsTitle = "Sniptale $([char]0x2014) Settings"
  $condition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::ClassNameProperty, "SniptaleSettingsWindowV2"
  )
  $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
  while ([DateTime]::UtcNow -lt $deadline) {
    $windows = [System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $condition)
    foreach ($window in $windows) {
      if ($window.Current.Name -eq $settingsTitle -and $window.Current.ProcessId -eq $script:agent.Id) { return $window }
    }
    Start-Sleep -Milliseconds 250
  }
  throw "native Desktop settings window not found for the installed resident process"
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
  if ($bounds.Width -lt 4 -or $bounds.Height -lt 4) { throw "UI element bounds are invalid" }
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


function Initialize-SniptaleMenuProbe {
  if (-not ('SniptaleRealExtensionMenu' -as [type])) { Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class SniptaleRealExtensionMenu {
  [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr window,int id);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
  public static void WheelDown() { mouse_event(0x0800,0,0,unchecked((uint)-120),UIntPtr.Zero); }
  [StructLayout(LayoutKind.Sequential)] struct Rect { public int Left,Top,Right,Bottom; }
  [StructLayout(LayoutKind.Sequential)] struct Point { public int X,Y; }
  delegate bool EnumProc(IntPtr window, IntPtr parameter);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr parameter);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window,StringBuilder name,int count);
  [DllImport("user32.dll")] static extern IntPtr SendMessage(IntPtr window,uint message,IntPtr wparam,IntPtr lparam);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr window,out Rect rect);
  [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(Point point);
  [DllImport("user32.dll")] static extern bool IsChild(IntPtr parent,IntPtr child);
  [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
  [DllImport("user32.dll")] static extern int GetMenuItemCount(IntPtr menu);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetMenuString(IntPtr menu,uint item,StringBuilder text,int count,uint flags);
  [DllImport("user32.dll")] static extern uint GetMenuState(IntPtr menu,uint item,uint flags);
  [DllImport("user32.dll")] static extern bool GetMenuItemRect(IntPtr window,IntPtr menu,uint item,out Rect rect);
  [DllImport("user32.dll")] static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] static extern void mouse_event(uint flags,uint x,uint y,uint data,UIntPtr extra);
  public static int ComboIndex(IntPtr control) { return SendMessage(control,0x0147,IntPtr.Zero,IntPtr.Zero).ToInt32(); }
  public static int ComboCount(IntPtr control) { return SendMessage(control,0x0146,IntPtr.Zero,IntPtr.Zero).ToInt32(); }
  public static bool ComboDropped(IntPtr control) { return SendMessage(control,0x0157,IntPtr.Zero,IntPtr.Zero)!=IntPtr.Zero; }
  public static bool CanClickControl(IntPtr control,bool combo) { return PointerControl(control,combo,false); }
  public static bool ClickControl(IntPtr control,bool combo) { return PointerControl(control,combo,true); }
  static bool PointerControl(IntPtr control,bool combo,bool click) {
    // GetWindowRect is DPI-virtualized. Keep geometry, hit testing and pointer input in physical pixels.
    var previous=SetThreadDpiAwarenessContext(new IntPtr(-4));
    if(previous==IntPtr.Zero) return false;
    try {
      Rect bounds;
      if(!GetWindowRect(control,out bounds) || bounds.Right-bounds.Left<4 || bounds.Bottom-bounds.Top<4) return false;
      var point=new Point { X=bounds.Left+(bounds.Right-bounds.Left)/2,
        Y=bounds.Top+(combo?Math.Min(16,(bounds.Bottom-bounds.Top)/2):(bounds.Bottom-bounds.Top)/2) };
      var hit=WindowFromPoint(point);
      if(hit!=control && !IsChild(control,hit)) return false;
      if(!click) return true;
      if(!SetCursorPos(point.X,point.Y)) return false;
      mouse_event(2,0,0,0,UIntPtr.Zero);
      mouse_event(4,0,0,0,UIntPtr.Zero);
      return true;
    } finally {
      if(SetThreadDpiAwarenessContext(previous)==IntPtr.Zero) throw new InvalidOperationException("DPI context restoration failed");
    }
  }
  public static bool Click(string label) {
    bool clicked=false;
    EnumWindows((window,parameter)=> {
      var cls=new StringBuilder(64);
      if(GetClassName(window,cls,cls.Capacity)==0 || cls.ToString()!="#32768") return true;
      IntPtr menu=SendMessage(window,0x01E1,IntPtr.Zero,IntPtr.Zero);
      int count=GetMenuItemCount(menu);
      if(count<1 || count>64) return true;
      for(uint i=0;i<count;i++) {
        var text=new StringBuilder(256);
        GetMenuString(menu,i,text,text.Capacity,0x400);
        if(text.ToString().Split('\t')[0].Replace("&","")!=label) continue;
        Rect rect;
        if((GetMenuState(menu,i,0x400)&3)!=0 || !GetMenuItemRect(IntPtr.Zero,menu,i,out rect) || rect.Right<=rect.Left || rect.Bottom<=rect.Top) return true;
        if(!SetCursorPos((rect.Left+rect.Right)/2,(rect.Top+rect.Bottom)/2)) return true;
        mouse_event(2,0,0,0,UIntPtr.Zero); mouse_event(4,0,0,0,UIntPtr.Zero);
        clicked=true; return false;
      }
      return true;
    },IntPtr.Zero);
    return clicked;
  }
}
'@
  }
}

function Invoke-SniptaleMenuItem([string] $Label) {
  Initialize-SniptaleMenuProbe
  foreach ($attempt in 1..30) {
    if ([SniptaleRealExtensionMenu]::Click($Label)) { return }
    Start-Sleep -Milliseconds 100
  }
  throw "native tray menu entry unavailable: $Label"
}

function Show-SniptaleTrayMenu {
  Initialize-SniptaleTrayProbe
  $trayWindow = [SniptaleTrayProbe]::FindWindow("SniptaleTrayCommandSource", $null)
  if ($trayWindow -eq [IntPtr]::Zero) { throw "native tray window unavailable" }
  [SniptaleTrayProbe]::SetCursorPos(480, 300) | Out-Null
  $callback = [IntPtr]((1 -shl 16) -bor 0x007b)
  if (-not [SniptaleTrayProbe]::PostMessage($trayWindow, 0x8001, [UIntPtr]::Zero, $callback)) { throw "native tray context callback failed" }
}

function Invoke-DesktopSettingsButton([IntPtr] $Window, [int] $Id) {
  $handle = [SniptaleRealExtensionMenu]::GetDlgItem($Window, $Id)
  if ($handle -eq [IntPtr]::Zero) { throw "Desktop button is missing" }
  $control = [System.Windows.Automation.AutomationElement]::FromHandle($handle)
  if (-not $control.Current.IsEnabled -or $control.Current.IsOffscreen) { throw "Desktop button is not reachable" }
  if (-not [SniptaleRealExtensionMenu]::ClickControl($handle, $false)) { throw "Desktop button did not pass pointer hit testing (control $Id)" }
  Start-Sleep -Milliseconds 300
}

function Select-DesktopBrowserDestination([string] $BrowserFamily, [bool] $AskAfterCapture = $false) {
  Add-Type -AssemblyName UIAutomationClient
  Add-Type -AssemblyName System.Windows.Forms
  Initialize-SniptaleMenuProbe
  Show-SniptaleTrayMenu
  Invoke-SniptaleMenuItem "Settings$([char]0x2026)"
  $window = Find-DesktopSettingsWindow 15
  $handle = [IntPtr]$window.Current.NativeWindowHandle
  if (-not [SniptaleRealExtensionMenu]::SetForegroundWindow($handle)) { throw "Desktop settings could not be focused" }
  Invoke-DesktopSettingsButton $handle 30
  $bounds = $window.Current.BoundingRectangle
  [SniptaleTrayProbe]::SetCursorPos([int]($bounds.Right - 30), [int]($bounds.Top + $bounds.Height / 2)) | Out-Null
  $deliveryVisible = $false
  foreach ($step in 0..12) {
    $deliveryVisible = $true
    foreach ($id in @(19, 20, 21, 22)) {
      $child = [SniptaleRealExtensionMenu]::GetDlgItem($handle, $id)
      if ($child -eq [IntPtr]::Zero) { throw "Desktop delivery control is missing" }
      $control = [System.Windows.Automation.AutomationElement]::FromHandle($child)
      if ($control.Current.IsOffscreen -or -not [SniptaleRealExtensionMenu]::CanClickControl($child, $id -eq 19)) { $deliveryVisible = $false; break }
    }
    if ($deliveryVisible) { break }
    if ($step -lt 12) { [SniptaleRealExtensionMenu]::WheelDown(); Start-Sleep -Milliseconds 100 }
  }
  if (-not $deliveryVisible) { throw "Desktop delivery controls could not be scrolled into view" }
  Invoke-DesktopSettingsButton $handle 22
  $registryPath = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent\browser-targets.json"
  $registry = Get-Content -Raw -LiteralPath $registryPath | ConvertFrom-Json
  $destinationMatches = @()
  for ($index = 0; $index -lt @($registry.targets).Count; $index++) {
    $target = $registry.targets[$index]
    if ($target.browserFamily -eq $BrowserFamily -and $target.extensionId -eq "elhndangpenapadjimfoodlaenpcndpo" -and $target.profileKey -notin @("e2e-profile", "smoke-profile")) { $destinationMatches += $index }
  }
  if ($destinationMatches.Count -ne 1) { throw "real browser destination is missing or ambiguous" }
  $selectedIndex = $destinationMatches[0]
  $combo = [System.Windows.Automation.AutomationElement]::FromHandle([SniptaleRealExtensionMenu]::GetDlgItem($handle, 19))
  if (-not $combo.Current.IsEnabled -or $combo.Current.IsOffscreen) { throw "Desktop browser selector must be reachable" }
  $comboHandle = [SniptaleRealExtensionMenu]::GetDlgItem($handle, 19)
  if ([SniptaleRealExtensionMenu]::ComboCount($comboHandle) -ne @($registry.targets).Count + 1) { throw "Desktop browser choices do not match registered destinations" }
  Save-UiWindow $window "desktop-destination-$BrowserFamily-before"
  if (-not [SniptaleRealExtensionMenu]::ClickControl($comboHandle, $true)) { throw "Desktop browser selector did not pass pointer hit testing" }
  Start-Sleep -Milliseconds 300
  [System.Windows.Forms.SendKeys]::SendWait("{HOME}")
  Start-Sleep -Milliseconds 300
  if (-not [SniptaleRealExtensionMenu]::ComboDropped($comboHandle)) { throw "Desktop presentation refresh closed the active browser dropdown" }
  foreach ($step in 0..$selectedIndex) {
    [System.Windows.Forms.SendKeys]::SendWait("{DOWN}")
    Start-Sleep -Milliseconds 300
    if (-not [SniptaleRealExtensionMenu]::ComboDropped($comboHandle)) { throw "Desktop browser dropdown lost its keyboard interaction during refresh" }
  }
  [System.Windows.Forms.SendKeys]::SendWait("{TAB}")
  Start-Sleep -Milliseconds 300
  if ([SniptaleRealExtensionMenu]::GetDlgItem($handle, 19) -eq [IntPtr]::Zero) { throw "Desktop window closed during browser selection" }
  if ([SniptaleRealExtensionMenu]::ComboIndex($comboHandle) -ne $selectedIndex + 1) { throw "Desktop browser selection did not reach the requested native list index" }
  Save-UiWindow $window "desktop-destination-$BrowserFamily-selected"
  if ([bool]$registry.askAfterCapture -ne $AskAfterCapture) { Invoke-DesktopSettingsButton $handle 20 }
  Invoke-DesktopSettingsButton $handle 21
  Start-Sleep -Milliseconds 500
  $saved = Get-Content -Raw -LiteralPath $registryPath | ConvertFrom-Json
  if ($saved.defaultTargetId -ne $registry.targets[$selectedIndex].id -or [bool]$saved.askAfterCapture -ne $AskAfterCapture) { throw "explicit Desktop destination selection was not persisted" }
  Save-UiWindow $window "desktop-destination-$BrowserFamily"
  $window.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
}
