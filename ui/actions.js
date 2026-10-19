// Cue UI — actions: a project's own buttons for its developer commands (install, test, clean a cache, a dev
// server). They belong to the project (its folder, and its machine for a remote session), not to a session:
// they run any time, an agent working or not. Cue runs each in tmux (actions.rs), so a server it started is
// never lost. A project has a few (two or three, in its .cue/actions.json and .cue/actions.local.json). Where
// they show: as buttons under the agent's latest reply, ⌘E (a palette over the window; ⌘⇧E runs the last one
// again), dots on the project chips in the header for what's running (a click: their popover), and the Actions
// tab in the open session's tab row (every one, to run and edit). A command from a file runs only once you've
// allowed it (the trust sheet), and again whenever it changes.
const actLists = new Map();     // `${machine}|${folder}` → { at, data, loading, err }: actions_list
let actRunning = [], actRunningAt = 0;   // actions_running(): what runs now in every project (the chips' dots)
const actTails = new Map();     // `${key}|${id}|${at}` → the last lines of a failed run, under its row
const actQueued = new Map();    // session id → [{ pl, id, log }]: "After this turn", run when its turn ends
const actTabOn = new Set();     // sessions showing the Actions tab
let actPal = null;              // ⌘E: { pl, sel, shown } while open
let actPop = null;              // a chip's dots clicked: { project, where }
let actEdit = null;             // the edit sheet: { pl, oldId, keep, ask, scope, copied }
let actTrust = null;            // the trust sheet: { pl, then: { id, log } | null, changed }
let actAsk = null;              // "Run now / After this turn": { pl, id, log, sid }
let actLogView = null;          // the output sheet: { pl, id, name, text, pinned, top }
let actLast = null;             // the last run, for ⌘⇧E: { pl, id }
let actPromptInit = false;      // Settings → Actions has put the prompt in its box
const ACT_FRESH = 10000, ACT_FRESH_REMOTE = 60000;
/** Cue's own prompt, shown when no project has been read yet (the backend keeps the one in use). */
const ACT_PROMPT = "Look at this project and set up its Cue actions in .cue/actions.local.json (only for me, not shared with the repo). Read the \"about\" notes at the top of the file first and follow them (if the file doesn't exist yet, create it with those notes). Keep the actions already there unless they're wrong. Then tell me what you added, changed or removed, and why.";
const ACT_SOURCES = [["shared", "THIS PROJECT", ".cue/actions.json · shared with the repo"], ["local", "MINE", ".cue/actions.local.json · only you"]];

// ---------- where: a project's actions are in a checkout (a folder, and a machine) ----------
const actKey = (pl) => `${pl.machine || ""}|${pl.folder}`;
/** A session's place: the top of its checkout on this Mac (a session in web-app/ui has web-app's
 *  actions; a worktree its own), its folder on another machine, and its project as Cue's state says it. */
const actFolder = (cwd, machine = "") => (!machine && state.projects?.[cwd]?.top) || cwd;
const actPlace = (x) => ({ folder: actFolder(x.cwd, x.machine), machine: x.machine || "", project: x.project || projOf(x.cwd), sid: x.session_id || "" });
const acAttrs = (pl, id) => `data-f="${esc(pl.folder)}" data-m="${esc(pl.machine)}"${id ? ` data-id="${esc(id)}"` : ""}`;
/** The project of the open session, else of the project picked at the top. */
function actHere() {
  const sid = openSid();
  const x = sid && (sessionOf(sid) || (state.live || []).find((q) => q.session_id === sid) || state.items.find((i) => i.session_id === sid));
  if (x?.cwd) return actPlace(x);
  return projFilter ? actOfProject(projFilter) : null;
}
/** A project by name (a header chip): the open session's checkout when it's in it, else a session's, or
 *  where something runs there. */
function actOfProject(p) {
  const here = actHere();
  if (here?.project === p) return here;
  const s = state.sessions.find((x) => x.project === p && x.cwd);
  if (s) return actPlace(s);
  const r = actRunning.find((x) => x.project === p);
  return r ? { folder: r.folder, machine: r.machine || "", project: p, sid: "" } : null;
}
/** A folder and machine as a place, its project named as its sessions name it. */
function actPlaceAt(folder = "", machine = "") {
  const s = state.sessions.find((x) => x.cwd && actFolder(x.cwd, x.machine) === folder && (x.machine || "") === machine);
  return { folder, machine, project: s?.project || actRunning.find((r) => r.folder === folder)?.project || baseName(folder), sid: s?.session_id || "" };
}
/** …from a clicked element's data-f / data-m. */
const actPlaceOf = (el) => actPlaceAt(el.dataset.f, el.dataset.m);
/** The session to talk to about a project: the open one when it's there, else any in that folder. */
function actSessionOn(pl) {
  const same = (x) => x?.cwd && actFolder(x.cwd, x.machine) === pl.folder && (x.machine || "") === pl.machine;
  const open = sessionOf(openSid());
  if (same(open)) return open;
  return state.sessions.find(same) || (pl.sid && sessionOf(pl.sid)) || null;
}
/** A session in that folder mid-turn (an "ask" action then asks first). */
const actWorking = (pl) => state.sessions.find((x) => x.cwd && actFolder(x.cwd, x.machine) === pl.folder && (x.machine || "") === pl.machine && x.state === "working") || null;

// ---------- what Cue knows: the list, and what runs ----------
/** A project's actions (actions_list), read when first wanted and again when stale; null until in. */
function actData(pl, force = false, why = "") {
  if (!pl?.folder) return null;
  const k = actKey(pl);
  let c = actLists.get(k);
  if (!c) actLists.set(k, (c = { at: 0, data: null, loading: false, err: "" }));
  if (!c.loading && (force || Date.now() - c.at > (pl.machine ? ACT_FRESH_REMOTE : ACT_FRESH))) {
    c.loading = true;
    invoke("actions_list", { folder: pl.folder, machine: pl.machine }).then((v) => { c.loading = false; c.at = Date.now(); c.err = ""; actTake(pl, v, why); }, (x) => { c.loading = false; c.at = Date.now(); c.err = String(x || "couldn't read them"); renderMain(); });
  }
  return c.data;
}
/** A fresh list in (from actions_list, or a command that answers with one). A project's files changed
 *  while you were in Cue and brought something new to allow: the trust sheet says so, on its own. */
