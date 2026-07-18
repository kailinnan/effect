use std::{error::Error, ffi::c_void, fs};

use serde::{Deserialize, Serialize};
use windows::{
    Win32::{
        Foundation::COLORREF,
        System::Com::{
            CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
            CoUninitialize,
        },
        UI::Shell::{DESKTOP_WALLPAPER_POSITION, DesktopWallpaper, IDesktopWallpaper},
    },
    core::{Error as WindowsError, HRESULT, HSTRING, PCWSTR, PWSTR, Result as WindowsResult},
};

use crate::config;

#[derive(Debug, Deserialize, Serialize)]
struct DesktopBackup {
    monitors: Vec<MonitorWallpaper>,
    position: i32,
    background_color: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct MonitorWallpaper {
    monitor_id: String,
    wallpaper_path: String,
}

pub fn backup_exists() -> bool {
    config::backup_path().is_file()
}

pub fn save_current_wallpaper() -> Result<(), Box<dyn Error>> {
    let backup = with_desktop_wallpaper(|desktop| unsafe {
        let count = desktop.GetMonitorDevicePathCount()?;
        let mut monitors = Vec::with_capacity(count as usize);

        for index in 0..count {
            let monitor_id_ptr = desktop.GetMonitorDevicePathAt(index)?;
            let monitor_id = take_com_string(monitor_id_ptr)?;
            let wallpaper_ptr = desktop.GetWallpaper(&HSTRING::from(&monitor_id))?;
            let wallpaper_path = take_com_string(wallpaper_ptr)?;
            monitors.push(MonitorWallpaper {
                monitor_id,
                wallpaper_path,
            });
        }

        Ok(DesktopBackup {
            monitors,
            position: desktop.GetPosition()?.0,
            background_color: desktop.GetBackgroundColor()?.0,
        })
    })?;

    config::write_json(&config::backup_path(), &backup)
}

pub fn restore_saved_wallpaper() -> Result<bool, Box<dyn Error>> {
    let path = config::backup_path();
    if !path.is_file() {
        return Ok(false);
    }

    let backup: DesktopBackup = config::read_json(&path)?;
    with_desktop_wallpaper(|desktop| unsafe {
        desktop.SetPosition(DESKTOP_WALLPAPER_POSITION(backup.position))?;
        desktop.SetBackgroundColor(COLORREF(backup.background_color))?;

        for monitor in &backup.monitors {
            desktop.SetWallpaper(
                &HSTRING::from(&monitor.monitor_id),
                &HSTRING::from(&monitor.wallpaper_path),
            )?;
        }

        Ok(())
    })?;

    fs::remove_file(path)?;
    Ok(true)
}

pub fn refresh_desktop() {
    crate::wallpaper::windows_shell::refresh_desktop();
}

fn with_desktop_wallpaper<T>(
    operation: impl FnOnce(&IDesktopWallpaper) -> WindowsResult<T>,
) -> WindowsResult<T> {
    unsafe {
        let coinit = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let initialized = coinit.is_ok();
        if !initialized && coinit.0 != 0x80010106u32 as i32 {
            return Err(WindowsError::from_hresult(coinit));
        }

        let desktop =
            CoCreateInstance::<_, IDesktopWallpaper>(&DesktopWallpaper, None, CLSCTX_ALL)?;
        let result = operation(&desktop);

        if initialized {
            CoUninitialize();
        }
        result
    }
}

unsafe fn take_com_string(value: PWSTR) -> WindowsResult<String> {
    let result = unsafe { PCWSTR(value.0).to_string() };
    unsafe { CoTaskMemFree(Some(value.0.cast::<c_void>())) };
    result.map_err(|error| WindowsError::new(HRESULT(0x80004005u32 as i32), error.to_string()))
}
