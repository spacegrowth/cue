// Cue UI sessions — the Sessions view, the live pop-up and the + New session form.
/** Folders you've worked in, most recent first: live sessions, then answered history. */
function recentFolders() {
  const seen = new Map();
  const add = (cwd, at) => { if (cwd && !(seen.get(cwd) >= at)) seen.set(cwd, at || 0); };
  for (const s of state.sessions) add(s.cwd, s.since_ms);
  for (const q of state.live || []) add(q.cwd, q.since_ms);
  for (const h of state.history || []) add(h.cwd, h.created_ms);
  return [...seen.entries()].sort((a, b) => b[1] - a[1]).map(([cwd]) => cwd).slice(0, 12);
}
function livePop(rows) {
  // + New: just the form, not the sessions under it.
  if (newOpen) {
    liveShown = [];
    return `<div class="lv-pop nf-pop"><div class="nf-head"><span>New session</span><button class="nf-x" data-lv="cancel" aria-label="Close">×</button></div>${newForm()}</div>`;
  }
  const q = draft("find-live").text.trim().toLowerCase();
  const hay = (r) => { const ab = svAbout(r); return [r.name, r.cwd, ab.about, ab.now, (r.crew || crewOf(r.sid))?.lead_name].join(" ").toLowerCase(); };
  const hits = rows.filter((r) => !q || q.split(/\s+/).every((w) => hay(r).includes(w)));
  // By project: a repository's worktrees share its group, each row saying its branch.
  const groups = new Map();
  for (const r of hits) (groups.get(r.project) || groups.set(r.project, []).get(r.project)).push(r);
  const spread = (rs) => new Set(rs.map((r) => r.cwd)).size > 1;
  const row = (r) => {
    const ab = svAbout(r), unnamed = r.name === baseName(r.cwd) && (ab.about || ab.now);
    const name = unnamed ? ab.about || ab.now : r.name, tip = [ab.about, ab.now].filter(Boolean).join(" · ");
    const m = r.crew || crewOf(r.sid);
    // No Reply: the row opens it. Answer stays, for what's blocked on you.
    const main = r.it ? (r.st === "asks" ? `<button class="btn primary" data-sv="open" data-sid="${esc(r.sid)}">Answer</button>` : "")
      : !r.quiet || r.harness === "claude" ? `<button class="btn" data-sv="${r.quiet ? "quiet" : "open"}" data-sid="${esc(r.sid)}">Open</button>` : "";
    // The whole row opens its chat in Active (a quiet session has none in Cue yet: its tab instead).
    const sel = liveShown[liveSel]?.sid === r.sid ? "sel" : "";
    const viewing = r.sid === (quietOpen || active?.sid) ? "on" : "";
    return `<div class="lv-row ${sel} ${viewing}" role="button" data-sv="${r.quiet ? "quiet" : "open"}" data-sid="${esc(r.sid)}">${badge(r.harness)}${m ? `<span class="role ${m.role}">${m.role === "lead" ? "LEAD" : "EXEC"}</span>` : ""}<span class="lv-name">${esc(name)}</span>${r.where ? `<span class="sx-branch">${esc(r.where)}</span>` : ""}${newPill(r.sid)}${viewing ? VIEWING : ""}${svChipState(r)}${placeIcon(r.sid)}<span class="lv-acts">${main}</span></div>`;
  };
  const ordered = [...groups.entries()]
    .map(([p, rs]) => [p, rs.sort((a, b) => svRank(a) - svRank(b) || b.since - a.since)])
    .sort((a, b) => svRank(a[1][0]) - svRank(b[1][0]));
  liveShown = ordered.flatMap(([, rs]) => rs.map((r) => ({ sid: r.sid, act: r.quiet ? "quiet" : "open" })));
  liveSel = Math.min(Math.max(liveSel, 0), Math.max(liveShown.length - 1, 0));
  const list = ordered
    .map(([p, rs]) => {
      const many = spread(rs), cwd = many ? projRoot(rs[0].cwd) : rs[0].cwd;
      for (const r of rs) r.where = many ? state.branches?.[r.cwd] || baseName(r.cwd) : "";
      return `<div class="lv-group"><div class="lv-ghead">${projDot(p, rs.some((r) => r.st === "working"))}<span>${esc(p || "(no folder)")}</span><span class="sx-branch">${esc(many ? "" : state.branches?.[cwd] || "")}</span><button class="sx-plus" data-lv="new" data-cwd="${esc(cwd)}" title="New session in ${esc(homeless(cwd))}">+</button></div>${rs.map(row).join("")}</div>`;
    }).join("");
  return `<div class="lv-pop">
    <div class="lv-top"><label class="sv-search">${SEARCH_ICON}<input data-text="find-live" placeholder="Find a session…  ↑ ↓ Enter" spellcheck="false" autocomplete="off" value="${esc(draft("find-live").text)}"/></label><button class="btn primary" data-lv="new" title="New session (⌘N)">+ New</button></div>
    <div class="lv-list">${list || `<div class="quiet-line">${q ? `No live session matches “${esc(q)}”.` : "No live sessions."}</div>`}</div>
    <div class="lv-foot"><button data-lv="all">All sessions, as tiles →</button><span class="lv-keys"><kbd>↑</kbd><kbd>↓</kbd> move · <kbd>↵</kbd> open · <kbd>${liveSpot ? "⌘K" : "⌘L"}</kbd> open / close</span></div></div>`;
}
/** + New session: agent, folder (recent ones offered), an optional first message, and (Claude) a name. */
/** + New session: how much it may do without asking, per agent (Rust turns these into its flags). */
const PERMS = {
  claude: [["ask", "Ask"], ["auto", "Auto"], ["edits", "Accept edits"], ["plan", "Plan"], ["skip", "Skip all"]],
  codex: [["ask", "Ask"], ["edits", "Edits in folder"], ["skip", "Skip all"]],
  pi: [["ask", "Ask"], ["skip", "Skip all"]],
};
/** The permission you last picked for each agent stays picked for the next + New, until you change it. */
const savedPerm = (a) => { const p = remembered(`newPerm.${a}`); return PERMS[a]?.some(([v]) => v === p) ? p : "ask"; };
let newPerm = savedPerm(newAgent);
/** The model + New starts Claude Code on: one of Claude Code's own list ("default": its pick, no flag),
 *  and whether to ask for its 1M-token context (`opus[1m]`). Both remembered, like the other rows. */
