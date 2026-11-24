// Cue UI board — the Board view: cards, decision cards, drawers, usage, renames, and the update card.
/** Every project with something live, waiting or parked (in History, also those with past answers): name, sessions, how many need you. */
function projectsKnown() {
  const m = new Map();
  const add = (p, need = false) => { if (!p) return; const r = m.get(p) || m.set(p, { n: 0, need: 0 }).get(p); r.n++; if (need) r.need++; };
  const asking = new Set(state.items.map((i) => i.session_id));
  for (const s of state.sessions) add(s.project, asking.has(s.session_id));
  for (const q of state.live || []) add(q.project || projOf(q.cwd));
  for (const p of state.parked || []) if (!state.sessions.some((s) => s.session_id === p.session_id)) add(p.project);
  // Projects with nothing live now, but answers in History: listed too (with no count) in History, so it can be
  // narrowed to one. Not on the Board or Sessions: picked there, they'd show nothing.
  if (view === "history") for (const h of state.history || []) if (h.project && !m.has(h.project)) m.set(h.project, { n: 0, need: 0 });
  if (projFilter && !m.has(projFilter)) m.set(projFilter, { n: 0, need: 0 });
  // Projects you made in the list, listed while they're still empty.
  for (const p of madeList()) if (!m.has(p)) m.set(p, { n: 0, need: 0, made: true });
  return [...m.entries()].sort((a, b) => b[1].need - a[1].need || b[1].n - a[1].n || a[0].localeCompare(b[0]));
}
/** Enter in the projects list's + New project… box: the project is added, empty, and stays in the list until
 *  you remove it. Sessions go in by ⋯ → Move to project, or by picking it on + New's Project line. */
function submitPjNew() {
  const name = draft("pjnew").text.trim().replace(/\s+/g, " ").slice(0, 40);
  if (!name) return;
  const taken = takenProject(name);
  if (taken) { pjNewNote = `${projLabel(taken)} is already a project: pick it above`; return renderMain(); }
  delete drafts.pjnew;
  pjNewOpen = false; pjNewNote = "";
  const pr = ((state.settings ||= {}).project ||= {});   // shown now; Cue's next update says the same
  pr.made = [...madeList(), name];
  setSetting("project.made", pr.made);
  toast(`Added ${name}`);
  renderMain();
}
function projChip() {
  const list = projectsKnown();
  const pins = projPins();
  if (!list.length && !projFilter && !pins.length) return "";
  const total = list.reduce((n, [, r]) => n + r.n, 0);
  const pin = (p) => `<button class="pj-pin ${pins.includes(p) ? "on" : ""}" data-pjpin="${esc(p)}" aria-label="${pins.includes(p) ? "Unpin" : "Pin"} ${esc(p)}" aria-pressed="${pins.includes(p)}">${PIN_ICON}</button>`;
  // A project's name: yours (its own under it, dim), or the box you're renaming it in (Enter keeps it; empty: its own again).
  const label = (p) => pjRenaming === p
    ? `<input class="pj-name-in" data-text="pjname:${esc(p)}" maxlength="40" placeholder="${esc(p)}" spellcheck="false" value="${esc(draft(`pjname:${p}`).text)}"/>`
    : `<span class="pj-name">${esc(projLabel(p))}${projLabel(p) !== p ? `<span class="pj-orig">${esc(p)}</span>` : ""}</span>`;
  const ren = (p) => `<button class="pj-ren" data-pjren="${esc(p)}" aria-label="Rename ${esc(projLabel(p))}">✎</button>`;
  // A project you made: × removes it (it reads Remove first; the second click does it).
  const del = (p) => !madeProject(p) ? "" : pjDelArm === p
    ? `<button class="pj-del armed" data-pjdel="${esc(p)}">Remove</button>`
    : `<button class="pj-del" data-pjdel="${esc(p)}" aria-label="Remove ${esc(projLabel(p))}">×</button>`;
  const row = ([p, r]) => `<div class="pj-row ${projFilter === p ? "on" : ""}" role="button" data-pj="pick" data-project="${esc(p)}">${projBtn(p, "pj")}${label(p)}<span class="pj-n">${r.n ? `${r.n}${r.need ? ` · ${r.need} need${r.need === 1 ? "s" : ""} you` : ""}` : r.made ? "empty" : "history"}</span>${ren(p)}${del(p)}${pin(p)}</div>`;
  const add = pjNewOpen
    ? `<div class="pm-new"><input class="pm-new-in pj-new-in" data-text="pjnew" maxlength="40" placeholder="Name the new project" spellcheck="false" value="${esc(draft("pjnew").text)}"/>${pjNewNote ? `<div class="pm-note">${esc(pjNewNote)}</div>` : ""}</div>`
    : `<button class="pj-row pj-add" data-pj-new="1">+ New project…</button>`;
  const pop = pjOpen ? `<div class="pj-pop"><div class="pj-row all ${projFilter ? "" : "on"}" role="button" data-pj="pick" data-project="">${projBtn(ALL_PROJ, "pj")}<span class="pj-name">All projects</span><span class="pj-n">${total}</span></div><div class="pj-sep"></div>${list.map(row).join("")}<div class="pj-sep"></div>${add}</div>` : "";
  // A pinned project picked shows as its own chip, lit; the projects chip then reads All projects.
  const shown = projFilter && !pins.includes(projFilter) ? projFilter : "";
  // Its dot changes the color right here (the project shown, or all of them); its name opens the list.
  const main = `<span class="pj-wrap"><span class="hchip pj-chip">${projBtn(shown || ALL_PROJ, "chip")}<button class="pj-open" data-pj="toggle" aria-label="Projects, ${shown ? esc(projLabel(shown)) : "All projects"}">${shown ? esc(projLabel(shown)) : `<span class="hlbl">All projects</span>`}<span class="pj-car">▾</span></button></span>${pop}</span>`;
  // Pinned: one click shows that project alone; again, all of them.
  // Its dot changes its color, as the projects chip's does.
  // Replies that just came back there, in a conversation you're having (In conversation's rule), while another project
  // is shown: what's been waiting a while you know about; with All projects, or on that one, it's on screen already.
  const backIn = (p) => projFilter && projFilter !== p ? state.items.filter((i) => i.project === p && !parkedIds().has(i.session_id) && talking(i)).length : 0;
  const waitN = (p) => { const n = backIn(p); return n ? `<span class="pj-wait" aria-label="${n} just replied">${n}</span>` : ""; };
  const chips = pins.map((p) => `<span class="pj-wrap"><span class="hchip pj-chip pj-pinned ${projFilter === p ? "hot" : ""}">${projBtn(p, "pin")}<button class="pj-open" data-pj="pick" data-project="${projFilter === p ? "" : esc(p)}" aria-pressed="${projFilter === p}"><span class="pj-pn">${esc(projLabel(p))}</span>${waitN(p)}</button></span></span>`).join("");
  return main + chips;
}

