// A diff over the page: a click on a changed file (a Desk item's Files changed, a session's "Changed this turn" on
// the Desk) opens it here instead of leaving for the session's Diff tab. Files on the left, the picked file's diff on the
// right; ↑ ↓ step through the files, Esc (or a click outside, or ×) closes it and you're where you were.
// Drawn on its own element over the window, apart from renderMain, so a redraw under it doesn't touch it.

/** What's open: { title, sub, files (diffParse's), at, back (the element to give the cursor back to) }, or null. */
let dov = null;
let dovEl = null;

/** Opens the overlay on `files` (diffParse's, with their lines), file `at` picked. */
function diffOverlay(title, sub, files, at = 0) {
  if (!files?.length) return toast("No changes to show");
  dov = { title, sub, files, at: Math.min(Math.max(0, at | 0), files.length - 1), back: document.activeElement };
  dovDraw();
  dovEl.querySelector(".dov-f.on")?.focus({ preventScroll: true });
}

function diffOverlayClose() {
  if (!dov) return;
  const back = dov.back;
  dov = null;
  dovEl?.remove();
  if (back?.isConnected) back.focus({ preventScroll: true });
}

const dovSum = (files) => { const add = files.reduce((n, f) => n + (f.add || 0), 0), del = files.reduce((n, f) => n + (f.del || 0), 0); return `<span class="dv-n dv-add">+${add}</span> <span class="dv-n dv-del">−${del}</span>`; };
const dovFileNums = (f) => (f.unread ? "" : `<span class="dv-n dv-add">+${f.add}</span><span class="dv-n dv-del">−${f.del}</span>`);

function dovDraw() {
  if (!dovEl) {
    dovEl = document.createElement("div");
    dovEl.className = "dov-scrim";
    dovEl.addEventListener("click", dovClick);
  }
  const { title, sub, files, at } = dov, f = files[at];
  const list = files.map((x, i) => `<button class="dov-f ${i === at ? "on" : ""}" data-dov-at="${i}" aria-current="${i === at}"><span class="dov-p">${esc(x.path)}</span>${x.isNew ? `<span class="dov-new">new</span>` : ""}${dovFileNums(x)}</button>`).join("");
  dovEl.innerHTML = `<div class="dov" role="dialog" aria-modal="true" aria-label="${esc(title)}">
    <header class="dov-head"><b class="dov-title">${esc(title)}</b>${sub ? `<span class="dov-sub">${esc(sub)}</span>` : ""}<span class="grow"></span>
      <span class="dov-keys"><kbd>↑</kbd><kbd>↓</kbd> file · <kbd>Esc</kbd> close</span><button class="dov-x" data-dov-close aria-label="Close">×</button></header>
    <div class="dov-main">
      <nav class="dov-list" aria-label="Files"><div class="dov-k">${files.length} file${files.length === 1 ? "" : "s"} ${dovSum(files)}</div>${list}</nav>
      <section class="dov-diff"><div class="dov-fh"><span class="dov-p">${esc(f.path)}</span>${f.isNew ? `<span class="dov-new">new</span>` : ""}${dovFileNums(f)}</div>
        <div class="dov-lines dv-lines">${f.lines?.length || f.unread ? dvLines(f) : `<div class="h">No lines to show.</div>`}</div></section>
    </div></div>`;
  if (!dovEl.isConnected) document.body.append(dovEl);
}

/** Picks file `i`, keeping the cursor on the list and the picked row in view. */
function dovPick(i) {
  if (!dov || i < 0 || i >= dov.files.length || i === dov.at) return;
  dov.at = i;
  dovDraw();
  const on = dovEl.querySelector(".dov-f.on");
  on?.focus({ preventScroll: true });
  on?.scrollIntoView({ block: "nearest" });
}

function dovClick(e) {
  e.stopPropagation();
  if (e.target === dovEl || e.target.closest("[data-dov-close]")) return diffOverlayClose();
  const f = e.target.closest("[data-dov-at]");
  if (f) dovPick(+f.dataset.dovAt);
}

/** While it's open it takes every key before the page under it does (Esc there would close a Desk item, say;
 *  app/keys.js KEY_FIRST); ⌘C and the like still work, as only these keys are kept from their default. */
function diffOverlayKey(e) {
  if (!dov) return false;
  if (e.key === "Escape") { e.preventDefault(); diffOverlayClose(); }
  else if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); dovPick(dov.at + (e.key === "ArrowDown" ? 1 : -1)); }
  return true;
}

/** A short list of changed files, as the Desk's panel shows them: each path with its +/−, the first
 *  `max`, then "and N more files ›". `files`: diffParse's, or { path } alone while their lines are on the way.
 *  `attrs(n)`: the click attributes that open file n's diff, "" for a plain row; `more`: the "more" link's ("" leaves
 *  it out). */
function fileListHtml(files, { attrs = () => "", more = "", max = 3 } = {}) {
  // "new" sits by its path, so the counts stay in one column whatever's new (#394).
  const nums = (f) => (f.add == null ? "" : `<span class="ib-nums"><span class="ib-add">+${f.add || 0}</span> <span class="ib-del">−${f.del || 0}</span></span>`);
  const cells = (f) => `<code>${esc(f.path)}</code>${f.isNew && f.add != null ? `<span class="ib-new">new</span>` : ""}${nums(f)}`;
  const row = (f, n) => { const a = attrs(n); return a ? `<button class="ib-file" ${a}>${cells(f)}</button>` : `<div class="ib-file">${cells(f)}</div>`; };
  const rest = files.length > max && more ? `<button class="ib-link" ${more}>and ${files.length - max} more files ›</button>` : "";
  return `<div class="ib-files">${files.slice(0, max).map(row).join("")}</div>${rest}`;
}
