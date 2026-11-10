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
  const hay = (r) => { const ab = svAbout(r); return [r.name, r.cwd, ab.about, ab.now].join(" ").toLowerCase(); };
  const hits = rows.filter((r) => !q || q.split(/\s+/).every((w) => hay(r).includes(w)));
  // By project: a repository's worktrees share its group, each row saying its branch.
  const groups = new Map();
  for (const r of hits) (groups.get(r.project) || groups.set(r.project, []).get(r.project)).push(r);
  const spread = (rs) => new Set(rs.map((r) => r.cwd)).size > 1;
  const row = (r) => {
    const ab = svAbout(r), unnamed = r.name === baseName(r.cwd) && (ab.about || ab.now);
    const name = unnamed ? ab.about || ab.now : r.name, tip = [ab.about, ab.now].filter(Boolean).join(" · ");
    // No Reply: the row opens it. Answer stays, for what's blocked on you.
    const main = r.it ? (r.st === "asks" ? `<button class="btn primary" data-sv="open" data-sid="${esc(r.sid)}">Answer</button>` : "")
      : !r.quiet || r.harness === "claude" ? `<button class="btn" data-sv="${r.quiet ? "quiet" : "open"}" data-sid="${esc(r.sid)}">Open</button>` : "";
    // The whole row opens its chat in Active (a quiet session has none in Cue yet: its tab instead).
    const sel = liveShown[liveSel]?.sid === r.sid ? "sel" : "";
    const viewing = r.sid === (quietOpen || active?.sid) ? "on" : "";
    return `<div class="lv-row ${sel} ${viewing}" role="button" data-sv="${r.quiet ? "quiet" : "open"}" data-sid="${esc(r.sid)}">${badge(r.harness)}<span class="lv-name">${esc(name)}</span>${r.where ? `<span class="sx-branch">${esc(r.where)}</span>` : ""}${newPill(r.sid)}${viewing ? VIEWING : ""}${svChipState(r)}${placeIcon(r.sid)}<span class="lv-acts">${main}</span></div>`;
  };
  const ordered = [...groups.entries()]
    .map(([p, rs]) => [p, rs.sort((a, b) => svRank(a) - svRank(b) || b.since - a.since)])
    .sort((a, b) => svRank(a[1][0]) - svRank(b[1][0]));
  liveShown = ordered.flatMap(([, rs]) => rs.map((r) => ({ sid: r.sid, act: r.quiet ? "quiet" : "open" })));
  // ↑ ↓ go on past the live sessions into what search found (as many as it showed last time).
  liveSel = Math.min(Math.max(liveSel, 0), Math.max(liveShown.length + (q ? searchVisible.length : 0) - 1, 0));
  const found = q ? searchSections(q, liveSel - liveShown.length) : "";
  const list = ordered
    .map(([p, rs]) => {
      const many = spread(rs), cwd = many ? projRoot(rs[0].cwd) : rs[0].cwd;
      for (const r of rs) r.where = many ? state.branches?.[r.cwd] || baseName(r.cwd) : "";
      return `<div class="lv-group"><div class="lv-ghead">${projDot(p, rs.some((r) => r.st === "working"))}<span>${esc(p || "(no folder)")}</span><span class="sx-branch">${esc(many ? "" : state.branches?.[cwd] || "")}</span><button class="sx-plus" data-lv="new" data-cwd="${esc(cwd)}" title="New session in ${esc(homeless(cwd))}">+</button></div>${rs.map(row).join("")}</div>`;
    }).join("");
  return `<div class="lv-pop">
    <div class="lv-top"><label class="sv-search">${SEARCH_ICON}<input data-text="find-live" placeholder="Find a session, or anything said…  ↑ ↓ Enter" spellcheck="false" autocomplete="off" value="${esc(draft("find-live").text)}"/></label><button class="btn primary" data-lv="new" title="New session (⌘N)">+ New</button></div>
    <div class="lv-list">${list || `<div class="quiet-line">${q ? `No live session matches “${esc(q)}”.` : "No live sessions."}</div>`}${found}</div>
    <div class="lv-foot"><button data-lv="all">All sessions, as tiles →</button><span class="lv-keys"><kbd>↑</kbd><kbd>↓</kbd> move · <kbd>↵</kbd> open · <kbd>${liveSpot ? "⌘K" : "⌘L"}</kbd> open / close</span></div></div>`;
}
/** + New session: agent, folder (recent ones offered), an optional first message (images pasted, dropped or
 *  picked; the mic dictates it), and (Claude) a name. */
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
/** Claude Code's models on each machine ("" this Mac), as last read there: each machine's Claude Code
 *  names them its own way (one behind a proxy may list `claude-opus-5-5[1m]`). Kept until you press
 *  Reload (a new Claude Code, or a changed config, shows up then). Read once per machine, the first
 *  time the row is drawn for it with nothing kept. */
const modelsKey = (m) => (m ? `models.${m}` : "models");
const modelsKept = new Map();
const modelsOf = (m) => {
  if (!modelsKept.has(m)) modelsKept.set(m, (() => { try { return JSON.parse(remembered(modelsKey(m)) || "null"); } catch { return null; } })());
  return modelsKept.get(m);
};
/** The machines whose list is being read now. */
const modelsReadingOn = new Set();
function readModels(fresh, machine = newMachine) {
  if (modelsReadingOn.has(machine)) return;
  modelsReadingOn.add(machine);
  // Reload has Cue read every folder's list again too (one read serves both), so the "/" menus re-read as well.
  if (fresh) { cmdLists.clear(); cmdMissed.clear(); }
  const ask = (again) => invoke("models_list", { fresh: again ? false : fresh, machine }).then(({ models: l, reading }) => {
    // Still being read (a Claude Code start takes a moment): the list Cue had, if any, until the new one's in.
    if (l) { modelsKept.set(machine, l); remember(modelsKey(machine), JSON.stringify(l)); }
    if (!l || reading) { renderMain(); return setTimeout(() => ask(true), 1500); }
    modelsReadingOn.delete(machine); renderMain();
  }, () => { modelsReadingOn.delete(machine); renderMain(); });
  ask(false);
}
/** You typed the model yourself ("Other…"): anything Claude Code's list doesn't name, a full id or a
 *  dated one, `claude-opus-5-5[1m]` included. An empty one is still "Other…", with nothing to pass yet. */
