const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];
const FAIL = Symbol("fail");
const ROLES = ["主本", "其他版本", "译本", "分卷", "章节", "配套资料"];
const collator = new Intl.Collator("zh-CN-u-co-pinyin", { numeric: true, sensitivity: "base" });

const ICON = {
  all: '<path d="M4 5h7v6H4zM13 5h7v6h-7zM4 13h7v6H4zM13 13h7v6h-7z"/>',
  inbox: '<path d="M4 13l2.5-7h11L20 13v6H4z"/><path d="M4 13h5l1 2h4l1-2h5"/>',
  folder: '<path d="M3.5 6.5h6l2 2h9v10h-17z"/>',
  trash: '<path d="M5 7h14M10 7V5h4v2M7 7l1 12h8l1-12"/>',
  chev: '<path d="M9 6l6 6-6 6"/>',
  x: '<path d="M6 6l12 12M18 6L6 18"/>',
  open: '<path d="M14 4h6v6M20 4l-9 9"/><path d="M18 14v5H5V6h5"/>',
  reveal: '<path d="M3.5 6.5h6l2 2h9v10h-17z"/><path d="M12 12v4M10 14h4"/>',
  star: '<path d="M12 4l2.4 5 5.4.6-4 3.7 1.1 5.4L12 16l-4.9 2.7L8.2 13.3l-4-3.7 5.4-.6z"/>',
  split: '<path d="M8 4v6a4 4 0 0 0 4 4h0a4 4 0 0 1 4 4v2M16 4v6"/><path d="M5 7l3-3 3 3"/>',
  minus: '<path d="M6 12h12"/>',
  edit: '<path d="M4 20h4L19 9l-4-4L4 16z"/><path d="M13 7l4 4"/>',
  move: '<path d="M3.5 6.5h6l2 2h9v10h-17z"/><path d="M10 13.5h6M14 11l2.5 2.5L14 16"/>',
  merge: '<path d="M6 4v4a4 4 0 0 0 4 4h4a4 4 0 0 1 4 4v4M18 4v4a4 4 0 0 1-4 4"/>',
  image: '<rect x="4" y="5" width="16" height="14" rx="2"/><path d="M4 16l5-5 4 4 2-2 5 5"/>',
  restore: '<path d="M4 12a8 8 0 1 0 2.3-5.7"/><path d="M4 4v5h5"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  person: '<circle cx="12" cy="8" r="3.5"/><path d="M5 20c1.2-3.5 3.8-5 7-5s5.8 1.5 7 5"/>',
  globe: '<circle cx="12" cy="12" r="8"/><path d="M4 12h16M12 4c2.5 2.5 2.5 13.5 0 16M12 4c-2.5 2.5-2.5 13.5 0 16"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  root: '<path d="M4 5h6l2 2h8v12H4z"/>',
  scan: '<path d="M20 12a8 8 0 1 1-2.34-5.66"/><path d="M20 4v5h-5"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 3v2M12 19v2M3 12h2M19 12h2M5.6 5.6l1.4 1.4M17 17l1.4 1.4M5.6 18.4L7 17M17 7l1.4-1.4"/>',
  back: '<path d="M15 5L8 12l7 7"/>',
  panel: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M15 4v16"/>',
};
const svg = (name) => `<svg viewBox="0 0 24 24">${ICON[name] || ""}</svg>`;
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

// ---------------- state ----------------
const S = {
  st: null,
  works: [],
  byId: new Map(),
  cats: [],
  catById: new Map(),
  kids: new Map(),
  subtreeCount: new Map(),
  directCount: new Map(),
  uncatCount: 0,
  removedCount: 0,
  view: { kind: "all" },
  tab: "cat",
  search: "",
  sort: "title",
  tile: 160,
  side: 260,
  inspectW: 400,
  inspectOpen: true,
  theme: "system",
  visible: [],
  selected: new Set(),
  anchor: null,
  coverVer: new Map(),
  collapsed: new Set(),
  detailId: null,
  inspectId: null,
  fileSearch: "",
  fileSort: "name",
  drag: null,
  scanning: false,
  coversPending: 0,
};

// ---------------- backend helpers ----------------
async function call(cmd, args) {
  try {
    return await invoke(cmd, args);
  } catch (e) {
    toast(String(e), "error");
    return FAIL;
  }
}

function toast(text, kind = "") {
  const el = document.createElement("div");
  el.className = `toast ${kind}`;
  el.textContent = text;
  $("#toasts").appendChild(el);
  setTimeout(() => el.classList.add("out"), kind === "error" ? 5200 : 2800);
  setTimeout(() => el.remove(), kind === "error" ? 5600 : 3200);
}

let saveTimer = 0;
function saveSettings() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => call("save_settings", {
    tileWidth: Math.round(S.tile),
    sort: S.sort,
    theme: S.theme,
    sideWidth: Math.round(S.side),
    inspectWidth: Math.round(S.inspectW),
    inspectOpen: S.inspectOpen,
  }), 400);
}

// ---------------- data ----------------
const coverId = (w) => w.mainFileId ?? w.files[0]?.id ?? null;
const mainFile = (w) => w.files.find((f) => f.id === w.mainFileId) || w.files[0] || null;
const workLangs = (w) => new Set(w.files.map((f) => f.language).filter(Boolean));
const workTitle = (w) => (w.titleZh || w.titleEn || w.title || "").trim();
const workTitleEn = (w) => (w.titleEn || "").trim();
const workTitleZh = (w) => (w.titleZh || "").trim();
const workSub = (w) => {
  const zh = workTitleZh(w);
  const en = workTitleEn(w);
  if (zh && en) return en;
  return "";
};

function setData(lib) {
  S.works = lib.works;
  S.byId = new Map(lib.works.map((w) => [w.id, w]));
  S.cats = lib.categories;
  S.catById = new Map(lib.categories.map((c) => [c.id, c]));
  S.kids = new Map();
  for (const c of [...lib.categories].sort((a, b) => a.sort - b.sort || collator.compare(a.name, b.name))) {
    const k = c.parentId ?? null;
    if (!S.kids.has(k)) S.kids.set(k, []);
    S.kids.get(k).push(c);
  }
  const direct = new Map();
  S.directCount = direct;
  S.uncatCount = 0;
  S.removedCount = 0;
  for (const w of S.works) {
    if (w.removed) { S.removedCount++; continue; }
    if (w.categoryId == null || !S.catById.has(w.categoryId)) S.uncatCount++;
    else direct.set(w.categoryId, (direct.get(w.categoryId) || 0) + 1);
  }
  S.subtreeCount = new Map();
  const count = (id) => {
    let n = direct.get(id) || 0;
    for (const k of S.kids.get(id) || []) n += count(k.id);
    S.subtreeCount.set(id, n);
    return n;
  };
  for (const c of S.kids.get(null) || []) count(c.id);
  for (const id of [...S.selected]) if (!S.byId.has(id)) S.selected.delete(id);
  if (S.view.kind === "cat" && !S.catById.has(S.view.id)) S.view = { kind: "all" };
}

function subtreeIds(id) {
  const out = new Set([id]);
  const walk = (p) => (S.kids.get(p) || []).forEach((c) => { out.add(c.id); walk(c.id); });
  walk(id);
  return out;
}

function catPath(id) {
  const parts = [];
  let c = S.catById.get(id);
  while (c) { parts.unshift(c.name); c = S.catById.get(c.parentId); }
  return parts.join(" / ");
}

async function reload() {
  if (!S.st?.library) { setData({ works: [], categories: [] }); renderAll(); return; }
  const lib = await call("get_library");
  if (lib === FAIL) return;
  setData(lib);
  renderAll();
}

// ---------------- view filter ----------------
function matchesSearch(w, tokens) {
  if (!tokens.length) return true;
  const hay = [w.title, w.titleEn, w.titleZh, w.authors.join(" "), w.notes, ...w.files.map((f) => f.relPath)].join("\n").toLowerCase();
  return tokens.every((t) => hay.includes(t));
}

function computeVisible() {
  const v = S.view;
  const tokens = S.search.toLowerCase().split(/\s+/).filter(Boolean);
  const list = S.works.filter((w) => {
    if (v.kind === "removed") { if (!w.removed) return false; }
    else if (w.removed) return false;
    if (v.kind === "uncat" && w.categoryId != null && S.catById.has(w.categoryId)) return false;
    if (v.kind === "cat" && w.categoryId !== v.id) return false;
    if (v.kind === "author" && !w.authors.includes(v.value)) return false;
    if (v.kind === "lang") {
      const langs = workLangs(w);
      if (v.value === "未知" ? langs.size : !langs.has(v.value)) return false;
    }
    return matchesSearch(w, tokens);
  });
  const byTitle = (a, b) => collator.compare(workTitle(a), workTitle(b));
  if (S.sort === "author") list.sort((a, b) => (a.authors.length ? 0 : 1) - (b.authors.length ? 0 : 1) || collator.compare(a.authors[0] || "", b.authors[0] || "") || byTitle(a, b));
  else if (S.sort === "added") list.sort((a, b) => b.addedAt - a.addedAt || byTitle(a, b));
  else list.sort(byTitle);
  S.visible = list;
}

