// Cue UI core — the Tauri bridge, the window's state, and the helpers every view shares (formatting, markdown, names, crews, caches).
const T = window.__TAURI__;
const invoke = T.core.invoke;

let state = { items: [], history: [], sessions: [], settings: {}, connections: {}, window_ms: 1800000, now_ms: Date.now() };
let clockSkew = 0;          // backend clock − browser clock, so bars line up with the backend's timestamps
let sheet = null;           // null | "settings"
let view = "board";         // "board" | "sessions" | "history" — the switch in the header
let histOpen = null;        // the History row that's open (one at a time)
let histShown = null;       // the open row we've already scrolled to (so a redraw doesn't yank it back)
let active = null;          // { id, sid }: what the Active pane shows. Sticky: it stays after you act.
let holdChat = null;        // earlier messages just went in above: keep this distance from the chat's bottom
let activeKey = null;       // last rendered Active target + message count, to scroll the chat on change
let menuFor = null;         // decision card whose "Always…" menu is open
let redirectFor = null;     // decision card showing its "tell it instead" field
const cleared = new Set();  // finished turns you cleared here (their fading row says so)
const ghosts = new Map();   // id -> { it, at, h }: just-answered rows that linger in Waiting (h: the card's height before)
let folding = false;        // a ✓ row is folding away: redraws wait, or they'd pop it back mid-fold
const answering = new Map(); // id -> when you answered it here: it fades at once, and folds LINGER_MS after that
const openMsgs = new Set(); // older agent messages unfolded in the Active chat
const openFollow = new Set();  // cards whose "After a stop hook" is expanded
let dictating = null;       // { key, base, stopping } — the text box you're dictating into, and what it held before
let renaming = null;        // the session whose name is being edited in the Active pane
/** A model id as a label: "claude-opus-5-5[1m]" -> "opus-5-5", "anthropic/claude-sonnet-5-5" -> "sonnet-5-5". */
const shortModel = (m) => String(m || "").replace(/\[.*\]$/, "").replace(/^.*\//, "").replace(/^claude-/, "");
let revealed = "";             // the selection last scrolled into view (only a new one scrolls)
const seenCards = new Set();  // Waiting cards already on screen once (only new arrivals animate)
let cardsPrimed = false;        // the first draw just takes stock: nothing "arrives" at launch
let slideDir = 0;           // +1 / -1: the next redraw slides the Active pane in from the right / left
let chipNav = null;         // the header chip you're stepping through ("waiting" | "asking" | "working")
let forward = null;         // { key, text, from, fromSid, to } — an agent message being sent on to another session
let lightbox = null;        // { srcs: [...], i } — an image opened full size
let lastSent = null;        // { sid, text, images, via, at }: how the last message went out
const drafts = {};          // per item: { text, choices: { [question]: label | Set } }

// ---------- helpers ----------
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const now = () => Date.now() + clockSkew;
function ago(ms) {
  const s = Math.max(0, Math.round((now() - ms) / 1000));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
}
const firstLine = (s) => String(s ?? "").split("\n").find((l) => l.trim()) || "";
const shortPath = (p) => { const parts = String(p).split("/"); return parts.length > 3 ? "…/" + parts.slice(-3).join("/") : p; };
const questions = (it) => it.tool_input?.questions || [];
const tool = (it) => (it.tool_name || "").toLowerCase();
const isBash = (it) => (it.kind === "permission" || it.kind === "terminal") && tool(it) === "bash";
/** A prompt waiting in its terminal that Cue can't answer (a sandboxed command's network access): you go there. */
const inTerminal = (it) => it?.kind === "terminal";
/** How each agent is named on screen. */
const agentName = (h) => ({ claude: "Claude Code", codex: "Codex", pi: "Pi" }[h] || h || "agent");
// Each agent gets a Moss letter. (Anthropic's logo needs their written permission, so it isn't used.)
/** The agent's mark: a letter in its own colour (Claude's orange, Pi's black), so you can tell them apart at a glance. */
const badge = (h) => `<span class="badge h-${esc(h || "other")}" title="${esc(agentName(h))}">${h === "pi" ? "π" : h === "codex" ? "X" : esc((h || "?")[0].toUpperCase())}</span>`;
const draft = (id) => (drafts[id] ||= { text: "", choices: {}, images: [] });
/** A session's name. */
const nameSpan = (sid, project, cls = "proj") => `<span class="${cls}">${esc(nameOf(sid, project))}</span>`;
/** A machine's name, short enough for a card: its first part (no user@, no domain), and past 10 characters
 *  its first 5 and last 5. What you renamed it to comes first. (Notifications do the same: machines::display_name.) */
const machShort = (m) => {
  const label = machinesList?.find((x) => x.name === m)?.label;
  if (label) return label.length > 10 ? `${label.slice(0, 5)}…${label.slice(-5)}` : label;
  const h = String(m || "").replace(/^[^@]*@/, "");
  const n = /^[\d.]+$/.test(h) ? h : h.split(".")[0] || h;
  return n.length > 10 ? `${n.slice(0, 5)}…${n.slice(-5)}` : n;
};
/** The label on everything about a session that runs on another machine (none for this Mac). */
const machTag = (m) => m ? `<span class="mach" title="Runs on ${esc(m)}">${esc(machShort(m))}</span>` : "";
/** What Cue calls a session: the name you gave it, else its project. */
// ([Lead] / [Exec], which relay puts in front of a name, isn't shown: the LEAD / EXEC tag says it.)
const nameOf = (sid, project) => String(sessionOf(sid)?.name || "").replace(/^\[(?:ex-)?(?:Exec|Lead)\]\s*/i, "") || project;
const findItem = (id) => state.items.find((i) => i.id === id) || state.history.find((i) => i.id === id);
const sessionOf = (sid) => state.sessions.find((s) => s.session_id === sid);
/** A session you parked (stopped, kept to resume): not live, so not in state.sessions. */
const parkedOf = (sid) => (state.parked || []).find((p) => p.session_id === sid);
/** Sessions that ended in the last 30 days (at most 30), the Parked drawer's Closed tab: asked for when
 *  that tab shows (again after a minute), not part of every state update. */
let closedList = null, closedAt = 0, closedLoading = false;
const closedOf = (sid) => (closedList || []).find((c) => c.session_id === sid);
async function loadClosed() {
  if (closedLoading) return;
  closedLoading = true;
  try { closedList = await invoke("closed_sessions"); closedAt = now(); }
  catch (e) { closedList ||= []; toast(`Couldn't list closed sessions: ${e}`); }
  finally { closedLoading = false; renderMain(); }
}
/** The open session is a parked or a closed one (Active shows it read-only, with Resume): its record,
 *  with `kind` and when it stopped (`at`). */
function parkedActive() {
  if (!active?.sid || sessionOf(active.sid)) return null;
  const p = parkedOf(active.sid);
  if (p) return { ...p, kind: "parked", at: p.parked_ms };
  const c = closedOf(active.sid);
  return c ? { ...c, kind: "closed", at: c.ended_ms } : null;
}

// ---------- leads and executors (claude-relay, pi-lead) ----------
/** A session's place in a relay / pi-lead crew: { role: "lead"|"executor", name, lead, lead_name, … }, or undefined. */
const crewOf = (sid) => state.crew?.[sid];
// ---------- a project's color: its dot on every card, row and tile, picked by folder name ----------
/** The swatch (0–7) for a folder name: the one you picked in Settings, else a hash of the name, so it's
 *  the same after a restart and on every machine. */
function projIdx(project) {
  const c = state.settings?.project?.colors?.[project];
  if (Number.isInteger(c) && c >= 0 && c < 8) return c;
  let h = 0;
  for (const ch of String(project || "")) h = (h * 31 + ch.charCodeAt(0)) % 9973;
  return h % 8;
}
const projStyle = (project) => `--pc:var(--pc${projIdx(project)})`;
/** The dot: solid while the session waits or idles, pulsing while it works. */
const projDot = (project, live = false) => project ? `<i class="pdot ${live ? "live" : ""}" style="${projStyle(project)}"></i>` : "";
let colorFor = null;   // { p, w }: the project whose swatches are open, and where ("ap": the pane head, "pj": the projects list)
/** The dot as a button: a click opens the swatches under it (the folder's name above them). */
function projBtn(project, where) {
  if (!project) return "";
  const open = colorFor?.p === project && colorFor.w === where;
  const pop = open ? `<div class="pc-pop"><div class="pc-title"><b>${esc(project)}</b> · its color, for every session there</div><div class="pc-row">${[...Array(8).keys()].map((i) => `<button class="pc-sw ${projIdx(project) === i ? "on" : ""}" style="--c:var(--pc${i})" data-pcpick="${esc(project)}" data-pcidx="${i}" aria-label="Swatch ${i + 1}"></button>`).join("")}</div></div>` : "";
  return `<span class="pc-wrap"><button class="pdot-btn ${open ? "on" : ""}" data-pcolor="${esc(project)}" data-pcwhere="${where}" aria-label="Color for ${esc(project)}">${projDot(project)}</button>${pop}</span>`;
}
function setProjColor(project, idx) {
  const colors = { ...(state.settings?.project?.colors || {}), [project]: idx };
  ((state.settings ||= {}).project ||= {}).colors = colors;   // shown now; Cue's next update says the same
  setSetting("project.colors", colors);
}
// ---------- one project at a time: the Projects chip in the header hides every other project's cards ----------
let projFilter = (() => { try { return localStorage.getItem("cue.projFilter") || ""; } catch { return ""; } })();
let pjOpen = false;
const inProj = (project) => !projFilter || project === projFilter;
const roleTag = (sid, live = false) => { const m = crewOf(sid); return m ? `<span class="role ${m.role}">${m.role === "lead" ? "LEAD" : "EXEC"}</span>${m.role === "lead" ? autoToggle(sid, m, live) : ""}` : ""; };
/** An executor's lead: a link when Cue has that session, else its name. */
const leadLink = (m) => sessionOf(m.lead) ? `<button class="crew-lead" data-session="${esc(m.lead)}">${esc(m.lead_name || "its lead")}</button>` : `<b>${esc(m.lead_name || "its lead")}</b>`;
const pkt = (n) => n ? `packet ${n}` : "";
/** Under an executor's name: whose it is, which packet, and whether anyone's listening. */
function crewLine(sid, verdict = true) {
  const m = crewOf(sid);
  if (m?.role !== "executor") return "";
  const bits = [pkt(m.packet), exState({ ...m, session_id: sid }).word].filter(Boolean).map(esc);
  const orphan = m.lead_armed === false ? ` · <span class="crew-warn">its lead isn't running</span>` : "";
  const vf = verdict && reported(m.status) && m.verify ? ` ${verifyTag(m.verify)}` : "";
  return `<div class="crew-line">of ${leadLink(m)}${bits.length ? " · " + bits.join(" · ") : ""}${vf}${orphan}</div>`;
}
/** On a Waiting card, under the top row (which is full): the role tag, then an executor's lead
 *  ("EXEC of ● agent-relay · pkt 3 · busy") or a lead's executors ("● LEAD · 3 executors · 1 reported"). */
function cardCrew(sid) {
  const m = crewOf(sid);
  if (m?.role === "executor") return crewLine(sid, false).replace('<div class="crew-line">', `<div class="crew-line">${roleTag(sid)} `);
  if (m?.role !== "lead") return "";
  const n = (m.executors || []).length;
  return `<div class="crew-line">${roleTag(sid)} ${n ? `team of ${n}` : "no executors right now"}</div>`;
}
/** An executor that's done its packet and is waiting on its lead's review. */
const reported = (status) => status === "reported" || status === "idle";
/** A relay / pi-lead button: Review types the command into the lead; Diff opens a page. */
const crewBtn = (sid, action, label, primary = false, short = "") => `<button class="btn ${primary ? "primary" : ""}" data-crew="${action}" data-crew-sid="${esc(sid)}">${short ? lbl(label, short) : esc(label)}</button>`;
/** Diff: relay's diff page comes from relay's command, here or on its machine; pi-lead writes its page on the
 *  machine it runs on and Cue can't bring that one here, so a pi-lead executor on another machine has no Diff. */
const diffBtn = (sid, m) => m?.plugin === "pilead" && m.machine ? "" : crewBtn(sid, "diff", "Diff");
/** "in review" once you've sent an executor's review, "review queued" while it waits for its lead,
 *  "reviewed" once its lead has seen the report (Cue tells every screen), else "". */
const reviewWord = (sid) => { const w = state.reviews?.[sid] || ""; return w === "not picked up" ? "" : w; };
/** A report relay never woke its lead for: ready a while, unseen, its lead listening and free. Cue says so
 *  instead of "waiting for review"; Review in lead stays. */
const missedWake = (sid) => state.reviews?.[sid] === "not picked up";
/** (the narrow team tree on the board says just "not picked up") */
const NOT_PICKED_UP = "its lead hasn't picked this up";
/** An executor whose agent exited before it reported (stalled: it never got past a trust prompt, say; dead:
 *  its process is gone): nothing will ever report, so its row says so and offers Close. */
const stalled = (status) => status === "stalled" || status === "dead";
const stalledWord = (status) => status === "dead" ? "stopped: its agent is gone" : "stalled: its agent exited";
/** relay's auto mode beside a lead's LEAD tag: a switch in its chat (`live`: click to flip it),
 *  and on cards and rows just "auto" while it's on. In the chat, hover says what it means. */
const autoToggle = (sid, m, live = false) => {
  if (m?.plugin !== "relay") return "";
  const why = m.auto ? "Goes ahead on routine steps without asking. Commits still wait for you." : "Waits for you at every step.";
  return live ? `<button class="auto-tg ${m.auto ? "on" : ""}" data-crew="${m.auto ? "auto-off" : "auto-on"}" data-crew-sid="${esc(sid)}"><i></i>auto</button>`
    : m.auto ? `<span class="auto-tag">auto</span>` : "";
};
/** A button label with a shorter form the chat's header switches to when the pane is narrow (see .active-pane in the CSS). */
const lbl = (long, short) => `<span class="l-long">${esc(long)}</span><span class="l-short">${esc(short)}</span>`;
/** relay verify's verdict on a reported executor: just ✓ or ✕, what it means on hover. It checks
 *  that every file the report says it changed is staged, not that the work is right. */
function verifyTag(v) {
  if (!v?.verdict) return "";
  if (v.verdict === "COUNTS-MATCH") return `<span class="vf ok" title="Report matches what's staged">✓</span>`;
  const why = v.verdict === "MALFORMED" ? "its report's summary is missing or broken" : v.note || "its report lists changes that aren't staged";
  return `<span class="vf bad" title="Report doesn't match (relay verify): ${esc(why)}">✕</span>`;
}
/** The verdict as a small tag (the executor tag's look): MATCH, or MISMATCH / MALFORMED in the warning color. */
const verdictTag = (v) => !v?.verdict ? "" : `<span class="role vt${v.verdict === "COUNTS-MATCH" ? "" : " bad"}">${esc(v.verdict === "COUNTS-MATCH" ? "MATCH" : v.verdict)}</span>`;
/** Under an executor's name in the tree: its report's first line once it has reported, else its packet's goal. One line. */
const goalLine = (e) => { const t = e.outcome || e.goal; return t ? `<div class="tn-g">${esc(t)}</div>` : ""; };
async function crewAct(sid, action) {
  try {
    toast(await invoke("crew_action", { sessionId: sid, action }));
    // The review happens in the lead: show it there, so its answer arrives in front of you.
    const lead = action === "review" ? crewOf(sid)?.lead : null;
    if (lead && sessionOf(lead) && active?.sid !== lead) setActive(null, lead);
  } catch (e) { toast(`Couldn't ${action}: ${e}`); }
}
// ---------- teams: a lead and its executors, shown as one ----------
// The same team bar in the lead's chat and each executor's (lead first, then executors as they started;
// they never reorder), then one card for where you are: an executor's packet, or what the team needs
// from you in the lead's. On the board, a lead's card holds its executors as a small tree.
/** An executor by its session id: its own crew record (a session Cue holds), else its line on its lead's
 *  executor list (one Cue doesn't hold, on another machine say), with its lead filled in. Null for anything else. */
function execOf(sid) {
  const m = crewOf(sid);
  if (m?.role === "executor") return m;
  for (const [lead, lm] of Object.entries(state.crew || {})) {
    const e = (lm.executors || []).find((x) => x.session_id === sid);
    if (e) return { ...e, role: "executor", lead, lead_name: lm.name, lead_armed: true };
  }
  return null;
}
/** An executor on another machine that runs in a tmux session of its own: its terminal attaches to that
 *  session (never its lead's), whether or not Cue holds the session. */
const execTerm = (sid) => { const e = sid && execOf(sid); return e?.machine && e.tmux_session ? e : null; };
/** Where a row in a lead's tree goes: a remote executor's own terminal, else its chat when Cue has it. */
const exGo = (sid) => execTerm(sid) ? `data-exec-term="${esc(sid)}" role="button"` : sessionOf(sid) ? `data-session="${esc(sid)}"` : "";
/** The team a session is in: its lead (session id, name) and executors, or null. */
function teamOf(sid) {
  const m = execOf(sid) || crewOf(sid);
  if (!m) return null;
  const lead = m.role === "lead" ? sid : m.lead, lm = crewOf(lead);
  const ex = lm?.executors?.length ? lm.executors : m.role === "executor" ? [{ ...m, session_id: sid }] : [];
  return { lead, leadName: lm?.name || m.lead_name || "its lead", ex };
}
/** Something an executor asks you (a permission or a question): you, not its lead, answer that. */
const asksOf = (sid) => state.items.find((i) => i.session_id === sid && i.kind !== "waiting");
/** An executor's state, in a word or two. */
/** What a working session is doing right now ("Running: cargo test"), from its transcript, and since
 *  when: Cue reads it every few seconds, so it follows the agent step by step. Empty when unknown. */
const liveStep = (sid) => { const s = sessionOf(sid); return s?.state === "working" && s.doing ? { what: s.doing, at: s.doing_ms } : null; };
function exState(e) {
  if (asksOf(e.session_id)) return { word: "needs you", cls: "need" };
  if (reported(e.status)) return { word: "done", cls: "done" };
  if (e.status === "busy") return { word: "working", cls: "busy" };
  return { word: { stalled: "stalled", closed: "closed", superseded: "replaced", dead: "stopped" }[e.status] || e.status || "", cls: "" };
}
/** Under the chat's header, lead's or executor's: the whole team, this one lit; each chip ends with its role
 *  (lead / exec); a chip opens that chat. */
function teamBar(sid) {
  const t = teamOf(sid);
  if (!t) return "";
  const chip = (id, name, word, cls) => {
    const me = id === sid, go = !me && sessionOf(id);
    return `<${go ? "button" : "span"} class="tm-chip ${cls} ${me ? "me" : ""}" ${go ? `data-session="${esc(id)}"` : ""}>${cls === "busy" ? `<span class="dot-live"></span>` : ""}${esc(name)}${word ? ` <span class="tm-sub">${esc(word)}</span>` : ""}${cls !== "lead" ? ` <span class="tm-sub tm-role">exec</span>` : ""}</${go ? "button" : "span"}>`;
  };
  const lead = sessionOf(t.lead) || t.lead === sid ? chip(t.lead, t.leadName, "lead", "lead") : `<span class="tm-chip gone">${esc(t.leadName)} <span class="tm-sub">lead · not running</span></span>`;
  return `<div class="team-bar"><span class="tm-label">${esc(t.leadName)}${/s$/i.test(t.leadName) ? "'" : "'s"} team</span>${lead}${t.ex.map((e) => { const st = exState(e); return chip(e.session_id, e.name, st.word, st.cls); }).join("")}</div>`;
}
/** relay verify's verdict, in words: it checks every file the report lists is staged, not that the work is right. */
const verifyWords = (v) => !v?.verdict ? "" : v.verdict === "COUNTS-MATCH" ? "its report matches what's staged"
  : `<span class="crew-warn">its report doesn't match what's staged${v.verdict === "MALFORMED" ? " (its summary is missing or broken)" : v.note ? ` (${esc(v.note)})` : ""}</span>`;
/** An executor's chat: its packet (what it's for), where it stands, and Diff / Review once it's done. */
function packetCard(sid, m = crewOf(sid)) {
  if (m?.role !== "executor") return "";
  const lead = esc(m.lead_name || "its lead");
  let st, acts = "";
  if (reported(m.status)) {
    const asked = reviewWord(sid);
    st = [m.outcome ? `Done: ${esc(m.outcome)}` : "Done", asked === "reviewed" ? `reviewed by ${lead}` : asked ? `${asked} with ${lead}` : missedWake(sid) ? `${lead} hasn't picked this up` : "waiting for review", verifyWords(m.verify)].filter(Boolean).join(" · ");
    acts = diffBtn(sid, m) + (m.lead_armed && !asked ? crewBtn(sid, "review", "Review in lead", true) : "");
  } else {
    const now = liveStep(sid);
    st = now ? `${esc(now.what)} · when it reports, ${lead} reviews it` : m.status === "busy" ? `Working on it · when it reports, ${lead} reviews it` : esc(exState(m).word || m.status || "");
  }
  const gone = m.lead_armed === false ? `<div class="crew-warn">Its lead (${lead}) isn't running, so ${reported(m.status) ? "its report reaches no one" : "no one will review this"}.</div>` : "";
  return `<div class="team-card"><div class="tc-h">PACKET${m.packet ? ` ${m.packet}` : ""}</div>${m.goal ? `<div class="tc-goal">${esc(m.goal.charAt(0).toUpperCase() + m.goal.slice(1))}</div>` : ""}
    <div class="tc-st">${st}</div>${gone}${acts ? `<div class="tc-acts">${acts}</div>` : ""}</div>`;
}
/** A lead's chat: what its team needs from you (asks, finished packets), then its plan and auto mode. */
function leadCard(sid) {
  const m = crewOf(sid);
  if (m?.role !== "lead") return "";
  const rows = (m.executors || []).flatMap((e) => {
    const ask = asksOf(e.session_id);
    if (ask) return [`<div class="tc-row need" data-big="${esc(ask.id)}" role="button"><span><b>${esc(e.name)}</b> ${esc(verb(ask))} <span class="${isBash(ask) ? "mono" : ""}">${esc(plain(summary(ask)))}</span></span>${ask.kind === "permission" ? `<span class="tc-acts"><button class="btn deny" data-act="deny" data-id="${esc(ask.id)}">Deny</button><button class="btn primary" data-act="allow" data-id="${esc(ask.id)}">Allow</button></span>` : ""}</div>`];
    if (reported(e.status) && !reviewWord(e.session_id)) return [`<div class="tc-row"><span><b>${esc(e.name)}</b> is done${e.outcome ? `: ${esc(e.outcome)}` : ""}${missedWake(e.session_id) ? ` · ${NOT_PICKED_UP}` : ""}</span><span class="tc-acts">${diffBtn(e.session_id, e)}${crewBtn(e.session_id, "review", "Review", true)}</span></div>`];
    return [];
  });
  // Its plan: what it sends next, and how many wait. (Auto mode is the switch beside its LEAD tag.)
  const plan = m.plan_queued ? `<span class="crew-plan">Next: <b>${esc(m.plan_next)}</b> · ${m.plan_queued} more queued</span>` : "";
  return `<div class="team-card">${rows.join("") || `<div class="tc-st">Nothing from the team needs you.</div>`}${plan ? `<div class="tc-foot">${plan}</div>` : ""}</div>`;
}
/** On the board, inside a lead's card: its executors as a tree. What needs you, with its buttons; what's
 *  done, with Diff / Review; the rest, one line each (past five, a count). */
const TREE_BUSY = 5;
function teamTree(lead) {
  const openSid = quietOpen || active?.sid;
  const ex = crewOf(lead)?.executors || [];
  if (!ex.length) return "";
  const busy = [], rows = [];
  for (const e of ex) {
    const ask = asksOf(e.session_id), on = e.session_id === openSid ? "on" : "";
    if (ask) rows.push(`<div class="tn need ${on}" data-big="${esc(ask.id)}"><div class="tn-t"><b>${esc(e.name)}</b><span class="tn-need">needs you</span></div><div class="tn-x">${esc(verb(ask))} <span class="${isBash(ask) ? "mono" : ""}">${esc(plain(summary(ask)))}</span></div>${ask.kind === "permission" ? `<div class="tn-acts"><button class="btn deny" data-act="deny" data-id="${esc(ask.id)}">Deny</button><button class="btn primary" data-act="allow" data-id="${esc(ask.id)}">Allow</button></div>` : ""}</div>`);
    else if (reported(e.status) && reviewWord(e.session_id)) rows.push(`<div class="tn ${on}" ${exGo(e.session_id)}><div class="tn-t"><b>${esc(e.name)}</b><span>✓ done · ${reviewWord(e.session_id)}</span>${verdictTag(e.verify)}</div>${goalLine(e)}</div>`);
    else if (reported(e.status)) rows.push(`<div class="tn ${on}" ${exGo(e.session_id)}><div class="tn-t"><b>${esc(e.name)}</b><span>✓ done, ${missedWake(e.session_id) ? "not picked up" : "waiting for review"}</span>${verdictTag(e.verify)}</div>${goalLine(e)}<div class="tn-acts">${diffBtn(e.session_id, e)}${crewBtn(e.session_id, "review", "Review in lead", true)}</div></div>`);
    else if (stalled(e.status)) rows.push(`<div class="tn ${on}" ${exGo(e.session_id)}><div class="tn-t"><b>${esc(e.name)}</b><span>${stalledWord(e.status)}</span></div>${goalLine(e)}<div class="tn-acts">${crewBtn(e.session_id, "close", "Close")}</div></div>`);
    else busy.push(e);
  }
  const line = (e) => `<div class="tn ${e.session_id === openSid ? "on" : ""}" ${exGo(e.session_id)}><div class="tn-t">${e.status === "busy" ? `<span class="dot-live"></span>` : ""}<b>${esc(e.name)}</b>${(() => { const st = liveStep(e.session_id); return st ? `<span class="tn-x">${esc(st.what)}</span><span class="tn-age" data-ago="${st.at}">${ago(st.at)}</span>` : `<span class="tn-x">${esc(exState(e).word)}</span>`; })()}</div>${goalLine(e)}</div>`;
  // A big team: five working lines, then a count.
  const more = busy.length > TREE_BUSY ? [`<div class="tn"><div class="tn-t tn-x">+${busy.length - TREE_BUSY} more working</div></div>`] : [];
  return `<div class="team-tree">${[...rows, ...busy.slice(0, TREE_BUSY).map(line), ...more].join("")}</div>`;
}
const finishedText = (it) => it.message || it.context?.at(-1)?.text || "Finished. Your turn.";
/** The message as one plain line of prose, for previews (no markdown punctuation). */
const plain = (s) => String(s ?? "").replace(/```[\s\S]*?```/g, " ").replace(/[`*#]/g, "").replace(/^\s*(>\s?)+/gm, "").replace(/^\s*[-•]\s+/gm, "").replace(/\s+/g, " ").trim();

function summary(it) {
  const i = it.tool_input || {};
  if (it.kind === "question") return questions(it)[0]?.question || "Question";
  if (it.kind === "waiting") return firstLine(finishedText(it));
  if (inTerminal(it) && !it.tool_name) return it.message || "Asking in its terminal";
  switch (tool(it)) {
    case "bash": return i.command || "bash";
    case "edit": case "multiedit": case "write": case "notebookedit": return `${it.tool_name} ${shortPath(i.file_path || i.path || "")}`;
    case "webfetch": return `Fetch ${i.url || ""}`;
    default: return it.tool_name || "Request";
  }
}
function verb(it) {
  if (it.kind === "question") return "asks";
  if (inTerminal(it)) return "asks in its terminal";
  return { bash: "wants to run", edit: "wants to edit", multiedit: "wants to edit", write: "wants to write", webfetch: "wants to fetch" }[tool(it)] || `wants ${it.tool_name}`;
}
function describeSuggestion(s) {
  const where = { session: "this session", localSettings: "this project, just you", projectSettings: "this project", userSettings: "everywhere" }[s.destination] || s.destination || "";
  if (s.type === "addRules") return { title: `Always allow ${(s.rules || []).map((r) => (r.ruleContent ? `${r.toolName}(${r.ruleContent})` : r.toolName)).join(", ")}`, where };
  if (s.type === "addDirectories") return { title: `Allow access to ${(s.directories || []).map(shortPath).join(", ")}`, where };
  if (s.type === "setMode") return { title: `Switch to ${s.mode} mode`, where };
  return { title: s.type, where };
}

/** A Markdown table (`| a | b |` lines; an optional `|---|` row makes the line above it the header). */
function table(lines, inline) {
  const cells = (l) => l.trim().replace(/^\||\|$/g, "").split(/(?<!\\)\|/).map((c) => c.trim().replace(/\\\|/g, "|"));
  const SEP = /^\s*\|?(\s*:?-+:?\s*\|)*\s*:?-+:?\s*\|?\s*$/;
  const head = lines.length > 1 && SEP.test(lines[1]) ? cells(lines[0]) : null;
  const body = (head ? lines.slice(2) : lines).filter((l) => !SEP.test(l)).map(cells);
  const row = (cs, tag) => `<tr>${cs.map((c) => `<${tag}>${inline(c)}</${tag}>`).join("")}</tr>`;
  return `<div class="md-table"><table>${head ? `<thead>${row(head, "th")}</thead>` : ""}<tbody>${body.map((cs) => row(cs, "td")).join("")}</tbody></table></div>`;
}

/** Clickable links in already-escaped text: [label](url) and bare URLs (opened by open_link). Markdown links
 *  are set aside first so the bare-URL pass can't reach into their attributes. */
function linkify(h) {
  const links = [];
  const a = (u, label) => `<a href="#" data-href="${u}">${label}</a>`;
  // A path in a message is not a link on a plain click (paths are everywhere in agent text), but
  // ⌘-click opens it (a document in its app; a script or an app only shown in Finder): the click
  // handler sends data-path to open_link.
  const pa = (u) => `<a href="#" data-path="${u}">${u}</a>`;
  return h.replace(/\[([^\]\n]+)\]\(((?:https?|file):\/\/[^\s)]+)\)/g, (_, label, u) => `\u0000${links.push(a(u, label)) - 1}\u0000`)
    .replace(/(?:https?|file):\/\/[^\s<>"'\u0000]*[^\s<>"'.,;:!?)\]\u0000]/g, (u) => a(u, u))
    .replace(/(^|[^\w@/\\<])(\/[\w.@~+-]+(?:\/[\w.@~+-]+)*\/?|~\/[^\s<>"'`]+)/g, (m, pre, raw) => {
      const t = raw.match(/[.,;:!?]+$/)?.[0] || "";
      return pre + pa(raw.slice(0, raw.length - t.length)) + t;
    })
    .replace(/\u0000(\d+)\u0000/g, (_, n) => links[n]);
}
/** Markdown-lite for agents' messages: paragraphs, lists, tables, headings, quotes, code. Escapes first, so it's safe. */
function md(text) {
  const out = [];
  const inline = (s) => linkify(esc(s).replace(/`([^`]+)`/g, "<code>$1</code>").replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>"));
  const blocks = String(text ?? "").replace(/\r/g, "").split(/```/);
  blocks.forEach((chunk, i) => {
    if (i % 2 === 1) {
      // Long code folds to ~12 lines (scrolls inside); a dump doesn't push the conversation away.
      const code = chunk.replace(/^[a-z]*\n/, "").replace(/\n$/, "");
      const n = code.split("\n").length;
      out.push(n > 12 ? `<pre class="long">${esc(code)}</pre><div class="code-more">${n} lines · scroll inside</div>` : `<pre>${esc(code)}</pre>`);
      return;
    }
    const LIST = /^\s*([-*•]|\d+[.)])\s+/;
    const TABLE = /^\s*\|.*\|\s*$/;
    const QUOTE = /^\s*>\s?/;
    // "> " lines are a quote: a line down its side, as in Slack, with Markdown inside.
    for (const para of chunk.split(/\n\s*\n/)) {
      // A paragraph can mix a heading line, text lines and list lines: split it into runs.
      const runs = [];
      for (const l of para.split("\n").filter((x) => x.trim())) {
        const kind = QUOTE.test(l) ? "quote" : TABLE.test(l) ? "table" : LIST.test(l) ? "list" : /^#{1,4}\s/.test(l) || /^\*\*[^*]+\*\*:?$/.test(l.trim()) ? "head" : "text";
        const last = runs.at(-1);
        if (last && last.kind === kind && kind !== "head") last.lines.push(l);
        else runs.push({ kind, lines: [l] });
      }
      for (const r of runs) {
        if (r.kind === "quote") {
          out.push(`<blockquote>${md(r.lines.map((l) => l.replace(QUOTE, "")).join("\n"))}</blockquote>`);
        } else if (r.kind === "table") {
          out.push(table(r.lines, inline));
        } else if (r.kind === "list") {
          const ordered = /^\s*\d/.test(r.lines[0]);
          const start = ordered ? parseInt(r.lines[0], 10) || 1 : 1;
          out.push(`<${ordered ? `ol start="${start}"` : "ul"}>${r.lines.map((l) => `<li>${inline(l.replace(LIST, ""))}</li>`).join("")}</${ordered ? "ol" : "ul"}>`);
        } else if (r.kind === "head") {
          out.push(`<h4>${inline(r.lines[0].replace(/^#+\s/, "").replace(/\*\*/g, ""))}</h4>`);
        } else {
          out.push(`<p>${r.lines.map(inline).join("<br>")}</p>`);
        }
      }
    }
  });
  return out.join("");
}

