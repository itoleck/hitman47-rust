# Test harness: deploys dist\ to the game folder, starts HitMan-Rust.exe,
# captures screenshots at intervals, then closes the game and collects logs.
param(
    [string]$Game = 'E:\Games\SteamRoot\Steam\steamapps\common\Hitman Codename 47',
    [int]$Seconds = 40,
    [int]$Interval = 5,
    [string]$GameArgs = '',
    [switch]$KeepRunning,
    [string]$Exe = 'HitMan-Rust.exe',
    [switch]$NoDeploy,
    [string]$Tag = ''
)
$ErrorActionPreference = 'Continue'
$root = Split-Path $PSScriptRoot -Parent
$out = Join-Path $root '.claude\shots'
if ($Tag) { $out = Join-Path $out $Tag }
New-Item -ItemType Directory -Force -Path $out | Out-Null
Get-ChildItem $out -Filter *.png -ErrorAction SilentlyContinue | Remove-Item -Force

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Dpi { [DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); }
"@
[Dpi]::SetProcessDPIAware() | Out-Null
Add-Type -AssemblyName System.Windows.Forms

function Shot($name) {
    $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
    # Save a half-size copy to keep files small.
    $small = New-Object System.Drawing.Bitmap $bmp, ([int]($b.Width / 2)), ([int]($b.Height / 2))
    $small.Save((Join-Path $out "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose(); $small.Dispose()
}

# The engine allows one instance (mutex "Hitboy"): make sure earlier runs are gone.
Get-Process HitMan-Rust, Hitman -ErrorAction SilentlyContinue | ForEach-Object { $_.Kill(); $_.WaitForExit(15000) | Out-Null }
Start-Sleep -Seconds 2
if (-not $NoDeploy) { & (Join-Path $PSScriptRoot 'deploy.bat') $Game }
if ($GameArgs) {
    $p = Start-Process -FilePath (Join-Path $Game $Exe) -WorkingDirectory $Game -ArgumentList $GameArgs -PassThru
} else {
    $p = Start-Process -FilePath (Join-Path $Game $Exe) -WorkingDirectory $Game -PassThru
}
"started pid $($p.Id)"
$t = 0
while ($t -lt $Seconds) {
    Start-Sleep -Seconds $Interval
    $t += $Interval
    if ($p.HasExited) { "game exited with code $($p.ExitCode) after ${t}s"; break }
    Shot ("t{0:D3}" -f $t)
}
if (-not $p.HasExited -and -not $KeepRunning) { $p.Kill(); $p.WaitForExit(15000) | Out-Null; Start-Sleep -Seconds 2; "killed after ${t}s" }
$logName = if ($Tag) { "hm1vk-$Tag.log" } else { 'hm1vk.log' }
Copy-Item (Join-Path $Game 'HitManVK\hm1vk.log') (Join-Path $root ".claude\logs\$logName") -Force -ErrorAction SilentlyContinue
"done"
