// ---------- how work goes out to a hosted lead's team (hub/team_out.rs; docs/dev/cue-teams.md §6) ----------
// The Crew chip at the top right of a hosted Claude lead (split work: Auto, the lead decides; Always; Never; and whether
// a crew member's work is committed once its review, which runs by itself, is clean), Cue's own /crew (Always for one
// message), and a task handed out by drag: text dropped on a hosted
// lead's card under Sessions shows the files it names, with who already changes each, before it goes as a new
// teammate. Cue adds the words; the lead does the rest.

/** Cue's own command: the message after it goes with the Team switch On, this once (hub/team_out.rs with_team). */
const TEAM_CMD = { name: "crew", kind: "cue", hint: "[what to do]", desc: "Split this message among teammates: each independent part to its own" };
/** A hosted Claude Code session: the only kind that leads a team. */
const isTeamLead = (sid) => { const s = sid && sessionOf(sid); return !!s?.hosted && s.harness === "claude"; };
/** /team in a lead's command list (compose.js cmdList), unless its own list has one. */
const teamCmds = (sid, list) => isTeamLead(sid) && !list.some((c) => c.name === "crew") ? [TEAM_CMD] : [];

/** A switch you just clicked, until Cue's state says so too. */
const teamModeNow = new Map();
function teamMode(sid) {
  const was = sessionOf(sid)?.team_mode || "auto", now_ = teamModeNow.get(sid);
  if (now_ === was) teamModeNow.delete(sid);
  return now_ ?? was;
}
/** The commit setting you just clicked, until Cue's state says so too. */
const crewCommitNow = new Map();
function crewCommit(sid) {
  // "wait": every review waits for you; anything else (the default): Cue handles it.
  const was = sessionOf(sid)?.crew_commit === "wait" ? "wait" : "auto", now_ = crewCommitNow.get(sid);
  if (now_ === was) crewCommitNow.delete(sid);
  return now_ ?? was;
}
const TEAM_MODES = [["auto", "Auto"], ["on", "Always"], ["off", "Never"]];
const CREW_COMMITS = [["auto", "Handle it"], ["wait", "Ask me"]];
/** The lead whose Crew menu is open. */
let crewMenuFor = null;
/** Top right of a hosted Claude lead's head, beside ⋯: "Teammates: auto ▾", opening its two settings (split work into
 *  teammates; what happens once a teammate's review, which runs by itself, comes back). Nothing in the message box. */
function crewChip(sid) {
  if (!isTeamLead(sid)) return "";
  const m = teamMode(sid), open = crewMenuFor === sid;
  const seg = (list, cur, attrs) => `<span class="team-sw" role="group">${list.map(([v, label]) => `<button class="${cur === v ? "on" : ""}" ${attrs(v)} aria-pressed="${cur === v}">${label}</button>`).join("")}</span>`;
  const foot = crewCommit(sid) === "wait" ? "Each review waits for your Commit or Send fixes. Pushing stays yours."
    : "Fixes and notes go back to the teammate, up to 2 times; a clean review is committed to its branch. You're asked after that. Pushing stays yours.";
  const menu = !open ? "" : `<div class="crew-menu"><div class="crew-q">Split work into teammates</div>${seg(TEAM_MODES, m, (v) => `data-team-sw="${esc(sid)}" data-team-mode="${v}"`)}<div class="crew-q">When a review comes back</div>${seg(CREW_COMMITS, crewCommit(sid), (v) => `data-crew-commit="${esc(sid)}" data-crew-mode="${v}"`)}<div class="crew-foot">${foot}</div></div>`;
  const word = { auto: "auto", on: "always", off: "never" }[m] || "auto";
  return `<span class="crew-pick"><button class="tm-chip ${open ? "on" : ""}" data-crew-menu="${esc(sid)}">Teammates: ${word} <span class="tm-sub">▾</span></button>${menu}</span>`;
}
async function setCrewCommit(sid, mode) {
  crewCommitNow.set(sid, mode);
  renderMain();
  try { await invoke("crew_commit", { sessionId: sid, mode }); }
  catch (e) { crewCommitNow.delete(sid); toast(`Couldn't: ${e}`); renderMain(); }
}
async function setTeamMode(sid, mode) {
  teamModeNow.set(sid, mode);
  renderMain();
  try { await invoke("team_mode", { sessionId: sid, mode }); }
  catch (e) { teamModeNow.delete(sid); toast(`Couldn't: ${e}`); renderMain(); }
}

// ---------- drag a task onto a lead ----------
/** The task dropped on a lead's card, until it's sent or cancelled: its lead, its words, and what Cue says about the
 *  files it names (null while that's read). */
