# Boots missions/screens via the engine's DefaultScene key with optional
# .ini overrides, and captures screenshots (see test-run.ps1).
#   -Scenes  'C1_HongKong\C1_1.zip,OptionsScreen.zip'   (comma separated)
#   -Set     'Window 1;Resolution 1920x1080'            (ini lines, ';' separated, replace same keys)
#   -Prefix  '-d3d'                                     (launcher options before the @ini argument)
param(
    [string]$Game = 'E:\Games\SteamRoot\Steam\steamapps\common\Hitman Codename 47',
    [int]$Seconds = 45,
    # Comma-separated list (powershell -File cannot pass arrays).
    [string]$Scenes = 'C1_HongKong\C1_1.zip,C2_RotterdamHarbor\C2_1.zip,C3_BudapestHotel\C3_1.zip,C4_ColombianRainForest\C4_1.zip,C5_Sanitarium\C5_1.zip',
    [string]$Set = '',
    [string]$Prefix = '',
    [string]$TagSuffix = ''
)
$testDir = Join-Path $Game 'HitManVK\test'
New-Item -ItemType Directory -Force -Path $testDir | Out-Null
$overrides = @($Set -split ';' | ForEach-Object { $_.Trim() } | Where-Object { $_ })
$keys = $overrides | ForEach-Object { ($_ -split '\s+')[0] }
$base = Get-Content (Join-Path $Game 'HitMan-Rust.ini') | Where-Object {
    $k = ($_.Trim() -split '\s+')[0]
    ($k -ne 'DefaultScene') -and -not ($keys -contains $k)
}
$first = $true
foreach ($scene in ($Scenes -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ })) {
    $tag = [IO.Path]::GetFileNameWithoutExtension($scene) + $TagSuffix
    $ini = Join-Path $testDir "$tag.ini"
    ($base + $overrides + "DefaultScene $scene") | Set-Content -Path $ini -Encoding ASCII
    $gameArgs = if ($Prefix) { "$Prefix @$ini" } else { "@$ini" }
    $args = @{ Game = $Game; Seconds = $Seconds; Interval = 9; GameArgs = $gameArgs; Tag = $tag }
    if (-not $first) { $args.NoDeploy = $true }
    & (Join-Path $PSScriptRoot 'test-run.ps1') @args
    $first = $false
}
