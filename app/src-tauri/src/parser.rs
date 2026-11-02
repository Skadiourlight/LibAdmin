//! Title/author extraction from file names and embedded metadata, plus normalization used for grouping.
use fancy_regex::Regex as FRegex;
use regex::Regex;
use std::sync::LazyLock;

const CJK: &str = "\u{3400}-\u{9fff}\u{f900}-\u{faff}";
const CN_NUM: &str = "0-9一二三四五六七八九十两";

macro_rules! re {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new(&$pat).unwrap());
    };
}
macro_rules! fre {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<FRegex> = LazyLock::new(|| FRegex::new(&$pat).unwrap());
    };
}

re!(CJK_RE, format!("[{CJK}]"));
fre!(SOURCE_TAIL, r"(?i)\s*\((?=[^()]*\b(?:z-library|z-lib|1lib|libgen|annas?-archive)\b)[^()]*\)\s*$");
re!(DASH_SOURCE, r"(?i)\s*--\s*\(?\s*(?:WeLib\.org|Anna.?s Archive|libgen\S*)\s*\)?\s*$");
fre!(RANDOM_TAIL, r"_(?=[A-Za-z0-9]{7}$)(?=[^_]*[A-Z])(?=[^_]*[a-z])[A-Za-z0-9]{7}$");
re!(ISBN, r"(?i)^(?:ISBN[:\s]*)?(?:97[89][\d\-]{10,14}|\d{9}[\dXx])$");
re!(NOISE_PARENS, r"(?i)\s*[(（\[【][^)）\]】]*(?:OCR|bookmark|高清|文字版|扫描版|z-?lib)[^)）\]】]*[)）\]】]");
re!(COUNTRY, r"[\[［【(（]\s*(?:美|英|法|德|俄|苏|日|意|加|澳|奥|荷|瑞典|瑞士|以|印|波|匈|丹|挪|芬|比|西|葡|希|韩|美国|英国|法国|德国|俄罗斯|苏联|日本|意大利|加拿大|澳大利亚)\s*[\]］】)）]");
re!(AUTHOR_MARK, r"编著|主编|著");
re!(TRANSLATOR, r"(?i)译者|翻译|[(（]\s*译\s*[)）]|译$|^译|translat|重排");
re!(AUTHOR_PREFIX, r"(?i)^(?:(?:原著|原作者|作者|著者)\s*[:：]?|by\s*[:：]|by\s+)\s*");
re!(SEMI, r"[;；]");
re!(NAME_SPLIT, r"(?i)\s*(?:[,，、&/]|\band\b)\s*");
re!(PARENS, r"[(（][^)）]*[)）]");
re!(ETAL, r"(?i)\s*(?:等|etc\.?|et al\.?)$");
re!(EDITION, format!(
    r"(?i)第\s*[{CN_NUM}]+\s*版|\b\d+\s*(?:st|nd|rd|th)\s*(?:ed\.?|edition)\b|\b(?:first|second|third|fourth|fifth|sixth|seventh|eighth|ninth|tenth|revised|international|global)\s+edition\b|\d{{4}}\s*edition|\bedition\b|\b\d+e\b|chinese\s+(?:edition|version)|english\s+edition|中文版|中译本|中译|英文版|影印版|修订版|高清文字版|文字版|扫描版|彩色版|重排版|重排|合订本|中文学习译稿|中文译稿|译稿|中文翻译|中文译文|翻译|中文"
));
re!(VOLUME, format!(r"(?i)第\s*[{CN_NUM}]+\s*[册卷部辑]|\b(?:vol(?:ume)?|part|book|tome|band)\.?\s*(?:\d+|[ivx]+)\b|[上中下]册"));
fre!(TRAIL_VOL, format!(r"(?i)(?:[\s_\-]+(?:[ivx]{{1,4}}|\d{{1,2}})|(?<=[{CJK}])[上中下])$"));
re!(BRACKETS, r"[(（\[【][^)）\]】]*[)）\]】]");
re!(EMPTY_BRACKETS, r"[(（\[【]\s*[)）\]】]");
re!(SUPPLEMENT, r"(?i)答案|解答|题解|习题全解|习题详解|习题提示|勘误|指南|solutions?\b|errata|instructor|manual");
re!(KEY_STRIP, format!("[^0-9a-z{CJK}+#]"));
re!(AUTHOR_KEY_STRIP, format!("[^0-9a-z{CJK}]"));
re!(META_TITLE_EXT, r"(?i)\.(?:dvi|docx?|tex|pdf|indd|ps|qxd|p65)\b");
re!(ALL_PUNCT, r"^[\W\d_]+$");
re!(SHORT_LOWER, r"^[a-z]{1,4}$");
re!(WS, r"\s+");
re!(DASH_SPLIT, r"\s*--\s*");
re!(SPACED_DASH, r"\s+-\s+");
re!(VOL_CN, format!(r"第\s*([{CN_NUM}]+)\s*[册卷部辑]"));
re!(VOL_EN, r"(?i)\b(?:vol(?:ume)?|part|book|tome)\.?\s*(\d+|[ivx]+)\b");
re!(VOL_SXX, r"([上中下])册");
fre!(VOL_TRAIL_CN, format!(r"(?<=[{CJK}])([上中下])$"));
re!(VOL_TRAIL_NUM, r"(?i)[\s_\-]([ivx]{1,4}|\d{1,2})$");
fre!(CHAPTER_NUM, r"^([IVXLC]+|\d+)(?=[-_ .])");
re!(AFFIX_SEP, r"[-_]+");
re!(AFFIX_YEAR, r"^\s*\d{4}\s+|\s+\d{4}\s*$");

