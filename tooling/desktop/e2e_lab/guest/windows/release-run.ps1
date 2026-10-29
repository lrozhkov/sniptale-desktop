param(
  [Parameter(Mandatory = $true)][string] $Msi,
  [Parameter(Mandatory = $true)][string] $MsiSha256,
  [Parameter(Mandatory = $true)][string] $ExpectedExeSha256,
  [Parameter(Mandatory = $true)][string] $ExpectedManifestSha256,
  [Parameter(Mandatory = $true)][string] $ProductVersion,
  [string] $Output = "e2e-out"
)
$ErrorActionPreference = "Stop"
$outputPath = [IO.Path]::GetFullPath($Output)
New-Item -ItemType Directory -Force -Path $outputPath, "$outputPath/logs", "$outputPath/screenshots", "$outputPath/recordings" | Out-Null
$tests = [Collections.Generic.List[object]]::new()
$directory = Join-Path $env:LOCALAPPDATA "Sniptale/NativeAgent"
$installed = Join-Path $directory "sniptale-native-agent.exe"
$exports = Join-Path $env:USERPROFILE "Pictures/SniptaleAlphaProof"
New-Item -ItemType Directory -Force -Path $exports | Out-Null
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Windows.Forms, System.Drawing
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class AlphaUi {
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
  public delegate bool EnumCallback(IntPtr w, IntPtr p);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string n);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr w, uint m, UIntPtr p, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr w, uint m, IntPtr p, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr w);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f,uint x,uint y,uint d,UIntPtr e);
  [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr m);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetMenuString(IntPtr m,uint i,StringBuilder s,int c,uint f);
  [DllImport("user32.dll")] public static extern bool GetMenuItemRect(IntPtr w,IntPtr m,uint i,out Rect r);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumCallback c,IntPtr p);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr w,StringBuilder s,int c);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr w,out Rect r);
  public static IntPtr RightmostMenu() {
    IntPtr found=IntPtr.Zero; int x=-100000;
    EnumWindows((w,p)=>{var s=new StringBuilder(128);GetClassName(w,s,128);Rect r;
      if(s.ToString()=="#32768" && GetWindowRect(w,out r) && r.Left>x){x=r.Left;found=w;}return true;},IntPtr.Zero);
    return found;
  }
}
'@

