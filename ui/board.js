// Cue UI board — the Board view: cards, decision cards, header chips, drawers, usage, and the animations that move rows.
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
  return [...m.entries()].sort((a, b) => b[1].need - a[1].need || b[1].n - a[1].n || a[0].localeCompare(b[0]));
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
  const row = ([p, r]) => `<div class="pj-row ${projFilter === p ? "on" : ""}" role="button" data-pj="pick" data-project="${esc(p)}">${projBtn(p, "pj")}${label(p)}${actDots(p, "pj")}<span class="pj-n">${r.n ? `${r.n}${r.need ? ` · ${r.need} need${r.need === 1 ? "s" : ""} you` : ""}` : "history"}</span>${ren(p)}${pin(p)}</div>`;
  const pop = pjOpen ? `<div class="pj-pop"><div class="pj-row all ${projFilter ? "" : "on"}" role="button" data-pj="pick" data-project="">${projBtn(ALL_PROJ, "pj")}<span class="pj-name">All projects</span><span class="pj-n">${total}</span></div><div class="pj-sep"></div>${list.map(row).join("")}</div>` : "";
  // A pinned project picked shows as its own chip, lit; the projects chip then reads All projects.
  const shown = projFilter && !pins.includes(projFilter) ? projFilter : "";
  // Its dot changes the color right here (the project shown, or all of them); its name opens the list.
  const main = `<span class="pj-wrap"><span class="hchip pj-chip">${projBtn(shown || ALL_PROJ, "chip")}<button class="pj-open" data-pj="toggle">${shown ? esc(projLabel(shown)) : `<span class="hlbl">All projects</span>`}<span class="pj-car">▾</span></button>${actDots(shown, "chip")}</span>${pop}</span>`;
  // Pinned: one click shows that project alone; again, all of them.
  // Its dot changes its color, as the projects chip's does.
  const chips = pins.map((p) => `<span class="pj-wrap"><span class="hchip pj-chip pj-pinned ${projFilter === p ? "hot" : ""}">${projBtn(p, "pin")}<button class="pj-open" data-pj="pick" data-project="${projFilter === p ? "" : esc(p)}" aria-pressed="${projFilter === p}"><span class="pj-pn">${esc(projLabel(p))}</span></button>${actDots(p, "pin")}</span></span>`).join("");
  return main + chips;
}