function actTake(pl, v, why = "") {
  if (!v || !Array.isArray(v.actions)) return actData(pl, true);
  const c = actLists.get(actKey(pl)) || { at: Date.now(), loading: false, err: "" };
  const before = c.data ? new Set((c.data.pending || []).map((p) => `${p.id}|${p.run}`)) : null;
  c.data = v;
  c.at = Date.now();
  actLists.set(actKey(pl), c);
  const fresh = (v.pending || []).filter((p) => !before?.has(`${p.id}|${p.run}`));
  if (why === "event" && before && fresh.length && !sheet) { actTrust = { pl, then: null, changed: !!actSessionOn(pl) }; sheet = "act-trust"; }
  renderMain();
}
function actRunningLoad() {
  actRunningAt = Date.now();
  invoke("actions_running").then((v) => { actRunning = Array.isArray(v) ? v : []; renderMain(); }, () => {});
}
const actFind = (pl, id) => actLists.get(actKey(pl))?.data?.actions.find((a) => a.id === id) || null;
const actIsQueued = (pl, id) => [...actQueued.values()].some((l) => l.some((q) => q.id === id && actKey(q.pl) === actKey(pl)));
// Cue says a project's states or files changed: read it again, and what runs everywhere.
T.event.listen("actions", (e) => {
  const p = e.payload || {}, pl = actPlaceAt(p.folder || "", p.machine || "");
  actRunningLoad();
  if (actLists.has(actKey(pl))) actData(pl, true, "event");
});
/** Called with each new state, before it's taken: a session that was working and isn't any more ended its
 *  turn, so what you put off until then runs now. */
function actTurnEnds(next) {
  for (const [sid, list] of actQueued) {
    if (next.sessions.find((x) => x.session_id === sid)?.state === "working") continue;
    actQueued.delete(sid);
    for (const q of list) actGo(q.pl, q.id, q.log, " after the turn");
  }
}

// ---------- words ----------
const ACT_VERBS = /^(install|test|build|lint|format|clean|migrate|deploy|seed|check|compile|generate|bundle|typecheck|update|upgrade|reset|start|serve|watch|sync|fetch|publish|release|bench|package|prune|rebuild|reinstall|restart|run|stop|set|push|pull)$/i;
/** "Install" → "Installing", "Migrate db" → "Migrating db"; a name that isn't a verb: "Running Docs". */
function actIng(name) {
  const [w, ...rest] = String(name).split(" ");
  if (!ACT_VERBS.test(w)) return `Running ${name}`;
  const ing = /[^aeiou][aeiou][bdgmnprt]$/i.test(w) && w.length <= 4 ? w + w.slice(-1) + "ing" : /[^e]e$/i.test(w) ? w.slice(0, -1) + "ing" : w + "ing";
  return [ing, ...rest].join(" ");
}
const actSecs = (s) => (s == null ? "" : s < 60 ? `${Math.round(s)}s` : `${Math.floor(s / 60)}m${Math.round(s % 60) ? ` ${Math.round(s % 60)}s` : ""}`);
/** When a run ended: "2h", "yesterday", "3 days". */
function actWhen(at) {
  const d = Math.floor((now() - at) / 86400000);
  return d < 1 ? ago(at) : d < 2 ? "yesterday" : `${d} days`;
}
/** Its last run, one line: "✓ 12s · yesterday", "✗ exit 1 · 2h". */
function actLastRun(st) {
  if (!st || st.running || st.exit == null || !st.at) return "";
  return st.exit === 0 ? `✓ ${actSecs(st.secs)} · ${actWhen(st.at)}` : `✗ exit ${st.exit} · ${actWhen(st.at)}`;
}
/** While it runs: ":5173 · 4m" for a server, "running · 8s" for a one-off. */
const actUp = (st) => `${st.port ? `:${st.port} · ` : "running · "}<span data-ago="${+st.since || now()}">${ago(+st.since || now())}</span>`;

// ---------- running one ----------
/** ▶ from anywhere: not allowed yet → the trust sheet first; an "ask" one while an agent works there → Run now
 *  or After this turn; else it runs (a running one restarts). `log`: its output opens too (⌘↵). */
function actRun(pl, id, log = false) {
  const a = actFind(pl, id);
  if (!a) return toast("That action is gone");
  if (!a.allowed) { actTrust = { pl, then: { id, log }, changed: false }; sheet = "act-trust"; actPal = actPop = null; return renderMain(); }
  const w = a.ask && !a.state?.running ? actWorking(pl) : null;
  if (w) { actAsk = { pl, id, log, sid: w.session_id }; sheet = "act-ask"; actPal = actPop = null; return renderMain(); }
  return actGo(pl, id, log);
}
async function actGo(pl, id, log = false, when = "") {
  const a = actFind(pl, id), name = a?.name || id, restart = !!a?.state?.running;
  actLast = { pl, id };
  actPal = actPop = null;
  if (a) a.state = { ...(a.state || {}), running: true, since: now(), exit: null };
  renderMain();
  try {
    const st = await invoke("actions_run", { folder: pl.folder, machine: pl.machine, id });
    if (a && st) a.state = st;
    toast(`${restart ? "Restarted" : "Running"} ${name}${when}`);
    if (log) actOpenLog(pl, id);
  } catch (x) { toast(`Couldn't run ${name}: ${x}`); actData(pl, true); }
  actRunningLoad();
  renderMain();
}
async function actStop(pl, id, restart = false) {
  const a = actFind(pl, id);
  try {
    const st = await invoke(restart ? "actions_restart" : "actions_stop", { folder: pl.folder, machine: pl.machine, id });
    if (a && st) a.state = st;
    toast(`${restart ? "Restarted" : "Stopped"} ${a?.name || id}`);
  } catch (x) { toast(`Couldn't ${restart ? "restart" : "stop"} ${a?.name || id}: ${x}`); }
  actRunningLoad();
  actData(pl, true);
}
/** ⌘⇧E: the last action again. */
function actAgain() {
  if (!actLast) return toast("Nothing run yet: ⌘E picks one");
  if (!actFind(actLast.pl, actLast.id)) { actData(actLast.pl, true); return toast("Reading its actions again: press ⌘⇧E once more"); }
  actRun(actLast.pl, actLast.id);
}