function viewTitle() {
  const v = S.view;
  if (v.kind === "uncat") return "未分类";
  if (v.kind === "removed") return "已移除";
  if (v.kind === "cat") return catPath(v.id);
  if (v.kind === "author") return v.value;
  if (v.kind === "lang") return v.value;
  return "全部";
}

function setView(view) {
  S.view = view;
  S.selected.clear();
  S.anchor = null;
  if (S.detailId != null) closeDetail();
  renderSidebar();
  renderGrid();
  ensureInspect();
  $("#grid").scrollTop = 0;
}

// ---------------- render: sidebar ----------------
function renderAll() {
  renderTop();
  renderSidebar();
  renderGrid();
  renderStatus();
  if (S.detailId != null) {
    if (S.byId.has(S.detailId)) {
      renderDetail();
      const w = S.byId.get(S.detailId);
      const need = w?.files.filter((f) => !f.hasCover).map((f) => f.id) || [];
      if (need.length) call("ensure_covers", { fileIds: need });
    } else closeDetail();
  }
  if (S.detailId == null) ensureInspect();
}

function renderTop() {
  const sel = $("#lib-select");
  const libs = S.st?.libraries || [];
  sel.innerHTML = libs.length ? libs.map((n) => `<option ${n === S.st.library ? "selected" : ""}>${esc(n)}</option>`).join("") : `<option>（没有书库）</option>`;
  sel.disabled = !libs.length;
  $("#btn-new").disabled = !S.st?.library;
}

function navRow({ key, icon, name, num, active, attrs = "" }) {
  return `<div class="row ${active ? "active" : ""}" data-nav="${key}" ${attrs}>${svg(icon)}<span class="name">${esc(name)}</span><span class="num">${num ?? ""}</span></div>`;
}

function renderSidebar() {
  const v = S.view;
  const total = S.works.length - S.removedCount;
  $("#nav-fixed").innerHTML =
    navRow({ key: "all", icon: "all", name: "全部", num: total, active: v.kind === "all" }) +
    navRow({ key: "uncat", icon: "inbox", name: "未分类", num: S.uncatCount, active: v.kind === "uncat" });
  $("#nav-removed").innerHTML = navRow({ key: "removed", icon: "trash", name: "已移除", num: S.removedCount || "", active: v.kind === "removed" });

  const rows = [];
  const walk = (parent, depth) => {
    for (const c of S.kids.get(parent) || []) {
      const hasKids = (S.kids.get(c.id) || []).length > 0;
      const open = hasKids && !S.collapsed.has(c.id);
      rows.push(
        `<div class="row ${v.kind === "cat" && v.id === c.id ? "active" : ""}" data-cat="${c.id}" draggable="true" style="--depth:${depth}" title="${esc(catPath(c.id))}${c.folder ? `\n来自文件夹：${esc(c.folder)}` : ""}">` +
          `<span class="caret ${hasKids ? (open ? "open" : "") : "leaf"}">${svg("chev")}</span>${svg("folder")}` +
          `<span class="name">${esc(c.name)}</span><span class="num">${S.directCount.get(c.id) || ""}</span></div>`,
      );
      if (open) walk(c.id, depth + 1);
    }
  };
  walk(null, 0);
  $("#cat-tree").innerHTML = rows.join("") || `<div class="row muted" style="cursor:default">还没有类别</div>`;

  renderAuthors();
  renderLangs();
}

function renderAuthors() {
  const counts = new Map();
  for (const w of S.works) if (!w.removed) for (const a of w.authors) counts.set(a, (counts.get(a) || 0) + 1);
  const q = $("#author-filter").value.trim().toLowerCase();
  const list = [...counts].filter(([a]) => !q || a.toLowerCase().includes(q)).sort((a, b) => b[1] - a[1] || collator.compare(a[0], b[0]));
  $("#author-list").innerHTML = list
    .map(([a, n]) => `<div class="row ${S.view.kind === "author" && S.view.value === a ? "active" : ""}" data-author="${esc(a)}">${svg("person")}<span class="name">${esc(a)}</span><span class="num">${n}</span></div>`)
    .join("") || `<div class="row muted">没有作者信息</div>`;
}

function renderLangs() {
  const counts = new Map();
  for (const w of S.works) {
    if (w.removed) continue;
    const langs = workLangs(w);
    if (!langs.size) counts.set("未知", (counts.get("未知") || 0) + 1);
    for (const l of langs) counts.set(l, (counts.get(l) || 0) + 1);
  }
  $("#lang-list").innerHTML = [...counts]
    .sort((a, b) => b[1] - a[1])
    .map(([l, n]) => `<div class="row ${S.view.kind === "lang" && S.view.value === l ? "active" : ""}" data-lang="${esc(l)}">${svg("globe")}<span class="name">${esc(l)}</span><span class="num">${n}</span></div>`)
    .join("");
}

// ---------------- render: grid ----------------
function hue(s) {
  let h = 0;
  for (const ch of s) h = (h * 31 + ch.codePointAt(0)) % 360;
  return h;
}

function coverHtml(w, cls = "") {
  const cid = coverId(w);
  const ph = `<div class="ph ${cls}" style="--h:${hue(workTitle(w))}"><div class="ph-title">${esc(workTitle(w))}</div><div class="ph-author">${esc(workSub(w) || w.authors.join("、"))}</div></div>`;
  if (cid == null || !w.hasCover) return ph;
  return `<img src="http://cover.localhost/${cid}?v=${S.coverVer.get(cid) || 0}" loading="lazy" decoding="async" draggable="false" alt="" data-ph="1">`;
}

function tileHtml(w) {
  const mf = mainFile(w);
  const top = [];
  const left = [];
  const bottom = [];
  if (w.files.length > 1) top.push(`<span class="b files">${w.files.length} 个文件</span>`);
  if (w.files.some((f) => f.role === "译本")) left.push(`<span class="b trans">译本</span>`);
  if (w.files.some((f) => f.role === "配套资料")) top.push(`<span class="b supp">配套</span>`);
  if (w.files.some((f) => f.missing)) top.push(`<span class="b missing">缺失</span>`);
  if (mf) bottom.push(`<span class="b">${esc(mf.ext.toUpperCase())}</span>`);
  else bottom.push(`<span class="b">无文件</span>`);
  return (
    `<div class="tile ${S.selected.has(w.id) ? "selected" : ""}" data-id="${w.id}" draggable="true">` +
    `<div class="cover-box">${coverHtml(w)}<div class="badges top">${top.join("")}</div>${left.length ? `<div class="badges left">${left.join("")}</div>` : ""}<div class="badges bottom">${bottom.join("")}</div>` +
    `<button class="t-edit" title="详情与编辑">${svg("edit")}</button></div>` +
    `<div class="t-meta"><div class="t-title" title="${esc(workTitle(w))}">${esc(workTitle(w))}</div>` +
    `<div class="t-author" title="${esc(w.authors.join(" · "))}">${esc(w.authors.join(" · "))}</div>` +
    `</div></div>`
  );
}

function renderGrid() {
  computeVisible();
  const grid = $("#grid");
  const top = grid.scrollTop;
  grid.innerHTML = S.visible.map(tileHtml).join("");
  grid.scrollTop = top;
  $("#view-title").textContent = viewTitle();
  $("#view-count").textContent = `${S.visible.length} 个条目`;
  $("#filter-chips").innerHTML = S.search ? `<span class="chip" data-chip="search">搜索：${esc(S.search)} ${svg("x")}</span>` : "";

  const empty = $("#empty");
  let msg = "";
  if (!S.st?.root) msg = "<b>还没有设置书库根目录</b><span>点右上角 ⋯ →“选择书库根目录”</span>";
  else if (!S.st.library) msg = "<b>根目录下没有书库</b><span>书库根目录下的每个一级子文件夹就是一个书库</span>";
  else if (!S.visible.length) {
    if (S.search) msg = "<b>没有匹配的条目</b><span>换个关键词试试</span>";
    else if (S.view.kind === "removed") msg = "<b>没有已移除的条目</b><span>移除的条目会出现在这里，可以随时恢复</span>";
    else if (S.view.kind === "uncat") msg = "<b>没有未分类的条目</b><span>把条目拖到这里即可取消分类</span>";
    else if (S.scanning && !S.works.length) msg = "<b>正在建立索引…</b>";
    else msg = "<b>这里还没有书</b><span>把书拖到左侧类别上即可归类</span>";
  }
  empty.innerHTML = msg;
  empty.classList.toggle("hidden", !msg);
}

function refreshSelection() {
  for (const el of $$("#grid .tile")) el.classList.toggle("selected", S.selected.has(+el.dataset.id));
}

function renderStatus() {
  const files = S.works.reduce((n, w) => n + (w.removed ? 0 : w.files.length), 0);
  $("#status").textContent = S.st?.library ? `${S.works.length - S.removedCount} 个条目 · ${files} 个文件` : "";
  const st = S.st;
  $("#index-loc").textContent = st?.indexDir ? `索引：${st.indexDir}${st.fallback ? "（书库不可写，已存到软件目录）" : ""}` : "";
  const warn = [];
  if (st && !st.pdfAvailable) warn.push("缺少 pdfium，PDF 封面不可用");
  if (st && !st.djvuAvailable) warn.push("缺少 DjVuLibre，DJVU 封面不可用");
  if (!S.scanning) $("#progress").textContent = S.coversPending ? `正在生成封面 ${S.coversPending}` : warn.join("；");
}

