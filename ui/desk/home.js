// Desk home (All projects on the Desk): every Desk as a row of one table, the "B" pick. A row: the Desk's name and
// what it's about, its project and people, its open tasks, bugs and features (working and to check under each), and
// who's on it; a click opens that Desk. + New → New Desk (and the table's foot) asks a project and a name, then Cue
// starts a lead there and opens the Desk. The sheet is drawn on its own element over the window, apart from
// renderMain, so a redraw under it keeps what you typed.

/** What a Delete Desk here took away: the Desk and the sessions it closed, gone from the window as the delete comes
 *  back (the hub has dropped them by then), not when Cue's next update does. An update sent before the delete can't
 *  bring them back; each is let go once an update comes without it. */
const deskGoneNow = { desks: new Set(), sessions: new Set() };
function deskGoneApply(s) {
  const ds = s.desk?.projects || [], ss = s.sessions || [];
  for (const id of deskGoneNow.desks) if (!ds.some((d) => d.project === id)) deskGoneNow.desks.delete(id);
  for (const sid of deskGoneNow.sessions) if (!ss.some((x) => x.session_id === sid)) deskGoneNow.sessions.delete(sid);
  if (deskGoneNow.desks.size && s.desk) s.desk.projects = ds.filter((d) => !deskGoneNow.desks.has(d.project));
  if (deskGoneNow.sessions.size) s.sessions = ss.filter((x) => !deskGoneNow.sessions.has(x.session_id));
}