let newModel = remembered("newModel") || "default";
// An early build saved its "Other…" marker as a model ("\uFFFDother"): drop that, wherever it's kept.
for (const k of ["newModel", "newModelOther"]) if (/^\uFFFD?other$/.test(remembered(k) || "")) { remember(k, k === "newModel" ? "default" : ""); if (k === "newModel") newModel = "default"; }
let newModel1m = remembered("newModel1m") === "true";
/** Claude Code's models, as last read: kept on this Mac until you press Reload (a new Claude Code, or a
 *  changed config, shows up then). Read once, the first time the row is drawn with nothing kept.
 *  SHORTCUT: this Mac's list also for a session on another machine; the aliases (opus, sonnet…) are
 *  the same there, dated ids may not be. Ask that machine's Claude Code (machine_check could carry
 *  its list) if one ever differs. */
let modelsKnown = (() => { try { return JSON.parse(remembered("models") || "null"); } catch { return null; } })();
let modelsReading = false;
function readModels(fresh) {
  if (modelsReading) return;
  modelsReading = true;
  // Reload forgets every folder's list in Cue too (one read serves both), so the "/" menus re-read as well.
  if (fresh) { cmdLists.clear(); cmdMissed.clear(); }
  const ask = (again) => invoke("models_list", { fresh: again ? false : fresh }).then((l) => {
    if (!l) return setTimeout(() => ask(true), 1500);   // still being read: a Claude Code start takes a moment
    modelsReading = false; modelsKnown = l; remember("models", JSON.stringify(l)); renderMain();
  }, () => { modelsReading = false; renderMain(); });
  ask(false);
}
/** You typed the model yourself ("Other…"): anything Claude Code's list doesn't name, a full id or a
 *  dated one, `claude-opus-5-5[1m]` included. An empty one is still "Other…", with nothing to pass yet. */
const modelIsOther = () => newModel !== "default" && !(modelsKnown || []).some((m) => m.value === newModel);
/** Its flag for the start line ("" for Default, or an Other left empty: Claude Code picks). The 1M
 *  context goes on as `[1m]`, unless the id you typed already carries a bracket. */
const modelFlag = () => newModel === "default" || !newModel ? "" : `--model ${newModel}${newModel1m && !/\[.*\]$/.test(newModel) ? "[1m]" : ""}`;
function modelRow() {
  if (!modelsKnown && !modelsReading) readModels(false);
  const list = modelsKnown, other = modelIsOther(), pick = other ? OTHER : newModel;
  const opts = [...(list || [{ value: "default", label: "Default" }]), { value: OTHER, label: "Other…" }];
  const cur = opts.find((m) => m.value === pick);
  const hint = modelsReading ? "asking Claude Code…" : !list ? "couldn't read Claude Code's list" : other ? (newModel ? "passed as --model, as typed" : "a full id, an alias, or one with [1m]") : cur?.desc || "";
  const typed = other ? `<input class="nf-in mono nf-model-id" data-text="new-model-id" placeholder="claude-opus-5-5[1m]" spellcheck="false" autocomplete="off" value="${esc(newModel)}"/>` : "";
  const big = !newModel || newModel === "default" || /\[.*\]$/.test(newModel) ? "" : `<div class="seg"><button class="${newModel1m ? "on" : ""}" data-lv="model-1m">1M context</button></div>`;
  const reload = `<button class="nf-reload" data-lv="models-reload" aria-label="Reload the model list" ${modelsReading ? "disabled" : ""}>${RELOAD_ICON}</button>`;
  return `<div class="nf-row"><span class="nf-k">Model</span><div class="nf-where"><select class="nf-in nf-sel" id="new-model" data-model aria-label="Model">${opts.map((m) => `<option value="${esc(m.value)}" ${m.value === pick ? "selected" : ""}>${esc(m.label || m.value)}</option>`).join("")}</select>${typed}${big}${reload}<span class="nf-hint">${esc(hint)}</span></div></div>`;
}
const OTHER = "other…";   // the drop-down's "Other…" entry's value: no model id has an ellipsis
const RELOAD_ICON = `<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9"/><path d="M13.5 2.5v3h-3"/></svg>`;
let newWhere = { true: true, false: false }[remembered("newWhere")] ?? null;   // tmux or not (null: Settings' default)
const newTmux = () => newWhere ?? !!state.settings?.sessions?.tmux;
const termName = () => state.settings?.sessions?.terminal || "Terminal";
/** + New session: which machine (this Mac, or one from + New → Machine). Remembered. */
let newMachine = (() => { try { return localStorage.getItem("cue.newMachine") || ""; } catch { return ""; } })();
/** + New: the "+ Add" field for a new machine is open. */
let nfAdding = false;
const setNewMachine = (m) => { newMachine = m; try { localStorage.setItem("cue.newMachine", m); } catch {} };
/** Folders you started sessions in on a machine, most recent first (this window keeps them). */
const machineFolders = (m) => { try { return JSON.parse(localStorage.getItem(`cue.folders.${m}`) || "[]"); } catch { return []; } };
const rememberFolder = (m, dir) => { try { localStorage.setItem(`cue.folders.${m}`, JSON.stringify([dir, ...machineFolders(m).filter((d) => d !== dir)].slice(0, 10))); } catch {} };
/** + New's Folder field: Cue's own list of recent folders (a native one can't be picked with ↑ ↓ Enter,
 *  since Enter starts the session, and a redraw closes it). Open: ▾, ↓, or typing (which narrows it). */