// ---------------- selection & grid events ----------------
const tileOf = (el) => el?.closest?.(".tile");

function selectClick(id, e) {
  if (e.shiftKey && S.anchor != null) {
    const ids = S.visible.map((w) => w.id);
    const a = ids.indexOf(S.anchor);
    const b = ids.indexOf(id);
    if (a >= 0 && b >= 0) {
      if (!e.ctrlKey) S.selected.clear();
      for (let i = Math.min(a, b); i <= Math.max(a, b); i++) S.selected.add(ids[i]);
    }
  } else if (e.ctrlKey || e.metaKey) {
    S.selected.has(id) ? S.selected.delete(id) : S.selected.add(id);
    S.anchor = id;
  } else {
    S.selected = new Set([id]);
    S.anchor = id;
  }
  refreshSelection();
}

function openWork(w) {
  if (w) openDetail(w.id);
}

function bindGrid() {
  const grid = $("#grid");
  grid.addEventListener("click", (e) => {
    const t = tileOf(e.target);
    if (!t) { S.selected.clear(); refreshSelection(); return; }
    const id = +t.dataset.id;
    if (e.target.closest(".t-edit")) {
      S.selected = new Set([id]);
      S.anchor = id;
      refreshSelection();
      openInspect(id);
      return;
    }
    selectClick(id, e);
    if (!e.ctrlKey && !e.metaKey && !e.shiftKey && S.inspectOpen) openInspect(id);
  });
  grid.addEventListener("dblclick", (e) => {
    const t = tileOf(e.target);
    if (!t || e.target.closest(".t-edit")) return;
    openInspect(+t.dataset.id);
  });
  grid.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    const t = tileOf(e.target);
    if (!t) return;
    const id = +t.dataset.id;
    if (!S.selected.has(id)) { S.selected = new Set([id]); S.anchor = id; refreshSelection(); }
    tileMenu(id, e.clientX, e.clientY);
  });
  grid.addEventListener("error", (e) => {
    if (e.target.tagName === "IMG" && e.target.dataset.ph) {
      const w = S.byId.get(+tileOf(e.target)?.dataset.id);
      if (w) { w.hasCover = false; e.target.outerHTML = coverHtml(w); }
    }
  }, true);

  grid.addEventListener("dragstart", (e) => {
    const t = tileOf(e.target);
    if (!t) return;
    const id = +t.dataset.id;
    if (!S.selected.has(id)) { S.selected = new Set([id]); S.anchor = id; refreshSelection(); }
    S.drag = { type: "works", ids: [...S.selected] };
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", "works");
    const ghost = document.createElement("div");
    ghost.className = "drag-ghost";
    ghost.textContent = S.drag.ids.length > 1 ? `${S.drag.ids.length} 个条目` : workTitle(S.byId.get(id)).slice(0, 30);
    document.body.appendChild(ghost);
    e.dataTransfer.setDragImage(ghost, -8, -8);
    setTimeout(() => ghost.remove(), 0);
    for (const sid of S.drag.ids) $(`.tile[data-id="${sid}"]`)?.classList.add("dragging");
  });
  grid.addEventListener("dragover", (e) => {
    const t = tileOf(e.target);
    clearDropMarks();
    if (S.drag?.type !== "works" || !t || S.drag.ids.includes(+t.dataset.id) || S.view.kind === "removed") return;
    e.preventDefault();
    t.classList.add("drop-target");
  });
  grid.addEventListener("drop", async (e) => {
    const t = tileOf(e.target);
    const drag = S.drag;
    clearDropMarks();
    if (drag?.type !== "works" || !t) return;
    e.preventDefault();
    const target = +t.dataset.id;
    if (drag.ids.includes(target)) return;
    await mergeInto(target, drag.ids);
  });
  document.addEventListener("dragend", () => {
    S.drag = null;
    clearDropMarks();
    $$(".tile.dragging").forEach((el) => el.classList.remove("dragging"));
  });
}

function clearDropMarks() {
  $$(".drop-target,.drop-into,.drop-before").forEach((el) => el.classList.remove("drop-target", "drop-into", "drop-before"));
}

// ---------------- actions ----------------
async function mergeInto(target, others) {
  const tw = S.byId.get(target);
  others = others.filter((id) => id !== target);
  if (!tw || !others.length) return;
  const names = others.map((id) => `《${esc(workTitle(S.byId.get(id)))}》`).slice(0, 6).join("、") + (others.length > 6 ? " 等" : "");
  const ok = await confirmBox("合并条目", `把 ${names} 的所有文件并入《${esc(workTitle(tw))}》？<br><span class="muted">合并后可以在详情里把单个文件拆分出去。</span>`, "合并");
  if (!ok) return;
  if ((await call("merge_works", { target, others })) === FAIL) return;
  S.selected = new Set([target]);
  await reload();
  toast(`已合并到《${workTitle(tw)}》`);
}

async function removeWorks(ids) {
  if (!ids.length) return;
  const title = ids.length === 1 ? `《${esc(workTitle(S.byId.get(ids[0])))}》` : `${ids.length} 个条目`;
  const ok = await confirmBox("移除条目", `从索引中移除 ${title}？<br><span class="muted">磁盘上的文件不会被删除；之后可以在“已移除”里恢复。</span>`, "移除", true);
  if (!ok) return;
  if ((await call("remove_works", { ids })) === FAIL) return;
  if (ids.includes(S.detailId)) closeDetail();
  if (ids.includes(S.inspectId)) closeInspect();
  S.selected.clear();
  await reload();
  toast(`已移除 ${ids.length} 个条目`);
}

async function restoreWorks(ids) {
  if ((await call("restore_works", { ids })) === FAIL) return;
  S.selected.clear();
  await reload();
  toast(`已恢复 ${ids.length} 个条目`);
}

async function setCategory(ids, categoryId) {
  if ((await call("set_works_category", { ids, categoryId })) === FAIL) return;
  await reload();
  toast(`已把 ${ids.length} 个条目${categoryId == null ? "设为未分类" : `移到“${catPath(categoryId)}”`}`);
}

async function rescan() {
  if (!S.st?.library) return;
  await call("rescan");
}

// ---------------- context menus ----------------
function showMenu(items, x, y) {
  const m = $("#menu");
  m.innerHTML = items
    .map((it, i) => {
      if (it === "-") return `<div class="sep"></div>`;
      if (it.label) return `<div class="label">${esc(it.label)}</div>`;
      return `<div class="mi ${it.danger ? "danger" : ""} ${it.disabled ? "disabled" : ""}" data-i="${i}">${svg(it.icon || "")}<span>${esc(it.text)}</span></div>`;
    })
    .join("");
  m.classList.remove("hidden");
  const r = m.getBoundingClientRect();
  m.style.left = `${Math.min(x, innerWidth - r.width - 8)}px`;
  m.style.top = `${Math.min(y, innerHeight - r.height - 8)}px`;
  m.onclick = (e) => {
    const el = e.target.closest(".mi");
    if (!el) return;
    hideMenu();
    items[+el.dataset.i].run?.();
  };
}
const hideMenu = () => $("#menu").classList.add("hidden");

function tileMenu(id, x, y) {
  const ids = [...S.selected];
  const w = S.byId.get(id);
  const single = ids.length === 1;
  const mf = mainFile(w);
  const items = [];
  if (single) {
    items.push({ icon: "open", text: "打开条目", run: () => openInspect(id) });
    items.push({ icon: "open", text: "打开详情页", run: () => openDetail(id) });
    items.push({ icon: "open", text: "用阅读器打开", disabled: !mf, run: () => call("open_file", { fileId: mf.id }) });
    items.push({ icon: "edit", text: "编辑条目…", run: () => editWork(id) });
    items.push({ icon: "reveal", text: "在资源管理器中显示", disabled: !mf, run: () => call("reveal_file", { fileId: mf.id }) });
    items.push("-");
  } else items.push({ label: `已选 ${ids.length} 个条目` });
  if (w.removed) {
    items.push({ icon: "restore", text: "恢复", run: () => restoreWorks(ids) });
  } else {
    items.push({ icon: "move", text: "移动到类别…", run: () => pickCategory(ids) });
    if (!single) items.push({ icon: "merge", text: `合并到《${workTitle(w).slice(0, 18)}》`, run: () => mergeInto(id, ids) });
    if (single && mf) items.push({ icon: "image", text: "重新生成封面", run: () => call("regen_cover", { fileId: mf.id }) });
    items.push("-");
    items.push({ icon: "trash", text: single ? "移除条目" : `移除 ${ids.length} 个条目`, danger: true, run: () => removeWorks(ids) });
  }
  showMenu(items, x, y);
}