// ---------- its output ----------
/** Log: its tmux session as a tab in Cue's own terminal (⌘J), attached where it runs; without one, a sheet
 *  with its last lines, read again every second. */
async function actOpenLog(pl, id) {
  const a = actFind(pl, id), name = a?.name || id;
  actPal = actPop = null;
  let at = null;
  try { at = await invoke("actions_attach_id", { folder: pl.folder, machine: pl.machine, id }); } catch { at = null; }
  if (at?.tmux) {
    sheet = sheet?.startsWith("act-") ? null : sheet;
    renderMain();
    return openShell(pl.machine ? "" : pl.folder, { name, action: { folder: pl.folder, machine: pl.machine, id } });
  }
  actLogView = { pl, id, name, text: "", pinned: true, top: 0 };
  sheet = "act-log";
  renderMain();
  actLogRead();
}
async function actLogRead() {
  const v = actLogView;
  if (!v || sheet !== "act-log") return;
  let text;
  try { text = await invoke("actions_log", { folder: v.pl.folder, machine: v.pl.machine, id: v.id, lines: 400 }); } catch (x) { text = `Couldn't read its output: ${x}`; }
  if (actLogView !== v || text === v.text) return;
  v.text = String(text || "");
  const el = document.querySelector(".ac-log-text");
  if (el) { el.textContent = v.text || "No output yet."; if (v.pinned) el.scrollTop = el.scrollHeight; }
}
setInterval(actLogRead, 1000);
document.addEventListener("scroll", (e) => {
  if (!e.target?.matches?.(".ac-log-text") || !actLogView) return;
  const el = e.target;
  actLogView.top = el.scrollTop;
  actLogView.pinned = el.scrollHeight - el.scrollTop - el.clientHeight < 8;
}, true);
/** A failed run's last lines, under its row in the tab (read once per run). */
function actTail(pl, a) {
  const k = `${actKey(pl)}|${a.id}|${a.state.at}`;
  if (!actTails.has(k)) {
    actTails.set(k, null);
    invoke("actions_log", { folder: pl.folder, machine: pl.machine, id: a.id, lines: 6 }).then((t) => { actTails.set(k, String(t || "").trimEnd()); renderMain(); }, () => actTails.set(k, ""));
  }
  const t = actTails.get(k);
  return t ? `<pre class="ac-tail">${esc(t)}</pre>` : "";
}

// ---------- asking the agent ----------
/** Cue's actions prompt into the box of a session on that project, as a draft: you press Send. */
function actAskAgent(pl) {
  const s = actSessionOn(pl);
  actPal = actPop = null;
  if (sheet?.startsWith("act-")) sheet = null;
  if (!s) { renderMain(); return toast(`No session is open on ${pl.project}: start one there, then ask it`); }
  const prompt = actLists.get(actKey(pl))?.data?.prompt || ACT_PROMPT;
  const key = `s:${s.session_id}`, d = draft(key);
  if (!d.text.includes(prompt)) d.text = d.text.trim() ? `${d.text.trimEnd()}\n\n${prompt}` : prompt;
  saveDrafts();
  if (openSid() !== s.session_id) setActive(null, s.session_id);
  else { renderMain(); focusComposer(); }
  toast(`In ${nameOf(s.session_id, s.project)}'s box: press Send when you're ready`);
}
/** "Edit actions…": the Actions tab of a session on that project; with none, a new action. */
function actEditAll(pl) {
  const s = actSessionOn(pl);
  actPal = actPop = null;
  if (!s) return actEditOpen(pl, null);
  if (openSid() !== s.session_id) setActive(null, s.session_id);
  extTabOf.delete(s.session_id);
  actTabOn.add(s.session_id);
  renderMain();
}

