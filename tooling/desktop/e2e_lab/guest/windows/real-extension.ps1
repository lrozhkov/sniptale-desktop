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
    Set-BrowserLocation $window "chrome-extension://elhndangpenapadjimfoodlaenpcndpo/apps/extension/src/settings/index.html?section=native-app&view=connection"
    Start-Sleep -Seconds 3
    $scenarioFactory = Get-Content -Raw -LiteralPath "$PSScriptRoot\real-extension.js"
    $historyProof = Get-Content -Raw -LiteralPath "$PSScriptRoot\native-history-proof.js"
    $scenario = "($scenarioFactory)($historyProof)"
    $reload = Invoke-LabBrowserScript $port ("globalThis.__labAction='reload-runtime';" + $scenario)
    if (-not $reload.passed) { throw "$Name laboratory runtime reload was not scheduled" }
    Start-Sleep -Seconds 3
    $window = Focus-BrowserWindow $Name 15
    Set-BrowserLocation $window "chrome-extension://elhndangpenapadjimfoodlaenpcndpo/apps/extension/src/settings/index.html?section=native-app&view=connection"
    Start-Sleep -Seconds 3
    $connect = Invoke-LabBrowserScript $port ("globalThis.__labAction='connect';" + $scenario)
    if (-not $connect.passed) { throw "$Name real extension connection failed: $($connect.connectionState) $($connect.category) $($connect.rejectedMessage)" }
    Write-ProgressEvent "real-extension-$Name-desktop-selection" "started"
    Select-DesktopBrowserDestination $Name
    $selected = Invoke-LabBrowserScript $port ("globalThis.__labAction='await-selected';" + $scenario)
    if (-not $selected.passed -or -not $selected.selected) { throw "$Name did not receive a selected Desktop media lease and read-only settings" }
    Write-ProgressEvent "real-extension-$Name-desktop-selection" "pass"
    $before = Invoke-LabBrowserScript $port ("globalThis.__labAction='before-capture';" + $scenario)
    if (-not $before.passed) { throw "$Name real extension could not observe its media store" }
    # Exercise the native tray entry. Windows owns bare PrintScreen on this OS;
    # this scenario proves native capture and real browser delivery, not that OS shortcut.
    $profilePath = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent\local-profile.json"
    $localProfile = Get-Content -Raw -LiteralPath $profilePath | ConvertFrom-Json
    $destination = [string]$localProfile.manualProfile.destination.path
    if (-not $destination) { throw "native capture requires its local fallback folder" }
    $priorFiles = @{}
    Get-ChildItem -LiteralPath $destination -Filter *.png | ForEach-Object { $priorFiles[$_.Name] = $true }
    Write-ProgressEvent "real-extension-$Name-native-media" "started"
    Invoke-RealExtensionScreenCapture
    $nativeFile = $null
    $nativeDeadline = [DateTime]::UtcNow.AddSeconds(30)
    while (-not $nativeFile -and [DateTime]::UtcNow -lt $nativeDeadline) {
      $nativeFile = Get-ChildItem -LiteralPath $destination -Filter *.png | Where-Object { -not $priorFiles.ContainsKey($_.Name) } | Sort-Object LastWriteTime | Select-Object -Last 1
      if (-not $nativeFile) { Start-Sleep -Milliseconds 250 }
    }
    if (-not $nativeFile) { throw "native capture did not publish its local fallback" }
    Add-Type -AssemblyName System.Drawing
    $image = [Drawing.Image]::FromFile($nativeFile.FullName)
    try {
      $expected = [ordered]@{ sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $nativeFile.FullName).Hash.ToLowerInvariant(); totalBytes = $nativeFile.Length; width = $image.Width; height = $image.Height }
    } finally { $image.Dispose() }
    $expectedJson = $expected | ConvertTo-Json -Compress
    $null = Invoke-LabBrowserScript $port ("globalThis.__labExpectedMedia=$expectedJson;({passed:true})")
    $media = Invoke-LabBrowserScript $port ("globalThis.__labAction='verify-capture';" + $scenario)
    if (-not $media.passed) { throw "$Name native screenshot was not received and hash-verified by the real extension: $($media.category)" }
    Write-ProgressEvent "real-extension-$Name-native-media" "pass"
    Save-Desktop "real-extension-$Name"
    Write-ProgressEvent "real-extension-$Name-destination-recovery" "started"
    $recovery = Invoke-DeliveryCancellationRecovery -Name $Name -Port $port -Scenario $scenario -Destination $destination
    Write-ProgressEvent "real-extension-$Name-destination-recovery" "pass"
    $recording = Invoke-RealExtensionPointerRecording -Name $Name -Port $port -Scenario $scenario -Destination $destination
    # A fresh product-level reconnect must restore lease and applied settings in the same profile.
    $reconnect = Invoke-LabBrowserScript $port ("globalThis.__labAction='connect';" + $scenario)
    if (-not $reconnect.passed -or -not $reconnect.selected) { throw "$Name product reconnect failed" }
    Restart-AgentWithMode "extension-managed"
    $restart = Invoke-LabBrowserScript $port ("globalThis.__labAction='connect';" + $scenario)
    if (-not $restart.passed -or -not $restart.selected) { throw "$Name reconnect after resident restart failed" }
    return [ordered]@{ browser = $Name; profile_reused = $profileExists; connection = $connect; selected = $selected; capture_trigger = "native-tray-current-screen"; media = $media; recovery = $recovery; recording = $recording; reconnect = $reconnect; resident_restart = $restart }
  } catch {
    $failure = $_
    try { Save-Desktop "real-extension-$Name-failure" } catch { Write-Verbose 'Failure screenshot unavailable; preserving the scenario failure.' }
    throw $failure
  } finally {
    Stop-ProbeBrowser
  }
}