/** A Desk's sessions (its lead among them) and their crew still in play. */
function deskPeople(d) {
  const ss = (state.sessions || []).filter((s) => s.project === deskWorkspace(d) && deskHasSession(d, s.session_id));
  const crew = ss.reduce((n, s) => n + (sessionOf(s.session_id)?.team || []).filter((t) => deskMateLive(s.session_id, t)).length, 0);
  return { sessions: ss, crew };
}
/** One line on what a Desk is about: the item at work (else the newest open one, else the latest done). */
function deskAbout(d) {
  const open = d.items.filter((i) => i.state !== "done"), by = (a, b) => deskMs(b.updated) - deskMs(a.updated);
  const at = open.filter(deskMoving).sort(by)[0], next = [...open].sort(by)[0];
  const last = d.items.filter((i) => i.state === "done").sort((a, b) => deskMs(b.done_at || b.updated) - deskMs(a.done_at || a.updated))[0];
  const it = at || next || last;
  if (!it) return "Nothing on it yet.";
  return `${it === at ? "On" : it === next ? "Next" : "Last done"}: ${deskKindIcon(it)}${esc(deskNum(it))} ${esc(it.title)}`;
}
/** A kind's cell: "3 open", and under it how many are working, finishing and waiting on your check (or "none moving"). */
function deskKindCell(d, kind) {
  const open = deskItemsWithRequests(d).filter((i) => i.kind === kind && i.state !== "done");
  const icon = deskKindIcon(kind);
  if (!open.length) return `<span class="dh-cell z"><span class="dh-big">${icon}–</span></span>`;
  // Finishing (committed, being built and installed) is moving too, said apart from working.
  const working = open.filter((i) => i.state === "working" && !i.stopped).length, fin = open.filter(deskFinishing).length;
  const need = open.filter(deskWaits).length, review = open.filter((i) => deskReadyToReview(i) && !deskWaits(i)).length;
  const resume = open.filter((i) => i.state === "needs_resume").length, paused = open.filter((i) => i.state === "paused").length, held = open.filter((i) => i.state === "held").length;
  // Each piece whole on its line; the line wraps between them, so it stays in its column (#440).
  const sub = [working ? `${working} working` : "", resume ? `${resume} to resume` : "", paused ? `${paused} paused` : "", held ? `${held} parked` : "", fin ? `${fin} finishing` : "", need ? `<span class="dh-ck">${need} need you</span>` : "", review ? `${review} to review` : ""].filter(Boolean).map((x) => `<span class="dh-pc">${x}</span>`).join(" · ") || "none moving";
  return `<span class="dh-cell"><span class="dh-big">${icon}${open.length} open</span><span class="dh-sm dh-sub">${sub}</span></span>`;
}
/** Who's on a Desk: its lead, then the sessions and crew at work. */
function deskOnIt(d, ss) {
  const leadSid = deskLead(d)?.sid, lead = deskLeadName(d);
  const busy = ss.filter((s) => s.session_id !== leadSid && deskSessState(s)[1] === "working").map((s) => deskHolderName(s.session_id));
  for (const s of ss) for (const t of sessionOf(s.session_id)?.team || []) if (deskMateBusy(t)) busy.push(t.name);
  return `${lead ? `<b>${esc(lead)}</b>` : `<span class="dh-none">no lead yet</span>`}${busy.length ? ` · ${esc(busy.join(", "))}` : ""}`;
}
/** Live sessions explicitly on this Desk, with shared and busy ones kept open. */
function deskDeleteSessions(d) {
  const belongs = (desk, sid) => desk.lead?.sid === sid || (desk.members || []).includes(sid) || (desk.items || []).some((i) => i.state !== "done" && i.state !== "dropped" && (i.holder?.sid === sid || i.holder?.lead_sid === sid));
  return (state.sessions || []).filter((s) => belongs(d, s.session_id)).map((s) => {
    const sid = s.session_id;
    const shared = (state.desk?.projects || []).some((other) => other.project !== d.project && belongs(other, sid));
    const busy = ["working", "deciding", "agent"].includes(s.state) || (state.items || []).some((i) => i.session_id === sid && i.status === "pending" && i.kind !== "waiting");
    return { sid, name: deskHolderName(sid), shared, busy };
  });
}
function deskDeleteConfirmation(d) {
  if (deskDeleteAsk !== d.project) return "";
  const closeRows = deskDeleteSessions(d);
  const closeHtml = closeRows.map((s) => `<label class="dh-close-row"><input type="checkbox" data-desk-close-session="${esc(s.sid)}" ${deskCloseSelected.has(s.sid) && !s.shared && !s.busy ? "checked" : ""} ${s.shared || s.busy || deskDeleting ? "disabled" : ""}><span>${esc(s.name)}</span>${s.shared ? `<span class="dim">Shared with another Desk · stays open</span>` : s.busy ? `<span class="dim">Working or needs you · stays open</span>` : ""}</label>`).join("");
  return `<div class="set-confirm dh-confirm"><div class="end-text">Delete ${esc(deskName(d))} and its items and decisions permanently? Conversations and files stay.</div>${closeHtml ? `<div class="dh-close-title">Close these sessions <span class="dim">· uncheck any to keep open</span></div><div class="dh-close-list">${closeHtml}</div>` : ""}<div class="row-btns"><span class="grow"></span><button class="btn small" data-desk-delete-cancel ${deskDeleting ? "disabled" : ""}>Cancel</button><button class="btn small deny" data-desk-delete-confirm ${deskDeleting ? "disabled" : ""}>${deskDeleting ? "Deleting…" : "Delete Desk"}</button></div></div>`;
}
function deskHomeRow(d) {
  const { sessions, crew } = deskPeople(d), n = sessions.length;
  const people = `${n} session${n === 1 ? "" : "s"} · ${crew ? `${crew} crew` : "no crew"}`;
  return `<div class="dh-tr dk-desk" role="button" tabindex="0" data-desk-project="${esc(d.project)}">
    <span class="dh-cell"><b class="dh-nm">${esc(deskName(d))}</b><span class="dh-sm dh-ab">${deskAbout(d)}</span></span>
    <span class="dh-cell"><span class="dh-pj">${projDot(deskWorkspace(d))}${esc(projLabel(deskWorkspace(d)))}</span><span class="dh-sm">${people}</span></span>
    ${["task", "bug", "feature"].map((k) => deskKindCell(d, k)).join("")}<span class="dh-on">${deskOnIt(d, sessions)}</span><span class="dh-go"><button class="btn small" data-desk-delete="${esc(d.project)}" aria-label="Delete ${esc(deskName(d))}">Delete…</button></span></div>${deskDeleteConfirmation(d)}`;
}
/** Every Desk, those with the most waiting on you first. */
const deskSorted = () => (state.desk?.projects || []).map((x) => {
  const d = deskOf(x.project);
  return { d, need: deskWaitingItems(d).length };
}).sort((a, b) => b.need - a.need || deskName(a.d).localeCompare(deskName(b.d))).map(({ d }) => d);
function deskAll() {
  const ds = deskSorted();
  const head = `<div class="dh-tr dh-hd" aria-hidden="true"><span>Desk</span><span>Project · people</span><span>Tasks</span><span>Bugs</span><span>Features</span><span>On it</span><span></span></div>`;
  return `<div class="board dk-board all"><div class="dk-desks" data-scroll-key="desks"><div class="dk-hh">Desks <span class="dh-n">· ${ds.length}</span></div>
    <div class="dh-tbl">${ds.length ? head + ds.map(deskHomeRow).join("") : `<div class="dh-empty">No Desks yet.</div>`}<button class="dh-add" data-desk-new-desk>+ New Desk</button></div></div></div>`;
}