const JUNK_AUTHORS: &[&str] = &[
    "author", "authors", "unknown", "administrator", "admin", "user", "cnki", "owner", "pc", "未知", "佚名", "作者",
    "anonymous",
];
const JUNK_TITLES: &[&str] = &["title", "untitled", "无标题", "unknown", "document"];

fn fsub(re: &FRegex, s: &str, rep: &str) -> String {
    re.replace_all(s, rep).into_owned()
}

fn trim_chars<'a>(s: &'a str, chars: &str) -> &'a str {
    s.trim_matches(|c| chars.contains(c))
}

pub fn has_cjk(text: &str) -> bool {
    CJK_RE.is_match(text)
}

fn is_cjk_char(c: char) -> bool {
    ('\u{3400}'..='\u{9fff}').contains(&c) || ('\u{f900}'..='\u{faff}').contains(&c)
}

fn cn_digit(c: char) -> Option<i32> {
    Some(match c {
        '一' => 1,
        '二' | '两' => 2,
        '三' => 3,
        '四' => 4,
        '五' => 5,
        '六' => 6,
        '七' => 7,
        '八' => 8,
        '九' => 9,
        _ => return None,
    })
}

fn cn_to_int(s: &str) -> Option<i32> {
    if let Ok(n) = s.parse() {
        return Some(n);
    }
    if s == "十" {
        return Some(10);
    }
    if let Some((tens, ones)) = s.split_once('十') {
        let t = tens.chars().next().and_then(cn_digit).unwrap_or(1);
        let o = ones.chars().next().and_then(cn_digit).unwrap_or(0);
        return Some(t * 10 + o);
    }
    let mut it = s.chars();
    let c = it.next()?;
    if it.next().is_some() {
        return None;
    }
    cn_digit(c)
}

fn roman_to_int(s: &str) -> Option<i32> {
    let val = |c: char| match c.to_ascii_lowercase() {
        'i' => Some(1),
        'v' => Some(5),
        'x' => Some(10),
        'l' => Some(50),
        'c' => Some(100),
        _ => None,
    };
    let vals: Option<Vec<i32>> = s.chars().map(val).collect();
    let vals = vals?;
    if vals.is_empty() {
        return None;
    }
    let mut total = 0;
    for (i, v) in vals.iter().enumerate() {
        if i + 1 < vals.len() && vals[i + 1] > *v { total -= v } else { total += v }
    }
    Some(total)
}

fn num_or_roman(g: &str) -> Option<i32> {
    g.parse().ok().or_else(|| roman_to_int(g))
}

