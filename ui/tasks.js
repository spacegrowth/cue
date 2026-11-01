// ---------- The Tasks tab: what's left in a session ----------
// Off until you turn it on for a session. Then a small model reads the session's chat and lists what's left, and
// after each of its turns reads what was said since and edits the list (tasks.rs). The window's state carries
// every tracked session's list (state.tasks); the tab shows it grouped, what's done struck through, what went off the list hidden,
// and the cards say "N left". The list itself isn't edited: to change what's left you tell the session, and the
// next read takes it from the chat. The session never sees the list. Ticking items still left gives two ways to act
// on them: Finish these sends the session a message asking it to finish them, and Ask about these puts a request to
// explain them in its box, for you to add to and send.
const TASK_GROUPS = [["ok", "WAITING ON YOUR OK"], ["decide", "TO DECIDE"], ["check", "TO CHECK"], ["build", "TO BUILD"]];
const tasksTabOn = new Set();     // sessions showing the Tasks tab, or its About tab (tasksAboutOn)
const tasksAboutOn = new Set();   // of those, the ones on About: what it's about, where it stands, a line per turn
const tasksTurnsOpen = new Set(); // sessions showing every turn, not just the newest 6
const tasksPicked = new Map();    // sid -> Set of ticked item ids (open items only)
const tasksOpenRows = new Set();  // "sid|id" of items showing what they are (their detail)
const tasksDoneOn = new Set();    // sessions showing what's done (struck through) under what's left

const tasksOf = (sid) => state.tasks?.[sid];
/** Tasks are on (Settings → Tasks): off, the window shows none of it, tracked or not. */
const tasksShown = () => state.settings?.tasks?.on === true;
/** The session's track while Tasks are on; null otherwise. */
const tasksTracked = (sid) => (tasksShown() ? tasksOf(sid) || null : null);
const tasksLeft = (t) => (t?.items || []).filter((i) => i.state === "open").length;

/** The tab's button: how many are left once it's tracked, a live dot while the list is being read. */
function tasksTabBtn(sid) {
  if (!tasksShown()) return "";
  const t = tasksOf(sid);
  const n = t ? `<span class="stab-n">${t.busy ? `<span class="dot-live"></span>` : ""}${tasksLeft(t)} left</span>` : "";
  return `<button class="stab ${tasksTabOn.has(sid) && !tasksAboutOn.has(sid) ? "on" : ""}" data-tasks-tab data-sid="${esc(sid)}">Tasks${n}</button>`;
}
/** The About tab, last in the row: a tracked session's About, Now and turns, apart from the list. */
function tasksAboutBtn(sid) {
  if (!tasksTracked(sid)) return "";
  return `<button class="stab ${tasksTabOn.has(sid) && tasksAboutOn.has(sid) ? "on" : ""}" data-tasks-tab data-about="1" data-sid="${esc(sid)}">About</button>`;
}
/** Where the tab's page goes, in place of the chat (extensions.js placeExt hides the chat under it). */
const tasksSlot = (sid) => (sid && tasksShown() && tasksTabOn.has(sid) ? `<div class="ac-slot" data-tasks-slot>${tasksAboutOn.has(sid) && tasksTracked(sid) ? tasksAboutPage(sid) : tasksPage(sid)}</div>` : "");
/** On a session's card: "N left" for a tracked session with any; a click opens its Tasks tab. */
function tasksPill(s) {
  const sid = s?.session_id, n = sid ? tasksLeft(tasksTracked(sid)) : 0;
  return n ? `<button class="bg-pill" data-tasks-open="${esc(sid)}">${n} left</button>` : "";
}

/** D, on a card: what a tracked session is about (bold) and where it stands, in place of its last reply's opening;
 *  "" when it has neither yet. */
function tasksCardText(sid) {
  const t = tasksTracked(sid);
  if (!t?.about && !t?.now) return "";
  return `<div class="tk-card">${t.about ? `<div class="tk-about">${esc(t.about)}</div>` : ""}${t.now ? `<div class="tk-now">${esc(t.now)}</div>` : ""}</div>`;
}