// ---------- C · ⌘E ----------
/** What ⌘E lists: what runs first, then the project's, then yours. */
function actPalList(d) {
  const rank = (a) => (a.state?.running ? 0 : a.source === "shared" ? 1 : 2);
  return d.actions.map((a, i) => [a, i]).sort((x, y) => rank(x[0]) - rank(y[0]) || x[1] - y[1]).map(([a]) => a);
}
function actPalToggle() {
  if (actPal) { actPal = null; renderMain(); return focusComposer(); }
  sheet = lightbox = null;
  liveOpen = newOpen = false;
  actPop = null;
  actPal = { pl: actHere(), sel: 0, shown: [] };
  draft("ac-find").text = "";
  if (actPal.pl) actData(actPal.pl);
  renderMain();
  document.querySelector('[data-text="ac-find"]')?.focus();
}
function actPalette() {
  const pl = actPal.pl, c = pl && actLists.get(actKey(pl)), d = c?.data;
  const q = draft("ac-find").text.trim().toLowerCase();
  const rows = d ? actPalList(d).filter((a) => !q || q.split(/\s+/).every((w) => `${a.name} ${a.run}`.toLowerCase().includes(w))) : [];
  actPal.shown = rows.map((a) => a.id);
  actPal.sel = Math.min(Math.max(actPal.sel, 0), Math.max(rows.length - 1, 0));
  const row = (a, n) => {
    const st = a.state || {}, last = actLastRun(st);
    const right = st.running ? `<span class="ac-st on">${actUp(st)}</span><span class="ac-hint">↵ restart</span>`
      : !a.allowed ? `<span class="ac-tag">NEEDS YOUR OK</span>` : last ? `<span class="ac-st ${st.exit ? "bad" : ""}">${last}</span>` : "";
    return `<div class="ac-prow ${n === actPal.sel ? "sel" : ""}" role="button" data-ac="run" ${acAttrs(pl, a.id)}>${st.running ? `<i class="ac-live"></i>` : `<span class="ac-play">▶</span>`}<span class="ac-pname">${esc(a.name)}</span><code class="ac-cmd">${esc(a.run)}</code><span class="grow"></span>${right}</div>`;
  };
  const body = !pl ? `<div class="ac-empty">Open a session, or pick a project at the top, to see its actions.</div>`
    : !d ? `<div class="ac-empty">${c?.err ? `Couldn't read its actions: ${esc(c.err)}` : "Reading its actions…"}</div>`
    : rows.length ? rows.map(row).join("") : `<div class="ac-empty">${q ? `No action matches “${esc(q)}”.` : `No actions yet for ${esc(pl.project)}.`}</div>`;
  return `<div class="scrim spot-scrim ac-scrim"><div class="ac-pal" role="dialog" aria-label="Actions">
    <div class="lv-top"><label class="sv-search">${SEARCH_ICON}<input data-text="ac-find" placeholder="${pl ? `Run an action in ${esc(projLabel(pl.project))}…` : "Run an action…"}  ↑ ↓ ↵" spellcheck="false" autocomplete="off" value="${esc(draft("ac-find").text)}"/></label></div>
    ${pl ? `<div class="ac-phead">${projDot(pl.project)}<span>${esc(projLabel(pl.project))}</span>${machTag(pl.machine)}<span class="dim mono">${esc(homeless(pl.folder))}</span></div>` : ""}
    <div class="ac-plist">${body}</div>
    <div class="lv-foot">${pl ? `<button data-ac="edit-all" ${acAttrs(pl)}>Edit actions…</button><button data-ac="ask-agent" ${acAttrs(pl)}>Ask the agent to set up actions</button>` : ""}<span class="lv-keys"><kbd>↵</kbd> run · <kbd>⌘↵</kbd> and its output · <kbd>⌘⇧E</kbd> again</span></div></div></div>`;
}

// ---------- D · under the latest reply ----------
/** Under the agent's latest reply: the project's actions as buttons, so you run one yourself instead of
 *  asking the agent to. A click runs it; the button then says how it's going. */
function actReplyHtml(s, at) {
  if (!s?.cwd) return "";
  const pl = actPlace(s), d = actData(pl);
  if (!d?.actions?.length) return "";
  return `<span class="ac-sugs"><span class="ac-div" aria-hidden="true"></span>${d.actions.map((a) => actSugBtn(pl, a, at)).join("")}</span>`;
}
function actSugBtn(pl, a, at) {
  const st = a.state || {}, name = esc(a.name), ids = acAttrs(pl, a.id);
  if (actIsQueued(pl, a.id)) return `<span class="cv-pill ac-sug wait">${name} · runs after this turn</span>`;
  if (st.running && a.keep) return `<button class="cv-pill ac-sug on" data-ac="log" ${ids}><i class="ac-live"></i>${name} · ${actUp(st)}</button>`;
  if (st.running) return `<button class="cv-pill ac-sug on" data-ac="log" ${ids}>${esc(actIng(a.name))}… <span data-ago="${+st.since || now()}">${ago(+st.since || now())}</span></button>`;
  if (st.at && st.at >= at && st.exit != null) return st.exit === 0
    ? `<button class="cv-pill ac-sug" data-ac="log" ${ids}>✓ ${name} · ${actSecs(st.secs)}</button>`
    : `<button class="cv-pill ac-sug bad" data-ac="log" ${ids}>✗ ${name} · exit ${esc(st.exit)}</button>`;
  return `<button class="cv-pill ac-sug" data-ac="run" ${ids}>▶ ${name}</button>`;
}

// ---------- E · dots on the project chips ----------
/** What runs in a project (by name: a chip names a project, a project may be several checkouts). Cue says
 *  each one's project by the folder it runs in, never by a session, so it's here with every session closed. */
const actRunningOf = (p) => actRunning.filter((r) => r.project === p);
/** One small dot per running action, on a project's chip or row; a click opens what runs there. */
function actDots(p, where) {
  if (!actRunningAt || Date.now() - actRunningAt > 15000) actRunningLoad();
  const rs = p ? actRunningOf(p) : [];
  if (!rs.length) return "";
  return `<button class="ac-dots ${actPop?.project === p && actPop.where === where ? "open" : ""}" data-ac="pop" data-project="${esc(p)}" data-where="${esc(where)}" aria-label="${rs.length} running in ${esc(projLabel(p))}">${rs.slice(0, 5).map(() => "<i></i>").join("")}${rs.length > 5 ? `<b>+${rs.length - 5}</b>` : ""}</button>`;
}
function actPopHtml() {
  const p = actPop.project, pl = actOfProject(p), d = pl && actData(pl);
  const rs = actRunningOf(p);
  // Running in more than one checkout (two worktrees, or a machine too): each says which.
  const many = new Set(rs.map((r) => `${r.machine || ""}|${r.folder}`)).size > 1;
  const running = rs.map((r) => {
    const rp = { folder: r.folder, machine: r.machine || "", project: p }, ids = acAttrs(rp, r.id);
    const where = many ? [r.branch || r.checkout, r.machine && machShort(r.machine)].filter(Boolean).join(" · ") : "";
    return `<div class="ac-poprow"><i class="ac-live"></i><span class="ac-pname">${esc(r.name)}</span>${where ? `<span class="ac-where mono">${esc(where)}</span>` : ""}<span class="ac-st">${actUp({ port: r.port, since: r.since })}</span><span class="grow"></span>
      <button class="btn small" data-ac="log" ${ids}>Log</button><button class="btn small" data-ac="restart" ${ids}>⟳ Restart</button><button class="btn small" data-ac="stop" ${ids}>Stop</button></div>`;
  }).join("");
  const rest = d ? d.actions.filter((a) => !a.state?.running) : [];
  return `<div class="ac-pop" role="dialog" aria-label="Actions in ${esc(projLabel(p))}">
    <div class="ac-poph">RUNNING</div>${running || `<div class="ac-empty">Nothing running.</div>`}
    ${rest.length ? `<div class="ac-poph">RUN</div><div class="ac-poprun">${rest.map((a) => `<button class="cv-pill" data-ac="run" ${acAttrs(pl, a.id)}>▶ ${esc(a.name)}</button>`).join("")}</div>` : ""}
    <div class="ac-popfoot">${pl ? `<button data-ac="pal" ${acAttrs(pl)}>All actions <kbd>⌘E</kbd></button><button data-ac="edit-all" ${acAttrs(pl)}>Edit actions…</button>` : ""}</div></div>`;
}