// ---------- The Desk tab: a Desk, not this list ----------

/** The Desk last open (kept across launches): the Desk tab goes back to it; Desk home is behind ‹ Desks. */
let deskLast = (() => { try { return localStorage.getItem("cue.deskLast") || ""; } catch { return ""; } })();
function deskLastSet(p) {
  if (p === deskLast || !(state.desk?.projects || []).some((d) => d.project === p)) return;
  deskLast = p;
  try { localStorage.setItem("cue.deskLast", p); } catch {}
}
/** The Desk tab with All projects picked: the Desk last open, else the first on Desk home; none yet, Desk home. */
function deskTabOpen() {
  if (deskSelection()) return;
  const ds = deskSorted(), p = ds.some((d) => d.project === deskLast) ? deskLast : ds[0]?.project;
  if (!p) return;
  deskSelect(p); deskOpen = null; deskTab = "lead"; deskChatEnd = true;
}

// ---------- + New: New session or New Desk ----------

let newMenuOpen = false;
/** + New in the header: a menu of New session (⌘N goes straight there) and New Desk. */
const newChip = () => `<span class="nw-wrap"><button class="hchip new-chip ${newMenuOpen ? "on" : ""}" data-new-menu aria-haspopup="menu" aria-expanded="${newMenuOpen}">+ New ▾</button>${newMenuOpen
  ? `<div class="nw-pop" role="menu"><button role="menuitem" data-lv="new" data-spot="1">New session<kbd>⌘N</kbd></button><button role="menuitem" data-desk-new-desk>New Desk</button></div>` : ""}</span>`;

// ---------- New Desk: a project and a name ----------

let ndEl = null, ndBusy = false, ndBack = null, ndAgent = "claude";
/** The Project picker, Cue's own (not the system's menu): the choices, the one picked, whether its list is open and
 *  the row the arrow keys are on. */