function catMenu(id, x, y) {
  const c = S.catById.get(id);
  showMenu(
    [
      { label: catPath(id) },
      { icon: "plus", text: "新建子类别…", run: () => newCategory(id) },
      { icon: "edit", text: "重命名…", run: () => renameCategory(id) },
      { icon: "root", text: "移到顶层", disabled: c.parentId == null, run: () => moveCategory(id, null, null) },
      "-",
      { icon: "trash", text: "删除类别…", danger: true, run: () => deleteCategory(id) },
    ],
    x,
    y,
  );
}

async function regenAllCovers() {
  const ok = await confirmBox(
    "重新生成封面",
    "重新生成当前书库里所有文件的封面？<br><span class=\"muted\">只重做封面缓存，不会改动书本文件。书多时会花一些时间。</span>",
    "重新生成",
  );
  if (!ok) return;
  const n = await call("regen_all_covers");
  if (n === FAIL) return;
  toast(n ? `正在重新生成 ${n} 个封面` : "没有需要生成的封面");
}

function mainMenu(x, y) {
  const th = (v, t) => ({ icon: S.theme === v ? "check" : "", text: t, run: () => { S.theme = v; applyTheme(); saveSettings(); } });
  showMenu(
    [
      { icon: "root", text: "选择书库根目录…", run: chooseRoot },
      { icon: "reveal", text: "打开书库文件夹", disabled: !S.st?.library, run: () => call("open_library_folder") },
      { icon: "scan", text: "重新扫描 (F5)", disabled: !S.st?.library, run: rescan },
      { icon: "image", text: "重新生成全部封面", disabled: !S.st?.library, run: regenAllCovers },
      "-",
      { label: "外观" },
      th("system", "跟随系统"),
      th("light", "浅色"),
      th("dark", "深色"),
    ],
    x,
    y,
  );
}

// ---------------- categories ----------------
async function newCategory(parentId) {
  const name = await promptBox(parentId == null ? "新建类别" : `在“${catPath(parentId)}”下新建子类别`, "名称", "");
  if (!name) return;
  const id = await call("create_category", { parentId, name });
  if (id === FAIL) return;
  if (parentId != null) S.collapsed.delete(parentId);
  saveCollapsed();
  await reload();
}

async function renameCategory(id) {
  const c = S.catById.get(id);
  const name = await promptBox("重命名类别", "名称", c.name);
  if (!name || name === c.name) return;
  if ((await call("rename_category", { id, name })) === FAIL) return;
  await reload();
}

async function deleteCategory(id) {
  const n = S.subtreeCount.get(id) || 0;
  const sub = (S.kids.get(id) || []).length;
  const ok = await confirmBox(
    "删除类别",
    `删除“${esc(catPath(id))}”${sub ? "及其所有子类别" : ""}？<br><span class="muted">${n ? `其中的 ${n} 个条目会移到“未分类”，` : ""}不会删除任何文件。</span>`,
    "删除",
    true,
  );
  if (!ok) return;
  if ((await call("delete_category", { id })) === FAIL) return;
  await reload();
}

async function moveCategory(id, parentId, beforeId) {
  if ((await call("move_category", { id, parentId, beforeId })) === FAIL) return;
  if (parentId != null) S.collapsed.delete(parentId);
  saveCollapsed();
  await reload();
}

function categoryOptions(selected, { none = "未分类" } = {}) {
  const out = [`<option value="" ${selected == null ? "selected" : ""}>${esc(none)}</option>`];
  const walk = (p, d) => {
    for (const c of S.kids.get(p) || []) {
      out.push(`<option value="${c.id}" ${selected === c.id ? "selected" : ""}>${"\u3000".repeat(d)}${esc(c.name)}</option>`);
      walk(c.id, d + 1);
    }
  };
  walk(null, 0);
  return out.join("");
}

async function pickCategory(ids) {
  const first = S.byId.get(ids[0])?.categoryId ?? null;
  const res = await modal({
    title: ids.length > 1 ? `移动 ${ids.length} 个条目到类别` : "移动到类别",
    body: `<select class="field" id="m-cat" size="12" style="height:auto">${categoryOptions(first)}</select>`,
    ok: "移动",
    onOk: (root) => ({ value: $("#m-cat", root).value }),
  });
  if (!res) return;
  await setCategory(ids, res.value === "" ? null : +res.value);
}

function saveCollapsed() {
  if (S.st?.library) localStorage.setItem(`libadmin.collapsed.${S.st.library}`, JSON.stringify([...S.collapsed]));
}
function loadCollapsed() {
  try {
    S.collapsed = new Set(JSON.parse(localStorage.getItem(`libadmin.collapsed.${S.st?.library}`) || "[]"));
  } catch {
    S.collapsed = new Set();
  }
}

function bindSidebar() {
  $$(".seg button").forEach((b) =>
    b.addEventListener("click", () => {
      S.tab = b.dataset.tab;
      $$(".seg button").forEach((x) => x.classList.toggle("active", x === b));
      for (const t of ["cat", "author", "lang"]) $(`#panel-${t}`).classList.toggle("hidden", t !== S.tab);
    }),
  );
  $("#author-filter").addEventListener("input", renderAuthors);
  $("#btn-add-cat").addEventListener("click", () => newCategory(null));

  const side = $(".sidebar");
  side.addEventListener("click", (e) => {
    const row = e.target.closest(".row");
    if (!row) return;
    if (row.dataset.nav) setView({ kind: row.dataset.nav });
    else if (row.dataset.cat) {
      const id = +row.dataset.cat;
      if (e.target.closest(".caret") && !e.target.closest(".caret.leaf")) {
        S.collapsed.has(id) ? S.collapsed.delete(id) : S.collapsed.add(id);
        saveCollapsed();
        renderSidebar();
      } else setView({ kind: "cat", id });
    } else if (row.dataset.author) setView({ kind: "author", value: row.dataset.author });
    else if (row.dataset.lang) setView({ kind: "lang", value: row.dataset.lang });
  });
  side.addEventListener("dblclick", (e) => {
    const row = e.target.closest(".row[data-cat]");
    if (row && !e.target.closest(".caret")) renameCategory(+row.dataset.cat);
  });
  side.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    const row = e.target.closest(".row[data-cat]");
    if (row) catMenu(+row.dataset.cat, e.clientX, e.clientY);
    else if (e.target.closest("#panel-cat")) showMenu([{ icon: "plus", text: "新建类别…", run: () => newCategory(null) }], e.clientX, e.clientY);
  });

  side.addEventListener("dragstart", (e) => {
    const row = e.target.closest(".row[data-cat]");
    if (!row) return;
    S.drag = { type: "cat", id: +row.dataset.cat };
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", "cat");
  });

  const dropInfo = (e) => {
    const d = S.drag;
    if (!d) return null;
    const row = e.target.closest(".row");
    const head = e.target.closest("#tree-root-drop");
    if (d.type === "works") {
      if (row?.dataset.cat) return { el: row, cls: "drop-into", act: () => setCategory(d.ids, +row.dataset.cat) };
      if (row?.dataset.nav === "uncat") return { el: row, cls: "drop-into", act: () => setCategory(d.ids, null) };
      if (row?.dataset.nav === "removed" && S.view.kind !== "removed") return { el: row, cls: "drop-into", act: () => removeWorks(d.ids) };
      if (row?.dataset.nav === "all" && S.view.kind === "removed") return { el: row, cls: "drop-into", act: () => restoreWorks(d.ids) };
      return null;
    }
    if (d.type === "cat") {
      if (head) return { el: head, cls: "drop-into", act: () => moveCategory(d.id, null, null) };
      if (!row?.dataset.cat) return null;
      const target = +row.dataset.cat;
      if (subtreeIds(d.id).has(target)) return null;
      const r = row.getBoundingClientRect();
      const tc = S.catById.get(target);
      if (e.clientY < r.top + r.height * 0.3) return { el: row, cls: "drop-before", act: () => moveCategory(d.id, tc.parentId ?? null, target) };
      return { el: row, cls: "drop-into", act: () => moveCategory(d.id, target, null) };
    }
    return null;
  };
  side.addEventListener("dragover", (e) => {
    clearDropMarks();
    const info = dropInfo(e);
    if (!info) return;
    e.preventDefault();
    info.el.classList.add(info.cls);
  });
  side.addEventListener("drop", (e) => {
    const info = dropInfo(e);
    clearDropMarks();
    S.drag = null;
    if (!info) return;
    e.preventDefault();
    info.act();
  });
}

