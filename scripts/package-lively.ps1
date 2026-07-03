param(
    [string]$Project = "clouds"
)

$ErrorActionPreference = "Stop"

if ($Project -match '[\\/]|\.\.' -or [string]::IsNullOrWhiteSpace($Project)) {
    throw "Invalid static project name: $Project"
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$staticDir = Join-Path $repoRoot "static"
$wallpaperDir = Join-Path $staticDir $Project
$distDir = Join-Path $repoRoot "dist"
$zipPath = Join-Path $distDir "$Project-lively.zip"

if (-not (Test-Path -LiteralPath $wallpaperDir -PathType Container)) {
    throw "Missing static project directory: static\$Project"
}

if (-not (Test-Path -LiteralPath (Join-Path $wallpaperDir "index.html") -PathType Leaf)) {
    throw "Missing required wallpaper file: static\$Project\index.html"
}

New-Item -ItemType Directory -Force -Path $distDir | Out-Null
if (Test-Path -LiteralPath $zipPath) {
    Remove-Item -LiteralPath $zipPath -Force
}

$tempDir = Join-Path $env:TEMP "effect-lively-$Project"
if (Test-Path -LiteralPath $tempDir) {
    Remove-Item -LiteralPath $tempDir -Recurse -Force
}

New-Item -ItemType Directory -Force -Path $tempDir | Out-Null
Copy-Item -Path (Join-Path $wallpaperDir "*") -Destination $tempDir -Recurse -Force

$livelyInfoPath = Join-Path $tempDir "LivelyInfo.json"
if (-not (Test-Path -LiteralPath $livelyInfoPath -PathType Leaf)) {
    $title = (Get-Culture).TextInfo.ToTitleCase($Project.Replace("-", " ").Replace("_", " "))
    $livelyInfo = [ordered]@{
        AppVersion = "1.0.0.0"
        Title = $title
        Thumbnail = $null
        Preview = $null
        Desc = "Static HTML wallpaper from static/$Project."
        Author = "effect"
        License = $null
        Contact = $null
        Type = 1
        FileName = "index.html"
        Arguments = $null
        IsAbsolutePath = $false
    }
    $livelyInfo | ConvertTo-Json | Set-Content -LiteralPath $livelyInfoPath -Encoding UTF8
}

Compress-Archive -Path (Join-Path $tempDir "*") -DestinationPath $zipPath -Force
Remove-Item -LiteralPath $tempDir -Recurse -Force

Write-Host "Created Lively wallpaper package:"
Write-Host "  $zipPath"
Write-Host ""
Write-Host "Import this zip in Lively Wallpaper."
