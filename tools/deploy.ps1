<#
.SYNOPSIS
    Copies the built files from dist\ into the Hitman: Codename 47 folder.
.DESCRIPTION
    Always copies HitMan-Rust.exe and HitManVK\ddraw.dll. HitMan-Rust.ini and
    HitManVK\hm1vk.ini are copied only if they are not already in the game
    folder, so local settings are kept.
.PARAMETER Game
    The game folder (the one containing System.dll).
.EXAMPLE
    powershell -ExecutionPolicy Bypass -File tools\deploy.ps1
.EXAMPLE
    powershell -ExecutionPolicy Bypass -File tools\deploy.ps1 -Game "D:\Games\Hitman Codename 47"
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [string]$Game = 'E:\Games\SteamRoot\Steam\steamapps\common\Hitman Codename 47'
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$dist = Join-Path $root 'dist'

if (-not (Test-Path -LiteralPath (Join-Path $Game 'System.dll'))) {
    [Console]::Error.WriteLine("Game folder not found: $Game")
    exit 1
}
$vk = Join-Path $Game 'HitManVK'
New-Item -ItemType Directory -Force -Path $vk | Out-Null

try {
    Copy-Item -LiteralPath (Join-Path $dist 'HitMan-Rust.exe') -Destination $Game -Force
    Copy-Item -LiteralPath (Join-Path $dist 'HitManVK\ddraw.dll') -Destination $vk -Force
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}

# Config files: only if missing, so the user's settings are not overwritten.
foreach ($f in 'HitMan-Rust.ini', 'HitManVK\hm1vk.ini') {
    $dst = Join-Path $Game $f
    if (-not (Test-Path -LiteralPath $dst)) {
        Copy-Item -LiteralPath (Join-Path $dist $f) -Destination $dst -ErrorAction Continue
    }
}
"Deployed to $Game"
exit 0