// ---------- actions ----------
/** You answered a card: it fades right away (so you know it took), before Cue confirms. Undone if it didn't go. */
function answered(id, on = true) {
  on ? answering.set(id, Date.now()) : answering.delete(id);
  renderMain();
}
async function respond(it, decision) {
  answered(it.id);
  const ok = await invoke("respond", { id: it.id, decision }).catch(() => false);
  if (!ok) { answered(it.id, false); toast("Already answered somewhere else"); }
  delete drafts[it.id];
  menuFor = redirectFor = null;
}
const allow = (it) => respond(it, { behavior: "allow" });
const deny = (it, message) => respond(it, { behavior: "deny", ...(message ? { message } : {}) });
function answersFor(it) {
  const d = draft(it.id);
  const out = {};
  for (const q of questions(it)) {
    const c = d.choices[q.question];
    const picked = q.multiSelect ? [...(c || [])] : c ? [c] : [];
    if (!picked.length) return null;
    out[q.question] = picked.join(", ");
  }
  return out;
}
const qStep = {};           // item id -> which question of a multi-question ask is showing
const isAnswered = (it, q) => { const c = draft(it.id).choices[q.question]; return q.multiSelect ? !!c?.size : !!c; };
/** The question on screen: the one you picked, else the first one not answered yet. */
function stepOf(it) {
  const qs = questions(it);
  const first = qs.findIndex((q) => !isAnswered(it, q));
  return Math.min(qStep[it.id] ?? (first < 0 ? qs.length - 1 : first), qs.length - 1);
}
function pick(it, qi, oi) {
  const q = questions(it)[qi];
  const label = q?.options?.[oi]?.label;
  if (!label) return;
  const d = draft(it.id);
  if (q.multiSelect) { const s = (d.choices[q.question] ||= new Set()); s.has(label) ? s.delete(label) : s.add(label); }
  else d.choices[q.question] = label;
  // One single-choice question: the click is the answer.
  if (questions(it).length === 1 && !q.multiSelect) return respond(it, { behavior: "allow", answers: { [q.question]: label } });
  const a = answersFor(it);
  if (a && questions(it).every((x) => !x.multiSelect)) return respond(it, { behavior: "allow", answers: a });
  // Several questions: a single choice moves on to the next one still unanswered.
  if (!q.multiSelect) {
    const next = questions(it).findIndex((x, n) => n !== qi && !isAnswered(it, x));
    if (next >= 0) qStep[it.id] = next;
  }
  render();
}

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
const starredIds = () => new Set(state.sessions.map((s) => s.session_id).filter(isStarred));
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
/** Need to decide, under Waiting: a drawer of its own, folded unless you opened it (remembered). */
function laterOpen() {
  try { return localStorage.getItem("cue.laterOpen") === "1"; } catch { return false; }
}
/** Waiting's Oldest first group, folded under its heading (only while In conversation is there too). */
const oldestShut = () => { try { return localStorage.getItem("cue.oldestShut") === "1"; } catch { return false; } };
function foldOldest() {
  try { localStorage.setItem("cue.oldestShut", oldestShut() ? "0" : "1"); } catch {}
  renderMain();
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
  const oldest = (a, b) => (a.back_ms ?? a.created_ms) - (b.back_ms ?? b.created_ms);
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
const field = (it, placeholder, label) => box(it.kind === "waiting" ? `s:${it.session_id}` : it.id, placeholder, label, `data-act="send" data-id="${esc(it.id)}"`, it.kind === "waiting");

function requestBody(it) {
  const i = it.tool_input || {};
  const t = tool(it);
  if (it.kind === "question") {
    const d = draft(it.id);
    const qs = questions(it);
    const opts = (q, qi) => `<div class="opts">${(q.options || []).map((o, oi) => {
      const on = q.multiSelect ? d.choices[q.question]?.has?.(o.label) : d.choices[q.question] === o.label;
      return `<button class="opt ${on ? "on" : ""}" data-pick="${qi}:${oi}" data-id="${esc(it.id)}">${oi + 1} · ${esc(o.label)}${o.description ? `<span class="d">: ${esc(o.description)}</span>` : ""}</button>`;
    }).join("")}</div>`;
    if (qs.length === 1) return `<div class="qtext">${esc(qs[0].question)}</div>${opts(qs[0], 0)}${qs[0].multiSelect ? submitAnswers(it) : ""}`;
    // Several questions: one at a time, with tabs to move between them (like Claude Code's own prompt).
    const cur = stepOf(it);
    const tabs = qs.map((q, n) => `<button class="qtab ${n === cur ? "on" : ""} ${isAnswered(it, q) ? "done" : ""}" data-qstep="${esc(it.id)}:${n}">${isAnswered(it, q) ? "✓ " : ""}${esc(q.header || `Question ${n + 1}`)}</button>`).join("");
    const q = qs[cur];
    const nav = q.multiSelect && cur < qs.length - 1 ? `<button class="btn" data-qstep="${esc(it.id)}:${cur + 1}">Next question</button>` : "";
    // An answer you typed yourself (not one of the options) shows under the question.
    const typed = [...(q.multiSelect ? d.choices[q.question] || [] : [d.choices[q.question]].filter(Boolean))].filter((c) => !(q.options || []).some((o) => o.label === c));
    const yours = typed.length ? `<div class="typed-answer">Your answer: “${esc(typed.join(", "))}”</div>` : "";
    return `<div class="qtabs">${tabs}<span class="qcount">${cur + 1} of ${qs.length}</span></div><div class="qtext">${esc(q.question)}${q.multiSelect ? `<span class="dim"> · pick any</span>` : ""}</div>${opts(q, cur)}${yours}<div class="row-btns">${nav}${submitAnswers(it)}</div>`;
  }
  if (t === "bash") return `<div class="code-wrap"><pre class="slab"><span class="ps">$ </span>${esc(i.command)}</pre>${COPY_BTN}</div>`;
  if (t === "edit" || t === "multiedit") {
    const edits = t === "multiedit" ? i.edits || [] : [{ old_string: i.old_string, new_string: i.new_string }];
    return `<div class="dim" style="font-size:12px">${esc(shortPath(i.file_path || ""))}</div>` + edits.map((e) => `<div class="diff"><pre class="del">− ${esc(e.old_string)}</pre><pre>+ ${esc(e.new_string)}</pre></div>`).join("");
  }
  if (t === "write") return `<div class="dim" style="font-size:12px">${esc(shortPath(i.file_path || i.path || ""))}</div><pre class="slab">${esc(i.content)}</pre>`;
  return `<pre class="slab">${esc(JSON.stringify(i, null, 2))}</pre>`;
}
/** Send all answers: shown once every question has one (needed when any allow several picks). */
function submitAnswers(it) {
  if (!questions(it).some((q) => q.multiSelect)) return "";
  const ready = !!answersFor(it);
  return `<span class="grow"></span><button class="btn primary" data-act="submit-answers" data-id="${esc(it.id)}" ${ready ? "" : "disabled"}>${ready ? "Send answers" : "Answer each question"}</button>`;
}
function whyLine(it) {
  const c = (it.context || []).filter((x) => x.role !== "user").at(-1) || it.context?.at(-1);
  return c ? `<div class="why"${c.text.trim().includes("\n") ? "" : " data-cut"}>Why: ${esc(firstLine(c.text))}</div>` : "";
}
/** A dialog Cue read off the session's screen (a terminal card): what it says, and its choices as buttons. */
function dialogHtml(it) {
  const d = it.tool_input?.dialog;
  if (!d?.options?.length) return "";
  return `${d.detail ? `<div class="qtext">${esc(d.detail)}</div>` : ""}<div class="opts">${d.options.map((o, n) => `<button class="opt ${n === d.selected ? "on" : ""}" data-tpick="${n + 1}" data-id="${esc(it.id)}">${n + 1} · ${esc(o)}</button>`).join("")}</div>`;
}
function decisionButtons(it) {
  if (inTerminal(it) && inTmux(it.session_id) && termSid === it.session_id) return `<div class="row-btns"><span class="dim">Answer it in the terminal above.</span></div>`;
  if (inTerminal(it) && it.tool_input?.dialog?.options?.length) return `${dialogHtml(it)}<div class="row-btns"><span class="dim">Or answer it in its terminal.</span><span class="grow"></span>${inTmux(it.session_id) ? `<button class="btn" data-act="term-open" data-sid="${esc(it.session_id)}">Open terminal</button>` : `<button class="btn" data-act="go" data-id="${esc(it.id)}">${openInLabel(sessionOf(it.session_id))}</button>`}</div>`;
  if (inTerminal(it) && inTmux(it.session_id)) return `<div class="row-btns"><span class="dim">It's waiting in its terminal: answer it there.</span><span class="grow"></span><button class="btn primary" data-act="term-open" data-sid="${esc(it.session_id)}">Answer in terminal</button></div>`;
  if (inTerminal(it)) return `<div class="row-btns"><span class="dim">Cue can't answer this one: answer it in its terminal.</span><span class="grow"></span><button class="btn primary" data-act="go" data-id="${esc(it.id)}">${openInLabel(sessionOf(it.session_id))}</button></div>`;
  if (redirectFor === it.id) return `<div class="row-btns">${field(it, it.kind === "question" ? (questions(it).length > 1 ? `Your own answer to question ${stepOf(it) + 1}…` : "Your own answer…") : "Tell it what to do instead…", it.kind === "question" ? (questions(it).length > 1 && questions(it).filter((q) => !isAnswered(it, q)).length > 1 ? "Next" : "Send") : "Redirect")}</div>`;
  if (it.kind === "question") return `<div class="row-btns"><button class="btn deny" data-act="deny" data-id="${esc(it.id)}">Decline</button><button class="btn" data-act="redirect" data-id="${esc(it.id)}">Type answer…</button></div>`;
  const sugg = it.suggestions || [];
  const menu = menuFor === it.id && sugg.length ? `<div class="menu">${sugg.map((s, n) => { const x = describeSuggestion(s); return `<button data-always="${n}" data-id="${esc(it.id)}"><div>${esc(x.title)}</div><div class="sub">${esc(x.where)}</div></button>`; }).join("")}</div>` : "";
  return `<div class="row-btns"><button class="btn deny flex" data-act="deny" data-id="${esc(it.id)}">Deny</button>
    ${sugg.length ? `<span class="always">${menu}<button class="btn" data-act="menu" data-id="${esc(it.id)}">Always…</button></span>` : ""}
    <button class="btn" data-act="redirect" data-id="${esc(it.id)}">↪</button>
    <button class="btn primary flex" data-act="allow" data-id="${esc(it.id)}">Allow</button></div>`;
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

// ---------- Board view: ACTIVE | WAITING | WORKING + RECENTLY ANSWERED ----------
// In conversation: a card that came back within a few minutes of your last message to that session.
// It leaves the group (back to its place, oldest first) once it's sat that long unanswered, except
// while it's open in Active, so it never moves under you.
const TALK_MS = 5 * 60000;
/** How soon after your message this card came back (ms), or null: not a conversation. */
function talkGap(it) {
  if (it.back_ms) return null;   // you sent it to the back
  const you = (sessionOf(it.session_id)?.thread || []).filter((e) => e.role === "you" && e.at_ms <= it.created_ms).pop();
  const gap = you ? it.created_ms - you.at_ms : null;
  return gap !== null && gap <= TALK_MS ? gap : null;
}
const talkStale = (it) => now() - it.created_ms > TALK_MS && active?.sid !== it.session_id;
const talking = (it) => talkGap(it) !== null && !talkStale(it);
/** Everything waiting on you, finished turns and decisions together: conversations first (in the order
 *  they came back), then oldest first. */
const needsYou = () => { const all = [...groups().decide, ...groups().yours].sort((a, b) => (a.back_ms ?? a.created_ms) - (b.back_ms ?? b.created_ms)); return [...all.filter(talking), ...all.filter((i) => !talking(i))]; };
/** Working on what you said in the last few minutes: it stays in the conversation group while it works
 *  (a card like the others, saying "working"; its card stays under Working too); once it's worked longer, it's a job, and only in Working. */
const workingOnYours = (s) => { const you = (s.thread || []).filter((e) => e.role === "you").pop(); return !!you && now() - you.at_ms <= TALK_MS; };
/** "3 bg": what a session has running in the background (agents and scripts), beside its state, on any of its
 *  cards. A click opens the session onto it (app.js). */
const bgPill = (s) => s?.background > 0 ? `<button class="bg-pill" data-bg-open="${esc(s.session_id)}">${s.background} bg</button>` : "";
const talkWorkRow = (s, open) => `<div class="nrow talk-work ${open ? "on" : ""}" data-session="${esc(s.session_id)}"><div class="nrow-top">${projDot(s.project, true)}${badge(s.harness)}${nameSpan(s.session_id, s.project)}${machTag(s.machine)}<span class="dim">working</span>${bgPill(s)}<span class="grow"></span><span class="age">${ago(s.since_ms)}</span></div><div class="nrow-text">${esc(s.compacting_ms ? "Compacting…" : s.doing || "Thinking")}</div>${s.prompt ? `<div class="nrow-text talk-said">› ${esc(s.prompt)}</div>` : ""}</div>`;
/** The grey line under a card's name: why it's in conversation, or that it just left it. */
function talkNote(it) {
  const gap = talkGap(it);
  if (gap === null) return "";
  if (!talkStale(it)) return `<div class="talk-why">answered ${ago(now() - gap)} after you</div>`;
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
  const last = [...(p.thread || [])].reverse().find((e) => e.role === "agent");
  return `<div class="tl star-tl parked ${w ? "waking" : ""} ${open ? "on" : ""}" style="${projStyle(p.project)}" data-session="${esc(sid)}">${P_SIGN}
    <div class="tl-top"><span class="tl-name">${esc(p.name || p.project)}</span>${oddAgent(p.harness)}${machTag(p.machine)}<span class="tl-parked">${w ? "resuming…" : `parked ${parkedAgo(p)}`}</span>${starBtn(sid)}</div>
    <div class="tl-title tl-said"><span>${esc(last ? plain(firstLine(last.text)) : homeless(p.cwd))}</span>${w ? "" : `<button class="tl-wake" data-wake="${esc(sid)}">↺ Resume</button>`}</div></div>`;
}

/** × on a finished turn: nothing to reply, take it off Waiting. Decisions don't get one (the agent is blocked on them). */
const clearX = (it) => it.kind === "waiting" ? `<button class="x-clear" data-act="dismiss" data-id="${esc(it.id)}" title="Take it off Waiting" aria-label="Take it off Waiting">×</button>` : "";
/** "Later" on a Waiting row: move the session to Need to decide. */
const parkBtn = (sid) => `<button class="park-btn" data-park="${esc(sid)}:1">Later</button>`;
const backBtn = (it) => `<button class="back-btn" data-act="back" data-id="${esc(it.id)}" title="Send to back of the queue" aria-label="Send to back of the queue">↓</button>`;
/** A row in Need to decide: the session, its finished turn if any, ↩ to put it back. Click to open it. */
function laterRow({ sid, s, it }) {
  const on = active?.sid === sid ? "on" : "";
  const what = it ? "" : s.state === "working" ? "working" : "idle";
  const text = it ? plain(summary(it)) : s.prompt ? `› ${s.prompt}` : "";
  return `<div class="nrow later ${on}" ${it ? `data-big="${esc(it.id)}"` : `data-session="${esc(sid)}"`}>
    <div class="nrow-top">${projDot(it?.project || s.project, s.state === "working")}${badge(it?.harness || s.harness)}${nameSpan(sid, it?.project || s.project)}${machTag(it?.machine || s.machine)}${what ? `<span class="dim">${what}</span>` : ""}${bgPill(s)}<span class="grow"></span><span class="age">${ago(it?.created_ms ?? s.since_ms)}</span>${starBtn(sid)}<button class="park-btn back" data-park="${esc(sid)}:0">↩ Waiting</button></div>
    ${cardCrew(sid)}
    ${text ? `<div class="nrow-text">${esc(text)}</div>` : ""}${insCardLine(sid)}</div>`;
}
/** A compact row in Waiting: enough to recognise it, quick answers for the easy ones, click to open in Active. */
function needRow(it, ghost, open = false) {
  if (ghost) {
    const h = state.history.find((x) => x.id === it.id);
    return `<div class="nrow ghost ${ghost === "fresh" ? "fresh" : ""} ${ghosts.get(it.id)?.soft ? "soft" : ""}" data-detail="${esc(it.id)}"><div class="nrow-top">${badge(it.harness)}${nameSpan(it.session_id, it.project)}${machTag(it.machine)}</div>
      <div class="nrow-done ${h ? outcomeClass(h) : ""}">✓ ${esc(h ? outcomeText(h) : cleared.has(it.id) ? "cleared" : "picked up in the terminal")}</div></div>`;
  }
  const q = it.kind === "question" ? questions(it) : [];
  let quick = "";
  if (it.kind === "permission") quick = `<div class="nrow-acts"><button class="btn deny" data-act="deny" data-id="${esc(it.id)}">Deny</button><button class="btn primary" data-act="allow" data-id="${esc(it.id)}">Allow</button></div>`;
  else if (inTerminal(it) && (it.tool_input?.dialog?.options || []).length && it.tool_input.dialog.options.length <= 4) quick = `<div class="nrow-acts wrap">${it.tool_input.dialog.options.map((o, n) => `<button class="btn" data-tpick="${n + 1}" data-id="${esc(it.id)}">${esc(o)}</button>`).join("")}</div>`;
  else if (inTerminal(it)) quick = `<div class="nrow-acts">${inTmux(it.session_id) ? `<button class="btn primary" data-act="term-card" data-id="${esc(it.id)}">Answer in terminal</button>` : `<button class="btn primary" data-act="go" data-id="${esc(it.id)}">${openInLabel(sessionOf(it.session_id))}</button>`}</div>`;
  else if (q.length === 1 && !q[0].multiSelect && (q[0].options || []).length <= 4) quick = `<div class="nrow-acts wrap">${q[0].options.map((o, oi) => `<button class="btn" data-pick="0:${oi}" data-id="${esc(it.id)}">${esc(o.label)}</button>`).join("")}</div>`;
  // An interrupted turn (Esc) waits at "What should Claude do instead?": say so, and offer Continue.
  if (it.interrupted) quick = `<div class="nrow-acts"><button class="btn primary" data-act="continue" data-id="${esc(it.id)}">Continue</button></div>`;
  return `<div class="nrow ${open ? "on" : ""} ${it.kind === "waiting" ? "turn" : "ask"} ${answering.has(it.id) ? "answering" : ""}" data-big="${esc(it.id)}">
    <div class="nrow-top">${projDot(it.project, sessionOf(it.session_id)?.state === "working")}${badge(it.harness)}${nameSpan(it.session_id, it.project)}${machTag(it.machine)}${it.kind !== "waiting" ? `<span class="dim">${esc(verb(it))}</span>` : it.interrupted ? `<span class="dim intr">interrupted</span>` : ""}${bgPill(sessionOf(it.session_id))}<span class="grow"></span><span class="age">${ago(it.created_ms)}</span>${starBtn(it.session_id)}${it.kind === "waiting" ? backBtn(it) + parkBtn(it.session_id) : ""}${clearX(it)}</div>
    ${talkNote(it)}${cardCrew(it.session_id)}
    <div class="nrow-text ${isBash(it) ? "mono" : ""}">${esc(plain(summary(it)))}</div>${insCardLine(it.session_id, it.kind !== "waiting")}${quick}${crewOf(it.session_id)?.role === "lead" ? teamTree(it.session_id) : ""}</div>`;
}
/** A lead whose team needs you while it has nothing waiting itself: a slim card holding the team's tree.
 *  Its title line opens the lead; a row in the tree opens that executor. */
function teamHead(lead, open) {
  const s = sessionOf(lead);
  return `<div class="nrow ask team-head ${open ? "on" : ""}"><div class="nrow-top" data-session="${esc(lead)}" role="button">${projDot(s.project, s.state === "working")}${badge(s.harness)}${nameSpan(lead, s.project)}<span class="dim">its team needs you</span></div>
    ${cardCrew(lead)}${teamTree(lead)}</div>`;
}

/** Why a session is idle (shown under Sessions → Idle); "" when it's working. */
// ---------- usage limits ----------
/** A limit that has lifted (its reset time passed, or another turn went through). */
const lifted = (l) => !!l?.resets_ms && l.resets_ms <= now();
const limitedSessions = () => state.sessions.filter((s) => s.state === "limited" && s.limit);
/** "4:10am" today (or within the next day), "Sat 8am" further out. */
function clockAt(ms) {
  const d = new Date(ms), h = d.getHours() % 12 || 12, m = d.getMinutes(), ap = d.getHours() < 12 ? "am" : "pm";
  const t = m ? `${h}:${String(m).padStart(2, "0")}${ap}` : `${h}${ap}`;
  return ms - now() < 20 * 3600000 ? t : `${d.toLocaleDateString(undefined, { weekday: "short" })} ${t}`;
}
const untilText = (ms) => { const m = Math.round(ms / 60000); return m < 1 ? "under a minute" : m < 60 ? `${m}m` : `${Math.floor(m / 60)}h ${m % 60}m`; };
const scopeName = (l) => l.scope === "all" ? "Claude Code" : l.scope;
function limitNote(s) {
  const l = s.limit;
  if (!l) return "out of usage";
  if (lifted(l)) return "usage is back · resend";
  if (l.scope !== "all") return `${l.scope} used up${l.resets_ms ? ` until ${clockAt(l.resets_ms)}` : ""}`;
  return `out of usage${l.resets_ms ? ` · resets ${clockAt(l.resets_ms)}` : ""}`;
}
/** Resend what each paused session didn't get to run. */
async function resend(sids) {
  let ok = 0;
  for (const sid of sids) {
    const s = sessionOf(sid);
    if (!s?.prompt) continue;
    try { await invoke("send_to_session", { sessionId: sid, text: s.prompt, images: [], now: false }); ok++; }
    catch (e) { toast(`Couldn't resend to ${s.project}: ${e}`); }
  }
  if (ok) toast(ok === 1 ? "Resent" : `Resent to ${ok} sessions`);
}
let usageOpen = false;
/** Next to Board / History: the plan's usage, or loudly, that it's used up. */
function usageChip() {
  const limited = limitedSessions();
  const out = limited.filter((s) => s.limit.scope === "all" && !lifted(s.limit));
  const back = limited.filter((s) => lifted(s.limit) && now() - s.limit.resets_ms < 5 * 60000);
  const model = limited.filter((s) => s.limit.scope !== "all" && !lifted(s.limit));
  const paused = (n) => `${n} paused`;
  if (out.length) {
    const r = Math.max(...out.map((s) => s.limit.resets_ms || 0));
    return `<span class="usage-wrap"><button class="usage-out" data-usage="out"><span class="uo-dot"></span>Claude Code out of usage<span class="uo-sub">${r ? ` · resets ${clockAt(r)}` : ""} · ${paused(out.length)}</span></button>${usagePop()}</span>`;
  }
  if (back.length) return `<span class="usage-wrap"><span class="usage-back"><span class="ub-dot"></span>${esc(scopeName(back[0].limit))} is back<button class="ub-btn" data-usage="resend">Resend ${back.length}</button></span></span>`;
  if (model.length) {
    const l = model[0].limit;
    return `<span class="usage-wrap"><button class="usage-chip soft-warn" data-usage="model"><span class="uf-dot"></span>${esc(l.scope)} used up<span class="uo-sub">${l.resets_ms ? ` · ${clockAt(l.resets_ms)}` : ""} · ${paused(model.length)}</span></button>${usagePop()}</span>`;
  }
  const u = state.usage;
  const uf = state.usage_file || {};
  // Settings → Usage can hide Claude Code's limits (e.g. behind a proxy, where only your file matters).
  const claude = state.settings?.usage?.claude !== false && u && (u.five_hour || u.seven_day);
  if (!claude && !(uf.meters || []).length && !uf.error) return "";
  const meter = (label, w) => w ? `<span class="ul">${label}</span><span class="um ${w.pct >= 100 ? "full" : w.pct >= 85 ? "high" : ""}"><i style="width:${Math.min(100, w.pct)}%"></i></span><span class="up">${Math.round(w.pct)}%</span>` : "";
  // Your own meters (Settings → Usage file) sit beside Claude Code's, drawn the same way: the first
  // row in the bar, and all of them in a list of the same look that hovering it shows.
  const level = (pct) => pct === null ? "" : pct >= 100 ? "full" : pct >= 85 ? "high" : "";
  const fileMeter = (m, short) => {
    const pct = m.limit ? (m.spent / m.limit) * 100 : null;
    const label = short && m.label.length > 12 ? m.label.slice(0, 11) + "…" : m.label;
    return `${m.label ? `<span class="ul">${esc(label)}</span>` : ""}${pct === null ? "" : `<span class="um ${level(pct)}"><i style="width:${Math.min(100, pct)}%"></i></span>`}<span class="up">${esc(spendText(m))}</span>`;
  };
  const meters = uf.meters || [];
  const mine = meters.length ? `<span class="uf-first">${fileMeter(meters[0], true)}</span>` : "";
  const more = meters.length > 1 && !usageOpen ? `<span class="uf-more">${meters.map((m) => `<span class="uf-row">${fileMeter(m)}${m.resets_ms ? `<span class="uf-note">resets ${clockAt(m.resets_ms)}</span>` : ""}</span>`).join("")}</span>` : "";
  const broken = uf.error ? `<span class="ul uf-bad" title="${esc(uf.error)}">usage file ⚠</span>` : "";
  return `<span class="usage-wrap"><button class="usage-chip" data-usage="meter">${claude ? meter("5h", u.five_hour) + meter("wk", u.seven_day) : ""}${mine}${broken}</button>${more}${usagePop()}</span>`;
}
/** "$12.40/$50", "48/200 credits", "$3.10": an amount from your usage file, in its unit. */
function spendText(m, long = false) {
  const n = (x) => Number(x).toLocaleString(undefined, { maximumFractionDigits: 2, minimumFractionDigits: Number.isInteger(x) ? 0 : 2 });
  const sym = m.unit.length <= 2 && !/[a-z]/i.test(m.unit);
  const amt = (x) => (sym ? `${m.unit}${n(x)}` : n(x));
  const both = m.limit ? `${amt(m.spent)}${long ? " of " : "/"}${amt(m.limit)}` : amt(m.spent);
  return sym ? both : `${both} ${m.unit}`;
}
function usagePop() {
  const u = state.usage;
  const uf = state.usage_file || {};
  if (!usageOpen) return "";
  // Your usage file: its meters with their details, or what's wrong with it.
  const fileSec = !(uf.meters || []).length && !uf.error ? "" : `<div class="up-group"><div class="up-title">${uf.error ? "Usage file" : "Your usage"}<span> · ${esc(uf.path || "")}${uf.as_of_ms ? `, as of ${clockAt(uf.as_of_ms)}` : ""}</span></div>
    ${uf.error ? `<div class="up-err">${esc(uf.error)}</div>` : uf.meters.map((m) => {
      const pct = m.limit ? (m.spent / m.limit) * 100 : null;
      const lvl = pct === null ? "" : pct >= 100 ? "full" : pct >= 85 ? "high" : "";
      return `<div class="up-row money ${lvl}"><span class="up-name">${esc(m.label || "Usage")}</span>${pct === null ? `<span class="up-bar none"></span>` : `<span class="up-bar"><i style="width:${Math.min(100, pct)}%"></i></span>`}<span class="up-pct">${esc(spendText(m, true))}</span>${m.resets_ms ? `<span class="up-note">resets ${clockAt(m.resets_ms)}</span>` : ""}</div>`;
    }).join("")}</div>`;
  const claude = state.settings?.usage?.claude !== false && u && (u.five_hour || u.seven_day);
  if (!claude) return `<div class="usage-pop"><div class="up-head">Usage</div>${fileSec}</div>`;
  const lvl = (w) => (w.pct >= 100 ? "full" : w.pct >= 85 ? "high" : "");
  const row = (name, w, note = "") => w ? `<div class="up-row ${lvl(w)}"><span class="up-name">${esc(name)}</span><span class="up-bar"><i style="width:${Math.min(100, w.pct)}%"></i></span><span class="up-pct">${Math.round(w.pct)}%</span>${note ? `<span class="up-note">${esc(note)}</span>` : ""}</div>` : "";
  // Rows that reset together sit under one heading that says when, instead of repeating it per row.
  const group = (title, w, rows) => rows ? `<div class="up-group"><div class="up-title">${title}${w?.resets_ms ? `<span> · resets ${clockAt(w.resets_ms)}</span>` : ""}</div>${rows}</div>` : "";
  // Per-model rows come from Claude Code's /usage panel, which only refreshes while it's open.
  const models = (u.models || []).map((m) => {
    const notes = [m.resets_ms && Math.abs(m.resets_ms - (u.seven_day?.resets_ms || 0)) > 3600e3 ? `resets ${clockAt(m.resets_ms)}` : "", now() - m.fetched_ms > 15 * 60000 ? `${ago(m.fetched_ms)} old` : ""];
    return row(m.name, m, notes.filter(Boolean).join(" · "));
  }).join("");
  return `<div class="usage-pop"><div class="up-head">Claude Code usage<span class="dim">${ago(u.at_ms)} ago</span></div>
    ${group("Session", u.five_hour, row("5 hours", u.five_hour))}${group("Weekly", u.seven_day, row("All models", u.seven_day) + models)}
    ${fileSec}<div class="up-foot">Claude Code's limits don't count Codex and Pi.</div></div>`;
}
/** Its machine couldn't be reached on Cue's last try: the card says so whatever it last showed, and × takes it out of Cue. */
const cutOff = (s) => !!s.machine && (state.unreachable || []).includes(s.machine);
const idleNote = (s) => cutOff(s) ? `can't reach ${machShort(s.machine)}` : s.state === "working" ? "" : s.state === "limited" ? limitNote(s) : s.state === "agent" ? `waiting on ${s.driven_by || "another agent"}` : s.state === "stopped" ? "stopped by you" : "cleared from Waiting";

// The right column: Sessions takes what it needs (up to half), Recently answered scrolls in the rest.
const RECENT = 10;
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
  };
  const open = !wColl ? "waiting" : !sColl ? "sessions" : null;
  return {
    tpl: ["minmax(0,1fr)", ...(open ? [share(colFr[open])] : []), "34px"].join(" "),
    grips: open ? [{ name: open, left: `calc(100% - 62px - ${share(colFr[open])})` }] : [],
  };
}
const colGrip = (g) => `<div class="col-grip ${colSizing?.name === g.name ? "on" : ""}" data-col-grip="${g.name}" style="left:${g.left}"><i></i>${colSizing?.name === g.name && colSizing.edge ? `<b>${g.name.toUpperCase()} · ${colSizing.edge.toUpperCase()}</b>` : ""}</div>`;
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
function boardView() {
  if (!quietOpen) current();   // settles what Active shows before the lists mark it
  const { working, later } = groups();
  const needs = needsYou();
  // What the Active pane shows stays where it is in its list, lit up (lime): opening
  // something never moves a row, so the lists don't shift under your pointer.
  const openSid = quietOpen || active?.sid || null;
  const isOpen = (sid) => !!openSid && sid === openSid;
  const live = new Set(needs.map((i) => i.id));
  // A ghost fades in once: every later render (they come often while sessions work) draws it still.
  const ghostRows = [...ghosts.values()].filter((g) => !live.has(g.it.id)).map((g) => { const fresh = !g.shown; g.shown = true; return { ...g.it, _ghost: fresh ? "fresh" : "shown" }; });
  // A team is one card: an executor's card goes into its lead's tree (when Cue has the lead). A lead with
  // nothing of its own waiting still gets a card, where its executor's would have been.
  // Only while the lead still lists it: an executor that left its team (closed, or handed to another lead)
  // can still name the old lead, whose card would then hold an empty tree and its own card none.
  const listed = (sid, lead) => (crewOf(lead)?.executors || []).some((e) => e.session_id === sid);
  const leadOf = (i) => { const m = crewOf(i.session_id); return m?.role === "executor" && sessionOf(m.lead) && listed(i.session_id, m.lead) ? m.lead : null; };
  const teamLeads = new Set(needs.map(leadOf).filter(Boolean));
  const heads = [...teamLeads].filter((l) => !needs.some((i) => i.session_id === l))
    .map((l) => ({ _team: l, created_ms: Math.min(...needs.filter((i) => leadOf(i) === l).map((i) => i.back_ms ?? i.created_ms)) }));

  // A lead's team is one tree under the lead, wherever its card is: in Waiting (its own card, or the slim one
  // for a team that needs you), else in Sessions, working or idle. Its executors are in that tree whatever
  // their own project is, busy ones too, not on cards of their own.
  const waitTree = new Set([...needs.filter((i) => crewOf(i.session_id)?.role === "lead").map((i) => i.session_id), ...teamLeads]);
  const sessTree = new Set(working.filter((s) => crewOf(s.session_id)?.role === "lead" && !waitTree.has(s.session_id)).map((s) => s.session_id));
  const inTree = (sid) => { const m = crewOf(sid); return m?.role === "executor" && (waitTree.has(m.lead) || sessTree.has(m.lead)) && listed(sid, m.lead); };
  const talkWork = working.filter((s) => s.state === "working" && !idleNote(s) && !inTree(s.session_id) && !sessTree.has(s.session_id) && workingOnYours(s));
  // Conversations first (when there are any, under their own heading): the cards that came back, then the
  // sessions still working on your message. Then oldest first. A card you just answered fades out in the
  // group it was in, unless its session is back at work there.
  const talkSids = new Set(talkWork.map((x) => x.session_id));
  // A session working on your message shows under In conversation and under Sessions; 5 minutes on, Sessions only.
  const shown = working.filter((s) => !inTree(s.session_id));
  const rows = [...needs.filter((i) => !leadOf(i)), ...heads, ...ghostRows.filter((g) => !talkSids.has(g.session_id))].sort((a, b) => (a.back_ms ?? a.created_ms) - (b.back_ms ?? b.created_ms));
  const inTalk = (i) => !i._team && talking(i);
  const draw = (list) => list.map((i) => (i._team ? teamHead(i._team, isOpen(i._team)) : needRow(i, i._ghost, isOpen(i.session_id)))).join("");
  const talks = rows.filter(inTalk), rest = rows.filter((i) => !inTalk(i)), count = (list) => list.filter((i) => !i._ghost).length;
  // With a project picked, what's waiting in the others is off the Board, never out of mind: a line says
  // how many, and shows all of them. (An agent blocked on you with nothing on screen was the bug.)
  const elsewhere = projFilter ? state.items.filter((i) => !inProj(i.project) && !parkedIds().has(i.session_id)).length : 0;
  const needRows = talks.length || talkWork.length
    ? `<div class="col-sub">IN CONVERSATION · LAST ${TALK_MS / 60000} MIN${count(talks) ? ` · ${count(talks)}` : ""}${talkWork.length ? ` · ${talkWork.length} working` : ""}</div>${draw(talks)}${talkWork.map((x) => talkWorkRow(x, isOpen(x.session_id))).join("")}${rest.length ? `<button class="col-sub fold-head sub-fold" data-fold-oldest aria-expanded="${!oldestShut()}"><span class="fold-arrow">${oldestShut() ? "▸" : "▾"}</span> OLDEST FIRST · ${count(rest)}</button>${oldestShut() ? "" : draw(rest)}` : ""}`
    : draw(rest);
  // Sessions counts what it shows: its cards, and the executors in its leads' trees.
  const treed = working.filter((s) => inTree(s.session_id) && sessTree.has(crewOf(s.session_id).lead)).length;
  const stateNote = (s) => { const i = state.items.find((x) => x.session_id === s.session_id); return i ? (i.kind === "waiting" ? "your turn" : "asks you") : preStart(s) ? "needs you in its terminal" : idleNote(s); };
  // A lead's card in Sessions holds its tree: only its top part opens the lead (a row opens that executor,
  // and the tree's buttons are their own).
  const card = (s, open = false) => { const tree = sessTree.has(s.session_id) ? teamTree(s.session_id) : "", go = `data-session="${esc(s.session_id)}"`;
    const doing = preStart(s) ? `Not started: ${preStart(s)}` : s.compacting_ms ? "Compacting…" : s.state === "working" && s.doing ? s.doing : "";
    return `<div class="working click ${tree ? "team" : ""} ${idleNote(s) ? "on-agent" : ""} ${open ? "on" : ""}" ${tree ? "" : go}>${tree ? `<div class="team-top" ${go} role="button">` : ""}
      <div class="card-head">${projDot(s.project, (s.state === "working" || s.compacting_ms) && !cutOff(s) && !preStart(s))}${nameSpan(s.session_id, s.project)}${roleTag(s.session_id) || oddAgent(s.harness)}${machTag(s.machine)}${bgPill(s)}<span class="grow"></span><span class="age" style="color:inherit">${idleNote(s) && s.state !== "limited" && !state.items.some((x) => x.session_id === s.session_id) ? "idle " : ""}${ago(s.since_ms)}</span>${starBtn(s.session_id)}${isStarred(s.session_id) && !cutOff(s) ? "" : hideX(s)}</div>
      ${stateNote(s) ? `<div class="agent-note">${esc(stateNote(s))}</div>` : ""}
      ${s.queued || s.held ? ((q) => `<div class="queued-note">${s.held ? "Kept" : "Queued"}: “${esc(q.length > 80 ? q.slice(0, 80) + "…" : q)}”</div>`)((s.held || s.queued).text) : ""}
      ${bar(s.session_id)}${doing ? `<div class="doing">${esc(doing)}</div>` : ""}${s.prompt ? `<div class="prompt">› ${esc(s.prompt)}</div>` : ""}${tree ? `</div>${tree}` : ""}</div>`; };
  const busy = shown.filter((s) => !idleNote(s)), idle = shown.filter((s) => idleNote(s) && s.state !== "limited"), outs = shown.filter((s) => s.state === "limited");
  const workCol = (busy.length ? `<div class="col-sub">WORKING · ${busy.length}</div>${busy.map((s) => card(s, isOpen(s.session_id))).join("")}` : "")
    + (outs.length ? `<div class="col-sub lim">${outs.every((s) => lifted(s.limit)) ? "READY TO RESEND" : "OUT OF USAGE"} · ${outs.length}</div>${outs.map((s) => card(s, isOpen(s.session_id))).join("")}` : "")
    + (idle.length ? `<div class="col-sub">IDLE · ${idle.length}</div>${idle.map((s) => card(s, isOpen(s.session_id))).join("")}` : "")
    || `<div class="quiet-line">No other sessions.</div>`;

  // One entry per session, its latest answer (the rest are in History).
  const seen = new Set();
  const recent = state.history.filter((i) => inProj(i.project) && !seen.has(i.session_id || i.id) && seen.add(i.session_id || i.id)).slice(0, RECENT);
  const recentList = () => recent.length
    ? `<div class="card recent">${recent.map(recentEntry).join("")}
        ${state.history.length > recent.length ? `<button class="link" style="padding:4px 12px 10px" data-act="open-history">All history →</button>` : ""}</div>`
    : `<div class="empty-col">Your answers show up here.</div>`;
  // Starred, in the order you starred them (so they don't shuffle as they work).
  // A starred one you parked stays here too, as parked.
  const stars = [...state.sessions, ...(state.parked || [])].filter((s) => isStarred(s.session_id) && inProj(s.project)).sort((a, b) => (a.starred_ms || Infinity) - (b.starred_ms || Infinity));
  const starNeeds = stars.filter((s) => state.items.some((i) => i.session_id === s.session_id)).length;
  const drawer = (name, title, count, body) => { const open = drawerOpen(name); return `<div class="sec sec-drawer sec-${name} ${open ? "" : "shut"}"><button class="col-head fold-head" data-fold="${name}" aria-expanded="${open}"><span class="fold-arrow">${open ? "▾" : "▸"}</span>${title} <span>${count}</span></button>${open ? `<div class="sec-body">${body()}</div>` : ""}</div>`; };
  // Fixed layout: every column stays where it is (nothing jumps as the queue changes).
  const coll = (name) => (focus ? !focusShown.has(name) : collapsed.has(name));
  const wColl = coll("waiting"), sColl = coll("sessions");
  const railLabel = (name, count) => {
    const prev = railLast[name];
    if (prev !== undefined && count > prev) railBubble[name] = Date.now() + 900;
    railLast[name] = count;
    return `<button class="pin ${railBubble[name] > Date.now() ? "bubble" : ""}" data-open-section="${name}" aria-label="Open ${name}"><span class="vt">${name.toUpperCase()} · ${count}</span></button>`;
  };
  const waitingCol = `<div class="col split col-waiting">
      <div class="sec sec-waiting"><div class="col-head">WAITING <span>${needs.length}${needs.some((i) => i.kind !== "waiting") ? ` · ${needs.filter((i) => i.kind !== "waiting").length} asking` : ""}${needs.length > 1 && !talks.length && !talkWork.length ? " · oldest first" : ""}</span><span class="grow"></span>${colReset("waiting")}<button class="collapse-x" data-collapse="waiting" aria-label="Fold Waiting">»</button></div><div class="sec-body">${needRows || `<div class="quiet-line">Nothing waiting.</div>`}${elsewhere ? `<button class="quiet-line link" data-pj="pick" data-project="">${elsewhere} waiting in other projects · show all</button>` : ""}</div></div>
      ${later.length ? drawer("later", "NEED TO DECIDE", later.length, () => later.map(laterRow).join("")) : ""}
    </div>`;
  const sessionsCol = `<div class="col split col-sessions ${DRAWERS.some(drawerOpen) ? "" : "drawers-shut"}">
      <div class="sec sec-sessions"><div class="col-head">SESSIONS <span>${shown.length + treed}</span><span class="grow"></span>${colReset("sessions")}<button class="collapse-x" data-collapse="sessions" aria-label="Fold Sessions">»</button></div><div class="sec-body">${workCol}</div></div>
      ${drawer("starred", "STARRED", stars.length ? `${stars.length}${starNeeds ? ` · ${starNeeds} need${starNeeds === 1 ? "s" : ""} you` : ""}` : "", () => stars.length ? `<div class="card recent">${stars.map((s) => (s.parked_ms ? starParkedEntry(s, isOpen(s.session_id)) : starEntry(s, isOpen(s.session_id)))).join("")}</div>` : `<div class="quiet-line">Star a session (★ on its card) to keep it here.</div>`)}
      ${drawer("recent", "RECENTLY ANSWERED", recent.length ? `${recent.length} session${recent.length === 1 ? "" : "s"}` : "", recentList)}
      ${shelfDrawer(isOpen)}
    </div>`;
  const railCol = (wColl || sColl) ? `<div class="col rail-col"><div class="rail">${wColl ? railLabel("waiting", needs.length) : ""}${sColl ? railLabel("sessions", shown.length + treed) : ""}</div></div>` : "";
  // main's columns (1.9 : 1 : 0.85 of the board after its two 14px gaps, or as you dragged them); a section
  // that stays open keeps its width when the other folds, and Active takes the room.
  const lay = colLayout(wColl, sColl);
  return `<div class="board" style="grid-template-columns:${lay.tpl}">
    <div class="col main">${focus ? "" : `<div class="col-head">ACTIVE</div>`}${activePane()}</div>
    ${wColl ? "" : waitingCol}${sColl ? "" : sessionsCol}${railCol}${lay.grips.map(colGrip).join("")}
  </div>`;
}

