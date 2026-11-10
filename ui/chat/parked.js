// Cue UI chat — parked sessions, restarting, and Tell me when done.
// ---------- Parked: sessions you stopped, to come back to ----------
// Park ends a session's agent (as Close does) and keeps it in the Parked drawer; Resume runs its
// agent's own resume command and it picks up where it left off. Nothing wakes it but you.
const PARKABLE = new Set(["claude", "codex", "pi"]);
const waking = new Set();   // parked sessions on their way back
let parkedAll = null;       // the parked session whose whole kept thread shows (else its last 2 turns)
/** The parking sign: a small rounded square with a bold P, wherever a session is parked. */
const P_SIGN = `<i class="p-sign" aria-hidden="true">P</i>`;
/** How long ago it was parked: "just now" for the first minute. */
const parkedAgo = (p) => { const at = p.at ?? p.parked_ms; return now() - at < 60000 ? "just now" : `${ago(at)} ago`; };
/** Its last `n` turns: from your n-th last message to the end (all of it if it has fewer). */
function lastTurns(thread, n) {
  const yours = thread.flatMap((e, i) => (e.role === "you" ? [i] : []));
  return yours.length > n ? thread.slice(yours.at(-n)) : thread;
}
/** Park, in the Active header's ⋯ menu: where Close is offered (not while it works or asks you), for an agent
 *  Cue can bring back. */
function parkMenuItem(sid, s, closable) {
  if (!closable || !s || !PARKABLE.has(s.harness) || !s.cwd) return "";
  return `<button data-park-session="${esc(sid)}">Park<span>Stop it and keep it under Parked; Resume brings it back whole</span></button>`;
}
/** Delete, from the ⋯ menu: asked first, in a dialog (Esc no, Enter yes). `deleteAsk`: the session it asks about. */
let deleteAsk = null;
function deleteDialog() {
  const sid = deleteAsk, name = nameOf(sid, sessionOf(sid)?.project || "this session");
  return `<div class="move-scrim"><div class="move-dialog end-dialog" role="alertdialog" aria-labelledby="del-title">
    <div class="move-title" id="del-title">Delete ${esc(name)}?</div>
    <div class="end-text">${sessionOf(sid) ? "It stops and its tab closes. " : ""}Cue forgets it, and its conversation is deleted, so it can't be resumed. This can't be undone.</div>
    <div class="row-btns"><span class="grow"></span><button class="btn" data-act="delete-no">Cancel <kbd>Esc</kbd></button><button class="btn deny" data-act="delete-yes">Delete <kbd>↵</kbd></button></div>
  </div></div>`;
}
async function deleteAnswer(yes) {
  const sid = deleteAsk;
  deleteAsk = null;
  renderMain();
  if (!yes || !sid) return;
  const name = nameOf(sid, sessionOf(sid)?.project || "it");
  try { toast(`${await invoke("delete_session", { sessionId: sid })}: ${name}`); }
  catch (e) { toast(`Couldn't delete ${name}: ${e}`); }
}
async function parkSession(sid) {
  const name = nameOf(sid, sessionOf(sid)?.project || "it");
  try { await invoke("park_session", { sessionId: sid }); toast(`Parked ${name}: its agent stopped and its terminal closed`); }
  catch (e) { toast(`Couldn't park ${name}: ${e}`); }
}
/** Resume (↺, the button, or Resume & send with what's in its box), a parked or a closed one: back in
 *  Sessions, open in Active. */
async function wakeParked(sid, message = "") {
  const parked = parkedOf(sid), p = parked || closedOf(sid);
  if (!p || waking.has(sid)) return;
  waking.add(sid);
  renderMain();
  try {
    const r = await invoke(parked ? "resume_parked" : "resume_closed", { sessionId: sid, message });
    // Until Cue's update has it live, Active keeps showing it here (else it would move on to something else).
    for (let n = 0; n < 30 && !sessionOf(sid); n++) await new Promise((ok) => setTimeout(ok, 100));
    if (!parked) closedList = (closedList || []).filter((c) => c.session_id !== sid);
    if (message) { draft(`s:${sid}`).text = ""; saveDrafts(); }
    toast(`${p.name || p.project} is back: ${r.replace(/^Resumed/, "resumed")}`);
    setActive(null, sid);
  } catch (e) {
    toast(`Couldn't resume ${p.name || p.project}: ${e}`);
  } finally {
    waking.delete(sid);
    renderMain();
  }
}
/** Park, on a closed one: kept under Parked (it has ended already, so nothing stops). */
async function parkClosed(sid) {
  const c = closedOf(sid);
  try { await invoke("park_closed", { sessionId: sid }); }
  catch (e) { return toast(`Couldn't park it: ${e}`); }
  // Shown parked now (Cue's update says the same), so Active stays on it.
  if (c && !parkedOf(sid)) state.parked = [{ ...c, parked_ms: Date.now() }, ...(state.parked || [])];
  closedList = (closedList || []).filter((x) => x.session_id !== sid);
  toast(`Parked ${c?.name || c?.project || "it"}`);
  renderMain();
}
/** Unpark: off the list, nothing started. Its conversation stays in History and ⌘F. */
async function unparkSession(sid) {
  try { await invoke("unpark_session", { sessionId: sid }); }
  catch (e) { return toast(`Couldn't unpark it: ${e}`); }
  if (active?.sid === sid) active = null;
  toast("Unparked: its conversation is still in History");
}
/** The last drawer (below Recently answered), headed "▸ PARKED 8 · CLOSED 30": each word opens its list,
 *  Parked (the latest parked first) or Closed (what ended in the last 30 days, at most 30, to find the
 *  ones that got lost). One slim line per session; a list too long for its room ends on "N more". */
