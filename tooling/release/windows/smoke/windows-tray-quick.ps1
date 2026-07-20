param(
  [string] $Repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..")).Path,
  [ValidateSet("stable", "beta", "dev")]
  [string] $Channel = "stable",
  [string] $ExtensionOrigin = "chrome-extension://pcegkgbagkiagakmfjchgfgheepkllda/",
  [switch] $DevLocalTrust
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Push-RepoLocation($Path) {
  if ($Path -notlike "\\*") {
    Push-Location $Path
    return $null
  }

  $driveName = "SX" + ([Guid]::NewGuid().ToString("N").Substring(0, 8))
  New-PSDrive -Name $driveName -PSProvider FileSystem -Root $Path -Scope Script | Out-Null
  Push-Location "${driveName}:\"
  return $driveName
}

function Import-VsDevEnvironment {
  $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
  if (-not (Test-Path -LiteralPath $vswhere)) {
    throw "Visual Studio Build Tools with C++ tools and Windows SDK were not found."
  }
  $installPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 Microsoft.VisualStudio.Component.Windows10SDK -property installationPath
  $vsDevCmd = Join-Path $installPath "Common7\Tools\VsDevCmd.bat"
  if (-not (Test-Path -LiteralPath $vsDevCmd)) {
    throw "VsDevCmd.bat not found: $vsDevCmd"
  }

  $previousLocation = Get-Location
  try {
    Set-Location $env:SystemRoot
    $environment = & cmd.exe /s /c "`"$vsDevCmd`" -arch=x64 -host_arch=x64 >nul && set"
  }
  finally {
    Set-Location $previousLocation
  }
  foreach ($line in $environment) {
    if ($line -match '^([^=]+)=(.*)$') {
      [Environment]::SetEnvironmentVariable($matches[1], $matches[2], "Process")
    }
  }
}

if ($ExtensionOrigin -notmatch '^chrome-extension://[a-p]{32}/$') {
  throw "Extension origin must be exact chrome-extension://<32-letter-id>/: $ExtensionOrigin"
}

$mappedDrive = Push-RepoLocation $Repo
try {
  Import-VsDevEnvironment
  Get-Process -Name "sniptale-native-agent" -ErrorAction SilentlyContinue |
    Stop-Process -Force

  $env:SNIPTALE_INSTALL_CHANNEL = $Channel
  $env:SNIPTALE_ALLOWED_EXTENSION_ORIGIN = $ExtensionOrigin
  $env:SNIPTALE_WINDOWS_RESOURCES = "require"
  if ($DevLocalTrust) {
    $env:SNIPTALE_DEV_LOCAL_BROWSER_TRUST = "1"
  }
  else {
    Remove-Item Env:\SNIPTALE_DEV_LOCAL_BROWSER_TRUST -ErrorAction SilentlyContinue
  }

  cargo build --release --target x86_64-pc-windows-msvc --bin sniptale-native-agent --all-features
  if ($LASTEXITCODE -ne 0) {
    throw "Windows release build failed with exit code $LASTEXITCODE"
  }

  $exe = Join-Path (Get-Location) "target\x86_64-pc-windows-msvc\release\sniptale-native-agent.exe"
  Start-Process -FilePath $exe
  Write-Host "Started tray agent from $exe"
}
finally {
  Pop-Location
  if ($mappedDrive) {
    Remove-PSDrive -Name $mappedDrive -Force
  }
}
