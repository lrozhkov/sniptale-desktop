param(
  [Parameter(Mandatory = $true)][string] $Runner,
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
  [string] $Cases = "",
  [int] $TimeoutSeconds = 3600,
  [ValidateSet("", "warm")][string] $WorkspaceMode = "",
  [string] $RealExtensionSha256 = "",
  [string] $LabCertificateSha256 = ""
)
$ErrorActionPreference = "Stop"
$runnerPath = [IO.Path]::GetFullPath((Join-Path $PWD $Runner))
$msiPath = [IO.Path]::GetFullPath((Join-Path $PWD $Msi))
$previousMsiPath = [IO.Path]::GetFullPath((Join-Path $PWD $PreviousMsi))
$identityVerifierPath = [IO.Path]::GetFullPath((Join-Path $PWD $IdentityVerifier))
$probePath = [IO.Path]::GetFullPath((Join-Path $PWD $BrowserProbe))
$outputPath = [IO.Path]::GetFullPath((Join-Path $PWD $Output))
$taskLog = [IO.Path]::GetFullPath((Join-Path $PWD "interactive-task.log"))
$entryPath = [IO.Path]::GetFullPath((Join-Path $PWD "interactive-entry.ps1"))
$commandPath = [IO.Path]::GetFullPath((Join-Path $PWD "interactive-entry.cmd"))
$configPath = [IO.Path]::GetFullPath((Join-Path $PWD "interactive-entry.json"))
foreach ($path in @($runnerPath, $msiPath, $previousMsiPath, $identityVerifierPath, $probePath)) {
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "interactive input missing: $([IO.Path]::GetFileName($path))" }
}

# Warm sessions must never accept a previous runner result if this launch fails.
if (Test-Path -LiteralPath $outputPath) { Remove-Item -LiteralPath $outputPath -Recurse -Force -ErrorAction Stop }

