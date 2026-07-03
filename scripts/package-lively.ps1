$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$wallpaperDir = Join-Path $repoRoot "static\clouds"
$distDir = Join-Path $repoRoot "dist"
$zipPath = Join-Path $distDir "clouds-lively.zip"

$requiredFiles = @(
    "index.html",
    "LivelyInfo.json",
    "css\normalize.min.css",
    "css\style.css",
    "js\three.min.js",
    "js\OrbitControls.js",
    "js\dat.gui.min.js",
    "js\script.js"
)

foreach ($file in $requiredFiles) {
    $path = Join-Path $wallpaperDir $file
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Missing required wallpaper file: $file"
    }
}

New-Item -ItemType Directory -Force -Path $distDir | Out-Null
if (Test-Path -LiteralPath $zipPath) {
    Remove-Item -LiteralPath $zipPath -Force
}

Compress-Archive -Path (Join-Path $wallpaperDir "*") -DestinationPath $zipPath -Force

Write-Host "Created Lively wallpaper package:"
Write-Host "  $zipPath"
Write-Host ""
Write-Host "Import this zip in Lively Wallpaper."