/** When you last looked at a session's chat before this look (chat.js catchUp), 0 if Cue doesn't know. */
const tasksLookedAt = (sid) => (catchUp?.sid === sid ? catchUp.since : looked[sid]) || 0;

/** D, in the chat: for a tracked session you've come back to (Catch me up's rule: 15 minutes or more away, with
 *  replies since), a divider where the new part starts, a line per turn since then. { at, html }, or null. */
function tasksSince(sid, ex) {
  const t = tasksTracked(sid), c = catchUp;
  if (!t || !c || c.sid !== sid || !c.since || c.openedAt - c.since < CATCH_UP_AFTER) return null;
  if (ex.some((e) => e.role === "you" && e.at_ms > c.openedAt)) return null;
  const turns = (t.turns || []).filter((u) => u.at_ms > c.since && u.at_ms <= c.openedAt);
  if (!turns.length && !ex.some((e) => e.role === "agent" && e.at_ms > c.since && e.at_ms <= c.openedAt)) return null;
  const rows = turns.map((u) => `<div class="tk-turn"><span class="tk-t">${esc(tasksClock(u.at_ms))}</span><span>${esc(u.line)}</span></div>`).join("");
  return { at: c.since + 1, html: `<div class="tk-since"><div class="tk-div"><i></i>${esc(ago(c.since).toUpperCase())} SINCE YOU LOOKED${turns.length ? ` · ${turns.length} TURN${turns.length === 1 ? "" : "S"}` : ""}<i></i></div>${rows}${rows ? `<div class="tk-div"><i></i></div>` : ""}</div>` };
}
const tasksClock = (ms) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

const tasksMoney = (usd) => (usd > 0 && usd < 0.01 ? "under 1¢" : `$${(usd || 0).toFixed(2)}`);

