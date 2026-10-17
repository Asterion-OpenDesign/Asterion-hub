<#
.SYNOPSIS
  Builds two versions of the Hub test MSI (1.0.0 and 1.1.0) into .dev\fixtures. Needs WiX 5 (wix.exe).
#>
param([string]$Wix = 'wix.exe', [string]$Out = (Join-Path $PSScriptRoot '..\..\.dev\fixtures'))
$ErrorActionPreference = 'Stop'
$payload = Join-Path $Out 'payload'
New-Item -ItemType Directory -Force $payload | Out-Null
# A small executable to act as the "application" (copied from the system for the local test only; never committed).
Copy-Item (Join-Path $env:SystemRoot 'System32\ping.exe') (Join-Path $payload 'testapp.exe') -Force
Set-Content (Join-Path $payload 'readme.txt') 'ASTERION Hub test app' -Encoding ascii
foreach ($v in '1.0.0', '1.1.0') {
    & $Wix build -arch x64 -d "Version=$v" -d "Payload=$payload" -o (Join-Path $Out "testapp-$v.msi") (Join-Path $PSScriptRoot 'testapp.wxs')
    if ($LASTEXITCODE -ne 0) { throw "wix failed for $v" }
}
Get-ChildItem $Out -Filter *.msi | Select-Object Name, Length