let ndCs = [], ndAt = 0, ndDrop = false, ndSel = 0;
/** Recent folders to reuse. A folder can have any number of Desks. */
function ndChoices() {
  const seen = new Set(), out = [];
  for (const f of recentFolders()) {
    const p = projOf(f);
    if (seen.has(f)) continue;
    seen.add(f); out.push({ folder: f, project: p });
  }
  return out;
}
function deskNewOpen() {
  newMenuOpen = false;
  const cs = ndCs = ndChoices();
  ndAt = ndSel = 0; ndDrop = false;
  ndAgent = newAgent;
  ndBack = document.activeElement;
  ndEl ||= Object.assign(document.createElement("div"), { className: "move-scrim nd-scrim" });
  ndEl.innerHTML = `<form class="move-dialog nd" role="dialog" aria-modal="true" aria-labelledby="nd-title">
    <div class="nd-head"><b id="nd-title">New Desk</b><span class="grow"></span><button type="button" class="nd-x" data-nd-close aria-label="Close">×</button></div>
    ${cs.length ? `<label class="nd-l" for="nd-proj">Recent folders</label><div class="nd-pickwrap">${ndPickHtml()}</div>` : ""}
    <label class="nd-l" for="nd-folder">Folder</label><input class="nd-f" id="nd-folder" required spellcheck="false" autocomplete="off" placeholder="~/path/to/folder" value="${esc(cs[0]?.folder || "")}"/>
    <label class="nd-l" for="nd-name">Name</label><input class="nd-f" id="nd-name" required maxlength="80" spellcheck="false" autocomplete="off" value="${esc(cs[0] ? projLabel(cs[0].project) : "")}"/>
    <span class="nd-l" id="nd-agent-label">Agent</span><div class="nd-agent" role="group" aria-labelledby="nd-agent-label"></div>
    <div class="nd-look-at"></div>
    <div class="nd-acts"></div></form>`;
  document.body.append(ndEl);
  ndAgentDraw();
  ndLookStart(cs[0]?.folder || "");
  if (!agentsAvail) invoke("agents_installed").then((a) => { agentsAvail = a?.length ? a : ["claude"]; if (ndEl?.isConnected) ndAgentDraw(); }, () => {});
  renderMain();
  ndEl.querySelector("#nd-name")?.select();
  ndEl.querySelector("#nd-name")?.focus();
}
/** The same installed-agent choices and segmented control as New session. */
function ndAgentDraw() {
  const agents = agentsAvail || [ndAgent];
  if (!agents.includes(ndAgent)) ndAgent = agents[0];
  const row = ndEl?.querySelector(".nd-agent");
  if (row) row.innerHTML = `<div class="seg">${agents.map((a) => `<button type="button" class="${ndAgent === a ? "on" : ""}" data-nd-agent="${esc(a)}" aria-pressed="${ndAgent === a}">${esc(agentName(a))}</button>`).join("")}</div>`;
}
/** A project's line: its dot and name, then its folder, muted. */
const ndOpt = (c) => `${projDot(c.project)}<span class="nd-pn">${esc(projLabel(c.project))}</span><span class="nf-hint nd-pp">${esc(homeless(c.folder))}</span>`;
function ndPickHtml() {
  const c = ndCs[ndAt];
  const btn = `<button type="button" class="nd-f nd-pick" id="nd-proj" data-nd-pick aria-haspopup="listbox" aria-expanded="${ndDrop}">${ndOpt(c)}<span class="nd-car">▾</span></button>`;
  const list = ndDrop ? `<div class="nf-drop nd-drop" role="listbox">${ndCs.map((x, i) => `<button type="button" role="option" aria-selected="${i === ndAt}" class="nf-dopt ${i === ndSel ? "on" : ""}" data-nd-at="${i}">${ndOpt(x)}</button>`).join("")}</div>` : "";
  return btn + list;
}
function ndPickDraw() {
  const w = ndEl?.querySelector(".nd-pickwrap");
  if (!w) return;
  w.innerHTML = ndPickHtml();
  w.querySelector(".nf-dopt.on")?.scrollIntoView({ block: "nearest" });
}
/** Picks project `i`; the name follows it, unless you'd typed one of your own. */
function ndPick(i) {
  const name = ndEl?.querySelector("#nd-name");
  ndAt = ndSel = i; ndDrop = false;
  ndPickDraw();
  const folder = ndEl?.querySelector("#nd-folder");
  const was = folder?.value;
  if (folder) folder.value = ndCs[i].folder;
  if (name && (!name.value.trim() || ndCs.some((c) => projLabel(c.project) === name.value))) name.value = projLabel(ndCs[i].project);
  if (folder && folder.value !== was) ndLookStart(folder.value);
  else ndLookDraw();
  ndEl?.querySelector("#nd-proj")?.focus();   // drawn afresh: the cursor goes back on it
}
function deskNewClose() {
  if (!ndEl?.isConnected) return;
  ndLookGen++; clearTimeout(ndLookTimer); ndLook = null;
  ndEl.remove();
  if (ndBack?.isConnected) ndBack.focus({ preventScroll: true });
}
/** Start a separate named Desk in the chosen folder: with the settings the look found and you accepted, or with the
 *  usual ones (`usual`: Skip, or the look failed). Skip ends a look still going: the box goes back to its hint. */
async function deskNewStart(usual = false) {
  const name = ndEl?.querySelector("#nd-name")?.value.trim().replace(/\s+/g, " ").slice(0, 80) || "";
  const folder = ndEl?.querySelector("#nd-folder")?.value.trim();
  if (!folder || !name || ndBusy) return;
  if (!usual && ndLook && !ndLook.look && !ndLook.err) return;   // Start waits for the look (its button is off)
  const lands = !usual && ndLook?.look && ndLook.folder === folder ? ndLookLands() : null;
  if (usual) { ndLookGen++; clearTimeout(ndLookTimer); ndLook = null; }
  ndBusy = true;
  ndLookDraw();
  const r = await deskStartIn(folder, name, ndAgent);
  if (r && lands) {
    try { await invoke("desk_lands_set", { desk: r.id || r.project, lands }); }
    catch (e) { toast(`The Desk started with the usual settings: ${e}`); }
  }
  ndBusy = false;
  if (!r) return ndLookFoot();
  deskNewClose();
}

// ---------- New Desk: how the folder builds, checks and lands (#927) ----------

/** The look at the folder picked: Cue reads its files, asks GitHub, counts how its last changes came in and reads its
 *  past sessions, each part ticked as it answers; then what it found as rows you can change. A newer look (another
 *  folder) throws an older one's answers away by its `gen`. */
let ndLook = null, ndLookGen = 0, ndLookTimer = 0;
const ND_LOOK_PARTS = [
  ["files", "Reading the project's files…"],
  ["github", "Asking GitHub whether main needs a pull request…"],
  ["history", "Your last 30 changes on main: how they came in"],
  ["sessions", "Reading past sessions here…"],
];
/** Each row's choices, in the words of the picked mock (desk/lands.rs PARTS keys). Cue's own check and install show
 *  only on Cue's own source. */