/** The 30-minute activity bar for a session: solid = working, striped = waiting on you. */
function bar(sid) {
  const s = sessionOf(sid);
  const w = state.window_ms || 1800000;
  const from = now() - w;
  const segs = (s?.segments || []).map((g) => {
    const start = Math.max(g.start_ms, from);
    const end = g.end_ms ?? now();
    const left = ((start - from) / w) * 100;
    const width = Math.max(0.6, ((end - start) / w) * 100);
    const live = g.end_ms == null ? " live" : "";
    return `<i class="${g.kind === "working" ? "w" : g.kind === "idle" ? "i" : "z"}${live}" style="left:${left.toFixed(2)}%;width:${Math.min(width, 100 - left).toFixed(2)}%"></i>`;
  });
  return `<div class="bar" title="last 30 min · solid = working, striped = waiting on you">${segs.join("")}<b class="now"></b></div>`;
}
/** Above the big bar: how long each stretch lasted, over the ones wide enough to hold a label. */
function barLabels(sid) {
  const s = sessionOf(sid), w = state.window_ms || 1800000, from = now() - w;
  const dur = (ms) => ms < 60000 ? `${Math.max(1, Math.round(ms / 1000))}s` : `${Math.round(ms / 60000)}m`;
  const spans = (s?.segments || []).filter((g) => g.kind !== "idle").map((g) => {
    const start = Math.max(g.start_ms, from), end = g.end_ms ?? now();
    return { kind: g.kind, left: ((start - from) / w) * 100, width: ((end - start) / w) * 100, ms: end - start };
  }).filter((g) => g.width >= 6);
  return `<div class="bar-labels">${spans.map((g) => `<span class="${g.kind === "working" ? "w" : "z"}" style="left:${g.left.toFixed(2)}%;width:${g.width.toFixed(2)}%">${dur(g.ms)}</span>`).join("")}</div>`;
}
/** Under the big bar: what the stripes mean and which end is now, so you needn't hover. */
const barKey = () => `<div class="bar-key"><span>30 min ago</span><span class="grow"></span><span><i class="sw w"></i>working</span><span><i class="sw z"></i>waiting on you</span><span class="grow"></span><span>now</span></div>`;

