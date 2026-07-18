use std::{
    cell::RefCell,
    error::Error,
    path::{Path, PathBuf},
    rc::Rc,
};

use native_windows_gui as nwg;

use crate::{
    config::{self, AppConfig},
    desktop,
    wallpaper::WallpaperRuntime,
};

struct AppState {
    config: AppConfig,
    runtime: Option<WallpaperRuntime>,
}

pub fn run() -> Result<(), Box<dyn Error>> {
    nwg::init()?;
    nwg::Font::set_global_family("Microsoft YaHei UI")?;

    let config = config::load();
    let initial_path = config
        .selected_path
        .as_deref()
        .map(path_text)
        .unwrap_or_default();

    let mut window = nwg::Window::default();
    let mut title = nwg::Label::default();
    let mut subtitle = nwg::Label::default();
    let mut path_label = nwg::Label::default();
    let mut path_input = nwg::TextInput::default();
    let mut choose_file = nwg::Button::default();
    let mut choose_folder = nwg::Button::default();
    let mut apply = nwg::Button::default();
    let mut stop = nwg::Button::default();
    let mut status = nwg::Label::default();
    let mut file_dialog = nwg::FileDialog::default();
    let mut folder_dialog = nwg::FileDialog::default();
    let mut title_font = nwg::Font::default();
    let mut status_font = nwg::Font::default();

    nwg::Font::builder()
        .family("Microsoft YaHei UI")
        .size(24)
        .weight(600)
        .build(&mut title_font)?;
    nwg::Font::builder()
        .family("Microsoft YaHei UI")
        .size(14)
        .weight(500)
        .build(&mut status_font)?;

    nwg::Window::builder()
        .size((680, 360))
        .center(true)
        .title("Effect 动态壁纸")
        .build(&mut window)?;

    nwg::Label::builder()
        .text("Effect 动态壁纸")
        .position((34, 28))
        .size((360, 42))
        .font(Some(&title_font))
        .parent(&window)
        .build(&mut title)?;
    nwg::Label::builder()
        .text("选择本地 HTML 特效，并将它安静地放到桌面图标后面。")
        .position((36, 72))
        .size((580, 26))
        .parent(&window)
        .build(&mut subtitle)?;
    nwg::Label::builder()
        .text("壁纸来源")
        .position((36, 118))
        .size((150, 24))
        .parent(&window)
        .build(&mut path_label)?;
    nwg::TextInput::builder()
        .text(&initial_path)
        .position((36, 148))
        .size((596, 34))
        .readonly(true)
        .parent(&window)
        .build(&mut path_input)?;
    nwg::Button::builder()
        .text("选择 HTML")
        .position((36, 198))
        .size((126, 38))
        .parent(&window)
        .build(&mut choose_file)?;
    nwg::Button::builder()
        .text("选择目录")
        .position((174, 198))
        .size((126, 38))
        .parent(&window)
        .build(&mut choose_folder)?;
    nwg::Button::builder()
        .text("应用")
        .position((382, 198))
        .size((118, 38))
        .parent(&window)
        .build(&mut apply)?;
    nwg::Button::builder()
        .text("停止并恢复")
        .position((514, 198))
        .size((118, 38))
        .parent(&window)
        .build(&mut stop)?;

    let initial_status = if desktop::backup_exists() {
        "● 检测到壁纸备份，可点击“停止并恢复”"
    } else {
        "● 已停止"
    };
    nwg::Label::builder()
        .text(initial_status)
        .position((36, 272))
        .size((596, 34))
        .font(Some(&status_font))
        .parent(&window)
        .build(&mut status)?;

    nwg::FileDialog::builder()
        .title("选择 HTML 壁纸")
        .action(nwg::FileDialogAction::Open)
        .filters("HTML 文件 (*.html;*.htm)|所有文件 (*.*)")
        .build(&mut file_dialog)?;
    nwg::FileDialog::builder()
        .title("选择包含 index.html 的目录")
        .action(nwg::FileDialogAction::OpenDirectory)
        .build(&mut folder_dialog)?;

    let state = Rc::new(RefCell::new(AppState {
        config,
        runtime: None,
    }));
    let event_window = Rc::new(window);
    let handler_window = event_window.clone();
    let handler_state = state.clone();

    let handler =
        nwg::full_bind_event_handler(&event_window.handle, move |event, _event_data, handle| {
            match event {
                nwg::Event::OnButtonClick if handle == choose_file.handle => {
                    if file_dialog.run(Some(&*handler_window))
                        && let Ok(selected) = file_dialog.get_selected_item()
                    {
                        update_selection(
                            PathBuf::from(selected),
                            &path_input,
                            &status,
                            &handler_state,
                        );
                    }
                }
                nwg::Event::OnButtonClick if handle == choose_folder.handle => {
                    if folder_dialog.run(Some(&*handler_window))
                        && let Ok(selected) = folder_dialog.get_selected_item()
                    {
                        update_selection(
                            PathBuf::from(selected),
                            &path_input,
                            &status,
                            &handler_state,
                        );
                    }
                }
                nwg::Event::OnButtonClick if handle == apply.handle => {
                    if let Err(error) = apply_wallpaper(&handler_state, &status) {
                        status.set_text("● 应用失败");
                        nwg::modal_error_message(
                            &handler_window.handle,
                            "无法应用动态壁纸",
                            &error.to_string(),
                        );
                    }
                }
                nwg::Event::OnButtonClick if handle == stop.handle => {
                    if let Err(error) = stop_wallpaper(&handler_state, &status) {
                        status.set_text("● 恢复失败");
                        nwg::modal_error_message(
                            &handler_window.handle,
                            "无法恢复静态壁纸",
                            &error.to_string(),
                        );
                    }
                }
                nwg::Event::OnWindowClose if handle == handler_window.handle => {
                    if let Err(error) = stop_wallpaper(&handler_state, &status) {
                        nwg::modal_error_message(
                            &handler_window.handle,
                            "退出前无法恢复静态壁纸",
                            &error.to_string(),
                        );
                        return;
                    }
                    nwg::stop_thread_dispatch();
                }
                _ => {}
            }
        });

    nwg::dispatch_thread_events();
    nwg::unbind_event_handler(&handler);
    Ok(())
}