const ND_LOOK_CHOICES = {
  check: [["none", "None"], ["cue", "Cue's own check"], ["command", "Your own steps"]],
  how: [["squash", "One commit"], ["merge", "Merge"], ["pr", "Pull request"], ["owner", "You commit"]],
  after: [["nothing", "Nothing"], ["install", "Install or serve Cue"], ["command", "Your own steps"]],
  push: [["after", "Right away"], ["ask", "Ask me first"], ["never", "Never"]],
};
const ND_LOOK_TAGS = { found: "Found", unsure: "Couldn't tell", follows: "Follows", usual: "Usual" };
const ndIcTick = `<svg class="nd-ic nd-tick" viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.5l3 3 6-7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>`;
const ndIcSpin = `<span class="nd-ic nd-spin" aria-hidden="true"></span>`;
/** The folder typed: the look waits until you stop typing (half a second), and Start waits for it. */
function ndLookSoon() {
  const folder = ndEl?.querySelector("#nd-folder")?.value.trim() || "";
  clearTimeout(ndLookTimer);
  const gen = ++ndLookGen;
  ndLook = folder ? { gen, folder, parts: {}, found: {}, look: null, err: "", rows: [] } : null;
  ndLookDraw();
  if (folder) ndLookTimer = setTimeout(() => { if (ndLookGen === gen) ndLookStart(folder); }, 500);
}
/** Starts a look at `folder`: its four parts at once, then the rows from all they found. */
function ndLookStart(folder) {
  clearTimeout(ndLookTimer);
  const gen = ++ndLookGen;
  ndLook = folder ? { gen, folder, parts: {}, found: {}, look: null, err: "", rows: [] } : null;
  ndLookDraw();
  if (!folder) return;
  const mine = () => ndLook?.gen === gen && ndLookGen === gen;
  const part = (p) => invoke("desk_look", { folder, part: p }).then((r) => {
    if (!mine() || ndLook.err) return;
    ndLook.parts[p] = r?.lines || [];
    ndLook.found[p] = r?.found ?? null;
    ndLookDraw();
  });
  Promise.all(ND_LOOK_PARTS.map(([p]) => part(p)))
    .then(() => mine() ? invoke("desk_look_rows", { folder, found: ndLook.found }) : null)
    .then((look) => {
      if (!mine() || !look) return;
      ndLook.look = look;
      ndLook.rows = (look.rows || []).map((r) => ({ ...r, text: (r.steps || []).join("\n"), mine: false, open: r.tag === "unsure" }));
    }, (e) => { if (mine()) ndLook.err = String(e?.message || e); })
    .then(() => { if (mine()) ndLookDraw(); });
}
/** A line of the ticked list: the file it names in mono. */
function ndLookLine(l) {
  const t = esc(l?.text || ""), f = l?.file ? esc(l.file) : "";
  return f && t.includes(f) ? t.replace(f, `<span class="nd-mono">${f}</span>`) : t;
}
/** A step as shown: its label ("[Lint] npm run lint" → Lint), else the command in mono. */
const ndStepLabel = (s) => { const m = /^\[([^\s\]][^\]]*)\]\s+/.exec(s); return m ? esc(m[1]) : `<span class="nd-mono">${esc(s)}</span>`; };
/** A step without its "[Word] " name, as desk/lands.rs reads one: a command that starts with "[ " (a test) keeps it. */
const ndStepPlain = (s) => s.replace(/^\[[^\s\]][^\]]*\]\s+/, "");
const ndStepsOf = (text) => (text || "").split("\n").map((x) => x.trim()).filter(Boolean);
/** Where a row's answer came from: its words, the files in them in mono, and the past sessions it went by. */
function ndLookSrc(r) {
  const files = (r.sources || []).filter((s) => s.kind === "file" && s.file).map((s) => s.file);
  let why = esc(r.why || "");
  if (!why && files.length) why = `From ${files.map(esc).join(", ")}.`;
  for (const f of [...new Set(files)].sort((a, b) => b.length - a.length)) why = why.split(esc(f)).join(`\u0000${esc(f)}\u0001`);
  why = why.replace(/\u0000/g, '<span class="nd-mono">').replace(/\u0001/g, "</span>");
  const ss = (r.sources || []).find((s) => s.kind === "sessions");
  if (ss && !/past session/.test(r.why || "")) {
    const names = ss.names || [], n = ss.count || names.length, shown = names.slice(0, 2);
    const more = n - shown.length;
    why += `${why ? " " : ""}From ${n} past session${n === 1 ? "" : "s"}${shown.length ? `: ${shown.map(esc).join(", ")}${more > 0 ? `, +${more}` : ""}` : ""}.`;
  }
  return why;
}
/** A row's choices as Desk settings shows them; a Couldn't tell row not yet picked has none lit. */
function ndLookSeg(r) {
  const place = ndLook.look.place;
  const cs = (ND_LOOK_CHOICES[r.part] || []).filter(([k]) => (k !== "cue" && k !== "install") || place === "cue");
  const lit = r.tag !== "unsure" || r.mine;
  return `<div class="seg nd-seg" role="radiogroup" aria-label="${esc(r.label)}">${cs.map(([k, l]) => {
    const on = lit && r.choice === k;
    return `<button type="button" role="radio" aria-checked="${on}" class="${on ? "on" : ""}" data-nd-row="${esc(r.part)}" data-nd-choice="${esc(k)}">${esc(l)}</button>`;
  }).join("")}</div>`;
}
/** One row of what the look found: its label and tag, its value (or its choices, open), Change, and where it came from. */
function ndLookRow(r) {
  const steps = r.part === "build" || ((r.part === "check" || r.part === "after") && r.choice === "command");
  // A Follows row stops following once the row it follows is yours.
  const unfollowed = r.tag === "follows" && ndLook.rows.find((x) => x.part === "how")?.mine;
  const chip = r.mine || unfollowed ? "" : `<span class="nd-chip ${r.tag === "unsure" ? "nd-chip-unsure" : ""}">${esc(ND_LOOK_TAGS[r.tag] || r.tag)}</span>`;
  let value;
  if (r.open) {
    value = `${r.part === "build" ? "" : ndLookSeg(r)}${steps ? `<textarea class="ld-cmd nd-cmd" rows="${Math.max(2, ndStepsOf(r.text).length)}" spellcheck="false" aria-label="${esc(r.label)}: one step per line" placeholder="One step per line, e.g. npm test" data-nd-steps="${esc(r.part)}">${esc(r.text)}</textarea>` : ""}`;
  } else if (r.part === "build") {
    const s = ndStepsOf(r.text)[0];
    value = s ? `<span class="nd-mono nd-val-t">${esc(ndStepPlain(s))}</span>` : `<span class="nd-muted">Nothing to build first</span>`;
  } else if (steps) {
    const ss = ndStepsOf(r.text);
    value = `<span class="nd-steps"><b>${ss.length} step${ss.length === 1 ? "" : "s"}:</b> ${ss.map(ndStepLabel).join(" · ")}</span>`;
  } else {
    value = `<span class="nd-val-t">${esc((ND_LOOK_CHOICES[r.part] || []).find(([k]) => k === r.choice)?.[1] || r.choice || "")}</span>`;
  }
  const act = r.open ? (r.tag === "unsure" && !r.mine ? "" : `<button type="button" class="nd-lk" data-nd-change="${esc(r.part)}">Done</button>`)
    : `<button type="button" class="nd-lk" data-nd-change="${esc(r.part)}">Change</button>`;
  const src = ndLookSrc(r);
  return `<div class="nd-row" data-nd-look-row="${esc(r.part)}"><div class="nd-ln"><b class="nd-rl">${esc(r.label)}</b><span class="grow"></span>${chip}</div>
    <div class="nd-ln nd-val ${r.open ? "nd-open" : ""}"><div class="nd-vbody">${value}</div>${act}</div>${src ? `<div class="nd-src">${src}</div>` : ""}</div>`;
}
/** The look's name for the folder: the Desk's name as typed, else the folder's last part. */
const ndLookName = () => ndEl?.querySelector("#nd-name")?.value.trim() || (ndLook?.folder || "").replace(/\/+$/, "").split("/").pop() || "this folder";
/** The box under Agent: the look going (its lines ticking), what it found, or why it couldn't. */
function ndLookDraw() {
  const at = ndEl?.querySelector(".nd-look-at");
  if (!at) return;
  const L = ndLook, name = esc(ndLookName());
  if (!L) at.innerHTML = `<div class="nd-hint">Cue starts a lead in this folder. Several Desks can share a folder.</div>`;
  else if (L.err) at.innerHTML = `<div class="nd-look nd-look-err" role="status"><div class="nd-lk-hd"><span>Couldn't work it out: ${esc(L.err)}</span></div><div class="nd-src">The Desk starts with the usual settings; you can change them in Desk settings.</div></div>`;
  else if (L.look) at.innerHTML = `<div class="nd-look nd-look-found"><div class="nd-lk-hd">${ndIcTick}<span>How ${name} builds, checks and lands</span><span class="grow"></span><span class="nd-lk-sum">${esc(L.look.summary || "")}</span></div>
    <div class="nd-rows">${L.rows.map(ndLookRow).join("")}</div></div>`;
  else at.innerHTML = `<div class="nd-look nd-looking" aria-live="polite"><div class="nd-lk-hd">${ndIcSpin}<span>Working out how ${name} builds, checks and lands…</span></div>
    <div class="nd-lk-lines">${ND_LOOK_PARTS.map(([p, wait]) => L.parts[p]
      ? L.parts[p].map((l) => `<div class="nd-lk-ln nd-done" data-nd-part="${p}">${ndIcTick}<span>${ndLookLine(l)}</span></div>`).join("")
      : `<div class="nd-lk-ln nd-wait" data-nd-part="${p}">${ndIcSpin}<span>${wait}</span></div>`).join("")}</div></div>`;
  ndLookFoot();
}
/** The sheet's foot: Skip while the look goes (Start waits for it), a word on Desk settings once it's done. */
function ndLookFoot() {
  const f = ndEl?.querySelector(".nd-acts");
  if (!f) return;
  const L = ndLook, looking = !!L && !L.look && !L.err;
  const left = looking ? `<button type="button" class="nd-lk" data-nd-skip${ndBusy ? " disabled" : ""}>Skip, use the usual settings</button>`
    : L?.look ? `<span class="nd-foot-t">You can change any of these later in Desk settings.</span>` : "";
  f.innerHTML = `${left}<span class="grow"></span><button type="button" class="btn" data-nd-close>Cancel</button><button class="btn primary" type="submit"${looking || ndBusy ? " disabled" : ""}>${ndBusy ? "Starting…" : "Start Desk"}</button>`;
}
/** The settings to start with: the look's, with each row's current choice and steps folded in. A step's "[Lint] "
 *  is only for showing: lands steps are plain commands. */