let nfFolderOpen = false, nfFolderQ = "", nfFolderSel = -1;
const folderMatches = (folders) => { const q = nfFolderQ.toLowerCase(); return q ? folders.filter((f) => f.toLowerCase().includes(q) && f !== nfFolderQ) : folders; };
const nfFolders = () => newMachine ? machineFolders(newMachine) : recentFolders().map(homeless);
function folderDrop(folders) {
  const m = nfFolderOpen ? folderMatches(folders) : [];
  return m.length ? `<div class="nf-drop">${m.map((f, i) => `<button class="nf-dopt mono ${i === nfFolderSel ? "on" : ""}" data-lv="folder" data-cwd="${esc(f)}">${esc(f)}</button>`).join("")}</div>` : "";
}
const closeFolders = () => { nfFolderOpen = false; nfFolderQ = ""; nfFolderSel = -1; };
function pickFolder(f) {
  draft("new-cwd").text = f;
  closeFolders();
  renderMain();
  const el = document.querySelector('[data-text="new-cwd"]');
  if (el) { el.focus(); el.setSelectionRange(f.length, f.length); }
}
/** ↓ / ↑ in the Folder field: open the list, move the highlight. */
function stepFolder(by) {
  if (!nfFolderOpen) { nfFolderOpen = true; nfFolderQ = ""; nfFolderSel = -1; }
  const n = folderMatches(nfFolders()).length;
  if (!n) return;
  nfFolderSel = nfFolderSel < 0 ? (by > 0 ? 0 : n - 1) : (nfFolderSel + by + n) % n;
  renderMain();
  document.querySelector(".nf-dopt.on")?.scrollIntoView({ block: "nearest" });
}
function newForm() {
  if (machinesList === null) loadMachines();
  if (newMachine && !(machinesList || []).some((m) => m.name === newMachine)) newMachine = machinesList === null ? newMachine : "";
  const remote = newMachine ? machineState.get(newMachine) : null;
  // On a machine: the agents its check found there (all three until it's been checked).
  const agents = newMachine ? ["claude", "codex", "pi"].filter((a) => !remote?.tools || remote.tools[a]) : agentsAvail || ["claude"];
  // Only once Cue knows what's installed: before that, the agent you last used stays picked.
  if ((newMachine ? remote?.tools : agentsAvail) && agents.length && !agents.includes(newAgent)) { newAgent = agents[0]; newPerm = savedPerm(newAgent); }
  const folders = nfFolders();
  const machines = machinesList || [];
  return `<div class="nf">
    <div class="nf-row"><span class="nf-k">Machine</span><div class="nf-where"><div class="seg">${[["", "This Mac"], ...machines.map((m) => [m.name, machShort(m.name)])].map(([v, l]) => `<button class="${newMachine === v && !nfAdding ? "on" : ""}" data-lv="machine" data-machine="${esc(v)}"${v ? ` title="${esc(machinesList.find((m) => m.name === v)?.host || v)}"` : ""}>${esc(l)}</button>`).join("")}<button class="${nfAdding ? "on" : ""}" data-lv="machine-add">+ Add</button></div>${extAddMenuHtml()}</div></div>
    ${nfAdding ? "" : machinePanel(newMachine)}
    ${nfAdding ? `<div class="nf-row nf-note"><span></span><div class="nf-addm"><div class="mc-add"><input class="nf-in mono" data-text="mc-host" placeholder="user@host or SSH alias" spellcheck="false" autocomplete="off" value="${esc(draft("mc-host").text)}"/><button class="btn primary small" data-mach="add">Add</button><button class="btn small" data-lv="machine-add">Cancel</button></div><span class="nf-hint wrap">Cue uses your own SSH (keys, agent, config) and never asks for a password. If it needs a login, you log in once in a terminal tab Cue opens. It installs nothing there; it sets up its hooks for the agents it finds.</span></div></div>` : ""}
    <div class="nf-row"><span class="nf-k">Agent</span><div class="seg">${agents.map((a) => `<button class="${newAgent === a ? "on" : ""}" data-lv="agent" data-agent="${a}">${esc(agentName(a))}</button>`).join("")}</div></div>
    <div class="nf-row"><span class="nf-k">Folder</span><div class="nf-fwrap"><input class="nf-in mono" data-text="new-cwd" placeholder="${newMachine ? `~/code/… on ${esc(machShort(newMachine))}` : "~/development/…"}" spellcheck="false" autocomplete="off" value="${esc(draft("new-cwd").text)}"/>${folders.length ? `<button class="nf-fbtn" data-lv="folders" aria-label="Recent folders">▾</button>` : ""}${folderDrop(folders)}</div></div>
    ${newAgent === "claude" ? modelRow() : ""}
    ${newAgent === "claude" ? `<div class="nf-row"><span class="nf-k">Name</span><input class="nf-in" data-text="new-name" placeholder="optional, e.g. fix-login" spellcheck="false" autocomplete="off" value="${esc(draft("new-name").text)}"/></div>` : ""}
    <div class="nf-row top"><span class="nf-k">Message</span><textarea class="nf-in" data-text="new-msg" rows="3" placeholder="optional: what it should start on">${esc(draft("new-msg").text)}</textarea></div>
    ${state.settings?.sessions?.tmux_installed && !newMachine ? `<div class="nf-row"><span class="nf-k">Runs in</span><div class="nf-where"><div class="seg">${[[true, "Cue"], [false, termName()]].map(([v, l]) => `<button class="${newTmux() === v ? "on" : ""} with-icon" data-lv="where" data-tmux="${v}">${v ? CUE_MARK_SM : TERM_ICON}${esc(l)}</button>`).join("")}</div><span class="nf-hint">${newTmux() ? "its terminal opens in Cue; it keeps running if Cue quits" : "a tab there, kept running by tmux underneath"}</span></div></div>` : ""}
    <div class="nf-row"><span class="nf-k">Permissions</span><div class="nf-where"><div class="seg">${PERMS[newAgent].map(([v, l]) => `<button class="${newPerm === v ? "on" : ""} ${v === "skip" ? "danger" : ""}" data-lv="perm" data-perm="${v}">${esc(l)}</button>`).join("")}</div></div></div>${newPerm === "skip" ? `<div class="nf-row nf-note"><span></span><span class="nf-hint warn">Runs anything without asking you</span></div>` : ""}
    <div class="nf-row"><span class="nf-k">Flags</span><input class="nf-in mono" data-text="new-flags" placeholder="optional, e.g. --effort high" spellcheck="false" autocomplete="off" value="${esc(draft("new-flags").text)}"/></div>
    <div class="nf-acts"><button class="btn" data-lv="cancel">Cancel</button><button class="btn primary" data-lv="start">${bangTyped(draft("new-msg").text) ? "Run" : `Start ${esc(agentName(newAgent))}`}</button></div>${(ranLog.get("new") || []).length ? `<div class="nf-ran">${(ranLog.get("new") || []).map((e, n) => ranCard("new", e, n)).join("")}</div>` : ""}</div>`;
}
/** ↓ / ↑ in the drop-down: move the highlight, keeping it in view. */
function stepLive(by) {
  if (!liveShown.length) return;
  liveSel = (liveSel + by + liveShown.length) % liveShown.length;
  renderMain();
  document.querySelector(".lv-row.sel")?.scrollIntoView({ block: "nearest" });
}
/** Enter: open the highlighted session's chat (a quiet one: its tab). */
function pickLive() {
  const r = liveShown[liveSel];
  if (!r) return;
  liveOpen = newOpen = false;
  return svAct(r.act, r.sid);
}
async function lvAct(act, d) {
  if (act === "toggle") {
    // Open the other way (⌘K's middle, or under the chip) moves it there rather than closing it.
    const spot = !!d.spot;
    liveOpen = !(liveOpen && liveSpot === spot);
    liveSpot = spot;
    liveSel = 0;
    newOpen = false;   // the live chip shows the sessions, never the New form
    renderMain();
    // Open: ready to type a name straight away.
    if (liveOpen) document.querySelector('[data-text="find-live"]')?.focus();
    return;
  }
  if (act === "all") { liveOpen = newOpen = false; view = "sessions"; return renderMain(); }
  if (act === "new") {
    if (d.spot) { sheet = lightbox = null; liveSpot = true; }   // the header's + New: in the middle, like ⌘N
    liveOpen = newOpen = true;
    draft("new-cwd").text = newMachine && !d.cwd ? draft("new-cwd").text || machineFolders(newMachine)[0] || "" : homeless(d.cwd || draft("new-cwd").text || machineFolders("")[0] || recentFolders()[0] || "");
    if (!agentsAvail) invoke("agents_installed").then((a) => { agentsAvail = a?.length ? a : ["claude"]; renderMain(); }, () => {});
    renderMain();
    return document.querySelector('[data-text="new-msg"]')?.focus();
  }
  if (act === "agent") { newAgent = d.agent; remember("newAgent", newAgent); newPerm = savedPerm(newAgent); return renderMain(); }
  if (act === "where") { newWhere = d.tmux === "true"; remember("newWhere", String(newWhere)); return renderMain(); }
  if (act === "machine-add") {
    if (nfAdding) { nfAdding = false; return renderMain(); }
    // Extensions with a window of their own are offered here too (a new container, say).
    if (extAddsAny()) { extAddMenu = !extAddMenu; return renderMain(); }
    nfAdding = true; renderMain(); return document.querySelector('[data-text="mc-host"]')?.focus();
  }
  if (act === "machine") { nfAdding = false; setNewMachine(d.machine || ""); draft("new-cwd").text = newMachine ? machineFolders(newMachine)[0] || "" : machineFolders("")[0] || homeless(recentFolders()[0] || ""); return renderMain(); }
  if (act === "perm") { newPerm = d.perm; remember(`newPerm.${newAgent}`, newPerm); return renderMain(); }
  if (act === "model-1m") { newModel1m = !newModel1m; remember("newModel1m", String(newModel1m)); return renderMain(); }
  if (act === "models-reload") { readModels(true); return renderMain(); }
  if (act === "folders") { const was = nfFolderOpen && !nfFolderQ; closeFolders(); nfFolderOpen = !was; renderMain(); return document.querySelector('[data-text="new-cwd"]')?.focus(); }
  if (act === "folder") return pickFolder(d.cwd);
  if (act === "cancel") { liveOpen = newOpen = false; closeFolders(); return renderMain(); }
  if (act === "start") {
    const cwd = draft("new-cwd").text.trim();
    if (!cwd) return toast("Pick a folder first");
    if (isBang(draft("new-msg").text)) { const line = draft("new-msg").text; draft("new-msg").text = ""; return runLine("new", line, cwd); }
    try {
      const typed = draft("new-flags").text.trim();
      const extra = newAgent === "claude" && !/(^|\s)--model(\s|=)/.test(typed) ? `${modelFlag()} ${typed}`.trim() : typed;
      const r = await invoke("new_session", { agent: newAgent, cwd, message: draft("new-msg").text, name: newAgent === "claude" ? draft("new-name").text : "", tmux: newTmux(), perm: newPerm, extra, machine: newMachine });
      rememberFolder(newMachine, cwd);
      toast(r.detail);
      draft("new-msg").text = draft("new-name").text = "";   // the rest stays as you set it, for the next one
      newOpen = liveOpen = false;
      // Cue knows the new session already (it picked the id): open it here. Its terminal stays closed
      // (the strip, or ⌃`) until you open it — the reply box comes first.
      if (r.session_id) { setActive(null, r.session_id); return; }
      renderMain();
    } catch (e) { toast(`Couldn't start it: ${e}`); }
  }
}
/** One entry per live session: the ones Cue knows, then the quiet ones (Claude's registry, Pi that only connected). */
function liveRows() {
  const rows = state.sessions.map((s) => {
    const it = state.items.find((i) => i.session_id === s.session_id && i.status === "pending");
    const st = it && it.kind !== "waiting" ? "asks" : it || s.state === "waiting" ? "yours" : s.state === "working" ? "working" : s.state;
    const what = s.trust_ms ? "Asking you to trust its folder" : s.compacting_ms && st !== "asks" ? "Compacting…" : st === "asks" ? plain(summary(it)) : st === "working" ? (s.doing || (s.prompt ? `› ${s.prompt}` : "")) : plain(firstLine(lastSaid(s)));
    return { sid: s.session_id, harness: s.harness, name: bareName(nameOf(s.session_id, s.project)), project: s.project || projOf(s.cwd), cwd: s.cwd || "", machine: s.machine || "", st, since: it?.created_ms ?? s.since_ms, what, run: st === "working" && !!s.doing, it, quiet: false };
  });
  for (const q of state.live || []) {
    rows.push({ sid: q.session_id, harness: q.harness, name: bareName(q.name) || baseName(q.cwd) || "session", project: projOf(q.cwd), cwd: q.cwd || "", machine: q.machine || "", st: q.status === "busy" ? "working" : "idle", since: q.since_ms, what: "", run: false, quiet: true });
  }
  markSeen(rows);
  return rows;
}
// A session that started a moment ago gets a "new" pill, so it's easy to find among the rest. Cue gets
// no start time, so it's when this window first saw it (kept across restarts); the ones already running
// the first time Cue looks don't count.
const NEW_FOR_MS = 2 * 60000;
let firstSeen = null;   // sid -> ms first seen (0: was there before Cue looked)
function markSeen(rows) {
  if (!state.live) return;   // the first snapshot isn't in yet
  if (!firstSeen) {
    let kept = null;
    try { kept = JSON.parse(localStorage.getItem("cue.firstSeen") || "null"); } catch {}
    firstSeen = new Map(Object.entries(kept || {}));
    if (!kept) for (const r of rows) firstSeen.set(r.sid, 0);
  }
  const t = now(), live = new Set(rows.map((r) => r.sid));
  let changed = false;
  for (const r of rows) if (!firstSeen.has(r.sid)) { firstSeen.set(r.sid, t); changed = true; setTimeout(renderMain, NEW_FOR_MS + 500); }
  for (const sid of firstSeen.keys()) if (!live.has(sid)) { firstSeen.delete(sid); changed = true; }
  if (changed) try { localStorage.setItem("cue.firstSeen", JSON.stringify(Object.fromEntries(firstSeen))); } catch {}
}
const isNewSession = (sid) => { const at = firstSeen?.get(sid); return !!at && now() - at < NEW_FOR_MS; };
const newPill = (sid) => isNewSession(sid) ? `<span class="new-pill">new</span>` : "";
const SV_ORDER = { asks: 0, yours: 1, working: 2, limited: 3, agent: 4, stopped: 5, idle: 6 };
const svRank = (r) => SV_ORDER[r.st] ?? 6;
/** Red tile: something there is blocked on you, or an executor's report reaches nobody. ("Your turn" isn't blocked.) */
const svHot = (r) => r.st === "asks" || (r.crew || crewOf(r.sid))?.lead_armed === false;
/** A chip's state, in words when it matters (asks you, your turn, reported…), else as a dot. */
function svChipState(r) {
  const m = r.crew || crewOf(r.sid);
  if (r.st === "asks") return `<span class="sx-st hot">asks you</span>`;
  if (r.st === "yours") return `<span class="sx-st">your turn</span>`;
  if (m?.role === "executor" && m.lead_armed === false) return `<span class="sx-st hot">orphaned</span>`;
  if (m?.role === "executor" && reported(m.status)) return `<span class="sx-st long">done${reviewWord(r.sid) ? ` · ${reviewWord(r.sid)}` : missedWake(r.sid) ? `, ${NOT_PICKED_UP}` : ", waiting for review"}</span>`;
  if (m?.role === "executor" && stalled(m.status)) return `<span class="sx-st hot">${stalledWord(m.status)}</span>`;
  if (r.quiet && r.st !== "working") return `<span class="sx-st">quiet</span>`;
  if (r.st === "limited") return `<span class="sx-st">out of usage</span>`;
  return `<i class="sx-dot ${r.st === "working" ? "busy" : "idle"}" title="${r.st === "working" ? "working" : "idle"}"></i>`;
}
/** What a session is about, without a model: an executor's packet goal; and your latest request to it
 *  (Claude's, else what Cue saw you send). */
