use std::{
    error::Error,
    ffi::c_void,
    path::{Path, PathBuf},
    ptr::null_mut,
    sync::mpsc,
    thread::{self, JoinHandle},
};

use tao::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::Event,
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy},
    platform::run_return::EventLoopExtRunReturn,
    platform::windows::{EventLoopBuilderExtWindows, WindowExtWindows},
    window::WindowBuilder,
};
use wry::{
    Rect, WebContext, WebViewBuilder,
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
    pub fn validate_selection(selection: &Path) -> Result<(), Box<dyn Error>> {
        AssetSource::from_selection(selection).map(|_| ())
    }

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
    let mut event_loop = builder.build();
    let bounds = screen_bounds(&event_loop);

    let window = WindowBuilder::new()
        .with_title("")
        .with_decorations(false)
        .with_resizable(false)
        .with_visible(false)
        .with_inner_size(PhysicalSize::new(bounds.width as u32, bounds.height as u32))
        .with_position(PhysicalPosition::new(bounds.x, bounds.y))
        .build(&event_loop)?;

    let asset_source = source.clone();
    let data_directory = webview_data_directory()?;
    let mut web_context = WebContext::new(Some(data_directory));
    let webview = WebViewBuilder::new_with_web_context(&mut web_context)
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
            position: PhysicalPosition::new(0, 0).into(),
            size: PhysicalSize::new(bounds.width as u32, bounds.height as u32).into(),
        })
        .with_initialization_script(FULL_SCREEN_DOCUMENT_SCRIPT)
        .with_url("effect://wallpaper/index.html")
        .build_as_child(&window)?;

    let _ = window.set_skip_taskbar(true);
    let parent_size = windows_shell::attach_behind_desktop_icons(&window)?;
    webview.set_bounds(Rect {
        position: PhysicalPosition::new(0, 0).into(),
        size: parent_size.into(),
    })?;

    let proxy = event_loop.create_proxy();
    ready.send(Ok(proxy)).map_err(|_| "管理窗口已关闭")?;

    event_loop.run_return(move |event, _, control_flow| {
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
                let _ = webview.set_bounds(Rect {
                    position: PhysicalPosition::new(0, 0).into(),
                    size: PhysicalSize::new(size.width, size.height).into(),
                });
            }
            Event::LoopDestroyed => windows_shell::refresh_desktop(),
            _ => {}
        }
    });

    Ok(())
}

const FULL_SCREEN_DOCUMENT_SCRIPT: &str = r#"
window.addEventListener('DOMContentLoaded', () => {
    const viewport = document.querySelector('meta[name="viewport"]') || document.createElement('meta');
    viewport.name = 'viewport';
    viewport.content = 'width=device-width, height=device-height, initial-scale=1.0, maximum-scale=1.0, user-scalable=no';
    if (!viewport.parentNode) document.head.appendChild(viewport);

    for (const element of [document.documentElement, document.body]) {
        element.style.setProperty('width', '100%', 'important');
        element.style.setProperty('height', '100%', 'important');
        element.style.setProperty('margin', '0', 'important');
        element.style.setProperty('padding', '0', 'important');
        element.style.setProperty('overflow', 'hidden', 'important');
    }
}, { once: true });
"#;