// ---------------- modal helpers ----------------
function modal({ title, body, ok = "确定", cancel = "取消", danger = false, onOk, onOpen }) {
  return new Promise((resolve) => {
    const wrap = document.createElement("div");
    wrap.className = "modal-wrap";
    wrap.innerHTML = `<div class="modal"><h2>${esc(title)}</h2><div class="m-body">${body}</div><div class="m-foot">${
      cancel ? `<button class="btn" data-a="cancel">${esc(cancel)}</button>` : ""
    }<button class="btn ${danger ? "solid-danger" : "primary"}" data-a="ok">${esc(ok)}</button></div></div>`;
    $("#modal-root").appendChild(wrap);
    const close = (v) => { wrap.remove(); document.removeEventListener("keydown", key, true); resolve(v); };
    const accept = async () => {
      const v = onOk ? await onOk(wrap) : true;
      if (v !== false && v !== undefined) close(v);
    };
    const key = (e) => {
      if (e.key === "Escape") { e.stopPropagation(); close(null); }
      else if (e.key === "Enter" && !e.shiftKey && e.target.tagName !== "TEXTAREA" && e.target.tagName !== "SELECT") { e.preventDefault(); e.stopPropagation(); accept(); }
    };
    document.addEventListener("keydown", key, true);
    wrap.addEventListener("mousedown", (e) => { if (e.target === wrap) close(null); });
    wrap.addEventListener("click", (e) => {
      const a = e.target.closest("[data-a]")?.dataset.a;
      if (a === "cancel") close(null);
      else if (a === "ok") accept();
    });
    onOpen?.(wrap);
    ($("input,textarea,select", wrap) || $("[data-a=ok]", wrap)).focus();
  });
}

const confirmBox = (title, html, ok = "确定", danger = false) => modal({ title, body: `<div>${html}</div>`, ok, danger }).then(Boolean);

async function promptBox(title, label, value) {
  const r = await modal({
    title,
    body: `<label>${esc(label)}</label><input class="field" id="m-input" value="${esc(value)}" spellcheck="false">`,
    onOk: (root) => {
      const v = $("#m-input", root).value.trim();
      return v ? { v } : false;
    },
    onOpen: (root) => setTimeout(() => $("#m-input", root).select(), 0),
  });
  return r?.v || null;
}

const splitAuthors = (s) => s.split(/[;；\n]+/).map((x) => x.trim()).filter(Boolean);

async function newWork() {
  if (!S.st?.library) return;
  const cat = S.view.kind === "cat" ? S.view.id : null;
  const res = await modal({
    title: "新建条目",
    body: `<label>英文名</label><input class="field" id="n-en" spellcheck="false">
      <label>中文名</label><input class="field" id="n-zh" spellcheck="false">
      <label>作者（多位用“；”分隔）</label><input class="field" id="n-authors" spellcheck="false">
      <label>类别</label><select class="field" id="n-cat">${categoryOptions(cat)}</select>
      <label>备注</label><textarea class="field" id="n-notes" rows="3"></textarea>
      <label class="check" style="margin:0"><input type="checkbox" id="n-files" checked>创建后选择要加入的书籍文件（不会复制文件）</label>`,
    ok: "创建",
    onOk: async (root) => {
      const titleEn = $("#n-en", root).value.trim();
      const titleZh = $("#n-zh", root).value.trim();
      if (!titleEn && !titleZh) { $("#n-zh", root).focus(); return false; }
      const c = $("#n-cat", root).value;
      const id = await call("create_work", { titleEn, titleZh, authors: splitAuthors($("#n-authors", root).value), categoryId: c === "" ? null : +c, notes: $("#n-notes", root).value });
      if (id === FAIL) return false;
      return { id, files: $("#n-files", root).checked };
    },
  });
  if (!res) return;
  if (res.files) await call("add_files", { workId: res.id });
  await reload();
  S.selected = new Set([res.id]);
  refreshSelection();
  openInspect(res.id);
}

// ---------------- detail page ----------------
const fmtSize = (n) => (n >= 1 << 30 ? `${(n / (1 << 30)).toFixed(2)} GB` : n >= 1 << 20 ? `${(n / (1 << 20)).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`);
const fileStem = (f) => f.name.includes(".") ? f.name.slice(0, f.name.lastIndexOf(".")) : f.name;
const fileTitle = (f) => (f.title && f.title.trim()) || fileStem(f);
const fileLoc = (f) => (f.external ? f.relPath : f.folder || "书库根目录");

function syncLayout() {
  const detail = S.detailId != null;
  const app = document.querySelector(".app");
  app.classList.toggle("page-detail", detail);
  $("#view-library").classList.toggle("hidden", detail);
  $("#view-detail").classList.toggle("hidden", !detail);
}

function openDetail(id) {
  S.detailId = id;
  S.fileSearch = "";
  syncLayout();
  renderDetail();
  const w = S.byId.get(id);
  if (w?.files.length) call("ensure_covers", { fileIds: w.files.map((f) => f.id) });
  $("#view-detail").scrollTop = 0;
}

function closeDetail() {
  S.detailId = null;
  syncLayout();
  ensureInspect();
  renderSidebar();
  renderGrid();
}

function applyPaneWidths() {
  document.documentElement.style.setProperty("--side", `${S.side}px`);
  document.documentElement.style.setProperty("--inspect", S.inspectOpen ? `${S.inspectW}px` : "0px");
  document.querySelector(".app").classList.toggle("inspect-collapsed", !S.inspectOpen);
  const btn = $("#btn-inspect");
  if (btn) {
    btn.classList.toggle("active", S.inspectOpen);
    btn.title = S.inspectOpen ? "收回条目栏" : "显示条目栏";
  }
}

function setInspectOpen(open) {
  if (S.inspectOpen === open) {
    if (open) ensureInspect();
    return;
  }
  S.inspectOpen = open;
  if (open) ensureInspect();
  applyPaneWidths();
  saveSettings();
}

const SIDE_MIN = 180, SIDE_MAX = 480;
const INSPECT_MIN = 280, INSPECT_MAX = 720;
const clamp = (n, a, b) => Math.round(Math.min(b, Math.max(a, n)));

function bindResizers() {
  const start = (which) => (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const el = e.currentTarget;
    el.classList.add("dragging");
    document.body.classList.add("resizing");
    document.querySelector(".app").classList.add("resizing");
    const startX = e.clientX;
    const origin = which === "side" ? S.side : S.inspectW;
    el.setPointerCapture(e.pointerId);
    const onMove = (ev) => {
      const dx = ev.clientX - startX;
      if (which === "side") S.side = clamp(origin + dx, SIDE_MIN, SIDE_MAX);
      else S.inspectW = clamp(origin - dx, INSPECT_MIN, INSPECT_MAX);
      applyPaneWidths();
    };
    const onUp = () => {
      el.classList.remove("dragging");
      document.body.classList.remove("resizing");
      document.querySelector(".app").classList.remove("resizing");
      el.removeEventListener("pointermove", onMove);
      el.removeEventListener("pointerup", onUp);
      saveSettings();
    };
    el.addEventListener("pointermove", onMove);
    el.addEventListener("pointerup", onUp);
  };
  $("#gutter-side").addEventListener("pointerdown", start("side"));
  $("#gutter-inspect").addEventListener("pointerdown", start("inspect"));
  $("#gutter-side").addEventListener("dblclick", () => { S.side = 260; applyPaneWidths(); saveSettings(); });
  $("#gutter-inspect").addEventListener("dblclick", () => { S.inspectW = 400; applyPaneWidths(); saveSettings(); });
}

function openInspect(id) {
  const switching = S.inspectId !== id;
  S.inspectId = id;
  if (!S.inspectOpen) {
    S.inspectOpen = true;
    applyPaneWidths();
    saveSettings();
  }
  renderInspect(switching);
  syncLayout();
  const w = S.byId.get(id);
  if (w?.files.length) call("ensure_covers", { fileIds: w.files.filter((f) => !f.hasCover).map((f) => f.id) });
}

function closeInspect() {
  S.inspectId = null;
  renderInspect();
}

function ensureInspect() {
  if (S.inspectId != null && S.byId.has(S.inspectId)) {
    renderInspect();
    return;
  }
  const first = S.visible[0];
  if (first) {
    S.inspectId = first.id;
    if (!S.selected.size) {
      S.selected = new Set([first.id]);
      S.anchor = first.id;
      refreshSelection();
    }
    renderInspect();
    const w = first;
    if (w.files.length) call("ensure_covers", { fileIds: w.files.filter((f) => !f.hasCover).map((f) => f.id) });
  } else {
    S.inspectId = null;
    renderInspect();
  }
}

function activeWork() {
  return S.byId.get(S.detailId ?? S.inspectId);
}

function fileCoverHtml(f) {
  const title = fileTitle(f);
  const ph = `<div class="ph" style="--h:${hue(title)}"><div class="ph-title">${esc(title)}</div><div class="ph-author">${esc(f.ext.toUpperCase())}</div></div>`;
  if (!f.hasCover) return ph;
  return `<img src="http://cover.localhost/${f.id}?v=${S.coverVer.get(f.id) || 0}" loading="lazy" decoding="async" draggable="false" alt="" data-ph="1">`;
}