// ---------- sheets ----------
/** The Active pane's title: the session's name with ✎ to rename it (an input while you do), and its
 *  project beside it once it has a name of its own. */
/** Under the Active header: an executor's lead on the left, the session's folder (~ for your home)
 *  on the right, under the buttons. One small line. */
function subHead(cwd, crew, sid, titleOf = sid) {
  // The star sits after the folder: the title row is full.
  // Too long: cut in the middle (~/develop…/web-app), so the folder's own name always shows.
  const path = homeless(cwd), cutAt = Math.max(path.lastIndexOf("/"), 0);
  const folder = cwd ? `<span class="ap-cwd sel"><span class="cwd-head">${esc(path.slice(0, cutAt))}</span><span class="cwd-tail">${esc(path.slice(cutAt))}</span></span>${sid ? starBtn(sid) : ""}` : "";
  const meta = titleOf ? metaInline(titleOf) : "";
  return folder || crew || meta ? `<div class="ap-sub">${meta}${crew}${folder}</div>` : "";
}
function nameHead(sid, project) {
  if (sid && renaming === sid) return `<input class="rename-in" data-text="rename:${esc(sid)}" maxlength="60" placeholder="Name this session" spellcheck="false" value="${esc(draft(`rename:${sid}`).text)}"/>`;
  const name = sid ? nameOf(sid, project) : project;
  // Just the name: its folder's full path is on the line under it (subHead).
  return `<span class="proj">${esc(name)}</span>${sid ? `<button class="rename-btn" data-rename="${esc(sid)}" title="Rename this session" aria-label="Rename">✎</button>` : ""}`;
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
  return `<div class="cv-status stuck"><span class="lim-dot"></span><span><b>${esc(s.rename_error)}.</b> Cue keeps “${esc(s.name)}”.</span><button class="btn small" data-act="rename-again" data-sid="${esc(s.session_id)}">Try again</button><button class="btn small" data-act="rename-dismiss" data-sid="${esc(s.session_id)}" aria-label="Dismiss">×</button></div>`;
}
/** Rename notices already announced (a toast once, wherever you are; the chat keeps showing it). */
const renameErrSeen = new Set();
let renameErrPrimed = false;
/** Search: every session Cue has seen, ended ones too (session names, your messages, the agents'
 *  replies). Running sessions match at once, from what the window already has, by the names you see
 *  (as ⌘K does); the best name match comes first. Then closed and older sessions by name, then what was
 *  said. A live session opens in Active; an ended one's message opens in place. */
