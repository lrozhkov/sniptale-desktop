# Playwright owns CDP framing, target discovery, evaluation, and connection timeouts.
function Invoke-LabBrowserScript([int] $Port, [string] $Expression) {
  if ($Port -notin @(9223, 9224) -or [Text.Encoding]::UTF8.GetByteCount($Expression) -gt 60000) { throw "invalid browser scenario input" }
  Invoke-LabBrowserRequest -Port $Port -Payload @{expression=$Expression}
}

function Invoke-LabNativeEditor([int] $Port) {
  Invoke-LabBrowserRequest -Port $Port -Payload @{scenario="native-video-editor"}
}

function Invoke-LabBrowserRequest([int] $Port, [hashtable] $Payload) {
  if ($Port -notin @(9223, 9224)) { throw "invalid browser scenario port" }
  $request = Join-Path $outputPath "playwright-request.json"
  try {
    [IO.File]::WriteAllText($request, ($Payload | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
    $result = & "C:\SniptaleLab\browser-tools\node.exe" "$PSScriptRoot\browser-playwright.cjs" $Port $request
    if ($LASTEXITCODE -ne 0) { throw "Playwright browser scenario failed" }
    return ($result | ConvertFrom-Json)
  } finally { Remove-Item -LiteralPath $request -Force -ErrorAction SilentlyContinue }
}
