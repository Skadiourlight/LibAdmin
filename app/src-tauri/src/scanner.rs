//! Incremental scan of a library folder into its index. Never touches the book files themselves.
use crate::config::{is_book_ext, SKIP_DIRS};
use crate::index::{LibraryIndex, NewFile, Result};
use crate::{extract, grouping, parser};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
    pub moved: usize,
    pub stale_file_ids: Vec<i64>,
}

pub struct FileMeta {
    pub pages: Option<i64>,
    pub title: String,
    pub authors: Vec<String>,
    pub language: String,
    pub meta_title: String,
    pub meta_author: String,
}

pub fn file_metadata(path: &Path) -> FileMeta {
    let (pages, meta_title, meta_author) = extract::read_metadata(path);
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let (title, authors) = parser::resolve_title_authors(&stem, &meta_title, &meta_author);
    let language = parser::detect_language(&[&stem, &title, &meta_title]);
    FileMeta { pages, title, authors, language, meta_title, meta_author }
}

pub fn new_file<'a>(rel: &'a str, size: i64, mtime: f64, m: &'a FileMeta, external: bool) -> NewFile<'a> {
    NewFile {
        rel_path: rel,
        size,
        mtime,
        pages: m.pages,
        title: &m.title,
        authors: &m.authors,
        language: &m.language,
        meta_title: &m.meta_title,
        meta_author: &m.meta_author,
        external,
    }
}

pub fn stat(path: &Path) -> Option<(i64, f64)> {
    let md = std::fs::metadata(path).ok()?;
    let mtime = md
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_secs_f64() * 1000.0).round() / 1000.0)
        .unwrap_or(0.0);
    Some((md.len() as i64, mtime))
}

fn walk(dir: &Path, root: &Path, index_dir: &Path, out: &mut HashMap<String, (i64, f64)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) || path == index_dir {
                continue;
            }
            walk(&path, root, index_dir, out);
        } else if ft.is_file() {
            let ext = path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
            if name.starts_with("~$") || !is_book_ext(&ext) {
                continue;
            }
            if let (Some(st), Ok(rel)) = (stat(&path), path.strip_prefix(root)) {
                out.insert(rel.to_string_lossy().replace('\\', "/"), st);
            }
        }
    }
}

pub fn walk_books(root: &Path, index_dir: &Path) -> HashMap<String, (i64, f64)> {
    let mut out = HashMap::new();
    walk(root, root, index_dir, &mut out);
    out
}

/// Gives new auto-created works the category matching their folder (or 未分类 if none).
/// The very first scan of a library builds the category tree from its folder structure.
pub fn assign_categories(index: &LibraryIndex, created: &[i64]) -> Result<()> {
    let mut folders: Vec<(i64, String)> = vec![];
    for &wid in created {
        if let Some(f) = index.work_folder(wid)? {
            folders.push((wid, f));
        }
    }
    if !index.categories_seeded() {
        let all: Vec<String> = folders.iter().map(|(_, f)| f.clone()).collect();
        index.seed_categories(&all)?;
    }
    for (wid, folder) in folders {
        index.set_work_category(wid, index.category_for_folder(&folder)?)?;
    }
    Ok(())
}

