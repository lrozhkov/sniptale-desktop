. "$PSScriptRoot\browser-playwright.ps1"
function Invoke-RealExtension([string] $Name, [string] $Executable, [string] $ExtensionPath) {
  Stop-ProbeBrowser
  $port = if ($Name -eq "chrome") { 9223 } else { 9224 }
  $browserProfile = "C:\SniptaleLab\browser-profiles\$Name"
  $installedMarker = Join-Path $browserProfile "sniptale-extension-installed"
  $profileExists = Test-Path -LiteralPath $installedMarker
  $null = New-Item -ItemType Directory -Force -Path $browserProfile
  $browserArguments = @(
    "--user-data-dir=$browserProfile", "--remote-debugging-address=127.0.0.1", "--remote-debugging-port=$port",
    "--force-renderer-accessibility", "--disable-background-mode", "--no-first-run", "--no-default-browser-check", "--new-window", "about:blank"
  )
  try {
    $window = [IntPtr]::Zero
    foreach ($startupAttempt in 1..3) {
      Write-ProgressEvent "real-extension-$Name-startup" "started" $startupAttempt 3 30
      $browser = Start-Process -FilePath $Executable -ArgumentList $browserArguments -PassThru
      try {
        $window = Focus-BrowserWindow $Name 30
        Write-ProgressEvent "real-extension-$Name-startup" "pass" $startupAttempt 3 30
        break
      } catch {
        $exitCategory = if ($browser.HasExited) { "exit-$($browser.ExitCode)" } else { "no-visible-window" }
        Stop-ProbeBrowser
        Write-ProgressEvent "real-extension-$Name-startup" "retry" $startupAttempt 3 30
        if ($startupAttempt -eq 3) { throw "$Name browser startup failed after 3 attempts ($exitCategory)" }
        Start-Sleep -Seconds 1
      }
    }
    if (-not $profileExists) {
      $internal = if ($Name -eq "edge") { "edge://extensions" } else { "chrome://extensions" }
      Set-BrowserLocation $window $internal
      Start-Sleep -Seconds 3
      if ($Name -eq "edge") { Open-EdgeExtensionsMenu $window }
      $developer = Find-Ui "Developer mode" 15
      Install-UnpackedBrowserExtension $Name $window $ExtensionPath $developer
      $null = Find-Ui "Sniptale" 20
      [IO.File]::WriteAllText($installedMarker, "lab-extension-v1")
    }
    Add-BrowserConnectionTrust $(if ($Name -eq "chrome") { "chrome" } else { "msedge" })
    Set-BrowserLocation $window "chrome-extension://elhndangpenapadjimfoodlaenpcndpo/apps/extension/src/settings/index.html"
    Start-Sleep -Seconds 3
    $scenario = Get-Content -Raw -LiteralPath "$PSScriptRoot\real-extension.js"
    $connect = Invoke-LabBrowserScript $port ("globalThis.__labAction='connect';" + $scenario)
    if (-not $connect.passed) { throw "$Name real extension connection failed: $($connect.connectionState) $($connect.category)" }
    $before = Invoke-LabBrowserScript $port ("globalThis.__labAction='before-capture';" + $scenario)
    if (-not $before.passed) { throw "$Name real extension could not observe its media store" }
    # Exercise the native tray entry. Windows owns bare PrintScreen on this OS;
    # this scenario proves native capture and real browser delivery, not that OS shortcut.
    Invoke-RealExtensionScreenCapture
    $media = Invoke-LabBrowserScript $port ("globalThis.__labAction='verify-capture';" + $scenario)
    if (-not $media.passed) { throw "$Name native screenshot was not received and hash-verified by the real extension: $($media.category)" }
    Save-Desktop "real-extension-$Name"
    # A fresh product-level reconnect must restore lease and applied settings in the same profile.
    $reconnect = Invoke-LabBrowserScript $port ("globalThis.__labAction='connect';" + $scenario)
    if (-not $reconnect.passed) { throw "$Name product reconnect failed" }
    Restart-AgentWithMode "extension-managed"
    $restart = Invoke-LabBrowserScript $port ("globalThis.__labAction='connect';" + $scenario)
    if (-not $restart.passed) { throw "$Name reconnect after resident restart failed" }
    return [ordered]@{ browser = $Name; profile_reused = $profileExists; connection = $connect; capture_trigger = "native-tray-current-screen"; media = $media; reconnect = $reconnect; resident_restart = $restart }
  } catch {
    $failure = $_
    try { Save-Desktop "real-extension-$Name-failure" } catch { Write-Verbose 'Failure screenshot unavailable; preserving the scenario failure.' }
    throw $failure
  } finally {
    Stop-ProbeBrowser
  }
}

function Invoke-RealExtensionScreenCapture {
  if (-not ('SniptaleRealExtensionMenu' -as [type])) { Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class SniptaleRealExtensionMenu {
  [StructLayout(LayoutKind.Sequential)] struct Rect { public int Left,Top,Right,Bottom; }
  delegate bool EnumProc(IntPtr window, IntPtr parameter);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr parameter);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window,StringBuilder name,int count);
  [DllImport("user32.dll")] static extern IntPtr SendMessage(IntPtr window,uint message,IntPtr wparam,IntPtr lparam);
  [DllImport("user32.dll")] static extern int GetMenuItemCount(IntPtr menu);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetMenuString(IntPtr menu,uint item,StringBuilder text,int count,uint flags);
  [DllImport("user32.dll")] static extern uint GetMenuState(IntPtr menu,uint item,uint flags);
  [DllImport("user32.dll")] static extern bool GetMenuItemRect(IntPtr window,IntPtr menu,uint item,out Rect rect);
  [DllImport("user32.dll")] static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] static extern void mouse_event(uint flags,uint x,uint y,uint data,UIntPtr extra);
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
  Initialize-SniptaleTrayProbe
  $trayWindow = [SniptaleTrayProbe]::FindWindow("SniptaleTrayCommandSource", $null)
  if ($trayWindow -eq [IntPtr]::Zero) { throw "native tray window unavailable for real extension capture" }
  [SniptaleTrayProbe]::SetCursorPos(480, 300) | Out-Null
  $callback = [IntPtr]((1 -shl 16) -bor 0x007b)
  if (-not [SniptaleTrayProbe]::PostMessage($trayWindow, 0x8001, [UIntPtr]::Zero, $callback)) { throw "native tray context callback failed" }
  foreach ($label in @("Screenshot", "Current screen")) {
    $clicked = $false
    foreach ($attempt in 1..30) {
      if ([SniptaleRealExtensionMenu]::Click($label)) { $clicked = $true; break }
      Start-Sleep -Milliseconds 100
    }
    if (-not $clicked) { throw "native tray capture menu entry unavailable" }
    Start-Sleep -Milliseconds 300
  }
}
