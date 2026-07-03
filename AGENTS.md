# Repository Guidelines

## Project Structure & Module Organization

This repository packages static WebGL effects as desktop wallpapers.

- `src/main.rs` hosts `static/clouds/index.html` in a borderless WebView and attaches that window behind the desktop icon layer with WorkerW/Progman APIs.
- `Cargo.toml` and `Cargo.lock` define the package metadata and locked dependency graph.
- `static/clook/index.html` contains the `clook` static page.
- `static/clouds/` contains the clouds Three.js/WebGL wallpaper, including `index.html`, `LivelyInfo.json`, CSS, and JavaScript.
- `scripts/package-lively.ps1` builds `dist/clouds-lively.zip` for Lively Wallpaper.
- `target/` is Cargo build output and should not be edited or committed.

Keep Rust code under `src/`. Place browser-facing assets under `static/<demo-name>/`, grouping styles in `css/` and scripts in `js/` when a demo has multiple files.

## Build, Test, and Development Commands

- `cargo build` compiles the Rust binary in debug mode.
- `cargo run` starts the WebView wallpaper host and keeps it running.
- `powershell -ExecutionPolicy Bypass -File scripts\package-lively.ps1` creates `dist/clouds-lively.zip`.
- `cargo test` runs Rust unit and integration tests.
- `cargo fmt` formats Rust source using `rustfmt`.
- `cargo clippy -- -D warnings` runs lint checks and treats warnings as failures.

Preview the clouds wallpaper through a local server: `python -m http.server 8000`, then open `http://localhost:8000/static/clouds/`.

## Coding Style & Naming Conventions

Use standard Rust 2024 style: four-space indentation, `snake_case` for functions and modules, `PascalCase` for types, and `SCREAMING_SNAKE_CASE` for constants. Run `cargo fmt` before submitting changes.

For static assets, use lowercase directory and file names. Keep third-party/minified files clearly identifiable, such as `three.min.js`. Keep `LivelyInfo.json` at the root of `static/clouds` so it lands at the zip root.

## Testing Guidelines

There are no tests yet. Add Rust unit tests next to the code they cover using `#[cfg(test)] mod tests`, and add integration tests under `tests/` when testing public behavior. Name tests after the expected behavior, for example `prints_default_message` or `loads_cloud_scene_config`.

Run `cargo test` before opening a pull request. For wallpaper changes, verify `cargo run`; stop the host with `Stop-Process -Name effect`.

## Commit & Pull Request Guidelines

This repository currently has no commit history, so no local convention is established. Use short, imperative commit subjects such as `Add cloud scene controls` or `Refactor main entry point`.

Pull requests should include a concise description, the commands run for validation, and screenshots or screen recordings for visible changes under `static/`. Link related issues when available and call out any added third-party assets or licensing considerations.
