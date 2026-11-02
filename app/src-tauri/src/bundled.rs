//! Embeds cover-generation tools so a release build is a single portable exe.
//! Files are extracted once to `%LOCALAPPDATA%\LibAdmin\runtime\<version>\tools`.
use std::path::{Path, PathBuf};

macro_rules! tool_bytes {
    ($rel:literal) => {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tools/", $rel))
    };
}

const FILES: &[(&str, &[u8])] = &[
    ("pdfium/pdfium.dll", tool_bytes!("pdfium/pdfium.dll")),
    ("djvulibre/djvused.exe", tool_bytes!("djvulibre/djvused.exe")),
    ("djvulibre/ddjvu.exe", tool_bytes!("djvulibre/ddjvu.exe")),
    ("djvulibre/libdjvulibre.dll", tool_bytes!("djvulibre/libdjvulibre.dll")),
    ("djvulibre/libjpeg.dll", tool_bytes!("djvulibre/libjpeg.dll")),
    ("djvulibre/libtiff.dll", tool_bytes!("djvulibre/libtiff.dll")),
    ("djvulibre/libz.dll", tool_bytes!("djvulibre/libz.dll")),
];

fn runtime_root() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("LibAdmin").join("runtime").join(env!("CARGO_PKG_VERSION"))
}

fn write_if_needed(path: &Path, bytes: &[u8]) {
    if std::fs::metadata(path).map(|m| m.len() == bytes.len() as u64).unwrap_or(false) {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let tmp = path.with_extension("part");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

pub fn extract_tools() -> PathBuf {
    let root = runtime_root().join("tools");
    for (rel, bytes) in FILES {
        write_if_needed(&root.join(rel), bytes);
    }
    root
}