let searchResults = [], searchSeq = 0, searchTimer = 0;
function liveChip() {
  const rows = liveRows().filter((r) => inProj(r.project)), asks = rows.filter((r) => r.st === "asks").length, fresh = rows.filter((r) => isNewSession(r.sid)).length;
  return `<span class="lv-wrap"><button class="hchip lv-chip ${liveOpen ? "on" : ""}" data-lv="toggle" title="Every live session (⌘K, ⌘L), and + New (⌘N)"><span class="hdot"></span>${rows.length}<span class="hlbl"> live</span>${asks ? ` <span class="hsub">· ${asks} asks</span>` : ""}${fresh ? ` <span class="hsub">· ${fresh} new</span>` : ""}</button>${liveOpen && !liveSpot ? livePop(rows) : ""}</span>`;
}
/** A newer Cue turned up in the background: a small card in the header's middle, until you choose. */
const updateCard = () => !upd.offer ? "" : `<div class="upd-card"><div><b>Cue ${esc(upd.version)} is out</b>${upd.status === "installing" ? `<div class="dim">Installing… Cue restarts by itself.</div>` : upd.status.startsWith("error:") ? `<div class="dim">${esc(upd.status.slice(6))}</div>` : ""}</div>
  <button class="btn primary small" data-upd="install" ${upd.status === "installing" ? "disabled" : ""}>Update and restart</button><button class="btn small" data-upd="later">Later</button></div>`;

