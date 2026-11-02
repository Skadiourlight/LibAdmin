//! Reads page count and embedded metadata, and renders cover thumbnails.
use crate::config::pdfium_dll;
use crate::djvu;
use image::{codecs::jpeg::JpegEncoder, DynamicImage};
use pdfium_render::prelude::*;
use std::io::Read;
use std::path::Path;
use std::sync::{LazyLock, Mutex, OnceLock};

pub const COVER_WIDTH: u32 = 600;
/// Bump when cover rendering changes so existing caches get regenerated.
pub const COVER_VERSION: &str = "2";

static PDFIUM: OnceLock<Option<Pdfium>> = OnceLock::new();
// Pdfium itself is not re-entrant; the scanner and the cover worker share this lock.
static PDF_LOCK: Mutex<()> = Mutex::new(());

fn pdfium() -> Option<&'static Pdfium> {
    PDFIUM.get_or_init(|| Pdfium::bind_to_library(pdfium_dll()).ok().map(Pdfium::new)).as_ref()
}

pub fn pdf_available() -> bool {
    pdfium().is_some()
}

fn ext_of(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Returns (pages, title, author). Pages are only reported for fixed-layout formats.
pub fn read_metadata(path: &Path) -> (Option<i64>, String, String) {
    let empty = (None, String::new(), String::new());
    match ext_of(path).as_str() {
        "pdf" => {
            let Some(p) = pdfium() else { return empty };
            let _guard = PDF_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let Ok(doc) = p.load_pdf_from_file(path, None) else { return empty };
            let tag = |t| doc.metadata().get(t).map(|m| m.value().trim().to_string()).unwrap_or_default();
            (
                Some(doc.pages().len() as i64),
                tag(PdfDocumentMetadataTagType::Title),
                tag(PdfDocumentMetadataTagType::Author),
            )
        }
        "epub" => epub_metadata(path).map(|(t, a)| (None, t, a)).unwrap_or(empty),
        "djvu" | "djv" => djvu::read_info(path),
        _ => empty,
    }
}

// ---------------- EPUB ----------------
static ATTR_CACHE: LazyLock<Mutex<std::collections::HashMap<String, regex::Regex>>> =
    LazyLock::new(|| Mutex::new(Default::default()));

fn attr(tag: &str, name: &str) -> Option<String> {
    let mut cache = ATTR_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let re = cache
        .entry(name.to_string())
        .or_insert_with(|| regex::Regex::new(&format!(r#"\b{name}\s*=\s*"([^"]*)""#)).unwrap());
    re.captures(tag).map(|c| c[1].to_string())
}

fn unescape(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix("<![CDATA[").and_then(|x| x.strip_suffix("]]>")).unwrap_or(s);
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'").replace("&amp;", "&")
}

fn read_zip_entry(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<Vec<u8>> {
    let mut f = zip.by_name(name).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn epub_opf(path: &Path) -> Option<(zip::ZipArchive<std::fs::File>, String, String)> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).ok()?).ok()?;
    let container = String::from_utf8_lossy(&read_zip_entry(&mut zip, "META-INF/container.xml")?).to_string();
    let opf_path = attr(&container, "full-path")?;
    let opf = String::from_utf8_lossy(&read_zip_entry(&mut zip, &opf_path)?).to_string();
    Some((zip, opf_path, opf))
}

static DC_TITLE: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?s)<dc:title[^>]*>(.*?)</dc:title>").unwrap());
static DC_CREATOR: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?s)<dc:creator[^>]*>(.*?)</dc:creator>").unwrap());
static ITEM: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"<(?:\w+:)?item\b[^>]*>").unwrap());
static META_COVER: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r#"<(?:\w+:)?meta\b[^>]*name\s*=\s*"cover"[^>]*>"#).unwrap());

fn epub_metadata(path: &Path) -> Option<(String, String)> {
    let (_, _, opf) = epub_opf(path)?;
    let title = DC_TITLE.captures(&opf).map(|c| unescape(&c[1])).unwrap_or_default();
    let authors: Vec<String> = DC_CREATOR.captures_iter(&opf).map(|c| unescape(&c[1])).filter(|a| !a.is_empty()).collect();
    Some((title, authors.join("; ")))
}

fn normalize_zip_path(base: &str, href: &str) -> String {
    let href = percent_encoding::percent_decode_str(href).decode_utf8_lossy().to_string();
    let mut parts: Vec<&str> = if base.is_empty() { vec![] } else { base.split('/').collect() };
    for seg in href.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn epub_cover_bytes(path: &Path) -> Option<Vec<u8>> {
    let (mut zip, opf_path, opf) = epub_opf(path)?;
    let items: Vec<&str> = ITEM.find_iter(&opf).map(|m| m.as_str()).collect();
    let mut href = items.iter().find(|t| attr(t, "properties").is_some_and(|p| p.contains("cover-image"))).and_then(|t| attr(t, "href"));
    if href.is_none() {
        if let Some(id) = META_COVER.find(&opf).and_then(|m| attr(m.as_str(), "content")) {
            href = items.iter().find(|t| attr(t, "id").as_deref() == Some(id.as_str())).and_then(|t| attr(t, "href"));
        }
    }
    if href.is_none() {
        href = items
            .iter()
            .find(|t| {
                attr(t, "id").is_some_and(|i| i.to_lowercase().contains("cover"))
                    && attr(t, "media-type").is_some_and(|m| m.starts_with("image/"))
            })
            .and_then(|t| attr(t, "href"));
    }
    let base = opf_path.rsplit_once('/').map(|(b, _)| b).unwrap_or("");
    read_zip_entry(&mut zip, &normalize_zip_path(base, &href?))
}

// ---------------- covers ----------------
fn render_pdf_cover(path: &Path) -> Option<DynamicImage> {
    let p = pdfium()?;
    let _guard = PDF_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let doc = p.load_pdf_from_file(path, None).ok()?;
    let page = doc.pages().first().ok()?;
    let config = PdfRenderConfig::new().set_target_width(COVER_WIDTH as i32).set_maximum_height((COVER_WIDTH * 3) as i32);
    let bitmap = page.render_with_config(&config).ok()?;
    let image = bitmap.as_image().ok();
    image
}

fn save_jpeg(img: DynamicImage, out: &Path) -> bool {
    let img = if img.width() > COVER_WIDTH {
        img.resize(COVER_WIDTH, u32::MAX, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let rgb = img.to_rgb8();
    let Ok(file) = std::fs::File::create(out) else { return false };
    let mut writer = std::io::BufWriter::new(file);
    JpegEncoder::new_with_quality(&mut writer, 90).encode_image(&rgb).is_ok()
}

/// Writes a JPEG thumbnail. Some(true) on success, Some(false) on failure, None if the format
/// cannot be rendered right now (e.g. tools missing), so it is retried later.
pub fn make_cover(path: &Path, out: &Path) -> Option<bool> {
    let img = match ext_of(path).as_str() {
        "pdf" => {
            pdfium()?;
            render_pdf_cover(path)
        }
        "epub" => epub_cover_bytes(path).and_then(|b| image::load_from_memory(&b).ok()),
        "djvu" | "djv" => {
            if !djvu::available() {
                return None;
            }
            let tmp = out.with_extension("ppm");
            let img = if djvu::render_first_page(path, &tmp, COVER_WIDTH) { image::open(&tmp).ok() } else { None };
            let _ = std::fs::remove_file(&tmp);
            img
        }
        _ => return Some(false),
    };
    Some(img.is_some_and(|i| save_jpeg(i, out)))
}