/** The selected card in each list (Waiting, Sessions, Starred, Recently answered), by list, with whether
 *  it shows in full inside the list's scrolling area. */
function selectedCards() {
  const out = new Map();
  for (const el of document.querySelectorAll(".working.on, .nrow.on, .tl.on")) {
    const sc = scrollerOf(el);
    const a = el.getBoundingClientRect(), b = (sc || document.documentElement).getBoundingClientRect();
    out.set(el.closest(".sec, .col")?.className || "", { el, full: a.top >= b.top - 1 && a.bottom <= b.bottom + 1 });
  }
  return out;
}
/** The list that scrolls a card (its nearest scrolling ancestor), or null. */
function scrollerOf(el) {
  let sc = el?.parentElement;
  while (sc && sc !== document.body && !/(auto|scroll)/.test(getComputedStyle(sc).overflowY)) sc = sc.parentElement;
  return sc && sc !== document.body ? sc : null;
}
/** Glide a list just enough to show a card in full (with a little room past it). Driven frame by frame
 *  and found again each frame: a redraw (every few seconds while sessions work) replaces the list, which
 *  would stop the browser's own smooth scroll part-way. Your own scrolling stops it. */
let glide = null;
function glideIntoView(el) {
  const sc = scrollerOf(el);
  if (!sc) return;
  const a = el.getBoundingClientRect(), b = sc.getBoundingClientRect(), M = 12;
  // Cut off below: up, but never past its top. Cut off above: down to its top.
  const delta = a.bottom + M > b.bottom ? Math.min(a.bottom + M - b.bottom, a.top - M - b.top) : a.top - M < b.top ? a.top - M - b.top : 0;
  if (Math.abs(delta) < 1) return;
  const from = sc.scrollTop, to = Math.max(0, Math.min(sc.scrollHeight - sc.clientHeight, from + delta));
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) { sc.scrollTop = to; return; }
  // Found again by its list and what it is (a redraw makes new elements).
  const list = el.closest(".sec, .col"), attr = ["data-session", "data-detail", "data-big"].find((n) => el.hasAttribute(n));
  const key = list && attr ? `.${[...list.classList].join(".")} [${attr}="${CSS.escape(el.getAttribute(attr))}"]` : null;
  const g = (glide = { t0: performance.now() });
  const step = (t) => {
    if (glide !== g) return;
    const s = key ? scrollerOf(document.querySelector(key)) : sc;
    if (!s) { glide = null; return; }
    const k = Math.min(1, (t - g.t0) / 280);
    s.scrollTop = from + (to - from) * (1 - Math.pow(1 - k, 3));   // eases out
    if (k < 1) requestAnimationFrame(step); else glide = null;
  };
  requestAnimationFrame(step);
}