/** Cards that move between redraws (back into Waiting, into Need to decide, or up to
 *  close a gap) glide from where they were instead of jumping, so you can follow where they went.
 *  Keyed by what they show: an ask or turn by its id, a session card by its session. */
const FLIP_SEL = ".board .col:not(.main) .nrow[data-big], .board .col:not(.main) .nrow[data-session]:not(.talk-work), .board .col:not(.main) .working[data-session]";
const flipKey = (el) => (el.dataset.big ? `i:${el.dataset.big}` : `s:${el.dataset.session}`);
const COL_SEL = ".board > .col";
const colKey = (el) => "col:" + (el.classList.contains("main") ? "main" : el.classList.contains("rail-col") ? "rail" : el.classList.contains("col-waiting") ? "waiting" : "sessions");
function flipRects() {
  const m = new Map();
  for (const el of document.querySelectorAll(FLIP_SEL)) m.set(flipKey(el), el.getBoundingClientRect());
  for (const el of document.querySelectorAll(COL_SEL)) m.set(colKey(el), el.getBoundingClientRect());
  return m;
}
function flipPlay(from) {
  if (!from.size || matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  for (const el of document.querySelectorAll(FLIP_SEL)) {
    const was = from.get(flipKey(el));
    if (!was) continue;
    const now = el.getBoundingClientRect(), dx = was.left - now.left, dy = was.top - now.top;
    if (Math.abs(dx) < 1 && Math.abs(dy) < 1) continue;
    el.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: "none" }], { duration: 340, easing: "cubic-bezier(.2,.8,.2,1)" });
  }
  // Columns glide when the board reflows (a section folds to the rail, or opens back out); a column
  // that just appeared (the rail, or a section re-opened) slides in from the right.
  for (const el of document.querySelectorAll(COL_SEL)) {
    const was = from.get(colKey(el));
    if (!was) { el.animate([{ opacity: 0, transform: "translateX(24px)" }, { opacity: 1, transform: "none" }], { duration: 240, easing: "ease-out" }); continue; }
    const now = el.getBoundingClientRect(), dx = was.left - now.left, dy = was.top - now.top;
    if (Math.abs(dx) < 1 && Math.abs(dy) < 1) continue;
    el.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: "none" }], { duration: 340, easing: "cubic-bezier(.2,.8,.2,1)" });
  }
}
/** The top bar never pushes Settings off the edge (a usage file adds meters; chips come and go): when it
 *  doesn't fit, it gives up room step by step: the chips' extras ("· oldest 4m"), the meters' bars, the
 *  meters' labels, then the chips' words (the number and dot stay; the tooltip still says what they are). */
