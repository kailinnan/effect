use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct AppConfig {
    pub selected_path: Option<PathBuf>,
}

pub fn load() -> AppConfig {
    read_json(&config_path()).unwrap_or_default()
}

pub fn save(config: &AppConfig) -> Result<(), Box<dyn Error>> {
    write_json(&config_path(), config)
}

pub fn backup_path() -> PathBuf {
    data_dir().join("wallpaper-backup.json")
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, Box<dyn Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn config_path() -> PathBuf {
    data_dir().join("config.json")
}

fn data_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("effect")
}
