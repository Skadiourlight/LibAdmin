//! SQLite index of one library: works (entries shown as tiles), their files, and the category tree.
use crate::parser;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, String>;

fn db<T>(r: rusqlite::Result<T>) -> Result<T> {
    r.map_err(|e| format!("数据库错误：{e}"))
}

/// Same tables as the Python version (schema 1); `migrate` adds the schema 2 columns on top.
const SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS works(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL DEFAULT '',
    authors TEXT NOT NULL DEFAULT '[]',
    category TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    main_file_id INTEGER,
    title_manual INTEGER NOT NULL DEFAULT 0,
    category_manual INTEGER NOT NULL DEFAULT 0,
    main_manual INTEGER NOT NULL DEFAULT 0,
    added_at REAL NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS files(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    work_id INTEGER,
    rel_path TEXT NOT NULL UNIQUE,
    size INTEGER NOT NULL,
    mtime REAL NOT NULL,
    pages INTEGER,
    title TEXT NOT NULL DEFAULT '',
    authors TEXT NOT NULL DEFAULT '[]',
    language TEXT NOT NULL DEFAULT '',
    meta_title TEXT NOT NULL DEFAULT '',
    meta_author TEXT NOT NULL DEFAULT '',
    chapter_dir TEXT,
    role TEXT NOT NULL DEFAULT '',
    role_manual INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_files_work ON files(work_id);
CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS categories(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_id INTEGER,
    name TEXT NOT NULL,
    sort INTEGER NOT NULL DEFAULT 0,
    folder TEXT
);
";

pub fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookFile {
    pub id: i64,
    pub work_id: Option<i64>,
    pub rel_path: String,
    pub size: i64,
    pub mtime: f64,
    pub pages: Option<i64>,
    pub title: String,
    pub authors: Vec<String>,
    pub language: String,
    pub meta_title: String,
    pub meta_author: String,
    pub chapter_dir: Option<String>,
    pub role: String,
    pub role_manual: bool,
    pub external: bool,
    pub title_manual: bool,
}

impl BookFile {
    pub fn name(&self) -> &str {
        self.rel_path.rsplit(['/', '\\']).next().unwrap_or(&self.rel_path)
    }
    pub fn stem(&self) -> &str {
        let n = self.name();
        n.rsplit_once('.').map(|(s, _)| s).unwrap_or(n)
    }
    pub fn ext(&self) -> String {
        self.name().rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default()
    }
    /// Library-relative folder ("" for the library root and for external files).
    pub fn folder(&self) -> &str {
        if self.external {
            return "";
        }
        self.rel_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
    }
    pub fn volume(&self) -> Option<i32> {
        parser::volume_index(&[self.stem(), &self.title])
    }
    pub fn supplement(&self) -> bool {
        parser::is_supplement(self.stem())
    }
    pub fn abs_path(&self, root: &Path) -> PathBuf {
        if self.external { PathBuf::from(&self.rel_path) } else { root.join(&self.rel_path) }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Work {
    pub id: i64,
    pub title: String,
    pub title_en: String,
    pub title_zh: String,
    pub authors: Vec<String>,
    pub notes: String,
    pub category_id: Option<i64>,
    pub main_file_id: Option<i64>,
    pub title_manual: bool,
    pub main_manual: bool,
    pub removed: bool,
    pub user_created: bool,
    pub added_at: f64,
    pub files: Vec<BookFile>,
}

impl Work {
    pub fn main_file(&self) -> Option<&BookFile> {
        self.files.iter().find(|f| Some(f.id) == self.main_file_id).or(self.files.first())
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    pub sort: i64,
    pub folder: Option<String>,
}

fn parse_list(s: String) -> Vec<String> {
    serde_json::from_str(&s).unwrap_or_default()
}
fn to_json(v: &[String]) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "[]".into())
}

const FILE_COLS: &str = "id, work_id, rel_path, size, mtime, pages, title, authors, language, meta_title, meta_author, chapter_dir, role, role_manual, external, title_manual";
const WORK_COLS: &str = "id, title, title_en, title_zh, authors, notes, category_id, main_file_id, title_manual, main_manual, removed, user_created, added_at";

fn file_from_row(r: &Row) -> rusqlite::Result<BookFile> {
    Ok(BookFile {
        id: r.get(0)?,
        work_id: r.get(1)?,
        rel_path: r.get(2)?,
        size: r.get(3)?,
        mtime: r.get(4)?,
        pages: r.get(5)?,
        title: r.get(6)?,
        authors: parse_list(r.get(7)?),
        language: r.get(8)?,
        meta_title: r.get(9)?,
        meta_author: r.get(10)?,
        chapter_dir: r.get(11)?,
        role: r.get(12)?,
        role_manual: r.get::<_, i64>(13)? != 0,
        external: r.get::<_, i64>(14)? != 0,
        title_manual: r.get::<_, i64>(15)? != 0,
    })
}

fn work_from_row(r: &Row) -> rusqlite::Result<Work> {
    Ok(Work {
        id: r.get(0)?,
        title: r.get(1)?,
        title_en: r.get(2)?,
        title_zh: r.get(3)?,
        authors: parse_list(r.get(4)?),
        notes: r.get(5)?,
        category_id: r.get(6)?,
        main_file_id: r.get(7)?,
        title_manual: r.get::<_, i64>(8)? != 0,
        main_manual: r.get::<_, i64>(9)? != 0,
        removed: r.get::<_, i64>(10)? != 0,
        user_created: r.get::<_, i64>(11)? != 0,
        added_at: r.get(12)?,
        files: vec![],
    })
}

pub fn file_sort_key(f: &BookFile) -> (bool, bool, i32, (i32, i32), Vec<parser::NatPart>) {
    (
        f.supplement(),
        f.chapter_dir.is_some(),
        f.volume().unwrap_or(0),
        if f.chapter_dir.is_some() { parser::chapter_order(f.stem()) } else { (0, 0) },
        parser::natural_key(&f.rel_path),
    )
}

fn sort_files(w: &mut Work) {
    w.files.sort_by_cached_key(file_sort_key);
    if let Some(main) = w.main_file_id {
        w.files.sort_by_key(|f| f.id != main);
    }
}

pub struct NewFile<'a> {
    pub rel_path: &'a str,
    pub size: i64,
    pub mtime: f64,
    pub pages: Option<i64>,
    pub title: &'a str,
    pub authors: &'a [String],
    pub language: &'a str,
    pub meta_title: &'a str,
    pub meta_author: &'a str,
    pub external: bool,
}

pub struct LibraryIndex {
    pub conn: Connection,
    pub root: PathBuf,
}

impl LibraryIndex {
    pub fn open(db_path: &Path, root: &Path) -> Result<Self> {
        let conn = db(Connection::open(db_path))?;
        db(conn.busy_timeout(std::time::Duration::from_secs(30)))?;
        db(conn.pragma_update(None, "journal_mode", "WAL"))?;
        let idx = Self { conn, root: root.to_path_buf() };
        idx.migrate()?;
        Ok(idx)
    }

    fn meta_get(&self, key: &str) -> Option<String> {
        self.conn.query_row("SELECT value FROM meta WHERE key=?", [key], |r| r.get(0)).optional().ok().flatten()
    }
    fn meta_set(&self, key: &str, value: &str) -> Result<()> {
        db(self.conn.execute("INSERT OR REPLACE INTO meta(key, value) VALUES(?, ?)", [key, value])).map(|_| ())
    }

    fn has_column(&self, table: &str, col: &str) -> bool {
        let mut stmt = match self.conn.prepare(&format!("PRAGMA table_info({table})")) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let cols: Vec<String> = stmt.query_map([], |r| r.get(1)).map(|it| it.flatten().collect()).unwrap_or_default();
        cols.iter().any(|c| c == col)
    }

    fn migrate(&self) -> Result<()> {
        db(self.conn.execute_batch(SCHEMA_V1))?;
        let version: i64 = self.meta_get("schema_version").and_then(|v| v.parse().ok()).unwrap_or(1);
        if version < 2 {
            for (table, col, ddl) in [
                ("works", "category_id", "INTEGER"),
                ("works", "removed", "INTEGER NOT NULL DEFAULT 0"),
                ("works", "user_created", "INTEGER NOT NULL DEFAULT 0"),
                ("files", "external", "INTEGER NOT NULL DEFAULT 0"),
            ] {
                if !self.has_column(table, col) {
                    db(self.conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {col} {ddl}")))?;
                }
            }
            // Python-era indexes stored the category as a "A/B" path string.
            let old: Vec<(i64, String)> = {
                let mut stmt = db(self.conn.prepare("SELECT id, category FROM works WHERE category != ''"))?;
                let rows = db(stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))))?;
                rows.flatten().collect()
            };
            for (wid, path) in &old {
                let cid = self.ensure_category_path(path)?;
                db(self.conn.execute("UPDATE works SET category_id=? WHERE id=?", params![cid, wid]))?;
            }
            let any_works: i64 = db(self.conn.query_row("SELECT COUNT(*) FROM works", [], |r| r.get(0)))?;
            if any_works > 0 {
                self.meta_set("categories_seeded", "1")?;
            }
            self.meta_set("schema_version", "2")?;
        }
        if version < 3 {
            for col in ["title_en", "title_zh"] {
                if !self.has_column("works", col) {
                    db(self.conn.execute_batch(&format!("ALTER TABLE works ADD COLUMN {col} TEXT NOT NULL DEFAULT ''")))?;
                }
            }
            let rows: Vec<(i64, String)> = {
                let mut stmt = db(self.conn.prepare("SELECT id, title FROM works"))?;
                let mapped = db(stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))))?;
                mapped.flatten().collect()
            };
            for (id, title) in rows {
                let (en, zh) = parser::collect_lang_titles([title.as_str()]);
                db(self.conn.execute("UPDATE works SET title_en=?, title_zh=? WHERE id=?", params![en, zh, id]))?;
            }
            let auto: Vec<i64> = {
                let mut stmt = db(self.conn.prepare("SELECT id FROM works WHERE title_manual=0"))?;
                let mapped = db(stmt.query_map([], |r| r.get(0)))?;
                mapped.flatten().collect()
            };
            for id in auto {
                self.refresh_work(id)?;
            }
            self.meta_set("schema_version", "3")?;
        }
        if version < 4 {
            if !self.has_column("files", "title_manual") {
                db(self.conn.execute_batch("ALTER TABLE files ADD COLUMN title_manual INTEGER NOT NULL DEFAULT 0"))?;
            }
            self.meta_set("schema_version", "4")?;
        }
        Ok(())
    }

    pub fn commit_point(&self) -> Result<()> {
        db(self.conn.execute_batch("COMMIT; BEGIN"))
    }
    pub fn begin(&self) -> Result<()> {
        db(self.conn.execute_batch("BEGIN"))
    }
    pub fn commit(&self) -> Result<()> {
        db(self.conn.execute_batch("COMMIT"))
    }
    pub fn rollback(&self) {
        let _ = self.conn.execute_batch("ROLLBACK");
    }

    /// Runs `f` in a transaction.
    pub fn tx<T>(&self, f: impl FnOnce(&Self) -> Result<T>) -> Result<T> {
        self.begin()?;
        match f(self) {
            Ok(v) => {
                self.commit()?;
                Ok(v)
            }
            Err(e) => {
                self.rollback();
                Err(e)
            }
        }
    }

    // ---------------- reading ----------------
    pub fn load_works(&self) -> Result<Vec<Work>> {
        let mut works: HashMap<i64, Work> = HashMap::new();
        {
            let mut stmt = db(self.conn.prepare(&format!("SELECT {WORK_COLS} FROM works")))?;
            for w in db(stmt.query_map([], work_from_row))?.flatten() {
                works.insert(w.id, w);
            }
        }
        {
            let mut stmt = db(self.conn.prepare(&format!("SELECT {FILE_COLS} FROM files WHERE work_id IS NOT NULL")))?;
            for f in db(stmt.query_map([], file_from_row))?.flatten() {
                if let Some(w) = f.work_id.and_then(|id| works.get_mut(&id)) {
                    w.files.push(f);
                }
            }
        }
        let mut out: Vec<Work> =
            works.into_values().filter(|w| !w.files.is_empty() || w.user_created).collect();
        for w in out.iter_mut() {
            sort_files(w);
        }
        Ok(out)
    }

    pub fn load_work(&self, id: i64) -> Result<Option<Work>> {
        let w = db(self
            .conn
            .query_row(&format!("SELECT {WORK_COLS} FROM works WHERE id=?"), [id], work_from_row)
            .optional())?;
        let Some(mut w) = w else { return Ok(None) };
        let mut stmt = db(self.conn.prepare(&format!("SELECT {FILE_COLS} FROM files WHERE work_id=?")))?;
        w.files = db(stmt.query_map([id], file_from_row))?.flatten().collect();
        sort_files(&mut w);
        Ok(Some(w))
    }

    pub fn load_file(&self, id: i64) -> Result<Option<BookFile>> {
        db(self.conn.query_row(&format!("SELECT {FILE_COLS} FROM files WHERE id=?"), [id], file_from_row).optional())
    }

    /// rel_path -> (id, size, mtime, work_id) for files inside the library.
    pub fn file_stats(&self) -> Result<HashMap<String, (i64, i64, f64, Option<i64>)>> {
        let mut stmt = db(self.conn.prepare("SELECT id, rel_path, size, mtime, work_id FROM files WHERE external=0"))?;
        let rows = db(stmt.query_map([], |r| Ok((r.get::<_, String>(1)?, (r.get(0)?, r.get(2)?, r.get(3)?, r.get(4)?)))))?;
        Ok(rows.flatten().collect())
    }

    pub fn find_file(&self, rel_path: &str) -> Result<Option<(i64, Option<i64>)>> {
        db(self
            .conn
            .query_row("SELECT id, work_id FROM files WHERE rel_path=?", [rel_path], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional())
    }

    pub fn files_without_work(&self) -> Result<Vec<BookFile>> {
        let mut stmt = db(self.conn.prepare(&format!("SELECT {FILE_COLS} FROM files WHERE work_id IS NULL")))?;
        let rows = db(stmt.query_map([], file_from_row))?;
        Ok(rows.flatten().collect())
    }

    pub fn folder_has_grouped_files(&self, folder: &str) -> Result<bool> {
        let mut stmt = db(self.conn.prepare("SELECT rel_path FROM files WHERE work_id IS NOT NULL AND external=0 AND rel_path LIKE ? ESCAPE '\\'"))?;
        let like = folder.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_") + "/%";
        let rows: Vec<String> = db(stmt.query_map([like], |r| r.get(0)))?.flatten().collect();
        Ok(rows.iter().any(|p| p.rsplit_once('/').map(|(d, _)| d) == Some(folder)))
    }

    // ---------------- file rows ----------------
    pub fn insert_file(&self, f: &NewFile) -> Result<i64> {
        db(self.conn.execute(
            "INSERT INTO files(rel_path,size,mtime,pages,title,authors,language,meta_title,meta_author,external) VALUES(?,?,?,?,?,?,?,?,?,?)",
            params![f.rel_path, f.size, f.mtime, f.pages, f.title, to_json(f.authors), f.language, f.meta_title, f.meta_author, f.external as i64],
        ))?;
        Ok(self.conn.last_insert_rowid())
    }

    fn file_title_manual(&self, id: i64) -> Result<bool> {
        Ok(db(self.conn.query_row("SELECT title_manual FROM files WHERE id=?", [id], |r| r.get::<_, i64>(0)))? != 0)
    }

    pub fn update_file_metadata(&self, id: i64, f: &NewFile) -> Result<()> {
        if self.file_title_manual(id)? {
            db(self.conn.execute(
                "UPDATE files SET size=?,mtime=?,pages=?,meta_title=?,meta_author=? WHERE id=?",
                params![f.size, f.mtime, f.pages, f.meta_title, f.meta_author, id],
            ))
            .map(|_| ())
        } else {
            db(self.conn.execute(
                "UPDATE files SET size=?,mtime=?,pages=?,title=?,authors=?,language=?,meta_title=?,meta_author=? WHERE id=?",
                params![f.size, f.mtime, f.pages, f.title, to_json(f.authors), f.language, f.meta_title, f.meta_author, id],
            ))
            .map(|_| ())
        }
    }

    /// Keeps the same file row (and its work) when the book was renamed or moved on disk.
    pub fn relocate_file(&self, id: i64, f: &NewFile) -> Result<()> {
        if self.file_title_manual(id)? {
            db(self.conn.execute(
                "UPDATE files SET rel_path=?,size=?,mtime=?,pages=?,meta_title=?,meta_author=?,external=? WHERE id=?",
                params![f.rel_path, f.size, f.mtime, f.pages, f.meta_title, f.meta_author, f.external as i64, id],
            ))
            .map(|_| ())
        } else {
            db(self.conn.execute(
                "UPDATE files SET rel_path=?,size=?,mtime=?,pages=?,title=?,authors=?,language=?,meta_title=?,meta_author=?,external=? WHERE id=?",
                params![f.rel_path, f.size, f.mtime, f.pages, f.title, to_json(f.authors), f.language, f.meta_title, f.meta_author, f.external as i64, id],
            ))
            .map(|_| ())
        }
    }

    pub fn update_file_info(&self, id: i64, title: &str, authors: &[String], language: &str, role: &str) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err("请填写名称".into());
        }
        db(self.conn.execute(
            "UPDATE files SET title=?, authors=?, language=?, role=?, role_manual=1, title_manual=1 WHERE id=?",
            params![title, to_json(authors), language.trim(), role, id],
        ))?;
        if let Some(f) = self.load_file(id)? {
            if let Some(wid) = f.work_id {
                self.refresh_work(wid)?;
            }
        }
        Ok(())
    }

    pub fn reset_file_info(&self, id: i64) -> Result<()> {
        let Some(f) = self.load_file(id)? else {
            return Err("没有这个文件".into());
        };
        let (title, authors) = parser::resolve_title_authors(f.stem(), &f.meta_title, &f.meta_author);
        let language = parser::detect_language(&[f.stem(), &title, &f.meta_title]);
        db(self.conn.execute(
            "UPDATE files SET title=?, authors=?, language=?, title_manual=0 WHERE id=?",
            params![title, to_json(&authors), language, id],
        ))?;
        if let Some(wid) = f.work_id {
            self.refresh_work(wid)?;
        }
        Ok(())
    }
    pub fn delete_file_row(&self, id: i64) -> Result<()> {
        db(self.conn.execute("DELETE FROM files WHERE id=?", [id])).map(|_| ())
    }
    pub fn set_chapter_dir(&self, ids: &[i64], dir: &str) -> Result<()> {
        for id in ids {
            db(self.conn.execute("UPDATE files SET chapter_dir=? WHERE id=?", params![dir, id]))?;
        }
        Ok(())
    }
    pub fn assign_files(&self, ids: &[i64], work_id: i64) -> Result<()> {
        for id in ids {
            db(self.conn.execute("UPDATE files SET work_id=? WHERE id=?", params![work_id, id]))?;
        }
        Ok(())
    }

    // ---------------- works ----------------
    pub fn create_auto_work(&self) -> Result<i64> {
        db(self.conn.execute("INSERT INTO works(added_at) VALUES(?)", [now()]))?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Recomputes everything about a work that the user has not set by hand.
    pub fn refresh_work(&self, work_id: i64) -> Result<()> {
        let Some(w) = self.load_work(work_id)? else { return Ok(()) };
        if w.files.is_empty() {
            if !w.user_created {
                db(self.conn.execute("DELETE FROM works WHERE id=?", [work_id]))?;
            }
            return Ok(());
        }
        let mut files = w.files.clone();
        files.sort_by_cached_key(file_sort_key);
        let main = files
            .iter()
            .find(|f| w.main_manual && Some(f.id) == w.main_file_id)
            .unwrap_or(&files[0])
            .clone();
        let chapter_dirs: HashSet<Option<&str>> = files.iter().map(|f| f.chapter_dir.as_deref()).collect();
        let pure_chapter = !chapter_dirs.contains(&None) && chapter_dirs.len() == 1;
        let multi_volume = files.iter().filter(|f| f.volume().is_some() && f.chapter_dir.is_none()).count() >= 2;

        for f in &files {
            if f.role_manual {
                continue;
            }
            let role = if files.len() == 1 {
                "主本"
            } else if f.chapter_dir.is_some() {
                "章节"
            } else if f.id == main.id {
                if multi_volume && f.volume().is_some() { "分卷" } else { "主本" }
            } else if f.supplement() {
                "配套资料"
            } else if multi_volume && f.volume().is_some() {
                "分卷"
            } else if f.language != main.language {
                "译本"
            } else {
                "其他版本"
            };
            if role != f.role {
                db(self.conn.execute("UPDATE files SET role=? WHERE id=?", params![role, f.id]))?;
            }
        }

        let (mut title, mut title_en, mut title_zh, mut authors) =
            (w.title.clone(), w.title_en.clone(), w.title_zh.clone(), w.authors.clone());
        if !w.title_manual {
            if pure_chapter {
                let dir = main.chapter_dir.clone().unwrap_or_default();
                title = dir.rsplit('/').next().unwrap_or(&dir).to_string();
                (title_en, title_zh) = parser::collect_lang_titles([title.as_str()]);
                let mut counts: HashMap<Vec<String>, usize> = HashMap::new();
                for f in files.iter().filter(|f| !f.authors.is_empty()) {
                    *counts.entry(f.authors.clone()).or_default() += 1;
                }
                authors = counts.into_iter().filter(|(_, n)| *n >= 2).max_by_key(|(_, n)| *n).map(|(a, _)| a).unwrap_or_default();
            } else {
                let names: Vec<String> = files
                    .iter()
                    .map(|f| if multi_volume { parser::strip_volume(&f.title) } else { f.title.clone() })
                    .collect();
                (title_en, title_zh) = parser::collect_lang_titles(names.iter().map(|s| s.as_str()));
                title = parser::display_title(&title_en, &title_zh);
                authors = if !main.authors.is_empty() {
                    main.authors.clone()
                } else {
                    files.iter().find(|f| !f.authors.is_empty()).map(|f| f.authors.clone()).unwrap_or_default()
                };
            }
        }
        db(self.conn.execute(
            "UPDATE works SET title=?, title_en=?, title_zh=?, authors=?, main_file_id=? WHERE id=?",
            params![title, title_en, title_zh, to_json(&authors), main.id, work_id],
        ))
        .map(|_| ())
    }

    /// Library folder a work belongs to, used to pick its default category.
    pub fn work_folder(&self, work_id: i64) -> Result<Option<String>> {
        let Some(w) = self.load_work(work_id)? else { return Ok(None) };
        Ok(w.main_file().map(|f| match &f.chapter_dir {
            Some(d) => d.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default(),
            None => f.folder().to_string(),
        }))
    }

    pub fn set_work_category(&self, work_id: i64, category_id: Option<i64>) -> Result<()> {
        db(self.conn.execute("UPDATE works SET category_id=? WHERE id=?", params![category_id, work_id])).map(|_| ())
    }

    pub fn create_user_work(&self, title_en: &str, title_zh: &str, authors: &[String], category_id: Option<i64>, notes: &str) -> Result<i64> {
        let title = parser::display_title(title_en, title_zh);
        db(self.conn.execute(
            "INSERT INTO works(title, title_en, title_zh, authors, notes, category_id, title_manual, user_created, added_at) VALUES(?,?,?,?,?,?,1,1,?)",
            params![title, title_en, title_zh, to_json(authors), notes, category_id, now()],
        ))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_work(&self, id: i64, title_en: &str, title_zh: &str, authors: &[String], notes: &str, category_id: Option<i64>) -> Result<()> {
        let title = parser::display_title(title_en, title_zh);
        let old: Option<(String, String, String)> = db(self
            .conn
            .query_row("SELECT title_en, title_zh, authors FROM works WHERE id=?", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional())?;
        db(self.conn.execute("UPDATE works SET notes=?, category_id=? WHERE id=?", params![notes, category_id, id]))?;
        let changed = match old {
            Some((en, zh, a)) => en != title_en || zh != title_zh || parse_list(a) != authors,
            None => true,
        };
        if changed {
            db(self.conn.execute(
                "UPDATE works SET title=?, title_en=?, title_zh=?, authors=?, title_manual=1 WHERE id=?",
                params![title, title_en, title_zh, to_json(authors), id],
            ))?;
        }
        Ok(())
    }

    pub fn reset_work_title(&self, id: i64) -> Result<()> {
        db(self.conn.execute("UPDATE works SET title_manual=0 WHERE id=? AND user_created=0", [id]))?;
        self.refresh_work(id)
    }

    pub fn set_main_file(&self, work_id: i64, file_id: i64) -> Result<()> {
        db(self.conn.execute("UPDATE works SET main_file_id=?, main_manual=1 WHERE id=?", params![file_id, work_id]))?;
        db(self.conn.execute(
            "UPDATE files SET role_manual=0 WHERE work_id=? AND (id=? OR role='主本')",
            params![work_id, file_id],
        ))?;
        self.refresh_work(work_id)
    }

    pub fn set_role(&self, file_id: i64, role: &str) -> Result<()> {
        db(self.conn.execute("UPDATE files SET role=?, role_manual=1 WHERE id=?", params![role, file_id])).map(|_| ())
    }

    pub fn merge_works(&self, target: i64, others: &[i64]) -> Result<()> {
        let target_cat: Option<i64> =
            db(self.conn.query_row("SELECT category_id FROM works WHERE id=?", [target], |r| r.get(0)).optional())?.flatten();
        for &oid in others.iter().filter(|&&o| o != target) {
            if target_cat.is_none() {
                db(self.conn.execute(
                    "UPDATE works SET category_id=(SELECT category_id FROM works WHERE id=?) WHERE id=? AND category_id IS NULL",
                    params![oid, target],
                ))?;
            }
            db(self.conn.execute("UPDATE files SET work_id=?, role_manual=0 WHERE work_id=?", params![target, oid]))?;
            db(self.conn.execute("DELETE FROM works WHERE id=?", [oid]))?;
        }
        self.refresh_work(target)
    }

    /// Moves one file out into a new entry that keeps the old entry's category.
    pub fn split_file(&self, file_id: i64) -> Result<i64> {
        let old: Option<i64> =
            db(self.conn.query_row("SELECT work_id FROM files WHERE id=?", [file_id], |r| r.get(0)).optional())?.flatten();
        let cat: Option<i64> = match old {
            Some(o) => db(self.conn.query_row("SELECT category_id FROM works WHERE id=?", [o], |r| r.get(0)).optional())?.flatten(),
            None => None,
        };
        let new_id = self.create_auto_work()?;
        db(self.conn.execute("UPDATE works SET category_id=? WHERE id=?", params![cat, new_id]))?;
        db(self.conn.execute(
            "UPDATE files SET work_id=?, role_manual=0, chapter_dir=NULL WHERE id=?",
            params![new_id, file_id],
        ))?;
        self.refresh_work(new_id)?;
        if let Some(o) = old {
            self.refresh_work(o)?;
        }
        Ok(new_id)
    }

    pub fn set_removed(&self, ids: &[i64], removed: bool) -> Result<()> {
        for id in ids {
            db(self.conn.execute("UPDATE works SET removed=? WHERE id=?", params![removed as i64, id]))?;
        }
        Ok(())
    }

    /// Takes a file out of its entry. Library files become a hidden (removed) entry so rescans
    /// don't bring them back; linked external files are simply forgotten.
    pub fn remove_file(&self, file_id: i64) -> Result<()> {
        let Some(f) = self.load_file(file_id)? else { return Ok(()) };
        if f.external {
            self.delete_file_row(file_id)?;
            if let Some(w) = f.work_id {
                self.refresh_work(w)?;
            }
            return Ok(());
        }
        let new_id = self.split_file(file_id)?;
        self.set_removed(&[new_id], true)
    }

    /// Permanently deletes user-created entries that have no files.
    pub fn delete_empty_user_work(&self, id: i64) -> Result<()> {
        db(self.conn.execute(
            "DELETE FROM works WHERE id=? AND user_created=1 AND NOT EXISTS (SELECT 1 FROM files WHERE work_id=?)",
            params![id, id],
        ))
        .map(|_| ())
    }

    pub fn delete_empty_auto_works(&self) -> Result<()> {
        db(self.conn.execute(
            "DELETE FROM works WHERE user_created=0 AND NOT EXISTS (SELECT 1 FROM files WHERE files.work_id = works.id)",
            [],
        ))
        .map(|_| ())
    }

    // ---------------- categories ----------------
    pub fn list_categories(&self) -> Result<Vec<Category>> {
        let mut stmt = db(self.conn.prepare("SELECT id, parent_id, name, sort, folder FROM categories ORDER BY sort, id"))?;
        let rows = db(stmt.query_map([], |r| {
            Ok(Category { id: r.get(0)?, parent_id: r.get(1)?, name: r.get(2)?, sort: r.get(3)?, folder: r.get(4)? })
        }))?;
        Ok(rows.flatten().collect())
    }

    fn find_child(&self, parent: Option<i64>, name: &str) -> Result<Option<i64>> {
        db(self
            .conn
            .query_row("SELECT id FROM categories WHERE parent_id IS ? AND name=?", params![parent, name], |r| r.get(0))
            .optional())
    }

    fn next_sort(&self, parent: Option<i64>) -> Result<i64> {
        db(self.conn.query_row("SELECT COALESCE(MAX(sort), 0) + 1 FROM categories WHERE parent_id IS ?", [parent], |r| r.get(0)))
    }

    /// Ensures the "A/B/C" path exists; nodes that correspond to real library folders remember it.
    pub fn ensure_category_path(&self, path: &str) -> Result<Option<i64>> {
        let mut parent: Option<i64> = None;
        let mut prefix = String::new();
        for part in path.split('/').filter(|p| !p.is_empty()) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            parent = Some(match self.find_child(parent, part)? {
                Some(id) => id,
                None => {
                    let folder = self.root.join(&prefix).is_dir().then(|| prefix.clone());
                    let sort = self.next_sort(parent)?;
                    db(self.conn.execute(
                        "INSERT INTO categories(parent_id, name, sort, folder) VALUES(?,?,?,?)",
                        params![parent, part, sort, folder],
                    ))?;
                    self.conn.last_insert_rowid()
                }
            });
        }
        Ok(parent)
    }

    pub fn categories_seeded(&self) -> bool {
        self.meta_get("categories_seeded").is_some()
    }

    pub fn seed_categories(&self, folders: &[String]) -> Result<()> {
        let mut sorted: Vec<&String> = folders.iter().filter(|f| !f.is_empty()).collect();
        sorted.sort_by_key(|f| parser::natural_key(f));
        sorted.dedup();
        for f in sorted {
            self.ensure_category_path(f)?;
        }
        self.meta_set("categories_seeded", "1")
    }

    pub fn category_for_folder(&self, folder: &str) -> Result<Option<i64>> {
        if folder.is_empty() {
            return Ok(None);
        }
        db(self.conn.query_row("SELECT id FROM categories WHERE folder=? ORDER BY id LIMIT 1", [folder], |r| r.get(0)).optional())
    }

    pub fn create_category(&self, parent: Option<i64>, name: &str) -> Result<i64> {
        let name = name.trim();
        if name.is_empty() {
            return Err("类别名称不能为空".into());
        }
        if self.find_child(parent, name)?.is_some() {
            return Err(format!("同一层级下已有“{name}”"));
        }
        let sort = self.next_sort(parent)?;
        db(self.conn.execute("INSERT INTO categories(parent_id, name, sort) VALUES(?,?,?)", params![parent, name, sort]))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn rename_category(&self, id: i64, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err("类别名称不能为空".into());
        }
        let parent: Option<i64> = db(self.conn.query_row("SELECT parent_id FROM categories WHERE id=?", [id], |r| r.get(0)))?;
        if let Some(other) = self.find_child(parent, name)? {
            if other != id {
                return Err(format!("同一层级下已有“{name}”"));
            }
        }
        db(self.conn.execute("UPDATE categories SET name=? WHERE id=?", params![name, id])).map(|_| ())
    }

    fn subtree(&self, id: i64) -> Result<Vec<i64>> {
        let cats = self.list_categories()?;
        let mut out = vec![id];
        let mut i = 0;
        while i < out.len() {
            let cur = out[i];
            out.extend(cats.iter().filter(|c| c.parent_id == Some(cur)).map(|c| c.id));
            i += 1;
        }
        Ok(out)
    }

    /// Moves a category under a new parent (None = top level), placed before `before` if given.
    pub fn move_category(&self, id: i64, parent: Option<i64>, before: Option<i64>) -> Result<()> {
        if let Some(p) = parent {
            if self.subtree(id)?.contains(&p) {
                return Err("不能把类别移动到它自己的子类别里".into());
            }
        }
        let name: String = db(self.conn.query_row("SELECT name FROM categories WHERE id=?", [id], |r| r.get(0)))?;
        if let Some(other) = self.find_child(parent, &name)? {
            if other != id {
                return Err(format!("目标位置已有同名类别“{name}”"));
            }
        }
        let siblings: Vec<i64> = self
            .list_categories()?
            .into_iter()
            .filter(|c| c.parent_id == parent && c.id != id)
            .map(|c| c.id)
            .collect();
        let pos = before.and_then(|b| siblings.iter().position(|&s| s == b)).unwrap_or(siblings.len());
        let mut order = siblings;
        order.insert(pos, id);
        db(self.conn.execute("UPDATE categories SET parent_id=? WHERE id=?", params![parent, id]))?;
        for (i, cid) in order.iter().enumerate() {
            db(self.conn.execute("UPDATE categories SET sort=? WHERE id=?", params![i as i64 + 1, cid]))?;
        }
        Ok(())
    }

    /// Deletes a category and its sub-categories; their entries go back to 未分类.
    pub fn delete_category(&self, id: i64) -> Result<()> {
        for cid in self.subtree(id)? {
            db(self.conn.execute("UPDATE works SET category_id=NULL WHERE category_id=?", [cid]))?;
            db(self.conn.execute("DELETE FROM categories WHERE id=?", [cid]))?;
        }
        Ok(())
    }
}
