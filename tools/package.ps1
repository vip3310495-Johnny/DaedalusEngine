# Builds the release application and the per-user installer:
#   dist\DaedalusEngine-Setup-V<version>.exe  (+ SHA256SUMS.txt)
# usage: powershell -ExecutionPolicy Bypass -File tools\package.ps1
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

$m = Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
if (-not $m) { throw 'workspace version not found in Cargo.toml' }
$ver = $m.Matches[0].Groups[1].Value

Write-Host "== DaedalusEngine V${ver}: building application (release) =="
cargo build --release -p daedalus-app -p daedalus-cli
if ($LASTEXITCODE -ne 0) { throw 'application build failed' }

# separate invocation: the installer's build script packs the finished application binaries
Write-Host '== building installer (release) =='
cargo build --release -p daedalus-installer
if ($LASTEXITCODE -ne 0) { throw 'installer build failed' }

$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
$out = Join-Path $dist "DaedalusEngine-Setup-V${ver}.exe"
Copy-Item (Join-Path $root 'target\release\DaedalusEngine-Setup.exe') $out -Force
$h = Get-FileHash -Algorithm SHA256 $out
"$($h.Hash.ToLower())  $(Split-Path $out -Leaf)" | Out-File -Encoding ascii (Join-Path $dist 'SHA256SUMS.txt')
$mb = [math]::Round((Get-Item $out).Length / 1MB, 2)
Write-Host "== done: $out ($mb MB) =="