// ---------- B · the Actions tab ----------
/** The tab's button, after Chat and the extensions' tabs: how many run, when any do. */
function actTabBtn(sid) {
  const x = sessionOf(sid) || (state.live || []).find((q) => q.session_id === sid);
  const n = x?.cwd ? actRunning.filter((r) => r.folder === actFolder(x.cwd, x.machine) && (r.machine || "") === (x.machine || "")).length : 0;
  return `<button class="stab ${actTabOn.has(sid) ? "on" : ""}" data-ac="tab" data-sid="${esc(sid)}">Actions${n ? `<span class="stab-n">${n} running</span>` : ""}</button>`;
}
/** Where the tab's page goes, in place of the chat (placeExt hides the chat under it). */
const actSlot = (sid) => (sid && actTabOn.has(sid) ? `<div class="ac-slot" data-act-slot>${actPage(sid)}</div>` : "");
function actPage(sid) {
  const x = sessionOf(sid) || (state.live || []).find((q) => q.session_id === sid) || state.items.find((i) => i.session_id === sid);
  if (!x?.cwd) return `<div class="ac-empty">This session has no folder Cue knows, so no actions.</div>`;
  const pl = actPlace(x), c = actLists.get(actKey(pl)), d = actData(pl);
  if (!d) return `<div class="ac-empty">${c?.err ? `Couldn't read its actions: ${esc(c.err)}` : "Reading this project's actions…"}</div>`;
  const shown = d.actions;
  const running = shown.filter((a) => a.state?.running);
  const sec = (src, head, sub) => {
    const rows = shown.filter((a) => a.source === src && !a.state?.running);
    return rows.length ? `<div class="ac-head">${head}<span>${sub}</span></div>${rows.map((a) => actRow(pl, a)).join("")}` : "";
  };
  const fp = (d.pending || []).length;
  const pend = fp ? `<div class="ac-note"><span>${fp === 1 ? "One action" : `${fp} actions`} from this project's files ${fp === 1 ? "needs" : "need"} your OK before ${fp === 1 ? "it runs" : "they run"}.</span><button class="btn small" data-ac="trust" ${acAttrs(pl)}>Review…</button></div>` : "";
  const ign = d.files?.local && d.files.localIgnored === false ? `<div class="ac-note"><span>Git doesn't ignore <code>.cue/actions.local.json</code>, so it could be committed. Add it to <code>.gitignore</code> to keep it yours.</span></div>` : "";
  const any = d.actions.length;
  return `<div class="ac-page">${pend}${ign}
    ${running.length ? `<div class="ac-head">RUNNING<span>${running.length}</span></div>${running.map((a) => actRow(pl, a)).join("")}` : ""}
    ${ACT_SOURCES.map(([s, h, sub]) => sec(s, h, sub)).join("")}
    ${any ? "" : `<div class="ac-empty">No actions yet for ${esc(projLabel(pl.project))}. Add the two or three you run most, or ask the agent to set them up from what the project uses.</div>`}
    <div class="ac-foot"><button class="btn" data-ac="new" ${acAttrs(pl)}>+ New action</button><button class="btn" data-ac="ask-agent" ${acAttrs(pl)}>Ask the agent to set up actions</button><span class="grow"></span><span class="lv-keys"><kbd>⌘E</kbd> run one from anywhere</span></div></div>`;
}
/** One action: its name, its command, how it last went; ▶ Run and ✎, or Log, ⟳ Restart and Stop while it runs. */
function actRow(pl, a) {
  const st = a.state || {}, ids = acAttrs(pl, a.id), last = actLastRun(st);
  const tags = `${a.keep ? `<span class="ac-tag">KEEPS RUNNING</span>` : ""}${a.ask ? `<span class="ac-tag">ASKS</span>` : ""}${!a.allowed ? `<span class="ac-tag warn">NEEDS YOUR OK</span>` : ""}`;
  const where = st.running ? `<span class="ac-st on">${actUp(st)}${st.noTmux ? " · ends when Cue quits" : ""}</span>` : actIsQueued(pl, a.id) ? `<span class="ac-st">runs after this turn</span>` : last ? `<span class="ac-st ${st.exit ? "bad" : ""}">${last}</span>` : "";
  const btns = st.running
    ? `<button class="btn small" data-ac="log" ${ids}>Log</button><button class="btn small" data-ac="restart" ${ids}>⟳ Restart</button><button class="btn small" data-ac="stop" ${ids}>Stop</button>`
    : `${last ? `<button class="btn small" data-ac="log" ${ids}>Log</button>` : ""}<button class="btn small primary" data-ac="run" ${ids}>▶ Run</button>`;
  return `<div class="ac-row ${st.running ? "on" : ""}"><div class="ac-main"><div class="ac-line">${st.running ? `<i class="ac-live"></i>` : ""}<span class="ac-pname">${esc(a.name)}</span>${tags}${where}</div><code class="ac-cmd">${esc(a.run)}</code>${!st.running && st.exit ? actTail(pl, a) : ""}</div>
    <div class="ac-btns">${btns}<button class="ac-edit" data-ac="edit" ${ids} aria-label="Edit ${esc(a.name)}">✎</button></div></div>`;
}

