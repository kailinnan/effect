use std::{
    error::Error,
    ffi::c_void,
    path::{Path, PathBuf},
    ptr::null_mut,
    sync::mpsc,
    thread::{self, JoinHandle},
};

use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event::Event,
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy},
    platform::windows::{EventLoopBuilderExtWindows, WindowExtWindows},
    window::WindowBuilder,
};
use wry::{
    Rect, WebViewBuilder,
    http::{Request, Response, header::CONTENT_TYPE},
};

enum WallpaperEvent {
    Stop,
}

pub struct WallpaperRuntime {
    proxy: EventLoopProxy<WallpaperEvent>,
    thread: Option<JoinHandle<()>>,
}

impl WallpaperRuntime {
    pub fn start(selection: &Path) -> Result<Self, Box<dyn Error>> {
        let source = AssetSource::from_selection(selection)?;
        let (sender, receiver) = mpsc::sync_channel(1);

        let thread = thread::spawn(move || {
            let result = run_wallpaper(source, sender.clone());
            if let Err(error) = result {
                let _ = sender.send(Err(error.to_string()));
            }
        });

        match receiver.recv() {
            Ok(Ok(proxy)) => Ok(Self {
                proxy,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error.into())
            }
            Err(error) => Err(format!("动态壁纸线程意外结束：{error}").into()),
        }
    }

    pub fn stop(mut self) {
        let _ = self.proxy.send_event(WallpaperEvent::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Clone)]
struct AssetSource {
    root: PathBuf,
    entry: PathBuf,
}

impl AssetSource {
    fn from_selection(selection: &Path) -> Result<Self, Box<dyn Error>> {
        let selection = selection.canonicalize()?;
        let (root, entry) = if selection.is_dir() {
            let entry = selection.join("index.html");
            (selection, entry)
        } else {
            let is_html = selection
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("html") || extension.eq_ignore_ascii_case("htm")
                });
            if !is_html {
                return Err("请选择 HTML 文件或包含 index.html 的目录".into());
            }
            let root = selection
                .parent()
                .ok_or("无法确定 HTML 文件所在目录")?
                .to_path_buf();
            (root, selection)
        };

        let entry = entry.canonicalize()?;
        if !entry.starts_with(&root) || !entry.is_file() {
            return Err("所选目录中没有可用的 index.html".into());
        }

        Ok(Self { root, entry })
    }
}

fn run_wallpaper(
    source: AssetSource,
    ready: mpsc::SyncSender<Result<EventLoopProxy<WallpaperEvent>, String>>,
) -> Result<(), Box<dyn Error>> {
    let mut builder = EventLoopBuilder::<WallpaperEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = builder.build();
    let bounds = screen_bounds(&event_loop);

    let window = WindowBuilder::new()
        .with_title("")
        .with_decorations(false)
        .with_resizable(false)
        .with_visible(false)
        .with_inner_size(LogicalSize::new(bounds.width as f64, bounds.height as f64))
        .with_position(LogicalPosition::new(bounds.x as f64, bounds.y as f64))
        .build(&event_loop)?;

    let asset_source = source.clone();
    let webview = WebViewBuilder::new()
        .with_custom_protocol(
            "effect".into(),
            move |_webview_id, request| match asset_response(request, &asset_source) {
                Ok(response) => response.map(Into::into),
                Err(error) => Response::builder()
                    .status(404)
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
        .with_url("effect://wallpaper/index.html")
        .build_as_child(&window)?;

    let _ = window.set_skip_taskbar(true);
    windows_shell::attach_behind_desktop_icons(&window, bounds.width, bounds.height)?;
    window.set_visible(true);

    let proxy = event_loop.create_proxy();
    ready.send(Ok(proxy)).map_err(|_| "管理窗口已关闭")?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(WallpaperEvent::Stop) => {
                window.set_visible(false);
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: tao::event::WindowEvent::Resized(size),
                ..
            } => {
                let size = size.to_logical::<u32>(window.scale_factor());
                let _ = webview.set_bounds(Rect {
                    position: LogicalPosition::new(0, 0).into(),
                    size: LogicalSize::new(size.width, size.height).into(),
                });
            }
            Event::LoopDestroyed => windows_shell::refresh_desktop(),
            _ => {}
        }
    });
}

fn asset_response(
    request: Request<Vec<u8>>,
    source: &AssetSource,
) -> Result<Response<Vec<u8>>, Box<dyn Error>> {
    let requested = request.uri().path().trim_start_matches('/');
    let path = if requested.is_empty() || requested == "index.html" {
        source.entry.clone()
    } else {
        source.root.join(requested).canonicalize()?
    };

    if !path.starts_with(&source.root) || !path.is_file() {
        return Err("blocked asset path".into());
    }

    Response::builder()
        .header(CONTENT_TYPE, mime_type(&path))
        .body(std::fs::read(path)?)
        .map_err(Into::into)
}

fn mime_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("mp3") => "audio/mpeg",
        Some("mp4") => "video/mp4",
        _ => "application/octet-stream",
    }
}

#[derive(Clone, Copy)]
struct Bounds {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

fn screen_bounds(event_loop: &EventLoop<WallpaperEvent>) -> Bounds {
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

pub mod windows_shell {
    use super::*;
    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, WPARAM},
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
        core::{BOOL, Result as WindowsResult, w},
    };

    const WM_SPAWN_WORKERW: u32 = 0x052c;

    pub fn refresh_desktop() {
        unsafe {
            let progman = FindWindowW(w!("Progman"), None).unwrap_or(HWND(null_mut()));
            if progman.is_invalid() {
                return;
            }
            let mut result = 0;
            let _ = SendMessageTimeoutW(
                progman,
                WM_SPAWN_WORKERW,
                WPARAM(0),
                LPARAM(0),
                SMTO_NORMAL,
                1000,
                Some(&mut result),
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

    pub fn attach_behind_desktop_icons(
        window: &tao::window::Window,
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
        unsafe {
            SetWindowLongPtrW(
                hwnd,
                GWL_STYLE,
                ((style & !frame_bits) | WS_CHILD.0) as isize,
            )
        };

        let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 };
        unsafe {
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                ((ex_style & !WS_EX_APPWINDOW.0) | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0)
                    as isize,
            )
        };
    }

    fn find_workerw_behind_icons() -> WindowsResult<HWND> {
        unsafe {
            let progman = FindWindowW(w!("Progman"), None)?;
            let mut result = 0;
            SendMessageTimeoutW(
                progman,
                WM_SPAWN_WORKERW,
                WPARAM(0),
                LPARAM(0),
                SMTO_NORMAL,
                1000,
                Some(&mut result),
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
            Ok(GetDesktopWindow())
        }
    }

    struct DesktopSearch {
        target_workerw: HWND,
    }

    unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let shell_view = unsafe { FindWindowExW(Some(hwnd), None, w!("SHELLDLL_DefView"), None) };
        if shell_view.is_ok() {
            let workerw = unsafe { FindWindowExW(None, Some(hwnd), w!("WorkerW"), None) };
            if let Ok(workerw) = workerw
                && !workerw.is_invalid()
            {
                unsafe { (*(lparam.0 as *mut DesktopSearch)).target_workerw = workerw };
                return BOOL(0);
            }
        }
        BOOL(1)
    }
}
