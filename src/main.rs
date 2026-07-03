#![windows_subsystem = "windows"]
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::{
    Rect, WebViewBuilder,
    http::{Request, Response, header::CONTENT_TYPE},
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "start".to_string());
    match command.as_str() {
        "start" => {
            let project = args.next().unwrap_or_else(|| "clouds".to_string());
            run_wallpaper(&project)
        }
        "stop" => {
            #[cfg(target_os = "windows")]
            windows_wallpaper::stop_wallpaper();
            stop_by_pid_file();
            #[cfg(target_os = "windows")]
            windows_wallpaper::refresh_desktop();
            println!("Stop signal sent.");
            Ok(())
        }
        "status" => {
            #[cfg(target_os = "windows")]
            println!(
                "{}",
                if pid_file_process_exists() {
                    "running"
                } else {
                    "stopped"
                }
            );
            #[cfg(not(target_os = "windows"))]
            println!("status is only implemented on Windows");
            Ok(())
        }
        _ => {
            eprintln!("Usage: cargo run -- [start [static-project]|stop|status]");
            Ok(())
        }
    }
}

fn run_wallpaper(project: &str) -> Result<(), Box<dyn Error>> {
    let asset_root = static_project_root(project)?;

    if pid_file_process_exists() {
        stop_by_pid_file();
        #[cfg(target_os = "windows")]
        windows_wallpaper::refresh_desktop();
    }

    write_pid_file()?;
    let event_loop = EventLoop::new();

    let bounds = screen_bounds(&event_loop);

    let window = WindowBuilder::new()
        .with_title("")
        .with_decorations(false)
        .with_resizable(false)
        .with_visible(false)
        .with_inner_size(LogicalSize::new(bounds.width as f64, bounds.height as f64))
        .with_position(LogicalPosition::new(bounds.x as f64, bounds.y as f64))
        .build(&event_loop)?;

    let webview = WebViewBuilder::new()
        .with_custom_protocol(
            "effect".into(),
            move |_webview_id, request| match asset_response(request, &asset_root) {
                Ok(response) => response.map(Into::into),
                Err(error) => Response::builder()
                    .status(500)
                    .header(CONTENT_TYPE, "text/plain; charset=utf-8")
                    .body(error.to_string().into_bytes())
                    .expect("valid error response")
                    .map(Into::into),
            },
        )
        .with_bounds(Rect {
            position: LogicalPosition::new(0, 0).into(),
            size: LogicalSize::new(bounds.width as u32, bounds.height as u32).into(),
        })
        .with_url("effect://index.html")
        .build_as_child(&window)?;

    #[cfg(target_os = "windows")]
    {
        use tao::platform::windows::WindowExtWindows;
        let _ = window.set_skip_taskbar(true);
        windows_wallpaper::attach_behind_desktop_icons(&window, bounds.width, bounds.height)?;
        window.set_visible(true);
    }

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        if let Event::WindowEvent { event, .. } = event {
            match event {
                WindowEvent::CloseRequested => {
                    window.set_visible(false);
                    let _ = remove_pid_file();
                    #[cfg(target_os = "windows")]
                    windows_wallpaper::refresh_desktop();
                    *control_flow = ControlFlow::Exit;
                }
                WindowEvent::Destroyed => {
                    #[cfg(target_os = "windows")]
                    windows_wallpaper::refresh_desktop();
                }
                WindowEvent::Resized(size) => {
                    let size = size.to_logical::<u32>(window.scale_factor());
                    let _ = webview.set_bounds(Rect {
                        position: LogicalPosition::new(0, 0).into(),
                        size: LogicalSize::new(size.width, size.height).into(),
                    });
                }
                _ => {}
            }
        }
    });
}

fn pid_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("effect-wallpaper.pid")
}

fn write_pid_file() -> Result<(), Box<dyn Error>> {
    let path = pid_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, std::process::id().to_string())?;
    Ok(())
}

