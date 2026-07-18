# Effect Dynamic Wallpaper

Effect 是一个 Windows 原生动态壁纸管理器。它可以选择本地 HTML 文件，然后通过
WebView2 将页面挂载到桌面图标层后方。

## 使用

从仓库根目录启动：

dist/effect.exe

```powershell
cargo run
```

管理窗口提供三个操作：

- `选择 HTML 文件`：选择壁纸入口。
- `应用`：记录当前 Windows 静态壁纸并启动 HTML 动态壁纸。
- `停止并恢复`：关闭动态壁纸并恢复应用前的静态壁纸。

停止动态壁纸不会关闭管理窗口，可以继续选择其他 HTML 并再次应用。动态壁纸运行时
选择新的 HTML 再点击应用，会直接切换到新的效果。

关闭管理窗口也会执行停止和恢复操作。

上次选择的路径与静态壁纸备份保存在：

```text
%APPDATA%\effect\config.json
%APPDATA%\effect\wallpaper-backup.json
```

静态示例仍存放在 `static/<demo-name>/`。例如可选择 `static/clouds` 目录。

## 开发检查

```powershell
cargo fmt -- --check
cargo clippy -- -D warnings
cargo test
```

## Lively Wallpaper 打包

独立的 Lively Wallpaper 包仍可通过脚本生成：

```powershell
powershell -ExecutionPolicy Bypass -File scripts\package-lively.ps1 -Project clouds
```