const TOP_STEPS = ["tfit1", "tfit2", "tfit3", "tfit4"];
function fitTop() {
  const h = document.querySelector(".app > .top");
  if (!h) return;
  h.classList.remove(...TOP_STEPS);
  // Fits: Settings (the last button) ends inside the bar. Measured by it, so an open drop-down doesn't count.
  const fits = () => !h.lastElementChild || h.lastElementChild.getBoundingClientRect().right <= h.getBoundingClientRect().right + 1;
  for (const step of TOP_STEPS) {
    if (fits()) return;
    h.classList.add(step);
  }
}
/** A project's swatches never get cut off: near the foot of the projects list (or the window), they open
 *  upward instead. */
function placeColors() {
  const pop = document.querySelector(".pc-pop");
  if (!pop) return;
  const box = pop.closest(".pj-pop")?.getBoundingClientRect() ?? { top: 0, bottom: innerHeight };
  const r = pop.getBoundingClientRect(), btn = pop.previousElementSibling.getBoundingClientRect();
  pop.classList.toggle("up", r.bottom > box.bottom && btn.top - r.height - 6 >= box.top);
}
/** The chat header's buttons never get cut off: when they don't fit, it gives up room step by step
 *  (short labels, then the agent and model, then the "your turn" pill, then the role tag, and the
 *  name shortens). The CSS widths below can't know how many buttons a session has. */