fn remove_pid_file() -> Result<(), Box<dyn Error>> {
    let path = pid_file();
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn stop_by_pid_file() {
    let Ok(pid_text) = fs::read_to_string(pid_file()) else {
        return;
    };
    let Ok(pid) = pid_text.trim().parse::<u32>() else {
        let _ = remove_pid_file();
        return;
    };

    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
    let _ = remove_pid_file();
}

fn pid_file_process_exists() -> bool {
    let Ok(pid_text) = fs::read_to_string(pid_file()) else {
        return false;
    };
    let Ok(pid) = pid_text.trim().parse::<u32>() else {
        return false;
    };

    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}")])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
        .unwrap_or(false)
}

#[derive(Clone, Copy)]
struct Bounds {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

fn screen_bounds(event_loop: &EventLoop<()>) -> Bounds {
    let Some(monitor) = event_loop
        .primary_monitor()
        .or_else(|| event_loop.available_monitors().next())
    else {
        return Bounds {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        };
    };

    let position = monitor.position();
    let size = monitor.size();

    Bounds {
        x: position.x,
        y: position.y,
        width: size.width as i32,
        height: size.height as i32,
    }
}

fn static_project_root(project: &str) -> Result<PathBuf, Box<dyn Error>> {
    if project.is_empty()
        || project.contains('/')
        || project.contains('\\')
        || project == "."
        || project == ".."
    {
        return Err(format!("invalid static project name: {project}").into());
    }

    let static_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("static")
        .canonicalize()?;
    let project_root = static_root.join(project).canonicalize()?;

    if !project_root.starts_with(&static_root) {
        return Err("blocked path outside static".into());
    }

    if !project_root.join("index.html").is_file() {
        return Err(format!("missing index.html in static/{project}").into());
    }

    Ok(project_root)
}

fn asset_response(
    request: Request<Vec<u8>>,
    root: &Path,
) -> Result<Response<Vec<u8>>, Box<dyn Error>> {
    let requested_path = request.uri().path().trim_start_matches('/');
    let relative_path = if requested_path.is_empty() {
        "index.html"
    } else {
        requested_path
    };

    let path = root.join(relative_path).canonicalize()?;
    if !path.starts_with(root) {
        return Err("blocked path outside selected static project".into());
    }

    Response::builder()
        .header(CONTENT_TYPE, mime_type(&path))
        .body(std::fs::read(path)?)
        .map_err(Into::into)
}

fn mime_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

#[cfg(target_os = "windows")]
mod windows_wallpaper {
    use std::{ffi::c_void, ptr::null_mut};

