//! Scans a library into a throwaway index and prints the resulting entries and categories.
//! Usage: cargo run --example scan_dump -- <library folder>
use libadmin_lib::{index::LibraryIndex, scanner};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

fn main() {
    let lib = PathBuf::from(std::env::args().nth(1).expect("library folder"));
    let tmp = std::env::temp_dir().join(format!("libadmin_dump_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let t = std::time::Instant::now();
    let r = scanner::scan_library(&lib, &tmp, &|_, _, _| {}, &AtomicBool::new(false)).unwrap();
    println!("added {} in {:.1}s", r.added, t.elapsed().as_secs_f64());
    let ix = LibraryIndex::open(&tmp.join("index.db"), &lib).unwrap();
    let cats = ix.list_categories().unwrap();
    let by_id: HashMap<i64, _> = cats.iter().map(|c| (c.id, c)).collect();
    let path = |mut id: Option<i64>| {
        let mut parts = vec![];
        while let Some(c) = id.and_then(|i| by_id.get(&i)) {
            parts.push(c.name.clone());
            id = c.parent_id;
        }
        parts.reverse();
        if parts.is_empty() { "未分类".to_string() } else { parts.join("/") }
    };
    let mut works = ix.load_works().unwrap();
    works.sort_by_key(|w| (path(w.category_id), w.title.clone()));
    for w in &works {
        println!("[{}] {} | {}", path(w.category_id), w.title, w.authors.join(", "));
        if w.files.len() > 1 {
            for f in w.files.iter().filter(|f| f.role != "章节") {
                println!("      {} {} {}", f.role, f.language, f.name().chars().take(60).collect::<String>());
            }
        }
    }
    drop(ix);
    let _ = std::fs::remove_dir_all(&tmp);
}