// ---------- actions ----------
/** You answered a card: it fades right away (so you know it took), before Cue confirms. Undone if it didn't go. */
function answered(id, on = true) {
  on ? answering.set(id, Date.now()) : answering.delete(id);
  renderMain();
}
async function respond(it, decision) {
  if (typeof deskAnswered === "function") deskAnswered(it, decision);   // an item's page on the Desk says what you picked
  try { if (typeof deskPermAnswered === "function") deskPermAnswered(it); } catch (e) { console.error(e); }   // a permission card on the Desk folds shut (#793)
  answered(it.id);
  const ok = await invoke("respond", { id: it.id, decision }).catch(() => false);
  if (!ok) { answered(it.id, false); toast("Already answered somewhere else"); }
  delete drafts[it.id];
  menuFor = redirectFor = null;
}
const allow = (it) => respond(it, { behavior: "allow" });
const deny = (it, message) => respond(it, { behavior: "deny", ...(message ? { message } : {}) });
// ---------- shared pieces ----------
/** Idle sessions you hid with ×, as "session:since" — back on their own once the session does anything. */
let hiddenIdle = new Set();
try { hiddenIdle = new Set(JSON.parse(localStorage.getItem("cue.hiddenIdle") || "[]")); } catch {}
const idleKey = (s) => `${s.session_id}:${s.since_ms}`;
function hideIdle(key) {
  hiddenIdle.add(key);
  const live = new Set(state.sessions.map(idleKey));
  hiddenIdle = new Set([...hiddenIdle].filter((k) => live.has(k)));   // forget ones that moved on
  try { localStorage.setItem("cue.hiddenIdle", JSON.stringify([...hiddenIdle])); } catch {}
  renderMain();
}
const hideX = (s) => cutOff(s) ? `<button class="x-clear" data-forget="${esc(s.session_id)}" aria-label="Remove">×</button>`
  : idleNote(s) ? `<button class="x-clear" data-hide-idle="${esc(idleKey(s))}" aria-label="Hide">×</button>` : "";
/** Sessions you starred: the ones you're following. Marked where they are (a star doesn't move them),
 *  and counted by the ★ chip in the header. Kept with the session in Cue (so anything showing
 *  Cue sees the same stars); a star you just clicked shows until Cue's next update says so too. */
const starNow = new Map();   // sid -> on, clicked here, not yet in Cue's state
const isStarred = (sid) => starNow.get(sid) ?? !!(sessionOf(sid) || parkedOf(sid))?.starred_ms;
function toggleStar(sid) {
  const on = !isStarred(sid);
  starNow.set(sid, on);
  invoke("set_starred", { sessionId: sid, on }).catch((e) => { starNow.delete(sid); toast(`Couldn't: ${e}`); renderMain(); });
  renderMain();
}
/** This window used to keep its own stars (and before that, pins): hand them to Cue once, then forget them. */
function moveOldStars() {
  let old = [];
  try { old = [...JSON.parse(localStorage.getItem("cue.starred") || "[]"), ...JSON.parse(localStorage.getItem("cue.pinned") || "[]")]; localStorage.removeItem("cue.starred"); localStorage.removeItem("cue.pinned"); } catch {}
  for (const sid of new Set(old)) invoke("set_starred", { sessionId: sid, on: true }).catch(() => {});
}
const STAR_ICON = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" aria-hidden="true"><path d="M12 3.2l2.7 5.6 6.1.8-4.5 4.2 1.1 6.1L12 17l-5.4 2.9 1.1-6.1-4.5-4.2 6.1-.8z"/></svg>`;
const starBtn = (sid) => {
  const on = isStarred(sid);
  return `<button class="star-btn ${on ? "on" : ""}" data-star="${esc(sid)}" aria-label="${on ? "Unstar" : "Star"}" aria-pressed="${on}">${STAR_ICON}</button>`;
};
/** Under Sessions, two drawers: Starred, then Recently answered. At most one is open (opening one folds
 *  the other); with both folded, Sessions takes the column. Starred is open to begin with, Recently
 *  answered folded. Remembered per window. */
const DRAWERS = ["starred", "recent", "parked"];
function openDrawer() {
  try {
    const d = localStorage.getItem("cue.drawer");
    if (d !== null) return d;
    return localStorage.getItem("cue.recentOpen") === "1" ? "recent" : "starred";   // what this window had before
  } catch { return "starred"; }
}
/** Waiting's NEWEST FIRST list, folded under its heading. */
const newestShut = () => { try { return localStorage.getItem("cue.newestShut") === "1"; } catch { return false; } };
function foldNewest() {
  try { localStorage.setItem("cue.newestShut", newestShut() ? "0" : "1"); } catch {}
  renderMain();
}
/** Need to decide, under Waiting: a drawer of its own, folded unless you opened it (remembered). */
function laterOpen() {
  try { return localStorage.getItem("cue.laterOpen") === "1"; } catch { return false; }
}
const drawerOpen = (name) => name === "later" ? laterOpen() : openDrawer() === name;
/** Open or fold a drawer like a drawer: folding, its height shrinks up into its heading while Sessions
 *  grows into the room; opening, it grows back down (the other one, if open, folds at once). Snaps with
 *  Reduce motion. */
function foldDrawer(name) {
  const closing = drawerOpen(name);
  const save = () => {
    try {
      if (name === "later") return localStorage.setItem("cue.laterOpen", closing ? "0" : "1");
      localStorage.setItem("cue.drawer", closing ? "" : name); localStorage.removeItem("cue.recentOpen"); localStorage.removeItem("cue.recentClosed");
    } catch {}
  };
  const slide = (sec, col, from, to, done) => {
    let ended = false;
    const end = () => { if (!ended) { ended = true; done(); } };
    col.classList.add("drawers-shut");             // Sessions may use whatever the drawer gives up
    sec.style.flex = "none";
    sec.style.overflow = "hidden";
    sec.animate([{ height: `${from}px` }, { height: `${to}px` }], { duration: 220, easing: "ease-in-out" }).onfinish = end;
    setTimeout(end, 300);                          // a redraw mid-way drops the animation, and its onfinish
  };
  const closedHeight = (sec) => sec.querySelector(".fold-head").offsetHeight + parseFloat(getComputedStyle(sec).paddingTop);
  let sec = document.querySelector(`.sec-${name}`);
  if (!sec || matchMedia("(prefers-reduced-motion: reduce)").matches) { save(); return renderMain(); }
  if (closing) return slide(sec, sec.closest(".col.split"), sec.offsetHeight, closedHeight(sec), () => { save(); renderMain(); });
  const from = closedHeight(sec);
  save();
  renderMain();                                    // open, so we can measure where it ends up
  sec = document.querySelector(`.sec-${name}`);
  const col = sec.closest(".col.split"), to = sec.offsetHeight;
  // Mid-slide, Sessions had the room the drawer was taking: once it's open, the lists' "N more" are re-placed.
  slide(sec, col, from, to, () => { sec.style.flex = sec.style.overflow = ""; col.classList.remove("drawers-shut"); markMore(); });
}
/** Put off for later ("Later"): kept with the session in Cue, not in this window. */
const parkedIds = () => new Set(state.sessions.filter((s) => s.later_ms).map((s) => s.session_id));
const isParked = (sid) => parkedIds().has(sid);
/** This window used to keep its own list: hand it to Cue once, then forget it. */
function moveOldParked() {
  let old = [];
  try { old = JSON.parse(localStorage.getItem("cue.parked") || "[]"); localStorage.removeItem("cue.parked"); } catch {}
  for (const sid of old) invoke("set_later", { sessionId: sid, on: true }).catch(() => {});
}
function setParked(sid, on) {
  if (isParked(sid) === on) return;
  invoke("set_later", { sessionId: sid, on }).catch((e) => toast(`Couldn't: ${e}`));
  const s = sessionOf(sid);
  if (s) s.later_ms = on ? Date.now() : 0;   // shown now; Cue's next update says the same
  const name = nameOf(sid, sessionOf(sid)?.project || state.items.find((i) => i.session_id === sid)?.project || "it");
  if (on && active?.sid === sid) active = null;   // the Active pane moves on to what's still waiting
  renderMain();
  if (on) toast(`Moved ${name} to Need to decide`);
}
function groups() {
  const parked = parkedIds();
  const oldest = (a, b) => a.created_ms - b.created_ms;   // the chips step through these, oldest first
  // With a project picked in the header, only its cards; the rest are off the board, not faded.
  const yours = state.items.filter((i) => i.kind === "waiting" && !parked.has(i.session_id) && inProj(i.project)).sort(oldest);
  const decide = state.items.filter((i) => i.kind !== "waiting" && inProj(i.project)).sort(oldest);
  const asking = new Set(state.items.map((i) => i.session_id));
  // Sessions = every live session without a card in Waiting: busy ones, and idle ones (driven by
  // another agent, or cleared from Waiting), so nothing you cleared drops out of sight.
  const working = state.sessions.filter((s) => inProj(s.project) && !asking.has(s.session_id) && !parked.has(s.session_id) && !(idleNote(s) && hiddenIdle.has(idleKey(s))))
    .sort((a, b) => !!idleNote(a) - !!idleNote(b));   // working first, then idle (minus the ones you hid)
  // Need to decide: parked sessions, with their finished turn if one is pending. Something it asks you
  // still goes in Waiting (the agent is blocked on it), so it shows in both.
  // (A parked one whose record is gone: its card's project says where it belongs.)
  const later = [...parked].filter((sid) => inProj(sessionOf(sid)?.project ?? state.items.find((i) => i.session_id === sid)?.project)).map((sid) => ({ sid, s: sessionOf(sid), it: state.items.find((i) => i.session_id === sid && i.kind === "waiting") }))
    .filter((x) => x.s || x.it).sort((a, b) => (a.it?.created_ms ?? a.s.since_ms) - (b.it?.created_ms ?? b.s.since_ms));
  return { yours, decide, working, later };
}
// A finished turn's box is the session's box: one draft per session, whichever card shows it.
/** What the Active pane shows, in the drop-down lists (where lime is the keyboard highlight). */
const VIEWING = `<span class="viewing-tag">viewing</span>`;
const field = (it, placeholder, label, extra = "", quick = true) => box(it.kind === "waiting" ? `s:${it.session_id}` : it.id, placeholder, label, `data-act="send" data-id="${esc(it.id)}"`, it.kind === "waiting", "", extra, quick);