fn last_paren_start(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s.char_indices().rev() {
        if c == ')' {
            depth += 1;
        } else if c == '(' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

pub fn clean_title(title: &str) -> String {
    let t = NOISE_PARENS.replace_all(title, "");
    let t = WS.replace_all(&t, " ");
    trim_chars(&t, " -_—·,，.").to_string()
}

/// Returns (title, authors, structured). `structured` means the name followed a known
/// "title + author" convention, so its title should win over embedded metadata.
pub fn parse_filename(stem: &str) -> (String, Vec<String>, bool) {
    let mut s = stem.trim().to_string();
    let mut had_source = false;
    loop {
        let start = SOURCE_TAIL
            .find(&s)
            .ok()
            .flatten()
            .map(|m| m.start())
            .or_else(|| DASH_SOURCE.find(&s).map(|m| m.start()));
        match start {
            Some(i) => {
                s = s[..i].trim_end().to_string();
                had_source = true;
            }
            None => break,
        }
    }
    s = fsub(&RANDOM_TAIL, &s, "");

    let (mut title, mut authors_raw, mut structured) = (s.clone(), String::new(), false);
    if s.contains("--") {
        let parts: Vec<&str> =
            DASH_SPLIT.split(&s).map(str::trim).filter(|p| !p.is_empty() && !ISBN.is_match(p)).collect();
        if !parts.is_empty() {
            title = parts[0].to_string();
            authors_raw = parts.get(1).map(|x| x.to_string()).unwrap_or_default();
            structured = true;
        }
    } else if SPACED_DASH.is_match(&s) {
        let all: Vec<&str> = SPACED_DASH.split(&s).map(str::trim).collect();
        let isbn = all.iter().any(|p| ISBN.is_match(p));
        let parts: Vec<&str> = all.into_iter().filter(|p| !p.is_empty() && !ISBN.is_match(p)).collect();
        if parts.len() >= 2 && (isbn || parts.len() >= 3) {
            title = parts[0].to_string();
            authors_raw = parts[1].to_string();
            structured = true;
        }
    } else if had_source && s.ends_with(')') {
        if let Some(start) = last_paren_start(&s) {
            if start > 0 {
                title = s[..start].trim().to_string();
                authors_raw = s[start + 1..s.len() - 1].to_string();
                structured = true;
            }
        }
    }
    if !structured {
        title = title.replace('_', " ");
    }
    (clean_title(&title), split_authors(&authors_raw), structured)
}

fn valid_author(name: &str) -> bool {
    let n = name.chars().count();
    if !(2..=60).contains(&n) {
        return false;
    }
    if JUNK_AUTHORS.contains(&name.to_lowercase().as_str()) {
        return false;
    }
    if SHORT_LOWER.is_match(name) || ALL_PUNCT.is_match(name) {
        return false;
    }
    !ISBN.is_match(name)
}

pub fn split_authors(raw: &str) -> Vec<String> {
    if raw.trim().is_empty() {
        return vec![];
    }
    let s = COUNTRY.replace_all(raw, "");
    let mut out: Vec<String> = vec![];
    for seg in SEMI.split(&s) {
        let seg = AUTHOR_PREFIX.replace(seg.trim(), "").to_string();
        if seg.is_empty() {
            continue;
        }
        let seg = match AUTHOR_MARK.find(&seg) {
            Some(m) => seg[..m.start()].to_string(),
            None if TRANSLATOR.is_match(&seg) => continue,
            None => seg,
        };
        for name in NAME_SPLIT.split(&seg) {
            let name = PARENS.replace_all(name, "");
            let name = ETAL.replace(&name, "");
            let name = trim_chars(&name, " .·-_").to_string();
            if valid_author(&name) && !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

pub fn valid_meta_title(t: &str) -> bool {
    let t = t.trim();
    if t.chars().count() < 2 || JUNK_TITLES.contains(&t.to_lowercase().as_str()) {
        return false;
    }
    if META_TITLE_EXT.is_match(t) || t.to_lowercase().starts_with("microsoft word") {
        return false;
    }
    !ALL_PUNCT.is_match(t)
}

pub fn clean_meta_title(t: &str) -> String {
    let t = t.trim();
    let t = match t.split_once('=') {
        Some((head, _)) if has_cjk(head) => head,
        _ => t,
    };
    clean_title(t)
}

pub fn resolve_title_authors(stem: &str, meta_title: &str, meta_author: &str) -> (String, Vec<String>) {
    let (fn_title, fn_authors, structured) = parse_filename(stem);
    let title = if structured || !valid_meta_title(meta_title) {
        if fn_title.is_empty() { stem.to_string() } else { fn_title }
    } else {
        let t = clean_meta_title(meta_title);
        if t.is_empty() { fn_title } else { t }
    };
    let authors = if structured && !fn_authors.is_empty() { fn_authors } else { split_authors(meta_author) };
    (title, authors)
}

pub fn detect_language(texts: &[&str]) -> String {
    let joined = texts.join(" ");
    if has_cjk(&joined) || joined.to_lowercase().contains("chinese") { "中文".into() } else { "外文".into() }
}

/// Picks one English and one Chinese title from a list of book names.
pub fn collect_lang_titles<'a>(titles: impl IntoIterator<Item = &'a str>) -> (String, String) {
    let mut en = String::new();
    let mut zh = String::new();
    for raw in titles {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        if has_cjk(t) {
            if zh.is_empty() {
                zh = t.to_string();
            }
        } else if en.is_empty() {
            en = t.to_string();
        }
    }
    (en, zh)
}

pub fn display_title(en: &str, zh: &str) -> String {
    if !zh.trim().is_empty() { zh.trim().to_string() } else { en.trim().to_string() }
}

pub fn title_key(title: &str) -> String {
    let t = BRACKETS.replace_all(&title.to_lowercase(), " ").to_string();
    let t = VOLUME.replace_all(&t, " ").to_string();
    let t = EDITION.replace_all(&t, " ").to_string();
    let t = fsub(&TRAIL_VOL, t.trim(), "");
    KEY_STRIP.replace_all(&t, "").to_string()
}

pub fn strip_volume(title: &str) -> String {
    let t = VOLUME.replace_all(title, " ").to_string();
    let t = fsub(&TRAIL_VOL, t.trim(), "");
    let t = EMPTY_BRACKETS.replace_all(&t, "");
    let t = clean_title(&t);
    if t.is_empty() { title.to_string() } else { t }
}

pub fn volume_index(texts: &[&str]) -> Option<i32> {
    for text in texts {
        let t = text.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(n) = VOL_CN.captures(t).and_then(|c| cn_to_int(&c[1])) {
            return Some(n);
        }
        if let Some(c) = VOL_EN.captures(t) {
            return num_or_roman(&c[1]);
        }
        let sxx = VOL_SXX
            .captures(t)
            .map(|c| c[1].to_string())
            .or_else(|| VOL_TRAIL_CN.captures(t).ok().flatten().map(|c| c[1].to_string()));
        if let Some(c) = sxx {
            return Some(match c.as_str() { "上" => 1, "中" => 2, _ => 3 });
        }
        if let Some(c) = VOL_TRAIL_NUM.captures(t) {
            return num_or_roman(&c[1]);
        }
    }
    None
}

pub fn is_supplement(stem: &str) -> bool {
    SUPPLEMENT.is_match(stem)
}

pub fn key_weight(key: &str) -> usize {
    key.chars().map(|c| if is_cjk_char(c) { 2 } else { 1 }).sum()
}

pub fn author_keys(authors: &[String]) -> Vec<String> {
    authors
        .iter()
        .map(|a| AUTHOR_KEY_STRIP.replace_all(&a.to_lowercase(), "").to_string())
        .filter(|k| k.chars().count() >= if has_cjk(k) { 2 } else { 4 })
        .collect()
}

pub fn authors_overlap(ka: &[String], kb: &[String]) -> bool {
    if ka.is_empty() || kb.is_empty() {
        return false;
    }
    let ja = ka.join("|");
    let jb = kb.join("|");
    ka.iter().any(|k| jb.contains(k.as_str())) || kb.iter().any(|k| ja.contains(k.as_str()))
}

/// Whether two normalized titles with their author keys describe the same work.
pub fn keys_match(k1: &str, k2: &str, a1: &[String], a2: &[String]) -> bool {
    if k1.is_empty() || k2.is_empty() {
        return false;
    }
    let (short, long) = if k1.len() <= k2.len() { (k1, k2) } else { (k2, k1) };
    if !long.starts_with(short) {
        return false;
    }
    let w = key_weight(short);
    let overlap = authors_overlap(a1, a2);
    let one_missing = a1.is_empty() || a2.is_empty();
    if short == long {
        if overlap {
            return w >= 2;
        }
        if a1.is_empty() && a2.is_empty() {
            return w >= 4;
        }
        return one_missing && w >= 8;
    }
    if overlap {
        return w >= 8;
    }
    one_missing && w >= 12
}

const FRONT: &[&str] = &["cover", "front", "copyright", "dedication", "contents", "preface", "foreword", "introduction", "前言", "序", "目录"];
const BACK: &[&str] = &["appendix", "list-of-symbols", "list of symbols", "symbols", "index", "bibliography", "references", "索引", "附录"];

pub fn chapter_order(stem: &str) -> (i32, i32) {
    if let Ok(Some(c)) = CHAPTER_NUM.captures(stem) {
        return (1, num_or_roman(&c[1]).unwrap_or(0));
    }
    let low = stem.to_lowercase();
    if let Some(i) = FRONT.iter().position(|w| low.starts_with(w)) {
        return (0, i as i32);
    }
    if let Some(i) = BACK.iter().position(|w| low.starts_with(w)) {
        return (2, i as i32);
    }
    (1, 9999)
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub enum NatPart {
    Num(u64),
    Text(String),
}

pub fn natural_key(s: &str) -> Vec<NatPart> {
    let mut out = vec![];
    let mut buf = String::new();
    let mut digits = false;
    for c in s.chars() {
        let d = c.is_ascii_digit();
        if !buf.is_empty() && d != digits {
            out.push(if digits { NatPart::Num(buf.parse().unwrap_or(u64::MAX)) } else { NatPart::Text(buf.to_lowercase()) });
            buf.clear();
        }
        digits = d;
        buf.push(c);
    }
    if !buf.is_empty() {
        out.push(if digits { NatPart::Num(buf.parse().unwrap_or(u64::MAX)) } else { NatPart::Text(buf.to_lowercase()) });
    }
    out
}

/// Longest common prefix or suffix of the given names.
pub fn common_affix(stems: &[String]) -> String {
    if stems.len() < 2 {
        return String::new();
    }
    let chars: Vec<Vec<char>> = stems.iter().map(|s| s.chars().collect()).collect();
    let min_len = chars.iter().map(Vec::len).min().unwrap_or(0);
    let pre = (0..min_len).take_while(|&i| chars.iter().all(|c| c[i] == chars[0][i])).count();
    let suf = (0..min_len)
        .take_while(|&i| chars.iter().all(|c| c[c.len() - 1 - i] == chars[0][chars[0].len() - 1 - i]))
        .count();
    if pre >= suf {
        chars[0][..pre].iter().collect()
    } else {
        chars[0][chars[0].len() - suf..].iter().collect()
    }
}

pub fn affix_title(affix: &str) -> String {
    let t = AFFIX_SEP.replace_all(affix, " ");
    let t = AFFIX_YEAR.replace_all(&t, " ");
    clean_title(&t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_conventions() {
        let (t, a, s) = parse_filename("数学物理方法 第三版 (with OCR and TOC bookmarks) (吴崇试, 高春媛) (z-library.sk, 1lib.sk, z-lib.sk)");
        assert_eq!(t, "数学物理方法 第三版");
        assert_eq!(a, vec!["吴崇试", "高春媛"]);
        assert!(s);
        let (t, a, _) = parse_filename("Mathematical Physics - Sadri Hassani - 978-3-319-01195-0");
        assert_eq!((t.as_str(), a), ("Mathematical Physics", vec!["Sadri Hassani".to_string()]));
        let (t, a, _) = parse_filename("Methods of modern mathematical physics Vol I II III & IV 1980edition-- Michael Reed Barry Simon -- ( WeLib.org )");
        assert!(t.starts_with("Methods of modern"));
        assert_eq!(a, vec!["Michael Reed Barry Simon"]);
        assert_eq!(parse_filename("Lee_光滑流形导论_中文译文_RyPo5vv").0, "Lee 光滑流形导论 中文译文");
    }

    #[test]
    fn authors() {
        assert_eq!(split_authors("[美]Mary L Boas著陈伟华 邓达强 译"), vec!["Mary L Boas"]);
        assert_eq!(split_authors("原著 Sadri Hassani；译者 ChatGPT, Congyu"), vec!["Sadri Hassani"]);
        assert_eq!(split_authors("（美）J. B. 康威著；吕以辇，张南岳译"), vec!["J. B. 康威"]);
        assert_eq!(split_authors("[美]列昂·库珀(Leon Neil Cooper); 杨基方(译); 汲长松(译)"), vec!["列昂·库珀"]);
        assert_eq!(split_authors("Byron Smith"), vec!["Byron Smith"]);
    }

    #[test]
    fn keys_and_volumes() {
        assert_eq!(title_key("自然科学及工程中的数学方法(第三册)"), title_key("自然科学及工程中的数学方法 第一册"));
        assert_eq!(title_key("C++ Templates（第2版）中文版"), "c++templates");
        assert_eq!(title_key("Amann I"), "amann");
        assert_eq!(volume_index(&["邹应数学分析下"]), Some(3));
        assert_eq!(volume_index(&["Amann_III"]), Some(3));
        assert_eq!(volume_index(&["自然科学及工程中的数学方法 第二册"]), Some(2));
        assert_eq!(strip_volume("数学分析（上册）"), "数学分析");
        assert_eq!(chapter_order("IV---Topological-Spaces_1972"), (1, 4));
        assert_eq!(chapter_order("Index_1972"), (2, 4));
        assert_eq!(
            collect_lang_titles(["Differential Forms in Algebraic Topology", "代数拓扑中的微分形式（翻译工作稿）"]),
            ("Differential Forms in Algebraic Topology".into(), "代数拓扑中的微分形式（翻译工作稿）".into()),
        );
        assert_eq!(display_title("Algebra", "代数"), "代数");
        assert_eq!(display_title("Algebra", ""), "Algebra");
    }
}