function svAbout(r) {
  const m = r.crew || crewOf(r.sid), a = state.about?.[r.sid];
  const now = a?.prompt || sessionOf(r.sid)?.prompt || "";
  if (m?.role === "executor" && m.goal) return { label: "Goal", about: m.goal, now: "", outcome: m.outcome || "" };
  return { label: "About", about: "", now, outcome: "" };
}
/** Close: not while it's working or asking you (you'd lose the turn / the question), not a pi-lead lead that
 *  still has executors, not a session Cue can't reach. A relay lead's executors close with it. Two clicks:
 *  Close, Close? (Close all N? for a lead and its executors). */
let svCloseArm = null, svCloseTimer = null;
/** Close: first click arms it ("Close?", red), a second within 4 s closes. In Sessions and the chat's header. */
function closeBtn(sid) {
  const m = crewOf(sid);
  const what = m?.role === "executor" ? `Close it through ${m.plugin === "pilead" ? "pilead" : "relay"} (marks it closed, closes its tab)` : "End it and close its tab (its conversation is kept: claude --resume brings it back)";
  const n = m?.role === "lead" ? (m.executors || []).length : 0;
  return svCloseArm === sid
    ? `<button class="btn deny sx-close on" data-sv="close" data-sid="${esc(sid)}">${n ? `Close all ${n + 1}?` : "Close?"}</button>`
    : `<button class="btn sx-close" data-sv="close" data-sid="${esc(sid)}" title="Close: ${esc(what)}">Close</button>`;
}
function svClosable(r) {
  const m = r.crew || crewOf(r.sid);
  if (r.ghost || r.st === "working" || r.st === "asks") return false;
  if (m?.role === "lead" && m.plugin !== "relay" && (m.executors || []).length) return false;
  return true;
}
/** Executors left behind by a relay lead that isn't running: hand them all to a lead running in the same
 *  tile (same machine), or close them all (two clicks, as Close). */