function requestBody(it) {
  const i = it.tool_input || {};
  const t = tool(it);
  if (it.kind === "question") return questionHtml(it);
  if (it.kind === "input") return formBody(it);
  if (t === "bash") return cmdCut(it.id, i.command);
  if (t === "edit" || t === "multiedit") {
    const edits = t === "multiedit" ? i.edits || [] : [{ old_string: i.old_string, new_string: i.new_string }];
    return `<div class="dim" style="font-size:var(--type-small)">${esc(shortPath(i.file_path || ""))}</div>` + edits.map((e) => `<div class="diff"><pre class="del">− ${esc(e.old_string)}</pre><pre>+ ${esc(e.new_string)}</pre></div>`).join("");
  }
  if (t === "write") return `<div class="dim" style="font-size:var(--type-small)">${esc(shortPath(i.file_path || i.path || ""))}</div>${reqCut(it.id, `<pre class="slab">${esc(i.content)}</pre>`, i.content)}`;
  // Codex's file change: the files, and the change to each as Codex wrote it.
  if (t === "apply_patch") return `<div class="dim" style="font-size:var(--type-small)">${esc((i.files || [i.file_path]).map(shortPath).join(", "))}</div>${reqCut(it.id, `<pre class="slab">${esc(i.patch || "")}</pre>`, i.patch)}`;
  // A plan to approve: the plan as written, not its JSON.
  if (t === "exitplanmode" && typeof i.plan === "string") return `<div class="plan-body">${md(i.plan)}</div>`;
  const json = JSON.stringify(i, null, 2);
  return reqCut(it.id, `<pre class="slab">${esc(json)}</pre>`, json);
}
function whyLine(it) {
  const c = (it.context || []).filter((x) => x.role !== "user").at(-1) || it.context?.at(-1);
  return c ? `<div class="why"${c.text.trim().includes("\n") ? "" : " data-cut"}>Why: ${esc(firstLine(c.text))}</div>` : "";
}
/** A dialog Cue read off the session's screen (a terminal card): what it says, and its choices as buttons. */
function dialogHtml(it) {
  const d = it.tool_input?.dialog;
  if (!d?.options?.length) return "";
  return `${d.detail ? `<div class="qtext">${linkify(esc(d.detail), sessBase(it.session_id))}</div>` : ""}<div class="opts">${d.options.map((o, n) => `<button class="opt ${n === d.selected ? "on" : ""}" data-tpick="${n + 1}" data-id="${esc(it.id)}">${n + 1} · ${esc(o)}</button>`).join("")}</div>`;
}
function decisionButtons(it, denyLabel = "Deny") {
  if (inTerminal(it) && inTmux(it.session_id) && termSid === it.session_id) return `<div class="row-btns"><span class="dim">Answer it in the terminal above.</span></div>`;
  if (inTerminal(it) && it.tool_input?.dialog?.options?.length) return `${dialogHtml(it)}<div class="row-btns"><span class="dim">Or answer it in its terminal.</span><span class="grow"></span>${inTmux(it.session_id) ? `<button class="btn" data-act="term-open" data-sid="${esc(it.session_id)}">Open terminal</button>` : `<button class="btn" data-act="go" data-id="${esc(it.id)}">${openInLabel(sessionOf(it.session_id))}</button>`}</div>`;
  if (inTerminal(it) && inTmux(it.session_id)) return `<div class="row-btns"><span class="dim">It's waiting in its terminal: answer it there.</span><span class="grow"></span><button class="btn primary" data-act="term-open" data-sid="${esc(it.session_id)}">Answer in terminal</button></div>`;
  if (inTerminal(it)) return `<div class="row-btns"><span class="dim">Cue can't answer this one: answer it in its terminal.</span><span class="grow"></span><button class="btn primary" data-act="go" data-id="${esc(it.id)}">${openInLabel(sessionOf(it.session_id))}</button></div>`;
  if (redirectFor === it.id) return `<div class="row-btns">${field(it, it.kind === "question" ? (questions(it).length > 1 ? `Your own answer to question ${stepOf(it) + 1}…` : "Your own answer…") : "Tell it what to do instead…", it.kind === "question" ? (questions(it).length > 1 ? "Use answer" : "Send") : "Redirect")}</div>`;
  if (it.kind === "input") return formButtons(it);
  if (it.kind === "question") return `<div class="row-btns"><button class="btn deny" data-act="deny" data-id="${esc(it.id)}">Decline</button><button class="btn" data-act="redirect" data-id="${esc(it.id)}">Type answer…</button><span class="grow"></span>${qSendBtn(it)}</div>`;
  // A permission's buttons sit in its title line (askHeadBtns), not under it.
  if (askBtnsInHead(it)) return "";
  return `<div class="row-btns">${permBtns(it, denyLabel, "flex")}</div>`;
}
/** A permission Cue can answer here, with no "what to do instead" box open: its buttons go in its title line (#800). */
const askBtnsInHead = (it) => it.kind === "permission" && !inTerminal(it) && redirectFor !== it.id;
/** A card's title line, its permission buttons at the right end (#800), where an item's buttons go: you answer without
 *  scrolling past the command. `title`: the line's HTML; `cls`: its class. A long title wraps; the buttons stay. */
