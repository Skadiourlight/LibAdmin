//! Background cover rendering. `<id>.none` marks files that failed so they are not retried.
use crate::extract::{self, COVER_VERSION};
use crossbeam_channel::{unbounded, Sender};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

struct Job {
    generation: u64,
    file_id: i64,
    path: PathBuf,
    out_dir: PathBuf,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CoverReady {
    file_id: i64,
}

pub struct CoverWorker {
    tx: Sender<Job>,
    queued: Arc<Mutex<HashSet<i64>>>,
    generation: Arc<AtomicU64>,
    pending: Arc<AtomicUsize>,
}

pub fn cover_path(covers_dir: &Path, file_id: i64) -> PathBuf {
    covers_dir.join(format!("{file_id}.jpg"))
}

pub fn forget_cover(covers_dir: &Path, file_id: i64) {
    let _ = std::fs::remove_file(cover_path(covers_dir, file_id));
    let _ = std::fs::remove_file(covers_dir.join(format!("{file_id}.none")));
}

/// Drops caches made by an older renderer.
pub fn check_cache_version(covers_dir: &Path) {
    let _ = std::fs::create_dir_all(covers_dir);
    let marker = covers_dir.join("version.txt");
    if std::fs::read_to_string(&marker).is_ok_and(|v| v.trim() == COVER_VERSION) {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(covers_dir) {
        for e in entries.flatten() {
            let p = e.path();
            if matches!(p.extension().and_then(|x| x.to_str()), Some("jpg" | "none")) {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    let _ = std::fs::write(marker, COVER_VERSION);
}

impl CoverWorker {
    pub fn start(app: AppHandle) -> Self {
        let (tx, rx) = unbounded::<Job>();
        let queued: Arc<Mutex<HashSet<i64>>> = Default::default();
        let generation = Arc::new(AtomicU64::new(0));
        let pending = Arc::new(AtomicUsize::new(0));
        let (q, g, p) = (queued.clone(), generation.clone(), pending.clone());
        std::thread::spawn(move || {
            for job in rx {
                if job.generation != g.load(Ordering::SeqCst) {
                    continue;
                }
                let out = cover_path(&job.out_dir, job.file_id);
                let result = if out.exists() { Some(true) } else { extract::make_cover(&job.path, &out) };
                q.lock().unwrap_or_else(|e| e.into_inner()).remove(&job.file_id);
                if job.generation != g.load(Ordering::SeqCst) {
                    continue;
                }
                match result {
                    Some(true) => {
                        let _ = app.emit("cover-ready", CoverReady { file_id: job.file_id });
                    }
                    Some(false) => {
                        let _ = std::fs::write(job.out_dir.join(format!("{}.none", job.file_id)), b"");
                    }
                    None => {}
                }
                let left = p.fetch_sub(1, Ordering::SeqCst).saturating_sub(1);
                let _ = app.emit("cover-pending", left);
            }
        });
        Self { tx, queued, generation, pending }
    }

    /// Invalidates everything queued for the previous library.
    pub fn reset(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.queued.lock().unwrap_or_else(|e| e.into_inner()).clear();
        self.pending.store(0, Ordering::SeqCst);
    }

    pub fn forget(&self, ids: &[i64]) {
        let mut q = self.queued.lock().unwrap_or_else(|e| e.into_inner());
        for id in ids {
            q.remove(id);
        }
    }

    pub fn enqueue(&self, out_dir: &Path, items: Vec<(i64, PathBuf)>) {
        let generation = self.generation.load(Ordering::SeqCst);
        let mut q = self.queued.lock().unwrap_or_else(|e| e.into_inner());
        for (file_id, path) in items {
            if q.contains(&file_id)
                || cover_path(out_dir, file_id).exists()
                || out_dir.join(format!("{file_id}.none")).exists()
            {
                continue;
            }
            q.insert(file_id);
            self.pending.fetch_add(1, Ordering::SeqCst);
            let _ = self.tx.send(Job { generation, file_id, path, out_dir: out_dir.to_path_buf() });
        }
    }
}
