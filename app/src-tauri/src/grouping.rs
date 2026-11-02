//! Assigns newly indexed files to works: chapter-split folders, then existing works, then each other.
use crate::index::{BookFile, LibraryIndex, Result};
use crate::parser;
use std::collections::{BTreeSet, HashMap, HashSet};

const MIN_CHAPTER_FILES: usize = 3;
const MIN_CHAPTER_AFFIX: usize = 12;

struct Node {
    file_ids: Vec<i64>,
    keys: Vec<String>,
    akeys: Vec<String>,
    pages: HashSet<i64>,
    work_id: Option<i64>,
    supplement: bool,
}

fn file_node(f: &BookFile) -> Node {
    let mut keys = vec![];
    for k in [parser::title_key(&f.title), parser::title_key(f.stem())] {
        if !k.is_empty() && !keys.contains(&k) {
            keys.push(k);
        }
    }
    Node {
        file_ids: vec![f.id],
        keys,
        akeys: parser::author_keys(&f.authors),
        pages: f.pages.filter(|&p| p >= 50).into_iter().collect(),
        work_id: None,
        supplement: f.supplement(),
    }
}

/// "<书名>习题答案" style files: the book's title is a prefix of the supplement's name.
fn supplement_of(supp: &Node, book: &Node) -> bool {
    if book.supplement {
        return false;
    }
    if !(parser::authors_overlap(&supp.akeys, &book.akeys) || supp.akeys.is_empty() || book.akeys.is_empty()) {
        return false;
    }
    supp.keys.iter().any(|ks| book.keys.iter().any(|kb| ks != kb && ks.starts_with(kb.as_str()) && parser::key_weight(kb) >= 4))
}

fn nodes_match(a: &Node, b: &Node) -> bool {
    for k1 in &a.keys {
        for k2 in &b.keys {
            if parser::keys_match(k1, k2, &a.akeys, &b.akeys) {
                return true;
            }
        }
    }
    if (a.supplement && supplement_of(a, b)) || (b.supplement && supplement_of(b, a)) {
        return true;
    }
    !a.pages.is_disjoint(&b.pages) && parser::authors_overlap(&a.akeys, &b.akeys)
}

fn prefix2(k: &str) -> String {
    k.chars().take(2).collect()
}

/// Buckets nodes so only plausible pairs are compared (title-key prefix or equal page count).
#[derive(Default)]
struct Blocker {
    by_prefix: HashMap<String, Vec<usize>>,
    by_pages: HashMap<i64, Vec<usize>>,
}

impl Blocker {
    fn add(&mut self, idx: usize, node: &Node) {
        for k in &node.keys {
            self.by_prefix.entry(prefix2(k)).or_default().push(idx);
        }
        for p in &node.pages {
            self.by_pages.entry(*p).or_default().push(idx);
        }
    }
    fn candidates(&self, node: &Node) -> BTreeSet<usize> {
        let mut out = BTreeSet::new();
        for k in &node.keys {
            if let Some(v) = self.by_prefix.get(&prefix2(k)) {
                out.extend(v);
            }
        }
        for p in &node.pages {
            if let Some(v) = self.by_pages.get(p) {
                out.extend(v);
            }
        }
        out
    }
}

fn detect_chapter_folders(index: &LibraryIndex, files: &mut [BookFile]) -> Result<Vec<Node>> {
    let mut by_folder: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, f) in files.iter().enumerate() {
        if !f.external {
            by_folder.entry(f.folder().to_string()).or_default().push(i);
        }
    }
    let mut nodes = vec![];
    for (folder, idxs) in by_folder {
        if folder.is_empty() || idxs.len() < MIN_CHAPTER_FILES || index.folder_has_grouped_files(&folder)? {
            continue;
        }
        let stems: Vec<String> = idxs.iter().map(|&i| files[i].stem().to_string()).collect();
        let affix = parser::common_affix(&stems);
        if affix.trim_matches(|c| " -_".contains(c)).chars().count() < MIN_CHAPTER_AFFIX {
            continue;
        }
        let ids: Vec<i64> = idxs.iter().map(|&i| files[i].id).collect();
        index.set_chapter_dir(&ids, &folder)?;
        for &i in &idxs {
            files[i].chapter_dir = Some(folder.clone());
        }
        let dir_name = folder.rsplit('/').next().unwrap_or(&folder);
        let keys: Vec<String> = [parser::title_key(dir_name), parser::title_key(&parser::affix_title(&affix))]
            .into_iter()
            .filter(|k| !k.is_empty())
            .collect();
        let mut counts: HashMap<&Vec<String>, usize> = HashMap::new();
        for &i in &idxs {
            if !files[i].authors.is_empty() {
                *counts.entry(&files[i].authors).or_default() += 1;
            }
        }
        let akeys = counts
            .into_iter()
            .filter(|(_, n)| *n >= 2)
            .max_by_key(|(_, n)| *n)
            .map(|(a, _)| parser::author_keys(a))
            .unwrap_or_default();
        nodes.push(Node { file_ids: ids, keys, akeys, pages: HashSet::new(), work_id: None, supplement: false });
    }
    Ok(nodes)
}