function fileTileHtml(w, f) {
  const isMain = f.id === (w.mainFileId ?? w.files[0]?.id);
  const top = [];
  const left = [];
  if (f.pages) top.push(`<span class="b">${f.pages} 页</span>`);
  if (f.missing) top.push(`<span class="b missing">缺失</span>`);
  const bottom = [`<span class="b">${esc((f.ext || "file").toUpperCase())}</span>`];
  if (f.role === "译本") left.push(`<span class="b trans">译本</span>`);
  else if (f.role && f.role !== "主本") bottom.push(`<span class="b ${f.role === "配套资料" ? "supp" : ""}">${esc(f.role)}</span>`);
  else if (isMain && w.files.length > 1) bottom.push(`<span class="b files">主本</span>`);
  return `<div class="tile file-tile ${isMain ? "main" : ""}" data-fid="${f.id}">
    <div class="cover-box">${fileCoverHtml(f)}<div class="badges top">${top.join("")}</div>${left.length ? `<div class="badges left">${left.join("")}</div>` : ""}<div class="badges bottom">${bottom.join("")}</div>
      <button class="t-edit" title="编辑此项">${svg("edit")}</button>
    </div>
    <div class="t-meta">
      <div class="t-title" title="${esc(fileTitle(f))}">${esc(fileTitle(f))}</div>
      <div class="t-author" title="${esc(f.relPath)}">${esc(fileLoc(f))}</div>
    </div>
  </div>`;
}

function visibleFiles(w) {
  const q = S.fileSearch.toLowerCase();
  let files = w.files.filter((f) => !q || [f.name, f.title, f.relPath, f.role].join("\n").toLowerCase().includes(q));
  const main = w.mainFileId ?? w.files[0]?.id;
  if (S.fileSort === "pages") files.sort((a, b) => (b.pages || 0) - (a.pages || 0) || collator.compare(fileTitle(a), fileTitle(b)));
  else if (S.fileSort === "role") files.sort((a, b) => ROLES.indexOf(a.role) - ROLES.indexOf(b.role) || collator.compare(fileTitle(a), fileTitle(b)));
  else files.sort((a, b) => (a.id === main ? -1 : b.id === main ? 1 : 0) || collator.compare(fileTitle(a), fileTitle(b)));
  return files;
}

function renderDetail() {
  const w = S.byId.get(S.detailId);
  const page = $("#view-detail");
  if (!w) { closeDetail(); return; }
  const mf = mainFile(w);
  const cat = S.catById.has(w.categoryId) ? catPath(w.categoryId) : "未分类";
  const ok = w.files.filter((f) => !f.missing).length;
  const langs = [...workLangs(w)];
  const stats = [
    `${w.files.length} 个文件`,
    w.files.length ? (ok === w.files.length ? "均可打开" : `${ok} 个可打开`) : "还没有文件",
    langs.join(" · "),
    cat,
  ].filter(Boolean).join(" · ");
  const path = mf ? (mf.external ? mf.relPath : [S.st?.library, mf.folder].filter(Boolean).join(" / ") || S.st?.library || "") : "";
  const files = visibleFiles(w);
  const fileTiles = files.map((f) => fileTileHtml(w, f)).join("");
  page.innerHTML = `
    <div class="page-inner">
      <nav class="crumbs">
        <button class="crumb" data-act="back" title="返回">${svg("back")}</button>
        <button class="crumb" data-act="lib">${esc(S.st?.library || "书库")}</button>
        <span class="sep">/</span>
        <button class="crumb" data-act="cat">${esc(cat)}</button>
        <span class="sep">/</span>
        <span class="here" title="${esc(workTitle(w))}">${esc(workTitle(w))}</span>
        <span class="spacer"></span>
        <label class="search"><svg viewBox="0 0 24 24"><circle cx="11" cy="11" r="6.5"/><path d="M16 16l4.5 4.5"/></svg>
          <input id="file-search" type="search" placeholder="搜索此条目中的文件…" value="${esc(S.fileSearch)}" autocomplete="off" spellcheck="false">
        </label>
        <select id="file-sort" class="ghost-select">
          <option value="name" ${S.fileSort === "name" ? "selected" : ""}>按名称</option>
          <option value="role" ${S.fileSort === "role" ? "selected" : ""}>按角色</option>
          <option value="pages" ${S.fileSort === "pages" ? "selected" : ""}>按页数</option>
        </select>
      </nav>
      <header class="entry-head">
        <div class="entry-cover">${coverHtml(w)}</div>
        <div class="entry-info">
          <h1>${esc(workTitle(w))}</h1>
          <div class="entry-names">
            <div><span class="k">英文名</span>${workTitleEn(w) ? esc(workTitleEn(w)) : `<span class="muted">未填写</span>`}</div>
            <div><span class="k">中文名</span>${workTitleZh(w) ? esc(workTitleZh(w)) : `<span class="muted">未填写</span>`}</div>
          </div>
          <div class="entry-stats">${esc(w.authors.join("、") || "未知作者")} · ${esc(stats)}</div>
          ${path ? `<div class="entry-path">${esc(path)}</div>` : ""}
          ${w.notes ? `<div class="entry-notes">${esc(w.notes)}</div>` : ""}
        </div>
        <div class="entry-actions">
          <button class="btn primary" data-act="read" ${mf && !mf.missing ? "" : "disabled"}>${svg("open")}打开主文件</button>
          <button class="btn" data-act="reveal" ${mf ? "" : "disabled"}>${svg("reveal")}在资源管理器中打开</button>
          <button class="btn" data-act="edit">${svg("edit")}编辑条目</button>
          <button class="btn" data-act="add">${svg("plus")}添加文件</button>
          ${w.titleManual && w.files.length ? `<button class="btn" data-act="auto">恢复自动识别</button>` : ""}
          ${w.removed
            ? `<button class="btn" data-act="restore">${svg("restore")}恢复条目</button>`
            : `<button class="btn danger" data-act="remove">${svg("trash")}移除条目</button>`}
        </div>
      </header>
      <section class="entry-files">
        <h2>包含内容 <span>共 ${w.files.length} 个${files.length !== w.files.length ? ` · 显示 ${files.length}` : ""}</span></h2>
        ${fileTiles ? `<div class="grid" id="file-grid">${fileTiles}</div>` : `<div class="page-empty">${w.files.length ? "没有匹配的文件" : "还没有文件，点“添加文件”把书链到这个条目（不会复制文件）"}</div>`}
      </section>
    </div>`;
  const img = $(".entry-cover img", page);
  if (img) img.onerror = () => { w.hasCover = false; $(".entry-cover", page).innerHTML = coverHtml(w); };
}

function renderInspect(animate = false) {
  const pane = $("#inspect");
  const w = S.inspectId != null ? S.byId.get(S.inspectId) : null;
  if (!w) {
    pane.innerHTML = `
      <div class="inspect-top">
        <span class="ttl">条目信息</span>
        <span class="spacer"></span>
        <button class="mini" data-act="collapse-inspect" title="收回">${svg("chev")}</button>
      </div>
      <div class="inspect-body">
        <div class="inspect-empty">双击左侧磁贴打开条目</div>
      </div>`;
    return;
  }
  const mf = mainFile(w);
  const cat = S.catById.has(w.categoryId) ? catPath(w.categoryId) : "未分类";
  const path = mf ? (mf.external ? mf.relPath : [S.st?.library, mf.folder].filter(Boolean).join(" / ") || S.st?.library || "") : "";
  const main = w.mainFileId ?? w.files[0]?.id;
  const files = [...w.files].sort((a, b) => (a.id === main ? -1 : b.id === main ? 1 : 0) || collator.compare(fileTitle(a), fileTitle(b)));
  const tiles = files.map((f) => fileTileHtml(w, f)).join("");
  pane.innerHTML = `
    <div class="inspect-top">
      <span class="ttl">条目信息</span>
      <span class="spacer"></span>
      <button class="mini" data-act="page" title="打开详情页">${svg("all")}</button>
      <button class="mini" data-act="collapse-inspect" title="收回">${svg("chev")}</button>
    </div>
    <div class="inspect-body">
      <div class="inspect-head">
        <div class="inspect-cover">${coverHtml(w)}</div>
        <div class="inspect-info">
          <h2>${esc(workTitle(w))}</h2>
          <div class="entry-names">
            <div><span class="k">英文名</span>${workTitleEn(w) ? esc(workTitleEn(w)) : `<span class="muted">未填写</span>`}</div>
            <div><span class="k">中文名</span>${workTitleZh(w) ? esc(workTitleZh(w)) : `<span class="muted">未填写</span>`}</div>
          </div>
          <div class="entry-stats">${esc(w.authors.join(" · ") || "未知作者")}</div>
          <div class="entry-path">${esc(cat)}${path ? ` · ${esc(path)}` : ""}</div>
        </div>
      </div>
      ${w.notes ? `<div class="entry-notes">${esc(w.notes)}</div>` : ""}
      <div class="inspect-actions">
        <button class="btn primary" data-act="read" ${mf && !mf.missing ? "" : "disabled"}>${svg("open")}打开</button>
        <button class="btn" data-act="reveal" ${mf ? "" : "disabled"}>${svg("reveal")}所在文件夹</button>
        <button class="btn" data-act="edit">${svg("edit")}编辑</button>
        <button class="btn" data-act="add">${svg("plus")}添加文件</button>
        ${w.titleManual && w.files.length ? `<button class="btn" data-act="auto">恢复识别</button>` : ""}
        ${w.removed
          ? `<button class="btn" data-act="restore">${svg("restore")}恢复</button>`
          : `<button class="btn danger" data-act="remove">${svg("trash")}移除</button>`}
      </div>
      <div class="d-section" style="margin:4px 0 8px;font-size:12px;font-weight:600;color:var(--muted)">包含内容 · ${w.files.length}</div>
      ${tiles ? `<div class="inspect-grid">${tiles}</div>` : `<div class="muted">还没有文件，点“添加文件”把书链到这个条目</div>`}
    </div>`;
  const img = $(".inspect-cover img", pane);
  if (img) img.onerror = () => { w.hasCover = false; $(".inspect-cover", pane).innerHTML = coverHtml(w); };
  if (animate) $(".inspect-body", pane)?.classList.add("enter");
}