const pkTab = () => { try { return localStorage.getItem("cue.pkTab") === "closed" ? "closed" : "parked"; } catch { return "parked"; } };
/** A parked or closed session in Active: its last two turns (more on request), where it stopped, Resume
 *  (and Unpark for a parked one, Park for a closed one). */
function parkedPane(p) {
  const sid = p.session_id, harness = p.harness, name = p.name || baseName(p.cwd) || p.project || "session", w = waking.has(sid), closed = p.kind === "closed";
  const thread = p.thread || [];
  const shown = parkedAll === sid ? thread : lastTurns(thread, 2);
  const hidden = thread.length - shown.length;
  const more = hidden > 0 ? `<button class="park-more" data-key="more" data-park-all="${esc(sid)}">▴ ${hidden} earlier message${hidden === 1 ? "" : "s"}</button>`
    : parkedAll === sid && lastTurns(thread, 2).length < thread.length ? `<button class="park-more" data-key="more" data-park-all="${esc(sid)}">▾ Just the last two turns</button>` : "";
  const K = keyer();
  const rows = shown.map((e) => keyed(e.role === "you"
    ? `<div class="cv-you"><div class="cv-you-text">${linkify(esc(e.text)).replace(/\n/g, "<br>")}</div>${thumbs(e.images)}<div class="cv-meta">You · ${ago(e.at_ms)} ago</div></div>`
    : `<div class="cv-agent"><div class="cv-meta">${esc(e.role === "peer" ? e.from || "another agent" : agentName(harness))}${machTag(p.machine)} · ${ago(e.at_ms)} ago</div><div class="msg cv-text">${md(addOn.plain(stripStatus(e.text)))}</div></div>`, K(`${e.role[0]}:${e.at_ms}`))).join("");
  const where = p.machine ? ` on ${esc(machShort(p.machine))}` : "";
  const line = w
    ? `<div class="cv-status" data-key="line"><span class="dot-live"></span>Resuming${where}…</div>`
    : `<div class="park-line" data-key="line">${closed ? "" : P_SIGN}${closed ? "Closed" : "Parked"} ${parkedAgo(p)} · ${esc(agentName(harness))} stopped${where} · Resume picks up right here</div>`;
  return `<div class="active-pane parked">
    <div class="ap-head">${projBtn(p.project, "ap")}${badge(harness)}<span class="proj">${esc(name)}</span>${machTag(p.machine)}<span class="pill soft park-pill">${w ? "resuming…" : `${closed ? "" : P_SIGN}${p.kind} · ${now() - p.at < 60000 ? "now" : ago(p.at)}`}</span>${closed ? "" : starBtn(sid)}<span class="grow"></span>
      ${closed ? `<button class="btn" data-park-closed="${esc(sid)}" ${w ? "disabled" : ""}>Park</button>` : `<button class="btn" data-unpark="${esc(sid)}" ${w ? "disabled" : ""}>Unpark</button>`}
      <button class="btn primary" data-wake="${esc(sid)}" ${w ? "disabled" : ""}>${w ? "Resuming…" : "Resume"}</button></div>
    ${subHead(p.cwd, "", "")}
    ${chatBox(`p|${sid}`, `${more}${rows || `<div class="dim cv-empty" data-key="empty">Cue kept no messages from it.</div>`}${line}${w ? "" : resumeCost(sid)}`)}
    <div class="foot">${box(`s:${sid}`, `Message ${name}… sending resumes it first`, w ? "Resuming…" : "Resume & send", `data-wake-send="${esc(sid)}"`, false)}</div></div>`;
}

/** Over a restarting session's chat and box: its steps (stop, start, load; hub/mod.rs Restart), the one it's on. */
function restartCover(rs, name) {
  const steps = [["stop", "Stop"], ["start", "Start"], ["load", !rs.mcp ? "Load skills and settings" : rs.servers == null ? "Load MCP servers" : `Load ${rs.servers} MCP server${rs.servers === 1 ? "" : "s"}`]];
  const at = steps.findIndex(([k]) => k === rs.step);
  return `<div class="rs-cover"><div class="rs-card" role="status"><div class="rs-title">Restarting ${esc(name)}</div><div class="rs-bar"><i></i></div>
    <div class="rs-steps">${steps.map(([, l], n) => `<span class="${n < at ? "done" : n === at ? "now" : ""}">${n < at ? "✓ " : ""}${esc(l)}</span>`).join("")}</div>
    <div class="rs-note">Usually a few seconds. The conversation carries on where it was.</div></div></div>`;
}