const askHead = (it, cls, title, denyLabel = "Deny") => { const b = askHeadBtns(it, denyLabel); return b ? `<div class="${cls} ask-hrow"><div class="ask-ht">${title}</div>${b}</div>` : `<div class="${cls}">${title}</div>`; };
/** A permission's buttons for its title line: No (Deny), Always…, ↪ (what to do instead), Allow; "" for anything else. */
function askHeadBtns(it, denyLabel = "Deny") {
  return askBtnsInHead(it) ? `<span class="row-btns ask-hbtns">${permBtns(it, denyLabel, "small")}</span>` : "";
}
/** Deny, Always…, ↪, Allow; `size`: "small" in a title line, "flex" in a row of their own. */
function permBtns(it, denyLabel, size) {
  const sm = size === "small" ? " small" : "", fl = size === "flex" ? " flex" : "";
  const sugg = it.suggestions || [];
  const menu = menuFor === it.id && sugg.length ? `<div class="menu">${sugg.map((s, n) => { const x = describeSuggestion(s); return `<button data-always="${n}" data-id="${esc(it.id)}"><div>${esc(x.title)}</div><div class="sub">${esc(x.where)}</div></button>`; }).join("")}</div>` : "";
  const plan = tool(it) === "exitplanmode";
  return `<button class="btn${sm} deny${fl}" data-act="deny" data-id="${esc(it.id)}">${plan ? "Keep planning" : esc(denyLabel)}</button>${sugg.length ? `<span class="always">${menu}<button class="btn${sm}" data-act="menu" data-id="${esc(it.id)}">Always…</button></span>` : ""}<button class="btn${sm}" data-act="redirect" data-id="${esc(it.id)}">↪</button><button class="btn${sm} primary${fl}" data-act="allow" data-id="${esc(it.id)}">${plan ? "Approve plan" : "Allow"}</button>`;
}

/** What the agent wrote after a Stop hook made it continue — collapsed under the real answer. */
function followHtml(it, always = false) {
  if (!it.followup) return "";
  const open = always || openFollow.has(it.id);
  return `<div class="follow"><button class="thread-toggle" data-follow="${esc(it.id)}">After a stop hook ${open ? "▴" : "▾"}</button>${open ? `<div class="follow-body">${md(it.followup)}</div>` : ""}</div>`;
}

// ---------- recently answered (on the page, not hidden in History) ----------
/** Outcome text as shown (older saved entries used em dashes). */
const outcomeText = (i) => String(i.outcome || i.status || "").replace(/\s+—\s+/g, ": ");
const outcomeClass = (i) => (/^(allowed|answered|replied)/.test(i.outcome) ? "ok" : /^denied/.test(i.outcome) ? "no" : "");
function recentEntry(i) {
  const fresh = now() - (i.resolved_ms || 0) < 8000 ? "fresh" : "";
  // Its session is at work again (on what you answered, or since): its dot pulses, as working ones do.
  const live = sessionOf(i.session_id)?.state === "working" ? "live" : "";
  // Lit when the Active pane shows this answer, however you got there (its row here, Sessions, Next…).
  return `<div class="tl ${outcomeClass(i)} ${fresh} ${live} ${!quietOpen && active?.id === i.id ? "on" : ""}" style="${projStyle(i.project)}" data-detail="${esc(i.id)}">
    <div class="tl-top">${nameSpan(i.session_id, i.project, "tl-name")}${oddAgent(i.harness)}${machTag(i.machine)}<span style="margin-left:auto">${ago(i.resolved_ms || i.created_ms)}</span></div>
    <div class="tl-title ${isBash(i) ? "mono" : ""}">${esc(plain(summary(i)))}</div><div class="tl-out">${esc(outcomeText(i))}</div>${i.images?.length ? thumbs(i.images) : ""}</div>`;
}

/** A starred session in its drawer, drawn like Recently answered: who, what it's on now, and its state
 *  in words. The dot: pulsing while it works, red when it asks you, Moss on your turn, amber when stuck. */
function starEntry(s, open) {
  const sid = s.session_id, it = state.items.find((i) => i.session_id === sid);
  const stuck = s.state === "working" && (s.stuck_ms || preStart(s)) && !s.compacting_ms;
  const dot = stuck ? "stuck" : it ? (it.kind === "waiting" ? "ok" : "no") : s.state === "working" || s.compacting_ms ? "live" : "";
  const what = it ? plain(summary(it)) : s.compacting_ms ? "Compacting…" : s.state === "working" ? s.doing || (s.prompt ? `› ${s.prompt}` : "Thinking") : s.prompt ? `› ${s.prompt}` : plain(firstLine(lastSaid(s)));
  const st = preStart(s) ? preStart(s) : stuck ? `no new output for ${ago(s.stuck_ms)}` : it ? (it.kind === "waiting" ? (it.interrupted ? "interrupted" : "your turn") : `asks you · ${verb(it)}`) : s.state === "working" ? "working" : idleNote(s) || "idle";
  return `<div class="tl star-tl ${dot} ${open ? "on" : ""}" style="${projStyle(s.project)}" data-session="${esc(sid)}">
    <div class="tl-top">${nameSpan(sid, s.project, "tl-name")}${oddAgent(s.harness)}${machTag(s.machine)}<span style="margin-left:auto">${ago(it?.created_ms ?? s.since_ms)}</span>${starBtn(sid)}</div>
    <div class="tl-title">${esc(what)}</div><div class="tl-out">${esc(st)}</div></div>`;
}

// ---------- Board view (no column headings: the toolbar counts, each list's first group label leads it): OPEN | WAITING | WORKING + RECENTLY ANSWERED ----------
// In conversation: a card that came back within a few minutes of your last message to that session.
// It leaves the group (back to its place, newest first) once it's sat that long unanswered, except
// while it's open in Active, so it never moves under you.
/** How soon after your message this card came back (ms), or null: not a conversation. */
function talkGap(it) {
  if (it.back_ms) return null;   // you sent it to the back
  const you = (sessionOf(it.session_id)?.thread || []).filter((e) => e.role === "you" && e.at_ms <= it.created_ms).pop();
  const gap = you ? it.created_ms - you.at_ms : null;
  return gap !== null && gap <= TALK_MS ? gap : null;
}
// Once out, it stays out: opening it doesn't pull it back to the top (only a new reply, a new card, does).
const leftTalk = new Set();
const talkStale = (it) => { if (leftTalk.has(it.id)) return true; const out = now() - it.created_ms > TALK_MS && active?.sid !== it.session_id; if (out) leftTalk.add(it.id); return out; };
const talking = (it) => talkGap(it) !== null && !talkStale(it);
/** Everything waiting on you, finished turns and decisions together: conversations first (in the order
 *  they came back), then newest first. */
// Below In conversation, newest first: a stack. The oldest isn't forgotten: the Waiting chip names it and warms
// as it waits. A card you moved to the bottom (↓) stays below the rest, in the order you moved them.
const newestFirst = (a, b) => (!!a.back_ms - !!b.back_ms) || (a.back_ms ? a.back_ms - b.back_ms : b.created_ms - a.created_ms);
const needsYou = () => { const all = [...groups().decide, ...groups().yours]; return [...all.filter(talking).sort((a, b) => a.created_ms - b.created_ms), ...all.filter((i) => !talking(i)).sort(newestFirst)]; };
/** "3 agents": a session's agents, any kind (agents.js agentPill), beside its state, on any of its cards; an agent
 *  that needs you names itself instead. A click opens the session on its Agents tab (app/clicks_chat.js). */
const bgPill = (s) => agentPill(s?.session_id);
/** A Waiting card's top line: who it is (dot, mark, name, machine, state, agents) and, at its right end, its time and
 *  buttons. When both don't fit (a narrow Waiting column) the time and buttons take a line of their own, right-aligned
 *  under it, before the name would shorten to a few letters or a button be cut off. */