const FIT_STEPS = ["fit1", "fit2", "fit3", "fit4"];
function fitHead() {
  const h = document.querySelector(".ap-head");
  if (!h) return;
  h.classList.remove(...FIT_STEPS);
  // Fits: the last button ends inside the header's right padding.
  const fits = () => {
    const last = [...h.children].filter((k) => k.offsetParent).at(-1);
    return !last || last.getBoundingClientRect().right <= h.getBoundingClientRect().right - parseFloat(getComputedStyle(h).paddingRight) + 1;
  };
  for (const step of FIT_STEPS) {
    if (fits()) return;
    h.classList.add(step);
  }
}

/** A list cut off at the bottom says so, quietly: "3 more" on its last card's bottom edge, counting
 *  the cards not in view below, and a click takes the list to its end. And no card shows half: the list
 *  ends on the last card that fits whole. The lists that scroll in the third column: Sessions, Starred,
 *  Recently answered, Parked (not Waiting and Need to decide: their column scrolls as one, every card in
 *  it, nothing cut short). `trim` re-fits the lists (a redraw, a resize); scrolling only recounts. The
 *  pill shows itself when it appears or its count changes, then fades after a few seconds so a still list
 *  stays clean; scrolling the list (`woke`) brings it back. */
const MORE_LISTS = ".sec-sessions .sec-body, .sec-starred .card.recent, .sec-recent .card.recent, .sec-parked .sec-body";
const moreCards = (sc) => [...sc.children].filter((c) => !c.matches(".col-sub, button"));
// Where a card ends inside its list, by layout (the list is its offsetParent), not by what's on screen:
// cards that moved slide to their new place after a redraw, and mid-slide they're elsewhere.
const cardEnd = (c) => c.offsetTop + c.offsetHeight;
function markMore(trim = true, woke = moreWoke) {
  const lists = [...document.querySelectorAll(MORE_LISTS)].map((sc) => ({ sc, host: sc.closest(".sec") }));
  if (trim) {
    // Lists share their column: trimming one gives the next more room, and a section can spill past
    // the column's bottom. So: from scratch, then a few passes until nothing changes, each list ending
    // on its last whole card above its own edge and the column's (less room for "N more").
    for (const { sc } of lists) sc.style.maxHeight = "";
    for (let pass = 0; pass < 4; pass++) {
      let changed = false;
      for (const { sc, host } of lists) {
        const r = sc.getBoundingClientRect(), colBottom = sc.closest(".col").getBoundingClientRect().bottom;
        if (sc.scrollHeight - sc.clientHeight <= 4 && r.bottom <= colBottom + 1) continue;
        const room = Math.min(r.bottom, colBottom) - r.top;   // the list's own height it can use
        const whole = moreCards(sc).filter((c) => cardEnd(c) - sc.scrollTop <= room + 1).at(-1);
        const h = whole ? cardEnd(whole) - sc.scrollTop + 2 : 0;
        if (h > 40 && (!sc.style.maxHeight || h < parseFloat(sc.style.maxHeight) - 0.5)) { sc.style.maxHeight = `${h}px`; changed = true; }
      }
      if (!changed) break;
    }
  }
  for (const { sc, host } of lists) {
    let pill = host.querySelector(":scope > .more-below");
    const bottom = sc.getBoundingClientRect().bottom;
    const n = sc.scrollHeight - sc.scrollTop - sc.clientHeight > 4 ? moreCards(sc).filter((c) => cardEnd(c) > sc.scrollTop + sc.clientHeight + 1).length : 0;
    // A redraw makes a new pill each time: when it was shown is kept by list, not by element.
    const key = [...host.classList].find((c) => c.startsWith("sec-")) || host.className;
    if (!n) { pill?.remove(); clearTimeout(moreShown.get(key)?.timer); moreShown.delete(key); continue; }
    if (!pill) {
      pill = document.createElement("button");
      pill.className = "more-below";
      host.append(pill);
    }
    pill.list = sc;
    const label = `${n} more ↓`;
    let shown = moreShown.get(key);
    // Scrolled: by you, to somewhere else. A redraw puts the list back where it was, which fires a scroll
    // too; that one isn't a move, or the pill would never fade while sessions work (a redraw a second).
    const moved = woke.has(sc) && Math.abs(sc.scrollTop - (shown?.top ?? sc.scrollTop)) > 1;
    if (!shown || shown.label !== label || moved) {
      clearTimeout(shown?.timer);
      shown = { label, until: Date.now() + MORE_SHOWN_MS, timer: setTimeout(() => markMore(false), MORE_SHOWN_MS + 20) };
      moreShown.set(key, shown);
    }
    shown.top = sc.scrollTop;
    pill.classList.toggle("quiet", Date.now() >= shown.until);
    pill.textContent = label;
    pill.style.top = `${bottom - host.getBoundingClientRect().top - 10}px`;   // on the last card's bottom edge
  }
  woke.clear();
}
// Each list's pill: what it said, until when it shows (then it fades, until the list scrolls or the count
// changes), and where its list was scrolled to when last looked at (`top`).
const MORE_SHOWN_MS = 4000;
const moreShown = new Map();
// Scrolling a list changes what's below it: once a frame at most, for the lists that scrolled (`moreWoke`).
let moreFrame = 0;
const moreWoke = new Set();

