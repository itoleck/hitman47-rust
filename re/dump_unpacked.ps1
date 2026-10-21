# Launches the original Hitman.Exe, waits for the PECompact stub to unpack itself,
# dumps the in-memory image of Hitman.Exe, then terminates the game.
param([string]$Game = 'E:\Games\SteamRoot\Steam\steamapps\common\Hitman Codename 47',
      [string]$Out = "$PSScriptRoot\..\.claude\dump", [int]$Delay = 6)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $Out | Out-Null
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices;
public static class Mem {
  [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr OpenProcess(int a, bool b, int pid);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool ReadProcessMemory(IntPtr h, IntPtr addr, byte[] buf, int size, out int read);
  [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);
}
"@
$p = Start-Process -FilePath "$Game\Hitman.Exe" -WorkingDirectory $Game -PassThru
Start-Sleep -Seconds $Delay
$h = [Mem]::OpenProcess(0x0410, $false, $p.Id)
$size = 0x10000
$buf = New-Object byte[] $size
$all = New-Object System.IO.MemoryStream
for ($off = 0; $off -lt $size; $off += 0x1000) {
  $page = New-Object byte[] 0x1000; $r = 0
  if (-not [Mem]::ReadProcessMemory($h, [IntPtr](0x400000 + $off), $page, 0x1000, [ref]$r)) { $page = New-Object byte[] 0x1000 }
  $all.Write($page, 0, 0x1000)
}
[IO.File]::WriteAllBytes("$Out\Hitman_mem_400000.bin", $all.ToArray())
"modules:" | Out-File "$Out\modules.txt"
try { (Get-Process -Id $p.Id).Modules | ForEach-Object { "{0:X8} {1:X8} {2}" -f $_.BaseAddress.ToInt64(), $_.ModuleMemorySize, $_.FileName } | Out-File -Append "$Out\modules.txt" } catch { "modules failed: $_" | Out-File -Append "$Out\modules.txt" }
[Mem]::CloseHandle($h) | Out-Null
Stop-Process -Id $p.Id -Force
"dumped pid $($p.Id)"