fn update_selection(
    selected: PathBuf,
    path_input: &nwg::TextInput,
    status: &nwg::Label,
    state: &Rc<RefCell<AppState>>,
) {
    path_input.set_text(&path_text(&selected));
    let mut state = state.borrow_mut();
    state.config.selected_path = Some(selected);
    match config::save(&state.config) {
        Ok(()) => status.set_text("● 已选择，点击“应用”开始"),
        Err(_) => status.set_text("● 已选择，但保存配置失败"),
    }
}

fn apply_wallpaper(
    state: &Rc<RefCell<AppState>>,
    status: &nwg::Label,
) -> Result<(), Box<dyn Error>> {
    let selection = state
        .borrow()
        .config
        .selected_path
        .clone()
        .ok_or("请先选择 HTML 文件或目录")?;

    let previous = state.borrow_mut().runtime.take();
    if let Some(runtime) = previous {
        runtime.stop();
    }

    if !desktop::backup_exists() {
        desktop::save_current_wallpaper()?;
    }

    match WallpaperRuntime::start(&selection) {
        Ok(runtime) => {
            state.borrow_mut().runtime = Some(runtime);
            status.set_text(&format!("● 正在运行：{}", display_name(&selection)));
            Ok(())
        }
        Err(error) => {
            let _ = desktop::restore_saved_wallpaper();
            desktop::refresh_desktop();
            Err(error)
        }
    }
}

fn stop_wallpaper(
    state: &Rc<RefCell<AppState>>,
    status: &nwg::Label,
) -> Result<(), Box<dyn Error>> {
    if let Some(runtime) = state.borrow_mut().runtime.take() {
        runtime.stop();
    }
    let restored = desktop::restore_saved_wallpaper()?;
    desktop::refresh_desktop();
    status.set_text(if restored {
        "● 已停止，并恢复之前的静态壁纸"
    } else {
        "● 已停止"
    });
    Ok(())
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("HTML 壁纸")
        .to_string()
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
