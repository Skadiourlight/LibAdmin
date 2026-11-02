pub mod bundled;
pub mod config;
pub mod covers;
pub mod djvu;
pub mod extract;
pub mod grouping;
pub mod index;
pub mod parser;
pub mod scanner;
pub mod storage;

use config::{Settings, BOOK_EXTS, SKIP_DIRS};
use covers::{check_cache_version, cover_path, forget_cover, CoverWorker};
use index::{BookFile, Category, LibraryIndex, Result, Work};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

struct OpenLib {
    name: String,
    root: PathBuf,
    index_dir: PathBuf,
    fallback: bool,
    covers_dir: PathBuf,
    index: LibraryIndex,
}

#[derive(Default)]
pub struct AppState {
    settings: Mutex<Settings>,
    lib: Mutex<Option<OpenLib>>,
    covers_dir: Mutex<Option<PathBuf>>,
    scanning: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    covers: OnceLock<CoverWorker>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    fn covers(&self) -> &CoverWorker {
        self.covers.get().expect("cover worker started in setup")
    }

    /// Runs a write operation on the open library inside a transaction.
    fn edit<T>(&self, f: impl FnOnce(&LibraryIndex) -> Result<T>) -> Result<T> {
        if self.scanning.load(Ordering::SeqCst) {
            return Err("正在扫描书库，请等扫描完成后再修改。".into());
        }
        let guard = lock(&self.lib);
        let lib = guard.as_ref().ok_or("还没有打开书库")?;
        lib.index.tx(f)
    }
}