let toastTimer;
// ---------- selecting text: redraws wait while you drag, and letting go copies it ----------
let mouseDown = false, heldDraw = false, selBefore = "", copied = null;   // copied: { at, words } for the pill   // selBefore: what was selected when you pressed
const inField = (n) => !!(n?.nodeType === 1 ? n : n?.parentElement)?.closest?.("input, textarea");
let composing = false;   // an IME composition is in progress in a box: redraws wait, Enter is the IME's
addEventListener("compositionstart", (e) => { if (inField(e.target)) composing = true; });
addEventListener("compositionend", () => { composing = false; if (heldDraw) { heldDraw = false; renderMain(); } });
// The box lost focus mid-composition (a click elsewhere, ⌘Tab): the composition is over for Cue's
// purposes, whether or not its end event came, so redraws never stay held.
addEventListener("focusout", () => { if (composing) { composing = false; if (heldDraw) { heldDraw = false; renderMain(); } } }, true);
// A selection made by this press (a click on a button leaves an older one in place: not that).
const dragSelecting = () => mouseDown && !getSelection().isCollapsed && getSelection().toString() !== selBefore;
// Pressing a link: a redraw before you let go would swap it for a copy, and the click would land on
// nothing (a working session redraws twice a second). It waits until the click has gone through.
let onLink = false;
addEventListener("mousedown", (e) => { mouseDown = e.button === 0 && !inField(e.target); onLink = mouseDown && !!e.target.closest?.("a[data-href], a[data-path]"); selBefore = getSelection().toString(); }, true);
addEventListener("mouseup", () => {
  if (!mouseDown) return;
  const sel = getSelection(), text = sel.toString(), fresh = dragSelecting();
  mouseDown = false;
  const wasLink = onLink;
  onLink = false;
  // Selected text in the window (not in a box you type in, where ⌘C works as ever): on your clipboard now.
  if (fresh && text.trim() && !inField(sel.anchorNode)) {
    if (!document.execCommand("copy")) navigator.clipboard?.writeText(text);
    copied = { at: Date.now(), words: text.length > 40 ? text.trim().split(/\s+/).length : 0 };
    if (!heldDraw && !showCopied()) toast("Copied");
  }
  // Pressed on a link: after the click (it fires after this), so the link is still there for it.
  if (heldDraw) { heldDraw = false; wasLink ? setTimeout(renderMain) : renderMain(); }
}, true);
// Let go outside the window (no mouseup here): the held redraw happens anyway.
addEventListener("blur", () => { mouseDown = onLink = false; if (heldDraw) { heldDraw = false; renderMain(); } });