// ---------- the sheets: edit, trust, ask, output ----------
function actEditOpen(pl, id) {
  const a = id ? actFind(pl, id) : null;
  actEdit = { pl, oldId: a ? a.id : null, keep: !!a?.keep, ask: !!a?.ask, scope: a?.source === "shared" ? "shared" : "local" };
  draft("ac-name").text = a?.name || "";
  draft("ac-run").text = a?.run || "";
  actPal = actPop = null;
  sheet = "act-edit";
  renderMain();
  document.querySelector(`[data-text="${a ? "ac-run" : "ac-name"}"]`)?.focus();
}
/** What the edit sheet would save. */
function actForm() {
  const e = actEdit;
  return { name: draft("ac-name").text.trim(), run: draft("ac-run").text.trim(), keep: e.keep, ask: e.ask };
}
/** The line it writes, as it'll be in the file. */
function actJson(f) {
  const o = { name: f.name || "…", run: f.run || "…" };
  if (f.keep) o.keep = true;
  if (f.ask) o.ask = true;
  return JSON.stringify(o).replace(/":/g, "\": ").replace(/,"/g, ", \"").replace(/^\{/, "{ ").replace(/\}$/, " }");
}
function actEditSheet() {
  const e = actEdit, f = actForm(), d = actLists.get(actKey(e.pl))?.data;
  const segs = (k, opts) => `<div class="seg">${opts.map(([v, l]) => `<button class="${e[k] === v ? "on" : ""}" data-ac="ed-set" data-k="${k}" data-v="${esc(JSON.stringify(v))}">${esc(l)}</button>`).join("")}</div>`;
  const file = e.scope === "shared" ? ".cue/actions.json" : ".cue/actions.local.json";
  const exists = e.scope === "shared" ? d?.files?.shared : d?.files?.local;
  const fileNote = [exists === false ? "Cue makes the file, with notes for agents at its top." : "", e.scope === "local" && d?.files?.localIgnored === false ? "Git doesn't ignore it yet: add it to .gitignore to keep it yours." : ""].filter(Boolean).join(" ");
  const fld = (label, ctl, sub = "") => `<div class="ac-f"><div class="ac-fl">${label}</div><div>${ctl}${sub ? `<div class="ac-fs">${sub}</div>` : ""}</div></div>`;
  return `<div class="sheet ac-sheet"><div class="sheet-head"><h3>${e.oldId ? "Edit action" : "New action"}</h3><span class="dim">${esc(projLabel(e.pl.project))}${e.pl.machine ? ` · on ${esc(machName(e.pl.machine))}` : ""}</span><span class="grow"></span><kbd>Esc</kbd></div><div class="sheet-body">
    ${fld("Name", `<input class="ac-in" data-text="ac-name" maxlength="40" placeholder="Install" spellcheck="false" value="${esc(draft("ac-name").text)}"/>`)}
    ${fld("Command", `<input class="ac-in mono" data-text="ac-run" placeholder="npm ci" spellcheck="false" autocomplete="off" value="${esc(draft("ac-run").text)}"/>`, `One line, run by your shell in <code>${esc(homeless(e.pl.folder))}</code>.`)}
    ${fld("It", segs("keep", [[false, "Runs once"], [true, "Keeps running"]]), e.keep ? "A server or a watcher: Cue shows its port, with Restart and Stop." : "")}
    ${fld("While an agent works", segs("ask", [[false, "Just run"], [true, "Ask: now or after the turn"]]), e.ask ? "For what could trip an agent mid-turn (an install, a clean)." : "")}
    <div class="ac-json"><span class="ac-jf">${esc(file)}</span><code data-ac-json>${esc(actJson(f))}</code>${fileNote ? `<span class="ac-jn">${esc(fileNote)}</span>` : ""}</div>
  </div><div class="ac-sheet-foot">${e.oldId ? `<button class="btn deny" data-ac="ed-delete">Delete</button>` : ""}<span class="grow"></span><button class="btn" data-ac="close">Cancel</button><button class="btn" data-ac="ed-try">▶ Try it</button><button class="btn primary" data-ac="ed-save">Save</button></div></div>`;
}
/** Save (and, from ▶ Try it, run it and show its output). Saved in Cue is allowed. */
async function actSave(andRun = false) {
  const e = actEdit, f = actForm();
  if (!f.name || !f.run) return toast(f.name ? "Give it a command" : "Give it a name");
  try {
    const v = await invoke("actions_save", { folder: e.pl.folder, machine: e.pl.machine, action: f, keep: e.scope, oldId: e.oldId });
    sheet = null;
    actEdit = null;
    actTake(e.pl, v);
    const a = (v?.actions || []).find((x) => x.name === f.name && x.source === e.scope);
    if (andRun && a) return actGo(e.pl, a.id, true);
    toast(`Saved ${f.name}`);
  } catch (x) { toast(`Couldn't save it: ${x}`); }
}
async function actDelete() {
  const e = actEdit;
  try { const v = await invoke("actions_delete", { folder: e.pl.folder, machine: e.pl.machine, id: e.oldId }); sheet = null; actEdit = null; toast(`Deleted ${draft("ac-name").text.trim()}`); actTake(e.pl, v); }
  catch (x) { toast(`Couldn't delete it: ${x}`); }
}
/** The trust sheet's rows: the files' new and changed ones. */
const actTrustList = (t, d) => d?.pending || [];
function actTrustSheet() {
  const t = actTrust, d = actLists.get(actKey(t.pl))?.data, pend = actTrustList(t, d);
  const fresh = pend.filter((p) => p.why !== "changed"), changed = pend.filter((p) => p.why === "changed");
  const shared = pend.some((p) => p.source === "shared");
  const title = t.changed ? "The agent changed this project's actions" : shared ? `${projLabel(t.pl.project)} shares actions in its repo` : `New actions in ${projLabel(t.pl.project)}`;
  const from = (p) => (p.source === "shared" ? ".cue/actions.json" : ".cue/actions.local.json");
  const row = (p) => `<div class="ac-trow"><span class="ac-pname">${esc(p.name)}</span><span class="ac-from">${esc(from(p))}</span>${p.was ? `<code class="ac-cmd was">${esc(p.was)}</code>` : ""}<code class="ac-cmd">${esc(p.run)}</code></div>`;
  const n = pend.length;
  return `<div class="sheet ac-sheet"><div class="sheet-head"><h3>${esc(title)}</h3><span class="grow"></span><kbd>Esc</kbd></div><div class="sheet-body">
    <div class="set-note">Cue runs a command from a file only once you've allowed it, and asks again whenever it changes. These run in <code>${esc(homeless(t.pl.folder))}</code>${t.pl.machine ? ` on ${esc(machName(t.pl.machine))}` : ""}.</div>
    ${fresh.length ? `<div class="ac-head">NEW<span>${fresh.length}</span></div>${fresh.map(row).join("")}` : ""}
    ${changed.length ? `<div class="ac-head">CHANGED<span>${changed.length}</span></div>${changed.map(row).join("")}` : ""}
    ${n ? "" : `<div class="ac-empty">Nothing waits for your OK now.</div>`}
  </div><div class="ac-sheet-foot"><span class="grow"></span><button class="btn" data-ac="close">Not now</button>${n ? `<button class="btn primary" data-ac="trust-yes">${n === 1 ? "Allow it" : `Allow these ${n}`}${t.then ? " and run" : ""}</button>` : ""}</div></div>`;
}
async function actAllow() {
  const t = actTrust, d = actLists.get(actKey(t.pl))?.data, ids = actTrustList(t, d).map((p) => p.id);
  try {
    const v = await invoke("actions_allow", { folder: t.pl.folder, machine: t.pl.machine, ids });
    sheet = null;
    actTrust = null;
    actTake(t.pl, v);
    if (t.then && (v?.actions || []).some((a) => a.id === t.then.id && a.allowed)) return actRun(t.pl, t.then.id, t.then.log);
    toast(ids.length === 1 ? "Allowed" : `Allowed ${ids.length}`);
  } catch (x) { toast(`Couldn't allow them: ${x}`); }
}
function actAskSheet() {
  const k = actAsk, a = actFind(k.pl, k.id), s = sessionOf(k.sid);
  return `<div class="sheet ac-sheet narrow"><div class="sheet-head"><h3>${esc(s ? nameOf(s.session_id, s.project) : "Its agent")} is working</h3><span class="grow"></span><kbd>Esc</kbd></div><div class="sheet-body">
    <div class="ac-ask"><b>${esc(a?.name || k.id)}</b> could trip it up mid-turn:<code class="ac-cmd">${esc(a?.run || "")}</code>Run it now anyway, or once this turn ends?</div>
  </div><div class="ac-sheet-foot"><button class="btn" data-ac="close">Cancel</button><span class="grow"></span><button class="btn" data-ac="ask-later">After this turn</button><button class="btn primary" data-ac="ask-now">▶ Run now</button></div></div>`;
}
function actLogSheet() {
  const v = actLogView, a = actFind(v.pl, v.id), st = a?.state || {}, ids = acAttrs(v.pl, v.id);
  const where = st.running ? `<span class="ac-st on">${actUp(st)}</span>` : actLastRun(st) ? `<span class="ac-st ${st.exit ? "bad" : ""}">${actLastRun(st)}</span>` : "";
  return `<div class="sheet ac-sheet wide"><div class="sheet-head"><h3>${esc(v.name)}</h3>${where}<span class="grow"></span>
    ${st.running ? `<button class="btn small" data-ac="restart" ${ids}>⟳ Restart</button><button class="btn small" data-ac="stop" ${ids}>Stop</button>` : `<button class="btn small" data-ac="run" ${ids}>▶ Run again</button>`}<kbd>Esc</kbd></div>
    ${a ? `<code class="ac-cmd ac-logcmd">${esc(a.run)}</code>` : ""}
    <pre class="ac-log-text">${esc(v.text || "No output yet.")}</pre></div>`;
}
/** Which of the sheets above is showing (sheet "act-…"). */
function actSheet() {
  if (sheet === "act-edit" && actEdit) return actEditSheet();
  if (sheet === "act-trust" && actTrust) return actTrustSheet();
  if (sheet === "act-ask" && actAsk) return actAskSheet();
  if (sheet === "act-log" && actLogView) return actLogSheet();
  return settingsSheet();
}
/** Over the window: ⌘E's palette and a chip's popover (the sheets go where every sheet goes). */
const actOverlay = () => (actPal ? actPalette() : "") + (actPop ? actPopHtml() : "");
/** After each draw: the popover under its dots (gone with them), and the output where you were in it. */
function placeActs() {
  const pop = document.querySelector(".ac-pop");
  if (pop) {
    const at = document.querySelector(`.ac-dots[data-project="${CSS.escape(actPop.project)}"][data-where="${CSS.escape(actPop.where)}"]`);
    if (!at) { actPop = null; pop.remove(); }
    else {
      const r = at.getBoundingClientRect(), w = pop.offsetWidth;
      pop.style.left = `${Math.max(8, Math.min(r.left - 12, innerWidth - w - 8))}px`;
      pop.style.top = `${r.bottom + 8}px`;
    }
  }
  const log = document.querySelector(".ac-log-text");
  if (log && actLogView) log.scrollTop = actLogView.pinned ? log.scrollHeight : actLogView.top;
}