const cardTop = (who, side) => `<div class="nrow-top"><span class="nrow-who">${who}</span><span class="nrow-side">${side}</span></div>`;
const talkWorkRow = (s, open) => `<div class="nrow talk-work ${open ? "on" : ""}" data-session="${esc(s.session_id)}">${cardTop(`${projDot(s.project, true)}${badge(s.harness)}${nameSpan(s.session_id, s.project)}${machTag(s.machine)}<span class="dim">working</span>${bgPill(s)}`, `<span class="age">${ago(s.since_ms)}</span>`)}<div class="nrow-text">${esc(s.compacting_ms ? "Compacting…" : s.doing || "Thinking")}</div>${s.prompt ? `<div class="nrow-text talk-said mg-plain">› ${cueMsgPlainHtml(s.prompt, nameOf(s.session_id, s.project)) ?? esc(s.prompt)}</div>` : ""}</div>`;
/** The grey line under a card's name: why it's in conversation (a finished turn answered you, anything else asks
 *  you), or that it just left it. */
function talkNote(it) {
  const gap = talkGap(it);
  if (gap === null) return "";
  if (!talkStale(it)) return `<div class="talk-why">${it.kind === "waiting" ? "answered" : "asked"} ${ago(now() - gap)} after you</div>`;
  return now() - it.created_ms < 2 * TALK_MS ? `<div class="talk-why">no reply for ${TALK_MS / 60000}m · back to its place</div>` : "";
}
const isPending = (it) => !!it && state.items.some((i) => i.id === it.id);
function shelfDrawer(isOpen) {
  // With a project picked, its own only (the counts too), as the rest of the Board.
  const open = drawerOpen("parked"), tab = pkTab(), ps = (state.parked || []).filter((p) => inProj(p.project)), closedHere = closedAll().filter((c) => inProj(c.project));
  // Closed is asked for once (for its count), and again when you pick it, or after a minute while it shows.
  if (!closedLoading && (!closedList || (open && tab === "closed" && now() - closedAt > 60000))) queueMicrotask(loadClosed);
  const word = (t, label, n) => `<button class="shelf-word ${open && tab === t ? "on" : ""}" data-shelf="${t}">${label}${n ? ` <span>${n}</span>` : ""}</button>`;
  let body = "";
  if (open) {
    const rows = tab === "parked" ? ps.map((p) => ({ ...p, kind: "parked", at: p.parked_ms })) : (closedHere || []).map((c) => ({ ...c, kind: "closed", at: c.ended_ms }));
    body = rows.length ? rows.map((r) => parkedRow(r, isOpen(r.session_id))).join("")
      : `<div class="quiet-line">${tab === "parked" ? "Nothing parked. Park (in a session's ⋯ menu) stops it and keeps it here." : closedList || state.ended?.length ? "Nothing closed in the last 30 days." : "Looking…"}</div>`;
  }
  // Every session, as tiles (sessions.js), the way All history sits under Recently answered.
  if (open) body += `<button class="shelf-all" data-lv="all">All sessions →</button>`;
  return `<div class="sec sec-drawer sec-parked ${open ? "" : "shut"}"><div class="col-head fold-head shelf-head"><button class="fold-arrow" data-fold="parked" aria-expanded="${open}" aria-label="${open ? "Fold" : "Open"}">${open ? "▾" : "▸"}</button>${word("parked", "PARKED", ps.length)}<span class="shelf-sep">·</span>${word("closed", "CLOSED", closedHere?.length)}</div>${open ? `<div class="sec-body">${body}</div>` : ""}</div>`;
}
/** PARKED or CLOSED in the heading: opens the drawer on that list; the one already showing folds it. */
function pickShelf(t) {
  const open = drawerOpen("parked"), same = pkTab() === t;
  try { localStorage.setItem("cue.pkTab", t); } catch {}
  if (t === "closed") closedAt = 0;   // asked again: what ended since
  if (!open || same) return foldDrawer("parked");
  renderMain();
}
function parkedRow(p, on) {
  const sid = p.session_id, w = waking.has(sid), name = p.name || baseName(p.cwd) || p.project || "session";
  return `<div class="prow ${p.kind} ${on ? "on" : ""} ${w ? "waking" : ""}" data-session="${esc(sid)}">
    ${p.kind === "parked" ? P_SIGN : ""}<b>${esc(name)}</b>${oddAgent(p.harness)}${machLead(p.machine)}<span class="prow-age">${w ? "resuming…" : now() - p.at < 60000 ? "now" : ago(p.at)}</span>
    <button class="prow-go" data-wake="${esc(sid)}" ${w ? "disabled" : ""} aria-label="Resume ${esc(name)}">↺</button></div>`;
}
/** A starred session that's parked, in Starred: the parking sign where the others have a dot, the row
 *  faded as if asleep; top right, how long it's been parked; Resume right after its last words. */
function starParkedEntry(p, open) {
  const sid = p.session_id, w = waking.has(sid);
  const last = [...(p.thread || [])].reverse().find(askLnSaid);
  return `<div class="tl star-tl parked ${w ? "waking" : ""} ${open ? "on" : ""}" style="${projStyle(p.project)}" data-session="${esc(sid)}">${P_SIGN}
    <div class="tl-top"><span class="tl-name">${esc(p.name || p.project)}</span>${oddAgent(p.harness)}${machTag(p.machine)}<span class="tl-parked">${w ? "resuming…" : `parked ${parkedAgo(p)}`}</span>${starBtn(sid)}</div>
    <div class="tl-title tl-said"><span>${esc(last ? plain(firstLine(last.text)) : homeless(p.cwd))}</span>${w ? "" : `<button class="tl-wake" data-wake="${esc(sid)}">↺ Resume</button>`}</div></div>`;
}

/** × on a finished turn: nothing to reply, take it off Waiting. Decisions don't get one (the agent is blocked on them). */
const clearX = (it) => it.kind === "waiting" ? `<button class="x-clear" data-act="dismiss" data-id="${esc(it.id)}" title="Take it off Waiting" aria-label="Take it off Waiting">×</button>` : "";
/** "Later" on a Waiting row: move the session to Need to decide. */
const parkBtn = (sid) => `<button class="park-btn" data-park="${esc(sid)}:1">Later</button>`;
/** A row in Need to decide: the session, its finished turn if any, ↩ to put it back. Click to open it. */
function laterRow({ sid, s, it }) {
  const on = active?.sid === sid ? "on" : "";
  const what = it ? "" : s.state === "working" ? "working" : "idle";
  const text = it ? plain(summary(it)) : s.prompt ? `› ${s.prompt}` : "";
  return `<div class="nrow later ${on}" ${it ? `data-big="${esc(it.id)}"` : `data-session="${esc(sid)}"`}>
    ${cardTop(`${projDot(it?.project || s.project, s.state === "working")}${badge(it?.harness || s.harness)}${nameSpan(sid, it?.project || s.project)}${machTag(it?.machine || s.machine)}${what ? `<span class="dim">${what}</span>` : ""}${bgPill(s)}`,
      `<span class="age">${ago(it?.created_ms ?? s.since_ms)}</span>${starBtn(sid)}<button class="park-btn back" data-park="${esc(sid)}:0">↩ Waiting</button>`)}
    ${text ? `<div class="nrow-text">${esc(text)}</div>` : ""}${insCardLine(sid)}</div>`;
}
const backBtn = (it) => `<button class="back-btn" data-act="back" data-id="${esc(it.id)}" aria-label="Move to bottom">↓</button>`;
/** A card's quick answers: Allow and Deny, a terminal dialog's choices, Choose… for a question, Continue for an
 *  interrupted turn ("" for none). Waiting's rows and a Desk's card in Waiting (board/desk_cards.js) alike. */