    use tao::{platform::windows::WindowExtWindows, window::Window};
    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, WPARAM},
            System::Com::{
                CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            UI::Shell::{DSD_FORWARD, DesktopWallpaper, IDesktopWallpaper},
            UI::WindowsAndMessaging::{
                EnumWindows, FindWindowExW, FindWindowW, GWL_EXSTYLE, GWL_STYLE, GetDesktopWindow,
                GetWindowLongPtrW, HWND_BOTTOM, SMTO_NORMAL, SW_SHOWNA, SWP_FRAMECHANGED,
                SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_SHOWWINDOW, SendMessageTimeoutW, SetParent,
                SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_SETTINGCHANGE, WS_BORDER,
                WS_CAPTION, WS_CHILD, WS_DLGFRAME, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
                WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU,
                WS_THICKFRAME,
            },
        },
        core::{BOOL, PCWSTR, Result as WindowsResult, w},
    };

    const WM_SPAWN_WORKERW: u32 = 0x052c;

    pub fn stop_wallpaper() {
        refresh_desktop();
    }

    pub fn refresh_desktop() {
        unsafe {
            advance_wallpaper_slideshow();

            let progman = FindWindowW(w!("Progman"), None).unwrap_or(HWND(null_mut()));
            if !progman.is_invalid() {
                let mut message_result = 0;
                let _ = SendMessageTimeoutW(
                    progman,
                    WM_SPAWN_WORKERW,
                    WPARAM(0),
                    LPARAM(0),
                    SMTO_NORMAL,
                    1000,
                    Some(&mut message_result),
                );
                let _ = SendMessageTimeoutW(
                    progman,
                    WM_SETTINGCHANGE,
                    WPARAM(0),
                    LPARAM(0),
                    SMTO_NORMAL,
                    1000,
                    None,
                );
            }
        }
    }

    unsafe fn advance_wallpaper_slideshow() {
        let coinit = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let initialized = coinit.is_ok();

        if initialized || coinit.0 == 0x80010106u32 as i32 {
            if let Ok(desktop_wallpaper) = unsafe {
                CoCreateInstance::<_, IDesktopWallpaper>(&DesktopWallpaper, None, CLSCTX_ALL)
            } {
                let _ = unsafe { desktop_wallpaper.AdvanceSlideshow(PCWSTR::null(), DSD_FORWARD) };
            }
        }

        if initialized {
            unsafe {
                CoUninitialize();
            }
        }
    }

    pub fn attach_behind_desktop_icons(
        window: &Window,
        width: i32,
        height: i32,
    ) -> WindowsResult<()> {
        let hwnd = HWND(window.hwnd() as *mut c_void);
        let parent = find_workerw_behind_icons()?;

        unsafe {
            configure_child_wallpaper_window(hwnd);
            let _ = SetParent(hwnd, Some(parent));
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_BOTTOM),
                0,
                0,
                width,
                height,
                SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            );
            let _ = ShowWindow(hwnd, SW_SHOWNA);
        }

        Ok(())
    }

    unsafe fn configure_child_wallpaper_window(hwnd: HWND) {
        let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 };
        let frame_bits = WS_POPUP.0
            | WS_CAPTION.0
            | WS_THICKFRAME.0
            | WS_BORDER.0
            | WS_DLGFRAME.0
            | WS_SYSMENU.0
            | WS_MINIMIZEBOX.0
            | WS_MAXIMIZEBOX.0;
        let style = (style & !frame_bits) | WS_CHILD.0;
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);
        }

        let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 };
        let ex_style = (ex_style & !WS_EX_APPWINDOW.0) | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0;
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style as isize);
        }
    }

    fn find_workerw_behind_icons() -> WindowsResult<HWND> {
        unsafe {
            let progman = FindWindowW(w!("Progman"), None)?;
            let mut message_result = 0;
            SendMessageTimeoutW(
                progman,
                WM_SPAWN_WORKERW,
                WPARAM(0),
                LPARAM(0),
                SMTO_NORMAL,
                1000,
                Some(&mut message_result),
            );

            let mut state = DesktopSearch {
                target_workerw: HWND(null_mut()),
            };
            let _ = EnumWindows(
                Some(enum_windows_proc),
                LPARAM((&mut state as *mut DesktopSearch) as isize),
            );

            if !state.target_workerw.is_invalid() {
                return Ok(state.target_workerw);
            }

            if !progman.is_invalid() {
                return Ok(progman);
            }
        }

        Ok(unsafe { GetDesktopWindow() })
    }

    struct DesktopSearch {
        target_workerw: HWND,
    }

    unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let shell_view = unsafe { FindWindowExW(Some(hwnd), None, w!("SHELLDLL_DefView"), None) };

        if shell_view.is_ok() {
            let workerw_after_icon_layer =
                unsafe { FindWindowExW(None, Some(hwnd), w!("WorkerW"), None) };

            if let Ok(workerw) = workerw_after_icon_layer {
                if !workerw.is_invalid() {
                    let state = lparam.0 as *mut DesktopSearch;
                    unsafe {
                        (*state).target_workerw = workerw;
                    }
                    return BOOL(0);
                }
            }
        }

        BOOL(1)
    }
}