function leftActs(c, t) {
  const first = c.execs[0], m = crewOf(first?.sid);
  if (c.m.lead_armed !== false || m?.plugin !== "relay") return "";
  const to = t.crews.find((x) => x.lead && x.m.plugin === "relay" && (x.lead.machine || "") === (first.machine || ""));
  const key = `left:${first.sid}`, n = c.execs.length;
  const close = svCloseArm === key
    ? `<button class="btn deny sx-close on" data-sv="close-left" data-sid="${esc(first.sid)}">${n > 1 ? `Close all ${n}?` : "Close?"}</button>`
    : `<button class="btn sx-close" data-sv="close-left" data-sid="${esc(first.sid)}">${n > 1 ? `Close all ${n}` : "Close"}</button>`;
  return `<span class="sx-acts">${to ? `<button class="btn" data-sv="hand:${esc(to.lead.sid)}" data-sid="${esc(first.sid)}">Hand to ${esc(bareName(to.m.name || to.lead.name))}</button>` : ""}${close}</span>`;
}
/** What you can do with a session, right on it: answer or reply when it needs you, the relay buttons,
 *  open it in Active, or go to its terminal. A quiet Claude session opens too (Cue takes it in); other quiet ones only have the tab. */
function svActs(r) {
  const m = r.crew || crewOf(r.sid), acts = [];
  // No Reply: the tile opens it. Answer stays, for what's blocked on you.
  if (r.it && r.st === "asks") acts.push(`<button class="btn primary" data-sv="open" data-sid="${esc(r.sid)}">Answer</button>`);
  if (m?.role === "executor" && reported(m.status)) acts.push(verifyTag(m.verify) + diffBtn(r.sid, m) + (m.lead_armed && !reviewWord(r.sid) ? crewBtn(r.sid, "review", "Review", true) : ""));
  if (!r.it && (!r.quiet || r.harness === "claude") && !r.ghost) acts.push(`<button class="btn" data-sv="${svOpenAct(r)}" data-sid="${esc(r.sid)}">Open</button>`);
  // A stalled or stopped executor Cue has no session for: the board tree's Close (through its lead's plugin).
  if (m?.role === "executor" && stalled(m.status) && r.ghost) acts.push(crewBtn(r.sid, "close", "Close"));
  else if (svClosable(r)) acts.push(closeBtn(r.sid));
  return acts.join("");
}
/** What opening a tile does: a remote executor's own terminal ("exec"), a quiet session taken in, or its chat. */
const svOpenAct = (r) => execTerm(r.sid) ? "exec" : r.quiet ? "quiet" : "open";
/** One session, everything visible: name and state, what it's about, what it's on now, and its buttons.
 *  A lead's line also says it leads (and how); its executors drop the agent badge the lead already shows. */