/** "Copied", in Moss, at the right end just above the chat's text box (or its bottom, with no box). Put back
 *  after each redraw, its fade carrying on where it was. False when there's no chat to show it in. */
const COPIED_MS = 1600;
function showCopied() {
  const left = copied ? COPIED_MS - (Date.now() - copied.at) : 0;
  if (left <= 0) return false;
  const host = document.querySelector(".active-pane .foot") || document.querySelector(".active-pane");
  if (!host) return false;
  host.querySelector(":scope > .copied")?.remove();
  host.insertAdjacentHTML("beforeend", `<span class="copied" style="animation-delay:-${COPIED_MS - left}ms">Copied${copied.words ? ` · ${copied.words} words` : ""}</span>`);
  return true;
}

function toast(msg, ms = 2600) {
  let el = document.querySelector(".toast");
  if (!el) { el = document.createElement("div"); el.className = "toast"; document.body.append(el); }
  el.textContent = msg;
  el.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove("show"), ms);
}
async function setSetting(key, value) {
  try { await invoke("set_setting", { key, value }); } catch (e) { toast(`Couldn't save: ${e}`); }
}
const SEARCH_ICON = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/></svg>`;
const GEAR_ICON = `<svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>`;
const MIC_ICON = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="3" width="6" height="12" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/></svg>`;
const IMAGE_ICON = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="3"/><circle cx="9" cy="10" r="1.7"/><path d="M21 16l-5-5-8 8"/></svg>`;

/** A saved upload as something an <img> can show (Tauri's asset protocol). */
const fileSrc = (path) => (T.core.convertFileSrc ? T.core.convertFileSrc(path) : path);
// ---------- memory: fast first, with a ceiling ----------
/** Switching sessions should be instant, so what a session loads stays: its steps for as long as it's
 *  live (small), and its heavy parts (full tool outputs and diffs, older messages you scrolled back to)
 *  while they fit a generous budget, the least recently opened going first. The budget is a ceiling
 *  for a long day, not something normal use reaches. Images, the newest that fit theirs. Anything
 *  dropped loads again when it's shown. */
const HEAVY_BYTES = 256 * 2 ** 20;  // heavy parts of the sessions besides the open one, roughly
const IMAGE_BYTES = 48 * 2 ** 20;   // images (a screenshot is a few MB as a data: URL)
const recentSids = [];   // most recently opened first
/** Roughly what a cached value holds: its text (two bytes a character), plus a little per value. */
const sizeOf = (v) => (typeof v === "string" ? v.length * 2 : v && typeof v === "object" ? Object.values(v).reduce((n, x) => n + sizeOf(x), 16) : 8);
function seenSession(sid) {
  if (!sid) return;
  const at = recentSids.indexOf(sid);
  if (at >= 0) recentSids.splice(at, 1);
  recentSids.unshift(sid);
  recentSids.length = Math.min(recentSids.length, 50);
  trimCaches();
}
function trimCaches() {
  const sidOf = (k) => String(k).split("|")[0];
  const open = new Set([active?.sid, quietOpen].filter(Boolean));
  const live = new Set([...state.sessions.map((s) => s.session_id), ...(state.live || []).map((q) => q.session_id)]);
  // Steps and commands: kept for every live session (and the ones you opened lately).
  const near = new Set([...open, ...live, ...recentSids]);
  for (const m of [stepFeeds, cmdLists, cmdMissed]) for (const k of [...m.keys()]) if (!near.has(sidOf(k))) m.delete(k);
  for (const k of [...btwLog.keys()]) if (!near.has(k)) btwLog.delete(k);
  // Per-session bits of this window's: a terminal's last line, "!" lines (the + New box's stay), forwardable messages
  // (keyed by session or card id, then ":" and the time).
  for (const k of [...termLast.keys()]) if (!near.has(k)) termLast.delete(k);
  for (const k of [...ranLog.keys()]) if (k !== "new" && !near.has(k)) ranLog.delete(k);
  const cards = new Set(state.items.map((i) => i.id));
  for (const k of [...fwdMsgs.keys()]) { const who = k.slice(0, k.lastIndexOf(":")); if (!near.has(who) && !cards.has(who)) fwdMsgs.delete(k); }
  // The heavy parts: the open session's always; the rest newest first, while they fit.
  const heavy = [older, stepDetail];
  const bytes = (sid) => heavy.reduce((n, m) => n + [...m].reduce((b, [k, v]) => b + (sidOf(k) === sid ? sizeOf(v) : 0), 0), 0);
  const keep = new Set(open);
  let total = 0;
  for (const sid of recentSids) if (!open.has(sid) && (total += bytes(sid)) <= HEAVY_BYTES) keep.add(sid);
  for (const m of heavy) for (const k of [...m.keys()]) if (!keep.has(sidOf(k))) m.delete(k);
  // Images: the newest that fit (always the newest few, however big).
  let img = 0, kept = 0;
  for (const [k, v] of [...localImgs].reverse()) { img += sizeOf(v); if (++kept > 4 && img > IMAGE_BYTES) localImgs.delete(k); }
}

/** What the Active pane shows: your pick (kept after you act), else the oldest thing waiting on you. */
function current() {
  // A parked session is open: nothing live to show, and it stays the one open.
  if (parkedActive()) return null;
  if (active) {
    let it = active.id ? findItem(active.id) : null;
    const s = sessionOf(active.sid);
    // Same session asking again (it finished the turn you replied to): show the new ask in place.
    // Not when you opened an old answer yourself (from Recently answered): then show exactly that.
    if (!active.exact && !isPending(it)) { const next = state.items.find((i) => i.session_id === active.sid); if (next) { it = next; active.id = next.id; } }
    if (it || s) return { it, s };
  }
  const first = needsYou()[0];
  active = first ? { id: first.id, sid: first.session_id } : null;
  return first ? { it: first, s: sessionOf(first.session_id) } : null;
}
/** Where you were: the session open in Active, and which view. Kept across restarts, so reopening Cue
 *  lands you back on it instead of on whatever's oldest. Main window only. */
const SPOT = "cue.spot";
let spotRestored = false;   // the first draws (before restoreSpot) mustn't overwrite where you were
function saveSpot() {
  if (!spotRestored) return;
  try { localStorage.setItem(SPOT, JSON.stringify({ sid: active?.sid || null, view })); } catch {}
}
function restoreSpot() {
  spotRestored = true;
  let spot = null;
  try { spot = JSON.parse(localStorage.getItem(SPOT) || "null"); } catch {}
  if (!spot) return;
  if (["board", "sessions", "history"].includes(spot.view)) view = spot.view;
  // Only if that session is still around; otherwise the usual oldest-first pick stands.
  const card = state.items.find((i) => i.session_id === spot.sid);
  if (spot.sid && (card || sessionOf(spot.sid) || parkedOf(spot.sid))) active = { id: card?.id || null, sid: spot.sid };
  renderMain();
}
function setActive(id, sid, exact = false) {
  view = "board";
  quietOpen = null;
  active = { id: id || null, sid: sid || findItem(id)?.session_id || null, exact };
  seenSession(active.sid);
  if (termSid && termSid !== active.sid) closeTerm(true);
  loadSteps(active.sid);   // now, not on the next tick
  sheet = menuFor = redirectFor = btwFor = null;
  renderMain();
  // Picked a session (any list, a chip, the live list's Enter): ready to type to it.
  focusComposer();
}
/** The next thing waiting on you that isn't on screen. */
const nextUp = () => needsYou().find((i) => i.id !== active?.id && i.session_id !== active?.sid);
function goNext() { const n = nextUp(); if (n) setActive(n.id, n.session_id); }
/** History: everything you answered, newest first, by day. Click a row to read it in place. */
// ---------- "N live" in the header: every live session, one click from any screen, and + New ----------
let liveOpen = false;              // the drop-down is open
let liveSpot = false;              // …opened with ⌘K: in the middle of the window, like Spotlight
let liveSel = 0;                   // the highlighted row (↑ ↓ move it, Enter opens it)
let liveShown = [];                // the rows on screen, in order: [{ sid, act }]
let newOpen = false;               // its New session form is showing
/** + New starts as you last left it: agent, permissions (per agent), where it runs, flags, folder
 *  (per machine) and machine. Only the name and the message start empty. */
const remembered = (k) => { try { return localStorage.getItem(`cue.${k}`); } catch { return null; } };
const remember = (k, v) => { try { localStorage.setItem(`cue.${k}`, v); } catch {} };
let newAgent = remembered("newAgent") || "claude";
let agentsAvail = null;            // the agents installed on this Mac (asked once)

// ---------- Sessions view: every live session as a map of project tiles ----------
const homeless = (p) => String(p || "").replace(/^\/Users\/[^/]+(?=\/|$)/, "~");
const baseName = (p) => String(p || "").split("/").filter(Boolean).pop() || "";
const lastSaid = (s) => [...(s.thread || [])].reverse().find((e) => e.role === "agent")?.text || "";
/** relay names tabs "[Exec] web-lines-cleanup", "[Lead] relay-tmux-backend": the LEAD / EXEC tag says that already. */
const bareName = (n) => String(n || "").replace(/^\[(?:ex-)?(?:Exec|Lead)\]\s*/i, "");

/** Cue's mark (the app icon without its tile): an open C around a dot. */
/** The mark at 14px, and a terminal (a prompt in a window), for the "Runs in" choice. */
const CUE_MARK_SM = `<svg width="14" height="14" viewBox="200 200 624 624" aria-hidden="true"><circle cx="512" cy="512" r="250" fill="none" stroke="currentColor" stroke-width="92" stroke-dasharray="1180 400" stroke-linecap="round" transform="rotate(40 512 512)"/><circle cx="512" cy="512" r="84" fill="currentColor"/></svg>`;
const TERM_ICON = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="2.5" y="4" width="19" height="16" rx="3"/><path d="M7 9.5l3 2.5-3 2.5M12.5 15H17"/></svg>`;
const CUE_MARK = `<svg width="28" height="28" viewBox="200 200 624 624" aria-hidden="true"><circle cx="512" cy="512" r="250" fill="none" stroke="currentColor" stroke-width="92" stroke-dasharray="1180 400" stroke-linecap="round" transform="rotate(40 512 512)"/><circle cx="512" cy="512" r="84" fill="#B5C27A"/></svg>`;
// Drafts survive a restart: each session's unsent text and images are saved in ~/.cue/cue.db.
const savedDrafts = {};     // key -> what the database last got, so only changes are written
let draftTimer = null;
const draftJson = (d) => JSON.stringify({ text: d?.text || "", images: (d?.images || []).map(({ name, mime, data }) => ({ name, mime, data })) });
function saveDrafts() {
  clearTimeout(draftTimer);
  draftTimer = setTimeout(() => {
    const keys = new Set([...Object.keys(drafts), ...Object.keys(savedDrafts)].filter((k) => k.startsWith("s:")));
    for (const key of keys) {
      const now = draftJson(drafts[key]);
      if (now === (savedDrafts[key] ?? draftJson(null))) continue;
      savedDrafts[key] = now;
      const { text, images } = JSON.parse(now);
      invoke("set_draft", { key, text, images }).catch(() => {});
    }
  }, 300);
}
async function loadDrafts() {
  const saved = await invoke("get_drafts").catch(() => ({}));
  for (const [k, d] of Object.entries(saved || {})) {
    drafts[k] = { text: d.text || "", choices: {}, images: d.images || [] };
    savedDrafts[k] = draftJson(drafts[k]);
  }
  // Drafts from before the database lived in this window's storage: move them over once.
  try {
    const old = JSON.parse(localStorage.getItem("cue.drafts") || "{}");
    for (const [k, d] of Object.entries(old)) if (!drafts[k]) drafts[k] = { text: d.text || "", choices: {}, images: d.images || [] };
    localStorage.removeItem("cue.drafts");
    if (Object.keys(old).length) saveDrafts();
  } catch { /* nothing to move */ }
}