function needQuick(it) {
  const q = it.kind === "question" ? questions(it) : [];
  let quick = "";
  if (it.kind === "permission") quick = `<div class="nrow-acts"><button class="btn deny" data-act="deny" data-id="${esc(it.id)}">Deny</button><button class="btn primary" data-act="allow" data-id="${esc(it.id)}">Allow</button></div>`;
  else if (inTerminal(it) && (it.tool_input?.dialog?.options || []).length && it.tool_input.dialog.options.length <= 4) quick = `<div class="nrow-acts wrap">${it.tool_input.dialog.options.map((o, n) => `<button class="btn" data-tpick="${n + 1}" data-id="${esc(it.id)}">${esc(o)}</button>`).join("")}</div>`;
  else if (inTerminal(it)) quick = `<div class="nrow-acts">${inTmux(it.session_id) ? `<button class="btn primary" data-act="term-card" data-id="${esc(it.id)}">Answer in terminal</button>` : `<button class="btn primary" data-act="go" data-id="${esc(it.id)}">${openInLabel(sessionOf(it.session_id))}</button>`}</div>`;
  else if (q.length) quick = `${qFoldLine(it)}<div class="nrow-acts wrap">${qFoldBtns(it, `<button class="btn" data-act="choose" data-id="${esc(it.id)}">Choose…</button>`)}</div>`;
  // An interrupted turn (Esc) waits at "What should Claude do instead?": say so, and offer Continue.
  if (it.interrupted) quick = `<div class="nrow-acts"><button class="btn primary" data-act="continue" data-id="${esc(it.id)}">Continue</button></div>`;
  return quick;
}
/** A compact row in Waiting: enough to recognise it, quick answers for the easy ones, click to open in Active. */
function needRow(it, ghost, open = false) {
  if (ghost) {
    const h = state.history.find((x) => x.id === it.id);
    return `<div class="nrow ghost ${ghost === "fresh" ? "fresh" : ""} ${ghosts.get(it.id)?.soft ? "soft" : ""}" data-detail="${esc(it.id)}"><div class="nrow-top">${badge(it.harness)}${nameSpan(it.session_id, it.project)}${machTag(it.machine)}</div>
      <div class="nrow-done ${h ? outcomeClass(h) : ""}">✓ ${esc(h ? outcomeText(h) : cleared.has(it.id) ? "cleared" : sessionOf(it.session_id)?.hosted ? "picked up" : "picked up in the terminal")}</div></div>`;
  }
  const quick = needQuick(it);
  return `<div class="nrow ${open ? "on" : ""} ${it.kind === "waiting" ? "turn" : "ask"} ${answering.has(it.id) ? "answering" : ""}" data-big="${esc(it.id)}">
    ${cardTop(`${projDot(it.project, sessionOf(it.session_id)?.state === "working")}${badge(it.harness)}${nameSpan(it.session_id, it.project)}${mateTitle(it)}${machTag(it.machine)}${it.kind !== "waiting" ? `<span class="dim">${esc(verb(it))}</span>` : it.interrupted ? `<span class="dim intr">interrupted</span>` : ""}${mateOfAsk(it) ? "" : bgPill(sessionOf(it.session_id))}`,
      `<span class="age">${ago(it.created_ms)}</span>${starBtn(it.session_id)}${it.kind === "waiting" ? backBtn(it) + parkBtn(it.session_id) : ""}${clearX(it)}`)}
    ${talkNote(it)}
    <div class="nrow-text ${isBash(it) ? "mono" : ""}">${isBash(it) ? cmdWords(plain(summary(it))) : esc(plain(summary(it)))}</div>${insCardLine(it.session_id, it.kind !== "waiting")}${quick}</div>`;
}
/** Why a session is idle (shown under Sessions → Idle); "" when it's working. */
/** Its machine couldn't be reached on Cue's last try: the card says so whatever it last showed, and × takes it out of Cue. */
const cutOff = (s) => !!s.machine && (state.unreachable || []).includes(s.machine);
const idleNote = (s) => cutOff(s) ? `can't reach ${machShort(s.machine)}` : s.state === "working" ? "" : s.state === "limited" ? limitNote(s) : s.state === "agent" ? `waiting on ${s.driven_by || "another agent"}` : s.state === "stopped" ? "stopped by you" : "cleared from Waiting";

// The right column: Sessions takes what it needs (up to half), Recently answered scrolls in the rest.
const RECENT = 10;
/** Waiting's rows. Conversations first (when there are any, under their own heading): the cards that came back,
 *  then the sessions still working on your message (5 minutes on, under Sessions only). Then newest first. A card
 *  you just answered fades out in the group it was in, unless its session is back at work there. `count` is
 *  everything under WAITING, as drawn: In conversation, Newest first, and Need to decide (a parked one asking you
 *  is in Waiting already: once). */
function waitingRows(b) {
  const { needs, later, working, isOpen } = b;
  const live = new Set(needs.map((i) => i.id));
  // A ghost fades in once: every later render (they come often while sessions work) draws it still.
  const ghostRows = [...ghosts.values()].filter((g) => !live.has(g.it.id)).map((g) => { const fresh = !g.shown; g.shown = true; return { ...g.it, _ghost: fresh ? "fresh" : "shown" }; });
  const talkWork = working.filter((s) => s.state === "working" && !idleNote(s) && workingOnYours(s) && !onDeskCard(s.session_id));
  const talkSids = new Set(talkWork.map((x) => x.session_id));
  const rows = [...needs, ...ghostRows.filter((g) => !talkSids.has(g.session_id) && !onDeskCard(g.session_id))].sort(newestFirst);
  const inTalk = (i) => talking(i);
  const draw = (list) => list.map((i) => needRow(i, i._ghost, isOpen(i.session_id))).join("");
  // A Desk's sessions have no cards here (#825): their asks are in their Desk's card, their finished turns the lead's.
  const offDesk = (i) => !onDeskCard(i.session_id);
  const talks = rows.filter((i) => inTalk(i) && offDesk(i)).sort((a, c) => a.created_ms - c.created_ms), count = (list) => list.filter((i) => !i._ghost).length;
  const rest = rows.filter((i) => !inTalk(i) && offDesk(i));
  // Each Desk with something waiting on you is one card, among the rest by when the newest of it came.
  const desks = b.desks.cards;
  // The add-on's rows that wait on you sit in Waiting with the rest.
  const more = addOn.waiting();
  const restRows = rest.length || desks.length || more.count ? `<button class="col-sub fold-head sub-fold" data-fold-newest aria-expanded="${!newestShut()}"><span class="fold-arrow">${newestShut() ? "▸" : "▾"}</span> NEWEST FIRST · ${count(rest) + desks.length + more.count}</button>${newestShut() ? "" : more.html + drawWithDesks(rest, desks, draw, isOpen)}` : "";
  const html = (talks.length || talkWork.length
    ? `<div class="col-sub">IN CONVERSATION · LAST ${TALK_MS / 60000} MIN${count(talks) + talkWork.length ? ` · ${count(talks) + talkWork.length}` : ""}</div>${draw(talks)}${talkWork.map((x) => talkWorkRow(x, isOpen(x.session_id))).join("")}`
    : "") + restRows;
  return { html, count: count(talks) + talkWork.length + count(rest) + desks.reduce((n, w) => n + w.n, 0) + more.count + later.filter((l) => !needs.some((i) => i.session_id === l.sid)).length };
}

