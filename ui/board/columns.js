// Cue UI board: the columns: folded to the rail, full screen, and their widths (dragging the grips).
// Sections collapsed to the rail ("waiting" | "sessions") and the chat full-screen ("focus").
// Collapse is per-window (like the drawers). Focus is the window being full screen (⌃⌘F, the green
// button, View → Enter Full Screen); Esc leaves it. It starts with both sections on the rail; one opened
// there (focusShown) stays open only until you leave.
let collapsed = new Set();
try { collapsed = new Set(JSON.parse(localStorage.getItem("cue.collapsed") || "[]")); } catch {}
let focus = false;
let focusShown = new Set();
const railBubble = {};   // section -> until ms (its label pops while this is in the future)
const railLast = {};     // section -> last count drawn
function collapseSection(name) {
  if (focus) focusShown.delete(name);
  else { collapsed.add(name); saveCollapsed(); }
  renderMain();
}
function leaveFocus() { focus = false; invoke("set_fullscreen", { on: false }).catch(() => {}); }
function openSection(name) {
  if (focus) focusShown.add(name);
  else { collapsed.delete(name); saveCollapsed(); }
  delete railLast[name];
  renderMain();
}
/** Follow the window: full screen in, full-screen chat; out, the board as you left it. Asked once the
 *  resizing settles (macOS animates the change), and once at the start (Cue can open full screen). */
function followFullscreen() {
  invoke("is_fullscreen").then((v) => {
    const on = !!v;
    if (on === focus) return;
    focus = on;
    if (on) { focusShown = new Set(); delete railLast.waiting; delete railLast.sessions; }
    renderMain();
  }, () => {});
}
let focusCheck = 0;
addEventListener("resize", () => { clearTimeout(focusCheck); focusCheck = setTimeout(followFullscreen, 300); });
followFullscreen();
function saveCollapsed() { try { localStorage.setItem("cue.collapsed", JSON.stringify([...collapsed])); } catch {} }

// Column widths: Waiting and Sessions as shares of the board's 3.75 (Active has the rest), per window like
// the folds. A grip in the gap before each trades width with the column on its left, within limits that
// keep the cards readable; Reset in a moved column's head (or a double-click on its grip) puts it back.
const COLS = { waiting: { fr: 1, min: 260, max: 480 }, sessions: { fr: 0.85, min: 240, max: 440 } };
const ACTIVE_MIN = 560, SNAP = 12;
let colFr = { waiting: 1, sessions: 0.85 };
try {
  const saved = JSON.parse(localStorage.getItem("cue.colWidths") || "{}");
  for (const n of Object.keys(COLS)) if (Number.isFinite(saved[n]) && saved[n] > 0.3 && saved[n] < 1.8) colFr[n] = saved[n];
} catch {}
let colSizing = null;   // while you drag: { name, edge: "" | "widest" | "narrowest" }
const colMoved = (name) => Math.abs(colFr[name] - COLS[name].fr) > 0.005;
function saveColWidths() { try { localStorage.setItem("cue.colWidths", JSON.stringify(colFr)); } catch {} }
/** The board's columns and where each grip sits, for what's folded. A folded one keeps its width for later. */
function colLayout(wColl, sColl) {
  const share = (fr) => `calc((100% - 28px) * ${fr} / 3.75)`;
  const w = colFr.waiting, s = colFr.sessions;
  if (!wColl && !sColl) return {
    tpl: `minmax(0,${3.75 - w - s}fr) minmax(0,${w}fr) minmax(0,${s}fr)`,
    grips: [{ name: "waiting", left: share(3.75 - w - s) }, { name: "sessions", left: `calc(${share(3.75 - s)} + 14px)` }],
    main: share(3.75 - w - s),
  };
  const open = !wColl ? "waiting" : !sColl ? "sessions" : null;
  return {
    tpl: ["minmax(0,1fr)", ...(open ? [share(colFr[open])] : []), "34px"].join(" "),
    grips: open ? [{ name: open, left: `calc(100% - 62px - ${share(colFr[open])})` }] : [],
    // Active: what the open column, the rail and their gaps leave.
    main: open ? `calc(100% - 62px - ${share(colFr[open])})` : "calc(100% - 48px)",
  };
}
/** The width of the board's chat pane (its Active column) beside both columns, as you dragged them, as a length of the
 *  board's width (100%): the crew's page gives its chat the same (team_page.js). Folds and full screen don't count: with
 *  both folded the chat is nearly the whole board, which would leave the crew's list no room. */
