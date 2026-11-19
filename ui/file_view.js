// Cue UI — the file viewer: a file clicked anywhere in Cue (a path in a reply, in an ask, on a Desk item…) opens in a
// pop-up over the page (a markdown file opens in Read instead, with Listen: chat/reader.js openPathOrDoc): text and code with its line numbers, markdown set for reading, a picture at full size. Its
// head has Read (a markdown file, in Read with Listen), Open in app (settings.rs how_to_open: its app, or as text) and
// Copy path; Esc, a click outside or × closes it, and you're where you were. A folder, a binary file or one too big
// opens where it did before (open_link). Drawn on its own element over the window, apart from renderMain, as the diff
// overlay is (diff_overlay.js), so a redraw under it doesn't touch it.

/** What's open: peek_file's { path, kind, text, size, modified_ms }, with `sid` (the session that named it), `img`
 *  (a picture's data: URL; null while it loads, false when it can't be read) and `back` (the element to give the
 *  cursor back to); or null. */
let fv = null;
let fvEl = null;

/** Opens file `path` (as session `sid` named it: found from its folder, ipc/docs.rs peek_file) in the viewer. */
function fileView(path, sid = null) {
  const s = sid ? sessionOf(sid) : null;
  return invoke("peek_file", { path, cwd: s?.cwd || null, sid }).then((f) => {
    if (f.kind !== "text" && f.kind !== "image") return invoke("open_link", { url: f.path }).catch((err) => toast(`Couldn't open: ${err}`));
    fv = { ...f, sid, img: null, back: fv?.back || document.activeElement };
    fvDraw();
    fvEl.querySelector(".fv-body")?.focus({ preventScroll: true });
    if (f.kind === "image") {
      const got = (d) => { if (fv?.path === f.path) { fv.img = d; fvDraw(); } };
      invoke("image_data", { path: f.path }).then(got, () => got(false));
    }
  }, (err) => toast(String(err)));
}

function fileViewClose() {
  if (!fv) return;
  const back = fv.back;
  fv = null;
  fvEl?.remove();
  if (back?.isConnected) back.focus({ preventScroll: true });
}

const fvSize = (n) => (n < 1024 ? `${n} B` : n < 1024 * 1024 ? `${Math.round(n / 1024)} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`);

/** The file itself: a markdown file read as a reply is, a picture, or text with its line numbers. */
function fvBody(f) {
  if (f.kind === "image") {
    if (f.img === false) return `<div class="fv-none">Couldn't show this picture.</div>`;
    return f.img ? `<div class="fv-img"><img src="${f.img}" alt="${esc(readName(f.path))}"></div>` : `<div class="fv-none">Loading…</div>`;
  }
  const text = f.text || "";
  if (!text.trim()) return `<div class="fv-none">The file is empty.</div>`;
  if (isDocPath(f.path)) return `<div class="fv-md msg">${md(text, { pathsOf: f.sid, doc: true })}</div>`;
  const lines = text.replace(/\n$/, "").split("\n");
  const nums = lines.map((_, i) => i + 1).join("\n");
  return `<div class="fv-code"><pre class="fv-nums" aria-hidden="true">${nums}</pre><pre class="fv-text">${fvColor(lines.join("\n"), f.path)}</pre></div>`;
}

// ---------- Code colours: comments, strings, numbers and keywords, by the file's kind ----------
// A small scanner, not a parser: one pass over the text, each piece escaped. What it gets wrong (a regex literal, a
// heredoc) shows plain or as a string, never as anything but the file's own text.
const FV_KEYWORDS = new Set(("as async await break case catch class const continue crate def default del do elif else enum except export "
  + "extends false fi finally fn for from func function go if impl import in interface is let loop match mod module mut new nil "
  + "None not null or package pass pub raise return self Self static struct super switch then this throw true True False try type "
  + "typeof undefined unsafe use var void where while with yield echo local done esac").split(" "));
/** The comment forms of a file, by its name: `#` (shell, Python, Ruby, YAML, TOML…), `--` (SQL, Lua), `<!-- -->`
 *  (HTML, XML), or `//` and `/* *\/` (the C family, JavaScript, Rust, Swift, CSS…). */