function Add-Test([string] $Name, [scriptblock] $Body) {
  $watch = [Diagnostics.Stopwatch]::StartNew()
  try {
    & $Body | Out-Null
    $status = "pass"; $detail = "release scenario passed"
  } catch {
    $status = "fail"
    $category = $_.Exception.Message
    if ($category -notmatch '^[A-Za-z0-9 ._-]{1,160}$') { $category = $_.Exception.GetType().Name }
    $detail = "release scenario failed: $category"
  }
  $tests.Add([ordered]@{ name = $Name; status = $status; duration_ms = [int]$watch.ElapsedMilliseconds; detail = $detail })
  Write-Output "[release-progress] $Name $status"
  [IO.File]::AppendAllText("$outputPath/logs/progress.log", "$Name $status`n")
}
function Wait-For([scriptblock] $Probe, [int] $Seconds = 30) {
  $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
  do { $value = & $Probe; if ($value) { return $value }; Start-Sleep -Milliseconds 500 } while ([DateTime]::UtcNow -lt $deadline)
  throw "release condition timed out"
}
function Screenshot([string] $Name) {
  $bounds = [Windows.Forms.SystemInformation]::VirtualScreen
  $bitmap = [Drawing.Bitmap]::new($bounds.Width,$bounds.Height)
  $graphics = [Drawing.Graphics]::FromImage($bitmap)
  try { $graphics.CopyFromScreen($bounds.Left,$bounds.Top,0,0,$bounds.Size); $bitmap.Save("$outputPath/screenshots/$Name.png") }
  finally { $graphics.Dispose(); $bitmap.Dispose() }
}
function Tray([string[]] $Labels) {
  [Windows.Forms.SendKeys]::SendWait("{ESC}{ESC}")
  $window = Wait-For { [AlphaUi]::FindWindow("SniptaleTrayCommandSource",$null) -ne [IntPtr]::Zero }
  $window = [AlphaUi]::FindWindow("SniptaleTrayCommandSource",$null)
  [AlphaUi]::SetCursorPos(600,350) | Out-Null
  [AlphaUi]::PostMessage($window,0x8001,[UIntPtr]::Zero,[IntPtr]((1 -shl 16) -bor 0x007b)) | Out-Null
  foreach ($label in $Labels) {
    Start-Sleep -Milliseconds 800
    $popup = [AlphaUi]::RightmostMenu()
    if ($popup -eq [IntPtr]::Zero) { throw "tray menu unavailable" }
    $menu = [AlphaUi]::SendMessage($popup,0x01e1,[IntPtr]::Zero,[IntPtr]::Zero)
    $found = $false
    for ($i=0; $i -lt [AlphaUi]::GetMenuItemCount($menu); $i++) {
      $text = [Text.StringBuilder]::new(512)
      [void][AlphaUi]::GetMenuString($menu,$i,$text,512,0x400)
      if (($text.ToString().Replace('&','') -split "`t")[0] -eq $label) {
        $rect = [AlphaUi+Rect]::new()
        if (-not [AlphaUi]::GetMenuItemRect([IntPtr]::Zero,$menu,$i,[ref]$rect)) { throw "menu geometry unavailable" }
        [AlphaUi]::SetCursorPos([int](($rect.Left+$rect.Right)/2),[int](($rect.Top+$rect.Bottom)/2)) | Out-Null
        Start-Sleep -Milliseconds 700
        if ($label -eq $Labels[-1]) { [AlphaUi]::mouse_event(2,0,0,0,[UIntPtr]::Zero); [AlphaUi]::mouse_event(4,0,0,0,[UIntPtr]::Zero) }
        $found=$true; break
      }
    }
    if (-not $found) { Screenshot "missing-menu"; throw "menu action unavailable" }
  }
}
function Window([string] $Name) {
  Wait-For {
    [Windows.Automation.AutomationElement]::RootElement.FindFirst(
      [Windows.Automation.TreeScope]::Children,
      [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,$Name))
  }
}
function Click([Windows.Automation.AutomationElement] $Element) {
  $r = $Element.Current.BoundingRectangle
  [AlphaUi]::SetCursorPos([int]($r.X+$r.Width/2),[int]($r.Y+$r.Height/2)) | Out-Null
  [AlphaUi]::mouse_event(2,0,0,0,[UIntPtr]::Zero); [AlphaUi]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
}
function Button($Parent,[string] $Name) {
  $element=$Parent.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,$Name))
  if (-not $element) { throw "settings button unavailable" }; Click $element
}
function Stop-Agent {
  Get-Process sniptale-native-agent -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Seconds 2
}
function Start-Agent {
  $process = Start-Process $installed -PassThru
  Start-Sleep -Seconds 2
  if ($process.HasExited) { throw "resident exited before tray startup" }
}
function Msi([string] $Operation) {
  $process=Start-Process msiexec.exe -ArgumentList @($Operation,"`"$Msi`"","/qn","/norestart","/l*v","`"$outputPath/logs/msi-$($Operation.Trim('/')).log`"") -Wait -PassThru
  if ($process.ExitCode -notin @(0,1641,3010)) { throw "MSI operation failed" }
}

Add-Test "network-isolation" {
  $client=[Net.Sockets.TcpClient]::new()
  try { try { $null=$client.ConnectAsync("1.1.1.1",443).Wait(3000) } catch [AggregateException] { }; if ($client.Connected) { throw "guest egress available" } } finally { $client.Dispose() }
}
Add-Test "clean-install" {
  if ((Test-Path $installed) -or (Test-Path "$directory/local-profile.json") -or (Test-Path "$directory/dev-local-browser-trust.json")) { throw "guest is not clean" }
  if ((Get-FileHash $Msi -Algorithm SHA256).Hash.ToLowerInvariant() -ne $MsiSha256) { throw "MSI hash mismatch" }
  Msi "/i"
  if ((Get-FileHash $installed -Algorithm SHA256).Hash.ToLowerInvariant() -ne $ExpectedExeSha256) { throw "installed EXE hash mismatch" }
  if ((Get-FileHash "$directory/native-host.json" -Algorithm SHA256).Hash.ToLowerInvariant() -ne $ExpectedManifestSha256) { throw "installed manifest hash mismatch" }
  if ((Get-AuthenticodeSignature $installed).Status -ne "NotSigned") { throw "alpha signature policy mismatch" }
  $repair = & $installed --repair-registration | ConvertFrom-Json
  if (@($repair.issues | Where-Object { $_.category -eq "binary-signature" }).Count -ne 0) { throw "alpha startup still blocks unsigned Manual operations" }
  $version = & $installed --version-json | ConvertFrom-Json
  if (-not $version.platformKind) { throw "installed version JSON unavailable" }
  $autostart = (Get-ItemProperty "HKCU:/Software/Microsoft/Windows/CurrentVersion/Run").SniptaleNativeAgent
  if (-not $autostart -or $autostart.Contains("--native-host")) { throw "resident autostart registration invalid" }
  foreach ($browser in @("Google/Chrome","Microsoft/Edge","Chromium")) {
    if (-not (Test-Path "HKCU:/Software/$browser/NativeMessagingHosts/com.sniptale.dev.native_host")) { throw "host registration missing" }
  }
  if (Test-Path "$directory/dev-local-browser-trust.json") { throw "unexpected trust marker" }
}
Add-Test "first-run" {
  Start-Agent
  Tray @("Mode","Manual")
  Start-Sleep -Seconds 2
  [Windows.Forms.SendKeys]::SendWait("%d")
  [Windows.Forms.SendKeys]::SendWait($exports)
  [Windows.Forms.SendKeys]::SendWait("{ENTER}")
  Start-Sleep -Seconds 1
  [Windows.Forms.SendKeys]::SendWait("%s")
  $settings=Window "Sniptale $([char]0x2014) Manual settings"
  Screenshot "first-run-settings"
  Button $settings "Save changes"
  $profile=Wait-For { if (Test-Path "$directory/local-profile.json") { Get-Content "$directory/local-profile.json" -Raw | ConvertFrom-Json } }
  if ($profile.operatingMode -ne "manual" -or $profile.manualProfile.destination.path -ne $exports) { throw "first-run UI did not persist Manual destination" }
  [Windows.Forms.SendKeys]::SendWait("{ESC}")
}
Add-Test "capture" {
  Tray @("Screenshot","Current screen")
  $png=Wait-For { Get-ChildItem $exports -Filter *.png | Select-Object -First 1 }
  $image=[Drawing.Image]::FromFile($png.FullName)
  try { if ($image.Width -lt 100 -or $image.Height -lt 100) { throw "invalid captured PNG" } } finally { $image.Dispose() }
  Copy-Item $png.FullName "$outputPath/screenshots/product.png"
}
Add-Test "recording" {
  Tray @("Video","Start current screen")
  Start-Sleep -Seconds 12
  Tray @("Video","Stop")
  $mp4=Wait-For { Get-ChildItem $exports -Filter *.mp4 | Select-Object -First 1 } 60
  $bytes=[IO.File]::ReadAllBytes($mp4.FullName)
  if ($bytes.Length -lt 4096 -or [Text.Encoding]::ASCII.GetString($bytes,4,4) -ne "ftyp" -or [Text.Encoding]::ASCII.GetString($bytes) -notmatch 'moov') { throw "MP4 did not finalize" }
  $metadata = & ffprobe.exe -v error -show_streams -show_format -of json $mp4.FullName | ConvertFrom-Json
  if ($LASTEXITCODE -ne 0 -or [double]$metadata.format.duration -lt 5 -or @($metadata.streams | Where-Object { $_.codec_type -eq "video" -and $_.width -ge 100 -and $_.height -ge 100 }).Count -eq 0) { throw "recorded MP4 has no valid video stream" }
  & ffmpeg.exe -nostdin -v error -i $mp4.FullName -map 0:v:0 -f null - 2>&1 | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "recorded MP4 cannot be decoded" }
  Copy-Item $mp4.FullName "$outputPath/recordings/product.mp4"
}
Add-Test "restart" {
  $before=(Get-FileHash "$directory/local-profile.json" -Algorithm SHA256).Hash
  Stop-Agent
  Start-Agent
  Tray @("Settings$([char]0x2026)")
  $null=Window "Sniptale $([char]0x2014) Manual settings"
  if ((Get-FileHash "$directory/local-profile.json" -Algorithm SHA256).Hash -ne $before) { throw "restart changed user settings" }
  Screenshot "restarted-settings"
  if (Test-Path "$directory/dev-local-browser-trust.json") { throw "unexpected dev trust after restart" }
}
Add-Test "uninstall" {
  Stop-Agent
  $published=@(Get-ChildItem $exports -File | ForEach-Object { @{ name=$_.Name; hash=(Get-FileHash $_.FullName -Algorithm SHA256).Hash } })
  if ($published.Count -lt 2) { throw "published assets unavailable" }
  Msi "/x"
  if (Test-Path $installed) { throw "executable remains after uninstall" }
  if (Get-ItemProperty "HKCU:/Software/Microsoft/Windows/CurrentVersion/Run" -Name SniptaleNativeAgent -ErrorAction SilentlyContinue) { throw "autostart remains after uninstall" }
  foreach ($name in @("pipe-secret.json","trusted-connections.json","dev-local-browser-trust.json")) {
    if (Test-Path (Join-Path $directory $name)) { throw "private authorization state remains" }
  }
  foreach ($browser in @("Google/Chrome","Microsoft/Edge","Chromium")) {
    if (Test-Path "HKCU:/Software/$browser/NativeMessagingHosts/com.sniptale.dev.native_host") { throw "registration remains" }
  }
  foreach ($asset in $published) { if ((Get-FileHash (Join-Path $exports $asset.name) -Algorithm SHA256).Hash -ne $asset.hash) { throw "published asset changed" } }
}
$agentLogs = Join-Path $directory "logs"
Get-ChildItem $agentLogs -File -ErrorAction SilentlyContinue | ForEach-Object {
  Copy-Item $_.FullName "$outputPath/logs/agent-$($_.Name)" -Force
}
Get-ChildItem "$outputPath/logs" -File | Where-Object { $_.Extension -ne ".jsonl" } | ForEach-Object {
  $text = Get-Content $_.FullName -Raw
  [IO.File]::WriteAllText($_.FullName, $text, [Text.UTF8Encoding]::new($false))
}
$value=[ordered]@{ schema_version=1; lane="windows-11"; tests=@($tests); environment=@{ build=[Environment]::OSVersion.Version.ToString(); devLocalTrust=$false }; artifacts=@() }
[IO.File]::WriteAllText("$outputPath/result.json",($value | ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
if (@($tests | Where-Object { $_.status -ne "pass" }).Count -gt 0) { exit 1 }
