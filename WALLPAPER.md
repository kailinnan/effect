# Clouds Dynamic Wallpaper

This repository uses `static/clouds/index.html` as the dynamic wallpaper source.
`cargo run` creates a borderless WebView window, loads the clouds page, makes the window a WorkerW child, and places it behind the desktop icon layer.

## Apply the Wallpaper

Start from the repository root:

```powershell
cargo run -- start
```

Stop and refresh the desktop:

```powershell
cargo run -- stop
```

The stop command terminates the wallpaper host, advances the Windows wallpaper slideshow, and sends desktop refresh messages to Explorer.

Check status:

```powershell
cargo run -- status
```

## Preview Without Lively

```powershell
python -m http.server 8000
```

Open:

```text
http://localhost:8000/static/clouds/
```

## Notes

- `static/clouds/index.html` remains the source of the visual effect.
- `static/clouds/css/style.css` hides the dat.GUI debug panel for wallpaper use.
- `scripts/package-lively.ps1` is still available if you also want a manual Lively import zip.