function fvComments(path) {
  const ext = (/\.([\w]+)$/.exec(path)?.[1] || "").toLowerCase(), name = readName(path);
  if (/^(sh|bash|zsh|fish|command|py|rb|pl|r|jl|ex|exs|yaml|yml|toml|ini|conf|cfg|env|gitignore|dockerignore|tf|mk|nix)$/.test(ext) || /^(Makefile|Dockerfile|Gemfile|Rakefile|\.env.*)$/.test(name)) return String.raw`#[^\n]*`;
  if (/^(sql|lua|hs)$/.test(ext)) return String.raw`--[^\n]*`;
  if (/^(html?|xml|svg|plist|vue|svelte)$/.test(ext)) return String.raw`<!--[\s\S]*?-->|//[^\n]*|/\*[\s\S]*?\*/`;
  if (/^(css|scss|less)$/.test(ext)) return String.raw`/\*[\s\S]*?\*/`;
  if (/^(json|txt|log|csv|tsv|md|markdown|lock|diff|patch)$/.test(ext)) return "";
  return String.raw`//[^\n]*|/\*[\s\S]*?\*/`;
}
/** `text` as html, its comments, strings, numbers and keywords in spans (.fv-c, .fv-s, .fv-n, .fv-k). Plain text
 *  and logs stay plain. Rust's 'a (a lifetime) isn't a string: there only a one-character '…' is. */
function fvColor(text, path) {
  const ext = (/\.([\w]+)$/.exec(path)?.[1] || "").toLowerCase();
  if (/^(txt|log|csv|tsv)$/.test(ext)) return esc(text);
  const comment = fvComments(path);
  const single = ext === "rs" ? String.raw`'(?:\\.|[^'\\\n])'` : String.raw`'(?:\\.|[^'\\\n])*'`;
  const strs = String.raw`"(?:\\.|[^"\\\n])*"|` + single + (/^(js|mjs|cjs|ts|tsx|jsx|sh|bash|zsh|go)$/.test(ext) ? String.raw`|\`(?:\\.|[^\`\\])*\`` : "");
  const re = new RegExp(`(${comment || "(?!)"})|(${strs})|(\\b(?:0x[\\da-fA-F_]+|\\d[\\d_]*(?:\\.\\d+)?(?:e[+-]?\\d+)?)\\b)|([A-Za-z_]\\w*)`, "g");
  let out = "", at = 0;
  for (const m of text.matchAll(re)) {
    const cls = m[1] ? "c" : m[2] ? "s" : m[3] ? "n" : FV_KEYWORDS.has(m[4]) ? "k" : "";
    if (!cls) continue;
    out += esc(text.slice(at, m.index)) + `<span class="fv-${cls}">${esc(m[0])}</span>`;
    at = m.index + m[0].length;
  }
  return out + esc(text.slice(at));
}

function fvDraw() {
  if (!fvEl) {
    fvEl = document.createElement("div");
    fvEl.className = "dov-scrim fv-scrim";
    fvEl.addEventListener("click", fvClick);
  }
  const f = fv, place = readPlace(f.path), name = readName(f.path);
  const where = [place.name, place.dir].filter(Boolean).join(" · ");
  const lines = f.kind === "text" && f.text ? `${f.text.replace(/\n$/, "").split("\n").length} lines · ` : "";
  const read = f.kind === "text" && isDocPath(f.path) ? `<button class="btn" data-fv-read>Read</button>` : "";
  fvEl.innerHTML = `<div class="dov fv" role="dialog" aria-modal="true" aria-label="${esc(name)}">
    <header class="dov-head"><b class="dov-title">${esc(name)}</b><span class="dov-sub">${esc(where)}</span><span class="grow"></span>
      <span class="dov-sub fv-meta">${lines}${fvSize(f.size || 0)}</span>${read}<button class="btn" data-fv-open>Open in app</button><button class="btn" data-fv-copy>Copy path</button>
      <button class="dov-x" data-fv-close aria-label="Close">×</button></header>
    <div class="fv-body" tabindex="-1">${fvBody(f)}</div></div>`;
  if (!fvEl.isConnected) document.body.append(fvEl);
}

function fvClick(e) {
  e.stopPropagation();
  const t = e.target, f = fv;
  if (!f) return;
  if (t === fvEl || t.closest("[data-fv-close]")) return fileViewClose();
  if (t.closest("[data-fv-open]")) return invoke("open_link", { url: f.path }).catch((err) => toast(`Couldn't open: ${err}`));
  if (t.closest("[data-fv-copy]")) return invoke("copy_text", { text: f.path }).then(() => toast("Copied"), () => toast("Couldn't copy"));
  if (t.closest("[data-fv-read]")) { fileViewClose(); return openDoc(f.sid, f.path); }
  // A link in a markdown file: another file opens here in its place; a web link where it opens.
  const a = t.closest("a[data-path], a[data-href]");
  if (a) { e.preventDefault(); return openPathOrDoc(a.dataset.path || a.dataset.href, a.dataset.sid || f.sid); }
}

if (typeof window !== "undefined") {
  // While it's open it takes Esc before the page under it does (Esc there would close a Desk item, say); every other
  // key (⌘C, the arrows that scroll it) works as usual.
  window.addEventListener("keydown", (e) => {
    if (!fv || e.key !== "Escape") return;
    e.stopImmediatePropagation();
    e.preventDefault();
    fileViewClose();
  }, true);
}
