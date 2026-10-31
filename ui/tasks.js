// ---------- The Tasks tab: what's left in a session ----------
// Off until you turn it on for a session. Then a small model reads the session's chat and lists what's left, and
// after each of its turns reads what was said since and edits the list (tasks.rs). The window's state carries
// every tracked session's list (state.tasks); the tab shows it grouped, what's done or called off struck through,
// and the cards say "N left". It's only read: to change what's left you tell the session, and the next read takes
// it from the chat. The session never sees the list.
const TASK_GROUPS = [["ok", "WAITING ON YOUR OK"], ["decide", "TO DECIDE"], ["check", "TO CHECK"], ["build", "TO BUILD"]];
const tasksTabOn = new Set();     // sessions showing the Tasks tab

const tasksOf = (sid) => state.tasks?.[sid];
const tasksLeft = (t) => (t?.items || []).filter((i) => i.state === "open").length;

/** The tab's button: how many are left once it's tracked, a live dot while the list is being read. */
function tasksTabBtn(sid) {
  const t = tasksOf(sid);
  const n = t ? `<span class="stab-n">${t.busy ? `<span class="dot-live"></span>` : ""}${tasksLeft(t)} left</span>` : "";
  return `<button class="stab ${tasksTabOn.has(sid) ? "on" : ""}" data-tasks-tab data-sid="${esc(sid)}">Tasks${n}</button>`;
}
/** Where the tab's page goes, in place of the chat (extensions.js placeExt hides the chat under it). */
const tasksSlot = (sid) => (sid && tasksTabOn.has(sid) ? `<div class="ac-slot" data-tasks-slot>${tasksPage(sid)}</div>` : "");
/** On a session's card: "N left" for a tracked session with any; a click opens its Tasks tab. */
function tasksPill(s) {
  const sid = s?.session_id, n = sid ? tasksLeft(tasksOf(sid)) : 0;
  return n ? `<button class="bg-pill" data-tasks-open="${esc(sid)}">${n} left</button>` : "";
}

const tasksMoney = (usd) => (usd > 0 && usd < 0.01 ? "under 1¢" : `$${(usd || 0).toFixed(2)}`);

function tasksPage(sid) {
  const t = tasksOf(sid);
  if (!t) {
    return `<div class="tk-page"><div class="tk-intro">
      <p>Keep a list of what's left in this session: what you still have to check, what's still to build, what you have to decide, and what waits on your OK.</p>
      <p class="dim">A small model reads this session's chat and lists it, then reads each new turn and updates the list. About a cent to start and about a cent a turn. The session never sees it.</p>
      <button class="btn primary" data-tasks-do="start" data-sid="${esc(sid)}">Track what's left</button></div></div>`;
  }
  const items = t.items || [];
  if (!items.length && t.busy) return `<div class="tk-page"><div class="ac-empty">Reading the chat for what's left…</div></div>`;
  // Each group: what's left, then what's done or called off, struck through.
  const groups = TASK_GROUPS.map(([g, head]) => {
    const rows = items.filter((i) => i.group === g), left = rows.filter((i) => i.state === "open");
    if (!rows.length) return "";
    return `<div class="tk-head">${head} · ${left.length ? `${left.length} left` : "all done"}</div>${[...left, ...rows.filter((i) => i.state !== "open")].map(tasksRow).join("")}`;
  }).join("");
  const now = t.busy ? `Reading ${t.busy === "listing" ? "the chat" : "the last turn"}…` : t.err ? `Couldn't update: ${t.err}` : t.said ? `Last turn: ${t.said}` : "";
  return `<div class="tk-page">
    ${groups || `<div class="ac-empty">Nothing left, as far as the chat says.</div>`}
    <div class="tk-note dim">Read from the chat after each turn. To change what's left, tell the session.</div>
    <div class="tk-foot">${now ? `<span class="${t.err && !t.busy ? "tk-err" : ""}">${esc(now)}</span>` : ""}<span class="grow"></span>
      <span class="dim">so far ${esc(helperTokens(t.spent?.tokens || 0) || "0 tokens")} · ${tasksMoney(t.spent?.usd)}</span>
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
  const cmd = { start: "tasks_start", refresh: "tasks_refresh", stop: "tasks_stop" }[what];
  if (cmd) invoke(cmd, { sessionId: sid }).catch((x) => toast(String(x)));
}, true);