function ndLookLands() {
  const L = ndLook.look, out = { ...L.lands }, row = (p) => ndLook.rows.find((r) => r.part === p);
  const plain = (r) => ndStepsOf(r?.text).map(ndStepPlain).filter(Boolean);
  for (const p of ["how", "push"]) if (row(p)) out[p] = row(p).choice;
  // As Desk settings does: You commit works in the shared folder; a pull request or a merge needs an item's own copy.
  if (row("how")) out.where = out.how === "owner" ? "shared" : out.how === "pr" || out.how === "merge" ? "copy" : out.where;
  const b = row("build"), c = row("check"), a = row("after");
  if (c?.choice === "cue") out.check = "cue";
  else if (b || c) {
    out.check_steps = [...plain(b), ...(c?.choice === "command" ? plain(c) : [])];
    out.check = out.check_steps.length ? "command" : "none";
  }
  if (a) {
    out.after_steps = a.choice === "command" ? plain(a) : [];
    out.after = a.choice === "command" && !out.after_steps.length ? "nothing" : a.choice;
  }
  return out;
}
/** A click in the look's rows: Change/Done, a choice picked (a Couldn't tell row becomes yours). */
function ndLookClick(t) {
  const ch = t.closest("[data-nd-change]"), pick = t.closest("[data-nd-choice]");
  const r = ndLook?.rows.find((x) => x.part === (ch || pick)?.dataset[ch ? "ndChange" : "ndRow"]);
  if (!r) return false;
  if (ch) { r.open = !r.open; if (!r.open && r.tag === "unsure") r.mine = true; }
  else {
    if (r.choice !== pick.dataset.ndChoice || r.tag === "unsure") r.mine = true;
    r.choice = pick.dataset.ndChoice;
    if (!((r.part === "check" || r.part === "after") && r.choice === "command")) r.open = false;
  }
  ndLookDraw();
  ndEl.querySelector(`[data-nd-look-row="${r.part}"] ${r.open ? "[data-nd-steps], .nd-seg button.on" : "[data-nd-change]"}`)?.focus();
  return true;
}