// ---------------- DTOs ----------------
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StateDto {
    root: String,
    libraries: Vec<String>,
    library: Option<String>,
    index_dir: String,
    fallback: bool,
    settings: Settings,
    djvu_available: bool,
    pdf_available: bool,
    scanning: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileDto {
    #[serde(flatten)]
    file: BookFile,
    name: String,
    folder: String,
    ext: String,
    missing: bool,
    has_cover: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkDto {
    id: i64,
    title: String,
    title_en: String,
    title_zh: String,
    authors: Vec<String>,
    notes: String,
    category_id: Option<i64>,
    main_file_id: Option<i64>,
    title_manual: bool,
    removed: bool,
    user_created: bool,
    added_at: f64,
    has_cover: bool,
    files: Vec<FileDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryDto {
    works: Vec<WorkDto>,
    categories: Vec<Category>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    text: String,
    done: usize,
    total: usize,
}

fn list_libraries(root: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else { return vec![] };
    let mut out: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| !n.starts_with('.') && !SKIP_DIRS.contains(&n.as_str()))
        .collect();
    out.sort_by_cached_key(|n| parser::natural_key(n));
    out
}

fn state_dto(state: &AppState) -> StateDto {
    let settings = lock(&state.settings).clone();
    let lib = lock(&state.lib);
    StateDto {
        libraries: list_libraries(&settings.root),
        root: settings.root.clone(),
        library: lib.as_ref().map(|l| l.name.clone()),
        index_dir: lib.as_ref().map(|l| l.index_dir.to_string_lossy().to_string()).unwrap_or_default(),
        fallback: lib.as_ref().is_some_and(|l| l.fallback),
        settings,
        djvu_available: djvu::available(),
        pdf_available: extract::pdf_available(),
        scanning: state.scanning.load(Ordering::SeqCst),
    }
}

// ---------------- library lifecycle ----------------
fn wait_scan_stopped(state: &AppState) {
    state.cancel.store(true, Ordering::SeqCst);
    for _ in 0..600 {
        if !state.scanning.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn open_library_inner(app: &AppHandle, state: &AppState, name: &str) -> Result<()> {
    wait_scan_stopped(state);
    let root_dir = lock(&state.settings).root.clone();
    let root = scanner::canonical(&PathBuf::from(&root_dir).join(name));
    if !root.is_dir() {
        return Err(format!("找不到书库文件夹：{}", root.display()));
    }
    let (index_dir, fallback) = storage::resolve_index_dir(&root).map_err(|e| format!("无法创建索引：{e}"))?;
    let index = LibraryIndex::open(&index_dir.join("index.db"), &root)?;
    let covers_dir = index_dir.join("covers");
    check_cache_version(&covers_dir);
    state.covers().reset();
    *lock(&state.covers_dir) = Some(covers_dir.clone());
    *lock(&state.lib) = Some(OpenLib { name: name.to_string(), root, index_dir, fallback, covers_dir, index });
    {
        let mut s = lock(&state.settings);
        s.library = name.to_string();
        s.save();
    }
    start_scan(app, state)
}

fn start_scan(app: &AppHandle, state: &AppState) -> Result<()> {
    let (root, index_dir, covers_dir) = {
        let lib = lock(&state.lib);
        let lib = lib.as_ref().ok_or("还没有打开书库")?;
        (lib.root.clone(), lib.index_dir.clone(), lib.covers_dir.clone())
    };
    if state.scanning.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    state.cancel.store(false, Ordering::SeqCst);
    let app = app.clone();
    let (scanning, cancel) = (state.scanning.clone(), state.cancel.clone());
    std::thread::spawn(move || {
        let progress = |text: &str, done: usize, total: usize| {
            let _ = app.emit("scan-progress", Progress { text: text.into(), done, total });
        };
        let result = scanner::scan_library(&root, &index_dir, &progress, &cancel);
        scanning.store(false, Ordering::SeqCst);
        match result {
            Ok(r) => {
                for &id in &r.stale_file_ids {
                    forget_cover(&covers_dir, id);
                }
                app.state::<AppState>().covers().forget(&r.stale_file_ids);
                let _ = app.emit("scan-done", r);
            }
            Err(e) => {
                let _ = app.emit("scan-error", e);
            }
        }
    });
    Ok(())
}

// ---------------- commands: app ----------------
#[tauri::command]
fn get_state(state: State<AppState>) -> StateDto {
    state_dto(&state)
}

#[tauri::command]
async fn choose_root(app: AppHandle, state: State<'_, AppState>) -> Result<StateDto> {
    let start = lock(&state.settings).root.clone();
    let mut dialog = app.dialog().file().set_title("选择书库根目录（其下每个子文件夹是一个书库）");
    if !start.is_empty() {
        dialog = dialog.set_directory(&start);
    }
    let Some(picked) = dialog.blocking_pick_folder() else { return Ok(state_dto(&state)) };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    set_root_inner(&app, &state, path.to_string_lossy().to_string())?;
    Ok(state_dto(&state))
}

fn set_root_inner(app: &AppHandle, state: &AppState, root: String) -> Result<()> {
    wait_scan_stopped(state);
    *lock(&state.lib) = None;
    *lock(&state.covers_dir) = None;
    let libs = list_libraries(&root);
    let last = {
        let mut s = lock(&state.settings);
        s.root = root;
        s.save();
        s.library.clone()
    };
    let name = if libs.contains(&last) { Some(last) } else { libs.first().cloned() };
    match name {
        Some(n) => open_library_inner(app, state, &n),
        None => Ok(()),
    }
}

#[tauri::command]
fn init(app: AppHandle, state: State<AppState>) -> Result<StateDto> {
    if lock(&state.lib).is_none() {
        let root = lock(&state.settings).root.clone();
        let root = if !root.is_empty() && PathBuf::from(&root).is_dir() {
            root
        } else if config::default_root().is_dir() {
            config::default_root().to_string_lossy().to_string()
        } else {
            String::new()
        };
        if !root.is_empty() {
            set_root_inner(&app, &state, root)?;
        }
    }
    Ok(state_dto(&state))
}

#[tauri::command]
fn open_library(app: AppHandle, state: State<AppState>, name: String) -> Result<StateDto> {
    open_library_inner(&app, &state, &name)?;
    Ok(state_dto(&state))
}

#[tauri::command]
fn rescan(app: AppHandle, state: State<AppState>) -> Result<()> {
    start_scan(&app, &state)
}

#[tauri::command]
fn save_settings(
    state: State<AppState>,
    tile_width: f64,
    sort: String,
    theme: String,
    side_width: f64,
    inspect_width: f64,
    inspect_open: bool,
) {
    let mut s = lock(&state.settings);
    s.tile_width = tile_width.round().clamp(120.0, 260.0) as u32;
    s.sort = sort;
    s.theme = theme;
    s.side_width = side_width.round().clamp(180.0, 480.0) as u32;
    s.inspect_width = inspect_width.round().clamp(280.0, 720.0) as u32;
    s.inspect_open = inspect_open;
    s.save();
}

#[tauri::command]
fn get_library(state: State<AppState>) -> Result<LibraryDto> {
    let guard = lock(&state.lib);
    let lib = guard.as_ref().ok_or("还没有打开书库")?;
    let works = lib.index.load_works()?;
    let categories = lib.index.list_categories()?;

    let mut jobs = vec![];
    let dtos = works
        .into_iter()
        .map(|w: Work| {
            let main = w.main_file().map(|f| (f.id, f.abs_path(&lib.root)));
            let has_cover = main.as_ref().is_some_and(|(id, _)| cover_path(&lib.covers_dir, *id).exists());
            if let (Some(m), false, false) = (main, has_cover, w.removed) {
                jobs.push(m);
            }
            WorkDto {
                id: w.id,
                title: w.title,
                title_en: w.title_en,
                title_zh: w.title_zh,
                authors: w.authors,
                notes: w.notes,
                category_id: w.category_id,
                main_file_id: w.main_file_id,
                title_manual: w.title_manual,
                removed: w.removed,
                user_created: w.user_created,
                added_at: w.added_at,
                has_cover,
                files: w
                    .files
                    .into_iter()
                    .map(|f| {
                        let has_cover = cover_path(&lib.covers_dir, f.id).exists();
                        FileDto {
                            name: f.name().to_string(),
                            folder: f.folder().to_string(),
                            ext: f.ext(),
                            missing: f.external && !PathBuf::from(&f.rel_path).exists(),
                            has_cover,
                            file: f,
                        }
                    })
                    .collect(),
            }
        })
        .collect();
    state.covers().enqueue(&lib.covers_dir, jobs);
    Ok(LibraryDto { works: dtos, categories })
}

fn file_path(state: &AppState, file_id: i64) -> Result<PathBuf> {
    let guard = lock(&state.lib);
    let lib = guard.as_ref().ok_or("还没有打开书库")?;
    let f = lib.index.load_file(file_id)?.ok_or("文件不在索引中")?;
    Ok(f.abs_path(&lib.root))
}

#[tauri::command]
fn open_file(app: AppHandle, state: State<AppState>, file_id: i64) -> Result<()> {
    let path = file_path(&state, file_id)?;
    if !path.exists() {
        return Err(format!("找不到文件：{}\n可以点“重新扫描”更新索引。", path.display()));
    }
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| {
        format!("系统没有找到能打开此文件的默认程序（{e}）。DJVU 文件可安装 SumatraPDF 后再试。")
    })
}

#[tauri::command]
fn reveal_file(app: AppHandle, state: State<AppState>, file_id: i64) -> Result<()> {
    let path = file_path(&state, file_id)?;
    app.opener().reveal_item_in_dir(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_library_folder(app: AppHandle, state: State<AppState>) -> Result<()> {
    let root = lock(&state.lib).as_ref().map(|l| l.root.clone()).ok_or("还没有打开书库")?;
    app.opener().open_path(root.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

// ---------------- commands: works ----------------
#[tauri::command]
fn create_work(state: State<AppState>, title_en: String, title_zh: String, authors: Vec<String>, category_id: Option<i64>, notes: String) -> Result<i64> {
    let title_en = title_en.trim().to_string();
    let title_zh = title_zh.trim().to_string();
    if title_en.is_empty() && title_zh.is_empty() {
        return Err("英文名和中文名至少填一个".into());
    }
    state.edit(|ix| ix.create_user_work(&title_en, &title_zh, &authors, category_id, &notes))
}

#[tauri::command]
fn update_work(state: State<AppState>, id: i64, title_en: String, title_zh: String, authors: Vec<String>, notes: String, category_id: Option<i64>) -> Result<()> {
    let title_en = title_en.trim().to_string();
    let title_zh = title_zh.trim().to_string();
    if title_en.is_empty() && title_zh.is_empty() {
        return Err("英文名和中文名至少填一个".into());
    }
    state.edit(|ix| ix.update_work(id, &title_en, &title_zh, &authors, &notes, category_id))
}

#[tauri::command]
fn reset_work_title(state: State<AppState>, id: i64) -> Result<()> {
    state.edit(|ix| ix.reset_work_title(id))
}

#[tauri::command]
fn remove_works(state: State<AppState>, ids: Vec<i64>) -> Result<()> {
    state.edit(|ix| {
        for &id in &ids {
            ix.delete_empty_user_work(id)?;
        }
        ix.set_removed(&ids, true)
    })
}

#[tauri::command]
fn restore_works(state: State<AppState>, ids: Vec<i64>) -> Result<()> {
    state.edit(|ix| ix.set_removed(&ids, false))
}

#[tauri::command]
fn merge_works(state: State<AppState>, target: i64, others: Vec<i64>) -> Result<()> {
    state.edit(|ix| ix.merge_works(target, &others))
}

#[tauri::command]
fn split_file(state: State<AppState>, file_id: i64) -> Result<i64> {
    state.edit(|ix| ix.split_file(file_id))
}

#[tauri::command]
fn remove_file(state: State<AppState>, file_id: i64) -> Result<()> {
    state.edit(|ix| ix.remove_file(file_id))
}

#[tauri::command]
fn set_main_file(state: State<AppState>, work_id: i64, file_id: i64) -> Result<()> {
    state.edit(|ix| ix.set_main_file(work_id, file_id))
}

#[tauri::command]
fn set_role(state: State<AppState>, file_id: i64, role: String) -> Result<()> {
    if !config::ROLES.contains(&role.as_str()) {
        return Err("未知的角色".into());
    }
    state.edit(|ix| ix.set_role(file_id, &role))
}

#[tauri::command]
fn update_file(
    state: State<AppState>,
    file_id: i64,
    title: String,
    authors: Vec<String>,
    language: String,
    role: String,
) -> Result<()> {
    if !config::ROLES.contains(&role.as_str()) {
        return Err("未知的角色".into());
    }
    state.edit(|ix| ix.update_file_info(file_id, &title, &authors, &language, &role))
}

#[tauri::command]
fn reset_file_info(state: State<AppState>, file_id: i64) -> Result<()> {
    state.edit(|ix| ix.reset_file_info(file_id))
}

#[tauri::command]
fn ensure_covers(state: State<AppState>, file_ids: Vec<i64>) -> Result<()> {
    let guard = lock(&state.lib);
    let lib = guard.as_ref().ok_or("还没有打开书库")?;
    let mut jobs = vec![];
    for id in file_ids {
        if let Ok(Some(f)) = lib.index.load_file(id) {
            jobs.push((id, f.abs_path(&lib.root)));
        }
    }
    state.covers().enqueue(&lib.covers_dir, jobs);
    Ok(())
}

#[tauri::command]
fn regen_cover(state: State<AppState>, file_id: i64) -> Result<()> {
    let dir = lock(&state.covers_dir).clone().ok_or("还没有打开书库")?;
    forget_cover(&dir, file_id);
    state.covers().forget(&[file_id]);
    let path = file_path(&state, file_id)?;
    state.covers().enqueue(&dir, vec![(file_id, path)]);
    Ok(())
}

#[tauri::command]
fn regen_all_covers(state: State<AppState>) -> Result<usize> {
    let guard = lock(&state.lib);
    let lib = guard.as_ref().ok_or("还没有打开书库")?;
    let works = lib.index.load_works()?;
    let mut jobs = vec![];
    for w in works {
        for f in w.files {
            forget_cover(&lib.covers_dir, f.id);
            jobs.push((f.id, f.abs_path(&lib.root)));
        }
    }
    let ids: Vec<i64> = jobs.iter().map(|(id, _)| *id).collect();
    let dir = lib.covers_dir.clone();
    drop(guard);
    state.covers().forget(&ids);
    state.covers().enqueue(&dir, jobs);
    Ok(ids.len())
}

/// Lets the user pick book files and attaches them to an entry. Files inside the library are
/// referenced by relative path (and taken from whatever entry they were in); files outside it
/// are linked by absolute path. Nothing is copied.
#[tauri::command]
async fn add_files(app: AppHandle, state: State<'_, AppState>, work_id: i64) -> Result<usize> {
    if state.scanning.load(Ordering::SeqCst) {
        return Err("正在扫描书库，请等扫描完成后再修改。".into());
    }
    let root = lock(&state.lib).as_ref().map(|l| l.root.clone()).ok_or("还没有打开书库")?;
    let Some(picked) = app
        .dialog()
        .file()
        .set_title("选择要加入此条目的书籍文件（不会复制文件）")
        .set_directory(&root)
        .add_filter("书籍", BOOK_EXTS)
        .add_filter("所有文件", &["*"])
        .blocking_pick_files()
    else {
        return Ok(0);
    };
    let mut prepared = vec![];
    for p in picked {
        let Ok(path) = p.into_path() else { continue };
        let path = scanner::canonical(&path);
        let (rel, external) = scanner::rel_or_external(&root, &path);
        if !external && rel.split('/').next() == Some(config::INDEX_DIR_NAME) {
            continue;
        }
        let Some((size, mtime)) = scanner::stat(&path) else { continue };
        prepared.push((rel, external, size, mtime, path));
    }
    let mut metas = vec![];
    for (rel, external, size, mtime, path) in prepared {
        let existing = {
            let guard = lock(&state.lib);
            guard.as_ref().map(|l| l.index.find_file(&rel)).transpose()?.flatten()
        };
        let meta = if existing.is_none() { Some(scanner::file_metadata(&path)) } else { None };
        metas.push((rel, external, size, mtime, existing, meta));
    }
    state.edit(|ix| {
        let mut n = 0;
        let mut touched = std::collections::HashSet::new();
        for (rel, external, size, mtime, existing, meta) in &metas {
            let fid = match (existing, meta) {
                (Some((fid, old)), _) => {
                    if let Some(o) = old {
                        if *o == work_id {
                            continue;
                        }
                        touched.insert(*o);
                    }
                    *fid
                }
                (None, Some(m)) => ix.insert_file(&scanner::new_file(rel, *size, *mtime, m, *external))?,
                (None, None) => continue,
            };
            ix.assign_files(&[fid], work_id)?;
            n += 1;
        }
        ix.refresh_work(work_id)?;
        for o in touched {
            ix.refresh_work(o)?;
        }
        Ok(n)
    })
}

// ---------------- commands: categories ----------------
#[tauri::command]
fn create_category(state: State<AppState>, parent_id: Option<i64>, name: String) -> Result<i64> {
    state.edit(|ix| ix.create_category(parent_id, &name))
}

#[tauri::command]
fn rename_category(state: State<AppState>, id: i64, name: String) -> Result<()> {
    state.edit(|ix| ix.rename_category(id, &name))
}

#[tauri::command]
fn move_category(state: State<AppState>, id: i64, parent_id: Option<i64>, before_id: Option<i64>) -> Result<()> {
    state.edit(|ix| ix.move_category(id, parent_id, before_id))
}

#[tauri::command]
fn delete_category(state: State<AppState>, id: i64) -> Result<()> {
    state.edit(|ix| ix.delete_category(id))
}

#[tauri::command]
fn set_works_category(state: State<AppState>, ids: Vec<i64>, category_id: Option<i64>) -> Result<()> {
    state.edit(|ix| {
        for &id in &ids {
            ix.set_work_category(id, category_id)?;
        }
        Ok(())
    })
}

// ---------------- app ----------------
fn serve_cover(state: &AppState, path: &str) -> tauri::http::Response<Vec<u8>> {
    let id = path.trim_start_matches('/').split(['?', '/']).next().and_then(|s| s.parse::<i64>().ok());
    let dir = lock(&state.covers_dir).clone();
    let bytes = match (dir, id) {
        (Some(d), Some(id)) => std::fs::read(cover_path(&d, id)).ok(),
        _ => None,
    };
    let builder = tauri::http::Response::builder().header("Access-Control-Allow-Origin", "*");
    match bytes {
        Some(b) => builder.header("Content-Type", "image/jpeg").header("Cache-Control", "no-store").body(b),
        None => builder.status(404).body(vec![]),
    }
    .expect("valid response")
}

pub fn run() {
    let state = AppState { settings: Mutex::new(Settings::load()), ..Default::default() };
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .register_uri_scheme_protocol("cover", |ctx, request| {
            serve_cover(&ctx.app_handle().state::<AppState>(), request.uri().path())
        })
        .setup(|app| {
            let worker = CoverWorker::start(app.handle().clone());
            let _ = app.state::<AppState>().covers.set(worker);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            init,
            get_state,
            choose_root,
            open_library,
            rescan,
            save_settings,
            get_library,
            open_file,
            reveal_file,
            open_library_folder,
            create_work,
            update_work,
            reset_work_title,
            remove_works,
            restore_works,
            merge_works,
            split_file,
            remove_file,
            set_main_file,
            set_role,
            update_file,
            reset_file_info,
            regen_cover,
            regen_all_covers,
            ensure_covers,
            add_files,
            create_category,
            rename_category,
            move_category,
            delete_category,
            set_works_category,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LibAdmin");
}