function svItem(r, { leadHarness = "", crew = null } = {}) {
  const m = r.crew || crewOf(r.sid), ab = svAbout(r);
  // No name of its own: it's called after its folder, which the tile already says (two such look like
  // duplicates). Call it by what it's about instead.
  const unnamed = !crew && r.name === baseName(r.cwd) && (ab.about || ab.now);
  const name = unnamed ? ab.about || ab.now : crew ? bareName(crew.name || r.name) : r.name;
  const lines = [];
  if (!unnamed && ab.about) lines.push(`<div class="sx-l about">${esc(ab.about)}</div>`);
  if (ab.outcome) lines.push(`<div class="sx-l out"><span class="sx-k">Outcome</span>${esc(ab.outcome)}</div>`);
  if (r.st === "working" && r.run) lines.push(`<div class="sx-l run">${esc(r.what)}</div>`);
  else if (r.st === "asks" && r.what) lines.push(`<div class="sx-l ask">${esc(r.what)}</div>`);
  else if (ab.now && !(unnamed && name === ab.now)) lines.push(`<div class="sx-l now">› ${esc(ab.now)}</div>`);
  // A lead that took over from one still open says so; the one it took over from says where the work went.
  const pred = crew && predecessorOf(r.sid);
  if (pred) lines.push(`<div class="sx-l">took over from the previous lead${crew.started_ms ? ` ${ago(crew.started_ms)} ago` : ""}</div>`);
  const ho = !m && handoffOf(r.sid);
  const hot = r.st === "asks" || m?.lead_armed === false;
  const leadTag = crew ? `<span class="role lead">LEAD</span>${autoToggle(r.sid, crew)}` : exLead(r.sid) ? `<span class="role was">EX-LEAD</span>` : "";
  // The whole tile opens it: its chat in Active (a quiet Claude session too: Cue takes it in), else what it's doing.
  // An executor on another machine in a tmux session of its own: its terminal there too (see openExec).
  const act = svOpenAct(r);
  const go = r.ghost && act !== "exec" ? "" : ` role="button" data-sv="${act}" data-sid="${esc(r.sid)}"`;
  const viewing = r.sid === (quietOpen || active?.sid) ? "on" : "";
  return `<div class="sx-item ${viewing} ${hot ? "hot" : ""} ${r.quiet ? "quiet" : ""} ${crew ? "lead" : ""} ${leadHarness ? "exec" : ""} ${ho ? "was" : ""}"${go}>
    <div class="sx-iline">${r.harness === leadHarness ? "" : badge(r.harness)}<span class="sx-name">${esc(name)}</span>${r.where ? `<span class="sx-branch">${esc(r.where)}</span>` : ""}${newPill(r.sid)}${viewing ? VIEWING : ""}${leadTag}${ho ? "" : svChipState(r)}${r.since && !r.quiet && !ho ? `<span class="sx-age">${ago(r.since)}</span>` : ""}<span class="sx-acts">${ho && !crewOf(ho.to) ? newLeadBtn(ho, "btn") : ""}${svActs(r)}</span></div>
    ${lines.join("")}</div>`;
}
const svItems = (rs, leadHarness = "") => rs.sort((a, b) => svRank(a) - svRank(b) || b.since - a.since).map((r) => svItem(r, { leadHarness })).join("");
/** Sessions-tab sections you folded (a machine, "parked", "closed"), kept across restarts; and the
 *  Parked / Closed lists you asked to see whole (else they show what fits on screen). */
const svFolded = (() => { try { return new Set(JSON.parse(localStorage.getItem("cue.svFolded") || "[]")); } catch { return new Set(); } })();
const svShowAll = new Set();
function svFold(key) {
  svFolded.has(key) ? svFolded.delete(key) : svFolded.add(key);
  try { localStorage.setItem("cue.svFolded", JSON.stringify([...svFolded])); } catch {}
  renderMain();
}
/** Parked and Closed show the rows that fit on screen below what's above them (at least one row of
 *  them), then "Show N more"; the live tiles always show whole. Runs after each render and on resize. */
function fitSvRest() {
  const sc = document.querySelector(".sv-scroll");
  if (!sc) return;
  const bottom = sc.getBoundingClientRect().bottom;
  for (const list of sc.querySelectorAll(".sx-rest[data-fit]")) {
    const rows = [...list.children], key = list.dataset.fit;
    rows.forEach((r) => r.classList.remove("sx-cut"));
    list.parentElement.querySelector(":scope > .sx-more")?.remove();
    if (svShowAll.has(key)) continue;
    const cols = getComputedStyle(list).gridTemplateColumns.split(" ").length || 1;
    const fits = rows.filter((r) => r.getBoundingClientRect().bottom <= bottom - 34).length;
    const keep = Math.max(cols, fits - (fits % cols || 0) || cols);
    if (keep >= rows.length) continue;
    rows.slice(keep).forEach((r) => r.classList.add("sx-cut"));
    const more = document.createElement("button");
    more.className = "sx-more";
    more.dataset.svAll = key;
    more.textContent = `Show ${rows.length - keep} more`;
    list.after(more);
  }
}
window.addEventListener("resize", () => { if (view === "sessions") fitSvRest(); });
/** "Show N more": the rest of the list shows, and the tab scrolls up once, smoothly, to bring them into
 *  view (as many as fit, the first one just under its pinned heading, never past the list's end). */