pub fn scan_library(
    root: &Path,
    index_dir: &Path,
    progress: &dyn Fn(&str, usize, usize),
    cancel: &AtomicBool,
) -> Result<ScanResult> {
    let index = LibraryIndex::open(&index_dir.join("index.db"), root)?;
    let mut result = ScanResult::default();
    progress("正在扫描文件夹…", 0, 0);
    let disk = walk_books(root, index_dir);
    let db = index.file_stats()?;

    let mut added: Vec<String> = disk.keys().filter(|r| !db.contains_key(*r)).cloned().collect();
    let mut removed: Vec<String> = db.keys().filter(|r| !disk.contains_key(*r)).cloned().collect();
    let changed: Vec<String> = disk
        .iter()
        .filter(|(r, (size, mtime))| db.get(*r).is_some_and(|(_, s, m, _)| s != size || (m - mtime).abs() > 1.0))
        .map(|(r, _)| r.clone())
        .collect();
    added.sort_by_cached_key(|r| parser::natural_key(r));

    index.begin()?;
    let mut run = || -> Result<ScanResult> {
        let mut affected: HashSet<i64> = HashSet::new();
        // A vanished path plus a new path with the same file is a rename/move.
        let mut by_sig: HashMap<(i64, i64), Vec<String>> = HashMap::new();
        for r in &added {
            let (s, m) = disk[r];
            by_sig.entry((s, (m * 1000.0) as i64)).or_default().push(r.clone());
        }
        let mut moved_new: HashSet<String> = HashSet::new();
        let mut apply_move = |old_rel: &str, new_rel: &str| -> Result<bool> {
            let (fid, _, _, wid) = db[old_rel];
            let (size, mtime) = disk[new_rel];
            let m = file_metadata(&root.join(new_rel));
            index.relocate_file(fid, &new_file(new_rel, size, mtime, &m, false))?;
            if let Some(w) = wid {
                affected.insert(w);
            }
            Ok(true)
        };
        removed.retain(|r| {
            let (_, size, mtime, _) = db[r];
            let Some(cands) = by_sig.get_mut(&(size, (mtime * 1000.0) as i64)) else { return true };
            if cands.is_empty() {
                return true;
            }
            let base = r.rsplit('/').next().unwrap_or(r);
            let pos = cands.iter().position(|c| c.rsplit('/').next() == Some(base)).unwrap_or(0);
            let new_rel = cands.remove(pos);
            if apply_move(r, &new_rel).is_err() {
                return true;
            }
            moved_new.insert(new_rel);
            result.moved += 1;
            false
        });
        // Same size and only one leftover old/new path: still a rename (mtime sometimes changes).
        let mut add_by_size: HashMap<i64, Vec<String>> = HashMap::new();
        for r in &added {
            if moved_new.contains(r) {
                continue;
            }
            add_by_size.entry(disk[r].0).or_default().push(r.clone());
        }
        let mut rem_by_size: HashMap<i64, Vec<String>> = HashMap::new();
        for r in &removed {
            rem_by_size.entry(db[r].1).or_default().push(r.clone());
        }
        removed.retain(|r| {
            let size = db[r].1;
            let Some(olds) = rem_by_size.get(&size) else { return true };
            let Some(news) = add_by_size.get(&size) else { return true };
            if olds.len() != 1 || news.len() != 1 {
                return true;
            }
            let new_rel = news[0].clone();
            if moved_new.contains(&new_rel) || apply_move(r, &new_rel).is_err() {
                return true;
            }
            moved_new.insert(new_rel);
            result.moved += 1;
            false
        });
        added.retain(|r| !moved_new.contains(r));

        let total = changed.len() + added.len();
        let mut done = 0;
        for r in &changed {
            if cancel.load(Ordering::Relaxed) {
                return Err("已取消".into());
            }
            let (fid, _, _, wid) = db[r];
            let (size, mtime) = disk[r];
            let m = file_metadata(&root.join(r));
            index.update_file_metadata(fid, &new_file(r, size, mtime, &m, false))?;
            result.stale_file_ids.push(fid);
            affected.extend(wid);
            done += 1;
            progress("正在读取书籍信息…", done, total);
        }
        for r in &added {
            if cancel.load(Ordering::Relaxed) {
                return Err("已取消".into());
            }
            let (size, mtime) = disk[r];
            let m = file_metadata(&root.join(r));
            index.insert_file(&new_file(r, size, mtime, &m, false))?;
            done += 1;
            progress("正在读取书籍信息…", done, total);
            if done % 50 == 0 {
                index.commit_point()?;
            }
        }
        for r in &removed {
            let (fid, _, _, wid) = db[r];
            index.delete_file_row(fid)?;
            result.stale_file_ids.push(fid);
            affected.extend(wid);
        }

        progress("正在归并条目…", 0, 0);
        let grouped = grouping::group_new_files(&index)?;
        assign_categories(&index, &grouped.created)?;
        for wid in affected {
            index.refresh_work(wid)?;
        }
        index.delete_empty_auto_works()?;
        result.added = added.len();
        result.removed = removed.len();
        result.changed = changed.len();
        Ok(std::mem::take(&mut result))
    };
    match run() {
        Ok(r) => {
            index.commit()?;
            Ok(r)
        }
        Err(e) => {
            // Metadata already read is kept; grouping simply runs again next time.
            let _ = index.commit();
            Err(e)
        }
    }
}

pub fn rel_or_external(root: &Path, path: &Path) -> (String, bool) {
    match path.strip_prefix(root) {
        Ok(rel) => (rel.to_string_lossy().replace('\\', "/"), false),
        Err(_) => (path.to_string_lossy().to_string(), true),
    }
}

pub fn canonical(p: &Path) -> PathBuf {
    let c = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = c.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => c,
    }
}