function Invoke-RealExtensionScreenCapture {
  Initialize-SniptaleMenuProbe
  Show-SniptaleTrayMenu
  foreach ($label in @("Screenshot", "Current screen")) {
    Invoke-SniptaleMenuItem $label
    Start-Sleep -Milliseconds 300
  }
}

function Find-DeliveryDialog([int] $Seconds = 20, [ValidateSet("initial-cancel", "explicit-recovery")][string] $Stage = "initial-cancel", [switch] $AllowMissing) {
  $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
  $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ProcessIdProperty, [int]$script:agent.Id)
  $heading = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, "Where should this capture go?")
  while ([DateTime]::UtcNow -lt $deadline) {
    $windows = [System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $condition)
    foreach ($window in $windows) {
      if (-not $window.Current.IsOffscreen -and $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $heading)) { return $window }
    }
    Start-Sleep -Milliseconds 250
  }
  if ($AllowMissing) { return $null }
  throw "native delivery dialog was not presented ($Stage; residentExited=$($script:agent.HasExited))"
}

function Invoke-DeliveryCancellationRecovery([string] $Name, [int] $Port, [string] $Scenario, [string] $Destination) {
  Select-DesktopBrowserDestination $Name $true
  $before = Invoke-LabBrowserScript $Port ("globalThis.__labAction='before-capture';" + $Scenario)
  if (-not $before.passed) { throw "selected lease is required before cancellation proof" }
  $root = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent\temp-media\screenshots"
  $prior = @{}
  Get-ChildItem -LiteralPath $root -Filter *.json | ForEach-Object { $prior[$_.Name] = $true }
  Invoke-RealExtensionScreenCapture
  $dialog = Find-DeliveryDialog -Stage "initial-cancel"
  Save-UiWindow $dialog "delivery-$Name-cancel"
  $records = @(Get-ChildItem -LiteralPath $root -Filter *.json | Where-Object { -not $prior.ContainsKey($_.Name) })
  if ($records.Count -ne 1) { throw "cancellation capture metadata is missing or ambiguous" }
  $metadataPath = $records[0].FullName
  $snapshot = Get-Content -Raw -LiteralPath $metadataPath
  $record = $snapshot | ConvertFrom-Json
  if ([string]$record.captureId -cnotmatch '^[A-Za-z0-9_-]{1,96}$' -or [string]$record.sha256 -cnotmatch '^[0-9a-f]{64}$') { throw "retained fixture identity is not a bounded leaf or digest" }
  if ($record.state -ne "Finalized" -or -not $record.mediaProtection -or -not $record.deliveryRoute.'desktop-delivery'.ask_after_capture) { throw "capture must retain its protected frozen ask route" }
  $protectedPath = Join-Path $root ($record.captureId + ".png")
  $protectedHash = (Get-FileHash -LiteralPath $protectedPath).Hash
  $published = @(Get-ChildItem -LiteralPath $Destination -Filter *.png | Where-Object { $_.Length -eq $record.totalBytes -and (Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant() -eq $record.sha256 })
  if ($published.Count -lt 1) { throw "local fallback was not preserved before cancellation" }
  $dialog.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
  Start-Sleep -Seconds 2
  if ((Get-Content -Raw -LiteralPath $metadataPath) -cne $snapshot -or (Get-FileHash -LiteralPath $protectedPath).Hash -ne $protectedHash) { throw "closing delivery dialog changed retained source" }
  $expected = @{captureId=$record.captureId;sha256=$record.sha256;totalBytes=$record.totalBytes;width=$record.width;height=$record.height} | ConvertTo-Json -Compress
  $null = Invoke-LabBrowserScript $Port ("globalThis.__labExpectedMedia=$expected;({passed:true})")
  $retained = Invoke-LabBrowserScript $Port ("globalThis.__labAction='verify-not-delivered';" + $Scenario)
  if (-not $retained.passed) { throw "cancelled capture was delivered without explicit recovery" }
  Show-SniptaleTrayMenu
  Invoke-SniptaleMenuItem "Recover pending results$([char]0x2026)"
  $dialog = Find-DeliveryDialog -Stage "explicit-recovery"
  Save-UiWindow $dialog "delivery-$Name-recover"
  $registry = Get-Content -Raw -LiteralPath (Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent\browser-targets.json") | ConvertFrom-Json
  $indices = @(for ($i=0; $i -lt @($registry.targets).Count; $i++) { if ($registry.targets[$i].id -eq $registry.defaultTargetId) { $i } })
  if ($indices.Count -ne 1) { throw "explicit recovery browser is missing or ambiguous" }
  $target = $registry.targets[$indices[0]]
  $family = if ($Name -eq "chrome") { "Chrome" } else { "Edge" }
  $label = "$family $([char]0x2014) $($target.profileKey.Substring(0, [Math]::Min(24, $target.profileKey.Length)))"
  $choices = 0
  $deadline = [DateTime]::UtcNow.AddSeconds(60)
  while ([DateTime]::UtcNow -lt $deadline) {
    if ($dialog) {
      if ($choices -ge 32) { throw "explicit recovery exceeded its bounded media batch" }
      Invoke-DeliveryBrowserChoice -Dialog $dialog -Label $label
      $choices++
    }
    $dialog = Find-DeliveryDialog -Seconds 1 -Stage "explicit-recovery" -AllowMissing
    if (-not $dialog -and -not (Test-Path -LiteralPath $metadataPath)) { break }
  }
  if (Test-Path -LiteralPath $metadataPath) { throw "current retained capture did not complete within the recovery batch deadline" }
  $media = Invoke-LabBrowserScript $Port ("globalThis.__labAction='verify-capture';" + $Scenario)
  if (-not $media.passed) { throw "explicit recovery did not deliver exact native bytes" }
  if (Test-Path -LiteralPath $protectedPath) { throw "verified browser receipt did not release protected source" }
  if ((Get-FileHash -LiteralPath $published[0].FullName).Hash.ToLowerInvariant() -ne $record.sha256) { throw "browser receipt altered local fallback" }
  Select-DesktopBrowserDestination $Name
  return @{cancelledRetained=$true;localPreserved=$true;explicitRecovery=$true;sourceReleased=$true;media=$media}
}

function Invoke-DeliveryBrowserChoice([object] $Dialog, [string] $Label) {
  $enabled = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::IsEnabledProperty, $true)
  $links = @($dialog.FindAll([System.Windows.Automation.TreeScope]::Descendants, $enabled) | Where-Object { $_.Current.Name.Trim() -eq $label -and -not $_.Current.IsOffscreen })
  if ($links.Count -eq 0) { throw "explicit browser command link is missing" }
  $link = $links | Sort-Object { $_.Current.BoundingRectangle.Width * $_.Current.BoundingRectangle.Height } -Descending | Select-Object -First 1
  $bounds = $link.Current.BoundingRectangle
  foreach ($candidate in $links) {
    $other = $candidate.Current.BoundingRectangle
    $x = $other.Left + $other.Width / 2
    $y = $other.Top + $other.Height / 2
    if ($x -lt $bounds.Left -or $x -gt $bounds.Right -or $y -lt $bounds.Top -or $y -gt $bounds.Bottom) { throw "explicit browser command link is ambiguous" }
  }
  Click-UiElement $link
}

function Enable-DesktopPointerRecording {
  Add-Type -AssemblyName System.Windows.Forms
  $profilePath = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent\local-profile.json"
  $pointerProfile = Get-Content -Raw -LiteralPath $profilePath | ConvertFrom-Json
  if ($pointerProfile.manualProfile.video.quality -ne "low" -or $pointerProfile.manualProfile.video.fps -ne "fps24") { throw "pointer fixture requires the admitted low/24 FPS native profile" }
  Show-SniptaleTrayMenu
  Invoke-SniptaleMenuItem "Settings$([char]0x2026)"
  $window = Find-DesktopSettingsWindow 15
  $handle = [IntPtr]$window.Current.NativeWindowHandle
  if (-not [SniptaleRealExtensionMenu]::SetForegroundWindow($handle)) { throw "native pointer settings could not be focused" }
  Invoke-DesktopSettingsButton $handle 31
  $changed = $false
  foreach ($entry in @(@{id=10;change=[bool]$pointerProfile.manualProfile.video.includeCursor}, @{id=23;change=(-not [bool]$pointerProfile.manualProfile.video.collectPointerEvents)})) {
    if (-not $entry.change) { continue }
    $bounds = $window.Current.BoundingRectangle
    [SniptaleTrayProbe]::SetCursorPos([int]($bounds.Right - 30), [int]($bounds.Top + $bounds.Height / 2)) | Out-Null
    $control = [SniptaleRealExtensionMenu]::GetDlgItem($handle, $entry.id)
    $reachable = $false
    foreach ($step in 0..12) {
      if ([SniptaleRealExtensionMenu]::CanClickControl($control, $false)) { $reachable = $true; break }
      if ($step -lt 12) { [SniptaleRealExtensionMenu]::WheelDown(); Start-Sleep -Milliseconds 100 }
    }
    if (-not $reachable) { throw "native pointer recording checkbox is not reachable" }
    Invoke-DesktopSettingsButton $handle $entry.id
    $changed = $true
  }
  if ($changed) { Invoke-DesktopSettingsButton $handle 1 }
  $saved = Get-Content -Raw -LiteralPath $profilePath | ConvertFrom-Json
  if (-not $saved.manualProfile.video.collectPointerEvents -or $saved.manualProfile.video.includeCursor) { throw "native pointer opt-in or cursor-free source was not saved" }
  Save-UiWindow $window "native-pointer-settings"
  $window.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
}

function Wait-NativePointerRecordingReady([int] $Seconds = 20) {
  # The temp record precedes runtime start. Manual capture has no browser live-start event.
  $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
  Initialize-SniptaleMenuProbe
  while ([DateTime]::UtcNow -lt $deadline) {
    Show-SniptaleTrayMenu
    Start-Sleep -Milliseconds 100
    try {
      if ([SniptaleRealExtensionMenu]::HasDisabledStatus($script:agent.Id, "Recording")) { return }
    } finally { [System.Windows.Forms.SendKeys]::SendWait("{ESC}") }
    Start-Sleep -Milliseconds 100
  }
  throw "native pointer fixture did not observe its native Recording state"
}

function Invoke-RealExtensionPointerRecording([string] $Name, [int] $Port, [string] $Scenario, [string] $Destination) {
  Enable-DesktopPointerRecording
  $before = Invoke-LabBrowserScript $Port ("globalThis.__labAction='before-capture';" + $Scenario)
  if (-not $before.passed) { throw "native pointer recording requires its selected browser lease" }
  $prior = @{}
  Get-ChildItem -LiteralPath $Destination -Filter *.mp4 | ForEach-Object { $prior[$_.Name]=$true }
  $root = Join-Path $env:LOCALAPPDATA "Sniptale\NativeAgent\temp-media"
  $priorRecords = @{}
  Get-ChildItem -LiteralPath $root -Filter *.json | ForEach-Object { $priorRecords[$_.Name]=$true }
  $fixturePath = Join-Path $outputPath "pointer-fixture.ps1"
  $fixtureBody = @'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$form=[Windows.Forms.Form]::new()
$form.Text='Sniptale pointer fixture'
$form.Width=900; $form.Height=500; $form.StartPosition='CenterScreen'; $form.TopMost=$true
$label=[Windows.Forms.Label]::new(); $label.Dock='Fill'; $label.TextAlign='MiddleCenter'
$label.Font=[Drawing.Font]::new('Segoe UI',24); $label.BackColor=[Drawing.Color]::FromArgb(245,235,220)
$form.Controls.Add($label)
$watch=[Diagnostics.Stopwatch]::StartNew(); $timer=[Windows.Forms.Timer]::new(); $timer.Interval=400
$timer.Add_Tick({$label.Text='Native pointer recording '+[int]$watch.Elapsed.TotalSeconds; if($watch.Elapsed.TotalSeconds -gt 45){$form.Close()}})
$timer.Start()
try {$null=$form.ShowDialog()} finally {$timer.Dispose(); $form.Dispose()}
'@
  [IO.File]::WriteAllText($fixturePath, $fixtureBody, [Text.UTF8Encoding]::new($false))
  $fixture = Start-Process powershell.exe -ArgumentList @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$fixturePath`"") -PassThru
  $started = $false
  try {
    $null = Find-Ui "Sniptale pointer fixture" 15
    Initialize-SniptaleHotkeyInput
    [SniptaleHotkeyInput]::Press(0x76)
    $started = $true
    $record = $null
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    while (-not $record -and [DateTime]::UtcNow -lt $deadline) {
      $record = Get-ChildItem -LiteralPath $root -Filter *.json | Where-Object { -not $priorRecords.ContainsKey($_.Name) } | Select-Object -First 1
      if (-not $record) { Start-Sleep -Milliseconds 250 }
    }
    if (-not $record) { throw "native pointer recording did not start" }
    Wait-NativePointerRecordingReady
    $fixtureWindow = Find-Ui "Sniptale pointer fixture" 5
    if (-not [SniptaleRealExtensionMenu]::SetForegroundWindow([IntPtr]$fixtureWindow.Current.NativeWindowHandle)) { throw "native pointer fixture could not regain focus" }
    Write-ProgressEvent "real-extension-$Name-pointer-recording" "started"
    # The host sends Ctrl+Alt+Shift+F8 after all HID releases. Publication proves completion.
    $nativeFile = $null
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    while (-not $nativeFile -and [DateTime]::UtcNow -lt $deadline) {
      $nativeFile = Get-ChildItem -LiteralPath $Destination -Filter *.mp4 | Where-Object { -not $prior.ContainsKey($_.Name) } | Sort-Object LastWriteTime | Select-Object -Last 1
      if (-not $nativeFile) { Start-Sleep -Milliseconds 250 }
    }
    if (-not $nativeFile) { throw "native pointer recording did not publish its MP4 fallback" }
    $started = $false
    $expected = @{sha256=(Get-FileHash -LiteralPath $nativeFile.FullName).Hash.ToLowerInvariant();totalBytes=$nativeFile.Length} | ConvertTo-Json -Compress
    $null = Invoke-LabBrowserScript $Port ("globalThis.__labExpectedMedia=$expected;({passed:true})")
    $media = Invoke-LabBrowserScript $Port ("globalThis.__labAction='verify-history-recording';" + $Scenario)
    if (-not $media.passed) { throw "native MP4 or cursor telemetry verification failed: $($media.category)" }
    $opened = Invoke-LabBrowserScript $Port ("globalThis.__labAction='open-recording-editor';" + $Scenario)
    if (-not $opened.passed) { throw "verified native recording could not open its editor" }
    $editor = Invoke-LabNativeEditor $Port
    if (-not $editor.passed) { throw "native recording editor proof failed" }
    Copy-Item -LiteralPath $nativeFile.FullName -Destination (Join-Path $outputPath "recordings\native-pointer-$Name.mp4")
    Write-ProgressEvent "real-extension-$Name-pointer-recording" "pass"
    return @{media=$media;editor=$editor}
  } finally {
    if ($started) { [SniptaleHotkeyInput]::Press(0x77) }
    if ($fixture -and -not $fixture.HasExited) { Stop-Process -Id $fixture.Id -Force -ErrorAction SilentlyContinue }
    Remove-Item -LiteralPath $fixturePath -Force -ErrorAction SilentlyContinue
  }
}