// ---------- header chips: what's waiting, what's asking, what's going cold; click to open the oldest ----------
/** Each chip's list, oldest first (going cold: soonest first): what a click steps through. */
function chipList(kind) {
  const { yours, decide } = groups();
  if (kind === "waiting") return yours.map((i) => ({ id: i.id, sid: i.session_id, project: i.project, since: i.created_ms }));
  if (kind === "asking") return decide.map((i) => ({ id: i.id, sid: i.session_id, project: i.project, since: i.created_ms }));
  // Waiting on you with its cache counting down on its card (insights.js cacheWarn): the soonest to run out first.
  return [...yours, ...decide].map((i) => ({ id: i.id, sid: i.session_id, project: i.project, w: cacheWarn(i.session_id) }))
    .filter((x) => x.w && x.w.left > 0)
    .map((x) => ({ id: x.id, sid: x.sid, project: x.project, since: x.w.c.until_ms })).sort((a, b) => a.since - b.since);
}
function headerChips() {
  const chip = (kind, label, extra, cls, dot) => {
    const list = chipList(kind);
    if (!list.length) return "";
    // The chip you're stepping through: ‹ 2 of 3 waiting › (← → do the same).
    const at = chipNav === kind ? chipAt(list) : -1;
    if (at >= 0 && list.length > 1) return `<span class="hchip nav ${cls}"><button class="hstep" data-chip-step="${kind}:-1" title="Previous (←)" aria-label="Previous">‹</button><button class="hnav" data-chip="${kind}"><span class="hdot ${dot}"></span>${at + 1} of ${list.length}<span class="hlbl"> ${label}</span></button><button class="hstep" data-chip-step="${kind}:1" title="Next (→)" aria-label="Next">›</button></span>`;
    return `<button class="hchip ${cls}" data-chip="${kind}"><span class="hdot ${dot}"></span>${list.length}<span class="hlbl"> ${label}</span>${extra ? `<span class="hsub"> · ${extra}</span>` : ""}</button>`;
  };
  const waiting = chipList("waiting"), cold = chipList("cold");
  const oldest = waiting[0] ? now() - waiting[0].since : 0;
  // Stronger the longer the oldest has waited: soft under 5 minutes, then half, then solid at 15.
  const heat = oldest >= 15 * 60000 ? "hot" : oldest >= 5 * 60000 ? "warm" : "";
  const html = chip("waiting", "waiting", waiting[0] ? `oldest ${ago(waiting[0].since)}` : "", heat, "z")
    + chip("cold", "going cold", cold[0] ? `<span data-cold-in="${cold[0].since}">${coldIn(cold[0].since)}</span>` : "", "cold", "c")
    + chip("asking", "asking you", "", "", "ask");
  return html || `<span class="hall" data-tauri-drag-region>all clear</span>`;
}
/** Where the Active pane sits in a chip's list (-1 when it's showing something else). */
const chipAt = (list) => list.findIndex((x) => (x.id ? x.id === active?.id : x.sid === active?.sid && !isPending(findItem(active?.id))));
/** A chip click opens the oldest of its kind; clicking again (or ›, →) steps to the next, ‹ or ← back; both wrap. */
function openFromChip(kind, dir = 1) {
  const list = chipList(kind);
  if (!list.length) return;
  const at = chipAt(list);
  const next = list[at < 0 ? (dir > 0 ? 0 : list.length - 1) : (at + dir + list.length) % list.length];
  chipNav = kind;
  slideDir = dir;
  setActive(next.id, next.sid);
}

// ---------- boot ----------
const LINGER_MS = 1500;
/** A "✓ answered" card's time is up: it fades and folds away, and the cards below close the gap. */
function leaveGhosts() {
  const gone = [...ghosts].filter(([, g]) => Date.now() - g.at >= LINGER_MS).map(([id]) => id);
  if (!gone.length) return;
  const finish = () => { folding = false; heldDraw = false; gone.forEach((id) => ghosts.delete(id)); render(); };
  const els = gone.map((id) => document.querySelector(`.nrow.ghost[data-detail="${CSS.escape(id)}"]`)).filter(Boolean);
  if (!els.length || matchMedia("(prefers-reduced-motion: reduce)").matches) return finish();
  // Fades out first, then closes up, eating the column's 10px gap too, so nothing below jumps at the end.
  // Redraws wait meanwhile (sessions at work redraw often): one mid-fold would pop the row back.
  folding = true;
  for (const el of els) {
    const cs = getComputedStyle(el), h = `${el.offsetHeight}px`;
    el.style.overflow = "hidden";
    el.animate([
      { height: h, opacity: 0.6, paddingTop: cs.paddingTop, paddingBottom: cs.paddingBottom, marginBottom: "0px", offset: 0 },
      { height: h, opacity: 0, paddingTop: cs.paddingTop, paddingBottom: cs.paddingBottom, marginBottom: "0px", offset: 0.4 },
      { height: "0px", opacity: 0, paddingTop: "0px", paddingBottom: "0px", marginBottom: "-10px", offset: 1 },
    ], { duration: 420, easing: "ease-in-out", fill: "forwards" });
  }
  setTimeout(finish, 430);
}