const modelIsOther = () => newModel !== "default" && !(modelsOf(newMachine) || []).some((m) => m.value === newModel);
/** Its flag for the start line ("" for Default, or an Other left empty: Claude Code picks). The 1M
 *  context goes on as `[1m]`, unless the id you typed already carries a bracket. */
const modelFlag = () => newModel === "default" || !newModel ? "" : `--model ${newModel}${newModel1m && !/\[.*\]$/.test(newModel) ? "[1m]" : ""}`;
function modelRow() {
  const modelsReading = modelsReadingOn.has(newMachine);
  if (!modelsOf(newMachine) && !modelsReading) readModels(false);
  const list = modelsOf(newMachine), other = modelIsOther(), pick = other ? OTHER : newModel;
  const opts = [...(list || [{ value: "default", label: "Default" }]), { value: OTHER, label: "Other…" }];
  const cur = opts.find((m) => m.value === pick);
  const hint = modelsReading ? "asking Claude Code…" : !list ? "couldn't read Claude Code's list" : other ? (newModel ? "passed as --model, as typed" : "a full id, an alias, or one with [1m]") : cur?.desc || "";
  const typed = other ? `<input class="nf-in mono nf-model-id" data-text="new-model-id" placeholder="claude-opus-5-5[1m]" spellcheck="false" autocomplete="off" value="${esc(newModel)}"/>` : "";
  const big = !newModel || newModel === "default" || /\[.*\]$/.test(newModel) ? "" : `<div class="seg"><button class="${newModel1m ? "on" : ""}" data-lv="model-1m">1M context</button></div>`;
  const reload = `<button class="nf-reload" data-lv="models-reload" aria-label="Reload the model list" ${modelsReading ? "disabled" : ""}>${RELOAD_ICON}</button>`;
  return `<div class="nf-row"><span class="nf-k">Model</span><div class="nf-where"><select class="nf-in nf-sel" id="new-model" data-model aria-label="Model">${opts.map((m) => `<option value="${esc(m.value)}" ${m.value === pick ? "selected" : ""}>${esc(m.label || m.value)}</option>`).join("")}</select>${typed}${big}${reload}<span class="nf-hint">${esc(hint)}</span></div></div>`;
}
/** + New's Runs in: hosted by Cue (Settings → Cue-hosted sessions on), Cue's own terminal (tmux installed), or a
 *  terminal tab. On a machine: hosted there by cue-host, or tmux there (Cue's terminal over SSH). Shown when there's
 *  a choice. */
function whereRow() {
  if (newMachine) return machineWhereRow();
  const tmux = !!state.settings?.sessions?.tmux_installed;
  if (!tmux && !hostedOffered()) return "";
  const opts = [...(hostedOffered() ? [["hosted", "Hosted"]] : []), ...(tmux ? [[true, "Cue"]] : []), [false, termName()]];
  const cur = newHosted() ? "hosted" : newTmux() && tmux;
  const hint = cur === "hosted" ? `no terminal: Cue talks to ${agentName(newAgent)} directly, and every ask is a card` : cur ? "its terminal opens in Cue; it keeps running if Cue quits" : tmux ? "a tab there, kept running by tmux underneath" : "a tab there";
  return `<div class="nf-row"><span class="nf-k">Runs in</span><div class="nf-where"><div class="seg">${opts.map(([v, l]) => `<button class="${cur === v ? "on" : ""} with-icon" data-lv="where" data-tmux="${v}">${v === false ? TERM_ICON : CUE_MARK_SM}${esc(l)}</button>`).join("")}</div><span class="nf-hint">${hint}</span></div></div>`;
}
/** Runs in, on a machine: hosted there (cue-host, which Cue puts there) or tmux there. */
function machineWhereRow() {
  if (!hostedOffered()) return "";
  const cur = newHosted() ? "hosted" : true;
  const hint = cur === "hosted" ? `no terminal: it runs on that machine under cue-host (Cue puts it in ~/.cue there), every ask a card, and it carries on if Cue quits or the connection drops` : "in tmux there, shown in Cue's terminal";
  return `<div class="nf-row"><span class="nf-k">Runs in</span><div class="nf-where"><div class="seg">${[["hosted", "Hosted"], [true, "Cue"]].map(([v, l]) => `<button class="${cur === v ? "on" : ""} with-icon" data-lv="where" data-tmux="${v}">${CUE_MARK_SM}${esc(l)}</button>`).join("")}</div><span class="nf-hint">${hint}</span></div></div>`;
}
const OTHER = "other…";   // the drop-down's "Other…" entry's value: no model id has an ellipsis
const RELOAD_ICON = `<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9"/><path d="M13.5 2.5v3h-3"/></svg>`;
let newWhere = { true: true, false: false, hosted: "hosted" }[remembered("newWhere")] ?? null;   // tmux or not, or hosted (null: Settings' default)
/** + New can start a Claude Code, Pi or Codex session hosted by Cue, with no terminal: Settings → Cue-hosted sessions
 *  on, on this Mac or a machine (there by cue-host). */
const hostedOffered = () => !!state.settings?.hosted?.on && ["claude", "pi", "codex"].includes(newAgent);
const newHosted = () => hostedOffered() && newWhere === "hosted";
const newTmux = () => (newWhere === "hosted" || newWhere == null ? !!state.settings?.sessions?.tmux : newWhere);
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
/** + New's Project line: the typed folder's project as Cue works it out (project_for_folder: {name, own, how,
 *  key}), read again when the folder or machine changes; the one you picked instead (null: as it is), kept
 *  for its folder when the session starts; and whether the pick list is open. */