function showAllSvRest(btn) {
  const list = btn.previousElementSibling, first = list?.querySelector(".sx-cut");
  svShowAll.add(btn.dataset.svAll);
  fitSvRest();
  const sc = document.querySelector(".sv-scroll");
  if (!sc || !first) return;
  const box = sc.getBoundingClientRect(), head = list.parentElement.querySelector(".sx-sec-h")?.offsetHeight || 0;
  const toFirst = first.getBoundingClientRect().top - box.top - head - 8;
  const toEnd = list.getBoundingClientRect().bottom - box.bottom + 16;
  const by = Math.min(toFirst, toEnd);
  if (by > 0) sc.scrollBy({ top: by, behavior: "smooth" });
}
function sessionsView() {
  const all = liveRows().filter((r) => inProj(r.project));
  const q = draft("find-sessions").text.trim().toLowerCase();
  const hay = (r) => { const m = r.crew || crewOf(r.sid), ab = svAbout(r); return [r.name, r.cwd, homeless(r.cwd), state.branches?.[r.cwd], r.what, ab.about, ab.now, m?.lead_name, m?.name, r.harness].join(" ").toLowerCase(); };
  const match = (r) => !q || q.split(/\s+/).every((w) => hay(r).includes(w));
  const bySid = new Map(all.map((r) => [r.sid, r]));
  // Tiles by project (a repository and its worktrees). A crew lives in its lead's tile, whatever folders its
  // executors work in. Each machine its own tiles: the same project on two machines is two tiles.
  const tiles = new Map();
  const tile = (r) => { const p = r.project || projOf(r.cwd), k = `${r.machine || ""}\n${p}`; return tiles.get(k) || tiles.set(k, { project: p, cwd: r.cwd, machine: r.machine || "", solo: [], crews: [] }).get(k); };
  const placed = new Set();
  for (const lead of all.filter((r) => crewOf(r.sid)?.role === "lead")) {
    const m = crewOf(lead.sid);
    placed.add(lead.sid);
    const execs = (m.executors || []).map((e) => {
      placed.add(e.session_id);
      return bySid.get(e.session_id) || { sid: e.session_id, harness: m.plugin === "pilead" ? "pi" : "claude", name: e.name, cwd: "", st: e.status === "busy" ? "working" : "idle", since: 0, what: "", ghost: true,
        crew: { ...e, role: "executor", plugin: e.plugin || m.plugin, lead: lead.sid, lead_name: m.name, lead_armed: true } };
    });
    // The lead it took over from, while that one is still open: under this crew, not a tile of its own.
    const was = all.filter((r) => !placed.has(r.sid) && handoffOf(r.sid)?.to === lead.sid);
    was.forEach((r) => placed.add(r.sid));
    tile(lead).crews.push({ lead, m, execs, was });
  }
  // Executors whose lead Cue can't see (not running, or not in Cue yet): a crew line of their own, in their folder.
  const orphans = new Map();
  for (const r of all.filter((r) => !placed.has(r.sid) && crewOf(r.sid)?.role === "executor")) {
    placed.add(r.sid);
    const m = crewOf(r.sid), key = `${r.machine}\n${r.cwd}\n${m.lead}`;
    if (!orphans.has(key)) { orphans.set(key, { lead: null, m, execs: [] }); tile(r).crews.push(orphans.get(key)); }
    orphans.get(key).execs.push(r);
  }
  for (const r of all.filter((r) => !placed.has(r.sid))) tile(r).solo.push(r);

  const crewHtml = (c, t) => {
    const execs = c.execs.filter(match), was = (c.was || []).filter(match);
    const leadHit = c.lead && match(c.lead);
    if (q && !leadHit && !execs.length && !was.length) return "";
    const head = c.lead ? svItem(c.lead, { crew: c.m })
      : `<div class="sx-crew-h ${c.m.lead_armed === false ? "gone" : ""}">${badge(c.execs[0].harness)}<span class="sx-lname">${esc(bareName(c.m.lead_name) || "a lead")}</span><span class="sx-crew-n">${c.m.lead_armed === false ? `lead not running${c.execs.length > 1 ? ` · ${c.execs.length} executors still open` : ""}` : "lead not in Cue yet"}</span>${leftActs(c, t)}</div>`;
    return `<div class="sx-crew">${head}${execs.length ? `<div class="sx-execs">${svItems(execs, c.lead ? c.lead.harness : c.execs[0].harness)}</div>` : ""}${was.map((r) => svItem(r)).join("")}</div>`;
  };
  const cards = [...tiles.values()].map((t) => {
    const members = [...t.solo, ...t.crews.flatMap((c) => [c.lead, ...c.execs, ...(c.was || [])].filter(Boolean))];
    // One folder: its branch on the tile. Several (worktrees): each session says its own, and + starts in the main checkout.
    const folders = new Set(members.filter((r) => r.cwd).map((r) => r.cwd)), many = folders.size > 1;
    const cwd = many ? projRoot(t.cwd) : [...folders][0] || t.cwd, br = many ? "" : state.branches?.[cwd];
    for (const r of members) r.where = many && r.cwd ? state.branches?.[r.cwd] || baseName(r.cwd) : "";
    const solo = t.solo.filter(match);
    const crews = t.crews.map((c) => crewHtml(c, t)).filter(Boolean);
    if (!solo.length && !crews.length) return null;
    const rank = Math.min(...members.map(svRank)), latest = Math.max(0, ...members.map((r) => r.since || 0));
    const html = `<div class="sx-tile ${members.some(svHot) ? "hot" : ""}">
      <div class="sx-thead">${projDot(t.project, members.some((m) => m.st === "working"))}<span class="sx-tname">${esc(t.project || "(no folder)")}</span>${br ? `<span class="sx-branch">${esc(br)}</span>` : ""}<span class="sx-tcount">${members.length} session${members.length === 1 ? "" : "s"}</span><button class="sx-plus" data-lv="new" data-cwd="${esc(cwd)}" title="New session in ${esc(homeless(cwd))}">+</button></div>
      ${solo.length ? `<div class="sx-solo">${svItems(solo)}</div>` : ""}${crews.join("")}</div>`;
    return { machine: t.machine, rank, latest, html };
  }).filter(Boolean).sort((a, b) => a.rank - b.rank || b.latest - a.latest);
  const liveSids = new Set(all.map((r) => r.sid));
  const rest = [...(state.parked || []).map((p) => ({ ...p, kind: "parked", at: p.parked_ms })),
    ...(closedList || []).filter((c) => !(state.parked || []).some((p) => p.session_id === c.session_id)).map((c) => ({ ...c, kind: "closed", at: c.ended_ms }))]
    .filter((p) => !liveSids.has(p.session_id) && inProj(p.project) && match({ name: p.name || p.project, cwd: p.cwd, harness: p.harness }));
  if (!closedLoading && (!closedList || now() - closedAt > 60000)) queueMicrotask(loadClosed);   // what ended since, once a minute
  // Sections, each under a heading: This Mac, then each other machine (its live tiles), then Parked
  // and Closed (one slim line each, machine tag on the ones from elsewhere; Closed folds).
  const newest = (a, b) => b.at - a.at;
  const ps = rest.filter((p) => p.kind === "parked").sort(newest), cs = rest.filter((p) => p.kind === "closed").sort(newest), on = (sid) => sid === active?.sid;
  // Each heading folds its section (kept); a search shows everything.
  const sec = (key, label, n, body, cls = "") => {
    const open = !!q || !svFolded.has(key);
    return `<div class="sx-sec ${cls} ${open ? "" : "shut"}"><div class="sx-sec-h"><button class="sx-sec-fold" data-sv-fold="${esc(key)}" aria-expanded="${open}"><span class="sx-arrow">${open ? "▾" : "▸"}</span>${label}</button><span>${n}</span></div>${open ? body : ""}</div>`;
  };
  const machines = [...new Set(["", ...cards.map((c) => c.machine)])].sort((a, b) => (a !== "") - (b !== "") || machShort(a).localeCompare(machShort(b)));
  const liveSecs = machines.map((m) => {
    const mine = cards.filter((c) => c.machine === m), rs = all.filter((r) => r.machine === m);
    if (!mine.length && m) return "";
    const asks = rs.filter((r) => r.st === "asks").length, yours = rs.filter((r) => r.st === "yours").length;
    const n = [`${rs.length} live`, asks && `${asks} asks you`, yours && `${yours} your turn`].filter(Boolean).join(" · ");
    return sec(`m:${m}`, m ? esc(machShort(m)) : "This Mac", n, mine.length ? `<div class="sx-tiles">${mine.map((c) => c.html).join("")}</div>` : `<div class="quiet-line">${q ? `Nothing here matches “${esc(q)}”.` : "No live sessions on this Mac."}</div>`);
  }).join("");
  const restList = (key, rows) => `<div class="sx-rest" ${q ? "" : `data-fit="${key}"`}>${rows.map((r) => parkedRow(r, on(r.session_id))).join("")}</div>`;
  const restSecs = (ps.length ? sec("parked", "Parked", ps.length, restList("parked", ps), "sx-rest-sec") : "")
    + (cs.length ? sec("closed", "Closed", `${cs.length} · last 30 days`, restList("closed", cs), "sx-rest-sec") : "");

  const n = (st) => all.filter((r) => r.st === st).length, quiet = all.filter((r) => r.quiet).length;
  const counts = [`<b>${all.length} live</b>`, n("asks") && `${n("asks")} asks you`, n("yours") && `${n("yours")} your turn`, n("working") && `${n("working")} working`, quiet && `${quiet} quiet`].filter(Boolean).join(" · ");
  return `<div class="sv">
    <div class="sv-bar"><label class="sv-search">${SEARCH_ICON}<input data-text="find-sessions" placeholder="Find a session: name, folder, branch, what it's doing" spellcheck="false" autocomplete="off" value="${esc(draft("find-sessions").text)}"/></label>
      <button class="btn primary sv-new" data-lv="new">+ New session</button><span class="sv-count">${counts}</span></div>
    <div class="sv-scroll">${liveSecs}${restSecs}</div></div>`;
}
/** Two clicks for Close: the first arms it (for 4 s), the second goes. True on the second. */
function armClose(key) {
  clearTimeout(svCloseTimer);
  if (svCloseArm !== key) {
    svCloseArm = key;
    svCloseTimer = setTimeout(() => { svCloseArm = null; renderMain(); }, 4000);
    renderMain();
    return false;
  }
  svCloseArm = null;
  renderMain();
  return true;
}
async function svAct(act, sid) {
  // Executors whose lead is gone: closed together, or handed to a running lead (relay's adopt).
  if (act === "close-left") return armClose(`left:${sid}`) && crewAct(sid, "close-left");
  if (act.startsWith("hand:")) return crewAct(sid, act);
  if (act === "close") {
    if (!armClose(sid)) return;
    const was = sessionOf(sid);
    try {
      toast(await invoke("close_session", { sessionId: sid }));
      // Under Closed at once, as it was (its name with it), not after the list's next minutely read.
      if (was) closedList = [{ ...was, ended_ms: now() }, ...(closedList || []).filter((c) => c.session_id !== sid)];
    }
    catch (e) { toast(`Couldn't close it: ${e}`); }
    return;
  }
  if (act === "exec") return openExec(sid);
  if (act === "open") { const it = state.items.find((i) => i.session_id === sid && i.status === "pending"); return openSession(sid, it?.id || null); }
  if (act === "quiet") {
    // A Claude session Cue hasn't heard from (resumed after a restart, say): Cue takes it in, so it opens
    // like any other, with its chat and a box to type in. If it can't, what it can show read-only.
    if ((state.live || []).find((x) => x.session_id === sid)?.harness === "claude") {
      try { await invoke("adopt_session", { sessionId: sid }); setState(await invoke("get_state")); return setActive(null, sid); }
      catch (e) { toast(`Couldn't open it here: ${e}`); }
    }
    setActive(null, null); quietOpen = sid; loadSteps(sid); return renderMain();
  }
  if (act === "tab") {
    renderMain();
    try { toast(`Jumped to ${await invoke("focus_live", { sessionId: sid })}`); }
    catch (e) { toast(`Couldn't jump: ${e}`); }
  }
}
