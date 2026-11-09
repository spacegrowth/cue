// ---------- how work goes out to a hosted lead's team (hub/team_out.rs; docs/dev/cue-teams.md §6) ----------
// The Team switch on a hosted Claude lead's box (Auto: the lead decides; On: what you send is split among teammates;
// Off: no teammates), Cue's own /team (On for one message), and a task handed out by drag: text dropped on a hosted
// lead's card under Sessions shows the files it names, with who already changes each, before it goes as a new
// teammate. Cue adds the words; the lead does the rest.

/** Cue's own command: the message after it goes with the Team switch On, this once (hub/team_out.rs with_team). */
const TEAM_CMD = { name: "team", kind: "cue", hint: "[what to do]", desc: "Split this message among teammates: each independent part to its own" };
/** A hosted Claude Code session: the only kind that leads a team. */
const isTeamLead = (sid) => { const s = sid && sessionOf(sid); return !!s?.hosted && s.harness === "claude"; };
/** /team in a lead's command list (compose.js cmdList), unless its own list has one. */
const teamCmds = (sid, list) => isTeamLead(sid) && !list.some((c) => c.name === "team") ? [TEAM_CMD] : [];

/** A switch you just clicked, until Cue's state says so too. */
const teamModeNow = new Map();
function teamMode(sid) {
  const was = sessionOf(sid)?.team_mode || "auto", now_ = teamModeNow.get(sid);
  if (now_ === was) teamModeNow.delete(sid);
  return now_ ?? was;
}
const TEAM_MODES = [["auto", "Auto"], ["on", "On"], ["off", "Off"]];
/** Beside Send in a lead's box: "Team  Auto · On · Off". */
function teamSwitch(sid) {
  if (!isTeamLead(sid)) return "";
  const m = teamMode(sid);
  return `<span class="team-sw" role="group" aria-label="Team"><span class="team-sw-l">Team</span>${TEAM_MODES.map(([v, label]) => `<button class="${m === v ? "on" : ""}" data-team-sw="${esc(sid)}" data-team-mode="${v}" aria-pressed="${m === v}">${label}</button>`).join("")}</span>`;
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
  return `<div class="ho" data-ho="${esc(s.session_id)}"><div class="ho-title">Hand to ${esc(nameOf(s.session_id, s.project))} as a new teammate</div><div class="ho-task">${esc(task)}</div>${list}<div class="ho-acts"><button class="btn primary" data-ho-go ${p ? "" : "disabled"}>${go}</button><button class="btn" data-ho-cancel>Cancel</button></div></div>`;
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
    const el = e.target.closest?.("[data-team-sw], [data-ho-go], [data-ho-cancel], [data-ho]");
    if (!el) return;
    e.stopPropagation();
    if (el.dataset.teamSw) return setTeamMode(el.dataset.teamSw, el.dataset.teamMode);
    if (el.matches("[data-ho-go]")) return sendHandOut();
    if (el.matches("[data-ho-cancel]")) { handOut = null; return renderMain(); }
    // A click elsewhere in the panel doesn't open the lead under it.
  }, true);
}