async function handleDetailAct(act, fid) {
  if (act === "collapse-inspect") { setInspectOpen(false); return; }
  const w = activeWork();
  if (!w) return;
  const mf = mainFile(w);
  const f = fid != null ? w.files.find((x) => x.id === fid) : null;
  if (act === "page") { openDetail(w.id); return; }
  if (act === "edit-file" && f) { editFile(f.id); return; }
  if (act === "back" || act === "lib") closeDetail();
  else if (act === "cat") setView(w.categoryId != null && S.catById.has(w.categoryId) ? { kind: "cat", id: w.categoryId } : { kind: "uncat" });
  else if (act === "read") { if (mf) call("open_file", { fileId: mf.id }); }
  else if (act === "reveal") { if (mf) call("reveal_file", { fileId: mf.id }); }
  else if (act === "edit") editWork(w.id);
  else if (act === "add") {
    const n = await call("add_files", { workId: w.id });
    if (n !== FAIL && n) { await reload(); toast(`已加入 ${n} 个文件`); }
  } else if (act === "auto") {
    if ((await call("reset_work_title", { id: w.id })) !== FAIL) { await reload(); toast("已按文件重新识别标题和作者"); }
  } else if (act === "remove") removeWorks([w.id]);
  else if (act === "restore") restoreWorks([w.id]);
  else if (act === "open" && f) call("open_file", { fileId: f.id });
  else if (act === "reveal-file" && f) call("reveal_file", { fileId: f.id });
  else if (act === "main" && f) {
    if ((await call("set_main_file", { workId: w.id, fileId: f.id })) !== FAIL) await reload();
  } else if (act === "split" && f) {
    if ((await call("split_file", { fileId: f.id })) !== FAIL) { await reload(); toast(`“${f.name}”已拆分为独立条目`); }
  } else if (act === "drop-file" && f) {
    const ok = await confirmBox(
      "从条目移除文件",
      `把“${esc(f.name)}”从这个条目中移除？<br><span class="muted">${f.external ? "这是书库外的文件，只会删除链接。" : "文件会变成一个隐藏条目，可在“已移除”中找回。"}磁盘上的文件不会被删除。</span>`,
      "移除",
      true,
    );
    if (ok && (await call("remove_file", { fileId: f.id })) !== FAIL) await reload();
  } else if (act === "cover" && f) call("regen_cover", { fileId: f.id });
}

function fileMenu(fid, x, y) {
  const w = activeWork();
  const f = w?.files.find((x) => x.id === fid);
  if (!f) return;
  const isMain = f.id === (w.mainFileId ?? w.files[0]?.id);
  showMenu(
    [
      { icon: "open", text: "打开", disabled: f.missing, run: () => handleDetailAct("open", fid) },
      { icon: "edit", text: "编辑名称与信息…", run: () => editFile(fid) },
      ...(f.titleManual ? [{ icon: "scan", text: "恢复自动识别", run: async () => {
        if ((await call("reset_file_info", { fileId: fid })) !== FAIL) { await reload(); toast("已按文件名重新识别"); }
      } }] : []),
      { icon: "reveal", text: "在资源管理器中显示", run: () => handleDetailAct("reveal-file", fid) },
      { icon: "star", text: isMain ? "已是主文件" : "设为主文件", disabled: isMain, run: () => handleDetailAct("main", fid) },
      { icon: "image", text: "重新生成封面", run: () => handleDetailAct("cover", fid) },
      "-",
      { label: "角色" },
      ...ROLES.map((r) => ({ icon: f.role === r ? "check" : "", text: r, run: async () => {
        if ((await call("set_role", { fileId: fid, role: r })) !== FAIL) await reload();
      } })),
      "-",
      { icon: "split", text: "拆分为独立条目", disabled: w.files.length < 2, run: () => handleDetailAct("split", fid) },
      { icon: "minus", text: "从此条目移除", danger: true, run: () => handleDetailAct("drop-file", fid) },
    ],
    x,
    y,
  );
}

function bindDetail() {
  const page = $("#view-detail");
  page.addEventListener("click", (e) => {
    const tile = e.target.closest(".file-tile");
    if (e.target.closest(".t-edit") && tile) { editFile(+tile.dataset.fid); return; }
    const act = e.target.closest("[data-act]")?.dataset.act;
    if (act) { handleDetailAct(act); return; }
    if (tile) {
      $$(".file-tile", page).forEach((el) => el.classList.toggle("selected", el === tile));
    }
  });
  page.addEventListener("dblclick", (e) => {
    const tile = e.target.closest(".file-tile");
    if (tile) call("open_file", { fileId: +tile.dataset.fid });
  });
  page.addEventListener("contextmenu", (e) => {
    const tile = e.target.closest(".file-tile");
    if (!tile) return;
    e.preventDefault();
    fileMenu(+tile.dataset.fid, e.clientX, e.clientY);
  });
  page.addEventListener("input", (e) => {
    if (e.target.id === "file-search") {
      S.fileSearch = e.target.value.trim();
      const pos = e.target.selectionStart;
      renderDetail();
      const box = $("#file-search");
      if (box) { box.focus(); box.setSelectionRange(pos, pos); }
    }
  });
  page.addEventListener("change", (e) => {
    if (e.target.id === "file-sort") { S.fileSort = e.target.value; renderDetail(); }
  });
  page.addEventListener("error", (e) => {
    if (e.target.tagName !== "IMG" || !e.target.dataset.ph) return;
    const tile = e.target.closest(".file-tile");
    const w = activeWork();
    const f = w?.files.find((x) => x.id === +(tile?.dataset.fid || 0));
    if (f) { f.hasCover = false; e.target.outerHTML = fileCoverHtml(f); }
  }, true);
}

function bindInspect() {
  const pane = $("#inspect");
  pane.addEventListener("click", (e) => {
    const tile = e.target.closest(".file-tile");
    if (e.target.closest(".t-edit") && tile) { editFile(+tile.dataset.fid); return; }
    const act = e.target.closest("[data-act]")?.dataset.act;
    if (act) { handleDetailAct(act); return; }
    if (tile) {
      $$(".file-tile", pane).forEach((el) => el.classList.toggle("selected", el === tile));
    }
  });
  pane.addEventListener("dblclick", (e) => {
    const tile = e.target.closest(".file-tile");
    if (tile) call("open_file", { fileId: +tile.dataset.fid });
  });
  pane.addEventListener("contextmenu", (e) => {
    const tile = e.target.closest(".file-tile");
    if (!tile) return;
    e.preventDefault();
    fileMenu(+tile.dataset.fid, e.clientX, e.clientY);
  });
  pane.addEventListener("error", (e) => {
    if (e.target.tagName !== "IMG" || !e.target.dataset.ph) return;
    const tile = e.target.closest(".file-tile");
    const w = S.byId.get(S.inspectId);
    const f = w?.files.find((x) => x.id === +(tile?.dataset.fid || 0));
    if (f) { f.hasCover = false; e.target.outerHTML = fileCoverHtml(f); }
  }, true);
}

async function editFile(id) {
  const w = activeWork();
  const f = w?.files.find((x) => x.id === id);
  if (!f) return;
  const langs = [...new Set(["中文", "外文", f.language].filter(Boolean))];
  const roleOpts = ROLES.map((r) => `<option value="${esc(r)}" ${f.role === r ? "selected" : ""}>${esc(r)}</option>`).join("");
  const res = await modal({
    title: "编辑此项",
    body: `<div class="muted" style="margin-bottom:4px">只改条目里的显示信息，不会改磁盘上的文件。</div>
      <label>名称</label><input class="field" id="f-title" value="${esc(fileTitle(f))}" spellcheck="false">
      <label>作者（多位用“；”分隔）</label><input class="field" id="f-authors" value="${esc((f.authors || []).join("；"))}" spellcheck="false">
      <label>语言</label><select class="field" id="f-lang">${langs.map((l) => `<option ${l === f.language ? "selected" : ""}>${esc(l)}</option>`).join("")}</select>
      <label>角色</label><select class="field" id="f-role">${roleOpts}</select>
      <div class="entry-path">磁盘文件：${esc(f.name)}</div>`,
    ok: "保存",
    onOk: async (root) => {
      const title = $("#f-title", root).value.trim();
      if (!title) { $("#f-title", root).focus(); return false; }
      const r = await call("update_file", {
        fileId: id,
        title,
        authors: splitAuthors($("#f-authors", root).value),
        language: $("#f-lang", root).value,
        role: $("#f-role", root).value,
      });
      return r === FAIL ? false : true;
    },
  });
  if (res) { await reload(); toast("已保存此项"); }
}