/** Newest first's rows with the Desks' cards among them: a Desk's card goes above the first row older than the
 *  newest of what waits on it (rows you sent to the bottom stay below every card). */
function drawWithDesks(rows, desks, draw, isOpen) {
  const cards = [...desks].sort((a, c) => c.at - a.at);
  let out = "";
  for (const r of rows) {
    while (cards.length && (r.back_ms || cards[0].at >= r.created_ms)) out += deskLeadCard(cards.shift(), isOpen);
    out += draw([r]);
  }
  return out + cards.map((w) => deskLeadCard(w, isOpen)).join("");
}
/** What a Sessions card says under its name: its card's ask, a start that needs its terminal, or why it's idle. */
function sessionNote(s) {
  const i = state.items.find((x) => x.session_id === s.session_id);
  return i ? (i.kind === "waiting" ? "your turn" : "asks you") : preStart(s) ? "needs you in its terminal" : idleNote(s);
}
/** A message's words in one line (Queued: …, Not sent yet: …): Cue's in its plain line, anything else quoted, cut at
 *  `max`. */
const msgPlainWords = (text, to, max = 80) => cueMsgPlainHtml(text, to) ?? `“${esc(text.length > max ? text.slice(0, max) + "…" : text)}”`;
/** A session's card under Sessions. */
function sessionCard(s, open) {
  const go = `data-session="${esc(s.session_id)}"`;
  const doing = preStart(s) ? `Not started: ${preStart(s)}` : s.compacting_ms ? "Compacting…" : s.state === "working" && s.doing ? s.doing : "";
  const desk = deskCardLine(s.session_id);
  const words = deskSaid(s.prompt);
  const to = nameOf(s.session_id, s.project), cue = desk && /^Desk item #/i.test(words) ? null : cueMsgPlainHtml(words, to);
  const preview = desk && /^Desk item #/i.test(words) ? "" : cue ?? esc(deskAboutHead({ text: words })?.text ?? words);
  return `<div class="working click ${idleNote(s) ? "on-agent" : ""} ${open ? "on" : ""}" ${go}>
      <div class="card-head">${projDot(s.project, (s.state === "working" || s.compacting_ms) && !cutOff(s) && !preStart(s))}${nameSpan(s.session_id, s.project)}${oddAgent(s.harness)}${machTag(s.machine)}${bgPill(s)}<span class="grow"></span><span class="age" style="color:inherit">${idleNote(s) && s.state !== "limited" && !state.items.some((x) => x.session_id === s.session_id) ? "idle " : ""}${ago(s.since_ms)}</span>${starBtn(s.session_id)}${isStarred(s.session_id) && !cutOff(s) ? "" : hideX(s)}</div>
      ${addOn.cardLine(s.session_id)}${desk}${sessionNote(s) ? `<div class="agent-note">${esc(sessionNote(s))}</div>` : ""}
      ${s.queued || s.held ? `<div class="queued-note mg-plain on-bg">${s.held ? "Not sent yet" : "Queued"}: ${msgPlainWords(String((s.held || s.queued).text ?? ""), to)}</div>` : ""}
      ${bar(s.session_id)}${doing ? `<div class="doing">${esc(doing)}</div>` : ""}${preview ? `<div class="prompt mg-plain on-bg">› ${preview}</div>` : ""}${deskSessCache(s.session_id)}${handOutHtml(s)}</div>`;
}
/** Sessions' cards: working, then out of usage, then idle, each under its heading. */
function workColumn(all, isOpen, desks = []) {
  const card = (s) => sessionCard(s, isOpen(s.session_id));
  // A Desk's sessions have no cards here (#825): a line per Desk at the foot stands for them, while nothing on it
  // waits on you (then it's a card in Waiting). Out of usage stays up top: that one needs you.
  const shown = all.filter((s) => s.state === "limited" || !onDeskCard(s.session_id));
  const busy = shown.filter((s) => !idleNote(s)), idle = shown.filter((s) => idleNote(s) && s.state !== "limited"), outs = shown.filter((s) => s.state === "limited");
  return (busy.length ? `<div class="col-sub">WORKING · ${busy.length}</div>${busy.map(card).join("")}` : "")
    + (outs.length ? `<div class="col-sub lim">${outs.every((s) => lifted(s.limit)) ? "READY TO RESEND" : "OUT OF USAGE"} · ${outs.length}</div>${outs.map(card).join("")}` : "")
    + (idle.length ? `<div class="col-sub">IDLE · ${idle.length}</div>${idle.map(card).join("")}` : "")
    + (desks.length ? `<div class="col-sub dkw-sub">${deskKindIcon("tray")}DESKS · ${desks.length}</div>${desks.map(deskLine).join("")}` : "")
    || `<div class="col-sub">NO OTHER SESSIONS</div>`;
}

/** A drawer under Sessions or Waiting: its heading folds it; `body()` is drawn only while it's open. */
function drawerHtml(name, title, count, body) {
  const open = drawerOpen(name);
  return `<div class="sec sec-drawer sec-${name} ${open ? "" : "shut"}"><button class="col-head fold-head" data-fold="${name}" aria-expanded="${open}"><span class="fold-arrow">${open ? "▾" : "▸"}</span>${title} <span>${count}</span></button>${open ? `<div class="sec-body">${body()}</div>` : ""}</div>`;
}
/** A folded section's label on the rail; it bubbles a moment when its count goes up. */
function railLabel(name, count) {
  const prev = railLast[name];
  if (prev !== undefined && count > prev) railBubble[name] = Date.now() + 900;
  railLast[name] = count;
  return `<button class="pin ${railBubble[name] > Date.now() ? "bubble" : ""}" data-open-section="${name}" aria-label="Open ${name}"><span class="vt">${name.toUpperCase()} · ${count}</span></button>`;
}

/** The Waiting column: its cards, a line for what's waiting in other projects while one is picked (never out of
 *  mind: an agent blocked on you with nothing on screen was the bug), and Need to decide. */
function waitingColumn(b, wait) {
  const { needs, later } = b;
  const elsewhere = projFilter ? state.items.filter((i) => !inProj(i.project) && !parkedIds().has(i.session_id)).length : 0;
  return `<div class="col split col-waiting">
      <div class="sec sec-waiting"><div class="col-head col-tools"><span class="grow"></span>${colReset("waiting")}<button class="collapse-x" data-collapse="waiting" aria-label="Fold Waiting">»</button></div><div class="sec-body">${wait.html || `<div class="col-sub">NOTHING WAITING</div>`}${elsewhere ? `<button class="quiet-line link" data-pj="pick" data-project="">${elsewhere} waiting in other projects · show all</button>` : ""}</div></div>
      ${later.length ? drawerHtml("later", "NEED TO DECIDE", later.length, () => later.map(laterRow).join("")) : ""}
    </div>`;
}
/** Recently answered: one entry per session, its latest answer (the rest are in History). */
function recentDrawer() {
  const seen = new Set();
  const recent = state.history.filter((i) => inProj(i.project) && !seen.has(i.session_id || i.id) && seen.add(i.session_id || i.id)).slice(0, RECENT);
  const recentList = () => recent.length
    ? `<div class="card recent">${recent.map(recentEntry).join("")}
        ${state.history.length > recent.length ? `<button class="link" style="padding:4px 12px 10px" data-act="open-history">All history →</button>` : ""}</div>`
    : `<div class="empty-col">Your answers show up here.</div>`;
  return drawerHtml("recent", "RECENTLY ANSWERED", recent.length ? `${recent.length} session${recent.length === 1 ? "" : "s"}` : "", recentList);
}
/** Starred, in the order you starred them (so they don't shuffle as they work). A starred one you parked stays
 *  here too, as parked. */
