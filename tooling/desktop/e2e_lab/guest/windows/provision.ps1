$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$labRoot = "C:\SniptaleLab"
New-Item -ItemType Directory -Force -Path $labRoot | Out-Null
Set-Content -LiteralPath (Join-Path $labRoot "provisioning") -Value "started" -Encoding ascii
function Set-ProvisionStage([string]$Name) {
  Set-Content -LiteralPath (Join-Path $labRoot "provisioning") -Value $Name -Encoding ascii
}
trap {
  Set-Content -LiteralPath (Join-Path $labRoot "provision-failed") -Value $_.Exception.Message -Encoding utf8
  throw
}
Remove-Item -LiteralPath (Join-Path $labRoot "provision-failed") -Force -ErrorAction SilentlyContinue
Set-ProvisionStage "desktop"
Set-LocalUser -Name lab -PasswordNeverExpires $true
powercfg /change monitor-timeout-ac 0
powercfg /change standby-timeout-ac 0
powercfg /change hibernate-timeout-ac 0
powercfg /hibernate off
$desktop = "HKCU:\Control Panel\Desktop"
New-ItemProperty -Path $desktop -Name Win8DpiScaling -PropertyType DWord -Value 1 -Force | Out-Null
New-ItemProperty -Path $desktop -Name LogPixels -PropertyType DWord -Value 120 -Force | Out-Null

Set-ProvisionStage "openssh"
Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0 | Out-Null
Set-Service sshd -StartupType Automatic
Start-Service sshd
if (-not (Get-NetFirewallRule -Name "SniptaleLabSsh" -ErrorAction SilentlyContinue)) {
  New-NetFirewallRule -Name "SniptaleLabSsh" -DisplayName "Sniptale Lab SSH" -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22 -Profile Any | Out-Null
}
$sshDir = "C:\ProgramData\ssh"
New-Item -ItemType Directory -Force -Path $sshDir | Out-Null
$authorized = Join-Path $sshDir "administrators_authorized_keys"
Set-Content -LiteralPath $authorized -Value "{{SSH_PUBLIC_KEY}}" -Encoding ascii
icacls.exe $authorized /inheritance:r /grant "Administrators:F" /grant "SYSTEM:F" | Out-Null

$adapter = Get-NetAdapter | Where-Object { ($_.MacAddress -replace "-", ":") -eq "52:54:00:12:34:56" } | Select-Object -First 1
if (-not $adapter) { throw "Sniptale lab network adapter was not found" }
Set-DnsClientServerAddress -InterfaceIndex $adapter.ifIndex -ServerAddresses @("1.1.1.1", "8.8.8.8")

if (-not (Get-Command winget.exe -ErrorAction SilentlyContinue)) {
  throw "Windows Enterprise image does not provide winget; use the current official Evaluation ISO"
}
$packages = @(
  @("Microsoft.VisualStudio.2022.BuildTools", "--override", "--wait --quiet --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"),
  @("Microsoft.DotNet.SDK.8", ""),
  @("Rustlang.Rustup", ""),
  @("Google.Chrome", ""),
  @("Microsoft.Edge", ""),
  @("Gyan.FFmpeg", "")
)
foreach ($entry in $packages) {
  Set-ProvisionStage "winget-$($entry[0])"
  $installed = (& winget.exe list --source winget --id $entry[0] --exact --disable-interactivity | Out-String)
  if ($LASTEXITCODE -eq 0 -and $installed -match [regex]::Escape($entry[0])) {
    continue
  }
  $arguments = @("install", "--source", "winget", "--id", $entry[0], "--exact", "--silent", "--accept-package-agreements", "--accept-source-agreements", "--disable-interactivity")
  if ($entry.Count -eq 3) { $arguments += @($entry[1], $entry[2]) }
  $installedOk = $false
  for ($attempt = 1; $attempt -le 3; $attempt++) {
    & winget.exe @arguments
    if ($LASTEXITCODE -eq 0) {
      $installedOk = $true
      break
    }
    if ($attempt -lt 3) { Start-Sleep -Seconds 10 }
  }
  if (-not $installedOk) { throw "winget package failed after retries: $($entry[0]) exit=$LASTEXITCODE" }
}
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:USERPROFILE\.dotnet\tools;$env:ProgramFiles\dotnet;$env:Path"
Set-ProvisionStage "rust-toolchain"
& rustup.exe toolchain install stable --profile minimal
if ($LASTEXITCODE -ne 0) { throw "rustup toolchain install failed" }
& rustup.exe target add x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { throw "rustup target install failed" }
Set-ProvisionStage "wix"
& dotnet.exe tool install --global wix --version 4.*
if ($LASTEXITCODE -ne 0) { throw "WiX installation failed" }
Set-NetIPInterface -InterfaceIndex $adapter.ifIndex -Dhcp Disabled
Get-NetIPAddress -InterfaceIndex $adapter.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue |
  Remove-NetIPAddress -Confirm:$false -ErrorAction SilentlyContinue
New-NetIPAddress -InterfaceIndex $adapter.ifIndex -IPAddress "10.0.2.15" -PrefixLength 24 -DefaultGateway "10.0.2.2" | Out-Null
Set-DnsClientServerAddress -InterfaceIndex $adapter.ifIndex -ServerAddresses "10.0.2.3"
Set-ProvisionStage "complete"
Set-Content -LiteralPath (Join-Path $labRoot "provisioned") -Value "windows-11" -Encoding ascii
Remove-Item -LiteralPath (Join-Path $labRoot "provisioning") -Force
Remove-Item -LiteralPath (Join-Path $labRoot "provision-failed") -Force -ErrorAction SilentlyContinue
shutdown.exe /s /t 30 /f
