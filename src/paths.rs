//! Platform folders and opening files / links in the system.

use std::path::{Path, PathBuf};

fn home() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// The game folder of the official launcher
pub fn minecraft_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join(".minecraft")
    } else if cfg!(target_os = "macos") {
        home().join("Library").join("Application Support").join("minecraft")
    } else {
        home().join(".minecraft")
    }
}

/// Settings, crash log and skin cache of the application
pub fn app_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join("mc-exporter")
    } else if cfg!(target_os = "macos") {
        home().join("Library").join("Application Support").join("mc-exporter")
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config")).join("mc-exporter")
    }
}

fn open(target: &std::ffi::OsStr) {
    let program = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(program).arg(target).spawn();
}

/// Shows a folder in the file manager
pub fn open_folder(dir: &Path) {
    open(dir.as_os_str());
}

/// Opens a web page in the default browser
pub fn open_url(url: &str) {
    open(std::ffi::OsStr::new(url));
}
