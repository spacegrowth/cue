// Desk home (All projects on the Desk): every Desk as a row of one table, the "B" pick. A row: the Desk's name and
// what it's about, its project and people, its open tasks, bugs and features (working and to check under each), and
// who's on it; a click opens that Desk. + New → New Desk (and the table's foot) asks a project and a name, then Cue
// starts a lead there and opens the Desk. The sheet is drawn on its own element over the window, apart from
// renderMain, so a redraw under it keeps what you typed.

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
  const sub = [working ? `${working} working` : "", resume ? `${resume} to resume` : "", paused ? `${paused} paused` : "", held ? `${held} parked` : "", fin ? `${fin} finishing` : "", need ? `<span class="dh-ck">${need} need you</span>` : "", review ? `${review} to review` : ""].filter(Boolean).join(" · ") || "none moving";
  return `<span class="dh-cell"><span class="dh-big">${icon}${open.length} open</span><span class="dh-sm">${sub}</span></span>`;
}
/** Who's on a Desk: its lead, then the sessions and crew at work. */
function deskOnIt(d, ss) {
  const leadSid = deskLead(d)?.sid, lead = deskLeadName(d);
  const busy = ss.filter((s) => s.session_id !== leadSid && deskSessState(s)[1] === "working").map((s) => deskHolderName(s.session_id));
  for (const s of ss) for (const t of sessionOf(s.session_id)?.team || []) if (deskMateBusy(t)) busy.push(t.name);
  return `${lead ? `<b>${esc(lead)}</b>` : `<span class="dh-none">no dispatcher yet</span>`}${busy.length ? ` · ${esc(busy.join(", "))}` : ""}`;
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
    <div class="nd-hint">Cue starts a lead in this folder. Several Desks can share a folder.</div>
    <div class="nd-acts"><button type="button" class="btn" data-nd-close>Cancel</button><button class="btn primary" type="submit">Start Desk</button></div></form>`;
  document.body.append(ndEl);
  ndAgentDraw();
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
  if (folder) folder.value = ndCs[i].folder;
  if (name && (!name.value.trim() || ndCs.some((c) => projLabel(c.project) === name.value))) name.value = projLabel(ndCs[i].project);
  ndEl?.querySelector("#nd-proj")?.focus();   // drawn afresh: the cursor goes back on it
}
function deskNewClose() {
  if (!ndEl?.isConnected) return;
  ndEl.remove();
  if (ndBack?.isConnected) ndBack.focus({ preventScroll: true });
}
/** Start a separate named Desk in the chosen folder. */
async function deskNewStart() {
  const name = ndEl?.querySelector("#nd-name")?.value.trim().replace(/\s+/g, " ").slice(0, 80) || "";
  const folder = ndEl?.querySelector("#nd-folder")?.value.trim();
  if (!folder || !name || ndBusy) return;
  ndBusy = true;
  ndEl.querySelector('[type="submit"]').textContent = "Starting…";
  const r = await deskStartIn(folder, name, ndAgent);
  ndBusy = false;
  if (!r) { const b = ndEl?.querySelector('[type="submit"]'); if (b) b.textContent = "Start Desk"; return; }
  deskNewClose();
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
  document.addEventListener("submit", (e) => { if (ndEl?.contains(e.target)) { e.preventDefault(); deskNewStart(); } }, true);
  window.addEventListener("keydown", (e) => {
    if (ndEl?.isConnected) {
      e.stopImmediatePropagation();
      // The Project list open: ↑ ↓ move, Enter or Space picks, Esc closes just the list.
      if (ndDrop) {
        if (e.key === "Escape") { e.preventDefault(); ndDrop = false; ndPickDraw(); ndEl.querySelector("#nd-proj")?.focus(); }
        else if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); ndSel = Math.min(ndCs.length - 1, Math.max(0, ndSel + (e.key === "ArrowDown" ? 1 : -1))); ndPickDraw(); }
        else if (e.key === "Enter" || e.key === " ") { e.preventDefault(); ndPick(ndSel); }
        else if (e.key === "Tab") { ndDrop = false; ndPickDraw(); }
        return;
      }
      if (e.key === "Escape") { e.preventDefault(); deskNewClose(); }
      // On the picker, ↓ (or ↑) opens its list, as the system's menu would.
      else if ((e.key === "ArrowDown" || e.key === "ArrowUp") && e.target.id === "nd-proj") { e.preventDefault(); ndDrop = true; ndSel = ndAt; ndPickDraw(); }
      return;
    }
    if (newMenuOpen && e.key === "Escape") { e.stopImmediatePropagation(); newMenuOpen = false; renderMain(); }
  }, true);
}