$taskName = "SniptaleDesktopE2E"
$identity = "$env:COMPUTERNAME\lab"
$entryConfig = [ordered]@{
  runner = $runnerPath
  parameters = [ordered]@{
    Msi = $msiPath; MsiSha256 = $MsiSha256
    PreviousMsi = $previousMsiPath; PreviousMsiSha256 = $PreviousMsiSha256
    PreviousExeSha256 = $PreviousExeSha256; ExpectedExeSha256 = $ExpectedExeSha256
    ExpectedNativeHostManifestSha256 = $ExpectedNativeHostManifestSha256
    IdentityVerifier = $identityVerifierPath; IdentityVerifierSha256 = $IdentityVerifierSha256
    BrowserProbe = $probePath; BrowserProbeSha256 = $BrowserProbeSha256
    ProbeNonce = $ProbeNonce; Output = $outputPath; Cases = $Cases
  }
}
if ($WorkspaceMode) { $entryConfig.parameters.WorkspaceMode = $WorkspaceMode }
if ($LabCertificateSha256) { $entryConfig.parameters.LabCertificateSha256 = $LabCertificateSha256 }
if ($RealExtensionSha256) { $entryConfig.parameters.RealExtensionSha256 = $RealExtensionSha256 }
$entrySource = @'
param([Parameter(Mandatory = $true)][string] $ConfigPath)
$ErrorActionPreference = "Stop"
$config = Get-Content -Raw -LiteralPath $ConfigPath | ConvertFrom-Json
$parameters = @{}
foreach ($property in $config.parameters.psobject.Properties) {
  $parameters[$property.Name] = [string]$property.Value
}
$configuredRunner = [string]$config.runner
& $configuredRunner @parameters
'@
[IO.File]::WriteAllText($configPath, ($entryConfig | ConvertTo-Json -Depth 4 -Compress), [Text.UTF8Encoding]::new($false))
[IO.File]::WriteAllText($entryPath, $entrySource, [Text.UTF8Encoding]::new($false))
$commandSource = @"
@echo off
"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$entryPath" -ConfigPath "$configPath" > "$taskLog" 2>&1
exit /b %ERRORLEVEL%
"@
[IO.File]::WriteAllText($commandPath, $commandSource, [Text.ASCIIEncoding]::new())
$arguments = "/d /c `"`"$commandPath`"`""
$action = New-ScheduledTaskAction -Execute "C:\Windows\System32\cmd.exe" -Argument $arguments -WorkingDirectory ([IO.Path]::GetDirectoryName($runnerPath))
$principal = New-ScheduledTaskPrincipal -UserId $identity -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Seconds $TimeoutSeconds) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
# Normal CPU, I/O, and memory priority; Task Scheduler defaults to below normal.
$settings.Priority = 4
$task = New-ScheduledTask -Action $action -Principal $principal -Settings $settings

Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $taskLog -Force -ErrorAction SilentlyContinue
Register-ScheduledTask -TaskName $taskName -InputObject $task -Force | Out-Null
$seenRunning = $false
$progressPath = Join-Path $outputPath "logs\progress.log"
$progressLineCount = 0
try {
  $launched = [DateTime]::Now
  Start-ScheduledTask -TaskName $taskName
  $taskStartAttempts = 1
  $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
  while ([DateTime]::UtcNow -lt $deadline) {
    if (Test-Path -LiteralPath $progressPath) {
      $progressLines = @(Get-Content -LiteralPath $progressPath -ErrorAction SilentlyContinue)
      if ($progressLines.Count -gt $progressLineCount) {
        $progressLines[$progressLineCount..($progressLines.Count - 1)] | ForEach-Object { Write-Output "[guest-progress] $_" }
        $progressLineCount = $progressLines.Count
      }
    }
    try {
      $current = Get-ScheduledTask -TaskName $taskName -ErrorAction Stop
      $info = Get-ScheduledTaskInfo -TaskName $taskName -ErrorAction Stop
    } catch {
      $resultPath = Join-Path $outputPath "result.json"
      if (Test-Path -LiteralPath $resultPath) {
        $result = Get-Content -Raw -LiteralPath $resultPath | ConvertFrom-Json
        if (@($result.tests | Where-Object { $_.status -ne "pass" }).Count -gt 0) { exit 1 }
        exit 0
      }
      $entryProcesses = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object {
        $_.CommandLine -and $_.CommandLine.Contains($entryPath)
      })
      if ($entryProcesses.Count -gt 0) {
        $seenRunning = $true
        Start-Sleep -Seconds 2
        continue
      }
      if ($taskStartAttempts -lt 2) {
        $taskStartAttempts++
        Write-Output "[guest-progress] {`"schemaVersion`":1,`"scenario`":`"interactive-launch`",`"status`":`"retry`",`"attempt`":$taskStartAttempts,`"maxAttempts`":2,`"deadlineSeconds`":120}"
        Register-ScheduledTask -TaskName $taskName -InputObject $task -Force | Out-Null
        Start-ScheduledTask -TaskName $taskName
        $launched = [DateTime]::Now
        Start-Sleep -Seconds 2
        continue
      }
      throw "interactive scheduled task disappeared before result after bounded restart"
    }
    if ($current.State -eq "Running") { $seenRunning = $true }
    if ($info.LastRunTime -ge $launched.AddSeconds(-2)) { $seenRunning = $true }
    if (-not $seenRunning -and [DateTime]::Now -gt $launched.AddSeconds(120)) {
      throw "interactive task did not acquire the local console token"
    }
    if ($seenRunning -and $current.State -ne "Running") {
      $resultPath = Join-Path $outputPath "result.json"
      if (-not (Test-Path -LiteralPath $resultPath)) {
        New-Item -ItemType Directory -Force -Path (Join-Path $outputPath "logs") | Out-Null
        $detail = "task log unavailable"
        if (Test-Path -LiteralPath $taskLog) {
          Copy-Item -LiteralPath $taskLog -Destination (Join-Path $outputPath "logs\interactive-task.log") -Force
          $detail = (Get-Content -LiteralPath $taskLog -Tail 40 | Out-String).Trim()
        }
        throw "interactive runner returned $($info.LastTaskResult) without result.json: $detail"
      }
      $result = Get-Content -Raw -LiteralPath $resultPath | ConvertFrom-Json
      if (@($result.tests | Where-Object { $_.status -ne "pass" }).Count -gt 0) { exit 1 }
      if ($info.LastTaskResult -ne 0) { throw "interactive runner returned $($info.LastTaskResult)" }
      exit 0
    }
    Start-Sleep -Seconds 2
  }
  throw "interactive runner timed out after $TimeoutSeconds seconds"
} finally {
  Stop-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
  Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $taskLog -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $entryPath, $commandPath, $configPath -Force -ErrorAction SilentlyContinue
}