function starredDrawer(isOpen) {
  const stars = [...state.sessions, ...(state.parked || [])].filter((s) => isStarred(s.session_id) && inProj(s.project)).sort((a, c) => (a.starred_ms || Infinity) - (c.starred_ms || Infinity));
  const starNeeds = stars.filter((s) => state.items.some((i) => i.session_id === s.session_id)).length;
  return drawerHtml("starred", "STARRED", stars.length ? `${stars.length}${starNeeds ? ` · ${starNeeds} need${starNeeds === 1 ? "s" : ""} you` : ""}` : "", () => stars.length ? `<div class="card recent">${stars.map((s) => (s.parked_ms ? starParkedEntry(s, isOpen(s.session_id)) : starEntry(s, isOpen(s.session_id)))).join("")}</div>` : `<div class="quiet-line">Star a session (★ on its row) to keep it here.</div>`);
}
/** The Sessions column: its cards (`workCol`; `count`: how many), then the drawers. */
function sessionsColumn(b, workCol, count) {
  return `<div class="col split col-sessions ${DRAWERS.some(drawerOpen) ? "" : "drawers-shut"}">
      <div class="sec sec-sessions"><div class="col-head col-tools"><span class="grow"></span>${colReset("sessions")}<button class="collapse-x" data-collapse="sessions" aria-label="Fold Sessions">»</button></div><div class="sec-body">${workCol}</div></div>
      ${starredDrawer(b.isOpen)}
      ${recentDrawer()}
      ${shelfDrawer(b.isOpen)}
    </div>`;
}

function boardView() {
  if (!quietOpen) current();   // settles what Active shows before the lists mark it
  const { working, later } = groups();
  const needs = needsYou();
  // What the Active pane shows stays where it is in its list, lit up (lime): opening
  // something never moves a row, so the lists don't shift under your pointer.
  const openSid = quietOpen || active?.sid || null;
  const isOpen = (sid) => !!openSid && sid === openSid;
  const b = { needs, later, working, isOpen, desks: boardDeskSplit(needs) };
  const wait = waitingRows(b);
  const shown = working.filter((s) => s.state === "limited" || !onDeskCard(s.session_id)), sessCount = shown.length + b.desks.lines.length;
  // Drawn in this order: Waiting's cards, Sessions' cards, Need to decide, then Sessions' drawers.
  const workCol = workColumn(shown, isOpen, b.desks.lines);
  const waitingCol = waitingColumn(b, wait), sessionsCol = sessionsColumn(b, workCol, sessCount);
  // Fixed layout: every column stays where it is (nothing jumps as the queue changes).
  const coll = (name) => (focus ? !focusShown.has(name) : collapsed.has(name));
  const wColl = coll("waiting"), sColl = coll("sessions");
  const railCol = (wColl || sColl) ? `<div class="col rail-col"><div class="rail">${wColl ? railLabel("waiting", wait.count) : ""}${sColl ? railLabel("sessions", sessCount) : ""}</div></div>` : "";
  // main's columns (1.9 : 1 : 0.85 of the board after its two 14px gaps, or as you dragged them); a section
  // that stays open keeps its width when the other folds, and Active takes the room.
  const lay = colLayout(wColl, sColl);
  return `<div class="board" style="grid-template-columns:${lay.tpl}">
    <div class="col main ${focus ? "" : "below-labels"}">${activePane()}</div>
    ${wColl ? "" : waitingCol}${sColl ? "" : sessionsCol}${railCol}${lay.grips.map(colGrip).join("")}
  </div>`;
}

// ---------- sheets ----------
/** The Active pane's title: the session's name with ✎ to rename it (an input while you do), and its
 *  project beside it once it has a name of its own. */
/** Under the Active header: the session's folder (~ for your home) on the right, under the buttons. One small line. */
function subHead(cwd, sid, titleOf = sid) {
  // The star sits after the folder: the title row is full.
  // Too long: cut in the middle (~/develop…/web-app), so the folder's own name always shows.
  const path = homeless(cwd), cutAt = Math.max(path.lastIndexOf("/"), 0);
  const folder = cwd ? `<span class="ap-cwd sel"><span class="cwd-head">${esc(path.slice(0, cutAt))}</span><span class="cwd-tail">${esc(path.slice(cutAt))}</span></span>${sid ? starBtn(sid) : ""}` : "";
  const meta = titleOf ? metaInline(titleOf) : "";
  return folder || meta ? `<div class="ap-sub">${meta}${folder}</div>` : "";
}
function nameHead(sid, project) {
  if (sid && renaming === sid) return `<input class="rename-in" data-text="rename:${esc(sid)}" maxlength="60" placeholder="Name this session" spellcheck="false" value="${esc(draft(`rename:${sid}`).text)}"/>`;
  const name = sid ? nameOf(sid, project) : project;
  // Just the name: its folder's full path is on the line under it (subHead).
  return `${sid ? deskSessMark(sid) : ""}<span class="proj">${esc(name)}</span>${sid ? `<button class="rename-btn" data-rename="${esc(sid)}" title="Rename this session" aria-label="Rename">✎</button>` : ""}`;
}
/** Renames shown before Cue's next update has them: sid → { name, at }. */
const renamedNow = new Map();
async function submitRename(sid, again = false) {
  const key = `rename:${sid}`, name = again ? sessionOf(sid)?.name || "" : draft(key).text.trim().replace(/\s+/g, " ").slice(0, 60);
  renaming = null;
  delete drafts[key];
  // Shown at once, everywhere: the agent hears of it after. If it doesn't take it, the chat says so.
  const s = sessionOf(sid);
  if (s) { s.name = name; delete s.rename_error; renamedNow.set(sid, { name, at: Date.now() }); }
  renderMain();
  try { toast(await invoke("rename_session", { sessionId: sid, name })); }
  catch (e) { toast(String(e)); }
}
/** The agent didn't take the name you gave it: Cue keeps it, says why, and offers it again. */
function renameNotice(s) {
  if (!s?.rename_error) return "";
  return `<div class="cv-status mg-status stuck"><span class="lim-dot"></span><span><b>${esc(s.rename_error)}.</b> Cue keeps “${esc(s.name)}”.</span><button class="btn small" data-act="rename-again" data-sid="${esc(s.session_id)}">Try again</button><button class="btn small" data-act="rename-dismiss" data-sid="${esc(s.session_id)}" aria-label="Dismiss">×</button></div>`;
}
/** Rename notices already announced (a toast once, wherever you are; the chat keeps showing it). */
const renameErrSeen = new Set();
let renameErrPrimed = false;
/** Search: every session Cue has seen, ended ones too (session names, your messages, the agents'
 *  replies). Running sessions match at once, from what the window already has, by the names you see
 *  (as ⌘K does); the best name match comes first. Then closed and older sessions by name, then what was
 *  said. A live session opens in Active; an ended one's message opens in place. */
let searchResults = [], searchSeq = 0, searchTimer = 0;
/** A newer Cue turned up in the background: a small card in the header's middle, until you choose. */
const updateCard = () => !upd.offer ? "" : `<div class="upd-card"><div><b>Cue ${esc(upd.version)} is out</b>${upd.status === "installing" ? `<div class="dim">Installing… Cue restarts by itself.</div>` : upd.status.startsWith("error:") ? `<div class="dim">${esc(upd.status.slice(6))}</div>` : ""}</div>
  <button class="btn primary small" data-upd="install" ${upd.status === "installing" ? "disabled" : ""}>Update and restart</button><button class="btn small" data-upd="later">Later</button></div>`;