let deskStarting = false;   // a Desk starting, until Cue answers
/** Start a Desk in `folder`, then open it. Cue's answer ({ project, sid }), or null when it couldn't. */
async function deskStartIn(folder, name = undefined, agent = undefined) {
  if (!folder || deskStarting) return null;
  deskStarting = true;
  let r = null;
  try {
    r = await invoke("desk_start", { folder, ...(name && { name }), ...(agent && { agent }) });
    if (r?.project) { view = "desk"; deskSelect(r.id || r.project); deskOpen = null; deskTab = "lead"; deskChatEnd = true; }
  } catch (e) { toast(`Couldn't start the Desk: ${e}`); }
  deskStarting = false; renderMain();
  return r;
}

if (typeof window !== "undefined") {
  // The menu and the sheet answer their own clicks and keys before the window does.
  document.addEventListener("click", (e) => {
    if (e.target.closest?.("[data-new-menu]")) { e.stopPropagation(); newMenuOpen = !newMenuOpen; return renderMain(); }
    if (e.target.closest?.("[data-desk-new-desk]")) { e.stopPropagation(); return deskNewOpen(); }
    if (ndEl?.isConnected && ndEl.contains(e.target)) {
      e.stopPropagation();
      if (e.target === ndEl || e.target.closest("[data-nd-close]")) return deskNewClose();
      const agent = e.target.closest("[data-nd-agent]");
      if (agent) { ndAgent = agent.dataset.ndAgent; ndAgentDraw(); return; }
      if (e.target.closest("[data-nd-skip]")) return deskNewStart(true);
      if (ndLookClick(e.target)) return;
      const at = e.target.closest("[data-nd-at]");
      if (at) return ndPick(+at.dataset.ndAt);
      if (e.target.closest("[data-nd-pick]")) { ndDrop = !ndDrop; ndSel = ndAt; ndPickDraw(); return ndEl.querySelector("#nd-proj")?.focus(); }
      if (ndDrop && !e.target.closest(".nd-drop")) { ndDrop = false; ndPickDraw(); }
      return;
    }
    // Anything else (New session too) closes the menu, then does what it does.
    if (newMenuOpen) { newMenuOpen = false; queueMicrotask(renderMain); }
  }, true);
  document.addEventListener("change", (e) => {
    const input = e.target.closest?.("[data-desk-close-session]");
    if (!input) return;
    if (input.checked) deskCloseSelected.add(input.dataset.deskCloseSession); else deskCloseSelected.delete(input.dataset.deskCloseSession);
  });
  // Typing a folder starts a new look once you stop; the name typed is the one the look says; edited steps are yours.
  document.addEventListener("input", (e) => {
    if (!ndEl?.isConnected || !ndEl.contains(e.target)) return;
    if (e.target.id === "nd-folder") ndLookSoon();
    else if (e.target.id === "nd-name") { const hd = ndEl.querySelector(".nd-look:not(.nd-look-err) .nd-lk-hd span:not(.nd-ic)"); if (hd && ndLook) hd.textContent = ndLook.look ? `How ${ndLookName()} builds, checks and lands` : `Working out how ${ndLookName()} builds, checks and lands…`; }
    else if (e.target.dataset.ndSteps) { const r = ndLook?.rows.find((x) => x.part === e.target.dataset.ndSteps); if (r) { r.text = e.target.value; r.mine = true; const c = e.target.closest(".nd-row")?.querySelector(".nd-chip"); c?.remove(); } }
  });
  document.addEventListener("submit", (e) => { if (ndEl?.contains(e.target)) { e.preventDefault(); deskNewStart(); } }, true);
}
/** The sheet's keys, and Esc for the menu, before the window's (app/keys.js KEY_FIRST): while the sheet is open it
 *  takes every key (typing in its fields goes on as usual: only these keys are kept from their default). */
function deskHomeKey(e) {
  if (ndEl?.isConnected) {
    // The Project list open: ↑ ↓ move, Enter or Space picks, Esc closes just the list.
    if (ndDrop) {
      if (e.key === "Escape") { e.preventDefault(); ndDrop = false; ndPickDraw(); ndEl.querySelector("#nd-proj")?.focus(); }
      else if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); ndSel = Math.min(ndCs.length - 1, Math.max(0, ndSel + (e.key === "ArrowDown" ? 1 : -1))); ndPickDraw(); }
      else if (e.key === "Enter" || e.key === " ") { e.preventDefault(); ndPick(ndSel); }
      else if (e.key === "Tab") { ndDrop = false; ndPickDraw(); }
      return true;
    }
    if (e.key === "Escape") { e.preventDefault(); deskNewClose(); }
    // On the picker, ↓ (or ↑) opens its list, as the system's menu would.
    else if ((e.key === "ArrowDown" || e.key === "ArrowUp") && e.target.id === "nd-proj") { e.preventDefault(); ndDrop = true; ndSel = ndAt; ndPickDraw(); }
    return true;
  }
  if (newMenuOpen && e.key === "Escape") { newMenuOpen = false; renderMain(); return true; }
  return false;
}