const boardChatWidth = () => colLayout(false, false).main;
const colGrip = (g) => `<div class="col-grip ${colSizing?.name === g.name ? "on" : ""}" data-col-grip="${g.name}" style="left:${g.left}"><i></i>${colSizing?.name === g.name && colSizing.edge ? `<b>${g.name[0].toUpperCase() + g.name.slice(1)} · ${colSizing.edge}</b>` : ""}</div>`;
const colReset = (name) => colMoved(name) ? `<button class="col-reset" data-col-reset="${name}">Reset</button>` : "";
/** Put a column back to its usual width. */
function resetCol(name) {
  colFr[name] = COLS[name].fr;
  saveColWidths();
  renderMain();
}
/** Drag a grip: the column after it and the one before it trade width. The grip between Waiting and
 *  Sessions trades between those two; any other gives to or takes from Active. Near the usual width it
 *  clicks onto it; at a limit it stops and says so. Live: only the board's columns change until you let go. */
function dragColGrip(e) {
  const grip = e.target.closest?.("[data-col-grip]");
  if (!grip || e.button !== 0) return;
  e.preventDefault();
  const board = grip.closest(".board"), name = grip.dataset.colGrip;
  const wColl = !board.querySelector(".col-waiting"), sColl = !board.querySelector(".col-sessions");
  const left = name === "sessions" && !wColl ? "waiting" : null;   // null: Active is on its left
  const unit = (board.clientWidth - 28) / 3.75;   // px per share
  if (unit <= 0) return;
  const x0 = e.clientX, start = { ...colFr };
  const px = (fr) => fr * unit;
  // Limits widen to take in where it starts, so a small window never jumps on the first move.
  const lim = (n) => ({ min: Math.min(COLS[n].min, px(start[n])), max: Math.max(COLS[n].max, px(start[n])) });
  // Twice in a row without a drag: back to its usual width.
  if (lastGripPress.name === name && Date.now() - lastGripPress.at < 400) {
    lastGripPress = {};
    if (left) colFr[left] = COLS[left].fr;
    return resetCol(name);
  }
  lastGripPress = { name, at: Date.now() };
  const otherStart = board.querySelector(left ? ".col-waiting" : ".col.main").getBoundingClientRect().width;
  const me = lim(name), other = left ? lim(left) : { min: Math.min(ACTIVE_MIN, otherStart), max: Infinity };
  let moved = false;
  const move = (m) => {
    if (!moved) {
      if (Math.abs(m.clientX - x0) < 3) return;
      moved = true;
      lastGripPress = {};
      colSizing = { name, edge: "" };
      document.body.classList.add("col-sizing");
    }
    let w = px(start[name]) + (x0 - m.clientX);   // its left edge moves; it grows leftward
    const usual = px(COLS[name].fr);
    if (Math.abs(w - usual) < SNAP) w = usual;
    // what the column on its left can give or take
    const hi = Math.min(me.max, otherStart + px(start[name]) - other.min), lo = Math.max(me.min, otherStart + px(start[name]) - other.max);
    const edge = w >= hi ? "widest" : w <= lo ? "narrowest" : "";
    w = Math.max(lo, Math.min(hi, w));
    colFr[name] = w / unit;
    if (left) colFr[left] = start[left] + start[name] - colFr[name];
    colSizing.edge = edge;
    const lay = colLayout(wColl, sColl), live = document.querySelector(".board");
    if (!live) return;
    live.style.gridTemplateColumns = lay.tpl;
    for (const g of lay.grips) {
      const el = live.querySelector(`[data-col-grip="${g.name}"]`);
      if (el) el.outerHTML = colGrip(g);
    }
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    if (!moved) return;
    colSizing = null;
    document.body.classList.remove("col-sizing");
    saveColWidths();
    renderMain();
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up, { once: true });
}
let lastGripPress = {};
document.addEventListener("pointerdown", dragColGrip);