fn webview_data_directory() -> Result<PathBuf, Box<dyn Error>> {
    let path = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("effect")
        .join("webview2-user-data");
    std::fs::create_dir_all(&path)?;
    Ok(path.canonicalize()?)
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
            Foundation::{HWND, LPARAM, RECT, WPARAM},
            UI::WindowsAndMessaging::{
                EnumWindows, FindWindowExW, FindWindowW, GWL_EXSTYLE, GWL_STYLE, GetDesktopWindow,
                GetWindowInfo, GetWindowLongPtrW, HWND_BOTTOM, SMTO_NORMAL, SW_SHOWNA,
                SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_SHOWWINDOW,
                SendMessageTimeoutW, SetParent, SetWindowLongPtrW, SetWindowPos, ShowWindow,
                WINDOWINFO, WM_SETTINGCHANGE, WS_BORDER, WS_CAPTION, WS_CHILD, WS_DLGFRAME,
                WS_EX_APPWINDOW, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_EX_NOACTIVATE,
                WS_EX_STATICEDGE, WS_EX_TOOLWINDOW, WS_EX_WINDOWEDGE, WS_MAXIMIZEBOX,
                WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
            },
        },
        core::{BOOL, Error as WindowsError, HRESULT, Result as WindowsResult, w},
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
    ) -> WindowsResult<PhysicalSize<u32>> {
        let hwnd = HWND(window.hwnd() as *mut c_void);
        let parent = find_workerw_behind_icons()?;
        unsafe {
            SetParent(hwnd, Some(parent))?;
            configure_child_wallpaper_window(hwnd)?;
            let parent_info = window_info(parent)?;
            let host_info = window_info(hwnd)?;
            let size = rect_size(parent_info.rcClient)?;

            // SetWindowPos sizes the outer window. Offset and enlarge it by any
            // remaining non-client area so the host's client area, where the
            // WebView lives, exactly matches the parent WorkerW client area.
            let frame_left = host_info.rcClient.left - host_info.rcWindow.left;
            let frame_top = host_info.rcClient.top - host_info.rcWindow.top;
            let frame_right = host_info.rcWindow.right - host_info.rcClient.right;
            let frame_bottom = host_info.rcWindow.bottom - host_info.rcClient.bottom;
            SetWindowPos(
                hwnd,
                Some(HWND_BOTTOM),
                -frame_left,
                -frame_top,
                size.width as i32 + frame_left + frame_right,
                size.height as i32 + frame_top + frame_bottom,
                SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            )?;

            let fitted_host = window_info(hwnd)?;
            if fitted_host.rcClient != parent_info.rcClient {
                return Err(WindowsError::new(
                    HRESULT(0x80004005u32 as i32),
                    "壁纸客户区未能与桌面父窗口完全重合",
                ));
            }
            let _ = ShowWindow(hwnd, SW_SHOWNA);
            Ok(size)
        }
    }

    unsafe fn window_info(hwnd: HWND) -> WindowsResult<WINDOWINFO> {
        let mut info = WINDOWINFO {
            cbSize: std::mem::size_of::<WINDOWINFO>() as u32,
            ..Default::default()
        };
        unsafe { GetWindowInfo(hwnd, &mut info)? };
        Ok(info)
    }

    fn rect_size(rect: RECT) -> WindowsResult<PhysicalSize<u32>> {
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            return Err(WindowsError::new(
                HRESULT(0x80004005u32 as i32),
                "桌面父窗口客户区尺寸无效",
            ));
        }
        Ok(PhysicalSize::new(width as u32, height as u32))
    }

    unsafe fn configure_child_wallpaper_window(hwnd: HWND) -> WindowsResult<()> {
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

        let applied_style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 };
        if applied_style & frame_bits != 0 || applied_style & WS_CHILD.0 == 0 {
            return Err(WindowsError::new(
                HRESULT(0x80004005u32 as i32),
                "无法将壁纸宿主窗口设置为无边框子窗口",
            ));
        }

        let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 };
        let extended_frame_bits = WS_EX_APPWINDOW.0
            | WS_EX_CLIENTEDGE.0
            | WS_EX_DLGMODALFRAME.0
            | WS_EX_STATICEDGE.0
            | WS_EX_WINDOWEDGE.0;
        unsafe {
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                ((ex_style & !extended_frame_bits) | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0)
                    as isize,
            )
        };

        let applied_ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 };
        if applied_ex_style & extended_frame_bits != 0 {
            return Err(WindowsError::new(
                HRESULT(0x80004005u32 as i32),
                "无法清除壁纸宿主窗口的扩展边框样式",
            ));
        }

        Ok(())
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
