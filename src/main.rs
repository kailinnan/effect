#[cfg(target_os = "windows")]
mod config;
#[cfg(target_os = "windows")]
mod desktop;
#[cfg(target_os = "windows")]
mod ui;
#[cfg(target_os = "windows")]
mod wallpaper;

#[cfg(target_os = "windows")]
fn main() {
    eprintln!("effect: entering ui");
    if let Err(error) = ui::run() {
        eprintln!("effect: ui error: {error}");
        native_windows_gui::fatal_message("Effect 启动失败", &error.to_string());
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("Effect wallpaper manager only supports Windows.");
}
