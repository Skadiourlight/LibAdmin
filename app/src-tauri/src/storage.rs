//! Decides where a library's index lives: inside the library if writable, else under the app folder.
use crate::config::{fallback_data_dir, INDEX_DIR_NAME};
use std::path::{Path, PathBuf};

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn set_hidden(path: &Path) {
    use windows_sys::Win32::Storage::FileSystem::{SetFileAttributesW, FILE_ATTRIBUTE_HIDDEN};
    let w = wide(&path.to_string_lossy());
    unsafe {
        SetFileAttributesW(w.as_ptr(), FILE_ATTRIBUTE_HIDDEN);
    }
}
#[cfg(not(windows))]
fn set_hidden(_: &Path) {}

#[cfg(windows)]
fn volume_serial(path: &Path) -> String {
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;
    let s = path.to_string_lossy();
    let Some(colon) = s.find(':') else { return String::new() };
    let root = wide(&format!("{}\\", &s[..=colon]));
    let mut serial: u32 = 0;
    let ok = unsafe {
        GetVolumeInformationW(root.as_ptr(), std::ptr::null_mut(), 0, &mut serial, std::ptr::null_mut(),
            std::ptr::null_mut(), std::ptr::null_mut(), 0)
    };
    if ok != 0 { format!("{serial:08X}") } else { String::new() }
}
#[cfg(not(windows))]
fn volume_serial(_: &Path) -> String {
    String::new()
}

fn try_writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".write_test");
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Keyed by volume serial + path without drive letter, so a removable disk keeps its index after
/// its drive letter changes. Same scheme as the Python version, so both find the same folder.
pub fn fallback_index_dir(library: &Path) -> PathBuf {
    let s = library.to_string_lossy().to_string();
    let tail = match s.find(':') {
        Some(i) => s[i + 1..].to_lowercase(),
        None => s.to_lowercase(),
    };
    let key = sha1_smol::Sha1::from(format!("{}|{}", volume_serial(library), tail)).digest().to_string();
    let name: String = library
        .file_name()
        .map(|n| n.to_string_lossy().chars().map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect())
        .unwrap_or_else(|| "library".into());
    fallback_data_dir().join(format!("{}_{}", name, &key[..10]))
}

/// Returns (index_dir, is_fallback).
pub fn resolve_index_dir(library: &Path) -> std::io::Result<(PathBuf, bool)> {
    let local = library.join(INDEX_DIR_NAME);
    let existed = local.exists();
    if try_writable(&local) {
        if !existed {
            set_hidden(&local);
        }
        return Ok((local, false));
    }
    let fb = fallback_index_dir(library);
    std::fs::create_dir_all(&fb)?;
    Ok((fb, true))
}
