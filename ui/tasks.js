// ---------- The Tasks tab: what's left in a session ----------
// Off until you turn it on for a session. Then a small model reads the session's chat and lists what's left, and
// after each of its turns reads what was said since and edits the list (tasks.rs). The window's state carries
// every tracked session's list (state.tasks); the tab shows it grouped, what's done or called off struck through,
// and the cards say "N left". It's only read: to change what's left you tell the session, and the next read takes
// it from the chat. The session never sees the list.
const TASK_GROUPS = [["ok", "WAITING ON YOUR OK"], ["decide", "TO DECIDE"], ["check", "TO CHECK"], ["build", "TO BUILD"]];
const tasksTabOn = new Set();     // sessions showing the Tasks tab
const tasksTurnsOpen = new Set(); // sessions showing every turn, not just the newest 6

const tasksOf = (sid) => state.tasks?.[sid];
/** Tasks are on (Settings → Chat → Tasks): off, the window shows none of it, tracked or not. */
const tasksShown = () => state.settings?.tasks?.on === true;
/** The session's track while Tasks are on; null otherwise. */
const tasksTracked = (sid) => (tasksShown() ? tasksOf(sid) || null : null);
const tasksLeft = (t) => (t?.items || []).filter((i) => i.state === "open").length;

/** The tab's button: how many are left once it's tracked, a live dot while the list is being read. */
function tasksTabBtn(sid) {
  if (!tasksShown()) return "";
  const t = tasksOf(sid);
  const n = t ? `<span class="stab-n">${t.busy ? `<span class="dot-live"></span>` : ""}${tasksLeft(t)} left</span>` : "";
  return `<button class="stab ${tasksTabOn.has(sid) ? "on" : ""}" data-tasks-tab data-sid="${esc(sid)}">Tasks${n}</button>`;
}
/** Where the tab's page goes, in place of the chat (extensions.js placeExt hides the chat under it). */
const tasksSlot = (sid) => (sid && tasksShown() && tasksTabOn.has(sid) ? `<div class="ac-slot" data-tasks-slot>${tasksPage(sid)}</div>` : "");
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
      <p>Keep a list of what's left in this session: what you still have to check, what's still to build, what you have to decide, and what waits on your OK.</p>
      <p class="dim">A small model reads this session's chat and lists it, then reads each new turn and updates the list. The session never sees it. What it costs shows here.</p>
      <button class="btn primary" data-tasks-do="start" data-sid="${esc(sid)}">Track what's left</button></div></div>`;
  }
  const items = t.items || [];
  const seen = tasksLookedAt(sid), turns = [...(t.turns || [])].reverse();
  const shownTurns = tasksTurnsOpen.has(sid) ? turns : turns.slice(0, 6);
  const head = t.about || t.now ? `<div class="tk-top">${t.about ? `<div class="tk-about big">${esc(t.about)}</div>` : ""}${t.now ? `<div class="tk-nowline"><span class="tk-lbl">NOW</span>${esc(t.now)}</div>` : ""}</div>` : "";
  const unseen = turns.filter((u) => seen && u.at_ms > seen).length;
  const turnsHtml = turns.length ? `<div class="tk-head">TURNS${unseen ? ` · ${unseen} SINCE YOU LOOKED` : ""}</div>${shownTurns.map((u) => `<div class="tk-turn ${seen && u.at_ms > seen ? "new" : ""}"><span class="tk-t">${esc(tasksClock(u.at_ms))}</span><span>${esc(u.line)}</span></div>`).join("")}${turns.length > shownTurns.length ? `<button class="tk-more" data-tasks-do="turns" data-sid="${esc(sid)}">${turns.length - shownTurns.length} earlier</button>` : ""}` : "";
  if (!items.length && t.busy) return `<div class="tk-page"><div class="ac-empty">Reading the chat for what's left…</div></div>`;
  // Each group: what's left, then what's done or called off, struck through.
  const groups = TASK_GROUPS.map(([g, head]) => {
    const rows = items.filter((i) => i.group === g), left = rows.filter((i) => i.state === "open");
    if (!rows.length) return "";
    return `<div class="tk-head">${head} · ${left.length ? `${left.length} left` : "all done"}</div>${[...left, ...rows.filter((i) => i.state !== "open")].map(tasksRow).join("")}`;
  }).join("");
  const now = t.busy ? `Reading ${t.busy === "listing" ? "the chat" : "the last turn"}…` : t.err ? `Couldn't update: ${t.err}` : t.said ? `Last turn: ${t.said}` : "";
  return `<div class="tk-page">${head}${turnsHtml}${groups ? `<div class="tk-head tk-left">WHAT'S LEFT · ${tasksLeft(t)}</div>` : ""}
    ${groups || `<div class="ac-empty">Nothing left, as far as the chat says.</div>`}
    <div class="tk-note dim">Read from the chat after each turn. To change what's left, tell the session.</div>
    <div class="tk-foot">${now ? `<span class="${t.err && !t.busy ? "tk-err" : ""}">${esc(now)}</span>` : ""}<span class="grow"></span>
      <span class="dim">This session so far: ${tasksMoney(t.spent?.usd)} · ${esc(helperTokens(t.spent?.tokens || 0) || "0 tokens")}</span>
      <button class="btn" data-tasks-do="refresh" data-sid="${esc(sid)}" ${t.busy ? "disabled" : ""}>Refresh</button>
      <button class="btn" data-tasks-do="stop" data-sid="${esc(sid)}">Stop tracking</button></div></div>`;
}

/** One item: a dot while it's left; done (✓) or called off (–) struck through. Only marks: nothing to click. */
function tasksRow(i) {
  const over = i.state !== "open";
  return `<div class="tk-row ${over ? "over" : ""}"><span class="tk-mk ${i.state}" aria-hidden="true"></span><span class="tk-text">${esc(i.text)}</span>${i.state === "dropped" ? `<span class="tk-why">called off</span>` : ""}</div>`;
}

/** Open a session on its Tasks tab, from anywhere (a card's "N left"). */
function tasksOpen(sid) {
  actTabOn.delete(sid);
  bgTabOn.delete(sid);
  extTabOf.delete(sid);
  tasksTabOn.add(sid);
}

document.addEventListener("click", (e) => {
  const tab = e.target.closest?.("[data-tasks-tab]");
  if (tab) {
    e.stopPropagation();
    tasksOpen(tab.dataset.sid);
    helperView = null;
    return renderMain();
  }
  const pill = e.target.closest?.("[data-tasks-open]");
  if (pill) {
    e.stopPropagation();
    const sid = pill.dataset.tasksOpen, it = pill.closest("[data-big]")?.dataset.big;
    tasksOpen(sid);
    return it ? setActive(it) : openSession(sid);
  }
  const d = e.target.closest?.("[data-tasks-do]");
  if (!d) return;
  e.stopPropagation();
  const sid = d.dataset.sid, what = d.dataset.tasksDo;
  if (what === "turns") { tasksTurnsOpen.add(sid); return renderMain(); }
  const cmd = { start: "tasks_start", refresh: "tasks_refresh", stop: "tasks_stop" }[what];
  if (cmd) invoke(cmd, { sessionId: sid }).catch((x) => toast(String(x)));
}, true);