function tasksPage(sid) {
  const t = tasksOf(sid);
  if (!t) {
    return `<div class="tk-page"><div class="tk-intro">
      <p>A list of what's left in this session: what you still have to check, what's still to build, what you have to decide, and what waits on your OK.</p>
      <p class="dim">Make a list sends this session's chat to a small model (Haiku), which writes the list. After each turn Cue sends it the new messages and it updates the list. The session never sees this. What it costs shows here.</p>
      <button class="btn primary" data-tasks-do="start" data-sid="${esc(sid)}">Make a list</button></div></div>`;
  }
  const items = t.items || [];
  if (!items.length && t.busy) return `<div class="tk-page"><div class="ac-empty">Reading the chat for what's left…</div></div>`;
  // Each group: what's left; what's done, struck through, only with "Show N done". What went off the list (answered,
  // said again, moot) isn't shown.
  const showDone = tasksDoneOn.has(sid), nDone = items.filter((i) => i.state === "done").length;
  const groups = TASK_GROUPS.map(([g, head]) => {
    const left = items.filter((i) => i.group === g && i.state === "open"), done = showDone ? items.filter((i) => i.group === g && i.state === "done") : [];
    if (!left.length && !done.length) return "";
    return `<div class="tk-head">${tasksTag(g, head)}${left.length ? `${left.length} left` : "all done"}</div>${[...left, ...done].map((i) => tasksRow(sid, i)).join("")}`;
  }).join("");
  const doneBtn = nDone ? `<button class="tk-more" data-tasks-do="done" data-sid="${esc(sid)}">${showDone ? "Hide what's done" : `Show ${nDone} done`}</button>` : "";
  const picked = tasksPicks(sid, items);
  const pickBar = picked.length ? `<div class="tk-pick"><span class="tk-pick-n">${picked.length} ticked</span><span class="grow"></span>
      <button class="btn" data-tasks-do="unpick" data-sid="${esc(sid)}">Clear</button>
      <button class="btn" data-tasks-do="ask" data-sid="${esc(sid)}">Ask about these</button>
      <button class="btn primary" data-tasks-do="finish" data-sid="${esc(sid)}">Finish these</button></div>` : "";
  const now = t.busy ? `Reading ${t.busy === "listing" ? "the chat" : "the last turn"}…` : t.err ? `Couldn't update: ${t.err}` : t.said ? `Last turn: ${t.said}` : "";
  return `<div class="tk-page">${groups ? `<div class="tk-head tk-left">WHAT'S LEFT · ${tasksLeft(t)}</div>` : ""}
    ${groups || `<div class="ac-empty">Nothing left, as far as the chat says.</div>`}${doneBtn}
    <div class="tk-note dim">Written by a small model from the chat, after each turn. To change what's left, tell the session${tasksLeft(t) ? `, or tick some and ask it to finish them` : ""}.</div>
    <div class="tk-dock">${pickBar}<div class="tk-foot"><span class="tk-said ${t.err && !t.busy ? "tk-err" : ""}">${esc(now)}</span>
      <span class="dim">So far: ${tasksMoney(t.spent?.usd)} · ${esc(helperTokens(t.spent?.tokens || 0) || "0 tokens")}</span>
      <button class="btn" data-tasks-do="refresh" data-sid="${esc(sid)}" ${t.busy ? "disabled" : ""}>Refresh</button>
      <button class="btn" data-tasks-do="stop" data-sid="${esc(sid)}">Remove the list</button></div></div></div>`;
}

/** The About tab: what the session is about, where it stands now, and a line per turn (newest first, the ones since
 *  you looked in bold; 12, then "N earlier"). The same small model's reading as the list, after each turn. */
function tasksAboutPage(sid) {
  const t = tasksOf(sid), seen = tasksLookedAt(sid), turns = [...(t.turns || [])].reverse();
  const shown = tasksTurnsOpen.has(sid) ? turns : turns.slice(0, 12);
  const unseen = turns.filter((u) => seen && u.at_ms > seen).length;
  return `<div class="tk-page tk-aboutpage">${t.about ? `<div class="tk-about big">${esc(t.about)}</div>` : ""}
    ${t.now ? `<div class="tk-head">NOW</div><div class="tk-nowline">${esc(t.now)}</div>` : ""}
    ${turns.length ? `<div class="tk-head">TURNS${unseen ? ` · ${unseen} SINCE YOU LOOKED` : ""}</div>${shown.map((u) => `<div class="tk-turn ${seen && u.at_ms > seen ? "new" : ""}"><span class="tk-t">${esc(tasksClock(u.at_ms))}</span><span>${esc(u.line)}</span></div>`).join("")}${turns.length > shown.length ? `<button class="tk-more" data-tasks-do="turns" data-sid="${esc(sid)}">${turns.length - shown.length} earlier</button>` : ""}` : ""}
    ${!t.about && !t.now && !turns.length ? `<div class="ac-empty">${t.busy ? "Reading the chat…" : "Nothing yet: it fills in after the next turn."}</div>` : ""}</div>`;
}

/** A group's name as a tag: the two that wait on you (your OK, a decision) in the accent, the others plain. */
const tasksTag = (g, head) => `<span class="tk-tag ${g === "ok" || g === "decide" ? "you" : ""}">${esc(head)}</span>`;

/** One item: while it's left, a box to tick and its line, which opens what it is (the small model's detail; with
 *  none yet, the line ticks it too); done (✓) struck through. */
function tasksRow(sid, i) {
  if (i.state !== "open") return `<div class="tk-row over"><span class="tk-mk ${i.state}" aria-hidden="true"></span><span class="tk-text">${esc(i.text)}</span></div>`;
  const on = tasksPicked.get(sid)?.has(i.id), shown = i.detail && tasksOpenRows.has(`${sid}|${i.id}`);
  const box = `<button class="tk-box" data-tasks-pick="${esc(i.id)}" data-sid="${esc(sid)}" role="checkbox" aria-checked="${on ? "true" : "false"}" aria-label="Tick"></button>`;
  const line = i.detail
    ? `<button class="tk-line" data-tasks-more="${esc(i.id)}" data-sid="${esc(sid)}" aria-expanded="${shown ? "true" : "false"}"><span class="tk-text">${esc(i.text)}</span><span class="tk-chev" aria-hidden="true">${shown ? "▾" : "▸"}</span></button>`
    : `<button class="tk-line" data-tasks-pick="${esc(i.id)}" data-sid="${esc(sid)}"><span class="tk-text">${esc(i.text)}</span></button>`;
  return `<div class="tk-row tk-open ${on ? "on" : ""}">${box}<div class="tk-body">${line}${shown ? `<div class="tk-detail">${esc(i.detail)}</div>` : ""}</div></div>`;
}

/** The ticked items still left, in the list's order (one done or dropped since you ticked it falls away). */
function tasksPicks(sid, items = tasksOf(sid)?.items || []) {
  const set = tasksPicked.get(sid);
  if (!set?.size) return [];
  const open = items.filter((i) => i.state === "open");
  for (const id of set) if (!open.some((i) => i.id === id)) set.delete(id);
  return open.filter((i) => set.has(i.id));
}
const tasksBullets = (picks) => picks.map((i) => `- ${i.text}`).join("\n");

/** Text into the session's box, after what's typed there. */
function tasksIntoBox(sid, text) {
  const d = draft(`s:${sid}`);
  d.text = d.text.trim() ? `${d.text.replace(/\s+$/, "")}\n\n${text}` : text;
  saveDrafts();
}
/** Finish these: sent to the session as it is (queued while it works, as the box's Send does), what you've typed
 *  in its box left alone; the chat comes back to show it. Not sent (it's asking you something, it's restarting,
 *  sending failed): the message goes into the box instead, so nothing is lost. */
async function tasksFinish(sid) {
  const picks = tasksPicks(sid);
  if (!picks.length) return;
  const key = `tasks:${sid}`;
  drafts[key] = { text: `Please finish ${picks.length === 1 ? "this" : "these"}:\n${tasksBullets(picks)}`, choices: {}, images: [] };
  tasksPicked.delete(sid);
  tasksTabOn.delete(sid);
  renderMain();
  await sendTo(sid, false, key);
  const left = drafts[key]?.text;
  delete drafts[key];
  if (left?.trim()) { tasksIntoBox(sid, left); renderMain(); focusComposer(); }
}
/** Ask about these: a request to explain them into the box (after what's typed there), for you to add to and send. */
function tasksAsk(sid) {
  const picks = tasksPicks(sid);
  if (!picks.length) return;
  const one = picks.length === 1;
  tasksIntoBox(sid, `Explain ${one ? "this" : "these"} in plain words: what ${one ? "it is" : "each is"}, where it stands, and what you need from me.\n${tasksBullets(picks)}\n\n`);
  tasksPicked.delete(sid);
  tasksTabOn.delete(sid);
  renderMain();
  focusComposer();
}

/** Pointing at a card's "N left": a card under it with what it's about and what's left, by group (only what's left;
 *  a click on the pill opens the whole Tasks tab). Anything else pointed at puts it away, so a redraw can't strand it. */
const tasksPeekEl = Object.assign(document.createElement("div"), { className: "tk-peek" });
document.body.append(tasksPeekEl);
let tasksPeekFor = null, tasksPeekWait = 0;
function tasksPeekHide() { clearTimeout(tasksPeekWait); tasksPeekFor = null; tasksPeekEl.classList.remove("show"); }
function tasksPeekShow(pill) {
  const sid = pill.dataset.tasksOpen, t = tasksTracked(sid);
  const open = (t?.items || []).filter((i) => i.state === "open");
  if (!open.length) return tasksPeekHide();
  const groups = TASK_GROUPS.map(([g, head]) => {
    const rows = open.filter((i) => i.group === g);
    return rows.length ? `<div class="tk-peek-g">${tasksTag(g, head)}</div>${rows.map((i) => `<div class="tk-peek-r"><span class="tk-mk open" aria-hidden="true"></span><span>${esc(i.text)}</span></div>`).join("")}` : "";
  }).join("");
  tasksPeekEl.innerHTML = `${t.about ? `<div class="tk-about">${esc(t.about)}</div>` : ""}${groups}<div class="tk-peek-foot">Click for the whole list</div>`;
  tasksPeekEl.classList.add("show");
  const r = pill.getBoundingClientRect(), w = tasksPeekEl.offsetWidth, h = tasksPeekEl.offsetHeight;
  const below = r.bottom + 8 + h < innerHeight;
  tasksPeekEl.style.left = `${Math.min(Math.max(8, r.left + r.width / 2 - w / 2), innerWidth - w - 8)}px`;
  tasksPeekEl.style.top = `${below ? r.bottom + 6 : Math.max(8, r.top - h - 6)}px`;
}
document.addEventListener("mouseover", (e) => {
  const pill = e.target.closest?.("[data-tasks-open]");
  if (!pill) return tasksPeekHide();
  if (tasksPeekFor === pill.dataset.tasksOpen) return;
  tasksPeekHide();
  tasksPeekFor = pill.dataset.tasksOpen;
  tasksPeekWait = setTimeout(() => pill.isConnected && tasksPeekShow(pill), 250);
});
document.addEventListener("mouseleave", tasksPeekHide);

/** Open a session on its Tasks tab, from anywhere (a card's "N left"). */
function tasksOpen(sid) {
  actTabOn.delete(sid);
  bgTabOn.delete(sid);
  extTabOf.delete(sid);
  tasksTabOn.add(sid);
  tasksAboutOn.delete(sid);
}

document.addEventListener("click", (e) => {
  const tab = e.target.closest?.("[data-tasks-tab]");
  if (tab) {
    e.stopPropagation();
    tasksOpen(tab.dataset.sid);
    tab.dataset.about ? tasksAboutOn.add(tab.dataset.sid) : tasksAboutOn.delete(tab.dataset.sid);
    helperView = null;
    return renderMain();
  }
  const pill = e.target.closest?.("[data-tasks-open]");
  if (pill) {
    e.stopPropagation();
    tasksPeekHide();
    const sid = pill.dataset.tasksOpen, it = pill.closest("[data-big]")?.dataset.big;
    tasksOpen(sid);
    return it ? setActive(it) : openSession(sid);
  }
  const pk = e.target.closest?.("[data-tasks-pick]");
  if (pk) {
    e.stopPropagation();
    const sid = pk.dataset.sid, id = pk.dataset.tasksPick;
    const set = tasksPicked.get(sid) || tasksPicked.set(sid, new Set()).get(sid);
    set.has(id) ? set.delete(id) : set.add(id);
    return renderMain();
  }
  const more = e.target.closest?.("[data-tasks-more]");
  if (more) {
    e.stopPropagation();
    const k = `${more.dataset.sid}|${more.dataset.tasksMore}`;
    tasksOpenRows.has(k) ? tasksOpenRows.delete(k) : tasksOpenRows.add(k);
    return renderMain();
  }
  const d = e.target.closest?.("[data-tasks-do]");
  if (!d) return;
  e.stopPropagation();
  const sid = d.dataset.sid, what = d.dataset.tasksDo;
  if (what === "turns") { tasksTurnsOpen.add(sid); return renderMain(); }
  if (what === "done") { tasksDoneOn.has(sid) ? tasksDoneOn.delete(sid) : tasksDoneOn.add(sid); return renderMain(); }
  if (what === "unpick") { tasksPicked.delete(sid); return renderMain(); }
  if (what === "finish") return tasksFinish(sid);
  if (what === "ask") return tasksAsk(sid);
  const cmd = { start: "tasks_start", refresh: "tasks_refresh", stop: "tasks_stop" }[what];
  if (cmd) invoke(cmd, { sessionId: sid }).catch((x) => toast(String(x)));
}, true);