let handOut = null;   // { sid, text, preview: { files: [{ path, by }], overlap } | null, err }

/** Under the lead's card while a task dropped on it waits: where it goes, the files it would likely touch (amber: a
 *  teammate, or the add-on's work there, already changes it), Hand out (Send anyway when one is taken) and Cancel. */
function handOutHtml(s) {
  if (!handOut || handOut.sid !== s.session_id) return "";
  const p = handOut.preview, files = p?.files || [];
  const task = handOut.text.length > 160 ? `${handOut.text.slice(0, 160)}…` : handOut.text;
  const list = !p ? `<div class="ho-dim">${handOut.err ? esc(handOut.err) : "Reading what it would touch…"}</div>`
    : files.length ? `<div class="ho-cap">Files it would likely touch</div>${files.map((f) => `<div class="ho-f"><span class="mono">${esc(f.path)}</span>${f.by ? `<span class="ho-by">${esc(f.by)}</span>` : `<span class="ho-dim">free</span>`}</div>`).join("")}`
    : `<div class="ho-dim">It names no files: the lead settles what it touches.</div>`;
  const go = p?.overlap ? "Send anyway" : "Hand out";
  return `<div class="ho" data-ho="${esc(s.session_id)}"><div class="ho-title">Hand to ${esc(nameOf(s.session_id, s.project))}'s teammates</div><div class="ho-task">${esc(task)}</div>${list}<div class="ho-acts"><button class="btn primary" data-ho-go ${p ? "" : "disabled"}>${go}</button><button class="btn" data-ho-cancel>Cancel</button></div></div>`;
}

/** The lead card under the pointer, while something with text is dragged: its session, when it's a hosted lead. */
function leadUnder(e) {
  const card = e.target.closest?.(".working[data-session], .working .team-top[data-session]");
  const sid = card?.dataset.session;
  if (!sid || !isTeamLead(sid) || !(e.dataTransfer?.types || []).includes("text/plain")) return null;
  return { sid, card: card.closest(".working") };
}
let handOver = null;   // the card lit while a drag is over it
const unlight = () => { handOver?.classList.remove("ho-over"); handOver = null; };

async function dropOnLead(sid, text) {
  handOut = { sid, text, preview: null, err: "" };
  renderMain();
  try { const preview = await invoke("team_hand_out_preview", { sessionId: sid, text }); if (handOut?.sid === sid && handOut.text === text) handOut.preview = preview; }
  catch (e) { if (handOut?.sid === sid) handOut.err = `Couldn't read it: ${e}`; }
  renderMain();
}
async function sendHandOut() {
  const h = handOut;
  if (!h) return;
  handOut = null;
  renderMain();
  try { toast(await invoke("team_hand_out", { sessionId: h.sid, text: h.text })); }
  catch (e) { handOut = h; toast(`Couldn't hand it out: ${e}`); renderMain(); }
}

if (typeof document !== "undefined") {
  document.addEventListener("dragover", (e) => {
    const at = leadUnder(e);
    if (!at) return unlight();
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
    if (handOver !== at.card) { unlight(); handOver = at.card; handOver.classList.add("ho-over"); }
  });
  document.addEventListener("dragleave", (e) => { if (handOver && !handOver.contains(e.relatedTarget)) unlight(); });
  document.addEventListener("drop", (e) => {
    const at = leadUnder(e);
    unlight();
    if (!at) return;
    e.preventDefault();
    const text = (e.dataTransfer.getData("text/plain") || "").trim();
    if (text) dropOnLead(at.sid, text);
  });
  document.addEventListener("click", (e) => {
    // The Crew menu closes on a click anywhere outside it.
    if (crewMenuFor && !e.target.closest?.(".crew-pick")) { crewMenuFor = null; renderMain(); }
    const el = e.target.closest?.("[data-crew-menu], [data-crew-commit], [data-team-sw], [data-ho-go], [data-ho-cancel], [data-ho]");
    if (!el) return;
    e.stopPropagation();
    if (el.dataset.crewMenu) { crewMenuFor = crewMenuFor === el.dataset.crewMenu ? null : el.dataset.crewMenu; return renderMain(); }
    if (el.dataset.crewCommit) return setCrewCommit(el.dataset.crewCommit, el.dataset.crewMode);
    if (el.dataset.teamSw) return setTeamMode(el.dataset.teamSw, el.dataset.teamMode);
    if (el.matches("[data-ho-go]")) return sendHandOut();
    if (el.matches("[data-ho-cancel]")) { handOut = null; return renderMain(); }
    // A click elsewhere in the panel doesn't open the lead under it.
  }, true);
}
