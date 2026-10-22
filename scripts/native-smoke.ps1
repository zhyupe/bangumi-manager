param([string]$Executable = '.\src-tauri\target\release\bangumi-manager.exe', [switch]$LiveBangumi)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
if (Get-Process -Name bangumi-manager -ErrorAction SilentlyContinue) { throw 'Close the existing desktop instance before running the isolated smoke test.' }
if (Get-NetTCPConnection -LocalPort 9223 -State Listen -ErrorAction SilentlyContinue) {
    throw 'Test port 9223 is already in use.'
}
$nativeTestRoot = Join-Path (Get-Location).Path ('.test-data\native-smoke-' + [guid]::NewGuid().ToString('N'))
$previousDataDir = $env:BANGUMI_MANAGER_DATA_DIR
$previousWebViewArgs = $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
$previousNoProxy = $env:NO_PROXY
$nativeProcess = $null
try {
    $env:BANGUMI_MANAGER_DATA_DIR = Join-Path $nativeTestRoot 'state'
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9223'
    $env:NO_PROXY = 'localhost,127.0.0.1'
    $nativeProcess = Start-Process -FilePath $Executable -WindowStyle Hidden -PassThru
    for ($attempt = 0; $attempt -lt 60; $attempt++) {
        if ($nativeProcess.HasExited) { throw 'Desktop process exited before the test.' }
        if (Get-NetTCPConnection -LocalPort 9223 -State Listen -ErrorAction SilentlyContinue) { break }
        Start-Sleep -Milliseconds 250
    }
    if ($LiveBangumi) { node scripts/native-smoke.mjs --live } else { node scripts/native-smoke.mjs }
    if ($LASTEXITCODE -ne 0) { throw "Native smoke test failed: $LASTEXITCODE" }
} finally {
    if ($nativeProcess -and -not $nativeProcess.HasExited) { Stop-Process -Id $nativeProcess.Id }
    $env:BANGUMI_MANAGER_DATA_DIR = $previousDataDir
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = $previousWebViewArgs
    $env:NO_PROXY = $previousNoProxy
}
