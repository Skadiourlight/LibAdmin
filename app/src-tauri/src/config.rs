use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const INDEX_DIR_NAME: &str = ".libadmin";
pub const BOOK_EXTS: &[&str] = &["pdf", "epub", "djvu", "djv", "mobi", "azw3", "fb2"];
pub const SKIP_DIRS: &[&str] = &[INDEX_DIR_NAME, "$RECYCLE.BIN", "System Volume Information"];
pub const ROLES: &[&str] = &["主本", "其他版本", "译本", "分卷", "章节", "配套资料"];

fn is_libadmin_home(dir: &Path) -> bool {
    dir.join("tools").join("pdfium").join("pdfium.dll").is_file()
}

/// Folder that holds `settings.json` and the fallback `library_data`.
/// A packaged exe uses its own folder. Debug builds may live in a cargo target
/// directory, so they walk upward only until they find this repo's pdfium.
pub fn app_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        if !cfg!(debug_assertions) {
            return exe_dir;
        }
        let mut starts = vec![exe_dir.clone()];
        starts.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        if let Ok(cwd) = std::env::current_dir() {
            starts.push(cwd);
        }
        starts
            .into_iter()
            .find_map(|s| s.ancestors().find(|d| is_libadmin_home(d)).map(Path::to_path_buf))
            .unwrap_or(exe_dir)
    })
}

/// Cover tools: a `tools` folder next to the app, or (in a portable release) files extracted from the exe.
pub fn tools_root() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let beside = app_dir().join("tools");
        if beside.join("pdfium").join("pdfium.dll").exists() {
            beside
        } else {
            crate::bundled::extract_tools()
        }
    })
}

pub fn settings_file() -> PathBuf {
    app_dir().join("settings.json")
}
pub fn fallback_data_dir() -> PathBuf {
    app_dir().join("library_data")
}
pub fn djvu_tools_dir() -> PathBuf {
    tools_root().join("djvulibre")
}
pub fn pdfium_dll() -> PathBuf {
    tools_root().join("pdfium").join("pdfium.dll")
}
pub fn default_root() -> PathBuf {
    app_dir().join("LibeAdmin书库")
}

pub fn is_book_ext(ext: &str) -> bool {
    BOOK_EXTS.contains(&ext.to_lowercase().as_str())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub root: String,
    pub library: String,
    #[serde(alias = "tile_width")]
    pub tile_width: u32,
    pub sort: String,
    pub theme: String,
    pub side_width: u32,
    pub inspect_width: u32,
    pub inspect_open: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            root: String::new(),
            library: String::new(),
            tile_width: 160,
            sort: "title".into(),
            theme: "system".into(),
            side_width: 260,
            inspect_width: 400,
            inspect_open: true,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(settings_file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(settings_file(), s);
        }
    }
}