async function editWork(id) {
  const w = S.byId.get(id);
  if (!w) return;
  const res = await modal({
    title: "编辑条目",
    body: `<label>英文名</label><input class="field" id="e-en" value="${esc(workTitleEn(w))}" spellcheck="false">
      <label>中文名</label><input class="field" id="e-zh" value="${esc(workTitleZh(w))}" spellcheck="false">
      <label>作者（多位用“；”分隔）</label><input class="field" id="e-authors" value="${esc(w.authors.join("；"))}" spellcheck="false">
      <label>类别</label><select class="field" id="e-cat">${categoryOptions(S.catById.has(w.categoryId) ? w.categoryId : null)}</select>
      <label>备注</label><textarea class="field" id="e-notes" rows="4">${esc(w.notes)}</textarea>`,
    ok: "保存",
    onOk: async (root) => {
      const titleEn = $("#e-en", root).value.trim();
      const titleZh = $("#e-zh", root).value.trim();
      if (!titleEn && !titleZh) { $("#e-zh", root).focus(); return false; }
      const c = $("#e-cat", root).value;
      const r = await call("update_work", { id, titleEn, titleZh, authors: splitAuthors($("#e-authors", root).value), notes: $("#e-notes", root).value, categoryId: c === "" ? null : +c });
      return r === FAIL ? false : true;
    },
  });
  if (res) { await reload(); toast("已保存"); }
}

// ---------------- library / theme ----------------
function applyTheme() {
  const dark = S.theme === "dark" || (S.theme === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

function applyState(st) {
  S.st = st;
  S.scanning = st.scanning;
  loadCollapsed();
}

async function chooseRoot() {
  const st = await call("choose_root");
  if (st === FAIL) return;
  applyState(st);
  S.view = { kind: "all" };
  closeDetail();
  closeInspect();
  await reload();
}

async function switchLibrary(name) {
  closeDetail();
  closeInspect();
  const st = await call("open_library", { name });
  if (st === FAIL) { renderTop(); return; }
  applyState(st);
  S.view = { kind: "all" };
  S.selected.clear();
  S.coverVer.clear();
  await reload();
  $("#grid").scrollTop = 0;
}

function setScanning(on, p) {
  S.scanning = on;
  const bar = $("#scan-bar");
  bar.classList.toggle("on", on);
  $("#btn-scan").classList.toggle("spinning", on);
  if (on && p) {
    bar.classList.toggle("indeterminate", !p.total);
    $("div", bar).style.width = p.total ? `${(100 * p.done) / p.total}%` : "";
    $("#progress").textContent = p.text + (p.total ? `（${p.done}/${p.total}）` : "");
  }
  if (!on) renderStatus();
}

// ---------------- boot ----------------
function bindTop() {
  $("#lib-select").addEventListener("change", (e) => switchLibrary(e.target.value));
  let t = 0;
  $("#search").addEventListener("input", (e) => {
    clearTimeout(t);
    t = setTimeout(() => { S.search = e.target.value.trim(); S.selected.clear(); renderGrid(); }, 120);
  });
  $("#filter-chips").addEventListener("click", (e) => {
    if (e.target.closest('[data-chip="search"]')) { $("#search").value = ""; S.search = ""; renderGrid(); }
  });
  $("#sort").addEventListener("change", (e) => { S.sort = e.target.value; renderGrid(); saveSettings(); });
  $("#tile-size").addEventListener("input", (e) => {
    S.tile = +e.target.value;
    document.documentElement.style.setProperty("--tile", `${S.tile}px`);
    saveSettings();
  });
  $("#btn-new").addEventListener("click", newWork);
  $("#btn-inspect").addEventListener("click", () => setInspectOpen(!S.inspectOpen));
  $("#btn-scan").addEventListener("click", rescan);
  bindResizers();
  $("#btn-menu").addEventListener("click", (e) => {
    const r = e.currentTarget.getBoundingClientRect();
    mainMenu(r.right - 210, r.bottom + 6);
  });
  document.addEventListener("mousedown", (e) => { if (!e.target.closest("#menu")) hideMenu(); });
  window.addEventListener("blur", hideMenu);
  document.addEventListener("contextmenu", (e) => { if (!e.target.closest("input,textarea")) e.preventDefault(); });
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", applyTheme);

  document.addEventListener("keydown", (e) => {
    const typing = e.target.closest?.("input,textarea,select");
    if (e.key === "Escape") {
      if (!$("#menu").classList.contains("hidden")) hideMenu();
      else if (S.detailId != null) closeDetail();
      else if (S.inspectOpen) setInspectOpen(false);
      else if (typing) e.target.blur();
      else { S.selected.clear(); refreshSelection(); }
    } else if (e.ctrlKey && e.key.toLowerCase() === "f") {
      e.preventDefault();
      const box = S.detailId != null ? $("#file-search") : $("#search");
      box?.focus();
      box?.select();
    } else if (e.key === "F5") {
      e.preventDefault();
      rescan();
    } else if (typing || S.detailId != null || $("#modal-root").children.length) {
      return;
    } else if (e.ctrlKey && e.key.toLowerCase() === "a") {
      e.preventDefault();
      S.selected = new Set(S.visible.map((w) => w.id));
      refreshSelection();
    } else if (e.key === "Delete" && S.selected.size && S.view.kind !== "removed") {
      removeWorks([...S.selected]);
    } else if ((e.key === "Enter" || e.key === " ") && S.selected.size === 1) {
      e.preventDefault();
      openInspect([...S.selected][0]);
    }
  });
}

async function bindEvents() {
  await listen("scan-progress", (e) => setScanning(true, e.payload));
  await listen("scan-done", async (e) => {
    setScanning(false);
    const r = e.payload || {};
    await reload();
    const parts = [];
    if (r.added) parts.push(`新增 ${r.added}`);
    if (r.removed) parts.push(`消失 ${r.removed}`);
    if (r.changed) parts.push(`变更 ${r.changed}`);
    if (r.moved) parts.push(`移动/改名 ${r.moved}`);
    toast(parts.length ? `扫描完成：${parts.join("，")}` : "扫描完成，没有变化");
  });
  await listen("scan-error", (e) => {
    setScanning(false);
    if (e.payload !== "已取消") toast(`扫描失败：${e.payload}`, "error");
  });
  await listen("cover-pending", (e) => {
    S.coversPending = e.payload;
    renderStatus();
  });
  await listen("cover-ready", (e) => {
    const fid = e.payload.fileId;
    S.coverVer.set(fid, (S.coverVer.get(fid) || 0) + 1);
    for (const w of S.works) {
      const f = w.files.find((x) => x.id === fid);
      if (f) f.hasCover = true;
      if (coverId(w) === fid) {
        w.hasCover = true;
        const box = $(`#grid .tile[data-id="${w.id}"] .cover-box`);
        if (box) {
          const old = box.firstElementChild;
          const tmp = document.createElement("div");
          tmp.innerHTML = coverHtml(w);
          old.replaceWith(tmp.firstElementChild);
        }
      }
    }
    if (S.detailId != null) {
      const dw = S.byId.get(S.detailId);
      if (dw && coverId(dw) === fid) {
        const box = $(".entry-cover");
        if (box) box.innerHTML = coverHtml(dw);
      }
      const tile = $(`.file-tile[data-fid="${fid}"] .cover-box`);
      const f = dw?.files.find((x) => x.id === fid);
      if (tile && f) {
        const tmp = document.createElement("div");
        tmp.innerHTML = fileCoverHtml(f);
        tile.firstElementChild.replaceWith(tmp.firstElementChild);
      }
    }
    if (S.inspectId != null) {
      const iw = S.byId.get(S.inspectId);
      if (iw && coverId(iw) === fid) {
        const box = $(".inspect-cover");
        if (box) box.innerHTML = coverHtml(iw);
      }
      const tile = $(`#inspect .file-tile[data-fid="${fid}"] .cover-box`);
      const f = iw?.files.find((x) => x.id === fid);
      if (tile && f) {
        const tmp = document.createElement("div");
        tmp.innerHTML = fileCoverHtml(f);
        tile.firstElementChild.replaceWith(tmp.firstElementChild);
      }
    }
  });
}

async function boot() {
  bindTop();
  bindGrid();
  bindSidebar();
  bindDetail();
  bindInspect();
  await bindEvents();
  const st = await call("init");
  if (st === FAIL) { renderAll(); return; }
  applyState(st);
  S.sort = ["title", "author", "added"].includes(st.settings.sort) ? st.settings.sort : "title";
  S.tile = Math.min(260, Math.max(120, st.settings.tileWidth || 160));
  S.side = Math.min(480, Math.max(180, st.settings.sideWidth || 260));
  S.inspectW = Math.min(720, Math.max(280, st.settings.inspectWidth || 400));
  S.inspectOpen = st.settings.inspectOpen !== false;
  S.theme = st.settings.theme || "system";
  applyTheme();
  $("#sort").value = S.sort;
  $("#tile-size").value = S.tile;
  document.documentElement.style.setProperty("--tile", `${S.tile}px`);
  applyPaneWidths();
  if (st.scanning) setScanning(true, { text: "正在扫描…", done: 0, total: 0 });
  await reload();
}

applyTheme();
boot();