let nfProj = { at: "", info: null, pick: null, open: false };
/** A project you made, picked on the Project line: the session started from this + New goes in it alone. */
let nfNewProj = "";
const NF_HOW = { repo: "its git repository", "repo-says": "its repository says so", put: "you put this folder in it", folder: "this folder" };
function projLine() {
  if (nfNewProj) return `<div class="nf-row"><span class="nf-k">Project</span><div class="nf-fwrap nf-proj">${projDot(nfNewProj)}<b>${esc(nfNewProj)}</b><span class="nf-hint">this session only</span><button class="btn small" data-lv="proj-new-off">Change…</button></div></div>`;
  const cwd = draft("new-cwd").text.trim(), at = `${newMachine}|${cwd}`;
  // (On a machine "~" is that machine's home, which Cue only knows once a session's there.)
  if (!(newMachine ? /^\//.test(cwd) : /^[/~]/.test(cwd))) return "";
  if (nfProj.at !== at) {
    nfProj = { at, info: null, pick: null, open: false };
    invoke("project_for_folder", { folder: cwd, machine: newMachine }).then((v) => { if (nfProj.at === at) { nfProj.info = v; renderMain(); } }, () => {});
  }
  const i = nfProj.info;
  if (!i) return "";
  const name = nfProj.pick ?? i.name, how = nfProj.pick != null && nfProj.pick !== i.name ? (nfProj.pick === i.own ? "its own again, from now on" : "every session in this folder, from now on") : NF_HOW[i.how] || "";
  const names = [...new Set([i.own, ...projPins(), ...projectsKnown().map(([p]) => p), ...(state.history || []).map((h) => h.project)])].filter((p) => p && p !== name).slice(0, 12);
  const drop = nfProj.open ? `<div class="nf-drop nf-pdrop">${names.map((p) => `<button class="nf-dopt" data-lv="proj-pick" data-project="${esc(p)}">${projDot(p)}${esc(projLabel(p))}${p === i.own ? `<span class="nf-hint">its own</span>` : ""}</button>`).join("") || `<div class="nf-hint">No other projects yet</div>`}</div>` : "";
  const change = i.how === "folder" && nfProj.pick == null ? "Put in a project…" : "Change…";
  return `<div class="nf-row"><span class="nf-k">Project</span><div class="nf-fwrap nf-proj">${projDot(name)}<b>${esc(projLabel(name))}</b><span class="nf-hint">${esc(how)}</span><button class="btn small" data-lv="proj-open" aria-expanded="${nfProj.open}">${change}</button>${drop}</div></div>`;
}
/** Folders on this Mac under the path being typed (folders_under), for the text they were read for. */
let nfDisk = { q: null, list: [] };
const typedPath = (q) => !newMachine && /^(~|\/)/.test(q) && q.includes("/");
function diskFolders(q) {
  if (!typedPath(q)) return [];
  if (nfDisk.q !== q) {
    nfDisk = { q, list: nfDisk.list };
    invoke("folders_under", { typed: q }).then((l) => { if (nfDisk.q === q) { nfDisk.list = l || []; renderMain(); } }, () => {});
  }
  // The last list read, while the new one comes (each keystroke reads again): only what still fits what's typed.
  return nfDisk.list.filter((f) => f.toLowerCase().startsWith(q.toLowerCase()) && f !== q);
}
/** The Folder list: your recent folders that contain what's typed, then the folders on disk under it. */
const folderMatches = (folders) => {
  const q = nfFolderQ.toLowerCase();
  const recent = q ? folders.filter((f) => f.toLowerCase().includes(q) && f !== nfFolderQ) : folders;
  return [...new Set([...recent, ...diskFolders(nfFolderQ)])];
};
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
/** Tab in the Folder field: the highlighted folder (else the first), with a "/" so its own folders come next. */
function completeFolder() {
  const m = folderMatches(nfFolders()), f = m[nfFolderSel] || m[0];
  if (!f) return false;
  const t = `${f.replace(/\/+$/, "")}/`;
  draft("new-cwd").text = nfFolderQ = t;
  nfFolderOpen = true; nfFolderSel = -1;
  renderMain();
  const el = document.querySelector('[data-text="new-cwd"]');
  if (el) { el.focus(); el.setSelectionRange(t.length, t.length); }
  return true;
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
  const acts = `<div class="nf-acts">${nfFull ? "" : `<button class="btn" data-lv="nf-more">All options</button><span class="grow"></span>`}<button class="btn" data-lv="cancel">Cancel</button><button class="btn primary" data-lv="start">${bangTyped(draft("new-msg").text) ? "Run" : `Start ${esc(agentName(newAgent))}`}</button></div>`;
  const ran = (ranLog.get("new") || []).length ? `<div class="nf-ran">${(ranLog.get("new") || []).map((e, n) => ranCard("new", e, n)).join("")}</div>` : "";
  // The short form: a name, then what it should start on (images pasted, dropped or picked), both in the same large type.
  if (!nfFull) return `<div class="nf"><input class="nf-in nf-name" data-text="new-name" placeholder="Name it, or just press ↵" spellcheck="false" autocomplete="off" value="${esc(draft("new-name").text)}"/><div class="nf-msg nf-pmsg" data-drop="new-msg"><div class="nf-fwrap"><textarea class="nf-in nf-prompt" data-text="new-msg" rows="1" spellcheck="false" placeholder="What should it start on? (optional)">${esc(draft("new-msg").text)}</textarea><button class="icon-btn nf-attach" data-attach="new-msg" aria-label="Add image">${IMAGE_ICON}</button></div>${draftChips("new-msg")}</div>${nfRecentLine(folders)}<div class="nf-sum">${esc(nfSummary(false))}</div>${ran}</div>${acts}`;
  return `<div class="nf">
    <div class="nf-row"><span class="nf-k">Machine</span><div class="nf-where"><div class="seg">${[["", "This Mac"], ...machines.map((m) => [m.name, machShort(m.name)])].map(([v, l]) => `<button class="${newMachine === v && !nfAdding ? "on" : ""}" data-lv="machine" data-machine="${esc(v)}"${v ? ` title="${esc(machinesList.find((m) => m.name === v)?.host || v)}"` : ""}>${esc(l)}</button>`).join("")}<button class="${nfAdding ? "on" : ""}" data-lv="machine-add">+ Add</button></div>${extAddMenuHtml()}</div></div>
    ${nfAdding ? "" : machinePanel(newMachine)}
    ${nfAdding ? `<div class="nf-row nf-note"><span></span><div class="nf-addm"><div class="mc-add"><input class="nf-in mono" data-text="mc-host" placeholder="user@host or SSH alias" spellcheck="false" autocomplete="off" value="${esc(draft("mc-host").text)}"/><button class="btn primary small" data-mach="add">Add</button><button class="btn small" data-lv="machine-add">Cancel</button></div><span class="nf-hint wrap">Cue uses your own SSH (keys, agent, config) and never asks for a password. If it needs a login, you log in once in a terminal tab Cue opens. It installs nothing there but its hooks for the agents it finds, and, if you host a session there, one small program of its own (cue-host, in ~/.cue).</span></div></div>` : ""}
    <div class="nf-row"><span class="nf-k">Agent</span><div class="seg">${agents.map((a) => `<button class="${newAgent === a ? "on" : ""}" data-lv="agent" data-agent="${a}">${esc(agentName(a))}</button>`).join("")}</div></div>
    <div class="nf-row"><span class="nf-k">Folder</span><div class="nf-fwrap"><input class="nf-in mono" data-text="new-cwd" placeholder="${newMachine ? `~/code/… on ${esc(machShort(newMachine))}` : "~/development/…"}" spellcheck="false" autocomplete="off" value="${esc(draft("new-cwd").text)}"/>${folders.length ? `<button class="nf-fbtn" data-lv="folders" aria-label="Recent folders">▾</button>` : ""}${folderDrop(folders)}</div></div>
    ${projLine()}
    ${newAgent === "claude" && !newMachine && draft("new-cwd").text.trim() ? `<div class="nf-row"><span class="nf-k">MCP</span><div class="nf-mcp">${mcpList(`f:${draft("new-cwd").text.trim()}`, true)}</div></div>` : ""}
    ${newAgent === "claude" ? modelRow() : ""}
    ${`<div class="nf-row"><span class="nf-k">Name</span><input class="nf-in" data-text="new-name" placeholder="optional, e.g. fix-login" spellcheck="false" autocomplete="off" value="${esc(draft("new-name").text)}"/></div>`}
    <div class="nf-row top"><span class="nf-k">Message</span><div class="nf-msg" data-drop="new-msg"><div class="nf-fwrap"><textarea class="nf-in" data-text="new-msg" rows="3" placeholder="optional: what it should start on (paste or drop images too)">${esc(draft("new-msg").text)}</textarea><button class="icon-btn nf-attach" data-attach="new-msg" aria-label="Add image">${IMAGE_ICON}</button><button class="icon-btn nf-attach nf-mic ${dictating?.key === "new-msg" ? "on" : ""}" data-mic="new-msg" aria-label="${dictating?.key === "new-msg" ? "Stop dictating" : "Dictate"}">${MIC_ICON}</button></div>${draftChips("new-msg")}</div></div>
    ${whereRow()}
    <div class="nf-row"><span class="nf-k">Permissions</span><div class="nf-where"><div class="seg">${PERMS[newAgent].map(([v, l]) => `<button class="${newPerm === v ? "on" : ""} ${v === "skip" ? "danger" : ""}" data-lv="perm" data-perm="${v}">${esc(l)}</button>`).join("")}</div></div></div>${newPerm === "skip" ? `<div class="nf-row nf-note"><span></span><span class="nf-hint warn">Runs anything without asking you</span></div>` : ""}
    <div class="nf-row"><span class="nf-k">Flags</span><input class="nf-in mono" data-text="new-flags" placeholder="optional, e.g. --effort high" spellcheck="false" autocomplete="off" value="${esc(draft("new-flags").text)}"/></div>
    ${ran}</div>${acts}`;
}
/** + New opens short: a name and a first message, started with what you picked last time (this line says what that is).
 *  All options shows the whole form. */
let nfFull = false;
/** The short form's folder line: "In" and your three most recent folders (on the machine picked), the one it'll
 *  use filled, one click each; … opens All options at the folder field. */
function nfRecentLine(folders) {
  const cur = draft("new-cwd").text.trim();
  // In the order you last used them, so a click never moves them; one typed in All options comes first.
  const top = folders.slice(0, 3), three = !cur || top.includes(cur) ? top : [cur, ...top].slice(0, 3);
  if (!three.length) return "";
  const pill = (f) => `<button class="nf-rf ${f === cur ? "on" : ""}" data-lv="folder-quick" data-cwd="${esc(f)}">${projDot(projectAtFolder(f))}${esc(baseName(f))}</button>`;
  return `<div class="nf-recent"><span class="nf-k">In</span>${three.map(pill).join("")}<button class="nf-rf" data-lv="folder-more" aria-label="Another folder">…</button></div>`;
}
/** A folder's project, as Cue last had it for a session there (its own name if none). */
function projectAtFolder(f) {
  const same = (x) => x?.cwd && homeless(x.cwd) === homeless(f);
  return (state.sessions.find(same) || (state.live || []).find(same) || (state.history || []).find(same))?.project || baseName(f);
}
function nfSummary(withFolder = true) {
  const where = newMachine ? `${newHosted() ? "Hosted on " : ""}${machShort(newMachine)}` : newHosted() ? "Hosted" : newTmux() && state.settings?.sessions?.tmux_installed ? "in Cue" : termName();
  const model = newAgent !== "claude" ? "" : newModel === "default" || !newModel ? "" : `${(modelsOf(newMachine) || []).find((m) => m.value === newModel)?.label || newModel}${newModel1m && !/\[.*\]$/.test(newModel) ? " 1M" : ""}`;
  const perm = PERMS[newAgent]?.find(([v]) => v === newPerm)?.[1] || "";
  return [agentName(newAgent), withFolder && (draft("new-cwd").text.trim() || "no folder yet"), model, where, perm && `${perm}${newPerm === "skip" ? " (runs anything without asking)" : ""}`].filter(Boolean).join(" · ");
}
/** ↓ / ↑ in the drop-down: move the highlight, keeping it in view. */
function stepLive(by) {
  const n = liveShown.length + (draft("find-live").text.trim() ? searchVisible.length : 0);
  if (!n) return;
  liveSel = (liveSel + by + n) % n;
  renderMain();
  document.querySelector(".lv-row.sel, .lv-pop .sr.sel")?.scrollIntoView({ block: "nearest" });
}
/** Enter: open the highlighted session's chat (a quiet one: its tab), or what search found. */
function pickLive() {
  if (liveSel >= liveShown.length) return pickResult(searchVisible[liveSel - liveShown.length]);
  const r = liveShown[liveSel];
  if (!r) return;
  liveOpen = newOpen = false;
  return svAct(r.act, r.sid);
}
/** ⌘K, ⌘F, / and the magnifier: the one place to find a session, live or not, or anything said in one. */
function openFind() {
  sheet = lightbox = null;
  lvAct("toggle", { spot: true });
}
/** The live list opened or closed. Open the other way (⌘K's middle, or under the chip) moves it there rather than
 *  closing it; it shows the sessions, never the New form. Text left from last time is searched again, so what's
 *  found is as it is now; open, it's ready to type a name straight away. */
function lvToggle(d) {
  const spot = !!d.spot;
  liveOpen = !(liveOpen && liveSpot === spot);
  liveSpot = spot;
  liveSel = 0;
  newOpen = false;
  if (liveOpen && draft("find-live").text.trim()) return runSearch(draft("find-live").text), document.querySelector('[data-text="find-live"]')?.focus();
  renderMain();
  if (liveOpen) document.querySelector('[data-text="find-live"]')?.focus();
}
/** + New opened (from the header: in the middle, like ⌘N). On a project you made (one with no folder of its own),
 *  the new session goes in it, as Move to project would. */
function lvNew(d) {
  if (d.spot) { sheet = lightbox = null; liveSpot = true; }
  liveOpen = newOpen = true;
  nfFull = false;
  nfNewProj = projFilter && madeProject(projFilter) ? projFilter : "";
  draft("new-cwd").text = newMachine && !d.cwd ? draft("new-cwd").text || machineFolders(newMachine)[0] || "" : homeless(d.cwd || draft("new-cwd").text || machineFolders("")[0] || recentFolders()[0] || "");
  if (!agentsAvail) invoke("agents_installed").then((a) => { agentsAvail = a?.length ? a : ["claude"]; renderMain(); }, () => {});
  renderMain();
  return document.querySelector('[data-text="new-name"]')?.focus();
}
/** + New → + Add machine: its field, or the extensions with a window of their own (a new container, say). */
function lvMachineAdd() {
  if (nfAdding) { nfAdding = false; return renderMain(); }
  if (extAddsAny()) { extAddMenu = !extAddMenu; return renderMain(); }
  nfAdding = true; renderMain(); return document.querySelector('[data-text="mc-host"]')?.focus();
}
/** After + New started a session: its project (a project you made: this session alone, and the window goes to
 *  it; else a project picked on its Project line: its folder's from now on, every session there), the form
 *  emptied but for what you set, and the session open here. Cue knows it already (it picked the id); its terminal
 *  stays closed (the strip above the box) until you open it: the reply box comes first. */
function afterNewSession(r) {
  const i = nfProj.info;
  if (nfNewProj && r.session_id) {
    moveToProject(r.session_id, nfNewProj);
    setProjFilter(nfNewProj);
  } else if (i && nfProj.pick != null && nfProj.pick !== i.name) putFolder(i.key, nfProj.pick, i.own);
  nfProj = { at: "", info: null, pick: null, open: false };
  nfNewProj = "";
  toast(r.detail);
  draft("new-msg").text = draft("new-name").text = "";
  draft("new-msg").images = [];
  newOpen = liveOpen = false;
  if (r.session_id) { setActive(null, r.session_id); return; }
  renderMain();
}
/** + New's Start: a "!" line runs in the folder; else the session starts, with the MCP servers you picked for it
 *  (only once you changed a box; else its folder's, unpinned). */
async function lvStart(_d, trust = false) {
  const cwd = draft("new-cwd").text.trim().replace(/(.)\/+$/, "$1");
  if (!cwd) { nfFull = true; renderMain(); return toast("Pick a folder first"); }
  endDictation("new-msg");
  if (isBang(draft("new-msg").text)) { const line = draft("new-msg").text; draft("new-msg").text = ""; return runLine("new", line, cwd); }
  try {
    const typed = draft("new-flags").text.trim();
    const extra = newAgent === "claude" && !/(^|\s)--model(\s|=)/.test(typed) ? `${modelFlag()} ${typed}`.trim() : typed;
    const mk = `f:${cwd}`;
    const mcp = newAgent === "claude" && !newMachine && mcpPick.has(mk) ? [...mcpPick.get(mk)] : null;
    const r = await invoke("new_session", { agent: newAgent, cwd, message: draft("new-msg").text, name: draft("new-name").text, tmux: newTmux(), perm: newPerm, extra, machine: newMachine, mcp, images: draft("new-msg").images, hosted: newHosted(), trust });
    mcpPick.delete(mk);
    rememberFolder(newMachine, cwd);
    afterNewSession(r);
  } catch (e) {
    // Hosted Claude Code in a folder it hasn't been trusted in: Cue asks, as its terminal would (ipc/hosted.rs).
    if (String(e).startsWith(UNTRUSTED)) { trustAsk = { dir: String(e).slice(UNTRUSTED.length), machine: newMachine }; return renderMain(); }
    toast(`Couldn't start it: ${e}`);
  }
}
/** Hosted Claude Code never asks "Do you trust the files in this folder?" (it has no terminal), so Cue does, before
 *  its first session in a folder: Trust and start, or not. */
const UNTRUSTED = "untrusted folder: ";
let trustAsk = null;   // { dir, machine }
function trustDialog() {
  const t = trustAsk, where = t.machine ? ` on ${esc(machShort(t.machine))}` : "";
  return `<div class="move-scrim"><div class="move-dialog end-dialog" role="alertdialog" aria-labelledby="trust-title">
    <div class="move-title" id="trust-title">Trust this folder?</div>
    <div class="end-text"><span class="mono">${esc(homeless(t.dir))}</span>${where}. Claude Code will read, edit and run files in it, and a folder's own settings can run commands. In a terminal it asks this first; hosted, it has no terminal to ask in, so Cue asks, once per folder.</div>
    <div class="row-btns"><span class="grow"></span><button class="btn" data-act="trust-cancel">Cancel <kbd>Esc</kbd></button><button class="btn primary" data-act="trust-start">Trust and start</button></div>
  </div></div>`;
}
function trustAnswer(yes) {
  trustAsk = null;
  renderMain();
  if (yes) lvStart(null, true);
}
/** The live list's and + New's buttons (data-lv), by name, each with the button's data. */
const LV_ACTS = {
  toggle: lvToggle,
  all: () => { liveOpen = newOpen = false; view = "sessions"; renderMain(); },
  new: lvNew,
  agent: (d) => { newAgent = d.agent; remember("newAgent", newAgent); newPerm = savedPerm(newAgent); renderMain(); },
  where: (d) => { newWhere = d.tmux === "hosted" ? "hosted" : d.tmux === "true"; remember("newWhere", String(newWhere)); renderMain(); },
  "machine-add": lvMachineAdd,
  machine: (d) => { nfAdding = false; setNewMachine(d.machine || ""); draft("new-cwd").text = newMachine ? machineFolders(newMachine)[0] || "" : machineFolders("")[0] || homeless(recentFolders()[0] || ""); renderMain(); },
  perm: (d) => { newPerm = d.perm; remember(`newPerm.${newAgent}`, newPerm); renderMain(); },
  "model-1m": () => { newModel1m = !newModel1m; remember("newModel1m", String(newModel1m)); renderMain(); },
  "models-reload": () => { readModels(true); renderMain(); },
  folders: () => { const was = nfFolderOpen && !nfFolderQ; closeFolders(); nfFolderOpen = !was; renderMain(); document.querySelector('[data-text="new-cwd"]')?.focus(); },
  folder: (d) => pickFolder(d.cwd),
  "proj-open": () => { nfProj.open = !nfProj.open; renderMain(); },
  // A project you made in the projects list: this session alone goes in it (a repository's: the whole folder).
  "proj-pick": (d) => { if (madeList().includes(d.project)) nfNewProj = d.project; else nfProj.pick = d.project; nfProj.open = false; renderMain(); },
  "proj-new-off": () => { nfNewProj = ""; renderMain(); },
  cancel: () => { endDictation("new-msg"); liveOpen = newOpen = false; nfNewProj = ""; closeFolders(); renderMain(); },
  start: lvStart,
  "folder-quick": (d) => { draft("new-cwd").text = d.cwd; renderMain(); document.querySelector('[data-text="new-name"]')?.focus(); },
  "folder-more": () => { nfFull = true; closeFolders(); nfFolderOpen = true; renderMain(); document.querySelector('[data-text="new-cwd"]')?.focus(); },
  "nf-more": () => { nfFull = true; renderMain(); document.querySelector('[data-text="new-name"]')?.focus(); },
};
async function lvAct(act, d) {
  if (Object.hasOwn(LV_ACTS, act)) return LV_ACTS[act](d);
}
/** One entry per live session: the ones Cue knows, then the quiet ones (Claude's registry, Pi that only connected). */
function liveRows() {
  const rows = state.sessions.map((s) => {
    const it = state.items.find((i) => i.session_id === s.session_id && i.status === "pending");
    const st = it && it.kind !== "waiting" ? "asks" : it || s.state === "waiting" ? "yours" : s.state === "working" ? "working" : s.state;
    const what = s.trust_ms ? "Asking you to trust its folder" : s.compacting_ms && st !== "asks" ? "Compacting…" : st === "asks" ? plain(summary(it)) : st === "working" ? (s.doing || (s.prompt ? `› ${s.prompt}` : "")) : plain(firstLine(lastSaid(s)));
    return { sid: s.session_id, harness: s.harness, name: nameOf(s.session_id, s.project), project: s.project || projOf(s.cwd), cwd: s.cwd || "", machine: s.machine || "", st, since: it?.created_ms ?? s.since_ms, what, run: st === "working" && !!s.doing, it, quiet: false };
  });
  for (const q of state.live || []) {
    rows.push({ sid: q.session_id, harness: q.harness, name: q.name || baseName(q.cwd) || "session", project: q.project || projOf(q.cwd), cwd: q.cwd || "", machine: q.machine || "", st: q.status === "busy" ? "working" : "idle", since: q.since_ms, what: "", run: false, quiet: true });
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
/** Red tile: something there is blocked on you. ("Your turn" isn't blocked.) */
const svHot = (r) => r.st === "asks";
/** A chip's state, in words when it matters (asks you, your turn…), else as a dot. */
function svChipState(r) {
  if (r.st === "asks") return `<span class="sx-st hot">asks you</span>`;
  if (r.st === "yours") return `<span class="sx-st">your turn</span>`;
  if (r.quiet && r.st !== "working") return `<span class="sx-st">quiet</span>`;
  if (r.st === "limited") return `<span class="sx-st">out of usage</span>`;
  return `<i class="sx-dot ${r.st === "working" ? "busy" : "idle"}" title="${r.st === "working" ? "working" : "idle"}"></i>`;
}
/** What a session is about, without a model: your latest request to it (Claude's, else what Cue saw you send). */
function svAbout(r) {
  const a = state.about?.[r.sid];
  const now = a?.prompt || sessionOf(r.sid)?.prompt || "";
  return { label: "About", about: "", now, outcome: "" };
}
/** Close: not while it's working or asking you (you'd lose the turn / the question), not a session Cue can't
 *  reach. Two clicks: Close, Close?. */
let svCloseArm = null, svCloseTimer = null;
/** Close: first click arms it ("Close?", red), a second within 4 s closes. In Sessions and the chat's header. */
function closeBtn(sid) {
  const what = "End it and close its tab (its conversation is kept: claude --resume brings it back)";
  return svCloseArm === sid
    ? `<button class="btn deny sx-close on" data-sv="close" data-sid="${esc(sid)}">Close?</button>`
    : `<button class="btn sx-close" data-sv="close" data-sid="${esc(sid)}" title="Close: ${esc(what)}">Close</button>`;
}
function svClosable(r) {
  return r.st !== "working" && r.st !== "asks";
}
/** What you can do with a session, right on it: answer or reply when it needs you, open it in Active, or go to
 *  its terminal. A quiet Claude session opens too (Cue takes it in); other quiet ones only have the tab. */
function svActs(r) {
  const acts = [];
  // No Reply: the tile opens it. Answer stays, for what's blocked on you.
  if (r.it && r.st === "asks") acts.push(`<button class="btn primary" data-sv="open" data-sid="${esc(r.sid)}">Answer</button>`);
  if (!r.it && (!r.quiet || r.harness === "claude")) acts.push(`<button class="btn" data-sv="${svOpenAct(r)}" data-sid="${esc(r.sid)}">Open</button>`);
  if (svClosable(r)) acts.push(closeBtn(r.sid));
  return acts.join("");
}
/** What opening a tile does: a quiet session taken in, or its chat. */
const svOpenAct = (r) => r.quiet ? "quiet" : "open";
/** One session, everything visible: name and state, what it's about, what it's on now, and its buttons. */
function svItem(r) {
  const ab = svAbout(r);
  // No name of its own: it's called after its folder, which the tile already says (two such look like
  // duplicates). Call it by what it's about instead.
  const unnamed = r.name === baseName(r.cwd) && (ab.about || ab.now);
  const name = unnamed ? ab.about || ab.now : r.name;
  const lines = [];
  if (!unnamed && ab.about) lines.push(`<div class="sx-l about">${esc(ab.about)}</div>`);
  if (r.st === "working" && r.run) lines.push(`<div class="sx-l run">${esc(r.what)}</div>`);
  else if (r.st === "asks" && r.what) lines.push(`<div class="sx-l ask">${esc(r.what)}</div>`);
  else if (ab.now && !(unnamed && name === ab.now)) lines.push(`<div class="sx-l now">› ${esc(ab.now)}</div>`);
  const hot = r.st === "asks";
  // The whole tile opens it: its chat in Active (a quiet Claude session too: Cue takes it in), else what it's doing.
  const go = ` role="button" data-sv="${svOpenAct(r)}" data-sid="${esc(r.sid)}"`;
  const viewing = r.sid === (quietOpen || active?.sid) ? "on" : "";
  return `<div class="sx-item ${viewing} ${hot ? "hot" : ""} ${r.quiet ? "quiet" : ""}"${go}>
    <div class="sx-iline">${badge(r.harness)}<span class="sx-name">${esc(name)}</span>${r.where ? `<span class="sx-branch">${esc(r.where)}</span>` : ""}${newPill(r.sid)}${viewing ? VIEWING : ""}${svChipState(r)}${agentPill(r.sid)}${r.since && !r.quiet ? `<span class="sx-age">${ago(r.since)}</span>` : ""}<span class="sx-acts">${svActs(r)}</span></div>
    ${lines.join("")}</div>`;
}
const svItems = (rs) => rs.sort((a, b) => svRank(a) - svRank(b) || b.since - a.since).map((r) => svItem(r)).join("");
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
/** The Sessions view's search: every word of `q` somewhere in a row's name, folder, branch, what it's doing,
 *  or its agent. */
function svMatcher(q) {
  const hay = (r) => { const ab = svAbout(r); return [r.name, r.cwd, homeless(r.cwd), state.branches?.[r.cwd], r.what, ab.about, ab.now, r.harness].join(" ").toLowerCase(); };
  return (r) => !q || q.split(/\s+/).every((w) => hay(r).includes(w));
}
/** Live sessions in tiles by project (a repository and its worktrees). Each machine its own tiles: the same
 *  project on two machines is two tiles. */
function svTiles(all) {
  const tiles = new Map();
  const tile = (r) => { const p = r.project || projOf(r.cwd), k = `${r.machine || ""}\n${p}`; return tiles.get(k) || tiles.set(k, { project: p, cwd: r.cwd, machine: r.machine || "", solo: [] }).get(k); };
  for (const r of all) tile(r).solo.push(r);
  return [...tiles.values()];
}
/** A project's tile, ranked by its most urgent session, then its newest; null when a search leaves nothing in it.
 *  One folder: its branch on the tile. Several (worktrees): each session says its own, and + starts in the main
 *  checkout. */
function svTileCard(t, q, match) {
  const members = t.solo;
  const folders = new Set(members.filter((r) => r.cwd).map((r) => r.cwd)), many = folders.size > 1;
  const cwd = many ? projRoot(t.cwd) : [...folders][0] || t.cwd, br = many ? "" : state.branches?.[cwd];
  for (const r of members) r.where = many && r.cwd ? state.branches?.[r.cwd] || baseName(r.cwd) : "";
  const solo = t.solo.filter(match);
  if (!solo.length) return null;
  const rank = Math.min(...members.map(svRank)), latest = Math.max(0, ...members.map((r) => r.since || 0));
  const html = `<div class="sx-tile ${members.some(svHot) ? "hot" : ""}">
      <div class="sx-thead">${projDot(t.project, members.some((m) => m.st === "working"))}<span class="sx-tname">${esc(t.project ? projLabel(t.project) : "(no folder)")}</span>${br ? `<span class="sx-branch">${esc(br)}</span>` : ""}<span class="sx-tcount">${members.length} session${members.length === 1 ? "" : "s"}</span><button class="sx-plus" data-lv="new" data-cwd="${esc(cwd)}" title="New session in ${esc(homeless(cwd))}">+</button></div>
      <div class="sx-solo">${svItems(solo)}</div></div>`;
  return { machine: t.machine, rank, latest, html };
}
/** A section under its heading, which folds it (kept); a search shows everything. */
function svSection(q, key, label, n, body, cls = "") {
  const open = !!q || !svFolded.has(key);
  return `<div class="sx-sec ${cls} ${open ? "" : "shut"}"><div class="sx-sec-h"><button class="sx-sec-fold" data-sv-fold="${esc(key)}" aria-expanded="${open}"><span class="sx-arrow">${open ? "▾" : "▸"}</span>${label}</button><span>${n}</span></div>${open ? body : ""}</div>`;
}
/** The live sections: This Mac, then each other machine, its tiles and what they add up to. */
function svLiveSections(all, cards, q) {
  const machines = [...new Set(["", ...cards.map((c) => c.machine)])].sort((a, b) => (a !== "") - (b !== "") || machShort(a).localeCompare(machShort(b)));
  return machines.map((m) => {
    const mine = cards.filter((c) => c.machine === m), rs = all.filter((r) => r.machine === m);
    if (!mine.length && m) return "";
    const asks = rs.filter((r) => r.st === "asks").length, yours = rs.filter((r) => r.st === "yours").length;
    const n = [`${rs.length} live`, asks && `${asks} asks you`, yours && `${yours} your turn`].filter(Boolean).join(" · ");
    return svSection(q, `m:${m}`, m ? esc(machShort(m)) : "This Mac", n, mine.length ? `<div class="sx-tiles">${mine.map((c) => c.html).join("")}</div>` : `<div class="quiet-line">${q ? `Nothing here matches “${esc(q)}”.` : "No live sessions on this Mac."}</div>`);
  }).join("");
}
/** Parked and Closed: one slim line each, newest first, a machine tag on the ones from elsewhere; Closed folds.
 *  What ended since is read again once a minute. */
function svRestSections(all, q, match) {
  const liveSids = new Set(all.map((r) => r.sid));
  const rest = [...(state.parked || []).map((p) => ({ ...p, kind: "parked", at: p.parked_ms })),
    ...closedAll().filter((c) => !(state.parked || []).some((p) => p.session_id === c.session_id)).map((c) => ({ ...c, kind: "closed", at: c.ended_ms }))]
    .filter((p) => !liveSids.has(p.session_id) && inProj(p.project) && match({ name: p.name || p.project, cwd: p.cwd, harness: p.harness }));
  if (!closedLoading && (!closedList || now() - closedAt > 60000)) queueMicrotask(loadClosed);
  const newest = (a, b) => b.at - a.at;
  const ps = rest.filter((p) => p.kind === "parked").sort(newest), cs = rest.filter((p) => p.kind === "closed").sort(newest), on = (sid) => sid === active?.sid;
  const restList = (key, rows) => `<div class="sx-rest" ${q ? "" : `data-fit="${key}"`}>${rows.map((r) => parkedRow(r, on(r.session_id))).join("")}</div>`;
  return (ps.length ? svSection(q, "parked", "Parked", ps.length, restList("parked", ps), "sx-rest-sec") : "")
    + (cs.length ? svSection(q, "closed", "Closed", `${cs.length} · last 30 days`, restList("closed", cs), "sx-rest-sec") : "");
}
function sessionsView() {
  const all = liveRows().filter((r) => inProj(r.project));
  const q = draft("find-sessions").text.trim().toLowerCase();
  const match = svMatcher(q);
  const cards = svTiles(all).map((t) => svTileCard(t, q, match)).filter(Boolean).sort((a, b) => a.rank - b.rank || b.latest - a.latest);
  const liveSecs = svLiveSections(all, cards, q);
  const restSecs = svRestSections(all, q, match);
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
  if (act === "close") {
    if (!armClose(sid)) return;
    // Its record, name and all, comes with Cue's next update (state.ended) the moment its row goes.
    try { toast(await invoke("close_session", { sessionId: sid })); }
    catch (e) { toast(`Couldn't close it: ${e}`); }
    return;
  }
  if (act === "open") { const it = state.items.find((i) => i.session_id === sid && i.status === "pending"); return openSession(sid, it?.id || null); }
  if (act === "quiet") {
    // A Claude session Cue hasn't heard from (resumed after a restart, say): Cue takes it in, so it opens
    // like any other, with its chat and a box to type in. If it can't, what it can show read-only.
    if ((state.live || []).find((x) => x.session_id === sid)?.harness === "claude") {
      try { await invoke("adopt_session", { sessionId: sid }); setState(await invoke("get_state")); return setActive(null, sid); }
      catch (e) { toast(`Couldn't open it here: ${e}`); }
    }
    setActive(null, null); quietOpen = sid; followProject(sid); loadSteps(sid); return renderMain();
  }
  if (act === "tab") {
    renderMain();
    try { toast(`Jumped to ${await invoke("focus_live", { sessionId: sid })}`); }
    catch (e) { toast(`Couldn't jump: ${e}`); }
  }
}