// ---------- Settings → Actions ----------
const actPromptNow = () => [...actLists.values()].find((c) => c.data?.prompt)?.data.prompt || ACT_PROMPT;
function actSettings() {
  if (!actPromptInit) { draft("ac-prompt").text = actPromptNow(); actPromptInit = true; }
  const t = draft("ac-prompt").text, saved = actPromptNow();
  return `<div class="set-note">Actions are a project's own buttons for its commands: install, test, clean a cache, start a server. They live in its folder, in <code>.cue/actions.json</code> (shared with the repo) and <code>.cue/actions.local.json</code> (only you). <kbd>⌘E</kbd> runs one; the Actions tab of a session lists them all.</div>
    <div class="set-group">Ask the agent to set up actions</div>
    <div class="set-sub ac-psub">What Cue puts in the session's box when you ask (you press Send). ${saved === ACT_PROMPT ? "This is Cue's own." : "This is yours."}</div>
    <textarea class="ac-prompt" data-text="ac-prompt" rows="5" spellcheck="false">${esc(t)}</textarea>
    <div class="ac-pbtns"><button class="btn" data-ac="prompt-reset" ${saved === ACT_PROMPT && t === ACT_PROMPT ? "disabled" : ""}>Reset to Cue's</button><button class="btn primary" data-ac="prompt-save" ${t.trim() && t !== saved ? "" : "disabled"}>Save</button></div>`;
}
async function actPromptSave(text) {
  try {
    await invoke("actions_prompt_set", { text });
    const now_ = text || ACT_PROMPT;
    for (const c of actLists.values()) if (c.data) c.data.prompt = now_;
    if (![...actLists.values()].some((c) => c.data)) actLists.set("|", { at: Date.now(), loading: false, err: "", data: { actions: [], pending: [], files: {}, prompt: now_ } });
    draft("ac-prompt").text = now_;
    toast(text ? "Saved" : "Back to Cue's");
  } catch (x) { toast(`Couldn't save it: ${x}`); }
  renderMain();
}