fn existing_work_nodes(index: &LibraryIndex) -> Result<Vec<Node>> {
    let mut nodes = vec![];
    for w in index.load_works()? {
        if w.files.is_empty() {
            continue;
        }
        let mut keys: Vec<String> = vec![parser::title_key(&w.title), parser::title_key(&w.title_en), parser::title_key(&w.title_zh)];
        keys.extend(w.files.iter().map(|f| parser::title_key(&f.title)));
        keys.retain(|k| !k.is_empty());
        keys.dedup();
        let mut authors = w.authors.clone();
        for f in &w.files {
            for a in &f.authors {
                if !authors.contains(a) {
                    authors.push(a.clone());
                }
            }
        }
        nodes.push(Node {
            file_ids: w.files.iter().map(|f| f.id).collect(),
            keys,
            akeys: parser::author_keys(&authors),
            pages: w.files.iter().filter_map(|f| f.pages).filter(|&p| p >= 50).collect(),
            work_id: Some(w.id),
            supplement: false,
        });
    }
    Ok(nodes)
}

pub struct GroupResult {
    pub created: Vec<i64>,
    pub touched: HashSet<i64>,
}

/// Groups every file that has no work yet.
pub fn group_new_files(index: &LibraryIndex) -> Result<GroupResult> {
    let mut new_files = index.files_without_work()?;
    let mut result = GroupResult { created: vec![], touched: HashSet::new() };
    if new_files.is_empty() {
        return Ok(result);
    }

    let chapter_nodes = detect_chapter_folders(index, &mut new_files)?;
    let in_chapter: HashSet<i64> = chapter_nodes.iter().flat_map(|n| n.file_ids.iter().copied()).collect();
    let mut new_nodes = chapter_nodes;
    new_nodes.extend(new_files.iter().filter(|f| !in_chapter.contains(&f.id)).map(file_node));

    let existing = existing_work_nodes(index)?;
    let mut blocker = Blocker::default();
    for (i, n) in existing.iter().enumerate() {
        blocker.add(i, n);
    }

    let mut unmatched: Vec<Node> = vec![];
    for node in new_nodes {
        let target = blocker.candidates(&node).into_iter().find(|&i| nodes_match(&node, &existing[i]));
        match target.and_then(|i| existing[i].work_id) {
            Some(wid) => {
                index.assign_files(&node.file_ids, wid)?;
                result.touched.insert(wid);
            }
            None => unmatched.push(node),
        }
    }

    let mut parent: Vec<usize> = (0..unmatched.len()).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut blocker = Blocker::default();
    for i in 0..unmatched.len() {
        for j in blocker.candidates(&unmatched[i]) {
            let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
            if ri != rj && nodes_match(&unmatched[i], &unmatched[j]) {
                parent[ri] = rj;
            }
        }
        blocker.add(i, &unmatched[i]);
    }

    let mut groups: HashMap<usize, Vec<i64>> = HashMap::new();
    for i in 0..unmatched.len() {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().extend(&unmatched[i].file_ids);
    }
    let mut roots: Vec<usize> = groups.keys().copied().collect();
    roots.sort();
    for r in roots {
        let wid = index.create_auto_work()?;
        index.assign_files(&groups[&r], wid)?;
        result.created.push(wid);
        result.touched.insert(wid);
    }
    for &wid in &result.touched {
        index.refresh_work(wid)?;
    }
    Ok(result)
}