// ---------- clicks, keys, typing ----------
document.addEventListener("click", (e) => {
  const t = e.target.closest?.("[data-ac]");
  // A click anywhere else closes a chip's popover.
  if (actPop && !e.target.closest?.(".ac-pop") && t?.dataset.ac !== "pop") { actPop = null; renderMain(); }
  if (actPal && e.target.matches?.(".ac-scrim")) { e.stopPropagation(); actPal = null; renderMain(); return focusComposer(); }
  if (!t) return;
  e.stopPropagation();
  const ac = t.dataset.ac, pl = t.dataset.f != null ? actPlaceOf(t) : null, id = t.dataset.id;
  if (ac === "run") return actRun(pl, id, e.metaKey);
  if (ac === "log") return actOpenLog(pl, id);
  if (ac === "stop" || ac === "restart") return actStop(pl, id, ac === "restart");
  if (ac === "edit") return actEditOpen(pl, id);
  if (ac === "new") return actEditOpen(pl, null);
  if (ac === "edit-all") return actEditAll(pl);
  if (ac === "ask-agent") return actAskAgent(pl);
  if (ac === "pal") { actPop = null; actPalToggle(); if (actPal) { actPal.pl = pl; actData(pl); renderMain(); } return; }
  if (ac === "trust") { actTrust = { pl, then: null, changed: false }; sheet = "act-trust"; return renderMain(); }
  if (ac === "trust-yes") return actAllow();
  if (ac === "close") { sheet = null; actEdit = actTrust = actAsk = actLogView = null; return renderMain(); }
  if (ac === "ask-now" || ac === "ask-later") {
    const k = actAsk;
    sheet = null;
    actAsk = null;
    if (ac === "ask-now") return actGo(k.pl, k.id, k.log);
    actQueued.set(k.sid, [...(actQueued.get(k.sid) || []), { pl: k.pl, id: k.id, log: k.log }]);
    toast(`${actFind(k.pl, k.id)?.name || "It"} runs after this turn`);
    return renderMain();
  }
  if (ac === "tab") {
    const sid = t.dataset.sid;
    extTabOf.delete(sid);
    actTabOn.add(sid);
    return renderMain();
  }
  if (ac === "pop") {
    const p = t.dataset.project, w = t.dataset.where;
    actPop = actPop?.project === p && actPop.where === w ? null : { project: p, where: w };
    const pp = actPop && actOfProject(p);
    if (pp) actData(pp);
    return renderMain();
  }
  if (ac === "ed-set") { actEdit[t.dataset.k] = JSON.parse(t.dataset.v); return renderMain(); }
  if (ac === "ed-save" || ac === "ed-try") return actSave(ac === "ed-try");
  if (ac === "ed-delete") return actDelete();
  if (ac === "prompt-save") return actPromptSave(draft("ac-prompt").text.trim() === ACT_PROMPT ? "" : draft("ac-prompt").text.trim());
  if (ac === "prompt-reset") return actPromptSave("");
}, true);
document.addEventListener("input", (e) => {
  const k = e.target.dataset?.text;
  if (!k?.startsWith("ac-")) return;
  e.stopPropagation();
  draft(k).text = e.target.value;
  if (k === "ac-find") { if (actPal) actPal.sel = 0; return renderMain(); }
  if (k === "ac-prompt") { const b = document.querySelector('[data-ac="prompt-save"]'); if (b) b.disabled = !e.target.value.trim() || e.target.value === actPromptNow(); return grow(e.target); }
  const j = document.querySelector("[data-ac-json]");
  if (j && actEdit) j.textContent = actJson(actForm());
}, true);
document.addEventListener("keydown", (e) => {
  // ⌘E's palette: ↑ ↓ move, ↵ runs (⌘↵ also opens its output), Esc closes.
  if (actPal && ["ArrowDown", "ArrowUp", "Enter", "Escape"].includes(e.key) && !e.isComposing) {
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") { actPal = null; renderMain(); return focusComposer(); }
    const n = actPal.shown.length;
    if (e.key === "Enter") { const id = actPal.shown[actPal.sel]; return id && actRun(actPal.pl, id, e.metaKey); }
    if (n) actPal.sel = (actPal.sel + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
    return renderMain();
  }
  if (actPop && e.key === "Escape") { e.stopPropagation(); actPop = null; return renderMain(); }
  const k = e.target.dataset?.text;
  if (!k?.startsWith("ac-") || e.key !== "Enter" || e.isComposing) return;
  // The prompt's box takes Enter as a new line; the edit sheet's fields save.
  e.stopPropagation();
  if (k === "ac-prompt") return;
  e.preventDefault();
  if (sheet === "act-edit") actSave();
}, true);
