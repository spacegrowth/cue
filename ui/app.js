// Cue UI — everything that needs you, visible at once, on one board.
// Colors live in style.css (Moss, light + dark).
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
const ghosts = new Map();   // id -> { it, at }: just-answered rows that linger in Waiting
const openMsgs = new Set(); // older agent messages unfolded in the Active chat
const openFollow = new Set();  // cards whose "After a stop hook" is expanded
let dictating = null;       // { key, base, stopping } — the text box you're dictating into, and what it held before
let renaming = null;        // the session whose name is being edited in the Active pane
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
/** Hover on a session's name: what Claude Code titled the conversation (✎ in the chat names it). */
function nameTip(sid) {
  const title = state.about?.[sid]?.title || "";
  return title;
}
/** A session's name, with that hover. */
const nameSpan = (sid, project, cls = "proj") => { const tip = sid ? nameTip(sid) : ""; return `<span class="${cls}"${tip ? ` title="${esc(tip)}"` : ""}>${esc(nameOf(sid, project))}</span>`; };
/** What Cue calls a session: the name you gave it, else its project. */
// ([Lead] / [Exec], which relay puts in front of a name, isn't shown: the LEAD / EXEC tag says it.)
const nameOf = (sid, project) => String(sessionOf(sid)?.name || "").replace(/^\[(?:ex-)?(?:Exec|Lead)\]\s*/i, "") || project;
const findItem = (id) => state.items.find((i) => i.id === id) || state.history.find((i) => i.id === id);
const sessionOf = (sid) => state.sessions.find((s) => s.session_id === sid);

// ---------- leads and executors (claude-relay, pi-lead) ----------
/** A session's place in a relay / pi-lead crew: { role: "lead"|"executor", name, lead, lead_name, … }, or undefined. */
const crewOf = (sid) => state.crew?.[sid];
/** The lead's color: relay's own tab color, else a steady one picked from the lead's id. */
function crewColor(sid) {
  const m = crewOf(sid);
  if (!m) return "";
  if (m.color) return `rgb(${m.color.join(",")})`;
  const key = m.role === "lead" ? sid : m.lead;
  let h = 0;
  for (const c of String(key)) h = (h * 31 + c.charCodeAt(0)) % 360;
  return `hsl(${h} 32% 62%)`;
}
const shortModel = (m) => String(m || "").replace(/\[.*\]$/, "").replace(/^.*\//, "").replace(/^claude-/, "");
/** A dot in the lead's color (its iTerm tab color): beside a lead's name, and before the lead's name on its executors. */
const crewDot = (sid) => `<i class="crew-color" style="--crew:${crewColor(sid)}"></i>`;
const roleTag = (sid, live = false) => { const m = crewOf(sid); return m ? `${m.role === "lead" ? crewDot(sid) : ""}<span class="role ${m.role}" title="${m.role === "lead" ? "Lead: plans, reviews, commits" : "Executor: works one packet, stages, reports"}">${m.role === "lead" ? "LEAD" : "EXEC"}</span>${m.role === "lead" ? autoToggle(sid, m, live) : ""}` : ""; };
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
  return `<div class="crew-line">of ${crewDot(sid)}${leadLink(m)}${bits.length ? " · " + bits.join(" · ") : ""}${vf}${orphan}</div>`;
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
/** "in review" once you've sent an executor's review (Cue tells every screen), else "". */
const reviewWord = (sid) => state.reviews?.[sid] || "";
/** relay's auto mode beside a lead's LEAD tag: a switch in its chat (`live`: click to flip it),
 *  and on cards and rows just "auto" while it's on. In the chat, hover says what it means. */
const autoToggle = (sid, m, live = false) => {
  if (m?.plugin !== "relay") return "";
  const why = m.auto ? "Goes ahead on routine steps without asking. Commits still wait for you." : "Waits for you at every step.";
  return live ? `<button class="auto-tg ${m.auto ? "on" : ""}" data-crew="${m.auto ? "auto-off" : "auto-on"}" data-crew-sid="${esc(sid)}" title="${why}"><i></i>auto</button>`
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
/** The team a session is in: its lead (session id, name) and executors, or null. */
function teamOf(sid) {
  const m = crewOf(sid);
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
  return { word: { stalled: "stalled", closed: "closed", superseded: "replaced", dead: "gone" }[e.status] || e.status || "", cls: "" };
}
/** Under the chat's header, lead's or executor's: the whole team, this one lit; a chip opens that chat. */
function teamBar(sid) {
  const t = teamOf(sid);
  if (!t) return "";
  const chip = (id, name, word, cls) => {
    const me = id === sid, go = !me && sessionOf(id);
    return `<${go ? "button" : "span"} class="tm-chip ${cls} ${me ? "me" : ""}" ${go ? `data-session="${esc(id)}"` : ""}>${cls === "busy" ? `<span class="dot-live"></span>` : ""}${esc(name)}${word ? ` <span class="tm-sub">${esc(word)}</span>` : ""}</${go ? "button" : "span"}>`;
  };
  const lead = sessionOf(t.lead) || t.lead === sid ? chip(t.lead, t.leadName, "lead", "lead") : `<span class="tm-chip gone">${esc(t.leadName)} <span class="tm-sub">lead · not running</span></span>`;
  return `<div class="team-bar"><span class="tm-label">${crewDot(sid)}${esc(t.leadName)}${/s$/i.test(t.leadName) ? "'" : "'s"} team</span>${lead}${t.ex.map((e) => { const st = exState(e); return chip(e.session_id, e.name, st.word, st.cls); }).join("")}</div>`;
}
/** relay verify's verdict, in words: it checks every file the report lists is staged, not that the work is right. */
const verifyWords = (v) => !v?.verdict ? "" : v.verdict === "COUNTS-MATCH" ? "its report matches what's staged"
  : `<span class="crew-warn">its report doesn't match what's staged${v.verdict === "MALFORMED" ? " (its summary is missing or broken)" : v.note ? ` (${esc(v.note)})` : ""}</span>`;
/** An executor's chat: its packet (what it's for), where it stands, and Diff / Review once it's done. */
function packetCard(sid) {
  const m = crewOf(sid);
  if (m?.role !== "executor") return "";
  const lead = esc(m.lead_name || "its lead");
  let st, acts = "";
  if (reported(m.status)) {
    const asked = reviewWord(sid);
    st = [m.outcome ? `Done: ${esc(m.outcome)}` : "Done", asked ? `${asked} with ${lead}` : "waiting for review", verifyWords(m.verify)].filter(Boolean).join(" · ");
    acts = crewBtn(sid, "diff", "Diff") + (m.lead_armed && !asked ? crewBtn(sid, "review", "Review in lead", true) : "");
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
    if (reported(e.status) && !reviewWord(e.session_id)) return [`<div class="tc-row"><span><b>${esc(e.name)}</b> is done${e.outcome ? `: ${esc(e.outcome)}` : ""}</span><span class="tc-acts">${crewBtn(e.session_id, "diff", "Diff")}${crewBtn(e.session_id, "review", "Review", true)}</span></div>`];
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
    else if (reported(e.status) && reviewWord(e.session_id)) rows.push(`<div class="tn ${on}" ${sessionOf(e.session_id) ? `data-session="${esc(e.session_id)}"` : ""}><div class="tn-t"><b>${esc(e.name)}</b><span>✓ done · ${reviewWord(e.session_id)}</span></div></div>`);
    else if (reported(e.status)) rows.push(`<div class="tn ${on}" ${sessionOf(e.session_id) ? `data-session="${esc(e.session_id)}"` : ""}><div class="tn-t"><b>${esc(e.name)}</b><span>✓ done, waiting for review</span></div><div class="tn-acts">${crewBtn(e.session_id, "diff", "Diff")}${crewBtn(e.session_id, "review", "Review in lead", true)}</div></div>`);
    else busy.push(e);
  }
  const line = (e) => `<div class="tn ${e.session_id === openSid ? "on" : ""}" ${sessionOf(e.session_id) ? `data-session="${esc(e.session_id)}"` : ""}><div class="tn-t">${e.status === "busy" ? `<span class="dot-live"></span>` : ""}<b>${esc(e.name)}</b>${(() => { const st = liveStep(e.session_id); return st ? `<span class="tn-x">${esc(st.what)}</span><span class="tn-age" data-ago="${st.at}">${ago(st.at)}</span>` : `<span class="tn-x">${esc(exState(e).word)}</span>`; })()}</div></div>`;
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
  const a = (u, label) => `<a href="#" data-href="${u}" title="${u}">${label}</a>`;
  return h.replace(/\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)/g, (_, label, u) => `\u0000${links.push(a(u, label)) - 1}\u0000`)
    .replace(/https?:\/\/[^\s<>"'\u0000]*[^\s<>"'.,;:!?)\]\u0000]/g, (u) => a(u, u))
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
addEventListener("wheel", () => { glide = null; }, { capture: true, passive: true });
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
// A selection made by this press (a click on a button leaves an older one in place: not that).
const dragSelecting = () => mouseDown && !getSelection().isCollapsed && getSelection().toString() !== selBefore;
addEventListener("mousedown", (e) => { mouseDown = e.button === 0 && !inField(e.target); selBefore = getSelection().toString(); }, true);
addEventListener("mouseup", () => {
  if (!mouseDown) return;
  const sel = getSelection(), text = sel.toString(), fresh = dragSelecting();
  mouseDown = false;
  // Selected text in the window (not in a box you type in, where ⌘C works as ever): on your clipboard now.
  if (fresh && text.trim() && !inField(sel.anchorNode)) {
    if (!document.execCommand("copy")) navigator.clipboard?.writeText(text);
    copied = { at: Date.now(), words: text.length > 40 ? text.trim().split(/\s+/).length : 0 };
    if (!heldDraw && !showCopied()) toast("Copied");
  }
  if (heldDraw) { heldDraw = false; renderMain(); }
}, true);
// Let go outside the window (no mouseup here): the held redraw happens anyway.
addEventListener("blur", () => { mouseDown = false; if (heldDraw) { heldDraw = false; renderMain(); } });

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

function toast(msg) {
  let el = document.querySelector(".toast");
  if (!el) { el = document.createElement("div"); el.className = "toast"; document.body.append(el); }
  el.textContent = msg;
  el.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove("show"), 2600);
}

// ---------- actions ----------
async function respond(it, decision) {
  const ok = await invoke("respond", { id: it.id, decision });
  if (!ok) toast("Already answered somewhere else");
  delete drafts[it.id];
  menuFor = redirectFor = null;
}
const allow = (it) => respond(it, { behavior: "allow" });
const deny = (it, message) => respond(it, { behavior: "deny", ...(message ? { message } : {}) });
function answersFor(it) {
  const d = draft(it.id);
  const out = {};
  for (const q of questions(it)) {
    const c = d.choices[q.question];
    const picked = q.multiSelect ? [...(c || [])] : c ? [c] : [];
    if (!picked.length) return null;
    out[q.question] = picked.join(", ");
  }
  return out;
}
const qStep = {};           // item id -> which question of a multi-question ask is showing
const isAnswered = (it, q) => { const c = draft(it.id).choices[q.question]; return q.multiSelect ? !!c?.size : !!c; };
/** The question on screen: the one you picked, else the first one not answered yet. */
function stepOf(it) {
  const qs = questions(it);
  const first = qs.findIndex((q) => !isAnswered(it, q));
  return Math.min(qStep[it.id] ?? (first < 0 ? qs.length - 1 : first), qs.length - 1);
}
function pick(it, qi, oi) {
  const q = questions(it)[qi];
  const label = q?.options?.[oi]?.label;
  if (!label) return;
  const d = draft(it.id);
  if (q.multiSelect) { const s = (d.choices[q.question] ||= new Set()); s.has(label) ? s.delete(label) : s.add(label); }
  else d.choices[q.question] = label;
  // One single-choice question: the click is the answer.
  if (questions(it).length === 1 && !q.multiSelect) return respond(it, { behavior: "allow", answers: { [q.question]: label } });
  const a = answersFor(it);
  if (a && questions(it).every((x) => !x.multiSelect)) return respond(it, { behavior: "allow", answers: a });
  // Several questions: a single choice moves on to the next one still unanswered.
  if (!q.multiSelect) {
    const next = questions(it).findIndex((x, n) => n !== qi && !isAnswered(it, x));
    if (next >= 0) qStep[it.id] = next;
  }
  render();
}
/**
 * Send from a text box: your message shows in the chat right away ("Sending…"), the box clears,
 * and it settles to "Sent · via …" once delivered. On failure the text goes back in the box.
 */
const outbox = [];          // { sid, text, images, at, via } until the session's thread shows it
async function deliver(sid, key, call) {
  endDictation(key);
  if (isParked(sid) && (draft(key).text.trim() || draft(key).images.length)) setParked(sid, false);   // you replied: decided
  const d = draft(key);
  const text = d.text.trim();
  // "/btw …": a side question, in its panel (the session never sees it).
  const bt = /^\/btw(?:\s+([\s\S]*))?$/.exec(text);
  if (bt && sessionOf(sid)?.harness === "claude") { delete drafts[key]; return openBtw(sid, bt[1] || ""); }
  if (!text && !d.images.length) return;
  // " /…" (a leading space) is a message that starts with a slash; "/…" is a command. Keep the space.
  const typed = text.startsWith("/") && /^\s/.test(d.text) ? ` ${text}` : text;
  // A command this session doesn't have ("/reomte"): it would just fail in the terminal. Say so instead.
  if (typed.startsWith("/") && cmdOf(key) === null) return toast(`“${typed.split(/\s/)[0]}” isn't a command in this session. Use “Send as a message instead” under the box.`);
  const images = d.images.map(({ name, mime, data }) => ({ name, mime, data }));
  const entry = { sid, text, images, at: now(), via: null };
  outbox.push(entry);
  delete drafts[key];
  renderMain();
  try {
    const r = await call(typed, images);
    entry.via = String(r || "sent").replace(/^sent via /, "");
    sent(sid, text, images.length, r);
    renderMain();
  } catch (e) {
    outbox.splice(outbox.indexOf(entry), 1);
    drafts[key] = { ...draft(key), text, images: d.images };
    toast(`Couldn't send: ${e}`);
    renderMain();
  }
}
/** Has the session's own thread (or queue) caught up with this outbox entry? */
function landed(o) {
  const s = sessionOf(o.sid);
  if (!s) return !!o.via;
  const mine = (e) => e && e.role === "you" && e.at_ms >= o.at - 5000 && e.text.startsWith(o.text);
  return (s.thread || []).some(mine) || mine({ role: "you", ...s.queued });
}
// ---------- by the way: side questions about the open session, in a panel under its btw button ----------
// A copy of the conversation answers (see btw.rs): the session keeps working and its chat never shows them.
const btwLog = new Map();   // sid -> [{ q, a, err }]: this window's, newest last
let btwFor = null;          // the session whose panel is open
function openBtw(sid, q = "") {
  btwFor = sid;
  renderMain();
  document.querySelector(`[data-text="${CSS.escape(`btw:${sid}`)}"]`)?.focus();
  if (q.trim()) askBtw(sid, q);
}
async function askBtw(sid, q) {
  q = q.trim();
  if (!q) return;
  const log = btwLog.get(sid) || btwLog.set(sid, []).get(sid);
  const e = { q, a: null, err: null };
  log.push(e);
  delete drafts[`btw:${sid}`];
  renderMain();
  try { e.a = await invoke("btw", { sessionId: sid, question: q }); } catch (x) { e.err = String(x); }
  renderMain();
}
function btwPanel(sid) {
  if (!sid || btwFor !== sid) return "";
  const log = btwLog.get(sid) || [];
  const rows = log.slice().reverse().map((e) => `<div class="btw-q">${esc(e.q)}</div>${e.a != null ? `<div class="btw-a msg">${md(e.a)}</div>` : e.err ? `<div class="btw-a bad">${esc(e.err)}</div>` : `<div class="btw-a dim"><span class="dot-live"></span>Thinking…</div>`}`).join("");
  return `<div class="btw-pop">
    <div class="btw-head">BY THE WAY · NOT PART OF THE CHAT${log.length ? `<button class="btw-clear" data-act="btw-clear" data-sid="${esc(sid)}">Clear</button>` : ""}<button class="btw-x" data-act="btw-close" aria-label="Close">×</button></div>
    <div class="btw-ask"><textarea data-text="btw:${esc(sid)}" rows="1" placeholder="Ask without interrupting it…  ↵" spellcheck="false">${esc(draft(`btw:${sid}`).text)}</textarea></div>
    <div class="btw-body">${rows || `<div class="dim btw-empty">A copy of this conversation answers, so the session never sees it. Each question costs about one turn.</div>`}</div></div>`;
}
const reply = (it) => deliver(it.session_id, `s:${it.session_id}`, (text, images) => invoke("reply", { id: it.id, text, images }));
/** The text field: a reply on a finished card, "do this instead" on a permission, your own answer on a question. */
/** Dictation: the mic in a text box. Your words land in the box as you talk (after anything already
 *  typed); click the mic again, or send, to stop. Apple's on-device recognition, via cue-listen. */
function toggleDictation(key) {
  if (dictating?.key === key) { dictating.stopping = true; invoke("dictate_stop"); return renderMain(); }
  if (dictating) invoke("dictate_stop");
  dictating = { key, base: draft(key).text.trimEnd(), stopping: false };
  invoke("dictate_start").catch((e) => { dictating = null; toast(`Can't dictate: ${e}`); renderMain(); });
  renderMain();
  document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`)?.focus();
}
/** Sending ends dictation: what's in the box is what goes. */
function endDictation(key) {
  if (dictating?.key !== key) return;
  invoke("dictate_stop");
  dictating = null;
}
function onDictation(p) {
  if (p.error) { toast(p.error); dictating = null; return renderMain(); }
  if (p.done) { dictating = null; return renderMain(); }
  if (p.text == null || !dictating) return;
  const { key, base } = dictating;
  const text = base ? `${base} ${p.text}` : p.text;
  draft(key).text = text;
  saveDrafts();
  // Update the box in place (no full redraw per word), caret at the end.
  const el = document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`);
  if (el) {
    el.value = text;
    el.selectionStart = el.selectionEnd = text.length;
    grow(el);
    const b = el.closest(".composer")?.querySelector(".btn.send");
    if (b) b.disabled = !text.trim();
  }
}
function submitText(it) {
  endDictation(it.id);
  const text = draft(it.id).text.trim();
  if (it.kind === "waiting") return reply(it);
  if (!text) return;
  if (it.kind === "question") {
    const qs = questions(it);
    if (qs.length <= 1) return respond(it, { behavior: "allow", answers: Object.fromEntries(qs.map((q) => [q.question, text])) });
    // Several questions: your typed answer is for the one on screen only. Then on to the next
    // unanswered one; it's sent once every question has an answer.
    const cur = stepOf(it), q = qs[cur], d = draft(it.id);
    d.choices[q.question] = q.multiSelect ? new Set([text]) : text;
    d.text = "";
    const a = answersFor(it);
    if (a) return respond(it, { behavior: "allow", answers: a });
    const next = qs.findIndex((x, n) => n !== cur && !isAnswered(it, x));
    if (next >= 0) qStep[it.id] = next;
    return renderMain();
  }
  return deny(it, text);
}
/** Say something to a live session from the Active pane (it may be mid-turn). */
// now (Ctrl+Enter): if it's mid-turn, stop it first so this is handled right away.
/** Send now (the button on a queued message, or ⌘↵ with nothing new typed): stopping the turn makes the
 *  agent read what you queued straight away. */
function sendQueuedNow(sid) {
  invoke("send_queued_now", { sessionId: sid })
    .then(() => toast(`Sent now: ${nameOf(sid, sessionOf(sid)?.project || "it")} stopped its current turn and reads your message next`))
    .catch((e) => toast(`Couldn't send it now: ${e}`));
}
const sendTo = (sid, now = false) => sessionOf(sid)?.state === "deciding" ? toast("It's asking you something first. Answer that, then send.") : deliver(sid, `s:${sid}`, (text, images) => invoke("send_to_session", { sessionId: sid, text, images, now }));
/** Stop a working session mid-turn (Esc in its terminal; Pi aborts directly). */
async function interrupt(sid) {
  try { toast(`Stopped ${sessionOf(sid)?.project || "it"}: ${(await invoke("interrupt_session", { sessionId: sid })).replace(/^stopped via /, "via ")}`); }
  catch (e) { toast(`Couldn't stop it: ${e}`); }
}
const box_ = (key, placeholder, label, attrs) => box(key, placeholder, label, attrs, true);

/** Remember how the last message went out ("typed via tmux", "queued"), shown in the Active pane. */
function sent(sid, text, images, via) {
  lastSent = { sid, text, images, via: String(via || "").replace(/^sent via /, ""), at: Date.now() };
  setTimeout(() => { if (lastSent && Date.now() - lastSent.at >= 8000) { lastSent = null; render(); } }, 8100);
}

async function goTo(it) {
  try { toast(`Jumped to ${await invoke("focus_session", { id: it.id })}`); }
  catch (e) { toast(`Couldn't jump: ${e}`); }
}
async function setSetting(key, value) {
  try { await invoke("set_setting", { key, value }); } catch (e) { toast(`Couldn't save: ${e}`); }
}

// ---------- shared pieces ----------
/** Idle sessions you hid with ×, as "session:since" — back on their own once the session does anything. */
let hiddenIdle = new Set();
try { hiddenIdle = new Set(JSON.parse(localStorage.getItem("cue.hiddenIdle") || "[]")); } catch {}
const idleKey = (s) => `${s.session_id}:${s.since_ms}`;
function hideIdle(key) {
  hiddenIdle.add(key);
  const live = new Set(state.sessions.map(idleKey));
  hiddenIdle = new Set([...hiddenIdle].filter((k) => live.has(k)));   // forget ones that moved on
  try { localStorage.setItem("cue.hiddenIdle", JSON.stringify([...hiddenIdle])); } catch {}
  renderMain();
}
const hideX = (s) => idleNote(s) ? `<button class="x-clear" data-hide-idle="${esc(idleKey(s))}" title="Hide until it does something again" aria-label="Hide">×</button>` : "";
/** Sessions you starred: the ones you're following. Marked where they are (a star doesn't move them),
 *  and counted by the ★ chip in the header. Kept with the session in Cue (so anything showing
 *  Cue sees the same stars); a star you just clicked shows until Cue's next update says so too. */
const starNow = new Map();   // sid -> on, clicked here, not yet in Cue's state
const isStarred = (sid) => starNow.get(sid) ?? !!sessionOf(sid)?.starred_ms;
const starredIds = () => new Set(state.sessions.map((s) => s.session_id).filter(isStarred));
function toggleStar(sid) {
  const on = !isStarred(sid);
  starNow.set(sid, on);
  invoke("set_starred", { sessionId: sid, on }).catch((e) => { starNow.delete(sid); toast(`Couldn't: ${e}`); renderMain(); });
  renderMain();
}
/** This window used to keep its own stars (and before that, pins): hand them to Cue once, then forget them. */
function moveOldStars() {
  let old = [];
  try { old = [...JSON.parse(localStorage.getItem("cue.starred") || "[]"), ...JSON.parse(localStorage.getItem("cue.pinned") || "[]")]; localStorage.removeItem("cue.starred"); localStorage.removeItem("cue.pinned"); } catch {}
  for (const sid of new Set(old)) invoke("set_starred", { sessionId: sid, on: true }).catch(() => {});
}
const STAR_ICON = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" aria-hidden="true"><path d="M12 3.2l2.7 5.6 6.1.8-4.5 4.2 1.1 6.1L12 17l-5.4 2.9 1.1-6.1-4.5-4.2 6.1-.8z"/></svg>`;
const starBtn = (sid) => {
  const on = isStarred(sid);
  return `<button class="star-btn ${on ? "on" : ""}" data-star="${esc(sid)}" aria-label="${on ? "Unstar" : "Star"}" aria-pressed="${on}">${STAR_ICON}</button>`;
};
/** Under Sessions, two drawers: Starred, then Recently answered. At most one is open (opening one folds
 *  the other); with both folded, Sessions takes the column. Starred is open to begin with, Recently
 *  answered folded. Remembered per window. */
const DRAWERS = ["starred", "recent"];
function openDrawer() {
  try {
    const d = localStorage.getItem("cue.drawer");
    if (d !== null) return d;
    return localStorage.getItem("cue.recentOpen") === "1" ? "recent" : "starred";   // what this window had before
  } catch { return "starred"; }
}
const drawerOpen = (name) => openDrawer() === name;
/** Open or fold a drawer like a drawer: folding, its height shrinks up into its heading while Sessions
 *  grows into the room; opening, it grows back down (the other one, if open, folds at once). Snaps with
 *  Reduce motion. */
function foldDrawer(name) {
  const closing = drawerOpen(name);
  const save = () => { try { localStorage.setItem("cue.drawer", closing ? "" : name); localStorage.removeItem("cue.recentOpen"); localStorage.removeItem("cue.recentClosed"); } catch {} };
  const slide = (sec, col, from, to, done) => {
    let ended = false;
    const end = () => { if (!ended) { ended = true; done(); } };
    col.classList.add("drawers-shut");             // Sessions may use whatever the drawer gives up
    sec.style.flex = "none";
    sec.style.overflow = "hidden";
    sec.animate([{ height: `${from}px` }, { height: `${to}px` }], { duration: 220, easing: "ease-in-out" }).onfinish = end;
    setTimeout(end, 300);                          // a redraw mid-way drops the animation, and its onfinish
  };
  const closedHeight = (sec) => sec.querySelector(".fold-head").offsetHeight + parseFloat(getComputedStyle(sec).paddingTop);
  let sec = document.querySelector(`.sec-${name}`);
  if (!sec || matchMedia("(prefers-reduced-motion: reduce)").matches) { save(); return renderMain(); }
  if (closing) return slide(sec, sec.closest(".col.split"), sec.offsetHeight, closedHeight(sec), () => { save(); renderMain(); });
  const from = closedHeight(sec);
  save();
  renderMain();                                    // open, so we can measure where it ends up
  sec = document.querySelector(`.sec-${name}`);
  const col = sec.closest(".col.split"), to = sec.offsetHeight;
  slide(sec, col, from, to, () => { sec.style.flex = sec.style.overflow = ""; col.classList.remove("drawers-shut"); });
}
/** Put off for later ("Later"): kept with the session in Cue, not in this window. */
const parkedIds = () => new Set(state.sessions.filter((s) => s.later_ms).map((s) => s.session_id));
const isParked = (sid) => parkedIds().has(sid);
/** This window used to keep its own list: hand it to Cue once, then forget it. */
function moveOldParked() {
  let old = [];
  try { old = JSON.parse(localStorage.getItem("cue.parked") || "[]"); localStorage.removeItem("cue.parked"); } catch {}
  for (const sid of old) invoke("set_later", { sessionId: sid, on: true }).catch(() => {});
}
function setParked(sid, on) {
  if (isParked(sid) === on) return;
  invoke("set_later", { sessionId: sid, on }).catch((e) => toast(`Couldn't: ${e}`));
  const s = sessionOf(sid);
  if (s) s.later_ms = on ? Date.now() : 0;   // shown now; Cue's next update says the same
  const name = nameOf(sid, sessionOf(sid)?.project || state.items.find((i) => i.session_id === sid)?.project || "it");
  if (on && active?.sid === sid) active = null;   // the Active pane moves on to what's still waiting
  renderMain();
  if (on) toast(`Moved ${name} to Need to decide`);
}
function groups() {
  const parked = parkedIds();
  const oldest = (a, b) => a.created_ms - b.created_ms;
  const yours = state.items.filter((i) => i.kind === "waiting" && !parked.has(i.session_id)).sort(oldest);
  const decide = state.items.filter((i) => i.kind !== "waiting").sort(oldest);
  const asking = new Set(state.items.map((i) => i.session_id));
  // Sessions = every live session without a card in Waiting: busy ones, and idle ones (driven by
  // another agent, or cleared from Waiting), so nothing you cleared drops out of sight.
  const working = state.sessions.filter((s) => !asking.has(s.session_id) && !parked.has(s.session_id) && !(idleNote(s) && hiddenIdle.has(idleKey(s))))
    .sort((a, b) => !!idleNote(a) - !!idleNote(b));   // working first, then idle (minus the ones you hid)
  // Need to decide: parked sessions, with their finished turn if one is pending. Something it asks you
  // still goes in Waiting (the agent is blocked on it), so it shows in both.
  const later = [...parked].map((sid) => ({ sid, s: sessionOf(sid), it: state.items.find((i) => i.session_id === sid && i.kind === "waiting") }))
    .filter((x) => x.s || x.it).sort((a, b) => (a.it?.created_ms ?? a.s.since_ms) - (b.it?.created_ms ?? b.s.since_ms));
  return { yours, decide, working, later };
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
  for (const m of [stepFeeds, stepArrived, cmdLists]) for (const k of [...m.keys()]) if (!near.has(sidOf(k))) m.delete(k);
  for (const k of [...btwLog.keys()]) if (!near.has(k)) btwLog.delete(k);
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
setInterval(trimCaches, 5 * 60000);

/** Thumbnails of sent images; click to open full size (← → between them). */
function thumbs(paths) {
  if (!paths?.length) return "";
  const srcs = paths.map(fileSrc);
  return `<div class="thumbs">${srcs.map((src, n) => `<button class="thumb" data-lightbox='${esc(JSON.stringify({ srcs, i: n }))}' aria-label="Open image ${n + 1}"><img src="${esc(src)}" alt=""/></button>`).join("")}</div>`;
}

/** Images a message names by absolute path (`in backticks`, which may hold spaces, or bare),
 *  shown under it. The window can't load them itself; image_data hands them over. */
const localImgs = new Map();   // path -> data: URL, null while loading, false if unreadable
function imagePaths(text) {
  const IMG = String.raw`\.(?:png|jpe?g|gif|webp)`;
  const found = [...String(text ?? "").matchAll(new RegExp(String.raw`\`(\/[^\`\n]+?${IMG})\`|(?:^|[\s(])(\/[^\s\`'"<>()]+${IMG})(?![\w/])`, "gi"))].map((m) => m[1] || m[2]);
  return [...new Set(found)];
}
function localImages(text) {
  const paths = imagePaths(text).filter((p) => localImgs.get(p) !== false);
  for (const p of paths) {
    if (localImgs.has(p)) continue;
    localImgs.set(p, null);
    invoke("image_data", { path: p }).then((d) => localImgs.set(p, d), () => localImgs.set(p, false)).then(render);
  }
  if (!paths.length) return "";
  return `<div class="thumbs shots">${paths.map((p, n) => `<button class="thumb" data-local-lb='${esc(JSON.stringify({ paths, i: n }))}' title="${esc(p)}">${localImgs.get(p) ? `<img src="${localImgs.get(p)}" alt=""/>` : ""}</button>`).join("")}</div>`;
}

/** The "/" menu: the commands Claude Code has enabled for a session (its built-ins, your skills,
 *  plugins'), read by the Mac per folder. sid -> list; null while it's being read. */
const cmdLists = new Map();
const BTW_CMD = { name: "btw", kind: "cue", desc: "Ask on the side: answered in a panel here, never added to the chat" };
let cmdSel = 0, cmdShut = null;           // highlighted row; the box whose menu Esc closed (until you type)
function cmdList(sid) {
  if (!sid) return [];
  if (!cmdLists.has(sid)) {
    cmdLists.set(sid, null);
    const ask = (n) => invoke("session_commands", { sessionId: sid }).then((l) => {
      if (l) { cmdLists.set(sid, l); return refreshCmd(`s:${sid}`); }
      if (n < 40) setTimeout(() => ask(n + 1), 600); else cmdLists.delete(sid);
    }).catch(() => cmdLists.delete(sid));
    ask(0);
  }
  const list = cmdLists.get(sid) || [];
  // Cue's own: a side question, answered in a panel (Claude Code's /btw answers only in its terminal).
  return sessionOf(sid)?.harness === "claude" ? [BTW_CMD, ...list.filter((c) => c.name !== "btw")] : list;
}
const sidOfKey = (key) => (key.startsWith("s:") ? key.slice(2) : "");
/** "/comp": the command name being typed (no space yet), else null. A leading space means a message. */
const typingCmd = (text) => (/^\/[^\s]*$/.test(text) ? text.slice(1).toLowerCase() : null);
/** "/model so": the command and the value being typed for it (one word, no space after yet), else null. */
const typingArg = (text) => { const m = /^\/(\S+) (\S*)$/.exec(text); return m ? { name: m[1], q: m[2].toLowerCase() } : null; };
/** Matches for what's typed: names that start with it first, then names that contain it. After a command
 *  that takes a set of values (/model, /effort…), those values, as "model sonnet" (what follows the "/"). */
function cmdMatches(key) {
  if (cmdShut === key) return [];
  const list = cmdList(sidOfKey(key));
  const a = typingArg(draft(key).text);
  if (a) {
    const c = list.find((x) => x.name === a.name);
    const hits = (c?.args || []).filter((v) => [v.value, v.label].some((t) => t.toLowerCase().includes(a.q)));
    // Typed out in full: nothing left to pick.
    if (hits.length === 1 && hits[0].value.toLowerCase() === a.q) return [];
    return hits.map((v) => ({ name: `${c.name} ${v.value}`, label: v.label !== v.value ? v.label : "", desc: v.desc, arg: true }));
  }
  const q = typingCmd(draft(key).text);
  if (q === null) return [];
  const starts = list.filter((c) => c.name.toLowerCase().startsWith(q) || c.name.toLowerCase().split(":").pop().startsWith(q));
  return [...starts, ...list.filter((c) => !starts.includes(c) && c.name.toLowerCase().includes(q))];
}
function cmdMenu(key) {
  const m = cmdMatches(key);
  // The session's list is still being read (the first "/" in a session): say so rather than show nothing.
  if (!m.length && typingCmd(draft(key).text) !== null && cmdShut !== key && cmdLists.get(sidOfKey(key)) === null) return `<div class="cmd-menu"><div class="cmd-row cmd-wait">Loading commands…</div></div>`;
  if (!m.length) return "";
  cmdSel = Math.min(cmdSel, m.length - 1);
  // A command that takes a value: its hint ("[name]"); one with values to pick says so (→ shows them).
  const after = (c) => c.arg ? (c.label ? ` <em>${esc(c.label)}</em>` : "") : c.args?.length ? ` <em>…</em>` : c.hint ? ` <em>${esc(c.hint)}</em>` : "";
  return `<div class="cmd-menu" role="listbox">${m.map((c, n) => `<button class="cmd-row ${n === cmdSel ? "on" : ""}" data-cmd-pick="${esc(key)}" data-cmd="${esc(c.name)}" role="option"><b>/${esc(c.name)}${after(c)}</b>${c.desc ? `<span>${esc(c.desc)}</span>` : ""}${c.kind === "skill" ? `<i>skill</i>` : ""}</button>`).join("")}</div>`;
}
/** The command a box's text runs ("/compact now" -> compact), from the session's list; undefined when
 *  it isn't a command, null when it's one the session doesn't have (or the list isn't in yet: then unknown). */
function cmdOf(key) {
  const t = draft(key).text;
  if (!t.startsWith("/")) return undefined;
  const name = t.slice(1).split(/\s/)[0];
  const sid = sidOfKey(key), list = sid ? cmdList(sid) : [];
  if (!list.length) return { name, desc: "" };
  return list.find((c) => c.name === name) || null;
}
/** Under the box: what the command does, or that it isn't one here (with Send as a message). */
function slashHint(key) {
  const t = draft(key).text;
  if (!t.startsWith("/") || cmdMatches(key).length || cmdLists.get(sidOfKey(key)) === null) return `<div class="slash-hint" hidden></div>`;
  const c = cmdOf(key), name = t.slice(1).split(/\s/)[0];
  const asText = `<button data-astext="${esc(key)}">Send as a message instead</button>`;
  if (c === null) return `<div class="slash-hint bad">“/${esc(name)}” isn't a command in this session. ${asText}</div>`;
  return `<div class="slash-hint">${c.desc ? `<b>/${esc(c.name)}${c.hint ? ` ${esc(c.hint)}` : ""}</b>: ${esc(c.desc)}` : `Starts with “/”, so the agent runs it as a command. ${asText}`}</div>`;
}
/** Redraw just a box's menu and hint (typing never rebuilds the box, so the caret stays put). */
function refreshCmd(key) {
  const el = document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`);
  const f = el?.closest(".field");
  if (!f) return;
  const menu = f.querySelector(".cmd-slot");
  if (menu) menu.innerHTML = cmdMenu(key);
  f.querySelector(".slash-hint")?.replaceWith(document.createRange().createContextualFragment(slashHint(key)));
  f.querySelector(".cmd-row.on")?.scrollIntoView({ block: "nearest" });
}
/** Pick a command: Tab or a click puts "/name " in the box for its arguments; Enter runs it. One with
 *  values to pick (/model) never runs bare (that opens a picker in its terminal): it shows its values.
 *  A value ("model sonnet"): Tab or a click puts it in the box, Enter runs it. */
function pickCmd(key, name, run) {
  const c = cmdList(sidOfKey(key)).find((x) => x.name === name);
  if (c?.args?.length) run = false;
  draft(key).text = `/${name}${run || name.includes(" ") ? "" : " "}`;
  cmdSel = 0;
  saveDrafts();
  const el = document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`);
  if (el) { el.value = draft(key).text; el.focus(); el.selectionStart = el.selectionEnd = el.value.length; }
  refreshCmd(key);
  if (run) el?.closest(".composer")?.querySelector(".btn.send")?.click();
}
/** Quick phrases (Settings → Quick phrases): chips that send their text, after anything typed in the box. */
const phrases = () => state.settings?.quick?.phrases || [];
const quickRow = (attrs) => phrases().length ? `<div class="quick">${phrases().map((p) => `<button class="qp" ${attrs} data-phrase="${esc(p)}">${esc(p)}</button>`).join("")}</div>` : "";
/** Settings → Quick phrases as you type them (redraws keep them); saved when you leave a field. */
let qpEdit = null;
const withPhrase = (text, p) => (text.trim() ? `${text.trimEnd()} ${p}` : p);
/** A text box that grows as you type (Shift+Enter = new line, Enter = send), optionally with images.
 *  The boxes that message a session (the ones with images) also get the quick phrases. */
function box(key, placeholder, label, sendAttrs, images) {
  const d = draft(key);
  const ready = d.text.trim() || (images && d.images.length);
  const chips = images && d.images.length ? `<div class="chips">${d.images.map((im, n) => `<span class="chip"><button class="chip-img" data-draft-lightbox="${esc(key)}:${n}" aria-label="Open ${esc(im.name)}"><img src="${im.data}" alt="${esc(im.name)}"/></button><button class="chip-x" data-unattach="${esc(key)}:${n}" aria-label="Remove image">×</button></span>`).join("")}${d.images.length > 1 ? `<span class="chip-count">${d.images.length} images</span>` : ""}</div>` : "";
  // The send button sits outside the box, as tall as it, so typing never crowds it.
  return `<div class="composer"><div class="field ${images ? "drop" : ""}" data-drop="${images ? esc(key) : ""}">${sidOfKey(key) ? `<div class="cmd-slot">${cmdMenu(key)}</div>` : ""}${images ? quickRow(`data-quick="${esc(key)}"`) : ""}${chips}<div class="field-row">
    <textarea rows="2" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off" data-text="${esc(key)}" placeholder="${esc(placeholder)}">${esc(d.text)}</textarea>
    <button class="icon-btn mic ${dictating?.key === key ? "on" : ""}" data-mic="${esc(key)}" title="${dictating?.key === key ? "Stop dictating" : "Dictate: talk and it types here"}" aria-label="Dictate">${MIC_ICON}</button>
    ${images ? `<button class="icon-btn" data-attach="${esc(key)}" title="Add an image (or paste one with ⌘V)" aria-label="Add image">${IMAGE_ICON}</button>` : ""}</div>${slashHint(key)}</div>
    <button class="btn primary send" ${sendAttrs} ${ready ? "" : "disabled"}>${label}</button></div>`;
}
// A finished turn's box is the session's box: one draft per session, whichever card shows it.
/** What the Active pane shows, in the drop-down lists (where lime is the keyboard highlight). */
const VIEWING = `<span class="viewing-tag">viewing</span>`;
const field = (it, placeholder, label) => box(it.kind === "waiting" ? `s:${it.session_id}` : it.id, placeholder, label, `data-act="send" data-id="${esc(it.id)}"`, it.kind === "waiting");

function requestBody(it) {
  const i = it.tool_input || {};
  const t = tool(it);
  if (it.kind === "question") {
    const d = draft(it.id);
    const qs = questions(it);
    const opts = (q, qi) => `<div class="opts">${(q.options || []).map((o, oi) => {
      const on = q.multiSelect ? d.choices[q.question]?.has?.(o.label) : d.choices[q.question] === o.label;
      return `<button class="opt ${on ? "on" : ""}" data-pick="${qi}:${oi}" data-id="${esc(it.id)}">${oi + 1} · ${esc(o.label)}${o.description ? `<span class="d">: ${esc(o.description)}</span>` : ""}</button>`;
    }).join("")}</div>`;
    if (qs.length === 1) return `<div class="qtext">${esc(qs[0].question)}</div>${opts(qs[0], 0)}${qs[0].multiSelect ? submitAnswers(it) : ""}`;
    // Several questions: one at a time, with tabs to move between them (like Claude Code's own prompt).
    const cur = stepOf(it);
    const tabs = qs.map((q, n) => `<button class="qtab ${n === cur ? "on" : ""} ${isAnswered(it, q) ? "done" : ""}" data-qstep="${esc(it.id)}:${n}" title="${esc(q.question)}">${isAnswered(it, q) ? "✓ " : ""}${esc(q.header || `Question ${n + 1}`)}</button>`).join("");
    const q = qs[cur];
    const nav = q.multiSelect && cur < qs.length - 1 ? `<button class="btn" data-qstep="${esc(it.id)}:${cur + 1}">Next question</button>` : "";
    // An answer you typed yourself (not one of the options) shows under the question.
    const typed = [...(q.multiSelect ? d.choices[q.question] || [] : [d.choices[q.question]].filter(Boolean))].filter((c) => !(q.options || []).some((o) => o.label === c));
    const yours = typed.length ? `<div class="typed-answer">Your answer: “${esc(typed.join(", "))}”</div>` : "";
    return `<div class="qtabs">${tabs}<span class="qcount">${cur + 1} of ${qs.length}</span></div><div class="qtext">${esc(q.question)}${q.multiSelect ? `<span class="dim"> · pick any</span>` : ""}</div>${opts(q, cur)}${yours}<div class="row-btns">${nav}${submitAnswers(it)}</div>`;
  }
  if (t === "bash") return `<pre class="slab">$ ${esc(i.command)}</pre>`;
  if (t === "edit" || t === "multiedit") {
    const edits = t === "multiedit" ? i.edits || [] : [{ old_string: i.old_string, new_string: i.new_string }];
    return `<div class="dim" style="font-size:12px">${esc(shortPath(i.file_path || ""))}</div>` + edits.map((e) => `<div class="diff"><pre class="del">− ${esc(e.old_string)}</pre><pre>+ ${esc(e.new_string)}</pre></div>`).join("");
  }
  if (t === "write") return `<div class="dim" style="font-size:12px">${esc(shortPath(i.file_path || i.path || ""))}</div><pre class="slab">${esc(i.content)}</pre>`;
  return `<pre class="slab">${esc(JSON.stringify(i, null, 2))}</pre>`;
}
/** Send all answers: shown once every question has one (needed when any allow several picks). */
function submitAnswers(it) {
  if (!questions(it).some((q) => q.multiSelect)) return "";
  const ready = !!answersFor(it);
  return `<span class="grow"></span><button class="btn primary" data-act="submit-answers" data-id="${esc(it.id)}" ${ready ? "" : "disabled"}>${ready ? "Send answers" : "Answer each question"}</button>`;
}
function whyLine(it) {
  const c = (it.context || []).filter((x) => x.role !== "user").at(-1) || it.context?.at(-1);
  return c ? `<div class="why"${c.text.trim().includes("\n") ? "" : " data-cut"} title="${esc(c.text)}">Why: ${esc(firstLine(c.text))}</div>` : "";
}
function decisionButtons(it) {
  if (inTerminal(it)) return `<div class="row-btns"><span class="dim">Cue can't answer this one: answer it in its terminal.</span><span class="grow"></span><button class="btn primary" data-act="go" data-id="${esc(it.id)}">Go to tab</button></div>`;
  if (redirectFor === it.id) return `<div class="row-btns">${field(it, it.kind === "question" ? (questions(it).length > 1 ? `Your own answer to question ${stepOf(it) + 1}…` : "Your own answer…") : "Tell it what to do instead…", it.kind === "question" ? (questions(it).length > 1 && questions(it).filter((q) => !isAnswered(it, q)).length > 1 ? "Next" : "Send") : "Redirect")}</div>`;
  if (it.kind === "question") return `<div class="row-btns"><button class="btn deny" data-act="deny" data-id="${esc(it.id)}">Decline</button><button class="btn" data-act="redirect" data-id="${esc(it.id)}">Type answer…</button><span class="grow"></span><button class="btn" data-act="go" data-id="${esc(it.id)}">Tab</button></div>`;
  const sugg = it.suggestions || [];
  const menu = menuFor === it.id && sugg.length ? `<div class="menu">${sugg.map((s, n) => { const x = describeSuggestion(s); return `<button data-always="${n}" data-id="${esc(it.id)}"><div>${esc(x.title)}</div><div class="sub">${esc(x.where)}</div></button>`; }).join("")}</div>` : "";
  return `<div class="row-btns"><button class="btn deny flex" data-act="deny" data-id="${esc(it.id)}">Deny</button>
    ${sugg.length ? `<span class="always">${menu}<button class="btn" data-act="menu" data-id="${esc(it.id)}">Always…</button></span>` : ""}
    <button class="btn" data-act="redirect" data-id="${esc(it.id)}" title="Deny and tell it what to do instead">↪</button>
    <button class="btn primary flex" data-act="allow" data-id="${esc(it.id)}">Allow</button></div>`;
}

/** What the agent wrote after a Stop hook made it continue — collapsed under the real answer. */
function followHtml(it, always = false) {
  if (!it.followup) return "";
  const open = always || openFollow.has(it.id);
  return `<div class="follow"><button class="thread-toggle" data-follow="${esc(it.id)}">After a stop hook ${open ? "▴" : "▾"}</button>${open ? `<div class="follow-body">${md(it.followup)}</div>` : ""}</div>`;
}

// ---------- recently answered (on the page, not hidden in History) ----------
/** Outcome text as shown (older saved entries used em dashes). */
const outcomeText = (i) => String(i.outcome || i.status || "").replace(/\s+—\s+/g, ": ");
const outcomeClass = (i) => (/^(allowed|answered|replied)/.test(i.outcome) ? "ok" : /^denied/.test(i.outcome) ? "no" : "");
function recentEntry(i) {
  const fresh = now() - (i.resolved_ms || 0) < 8000 ? "fresh" : "";
  // Its session is at work again (on what you answered, or since): its dot pulses, as working ones do.
  const live = sessionOf(i.session_id)?.state === "working" ? "live" : "";
  return `<div class="tl ${outcomeClass(i)} ${fresh} ${live} ${!quietOpen && active?.exact && active?.id === i.id ? "on" : ""}" data-detail="${esc(i.id)}">
    <div class="tl-top">${nameSpan(i.session_id, i.project, "tl-name")}<span>${esc(agentName(i.harness))}</span><span style="margin-left:auto">${ago(i.resolved_ms || i.created_ms)}</span></div>
    <div class="tl-title ${isBash(i) ? "mono" : ""}">${esc(plain(summary(i)))}</div><div class="tl-out">${esc(outcomeText(i))}</div>${i.images?.length ? thumbs(i.images) : ""}</div>`;
}

/** A starred session in its drawer, drawn like Recently answered: who, what it's on now, and its state
 *  in words. The dot: pulsing while it works, red when it asks you, Moss on your turn, amber when stuck. */
function starEntry(s, open) {
  const sid = s.session_id, it = state.items.find((i) => i.session_id === sid);
  const stuck = s.state === "working" && s.stuck_ms && !s.compacting_ms;
  const dot = stuck ? "stuck" : it ? (it.kind === "waiting" ? "ok" : "no") : s.state === "working" || s.compacting_ms ? "live" : "";
  const what = it ? plain(summary(it)) : s.compacting_ms ? "Compacting…" : s.state === "working" ? s.doing || (s.prompt ? `› ${s.prompt}` : "Thinking") : s.prompt ? `› ${s.prompt}` : plain(firstLine(lastSaid(s)));
  const st = stuck ? `no new output for ${ago(s.stuck_ms)}` : it ? (it.kind === "waiting" ? (it.interrupted ? "interrupted" : "your turn") : `asks you · ${verb(it)}`) : s.state === "working" ? "working" : idleNote(s) || "idle";
  return `<div class="tl star-tl ${dot} ${open ? "on" : ""}" data-session="${esc(sid)}">
    <div class="tl-top">${nameSpan(sid, s.project, "tl-name")}<span>${esc(agentName(s.harness))}</span><span style="margin-left:auto">${ago(it?.created_ms ?? s.since_ms)}</span>${starBtn(sid)}</div>
    <div class="tl-title" data-cut title="${esc(what)}">${esc(what)}</div><div class="tl-out">${esc(st)}</div></div>`;
}

// ---------- Board view: ACTIVE | WAITING | WORKING + RECENTLY ANSWERED ----------
/** Everything waiting on you, oldest first: finished turns and decisions together. */
const needsYou = () => [...groups().decide, ...groups().yours].sort((a, b) => a.created_ms - b.created_ms);
const isPending = (it) => !!it && state.items.some((i) => i.id === it.id);

/** What the Active pane shows: your pick (kept after you act), else the oldest thing waiting on you. */
function current() {
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
  if (spot.sid && (card || sessionOf(spot.sid))) active = { id: card?.id || null, sid: spot.sid };
  renderMain();
}
function setActive(id, sid, exact = false) {
  view = "board";
  quietOpen = null;
  active = { id: id || null, sid: sid || findItem(id)?.session_id || null, exact };
  seenSession(active.sid);
  loadSteps(active.sid);   // now, not on the next tick
  sheet = menuFor = redirectFor = btwFor = null;
  renderMain();
  // Picked a session (any list, a chip, the live list's Enter): ready to type to it.
  focusComposer();
}
/** The next thing waiting on you that isn't on screen. */
const nextUp = () => needsYou().find((i) => i.id !== active?.id && i.session_id !== active?.sid);
function goNext() { const n = nextUp(); if (n) setActive(n.id, n.session_id); }

// ---------- earlier messages: a page at a time from Cue's log as you scroll up ----------
const OLDER_PAGE = 30;
const older = new Map();   // session id -> { msgs (oldest first), anchor (the thread's first message when they loaded), done, tdone, busy }
/** The next page before the oldest message shown: from Cue's log as you scroll up; past its start, from
 *  the session's transcript, only when you click for it (`fromTranscript`: a heavier read). */
async function loadOlder(sid, fromTranscript = false) {
  const o = older.get(sid) || { msgs: [], anchor: 0, done: false, tdone: false, busy: false };
  older.set(sid, o);
  const cur = current();
  if (o.busy || (fromTranscript ? !o.done || o.tdone : o.done) || !cur || (cur.s?.session_id || cur.it?.session_id) !== sid) return;
  const first = conversation(cur.it, cur.s)[0]?.at_ms;
  if (!first) return;
  o.busy = true;
  renderMain();
  let page = await invoke(fromTranscript ? "transcript_page" : "session_log_page", { sessionId: sid, beforeMs: first, limit: OLDER_PAGE }).catch(() => []);
  if (!Array.isArray(page)) page = [];
  if (!o.msgs.length) o.anchor = first;
  o.msgs = [...page, ...o.msgs];
  if (fromTranscript) o.tdone = page.length < OLDER_PAGE;
  else o.done = page.length < OLDER_PAGE;
  o.busy = false;
  const chat = document.querySelector("[data-chat]");
  if (chat) holdChat = chat.scrollHeight - chat.scrollTop;
  renderMain();
}
/** The thread keeps only the last few messages, so once earlier ones are loaded, new messages push
 *  its oldest out: fetch those back from the log so nothing goes missing between the two. */
async function fillOlderGap(sid, o, upTo) {
  const page = await invoke("session_log_page", { sessionId: sid, beforeMs: upTo, limit: 200 }).catch(() => []);
  o.msgs = [...o.msgs, ...page.filter((e) => e.at_ms >= o.anchor)];
  o.anchor = upTo;
  o.busy = false;
  renderMain();
}
/** The top of the chat: earlier messages to load, loading, or the start of what Cue has. */
function olderRow(sid, harness) {
  const o = older.get(sid);
  if (o?.busy) return `<div class="cv-older">Loading earlier messages…</div>`;
  if (o?.tdone) return `<div class="cv-older">Start of the conversation</div>`;
  // Cue's log starts when Cue first saw the session; anything before is only in Claude Code's transcript.
  if (o?.done) return `<div class="cv-older">Start of the conversation in Cue${harness === "claude" ? ` · <button data-act="load-transcript" data-sid="${esc(sid)}">Load earlier from its transcript</button>` : ""}</div>`;
  return `<div class="cv-older"><button data-act="load-older" data-sid="${esc(sid)}">Earlier messages</button></div>`;
}

/** The conversation for the Active pane: the live session's thread, or the item's own copy once the session is gone. */
function conversation(it, s) {
  let ex = s ? [...(s.thread || [])] : [...(it?.thread || [])];
  const o = older.get(s?.session_id || it?.session_id);
  if (o?.msgs.length) {
    const first = ex[0]?.at_ms ?? Infinity;
    if (first !== Infinity && first > o.anchor && !o.busy) { o.busy = true; queueMicrotask(() => fillOlderGap(s?.session_id || it?.session_id, o, first)); }
    ex = [...o.msgs.filter((e) => e.at_ms < first), ...ex];
  }
  // What you queued shows once, as queued: not also as the thread's copy of it (recorded at about the
  // same time; Claude Code's own report of it may come first, or carry the same words with more around them).
  const isQueued = (e) => e.role === "you" && Math.abs(e.at_ms - s.queued.at_ms) < 60000 && (e.text.trim() === s.queued.text.trim() || e.text.includes(s.queued.text.trim()));
  if (s?.queued) ex = ex.filter((e) => !isQueued(e));
  if (it?.kind === "waiting") {
    const msg = finishedText(it);
    if (!ex.some((e) => e.role !== "you" && e.text.trim() === msg.trim())) ex.push({ role: "agent", text: msg, at_ms: it.created_ms });
    // In time order: that card's message may have scrolled out of the session's last few exchanges
    // (Settings → Context) while you kept talking; appended, it would sit below your newer messages.
    ex.sort((a, b) => a.at_ms - b.at_ms);
  }
  // A pending decision is drawn as the action card below; drop the one-line note of it.
  if (isPending(it) && it.kind !== "waiting" && ex.at(-1)?.role !== "you") ex.pop();
  return ex;
}
/** What you forwarded from another session, as Cue typed it: an optional note, then the header and the message. */
const FORWARDED = /^(?:([\s\S]*?)\n\n)?\[Forwarded from the (.+?) session\]\n([\s\S]*)$/;
const fwdMsgs = new Map();  // chat key -> { text, from, fromSid }: what "Send to another session" sends
/** A message from another session: an accent stripe, who it's from, then the message (a forward quotes the original). */
function peerBubble(from, how, at, body, quote) {
  return `<div class="cv-peer"><div class="cv-peer-from">↘ From ${esc(from)}${how ? ` · ${esc(how)}` : ""} · ${ago(at)} ago</div>
    ${body ? `<div class="msg cv-text">${md(body)}</div>` : ""}${quote ? `<div class="cv-peer-quote">${md(quote)}</div>` : ""}</div>`;
}
// ---------- steps: what the agent did between your message and its reply ----------
// Read from the session's transcript by Cue on demand (only for the session open here), never saved.
// Settings → Steps: one line each (open a step for its output or diff), everything open, or hidden.
const stepFeeds = new Map();   // sid -> { version, turns, at }
const stepFlip = new Set();    // "sid|id": steps you opened (or, where everything is open, closed)
const turnFold = new Map();    // "sid|turn" -> open? (your choice over the default)
const turnAll = new Set();     // turns where "Show all" opened every step
const stepDetail = new Map();  // "sid|id" -> { full, output | diff } | "loading"
// Words and steps that came in while you watch: when (they appear smoothly, once). Not what was already
// there when the chat opened.
const stepArrived = new Map(); // "sid|item key" -> ms
const REVEAL_MS = 900;
/** The working light along the chat's top edge: brisk while steps land (a brighter flick for each new
 *  one), calmer after a quiet spell (a long think), still and amber once Cue thinks it's stuck. Where it
 *  is in its pass comes from the clock, so a redraw (they come often while it works) carries it on
 *  instead of starting it over. */
const SWEEP_MS = 1600, CALM_MS = 4200, QUIET_MS = 30000, FLICK_MS = 700;
function sweep(sid, s) {
  if (!(s?.state === "working" || s?.compacting_ms)) return "";
  if (s.stuck_ms && !s.compacting_ms) return `<i class="sweep stalled"></i>`;
  const lastStep = stepFeeds.get(sid)?.turns?.at(-1)?.items?.at(-1)?.at_ms || 0;
  const calm = now() - Math.max(s.doing_ms || 0, lastStep, s.compacting_ms || 0, s.since_ms || 0) > QUIET_MS;
  const period = calm ? CALM_MS : SWEEP_MS;
  const landed = Math.max(0, ...[...stepArrived].filter(([k]) => k.startsWith(`${sid}|`)).map(([, at]) => at));
  const since = Date.now() - landed;
  return `<i class="sweep ${calm ? "calm" : ""}" style="animation-duration:${period}ms;animation-delay:-${Date.now() % period}ms"></i>`
    + (since < FLICK_MS ? `<i class="sweep flick" style="animation-delay:-${since}ms"></i>` : "");
}
const itemKey = (turn, x, i) => (x.t === "step" ? x.id : `${turn.at_ms}:${i}`);
let stepsBusy = false;
const stepsMode = () => state.settings?.steps?.mode || "line";
const STEP_ICON = { read: "R", search: "?", edit: "E", run: "$", web: "W", agent: "A", todo: "✓", ask: "?", other: "•" };
async function loadSteps(sid) {
  if (stepsBusy || !sid) return;
  stepsBusy = true;
  const f = stepFeeds.get(sid);
  try {
    const r = await invoke("session_steps", { sessionId: sid, version: f?.version || 0 });
    if (r.turns && f) {
      const had = new Set(f.turns.flatMap((t) => t.items.map((x, i) => itemKey(t, x, i))));
      for (const [k, at] of stepArrived) if (Date.now() - at > REVEAL_MS) stepArrived.delete(k);   // done animating
      for (const t of r.turns) t.items.forEach((x, i) => { const k = itemKey(t, x, i); if (!had.has(k)) stepArrived.set(`${sid}|${k}`, Date.now()); });
    }
    stepFeeds.set(sid, { version: r.version, turns: r.turns ?? f?.turns ?? [], meta: r.turns ? r.meta : f?.meta, at: Date.now() });
    if (r.turns) { renderMain(); wantDetails(sid); }
  } catch {} finally { stepsBusy = false; }
}
/** Fetch the output / diff of every step that's open and not fetched yet (one call). */
async function wantDetails(sid) {
  const f = stepFeeds.get(sid);
  if (!f) return;
  const ids = [];
  for (const t of f.turns) {
    const all = stepsMode() === "all" || turnAll.has(`${sid}|${t.at_ms}`);
    for (const x of t.items) {
      const k = `${sid}|${x.id}`;
      if (x.t === "step" && x.done && all !== stepFlip.has(k) && !stepDetail.has(k)) ids.push(x.id);
    }
  }
  const want = ids.slice(-60);
  if (!want.length) return;
  want.forEach((id) => stepDetail.set(`${sid}|${id}`, "loading"));
  let got = {};
  try { got = await invoke("step_detail", { sessionId: sid, ids: want }); } catch {}
  want.forEach((id) => stepDetail.set(`${sid}|${id}`, got[id] || {}));
  renderMain();
}
/** "read 3 files · 2 edits · 4 commands · 1 failed": a folded turn in a few words. */
function turnSummary(steps) {
  const n = (k) => steps.filter((x) => x.kind === k).length;
  const bad = steps.filter((x) => x.bad).length;
  return [[n("read"), "read %", "file"], [n("search"), "%", "search", "searches"], [n("edit"), "%", "edit"], [n("run"), "%", "command"], [bad, "% failed"]]
    .filter(([c]) => c).map(([c, f, one, many]) => f.replace("%", one ? `${c} ${c === 1 ? one : many || one + "s"}` : c)).join(" · ");
}
function stepResult(x) {
  if (!x.done) return `<span class="dot-live"></span>`;
  const m = /^\+(\d+) −(\d+)$/.exec(x.result);
  if (m) return `<span class="add">+${m[1]}</span> <span class="del">−${m[2]}</span>`;
  return esc(x.result);
}
function stepDetailHtml(k, x) {
  const d = stepDetail.get(k);
  if (!d || d === "loading") return `<div class="stx-detail"><span class="dim">Loading…</span></div>`;
  // The whole command (or pattern, URL) when the line had to cut it; a file's full path, small.
  const cutShort = d.full && d.full.trim() !== x.subject.replace(/^“|”$/g, "").trim();
  const full = !d.full ? "" : x.kind === "edit" || x.kind === "read" ? `<div class="stx-path mono">${esc(d.full)}</div>` : cutShort ? `<div class="stx-full mono">${esc(d.full)}</div>` : "";
  let body = "";
  if (d.diff) {
    body = `<div class="stx-diff mono">${d.diff.map(([op, t]) => op === "…" ? `<div class="gap">⋯</div>` : `<div class="${op === "+" ? "a" : op === "-" ? "d" : ""}"><i>${op === " " ? "" : op}</i>${esc(t) || " "}</div>`).join("")}${d.skipped ? `<div class="gap">${d.skipped} more lines</div>` : ""}</div>`;
  } else if (d.output?.length) {
    const rows = d.output.map((l) => `<div>${esc(l) || " "}</div>`);
    if (d.skipped) rows.splice(d.gap_at || 0, 0, `<div class="gap">⋯ ${d.skipped} lines skipped ⋯</div>`);
    body = `<div class="stx-out mono">${rows.join("")}</div>`;
  } else if (x.kind !== "read") body = `<div class="dim">No output.</div>`;
  return `<div class="stx-detail">${full}${body}</div>`;
}
/** Just arrived: its arrival animation, picked up where it is. The chat may redraw several times
 *  while it plays; starting over would fade it in twice, dropping it would cut it short. Returns the
 *  attributes to add, or "" once it's done. */
const reveal = (k, ms, extra = "") => {
  const t = Date.now() - (stepArrived.get(k) || 0);
  return t < ms ? ` reveal" style="${extra}animation-delay:-${t}ms` : "";
};
function stepLine(sid, x, all) {
  const k = `${sid}|${x.id}`;
  const open = x.done && all !== stepFlip.has(k);
  return `<button class="stx-line ${x.bad ? "bad" : ""} ${open ? "open" : ""}${reveal(k, 250)}" ${x.done ? `data-step="${esc(k)}"` : "disabled"}>
    <i class="stx-ic k-${esc(x.kind)}">${STEP_ICON[x.kind] || "•"}</i><span class="stx-what">${esc(x.verb)}${x.subject ? ` <span class="${x.mono ? "mono" : ""}">${esc(x.subject)}</span>` : ""}</span>
    <span class="stx-res">${stepResult(x)}</span><span class="stx-chev">${x.done ? (open ? "⌄" : "›") : ""}</span></button>${open ? stepDetailHtml(k, x) : ""}`;
}
/** One turn's steps, between your message and its reply. `live`: the turn it's working on now;
 *  `latest`: the newest turn (open by default, so you see what led to its reply). `part`: when you
 *  wrote mid-turn, the turn is drawn in parts around your messages ({ from, to } ms); only the first
 *  part has the header (its count is the whole turn's). */
function stepsBlock(sid, turn, live, latest, part = { from: -Infinity, to: Infinity }, said = new Set()) {
  const mode = stepsMode();
  let kept = turn.items;
  // A finished turn's last words are its reply, already in the chat; so are words the chat shows as a
  // reply mid-turn (the answer, when a Stop hook made it carry on): not twice.
  if (!live) { let end = kept.length; while (end && kept[end - 1].t === "say") end--; kept = kept.slice(0, end); }
  kept = kept.filter((x) => x.t !== "say" || !said.has(String(x.text || "").trim()));
  const steps = kept.filter((x) => x.t === "step");
  if (!steps.length) return "";
  const first = part.from === -Infinity;
  const items = kept.map((x, i) => [x, i]).filter(([x]) => x.at_ms >= part.from && x.at_ms < part.to);
  const key = `${sid}|${turn.at_ms}`;
  const open = turnFold.has(key) ? turnFold.get(key) : mode === "all" || (mode === "line" && (live || latest));
  const all = mode === "all" || turnAll.has(key);
  const count = `${steps.length}${turn.more ? "+" : ""} step${steps.length === 1 ? "" : "s"}`;
  const head = `<div class="stx-head"><button class="stx-fold" data-turn-fold="${esc(key)}" data-open="${open ? 1 : 0}">${live ? `<span class="dot-live"></span>` : `<span class="stx-tri">${open ? "▾" : "▸"}</span>`}<b>${live ? `Working · ${count}` : count}</b>${open ? "" : `<span class="stx-sum"> · ${esc(turnSummary(steps))}</span>`}</button>${open ? `<button class="stx-all" data-turn-all="${esc(key)}">${all ? "Show less" : "Show all"}</button>` : ""}</div>`;
  if (!open) return first ? `<div class="stx">${head}</div>` : "";
  if (!items.length) return first ? `<div class="stx">${head}</div>` : "";
  let body = "", run = [];
  const flush = () => { if (run.length) body += `<div class="stx-steps">${run.join("")}</div>`; run = []; };
  items.forEach(([x, i]) => {
    if (x.t !== "say") return run.push(stepLine(sid, x, all));
    flush();
    // New words unroll top to bottom, quicker for short ones.
    const ms = Math.min(REVEAL_MS, 250 + x.text.length * 2);
    const rv = reveal(`${sid}|${itemKey(turn, x, i)}`, ms, `--rv:${ms}ms;`);
    body += `<div class="stx-say msg${rv}">${md(x.text)}</div>`;
  });
  flush();
  if (!first) return `<div class="stx">${body}</div>`;
  return `<div class="stx">${head}${turn.more ? `<div class="stx-more dim">${turn.more} earlier steps not shown</div>` : ""}${body}</div>`;
}
/** The chat with each turn's steps put in by time: after your message, before the reply. `extra`:
 *  more lines to place by time (what you chose on a decision, when you chose it). */
function withSteps(sid, s, ex, rows, extra = []) {
  const f = sid && stepFeeds.get(sid);
  const first = ex[0]?.at_ms ?? 0;
  const turns = f?.turns || [];
  const lastTurn = turns.at(-1);
  // Its replies, already in the chat: a Stop hook that made it carry on leaves the answer mid-turn.
  const said = new Set(ex.filter((e) => e.role === "agent").map((e) => e.text.trim()));
  // A turn you wrote into while it ran is split at your messages, so the chat stays in time order.
  const blocks = turns
    .filter((t) => (t.items.at(-1)?.at_ms ?? t.at_ms) >= first || (!ex.length && t === lastTurn))
    .flatMap((t) => {
      const start = t.items[0]?.at_ms ?? t.at_ms, end = t.items.at(-1)?.at_ms ?? start;
      const cuts = ex.map((e) => e.at_ms).filter((a) => a > start && a <= end);
      const bounds = [-Infinity, ...cuts, Infinity];
      return cuts.concat([null]).map((_, k) => ({
        at: k === 0 ? start : cuts[k - 1] + 0.5,
        html: stepsBlock(sid, t, t === lastTurn && s?.state === "working", t === lastTurn, { from: bounds[k], to: bounds[k + 1] }, said),
      }));
    })
    .concat(extra)
    .filter((b) => b.html)
    .sort((a, b) => a.at - b.at);
  if (!blocks.length) return rows;
  const out = [];
  let b = 0;
  ex.forEach((e, n) => { while (b < blocks.length && blocks[b].at < e.at_ms) out.push(blocks[b++].html); out.push(rows[n]); });
  while (b < blocks.length) out.push(blocks[b++].html);
  return out;
}
/** Agent replies left open: the newest few, so the one you were reading doesn't fold the moment the next lands. */
const OPEN_REPLIES = 3;
/** Agent: full-width prose. You: a compact tinted bubble on the right. Older agent turns fold to 3 lines.
 *  Another session: a striped bubble saying which one (sent by its agent, or forwarded by you). */
/** `paged`: the Active pane's chat, which loads earlier messages as you scroll up. */
function chatHtml(it, s, harness, paged = false) {
  const ex = conversation(it, s);
  const lastAgent = ex.map((e) => e.role).lastIndexOf("agent");
  const agentAt = ex.flatMap((e, n) => (e.role === "agent" ? [n] : []));
  const recent = new Set(agentAt.slice(-OPEN_REPLIES));
  const project = s?.project || it?.project || "";
  const sid = s?.session_id || it?.session_id;
  // What you chose on a decision sits in the chat when you chose it (later steps come below it).
  const chose = it && !isPending(it) && it.resolved_ms ? [{ at: it.resolved_ms, html: resultLine(it) }] : [];
  const rows = withSteps(sid, s, ex, ex.map((e, n) => {
    if (e.role === "peer") return peerBubble(e.from || "another agent", "", e.at_ms, e.text);
    const fwd = e.role === "you" && FORWARDED.exec(e.text);
    if (fwd) return peerBubble(fwd[2], "forwarded by you", e.at_ms, fwd[1], fwd[3]);
    if (e.role === "you") {
      const how = lastSent && lastSent.sid === (s?.session_id || it?.session_id) && e.text.startsWith(lastSent.text) ? lastSent.via : "";
      // "queued via iTerm session: it reads this…" read as "via queued via…": say it plainly.
      const via = !how ? "" : /^queued/.test(how) ? " · queued, it reads this at its next step" : ` · via ${esc(how)}`;
      // Typed into its terminal, but it never became a message: say so where you'll see it.
      const unsent = e.unsent ? `<div class="cv-unsent">⚠ This didn't go through as a message. It may have run as a command, or still be in its box. ${sid ? `<button data-act="go-session" data-sid="${esc(sid)}">Go to tab</button>` : ""}</div>` : "";
      return `<div class="cv-you ${e.unsent ? "unsent" : ""}"><div class="cv-you-text">${linkify(esc(e.text)).replace(/\n/g, "<br>")}</div>${thumbs(e.images)}<div class="cv-meta">You · ${ago(e.at_ms)} ago${via}</div>${unsent}</div>`;
    }
    const key = `${s?.session_id || it?.id}:${e.at_ms}`;
    const long = e.text.length > 280 || e.text.split("\n").length > 4;
    const folded = !recent.has(n) && long && !openMsgs.has(key);
    const follow = it?.followup && e.text === it.message ? followHtml(it) : "";
    fwdMsgs.set(key, { text: e.text, from: project, fromSid: sid });
    return `<div class="cv-agent ${folded ? "folded" : ""} ${n === lastAgent ? "last" : ""}"><div class="cv-meta">${esc(agentName(harness))} · ${ago(e.at_ms)} ago</div>
      <div class="msg cv-text">${md(e.text)}</div>${thumbs(e.images)}${localImages(e.text)}
      <div class="cv-acts">${!recent.has(n) && long ? `<button class="cv-more" data-msg="${esc(key)}">${folded ? "Show all" : "Fold"}</button>` : ""}<button class="cv-fwd" data-fwd="${esc(key)}">↗ Send to another session…</button></div>${follow}</div>`;
  }), chose);
  for (const o of outbox) if (o.sid === (s?.session_id || it?.session_id) && !landed(o)) rows.push(`<div class="cv-you ${o.via ? "" : "sending"}"><div class="cv-you-text">${esc(o.text).replace(/\n/g, "<br>")}</div>${o.images.length ? `<div class="thumbs">${o.images.map((im) => `<span class="thumb"><img src="${esc(im.data)}" alt=""/></span>`).join("")}</div>` : ""}<div class="cv-meta">${o.via ? `Sent · via ${esc(o.via)}` : "Sending…"}</div></div>`);
  if (s?.queued) rows.push(`<div class="cv-you queued"><div class="cv-you-text">${esc(s.queued.text).replace(/\n/g, "<br>")}</div>${thumbs(s.queued.images)}<div class="cv-meta">Queued · it reads this when it finishes the current step · <button class="q-now" data-act="send-now" data-sid="${esc(s.session_id)}" title="Stops its turn so it reads this now (⌘↵)">Send now</button></div></div>`);
  return rows.length ? (paged && sid ? olderRow(sid, harness) : "") + rows.join("") : `<div class="dim cv-empty">No messages yet in this session.</div>`;
}
/** A turn's changed files, one per line, for the pill's tooltip. */
const changesTip = (ch) => ch.files.map((f) => `${f.path}  +${f.add} −${f.del}`).join("\n");
/** One line under the chat: where the session is now. */
function statusLine(it, s) {
  // Started in a folder Claude Code doesn't trust yet: it's asking in its terminal before it does anything.
  if (s?.trust_ms) return `<div class="cv-status stuck"><span class="lim-dot"></span><span><b>Claude Code is asking whether you trust ${esc(homeless(s.cwd) || "this folder")}.</b> It won't start until you answer.</span><button class="btn small primary" data-act="trust" data-sid="${esc(s.session_id)}" title="Answers “Yes, I trust this folder” in its terminal">Trust folder</button><button class="btn small" data-act="go-session" data-sid="${esc(s.session_id)}">Go to tab</button></div>`;
  // Working, but nothing new from it in a while: a command waiting for input in its terminal, or hung.
  if (s?.state === "working" && s.stuck_ms) return `<div class="cv-status stuck"><span class="lim-dot"></span><span><b>No new output for <span data-ago="${s.stuck_ms}">${ago(s.stuck_ms)}</span>.</b> ${esc((s.doing || "Thinking").replace(/…$/, ""))}. If it's waiting for input, it's in its terminal.</span><button class="btn small" data-act="go-session" data-sid="${esc(s.session_id)}">Go to tab</button></div>`;
  // Compact (⋯) typed /compact: it's summarizing, until Claude Code says it's done.
  if (s?.compacting_ms) return `<div class="cv-status"><span class="dot-live"></span>Compacting: summarizing the conversation to free up context<span class="dim"> · <span data-ago="${s.compacting_ms}">${ago(s.compacting_ms)}</span></span></div>`;
  // It finished compacting (until the next turn starts).
  if (s?.compacted_ms && s.state !== "working") return `<div class="cv-status"><span class="ok-dot"></span>Compacted: the conversation was summarized and its context freed<span class="dim"> · <span data-ago="${s.compacted_ms}">${ago(s.compacted_ms)}</span> ago</span></div>`;
  // What it's doing right now (its latest step), else the prompt it's on.
  if (s?.state === "working" && s.doing) return `<div class="cv-status"><span class="dot-live"></span>${esc(s.doing)}<span class="dim"> · <span data-ago="${s.doing_ms}">${ago(s.doing_ms)}</span></span></div>`;
  // Nothing in its transcript yet (it's thinking; Claude Code writes a message once it's whole): a live
  // count since the turn started, so a long think doesn't look stuck.
  if (s?.state === "working") return `<div class="cv-status"><span class="dot-live"></span>Working${s.prompt ? ` on: ${esc(s.prompt.length > 120 ? s.prompt.slice(0, 120) + "…" : s.prompt)}` : ""}<span class="dim"> · <span data-ago="${s.since_ms}">${ago(s.since_ms)}</span></span></div>`;
  if (s?.state === "waiting" || s?.state === "deciding") return `<div class="cv-status"><i class="sw z live"></i>Waiting on you · ${ago(s.since_ms)}</div>`;
  if (s?.state === "agent") return `<div class="cv-status">Idle, waiting on ${esc(s.driven_by || "another agent")}</div>`;
  if (s?.state === "limited" && s.limit) {
    const l = s.limit, what = s.prompt ? `“${esc(s.prompt.length > 80 ? s.prompt.slice(0, 80) + "…" : s.prompt)}”` : "Your last message";
    if (lifted(l)) return `<div class="cv-status limit"><span class="lim-dot back"></span><span><b>${esc(scopeName(l))} is back.</b> ${what} didn't run.</span><button class="btn small" data-usage="resend-one" data-sid="${esc(s.session_id)}">Resend</button></div>`;
    const when = l.resets_ms ? ` · resets ${clockAt(l.resets_ms)}${l.resets_ms - now() < 20 * 3600000 ? `, in ${untilText(l.resets_ms - now())}` : ""}` : "";
    const head = l.scope === "all" ? "Out of usage" : `${esc(l.scope)} used up`;
    const tail = l.scope === "all" ? "Cue will tell you when it resets." : "Switch models with /model to keep going.";
    return `<div class="cv-status limit" data-cut title="${esc(l.text)}"><span class="lim-dot"></span><span><b>${head}</b>${when}. ${what} didn't run. ${tail}</span></div>`;
  }
  if (s?.state === "stopped") return `<div class="cv-status">Stopped by you · it's at its prompt</div>`;
  if (!s && it && !isPending(it)) return `<div class="cv-status">This session has ended.</div>`;
  return "";
}
/** After a decision: what you chose. Replies need no line, they're already in the chat. */
function resultLine(it) {
  if (!it || isPending(it) || !it.resolved_ms || it.kind === "waiting") return "";
  return `<div class="cv-result ${outcomeClass(it)}"><span class="sent-check">${outcomeClass(it) === "no" ? "✕" : "✓"}</span><span>${esc(outcomeText(it))}</span><span class="grow"></span><span class="dim nowrap">${ago(it.resolved_ms)} ago</span></div>`;
}
function nextBar() {
  const n = nextUp();
  if (!n) return "";
  // × only on a finished turn; a decision would just hide something the agent is still blocked on.
  const clear = n.kind === "waiting" ? `<button class="next-clear" data-act="dismiss" data-id="${esc(n.id)}" title="Take it off Waiting" aria-label="Take it off Waiting">×</button>` : "";
  return `<div class="next-bar" data-act="next" role="button" tabindex="0"><span class="next-label">Next</span>${badge(n.harness)}<span class="next-proj">${esc(n.project)}</span><span class="next-what">${esc(plain(summary(n)))}</span>${clear}<kbd>N</kbd></div>`;
}

/** The chat header's ⋯: what's used now and then. Compact (any agent's /compact), a relay lead's Hand
 *  off (asks once more first: the lead steps down), and Decide later / Back to Waiting. */
let moreFor = null, handoffArm = null;
function moreMenu(sid, s) {
  const open = moreFor === sid;
  const busy = s?.state === "working";
  const m = crewOf(sid);
  const lead = m?.role === "lead" && m?.plugin === "relay";
  const later = isParked(sid) ? `<button data-park="${esc(sid)}:0">Back to Waiting</button>` : `<button data-park="${esc(sid)}:1">Decide later<span>Move it to Need to decide while you think it over</span></button>`;
  const items = !open ? "" : `<div class="more-menu">
      <button data-cmd="compact" data-sid="${esc(sid)}" ${busy ? "disabled" : ""}>Compact<span>${busy ? "When this turn ends" : "Summarize the conversation to free up context"}</span></button>
      ${lead ? `<button data-cmd="handoff" data-sid="${esc(sid)}" ${busy ? "disabled" : ""} class="${handoffArm === sid ? "armed" : ""}">${handoffArm === sid ? "Hand off? Click again" : "Hand off…"}<span>${busy ? "When this turn ends" : "It writes its notes, opens a successor lead and steps down"}</span></button>` : ""}
      ${later}</div>`;
  return `<span class="more-wrap"><button class="btn more-btn" data-more="${esc(sid)}" aria-label="More" aria-expanded="${open}">⋯</button>${items}</span>`;
}
/** "claude-opus-5-5" → "Opus 5.5", "gpt-5.6-terra" → "GPT-5.6 Terra", "deepseek-v4-pro" → "DeepSeek V4 Pro". */
function prettyModel(id) {
  const m = String(id || "").replace(/\[.*\]$/, "").replace(/^.*\//, "");
  const claude = /^claude-([a-z]+)-(\d+)(?:-(\d+))?/.exec(m);
  if (claude) return `${claude[1][0].toUpperCase()}${claude[1].slice(1)} ${claude[2]}${claude[3] ? `.${claude[3]}` : ""}`;
  return m.split("-").map((w) => /^gpt$/i.test(w) ? "GPT" : /^deepseek$/i.test(w) ? "DeepSeek" : /^v\d/i.test(w) ? w.toUpperCase() : w[0] ? w[0].toUpperCase() + w.slice(1) : w)
    .join(" ").replace(/^GPT (\d)/, "GPT-$1");
}
/** The open session's model, how full its context is, and its cost so far
 *  (where the agent logs it), on the chat's title line after Claude's title; context as a small gauge. From its log. */
function metaInline(sid) {
  const m = sid && stepFeeds.get(sid)?.meta;
  if (!m || (!m.model && !m.context)) return "";
  const pct = m.window ? Math.min(100, Math.round((m.context / m.window) * 100)) : null;
  const lvl = pct === null ? "" : pct >= 95 ? "full" : pct >= 80 ? "high" : "";
  const k = (n) => (n >= 1e6 ? `${(n / 1e6).toFixed(n % 1e6 ? 2 : 0)}M` : `${Math.round(n / 1000)}k`);
  const money = (c) => `$${c >= 100 ? Math.round(c) : c.toFixed(2)}`;
  const tip = [m.model && `Model: ${m.model}`, m.window && `Context: ${k(m.context)} of ${k(m.window)} tokens (${pct}%)`, m.cost != null && `Cost so far: ${money(m.cost)}`].filter(Boolean).join("\n");
  // Just compacted: its log's last count is from before, so don't show it until a new reply brings a fresh one.
  const s = sessionOf(sid);
  const freed = s?.compacted_ms && s.state !== "working";
  const parts = [m.model ? `<b>${esc(prettyModel(m.model))}</b>` : "", freed ? `<span class="mi-ctx">context freed</span>` : pct !== null ? `<span class="mi-ctx ${lvl}"><span class="ctx-bar"><i style="width:${pct}%"></i></span>${pct}% context</span>` : "", m.cost != null ? money(m.cost) : ""].filter(Boolean);
  // Context high and it's not mid-turn: Compact right here (the same as ⋯ → Compact).
  const chip = pct !== null && pct >= 80 && s && s.state !== "working" && !s.compacting_ms && !freed ? `<button class="mi-compact" data-cmd="compact" data-sid="${esc(sid)}" title="Summarize the conversation to free up context">Compact</button>` : "";
  return `<span class="ap-meta" title="${esc(tip)}">${parts.join(" · ")}</span>${chip}`;
}
/** A quiet session open in Active: running, but started before Cue was connected, so it never
 *  reports to Cue. Read-only: what it's been doing, from its transcript, and how to make it a full one. */
let quietOpen = null;
function quietPane() {
  const q = (state.live || []).find((x) => x.session_id === quietOpen);
  if (!q) { quietOpen = null; return activePane(); }   // it ended (or reported in): back to the usual
  const f = stepFeeds.get(q.session_id);
  const busy = q.status === "busy";
  const turns = (f?.turns || []).slice(-8);
  const chat = turns.map((t, i) => quietTurn(q.session_id, t, busy && i === turns.length - 1, i === turns.length - 1, q.harness)).join("");
  return `<div class="active-pane ${busy ? "busy" : ""}">
    <div class="ap-head">${badge(q.harness)}<span class="proj">${esc(bareName(q.name) || baseName(q.cwd) || "session")}</span><span class="pill soft">${busy ? "working" : "quiet"}</span><span class="grow"></span>
      <button class="btn" data-sv="tab" data-sid="${esc(q.session_id)}">Go to tab</button></div>
    ${subHead(q.cwd, "", "", q.session_id)}
    <div class="quiet-note">It started before Cue was connected, so Cue can show what it's doing but can't answer it yet. In its terminal, type <code>/hooks</code> once to pick up Cue's hooks (or restart it with <code>claude --resume</code>; the conversation carries on), and you can reply, allow and answer from Cue.</div>
    <div class="ap-chat" data-chat>${chat || `<div class="dim cv-empty">${f ? "Nothing in its transcript yet." : "Reading its transcript…"}</div>`}</div></div>`;
}
/** One turn of a quiet session: your message, its steps, its reply (all from the transcript). */
function quietTurn(sid, t, live, latest, harness = "claude") {
  const you = t.prompt ? `<div class="cv-you"><div class="cv-you-text">${linkify(esc(t.prompt)).replace(/\n/g, "<br>")}</div><div class="cv-meta">You · ${ago(t.at_ms)} ago</div></div>` : "";
  // Its reply: the words after its last step (the steps leave a finished turn's last words to the chat).
  const tail = [];
  if (!live) for (let k = t.items.length - 1; k >= 0 && t.items[k].t === "say"; k--) tail.unshift(t.items[k]);
  const reply = tail.length ? `<div class="cv-agent"><div class="cv-meta">${esc(agentName(harness))} · ${ago(tail.at(-1).at_ms)} ago</div><div class="msg cv-text">${md(tail.map((x) => x.text).join("\n\n"))}</div></div>` : "";
  return you + stepsBlock(sid, t, live, latest) + reply;
}
function activePane() {
  if (quietOpen) return quietPane();
  const cur = current();
  if (!cur && state.connections && !state.connections.claude?.ok) return `<div class="active-pane empty"><div class="quiet-big">Connect Cue to Claude Code</div><div class="dim">Cue adds its hooks to Claude Code's settings (backed up first), then sessions that need you show up here.</div><button class="btn primary" data-connect="claude">Connect Claude Code</button></div>`;
  if (!cur) return `<div class="active-pane empty"><div class="quiet-big">Nothing waiting.</div><div class="dim">Pick anything on the right to read it or message the session.</div></div>`;
  const { it, s } = cur;
  const pending = isPending(it);
  const harness = it?.harness || s?.harness;
  const project = it?.project || s?.project;
  const sid = s?.session_id || it?.session_id;
  // A finished turn that changed files: the pill says what changed ("4 files · +98 −9"; hover for which), and Commit sits beside it.
  const ch = pending && it.kind === "waiting" && !it.interrupted ? s?.changes : null;
  const turnWord = ch ? `${ch.files.length} file${ch.files.length === 1 ? "" : "s"} · +${ch.add} −${ch.del}` : it?.interrupted ? "interrupted" : "your turn";
  const pill = pending
    ? `<span class="pill ${it.interrupted ? "intr" : ""}"${ch ? ` title="${esc(changesTip(ch))}"` : ""}>${it.kind === "waiting" ? turnWord : it.kind === "question" ? "asks you" : inTerminal(it) ? "asks in its terminal" : "needs a decision"} · ${ago(it.created_ms)}${it.kind === "waiting" ? `<button class="pill-x" data-act="dismiss" data-id="${esc(it.id)}" title="Take it off Waiting" aria-label="Take it off Waiting">×</button>` : ""}</span>`
    : `<span class="pill soft">${s ? { working: "working", waiting: "your turn", deciding: "deciding", agent: "on its lead", stopped: "stopped", limited: s.limit && lifted(s.limit) ? "usage is back" : "out of usage" }[s.state] || s.state : "answered"}</span>`;
  let foot;
  if (pending && it.kind !== "waiting") {
    // In its terminal: what Claude Code said ("A sandboxed command needs network access"), then the call it's about.
    const head = inTerminal(it) ? esc(it.message || `${agentName(harness)} ${verb(it)}`) : `${esc(agentName(harness))} ${esc(verb(it))}`;
    foot = `<div class="act-card"><div class="act-head">${head}</div>${inTerminal(it) && !it.tool_name ? "" : requestBody(it)}${it.kind !== "question" ? whyLine(it) : ""}${decisionButtons(it)}</div>`;
    // You were typing to this session when it asked: your box stays (cursor and text intact),
    // and sending waits until you've answered, so nothing gets typed into its prompt.
    const key = `s:${sid}`;
    if (document.activeElement?.dataset?.text === key || draft(key).text.trim() || draft(key).images.length) {
      foot += `<div class="foot held">${box_(key, "Answer above first; your message waits here…", "Answer first", `data-held="1" disabled`)}</div>`;
    }
  } else if (pending) {
    foot = `<div class="foot">${field(it, `Reply to ${project} (typed into its terminal)…`, "Send")}</div>`;
  } else if (s) {
    const busy = s.state === "working";
    // Working: what you send waits for its current step (⌘ Enter stops it and sends now, said on the button).
    foot = `<div class="foot">${box_(`s:${sid}`, busy ? `Message ${nameOf(sid, project)}… it reads this after its current step` : `Message ${project}…`, busy ? "Queue" : "Send", `data-act="send-to" data-sid="${esc(sid)}"${busy ? ` title="It reads this when it finishes its current step. ⌘ Enter stops it and sends now."` : ""}`)}</div>`;
  } else foot = "";
  const cm = crewOf(sid);
  const who = cm?.role === "executor" ? `${esc(cm.name)}${cm.model ? ` · ${esc(shortModel(cm.model))}` : ""}` : cm?.role === "lead" && cm.model ? `${esc(agentName(harness))} · ${esc(shortModel(cm.model))}` : esc(agentName(harness));
  // At work (a turn, or compacting): a light sweeps along the top edge, as in its terminal tab. Not while it waits.
  return `<div class="active-pane ${s?.state === "working" || s?.compacting_ms ? "busy" : ""}">${sweep(sid, s)}
    <div class="ap-head">${badge(harness)}${nameHead(sid, project)}${roleTag(sid, true)}<span class="dim">${who}</span>${pill}${ch ? `<button class="btn small commit-btn" data-act="commit" data-id="${esc(it.id)}">Commit</button>` : ""}<span class="grow"></span>
      ${sid ? moreMenu(sid, s) : ""}
      ${s && harness === "claude" ? `<button class="btn btw-btn ${btwFor === sid ? "on" : ""}" data-act="btw" data-sid="${esc(sid)}">btw</button>` : ""}
      ${s?.state === "working" ? `<button class="btn deny" data-act="interrupt" data-sid="${esc(sid)}" title="Stop it mid-turn (Esc twice)">Stop</button>` : ""}
      ${sid ? `<button class="btn" data-act="go-session" data-sid="${esc(sid)}" title="Go to its terminal tab">${lbl("Go to tab", "Tab")}</button>` : ""}
      ${sid && svClosable({ sid, st: pending && it.kind !== "waiting" ? "asks" : s?.state || "idle" }) ? closeBtn(sid) : ""}</div>
    ${btwPanel(sid)}
    ${subHead(s?.cwd || it?.cwd, "", sid)}
    ${sid ? teamBar(sid) + leadCard(sid) + packetCard(sid) : ""}
    ${sid ? `<div class="ap-bar">${barLabels(sid)}${bar(sid)}${barKey()}</div>` : ""}
    <div class="ap-chat" data-chat>${chatHtml(it, s, harness, true)}${statusLine(it, s)}</div>
    ${pending ? "" : nextBar()}
    ${foot}</div>`;
}

/** × on a finished turn: nothing to reply, take it off Waiting. Decisions don't get one (the agent is blocked on them). */
const clearX = (it) => it.kind === "waiting" ? `<button class="x-clear" data-act="dismiss" data-id="${esc(it.id)}" title="Take it off Waiting" aria-label="Take it off Waiting">×</button>` : "";
/** "Later" on a Waiting row: move the session to Need to decide. */
const parkBtn = (sid) => `<button class="park-btn" data-park="${esc(sid)}:1" title="Move it below for now">Later</button>`;
/** A row in Need to decide: the session, its finished turn if any, ↩ to put it back. Click to open it. */
function laterRow({ sid, s, it }) {
  const on = active?.sid === sid ? "on" : "";
  const what = it ? "your turn" : s.state === "working" ? "working" : "idle";
  const text = it ? plain(summary(it)) : s.prompt ? `› ${s.prompt}` : "";
  return `<div class="nrow later ${on}" ${it ? `data-big="${esc(it.id)}"` : `data-session="${esc(sid)}"`}>
    <div class="nrow-top">${badge(it?.harness || s.harness)}${nameSpan(sid, it?.project || s.project)}<span class="dim">${what}</span><span class="grow"></span><span class="age">${ago(it?.created_ms ?? s.since_ms)}</span>${starBtn(sid)}<button class="park-btn back" data-park="${esc(sid)}:0">↩ Waiting</button></div>
    ${cardCrew(sid)}
    ${text ? `<div class="nrow-text">${esc(text)}</div>` : ""}</div>`;
}
/** A compact row in Waiting: enough to recognise it, quick answers for the easy ones, click to open in Active. */
function needRow(it, ghost, open = false) {
  if (ghost) {
    const h = state.history.find((x) => x.id === it.id);
    return `<div class="nrow ghost ${ghost === "fresh" ? "fresh" : ""}" data-detail="${esc(it.id)}"><div class="nrow-top">${badge(it.harness)}${nameSpan(it.session_id, it.project)}</div>
      <div class="nrow-done ${h ? outcomeClass(h) : ""}">✓ ${esc(h ? outcomeText(h) : cleared.has(it.id) ? "cleared" : "picked up in the terminal")}</div></div>`;
  }
  const q = it.kind === "question" ? questions(it) : [];
  let quick = "";
  if (it.kind === "permission") quick = `<div class="nrow-acts"><button class="btn deny" data-act="deny" data-id="${esc(it.id)}">Deny</button><button class="btn primary" data-act="allow" data-id="${esc(it.id)}">Allow</button></div>`;
  else if (inTerminal(it)) quick = `<div class="nrow-acts"><button class="btn primary" data-act="go" data-id="${esc(it.id)}">Go to tab</button></div>`;
  else if (q.length === 1 && !q[0].multiSelect && (q[0].options || []).length <= 4) quick = `<div class="nrow-acts wrap">${q[0].options.map((o, oi) => `<button class="btn" data-pick="0:${oi}" data-id="${esc(it.id)}">${esc(o.label)}</button>`).join("")}</div>`;
  // An interrupted turn (Esc) waits at "What should Claude do instead?": say so, and offer Continue.
  if (it.interrupted) quick = `<div class="nrow-acts"><button class="btn primary" data-act="continue" data-id="${esc(it.id)}">Continue</button></div>`;
  return `<div class="nrow ${open ? "on" : ""} ${it.kind === "waiting" ? "turn" : "ask"}" data-big="${esc(it.id)}">
    <div class="nrow-top">${badge(it.harness)}${nameSpan(it.session_id, it.project)}<span class="dim ${it.interrupted ? "intr" : ""}">${it.kind === "waiting" ? (it.interrupted ? "interrupted" : "your turn") : esc(verb(it))}</span><span class="grow"></span><span class="age">${ago(it.created_ms)}</span>${starBtn(it.session_id)}${it.kind === "waiting" ? parkBtn(it.session_id) : ""}${clearX(it)}</div>
    ${cardCrew(it.session_id)}
    <div class="nrow-text ${isBash(it) ? "mono" : ""}">${esc(plain(summary(it)))}</div>${quick}${crewOf(it.session_id)?.role === "lead" ? teamTree(it.session_id) : ""}</div>`;
}
/** A lead whose team needs you while it has nothing waiting itself: a slim card holding the team's tree.
 *  Its title line opens the lead; a row in the tree opens that executor. */
function teamHead(lead, open) {
  const s = sessionOf(lead);
  return `<div class="nrow ask team-head ${open ? "on" : ""}"><div class="nrow-top" data-session="${esc(lead)}" role="button">${badge(s.harness)}${nameSpan(lead, s.project)}<span class="dim">its team needs you</span></div>
    ${cardCrew(lead)}${teamTree(lead)}</div>`;
}

/** Why a session is idle (shown under Sessions → Idle); "" when it's working. */
// ---------- usage limits ----------
/** A limit that has lifted (its reset time passed, or another turn went through). */
const lifted = (l) => !!l?.resets_ms && l.resets_ms <= now();
const limitedSessions = () => state.sessions.filter((s) => s.state === "limited" && s.limit);
/** "4:10am" today (or within the next day), "Sat 8am" further out. */
function clockAt(ms) {
  const d = new Date(ms), h = d.getHours() % 12 || 12, m = d.getMinutes(), ap = d.getHours() < 12 ? "am" : "pm";
  const t = m ? `${h}:${String(m).padStart(2, "0")}${ap}` : `${h}${ap}`;
  return ms - now() < 20 * 3600000 ? t : `${d.toLocaleDateString(undefined, { weekday: "short" })} ${t}`;
}
const untilText = (ms) => { const m = Math.round(ms / 60000); return m < 1 ? "under a minute" : m < 60 ? `${m}m` : `${Math.floor(m / 60)}h ${m % 60}m`; };
const scopeName = (l) => l.scope === "all" ? "Claude Code" : l.scope;
function limitNote(s) {
  const l = s.limit;
  if (!l) return "out of usage";
  if (lifted(l)) return "usage is back · resend";
  if (l.scope !== "all") return `${l.scope} used up${l.resets_ms ? ` until ${clockAt(l.resets_ms)}` : ""}`;
  return `out of usage${l.resets_ms ? ` · resets ${clockAt(l.resets_ms)}` : ""}`;
}
/** Resend what each paused session didn't get to run. */
async function resend(sids) {
  let ok = 0;
  for (const sid of sids) {
    const s = sessionOf(sid);
    if (!s?.prompt) continue;
    try { await invoke("send_to_session", { sessionId: sid, text: s.prompt, images: [], now: false }); ok++; }
    catch (e) { toast(`Couldn't resend to ${s.project}: ${e}`); }
  }
  if (ok) toast(ok === 1 ? "Resent" : `Resent to ${ok} sessions`);
}
let usageOpen = false;
/** Next to Board / History: the plan's usage, or loudly, that it's used up. */
function usageChip() {
  const limited = limitedSessions();
  const out = limited.filter((s) => s.limit.scope === "all" && !lifted(s.limit));
  const back = limited.filter((s) => lifted(s.limit) && now() - s.limit.resets_ms < 5 * 60000);
  const model = limited.filter((s) => s.limit.scope !== "all" && !lifted(s.limit));
  const paused = (n) => `${n} paused`;
  if (out.length) {
    const r = Math.max(...out.map((s) => s.limit.resets_ms || 0));
    return `<span class="usage-wrap"><button class="usage-out" data-usage="out" title="Click to step through the paused sessions"><span class="uo-dot"></span>Claude Code out of usage<span class="uo-sub">${r ? ` · resets ${clockAt(r)}` : ""} · ${paused(out.length)}</span></button>${usagePop()}</span>`;
  }
  if (back.length) return `<span class="usage-wrap"><span class="usage-back"><span class="ub-dot"></span>${esc(scopeName(back[0].limit))} is back<button class="ub-btn" data-usage="resend" title="Send each paused session the message that didn't run">Resend ${back.length}</button></span></span>`;
  if (model.length) {
    const l = model[0].limit;
    return `<span class="usage-wrap"><button class="usage-chip soft-warn" data-usage="model" title="${esc(l.text)} Other models still work."><span class="uf-dot"></span>${esc(l.scope)} used up<span class="uo-sub">${l.resets_ms ? ` · ${clockAt(l.resets_ms)}` : ""} · ${paused(model.length)}</span></button>${usagePop()}</span>`;
  }
  const u = state.usage;
  const uf = state.usage_file || {};
  // Settings → Usage can hide Claude Code's limits (e.g. behind a proxy, where only your file matters).
  const claude = state.settings?.usage?.claude !== false && u && (u.five_hour || u.seven_day);
  if (!claude && !(uf.meters || []).length && !uf.error) return "";
  const meter = (label, w) => w ? `<span class="ul">${label}</span><span class="um ${w.pct >= 100 ? "full" : w.pct >= 85 ? "high" : ""}"><i style="width:${Math.min(100, w.pct)}%"></i></span><span class="up">${Math.round(w.pct)}%</span>` : "";
  // Your own meters (Settings → Usage file) sit beside Claude Code's, drawn the same way.
  const mine = (uf.meters || []).map((m) => {
    const pct = m.limit ? (m.spent / m.limit) * 100 : null;
    return `${m.label ? `<span class="ul">${esc(m.label.length > 12 ? m.label.slice(0, 11) + "…" : m.label)}</span>` : ""}${pct === null ? "" : `<span class="um ${pct >= 100 ? "full" : pct >= 85 ? "high" : ""}"><i style="width:${Math.min(100, pct)}%"></i></span>`}<span class="up">${esc(spendText(m))}</span>`;
  }).join("");
  const broken = uf.error ? `<span class="ul uf-bad" title="${esc(uf.error)}">usage file ⚠</span>` : "";
  return `<span class="usage-wrap"><button class="usage-chip" data-usage="meter">${claude ? meter("5h", u.five_hour) + meter("wk", u.seven_day) : ""}${mine}${broken}</button>${usagePop()}</span>`;
}
/** "$12.40/$50", "48/200 credits", "$3.10": an amount from your usage file, in its unit. */
function spendText(m, long = false) {
  const n = (x) => Number(x).toLocaleString(undefined, { maximumFractionDigits: 2, minimumFractionDigits: Number.isInteger(x) ? 0 : 2 });
  const sym = m.unit.length <= 2 && !/[a-z]/i.test(m.unit);
  const amt = (x) => (sym ? `${m.unit}${n(x)}` : n(x));
  const both = m.limit ? `${amt(m.spent)}${long ? " of " : "/"}${amt(m.limit)}` : amt(m.spent);
  return sym ? both : `${both} ${m.unit}`;
}
function usagePop() {
  const u = state.usage;
  const uf = state.usage_file || {};
  if (!usageOpen) return "";
  // Your usage file: its meters with their details, or what's wrong with it.
  const fileSec = !(uf.meters || []).length && !uf.error ? "" : `<div class="up-group"><div class="up-title">${uf.error ? "Usage file" : "Your usage"}<span> · ${esc(uf.path || "")}${uf.as_of_ms ? `, as of ${clockAt(uf.as_of_ms)}` : ""}</span></div>
    ${uf.error ? `<div class="up-err">${esc(uf.error)}</div>` : uf.meters.map((m) => {
      const pct = m.limit ? (m.spent / m.limit) * 100 : null;
      const lvl = pct === null ? "" : pct >= 100 ? "full" : pct >= 85 ? "high" : "";
      return `<div class="up-row money ${lvl}"><span class="up-name">${esc(m.label || "Usage")}</span>${pct === null ? `<span class="up-bar none"></span>` : `<span class="up-bar"><i style="width:${Math.min(100, pct)}%"></i></span>`}<span class="up-pct">${esc(spendText(m, true))}</span>${m.resets_ms ? `<span class="up-note">resets ${clockAt(m.resets_ms)}</span>` : ""}</div>`;
    }).join("")}</div>`;
  const claude = state.settings?.usage?.claude !== false && u && (u.five_hour || u.seven_day);
  if (!claude) return `<div class="usage-pop"><div class="up-head">Usage</div>${fileSec}</div>`;
  const lvl = (w) => (w.pct >= 100 ? "full" : w.pct >= 85 ? "high" : "");
  const row = (name, w, note = "") => w ? `<div class="up-row ${lvl(w)}"><span class="up-name">${esc(name)}</span><span class="up-bar"><i style="width:${Math.min(100, w.pct)}%"></i></span><span class="up-pct">${Math.round(w.pct)}%</span>${note ? `<span class="up-note">${esc(note)}</span>` : ""}</div>` : "";
  // Rows that reset together sit under one heading that says when, instead of repeating it per row.
  const group = (title, w, rows) => rows ? `<div class="up-group"><div class="up-title">${title}${w?.resets_ms ? `<span> · resets ${clockAt(w.resets_ms)}</span>` : ""}</div>${rows}</div>` : "";
  // Per-model rows come from Claude Code's /usage panel, which only refreshes while it's open.
  const models = (u.models || []).map((m) => {
    const notes = [m.resets_ms && Math.abs(m.resets_ms - (u.seven_day?.resets_ms || 0)) > 3600e3 ? `resets ${clockAt(m.resets_ms)}` : "", now() - m.fetched_ms > 15 * 60000 ? `${ago(m.fetched_ms)} old` : ""];
    return row(m.name, m, notes.filter(Boolean).join(" · "));
  }).join("");
  return `<div class="usage-pop"><div class="up-head">Claude Code usage<span class="dim">${ago(u.at_ms)} ago</span></div>
    ${group("Session", u.five_hour, row("5 hours", u.five_hour))}${group("Weekly", u.seven_day, row("All models", u.seven_day) + models)}
    ${fileSec}<div class="up-foot">Claude Code's limits don't count Codex and Pi.</div></div>`;
}
const idleNote = (s) => s.state === "working" ? "" : s.state === "limited" ? limitNote(s) : s.state === "agent" ? `waiting on ${s.driven_by || "another agent"}` : s.state === "stopped" ? "stopped by you" : "cleared from Waiting";

// The right column: Sessions takes what it needs (up to half), Recently answered scrolls in the rest.
const RECENT = 30;
function boardView() {
  if (!quietOpen) current();   // settles what Active shows before the lists mark it
  const { working, later } = groups();
  const needs = needsYou();
  // What the Active pane shows stays where it is in its list, lit up (lime): opening
  // something never moves a row, so the lists don't shift under your pointer.
  const openSid = quietOpen || active?.sid || null;
  const isOpen = (sid) => !!openSid && sid === openSid;
  const laterSec = later.length ? `<div class="col-sub later-head">NEED TO DECIDE · ${later.length}</div>${later.map(laterRow).join("")}` : "";
  const live = new Set(needs.map((i) => i.id));
  // A ghost fades in once: every later render (they come often while sessions work) draws it still.
  const ghostRows = [...ghosts.values()].filter((g) => !live.has(g.it.id)).map((g) => { const fresh = !g.shown; g.shown = true; return { ...g.it, _ghost: fresh ? "fresh" : "shown" }; });
  // A team is one card: an executor's card goes into its lead's tree (when Cue has the lead). A lead with
  // nothing of its own waiting still gets a card, where its executor's would have been.
  const leadOf = (i) => { const m = crewOf(i.session_id); return m?.role === "executor" && sessionOf(m.lead) ? m.lead : null; };
  const teamLeads = new Set(needs.map(leadOf).filter(Boolean));
  const heads = [...teamLeads].filter((l) => !needs.some((i) => i.session_id === l))
    .map((l) => ({ _team: l, created_ms: Math.min(...needs.filter((i) => leadOf(i) === l).map((i) => i.created_ms)) }));
  // Oldest first.
  const needRows = [...needs.filter((i) => !leadOf(i)), ...heads, ...ghostRows].sort((a, b) => a.created_ms - b.created_ms)
    .map((i) => (i._team ? teamHead(i._team, isOpen(i._team)) : needRow(i, i._ghost, isOpen(i.session_id)))).join("");

  const stateNote = (s) => { const i = state.items.find((x) => x.session_id === s.session_id); return i ? (i.kind === "waiting" ? "your turn" : "asks you") : idleNote(s); };
  const card = (s, open = false) => `<div class="working click ${idleNote(s) ? "on-agent" : ""} ${open ? "on" : ""}" data-session="${esc(s.session_id)}">
      <div class="card-head">${s.state === "working" || s.compacting_ms ? `<span class="dot-live" title="working"></span>` : ""}${nameSpan(s.session_id, s.project)}<span>${esc(agentName(s.harness))}</span><span class="grow"></span><span class="age" style="color:inherit">${idleNote(s) && s.state !== "limited" && !state.items.some((x) => x.session_id === s.session_id) ? "idle " : ""}${ago(s.since_ms)}</span>${starBtn(s.session_id)}${isStarred(s.session_id) ? "" : hideX(s)}</div>
      ${stateNote(s) ? `<div class="agent-note">${esc(stateNote(s))}</div>` : ""}
      ${s.queued ? `<div class="queued-note">Queued: “${esc(s.queued.text.length > 80 ? s.queued.text.slice(0, 80) + "…" : s.queued.text)}”</div>` : ""}
      ${bar(s.session_id)}${s.trust_ms ? `<div class="doing">Asking you to trust its folder</div>` : s.compacting_ms ? `<div class="doing">Compacting…</div>` : s.state === "working" && s.doing ? `<div class="doing" data-cut title="${esc(s.doing)}">${esc(s.doing)}</div>` : ""}${s.prompt ? `<div class="prompt" data-cut title="${esc(s.prompt)}">› ${esc(s.prompt)}</div>` : ""}</div>`;
  const busy = working.filter((s) => !idleNote(s)), idle = working.filter((s) => idleNote(s) && s.state !== "limited"), outs = working.filter((s) => s.state === "limited");
  const workCol = (busy.length ? `<div class="col-sub">WORKING · ${busy.length}</div>${busy.map((s) => card(s, isOpen(s.session_id))).join("")}` : "")
    + (outs.length ? `<div class="col-sub lim">${outs.every((s) => lifted(s.limit)) ? "READY TO RESEND" : "OUT OF USAGE"} · ${outs.length}</div>${outs.map((s) => card(s, isOpen(s.session_id))).join("")}` : "")
    + (idle.length ? `<div class="col-sub">IDLE · ${idle.length}</div>${idle.map((s) => card(s, isOpen(s.session_id))).join("")}` : "")
    || `<div class="quiet-line">No other sessions.</div>`;

  // One entry per session, its latest answer (the rest are in History).
  const seen = new Set();
  const recent = state.history.filter((i) => { const k = i.session_id || i.id; return !seen.has(k) && seen.add(k); }).slice(0, RECENT);
  const recentList = () => recent.length
    ? `<div class="card recent">${recent.map(recentEntry).join("")}
        ${state.history.length > recent.length ? `<button class="link" style="padding:4px 12px 10px" data-act="open-history">All history →</button>` : ""}</div>`
    : `<div class="empty-col">Your answers show up here.</div>`;
  // Starred, in the order you starred them (so they don't shuffle as they work).
  const stars = state.sessions.filter((s) => isStarred(s.session_id)).sort((a, b) => (a.starred_ms || Infinity) - (b.starred_ms || Infinity));
  const starNeeds = stars.filter((s) => state.items.some((i) => i.session_id === s.session_id)).length;
  const drawer = (name, title, count, body) => { const open = drawerOpen(name); return `<div class="sec sec-drawer sec-${name} ${open ? "" : "shut"}"><button class="col-head fold-head" data-fold="${name}" aria-expanded="${open}"><span class="fold-arrow">${open ? "▾" : "▸"}</span>${title} <span>${count}</span></button>${open ? `<div class="sec-body">${body()}</div>` : ""}</div>`; };
  // Fixed layout: every column stays where it is (nothing jumps as the queue changes).
  return `<div class="board" style="grid-template-columns:minmax(0,1.9fr) minmax(0,1fr) minmax(0,0.85fr)">
    <div class="col main"><div class="col-head">ACTIVE</div>${activePane()}</div>
    <div class="col"><div class="col-head">WAITING <span>${needs.length}${needs.some((i) => i.kind !== "waiting") ? ` · ${needs.filter((i) => i.kind !== "waiting").length} asking` : ""}${needs.length > 1 ? " · oldest first" : ""}</span></div>${needRows || `<div class="quiet-line">Nothing waiting.</div>`}${laterSec}</div>
    <div class="col split ${DRAWERS.some(drawerOpen) ? "" : "drawers-shut"}">
      <div class="sec sec-sessions"><div class="col-head">SESSIONS <span>${working.length}</span></div><div class="sec-body">${workCol}</div></div>
      ${drawer("starred", "STARRED", stars.length ? `${stars.length}${starNeeds ? ` · ${starNeeds} need${starNeeds === 1 ? "s" : ""} you` : ""}` : "", () => stars.length ? `<div class="card recent">${stars.map((s) => starEntry(s, isOpen(s.session_id))).join("")}</div>` : `<div class="quiet-line">Star a session (★ on its card) to keep it here.</div>`)}
      ${drawer("recent", "RECENTLY ANSWERED", recent.length ? `${recent.length} session${recent.length === 1 ? "" : "s"}` : "", recentList)}
    </div>
  </div>`;
}

// ---------- sheets ----------
/** The Active pane's title: the session's name with ✎ to rename it (an input while you do), and its
 *  project beside it once it has a name of its own. */
/** Under the Active header: an executor's lead on the left, the session's folder (~ for your home)
 *  on the right, under the buttons. One small line. */
function subHead(cwd, crew, sid, titleOf = sid) {
  // The star sits after the folder: the title row is full.
  // Too long: cut in the middle (~/develop…/web-app), so the folder's own name always shows.
  const path = homeless(cwd), cutAt = Math.max(path.lastIndexOf("/"), 0);
  const folder = cwd ? `<span class="ap-cwd sel" data-cut title="${esc(cwd)}"><span class="cwd-head">${esc(path.slice(0, cutAt))}</span><span class="cwd-tail">${esc(path.slice(cutAt))}</span></span>${sid ? starBtn(sid) : ""}` : "";
  // Under the name: what Claude Code titled the conversation (unless that's already the name).
  const t = titleOf ? state.about?.[titleOf]?.title || "" : "";
  const title = t && t !== nameOf(titleOf, "") ? `<span class="ap-title" data-cut title="${esc(t)}">${esc(t)}</span>` : "";
  const meta = titleOf ? metaInline(titleOf) : "";
  return folder || crew || title || meta ? `<div class="ap-sub">${title}${meta}${crew}${folder}</div>` : "";
}
function nameHead(sid, project) {
  if (sid && renaming === sid) return `<input class="rename-in" data-text="rename:${esc(sid)}" maxlength="60" placeholder="Name this session" spellcheck="false" value="${esc(draft(`rename:${sid}`).text)}"/>`;
  const name = sid ? nameOf(sid, project) : project;
  // No hover here: the chat shows Claude Code's title as text under the name (subHead).
  return `<span class="proj">${esc(name)}</span>${sid ? `<button class="rename-btn" data-rename="${esc(sid)}" title="Rename this session" aria-label="Rename">✎</button>` : ""}${name !== project ? `<span class="dim">${esc(project)}</span>` : ""}`;
}
async function submitRename(sid) {
  const key = `rename:${sid}`, name = draft(key).text.trim();
  renaming = null;
  delete drafts[key];
  renderMain();
  try { toast(await invoke("rename_session", { sessionId: sid, name })); }
  catch (e) { toast(`Couldn't rename: ${e}`); }
}
/** Search: every session Cue has seen, ended ones too (session names, your messages, the agents'
 *  replies), newest first. A live session opens in Active; an ended one's message opens in place. */
let searchResults = [], searchSeq = 0, searchTimer = 0, searchSel = 0;
let searchVisible = [];        // result indexes in screen order (↓ / ↑ walk these)
const searchMore = new Set();  // sessions whose every match you asked to see
function runSearch(q) {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(async () => {
    const n = ++searchSeq;
    const r = q.trim() ? await invoke("search", { q }).catch(() => []) : [];
    if (n !== searchSeq) return;   // a newer search already started
    // Sessions first, then history grouped by session (the session with the newest match first):
    // the order they're shown in.
    const all = Array.isArray(r) ? r : [], msgs = all.filter((x) => x.kind === "message");
    const order = [...new Set(msgs.map((m) => m.session_id))];
    const sessions = all.filter((x) => x.kind === "session");
    searchResults = [...sessions.filter((x) => sessionOf(x.session_id)), ...sessions.filter((x) => !sessionOf(x.session_id)),
      ...all.filter((x) => x.kind === "older"), ...order.flatMap((sid) => msgs.filter((m) => m.session_id === sid))];
    searchMore.clear();
    searchSel = 0;
    renderMain();
  }, 140);
}
function openSearch() {
  sheet = "search";
  renderMain();
  const el = document.querySelector(".search-in");
  el?.focus(); el?.select();
}
function searchSheet() {
  const q = draft("search").text.trim();
  const mark = (html) => q ? html.replace(new RegExp(esc(q).replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "gi"), (m) => `<mark>${m}</mark>`) : html;
  searchVisible = [];
  const row = (r, i, inGroup = false) => {
    searchVisible.push(i);
    const top = inGroup ? `<span class="dim">${r.role === "you" ? "you said" : "agent"}</span><span class="grow"></span><span class="age">${ago(r.at_ms)}</span>` : `${badge(r.harness)}<span class="proj">${esc(r.name || r.project || "session")}</span>${r.name && r.project && r.name !== r.project ? `<span class="dim">${esc(r.project)}</span>` : ""}
      <span class="dim">${r.kind === "older" ? "" : r.kind === "session" ? sessionState(r) : r.role === "you" ? "you said" : "agent"}</span>${r.kind === "older" ? `<span class="sr-resume">Resume</span>` : ""}${r.ended && r.kind === "message" ? `<span class="sr-ended">ended</span>` : ""}<span class="grow"></span><span class="age">${ago(r.at_ms)}</span>`;
    const body = r.kind === "message" ? `<div class="sr-snip">${mark(esc(r.snippet))}</div>`
      : r.last ? `<div class="sr-snip sr-last">${r.last_role === "you" ? "<b>You:</b> " : ""}${mark(esc(r.last))}</div>` : "";
    // Under its session's name, one line is enough: who, what, when.
    if (inGroup) return `<button class="sr in-group ${searchSel === i ? "sel" : ""}" data-sr="${i}"><span class="sr-who">${r.role === "you" ? "you" : "agent"}</span><span class="sr-line">${mark(esc(r.snippet))}</span><span class="age">${ago(r.at_ms)}</span></button>`;
    return `<button class="sr ${searchSel === i ? "sel" : ""}" data-sr="${i}"><div class="sr-top">${top}</div>${body}</button>`;
  };
  // Sections, so it's clear what each result is: running now, ended (Cue saw it), older (from Claude
  // Code's own archive, before Cue: Resume opens it again), and what was said.
  const section = (title, note, keep) => {
    const rows = searchResults.map((r, i) => [r, i]).filter(([r]) => keep(r));
    return rows.length ? `<div class="sr-sec">${title.toUpperCase()} <span>${rows.length}</span><em>${note}</em></div>${rows.map(([r, i]) => row(r, i)).join("")}` : "";
  };
  // History, grouped by session: its name once, then its matches (the 3 newest, until you ask for more).
  const SHOWN = 3;
  const history = () => {
    const groups = [];
    searchResults.forEach((r, i) => {
      if (r.kind !== "message") return;
      let g = groups.find((x) => x.sid === r.session_id);
      if (!g) groups.push((g = { sid: r.session_id, r, rows: [] }));
      g.rows.push([r, i]);
    });
    if (!groups.length) return "";
    const total = groups.reduce((n, g) => n + g.rows.length, 0);
    return `<div class="sr-sec">HISTORY <span>${total}</span><em>messages and answers</em></div>` + groups.map((g) => {
      const all = searchMore.has(g.sid) || g.rows.length <= SHOWN + 1, name = esc(g.r.name || g.r.project || "session");
      const head = `<div class="sr-group">${badge(g.r.harness)}<span class="proj">${name}</span>${g.r.ended ? `<span class="sr-ended">ended</span>` : ""}<span class="dim">${g.rows.length} ${g.rows.length === 1 ? "match" : "matches"}</span></div>`;
      return head + (all ? g.rows : g.rows.slice(0, SHOWN)).map(([r, i]) => row(r, i, true)).join("")
        + (all ? "" : `<button class="sr-more" data-srmore="${esc(g.sid)}">+ ${g.rows.length - SHOWN} more in ${name}</button>`);
    }).join("");
  };
  const list = !q ? `<div class="dim sr-hint">Live and closed sessions, older Claude Code sessions by name (Resume them), and everything said in sessions Cue has seen.</div>`
    : searchResults.length
      ? section("Live", "running now", (r) => r.kind === "session" && sessionOf(r.session_id))
        + section("Closed", "ended; Cue has the conversation", (r) => r.kind === "session" && !sessionOf(r.session_id))
        + section("Older", "from Claude Code, before Cue; matched by name", (r) => r.kind === "older")
        + history()
      : `<div class="dim sr-hint">Nothing found for “${esc(q)}”.</div>`;
  return `<div class="sheet search-sheet"><div class="search-bar">${SEARCH_ICON}<input class="search-in" data-text="search" placeholder="Search sessions (live, closed, older) and messages…" spellcheck="false" autocomplete="off" value="${esc(draft("search").text)}"/><kbd>Esc</kbd></div>
    <div class="sheet-body">${list}</div></div>`;
}
/** ↓ / ↑ in the search box: move the highlight (Enter opens it), keeping it in view. */
function stepSearch(by) {
  if (!searchVisible.length) return;
  const at = Math.max(0, searchVisible.indexOf(searchSel));
  searchSel = searchVisible[(at + by + searchVisible.length) % searchVisible.length];
  renderMain();
  document.querySelector(`[data-sr="${searchSel}"]`)?.scrollIntoView({ block: "nearest" });
}
/** What a session result is up to: live state, or ended. */
function sessionState(r) {
  const s = sessionOf(r.session_id);
  if (!s) return "";   // ended: its section already says so
  if (state.items.some((i) => i.session_id === r.session_id)) return "your turn";
  return s.state === "working" ? "working" : "idle";
}
/** A result: a live session opens in Active; a message (or an ended session) opens its whole
 *  conversation, scrolled to that message. */
function pickResult(i) {
  const r = searchResults[i];
  if (!r) return;
  if (r.kind === "session" && sessionOf(r.session_id)) { sheet = null; return setActive(null, r.session_id); }
  if (r.kind === "older") return resumeOlder(r);
  openConvo(r, r.kind === "message" ? r.at_ms : null);
}
/** An older Claude Code session: `claude --resume` in a terminal tab behind Cue, and the session
 *  opens here in Active (with its last few messages), ready to type into. */
async function resumeOlder(r) {
  sheet = null;
  renderMain();
  try {
    const res = await invoke("resume_session", { sessionId: r.session_id });
    toast(`${res.detail}: ${r.name || r.project}`);
    setActive(null, res.session_id);
  } catch (e) { toast(`Couldn't resume it: ${e}`); }
}
let convo = null, convoScrolled = false;  // the conversation opened from search, and whether it's been scrolled to its hit
async function openConvo(r, hitAt) {
  const items = await invoke("session_log", { sessionId: r.session_id }).catch(() => []);
  convo = { sid: r.session_id, hitAt, name: r.name || r.project, harness: r.harness, items: Array.isArray(items) ? items : [] };
  sheet = "convo";
  convoScrolled = false;
  renderMain();
}
/** A session's whole conversation (from Cue's log), read-only, with the message you found in lime. */
function convoSheet() {
  const c = convo, live = sessionOf(c.sid);
  const when = (ms) => new Date(ms).toLocaleString([], { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
  const msgs = c.items.map((e) => {
    const hit = e.at_ms === c.hitAt ? " cv-hit" : "";
    if (e.role === "you") return `<div class="cv-you${hit}"><div class="cv-you-text">${linkify(esc(e.text)).replace(/\n/g, "<br>")}</div>${thumbs(e.images)}<div class="cv-meta">You · ${when(e.at_ms)}</div></div>`;
    return `<div class="cv-agent${hit}"><div class="cv-meta">${esc(e.role === "peer" ? "Another session" : agentName(c.harness))} · ${when(e.at_ms)}</div><div class="msg cv-text">${md(e.text)}</div></div>`;
  }).join("");
  return `<div class="sheet convo-sheet"><div class="sheet-head"><button class="btn" data-act="convo-back">← Results</button>${badge(c.harness)}<h3>${esc(c.name)}</h3>
      <span class="dim" style="font-size:12px">${live ? (live.state === "working" ? "working" : "live") : "ended"} · ${c.items.length} messages</span><span class="grow"></span>
      ${live ? `<button class="btn primary" data-act="convo-open" data-sid="${esc(c.sid)}">Open in Active</button>` : ""}<kbd>Esc</kbd></div>
    <div class="sheet-body convo-body">${msgs || `<div class="dim">Nothing logged for this session.</div>`}</div></div>`;
}
/** "Send to another session": a small menu right above the button. Pick a session, add a note if you like. */
function forwardPop() {
  const f = forward;
  const card = new Map(state.items.map((i) => [i.session_id, i]));
  const label = (x) => card.has(x.session_id) ? (card.get(x.session_id).kind === "waiting" ? "your turn" : "asks you") : idleNote(x) ? "idle" : "working";
  const opts = state.sessions.filter((x) => x.session_id !== f.fromSid).map((x) => `<button class="fwd-opt ${f.to === x.session_id ? "on" : ""}" data-fwd-to="${esc(x.session_id)}">
      ${badge(x.harness)}<span class="proj">${esc(x.name || x.project)}</span><span class="grow"></span><span class="fwd-st">${esc(label(x))}</span></button>`).join("");
  const to = sessionOf(f.to);
  return `<div class="fwd-catch" data-act="cancel-forward"></div><div class="fwd-pop" role="dialog" aria-label="Send to another session">
    <div class="fwd-head">SEND THIS MESSAGE TO</div>
    <div class="fwd-list">${opts || `<div class="dim" style="padding:6px 8px">No other sessions are open.</div>`}</div>
    <textarea rows="2" class="fwd-note" spellcheck="false" data-text="fwd" placeholder="Add a note (optional)">${esc(draft("fwd").text)}</textarea>
    <div class="fwd-foot"><button class="btn" data-act="cancel-forward">Cancel</button><button class="btn primary" data-act="send-forward" ${to ? "" : "disabled"}>${to ? `Send to ${esc(to.project)}` : "Pick a session"}</button></div></div>`;
}
/** Keep the menu just above its button (below it when there's no room), following the chat as it scrolls. */
function placeForward() {
  const pop = document.querySelector(".fwd-pop");
  if (!pop) return;
  const btn = document.querySelector(`[data-fwd="${CSS.escape(forward.key)}"]`);
  if (!btn) { sheet = forward = null; return renderMain(); }
  const r = btn.getBoundingClientRect(), h = pop.offsetHeight, w = pop.offsetWidth;
  pop.style.left = `${Math.max(8, Math.min(r.left, innerWidth - w - 8))}px`;
  pop.style.top = `${r.top - h - 6 >= 8 ? r.top - h - 6 : Math.min(r.bottom + 6, innerHeight - h - 8)}px`;
}
async function sendForward() {
  const f = forward, to = sessionOf(f?.to);
  if (!to) return;
  if (to.state === "deciding") return toast(`${to.project} is asking you something first. Answer that, then send.`);
  const note = draft("fwd").text.trim();
  const text = `${note ? `${note}\n\n` : ""}[Forwarded from the ${f.from} session]\n${f.text}`;
  sheet = forward = null;
  delete drafts.fwd;
  renderMain();
  try { await invoke("send_to_session", { sessionId: to.session_id, text, images: [] }); toast(`Sent to ${to.project}`); }
  catch (e) { toast(`Couldn't send to ${to.project}: ${e}`); }
}
/** An opened History row: the conversation as it stood, then (for a decision) what it asked, read-only. */
function histDetail(i) {
  // A reply goes in the conversation, right under what it answered, so opening the row lands on it.
  const reply = i.kind === "waiting" && /^replied: “/.test(outcomeText(i)) ? replyOf(i) : null;
  const yours = reply ? `<div class="cv-you hreply"><div class="cv-you-text">${esc(reply).replace(/\n/g, "<br>")}</div>${thumbs(i.images)}<div class="cv-meta">You replied · ${ago(i.resolved_ms)} ago</div></div>` : "";
  const chat = (i.thread || []).length || i.kind === "waiting" ? `<div class="hdetail">${chatHtml(i, null, i.harness)}${yours}</div>` : "";
  let ask = "";
  if (i.kind === "question") ask = questions(i).map((q) => `<div class="qtext">${esc(q.question)}</div><ul class="hopts">${(q.options || []).map((o) => `<li>${esc(o.label)}${o.description ? `<span class="dim">: ${esc(o.description)}</span>` : ""}</li>`).join("")}</ul>`).join("");
  else if (i.kind !== "waiting" && !(inTerminal(i) && !i.tool_name)) ask = requestBody(i);
  return chat + (ask ? `<div class="hask"><div class="act-head">${esc(agentName(i.harness))} ${esc(verb(i))}</div>${ask}</div>` : "");
}
/** What you replied, in full: from the live session's own log when it's still there (the stored outcome
 *  keeps only the first line), else the outcome's quote. */
function replyOf(i) {
  const e = (sessionOf(i.session_id)?.thread || []).find((x) => x.role === "you" && Math.abs(x.at_ms - i.resolved_ms) < 5000);
  return e?.text || (outcomeText(i).match(/^replied: “([\s\S]*)”/) || [])[1] || "";
}
/** Today at a glance, above History: how many you answered, how long they
 *  typically waited and the longest wait, what you did, and answers per hour over the last 12 hours. */
function histStats() {
  const midnight = new Date(now()); midnight.setHours(0, 0, 0, 0);
  // Every answer of the last day (state.answers), not the History list: that one is capped (Settings → History).
  const all = state.answers || state.history;
  const today = all.filter((i) => (i.resolved_ms || 0) >= midnight.getTime());
  const waits = today.map((i) => ({ i, w: (i.resolved_ms || 0) - i.created_ms })).filter((x) => x.w >= 0).sort((a, b) => a.w - b.w);
  const fmt = (ms) => { const m = Math.round(ms / 60000); return m < 1 ? "<1m" : m < 60 ? `${m}m` : `${Math.floor(m / 60)}h ${m % 60}m`; };
  const longest = waits.at(-1);
  const n = (re) => today.filter((i) => re.test(i.outcome || "")).length;
  const hour0 = new Date(now()); hour0.setMinutes(0, 0, 0);
  const hours = Array.from({ length: 12 }, (_, k) => {
    const from = hour0.getTime() - (11 - k) * 3600000, d = new Date(from);
    return { label: `${d.getHours() % 12 || 12}${d.getHours() < 12 ? "am" : "pm"}`, n: all.filter((i) => (i.resolved_ms || 0) >= from && (i.resolved_ms || 0) < from + 3600000).length };
  });
  const peak = Math.max(1, ...hours.map((h) => h.n));
  const tile = (value, label, title = "") => `<div class="hs-tile" title="${esc(title)}"><b>${esc(value)}</b><span>${esc(label)}</span></div>`;
  return `<div class="hstats">
    ${tile(String(today.length), "answered today")}
    ${tile(waits.length ? fmt(waits[Math.floor(waits.length / 2)].w) : "–", "median wait", "From when it asked to when you answered")}
    ${tile(longest ? fmt(longest.w) : "–", longest ? `longest · ${nameOf(longest.i.session_id, longest.i.project)}` : "longest wait")}
    ${tile(`${n(/^replied/)} · ${n(/^(allowed|answered)/)} · ${n(/^denied/)}`, "replied · allowed · denied")}
    <div class="hs-chart" role="img" aria-label="Answers by hour: ${esc(hours.map((h) => `${h.label} ${h.n}`).join(", "))}">
      <div class="hs-bars">${hours.map((h) => `<i class="${h.n ? "" : "zero"}" style="height:${h.n ? Math.max(4, (h.n / peak) * 100) : 6}%" title="${h.label} · ${h.n} answered"></i>`).join("")}</div>
      <div class="hs-axis"><span>${hours[0].label}</span><span>answers by hour</span><span>now</span></div>
    </div></div>`;
}
/** History: everything you answered, newest first, by day. Click a row to read it in place. */
// ---------- "N live" in the header: every live session, one click from any screen, and + New ----------
let liveOpen = false;              // the drop-down is open
let liveSpot = false;              // …opened with ⌘K: in the middle of the window, like Spotlight
let liveSel = 0;                   // the highlighted row (↑ ↓ move it, Enter opens it)
let liveShown = [];                // the rows on screen, in order: [{ sid, act }]
let newOpen = false;               // its New session form is showing
let newAgent = "claude";
let agentsAvail = null;            // the agents installed on this Mac (asked once)
/** Folders you've worked in, most recent first: live sessions, then answered history. */
function recentFolders() {
  const seen = new Map();
  const add = (cwd, at) => { if (cwd && !(seen.get(cwd) >= at)) seen.set(cwd, at || 0); };
  for (const s of state.sessions) add(s.cwd, s.since_ms);
  for (const q of state.live || []) add(q.cwd, q.since_ms);
  for (const h of state.history || []) add(h.cwd, h.created_ms);
  return [...seen.entries()].sort((a, b) => b[1] - a[1]).map(([cwd]) => cwd).slice(0, 12);
}
function liveChip() {
  const rows = liveRows(), asks = rows.filter((r) => r.st === "asks").length;
  return `<span class="lv-wrap"><button class="hchip lv-chip ${liveOpen ? "on" : ""}" data-lv="toggle" title="Every live session, and + New (⌘K, ⌘L)"><span class="hdot"></span>${rows.length}<span class="hlbl"> live</span>${asks ? ` <span class="hsub">· ${asks} asks</span>` : ""}</button>${liveOpen && !liveSpot ? livePop(rows) : ""}</span>`;
}
// ---------- ★ in the header: the sessions you starred, and which of them need you ----------
let starOpen = false;              // its list is open
let starNeeded = null;             // starred sessions that needed you at the last draw
let starLitTill = 0;               // lit up until then: one of them just started needing you
function starChip() {
  const stars = starredIds();
  const rows = liveRows().filter((r) => stars.has(r.sid));
  if (!rows.length) { starOpen = false; return ""; }
  const needs = rows.filter((r) => r.st === "asks" || r.st === "yours");
  // It lights up when one newly needs you, then goes quiet: lit all the time, you'd stop seeing it.
  if (starNeeded && needs.some((r) => !starNeeded.has(r.sid))) { starLitTill = now() + 6000; setTimeout(renderMain, 6100); }
  starNeeded = new Set(needs.map((r) => r.sid));
  const lit = now() < starLitTill;
  return `<span class="st-wrap"><button class="hchip st-chip ${starOpen ? "on" : ""} ${lit ? "hot" : ""}" data-star-chip title="Sessions you starred">${STAR_ICON}${rows.length}<span class="hlbl"> starred</span>${needs.length ? `<span class="hsub"> · ${needs.length} need${needs.length === 1 ? "s" : ""} you</span>` : ""}</button>${starOpen ? starPop(rows) : ""}</span>`;
}
/** The starred sessions: the ones that need you first. Click one to open it; ★ unstars it. */
function starPop(rows) {
  const sorted = [...rows].sort((a, b) => svRank(a) - svRank(b) || a.since - b.since);
  // Live: under its name, what it's doing now (or asking, or last said), redrawn as it changes; a permission
  // it asks for can be answered right here.
  const row = (r) => {
    const what = r.what || (r.it ? plain(summary(r.it)) : "");   // a finished turn: its message
    const ask = r.st === "asks" && r.it?.kind === "permission" ? `<span class="st-acts"><button class="btn deny" data-act="deny" data-id="${esc(r.it.id)}">Deny</button><button class="btn primary" data-act="allow" data-id="${esc(r.it.id)}">Allow</button></span>` : "";
    return `<div class="lv-row st-row ${r.sid === (quietOpen || active?.sid) ? "on" : ""}" role="button" data-sv="${r.quiet ? "quiet" : "open"}" data-sid="${esc(r.sid)}">
      <div class="st-line">${badge(r.harness)}<span class="lv-name">${esc(r.name)}</span>${svChipState(r)}<span class="age">${ago(r.since)}</span><span class="lv-acts">${starBtn(r.sid)}</span></div>
      ${what || ask ? `<div class="st-now ${r.st === "asks" ? "hot" : ""}">${r.run ? `<span class="dot-live"></span>` : ""}<span class="st-what ${r.st === "asks" && r.it && isBash(r.it) ? "mono" : ""}">${esc(what)}</span>${ask}</div>` : ""}</div>`;
  };
  return `<div class="lv-pop st-pop"><div class="st-head">Starred<span>${rows.length}</span></div><div class="lv-list">${sorted.map(row).join("")}</div><div class="lv-foot"><span class="lv-keys">Star a session on its card to follow it</span></div></div>`;
}
function livePop(rows) {
  const q = draft("find-live").text.trim().toLowerCase();
  const hay = (r) => { const ab = svAbout(r); return [r.name, r.cwd, ab.about, ab.now, (r.crew || crewOf(r.sid))?.lead_name].join(" ").toLowerCase(); };
  const hits = rows.filter((r) => !q || q.split(/\s+/).every((w) => hay(r).includes(w)));
  const groups = new Map();
  for (const r of hits) (groups.get(r.cwd) || groups.set(r.cwd, []).get(r.cwd)).push(r);
  const row = (r) => {
    const ab = svAbout(r), unnamed = r.name === baseName(r.cwd) && (ab.about || ab.now);
    const name = unnamed ? ab.about || ab.now : r.name, tip = [ab.about, ab.now].filter(Boolean).join(" · ");
    const m = r.crew || crewOf(r.sid);
    const main = r.it ? `<button class="btn primary" data-sv="open" data-sid="${esc(r.sid)}">${r.st === "asks" ? "Answer" : "Reply"}</button>`
      : !r.quiet || r.harness === "claude" ? `<button class="btn" data-sv="${r.quiet ? "quiet" : "open"}" data-sid="${esc(r.sid)}">Open</button>` : "";
    // The whole row opens its chat in Active (a quiet session has none in Cue yet: its tab instead).
    const sel = liveShown[liveSel]?.sid === r.sid ? "sel" : "";
    const viewing = r.sid === (quietOpen || active?.sid) ? "on" : "";
    return `<div class="lv-row ${sel} ${viewing}" role="button" data-sv="${r.quiet ? "quiet" : "open"}" data-sid="${esc(r.sid)}" ${tip ? `title="${esc(tip)}"` : ""}>${badge(r.harness)}${m ? `<span class="role ${m.role}">${m.role === "lead" ? "LEAD" : "EXEC"}</span>` : ""}<span class="lv-name">${esc(name)}</span>${viewing ? VIEWING : ""}${svChipState(r)}<span class="lv-acts">${main}<button class="btn" data-sv="tab" data-sid="${esc(r.sid)}">Tab</button></span></div>`;
  };
  const ordered = [...groups.entries()]
    .map(([cwd, rs]) => [cwd, rs.sort((a, b) => svRank(a) - svRank(b) || b.since - a.since)])
    .sort((a, b) => svRank(a[1][0]) - svRank(b[1][0]));
  liveShown = ordered.flatMap(([, rs]) => rs.map((r) => ({ sid: r.sid, act: r.quiet ? "quiet" : "open" })));
  liveSel = Math.min(Math.max(liveSel, 0), Math.max(liveShown.length - 1, 0));
  const list = ordered
    .map(([cwd, rs]) => `<div class="lv-group"><div class="lv-ghead"><span>${esc(baseName(cwd) || "(no folder)")}</span><span class="sx-branch">${esc(state.branches?.[cwd] || "")}</span><button class="sx-plus" data-lv="new" data-cwd="${esc(cwd)}" title="New session in ${esc(homeless(cwd))}">+</button></div>${rs.map(row).join("")}</div>`).join("");
  return `<div class="lv-pop">
    <div class="lv-top"><label class="sv-search">${SEARCH_ICON}<input data-text="find-live" placeholder="Find a session…  ↑ ↓ Enter" spellcheck="false" autocomplete="off" value="${esc(draft("find-live").text)}"/></label><button class="btn primary" data-lv="new">+ New</button></div>
    ${newOpen ? newForm() : ""}
    <div class="lv-list">${list || `<div class="quiet-line">${q ? `No live session matches “${esc(q)}”.` : "No live sessions."}</div>`}</div>
    <div class="lv-foot"><button data-lv="all">All sessions, as tiles →</button><span class="lv-keys"><kbd>↑</kbd><kbd>↓</kbd> move · <kbd>↵</kbd> open · <kbd>${liveSpot ? "⌘K" : "⌘L"}</kbd> open / close</span></div></div>`;
}
/** + New session: agent, folder (recent ones offered), an optional first message, and (Claude) a name. */
function newForm() {
  const agents = agentsAvail || ["claude"];
  if (!agents.includes(newAgent)) newAgent = agents[0];
  const folders = recentFolders();
  return `<div class="nf">
    <div class="nf-row"><span class="nf-k">Agent</span><div class="seg">${agents.map((a) => `<button class="${newAgent === a ? "on" : ""}" data-lv="agent" data-agent="${a}">${esc(agentName(a))}</button>`).join("")}</div></div>
    <div class="nf-row"><span class="nf-k">Folder</span><input class="nf-in mono" data-text="new-cwd" list="nf-folders" placeholder="~/development/…" spellcheck="false" autocomplete="off" value="${esc(draft("new-cwd").text)}"/>
      <datalist id="nf-folders">${folders.map((f) => `<option value="${esc(homeless(f))}"></option>`).join("")}</datalist></div>
    ${newAgent === "claude" ? `<div class="nf-row"><span class="nf-k">Name</span><input class="nf-in" data-text="new-name" placeholder="optional, e.g. fix-login" spellcheck="false" autocomplete="off" value="${esc(draft("new-name").text)}"/></div>` : ""}
    <div class="nf-row top"><span class="nf-k">Message</span><textarea class="nf-in" data-text="new-msg" rows="2" placeholder="optional: what it should start on">${esc(draft("new-msg").text)}</textarea></div>
    <div class="nf-acts"><button class="btn" data-lv="cancel">Cancel</button><button class="btn primary" data-lv="start">Start ${esc(agentName(newAgent))}</button></div></div>`;
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
    if (!liveOpen) newOpen = false;
    renderMain();
    // Open: ready to type a name straight away.
    if (liveOpen) document.querySelector('[data-text="find-live"]')?.focus();
    return;
  }
  if (act === "all") { liveOpen = newOpen = false; view = "sessions"; return renderMain(); }
  if (act === "new") {
    liveOpen = newOpen = true;
    draft("new-cwd").text = homeless(d.cwd || draft("new-cwd").text || recentFolders()[0] || "");
    if (!agentsAvail) invoke("agents_installed").then((a) => { agentsAvail = a?.length ? a : ["claude"]; renderMain(); }, () => {});
    renderMain();
    return document.querySelector('[data-text="new-msg"]')?.focus();
  }
  if (act === "agent") { newAgent = d.agent; return renderMain(); }
  if (act === "cancel") { newOpen = false; return renderMain(); }
  if (act === "start") {
    const cwd = draft("new-cwd").text.trim();
    if (!cwd) return toast("Pick a folder first");
    try {
      const r = await invoke("new_session", { agent: newAgent, cwd, message: draft("new-msg").text, name: newAgent === "claude" ? draft("new-name").text : "" });
      toast(r.detail);
      draft("new-msg").text = draft("new-name").text = "";
      newOpen = liveOpen = false;
      // Cue knows the new session already (it picked the id): open it here; its terminal tab stays behind.
      if (r.session_id) return setActive(null, r.session_id);
      renderMain();
    } catch (e) { toast(`Couldn't start it: ${e}`); }
  }
}

// ---------- Sessions view: every live session as a map of project tiles ----------
const homeless = (p) => String(p || "").replace(/^\/Users\/[^/]+(?=\/|$)/, "~");
const baseName = (p) => String(p || "").split("/").filter(Boolean).pop() || "";
const lastSaid = (s) => [...(s.thread || [])].reverse().find((e) => e.role === "agent")?.text || "";
/** relay names tabs "[Exec] web-lines-cleanup", "[Lead] relay-tmux-backend": the LEAD / EXEC tag says that already. */
const bareName = (n) => String(n || "").replace(/^\[(?:ex-)?(?:Exec|Lead)\]\s*/i, "");
/** One entry per live session: the ones Cue knows, then the quiet ones (Claude's registry, Pi that only connected). */
function liveRows() {
  const rows = state.sessions.map((s) => {
    const it = state.items.find((i) => i.session_id === s.session_id && i.status === "pending");
    const st = it && it.kind !== "waiting" ? "asks" : it || s.state === "waiting" ? "yours" : s.state === "working" ? "working" : s.state;
    const what = s.trust_ms ? "Asking you to trust its folder" : s.compacting_ms && st !== "asks" ? "Compacting…" : st === "asks" ? plain(summary(it)) : st === "working" ? (s.doing || (s.prompt ? `› ${s.prompt}` : "")) : plain(firstLine(lastSaid(s)));
    return { sid: s.session_id, harness: s.harness, name: bareName(nameOf(s.session_id, s.project)), cwd: s.cwd || "", st, since: it?.created_ms ?? s.since_ms, what, run: st === "working" && !!s.doing, it, quiet: false };
  });
  for (const q of state.live || []) {
    rows.push({ sid: q.session_id, harness: q.harness, name: bareName(q.name) || baseName(q.cwd) || "session", cwd: q.cwd || "", st: q.status === "busy" ? "working" : "idle", since: q.since_ms, what: "", run: false, quiet: true });
  }
  return rows;
}
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
  if (m?.role === "executor" && reported(m.status)) return `<span class="sx-st">done, waiting for review</span>`;
  if (r.quiet && r.st !== "working") return `<span class="sx-st">quiet</span>`;
  if (r.st === "limited") return `<span class="sx-st">out of usage</span>`;
  return `<i class="sx-dot ${r.st === "working" ? "busy" : "idle"}" title="${r.st === "working" ? "working" : "idle"}"></i>`;
}
/** What a session is about, without a model: an executor's packet goal; else Claude's AI title for the
 *  session (set early, never updated); and your latest request to it (Claude's, else what Cue saw you send). */
function svAbout(r) {
  const m = r.crew || crewOf(r.sid), a = state.about?.[r.sid];
  const now = a?.prompt || sessionOf(r.sid)?.prompt || "";
  if (m?.role === "executor" && m.goal) return { label: "Goal", about: m.goal, now: "", outcome: m.outcome || "" };
  return { label: "About", about: a?.title || "", now, outcome: "" };
}
/** Close: not while it's working or asking you (you'd lose the turn / the question), not a lead that
 *  still has executors (that's relay's handoff), not a session Cue can't reach. Two clicks: Close, Close?. */
let svCloseArm = null, svCloseTimer = null;
/** Close: first click arms it ("Close?", red), a second within 4 s closes. In Sessions and the chat's header. */
function closeBtn(sid) {
  const m = crewOf(sid);
  const what = m?.role === "executor" ? `Close it through ${m.plugin === "pilead" ? "pilead" : "relay"} (marks it closed, closes its tab)` : "End it and close its tab (its conversation is kept: claude --resume brings it back)";
  return svCloseArm === sid
    ? `<button class="btn deny sx-close on" data-sv="close" data-sid="${esc(sid)}">Close?</button>`
    : `<button class="btn sx-close" data-sv="close" data-sid="${esc(sid)}" title="Close: ${esc(what)}">Close</button>`;
}
function svClosable(r) {
  const m = r.crew || crewOf(r.sid);
  if (r.ghost || r.st === "working" || r.st === "asks") return false;
  if (m?.role === "lead" && (m.executors || []).length) return false;
  return true;
}
/** What you can do with a session, right on it: answer or reply when it needs you, the relay buttons,
 *  open it in Active, or go to its terminal. A quiet Claude session opens too (Cue takes it in); other quiet ones only have the tab. */
function svActs(r) {
  const m = r.crew || crewOf(r.sid), acts = [];
  if (r.it) acts.push(`<button class="btn primary" data-sv="open" data-sid="${esc(r.sid)}">${r.st === "asks" ? "Answer" : "Reply"}</button>`);
  if (m?.role === "executor" && reported(m.status)) acts.push(verifyTag(m.verify) + crewBtn(r.sid, "diff", "Diff") + (m.lead_armed && !reviewWord(r.sid) ? crewBtn(r.sid, "review", "Review", true) : ""));
  if (!r.it && (!r.quiet || r.harness === "claude") && !r.ghost) acts.push(`<button class="btn" data-sv="${r.quiet ? "quiet" : "open"}" data-sid="${esc(r.sid)}">Open</button>`);
  if (!r.ghost) acts.push(`<button class="btn" data-sv="tab" data-sid="${esc(r.sid)}" title="Go to its terminal tab">Tab</button>`);
  if (svClosable(r)) acts.push(closeBtn(r.sid));
  return acts.join("");
}
/** One session, everything visible: name and state, what it's about, what it's on now, and its buttons.
 *  A lead's line also says it leads (and how); its executors drop the agent badge the lead already shows. */
function svItem(r, { leadHarness = "", crew = null } = {}) {
  const m = r.crew || crewOf(r.sid), ab = svAbout(r);
  // No name of its own: it's called after its folder, which the tile already says (two such look like
  // duplicates). Call it by what it's about instead.
  const unnamed = !crew && r.name === baseName(r.cwd) && (ab.about || ab.now);
  const name = unnamed ? ab.about || ab.now : crew ? bareName(crew.name || r.name) : r.name;
  const lines = [];
  if (!unnamed && ab.about) lines.push(`<div class="sx-l about" data-cut title="${esc(ab.about)}">${esc(ab.about)}</div>`);
  if (ab.outcome) lines.push(`<div class="sx-l out" data-cut title="${esc(ab.outcome)}"><span class="sx-k">Outcome</span>${esc(ab.outcome)}</div>`);
  if (r.st === "working" && r.run) lines.push(`<div class="sx-l run" data-cut title="${esc(r.what)}">${esc(r.what)}</div>`);
  else if (r.st === "asks" && r.what) lines.push(`<div class="sx-l ask" data-cut title="${esc(r.what)}">${esc(r.what)}</div>`);
  else if (ab.now && !(unnamed && name === ab.now)) lines.push(`<div class="sx-l now" data-cut title="${esc(ab.now)}">› ${esc(ab.now)}</div>`);
  const hot = r.st === "asks" || m?.lead_armed === false;
  const lead = crew ? `${crewDot(r.sid)}` : "";
  const leadTag = crew ? `<span class="role lead" title="${crew.plugin === "pilead" ? "pi-lead" : "relay"} lead">LEAD</span>${autoToggle(r.sid, crew)}` : "";
  // The whole tile opens it: its chat in Active (a quiet Claude session too: Cue takes it in), else what it's doing.
  const go = r.ghost ? "" : ` role="button" data-sv="${r.quiet ? "quiet" : "open"}" data-sid="${esc(r.sid)}" ${r.quiet && r.harness !== "claude" ? ` title="See what it's doing (it started before Cue was connected)"` : ""}`;
  const viewing = r.sid === (quietOpen || active?.sid) ? "on" : "";
  return `<div class="sx-item ${viewing} ${hot ? "hot" : ""} ${r.quiet ? "quiet" : ""} ${crew ? "lead" : ""} ${leadHarness ? "exec" : ""}"${go}>
    <div class="sx-iline">${r.harness === leadHarness ? "" : badge(r.harness)}${lead}<span class="sx-name" data-cut title="${esc(name)}">${esc(name)}</span>${viewing ? VIEWING : ""}${leadTag}${svChipState(r)}${r.since && !r.quiet ? `<span class="sx-age">${ago(r.since)}</span>` : ""}<span class="sx-acts">${svActs(r)}</span></div>
    ${lines.join("")}</div>`;
}
const svItems = (rs, leadHarness = "") => rs.sort((a, b) => svRank(a) - svRank(b) || b.since - a.since).map((r) => svItem(r, { leadHarness })).join("");
function sessionsView() {
  const all = liveRows();
  const q = draft("find-sessions").text.trim().toLowerCase();
  const hay = (r) => { const m = r.crew || crewOf(r.sid), ab = svAbout(r); return [r.name, r.cwd, homeless(r.cwd), state.branches?.[r.cwd], r.what, ab.about, ab.now, m?.lead_name, m?.name, r.harness].join(" ").toLowerCase(); };
  const match = (r) => !q || q.split(/\s+/).every((w) => hay(r).includes(w));
  const bySid = new Map(all.map((r) => [r.sid, r]));
  // Tiles by folder. A crew lives in its lead's tile, whatever folders its executors work in.
  const tiles = new Map();
  const tile = (cwd) => tiles.get(cwd) || tiles.set(cwd, { cwd, solo: [], crews: [] }).get(cwd);
  const placed = new Set();
  for (const lead of all.filter((r) => crewOf(r.sid)?.role === "lead")) {
    const m = crewOf(lead.sid);
    placed.add(lead.sid);
    const execs = (m.executors || []).map((e) => {
      placed.add(e.session_id);
      return bySid.get(e.session_id) || { sid: e.session_id, harness: m.plugin === "pilead" ? "pi" : "claude", name: e.name, cwd: "", st: e.status === "busy" ? "working" : "idle", since: 0, what: "", ghost: true,
        crew: { role: "executor", plugin: m.plugin, status: e.status, model: e.model, packet: e.packet, goal: e.goal, outcome: e.outcome, verify: e.verify, lead: lead.sid, lead_name: m.name, lead_armed: true } };
    });
    tile(lead.cwd).crews.push({ lead, m, execs });
  }
  // Executors whose lead Cue can't see (not running, or not in Cue yet): a crew line of their own, in their folder.
  const orphans = new Map();
  for (const r of all.filter((r) => !placed.has(r.sid) && crewOf(r.sid)?.role === "executor")) {
    placed.add(r.sid);
    const m = crewOf(r.sid), key = `${r.cwd}\n${m.lead}`;
    if (!orphans.has(key)) { orphans.set(key, { lead: null, m, execs: [] }); tile(r.cwd).crews.push(orphans.get(key)); }
    orphans.get(key).execs.push(r);
  }
  for (const r of all.filter((r) => !placed.has(r.sid))) tile(r.cwd).solo.push(r);

  const crewHtml = (c) => {
    const execs = c.execs.filter(match);
    const leadHit = c.lead && match(c.lead);
    if (q && !leadHit && !execs.length) return "";
    const head = c.lead ? svItem(c.lead, { crew: c.m })
      : `<div class="sx-crew-h ${c.m.lead_armed === false ? "gone" : ""}">${badge(c.execs[0].harness)}${crewDot(c.execs[0].sid)}<span class="sx-lname">${esc(bareName(c.m.lead_name) || "a lead")}</span><span class="sx-crew-n">${c.m.lead_armed === false ? "lead not running" : "lead not in Cue yet"}</span></div>`;
    return `<div class="sx-crew">${head}${execs.length ? `<div class="sx-execs">${svItems(execs, c.lead ? c.lead.harness : c.execs[0].harness)}</div>` : ""}</div>`;
  };
  const cards = [...tiles.values()].map((t) => {
    const solo = t.solo.filter(match);
    const crews = t.crews.map(crewHtml).filter(Boolean);
    if (!solo.length && !crews.length) return null;
    const members = [...t.solo, ...t.crews.flatMap((c) => [c.lead, ...c.execs].filter(Boolean))];
    const rank = Math.min(...members.map(svRank)), latest = Math.max(0, ...members.map((r) => r.since || 0));
    const br = state.branches?.[t.cwd];
    const html = `<div class="sx-tile ${members.some(svHot) ? "hot" : ""}">
      <div class="sx-thead"><span class="sx-tname" data-cut title="${esc(t.cwd)}">${esc(baseName(t.cwd) || "(no folder)")}</span>${br ? `<span class="sx-branch">${esc(br)}</span>` : ""}<span class="sx-tcount">${members.length} session${members.length === 1 ? "" : "s"}</span><button class="sx-plus" data-lv="new" data-cwd="${esc(t.cwd)}" title="New session in ${esc(homeless(t.cwd))}">+</button></div>
      ${solo.length ? `<div class="sx-solo">${svItems(solo)}</div>` : ""}${crews.join("")}</div>`;
    return { rank, latest, html };
  }).filter(Boolean).sort((a, b) => a.rank - b.rank || b.latest - a.latest);

  const n = (st) => all.filter((r) => r.st === st).length, quiet = all.filter((r) => r.quiet).length;
  const counts = [`<b>${all.length} live</b>`, n("asks") && `${n("asks")} asks you`, n("yours") && `${n("yours")} your turn`, n("working") && `${n("working")} working`, quiet && `${quiet} quiet`].filter(Boolean).join(" · ");
  return `<div class="sv">
    <div class="sv-bar"><label class="sv-search">${SEARCH_ICON}<input data-text="find-sessions" placeholder="Find a session: name, folder, branch, what it's doing" spellcheck="false" autocomplete="off" value="${esc(draft("find-sessions").text)}"/></label>
      <button class="btn primary sv-new" data-lv="new">+ New session</button><span class="sv-count">${counts}</span></div>
    <div class="sv-scroll">${cards.length ? `<div class="sx-tiles">${cards.map((c) => c.html).join("")}</div>` : `<div class="quiet-line">${q ? `No live session matches “${esc(q)}”.` : "No live sessions."}</div>`}</div></div>`;
}
async function svAct(act, sid) {
  if (act === "close") {
    clearTimeout(svCloseTimer);
    if (svCloseArm !== sid) {
      svCloseArm = sid;
      svCloseTimer = setTimeout(() => { svCloseArm = null; renderMain(); }, 4000);
      return renderMain();
    }
    svCloseArm = null;
    renderMain();
    try { toast(await invoke("close_session", { sessionId: sid })); }
    catch (e) { toast(`Couldn't close it: ${e}`); }
    return;
  }
  if (act === "open") { const it = state.items.find((i) => i.session_id === sid && i.status === "pending"); return setActive(it?.id || null, sid); }
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

let histFilter = "all", histSession = "";  // History's filters: what kind, which session
const histOpenGroups = new Set();          // "DAY|session" groups you opened
const HIST_KINDS = [["all", "All"], ["waiting", "Replies"], ["permission", "Permissions"], ["question", "Questions"]];
/** History: what you answered, by day, and within a day by session (a session's answers fold into one
 *  row you can open). Filter by kind or session; times are clock times. */
function historyView() {
  const keep = state.settings?.history?.keep ?? 300;
  const day = (ms) => {
    const d = new Date(ms), today = new Date(now());
    const diff = Math.round((new Date(today.toDateString()) - new Date(d.toDateString())) / 86400000);
    return diff === 0 ? "TODAY" : diff === 1 ? "YESTERDAY" : d.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" }).toUpperCase();
  };
  const at = (i) => i.resolved_ms || i.created_ms;
  const clock = (ms) => new Date(ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  const inSession = (i) => !histSession || i.session_id === histSession;
  const count = (k) => state.history.filter((i) => (k === "all" || i.kind === k) && inSession(i)).length;
  const sessions = [...new Map(state.history.map((i) => [i.session_id, nameOf(i.session_id, i.project)])).entries()];
  const filters = `<div class="hfilter">${HIST_KINDS.map(([k, l]) => `<button class="hfchip ${histFilter === k ? "on" : ""}" data-hfilter="${k}">${l}<span>${count(k)}</span></button>`).join("")}
    <span class="grow"></span><select class="hsess" data-hsession aria-label="Session"><option value="">All sessions</option>${sessions.map(([sid, n]) => `<option value="${esc(sid)}" ${histSession === sid ? "selected" : ""}>${esc(n)}</option>`).join("")}</select></div>`;
  const row = (i, nested) => {
    const open = histOpen === i.id;
    return `<div class="hrow ${open ? "open" : ""} ${nested ? "nested" : ""}" data-hrow="${esc(i.id)}">
      <span class="hwho">${nested ? "" : `${badge(i.harness)}${nameSpan(i.session_id, i.project)}`}</span>
      <span class="hwhat ${isBash(i) ? "mono" : ""}">${esc(plain(summary(i)))}</span>
      <span class="hout ${outcomeClass(i)}">${esc(outcomeText(i))}</span>
      <span class="age">${clock(at(i))}</span></div>${open ? `<div class="hexpand">${histDetail(i)}${/^replied: “/.test(outcomeText(i)) ? "" : resultLine(i)}</div>` : ""}`;
  };
  // Day, then session in order of its latest answer.
  const groups = [];
  for (const i of state.history.filter((x) => (histFilter === "all" || x.kind === histFilter) && inSession(x))) {
    const d = day(at(i));
    let g = groups.find((x) => x.day === d && x.sid === i.session_id);
    if (!g) groups.push((g = { day: d, sid: i.session_id, items: [] }));
    g.items.push(i);
  }
  let html = `<div class="hhead"><span>session</span><span>what it wanted</span><span>you</span><span>when</span></div>`, lastDay = "";
  for (const g of groups) {
    if (g.day !== lastDay) { html += `<div class="hgroup">${g.day}</div>`; lastDay = g.day; }
    if (g.items.length === 1) { html += row(g.items[0], false); continue; }
    const key = `${g.day}|${g.sid}`, open = histSession || histOpenGroups.has(key), top = g.items[0];
    html += `<div class="hrow hsrow ${open ? "open" : ""}" data-hgroup="${esc(key)}">
      <span class="hwho"><span class="hfold">${open ? "▾" : "▸"}</span>${badge(top.harness)}${nameSpan(g.sid, top.project)}<span class="hcount">${g.items.length}</span></span>
      <span class="hwhat">${esc(plain(summary(top)))}</span>
      <span class="hout ${outcomeClass(top)}">${esc(outcomeText(top))}</span>
      <span class="age">${clock(at(g.items[g.items.length - 1]))}–${clock(at(top))}</span></div>`;
    if (open) html += g.items.map((i) => row(i, true)).join("");
  }
  if (!state.history.length) html += `<div class="quiet-line" style="padding:18px">Nothing answered yet.</div>`;
  else if (!groups.length) html += `<div class="quiet-line" style="padding:18px">Nothing here with these filters.</div>`;
  else html += `<div class="hfoot">${state.history.length} of the last ${keep.toLocaleString()} kept · change in Settings</div>`;
  return `${histStats()}${filters}<div class="hist">${html}</div>`;
}
const seg = (key, cur, opts) => `<div class="seg">${opts.map(([v, l]) => `<button class="${JSON.stringify(v) === JSON.stringify(cur) ? "on" : ""}" data-set="${key}" data-val='${esc(JSON.stringify(v))}'>${esc(l)}</button>`).join("")}</div>`;
const toggle = (key, on) => `<button class="toggle ${on ? "on" : ""}" data-set="${key}" data-val="${!on}" role="switch" aria-checked="${on}" aria-label="${esc(key)}"><span></span></button>`;
const setRow = (title, sub, control) => `<div class="set-row"><div class="set-text"><div class="set-title">${title}</div>${sub ? `<div class="set-sub">${sub}</div>` : ""}</div>${control}</div>`;
/** Settings rows an add-on adds (fetched when Settings opens; none without one). */
let extSections = null;
function extRows() {
  if (!extSections) { extSections = []; invoke("ext_settings").then((v) => { extSections = v || []; renderMain(); }, () => {}); }
  return extSections.map((g) => `<div class="set-group">${esc(g.group)}</div>${(g.rows || []).map((r) =>
    `<div class="set-row ${r.art ? "with-art" : ""}"><div class="set-text"><div class="set-title">${esc(r.title)}</div>${r.sub ? `<div class="set-sub">${r.sub}</div>` : ""}</div>${r.art ? `<div class="set-art">${r.art}</div>` : ""}</div>`).join("")}`).join("");
}
/** Updates: what Cue found (from GitHub's latest release, checked by Cue itself). `offer`: a newer
 *  version found in the background, shown as a card until you update or put it off. */
let upd = { current: "", version: "", notes: "", status: "", offer: false };
async function checkUpdate() {
  upd = { ...upd, status: "checking" };
  renderMain();
  try { const r = await invoke("update_check"); upd = { ...upd, current: r.current, version: r.version || "", notes: r.notes || "", status: r.version ? "found" : "latest" }; }
  catch (e) { upd = { ...upd, status: `error:${e}` }; }
  renderMain();
}
async function installUpdate() {
  upd = { ...upd, status: "installing" };
  renderMain();
  try { await invoke("update_install"); } catch (e) { upd = { ...upd, status: `error:${e}` }; renderMain(); }
}
function updateRow() {
  if (!upd.status) checkUpdate();   // the first time Settings opens
  const st = upd.status;
  const sub = st === "checking" ? "Checking…" : st === "installing" ? `Installing Cue ${esc(upd.version)}. Cue restarts by itself when it's done.`
    : st === "found" ? `Cue ${esc(upd.version)} is out.${upd.notes ? ` ${esc(upd.notes.split("\n")[0].slice(0, 140))}` : ""}`
    : st === "latest" ? "You have the latest version." : st.startsWith("error:") ? esc(st.slice(6)) : "";
  const btn = st === "found" ? `<button class="btn primary" data-upd="install">Update and restart</button>`
    : `<button class="btn" data-upd="check" ${st === "checking" || st === "installing" ? "disabled" : ""}>Check for updates</button>`;
  return setRow(`Cue ${esc(upd.current || "")}`, `${sub}${sub ? " " : ""}Cue also looks by itself a minute after it opens, then every 6 hours.`, btn);
}
/** A newer Cue turned up in the background: a small card in the header's middle, until you choose. */
const updateCard = () => !upd.offer ? "" : `<div class="upd-card"><div><b>Cue ${esc(upd.version)} is out</b>${upd.status === "installing" ? `<div class="dim">Installing… Cue restarts by itself.</div>` : upd.status.startsWith("error:") ? `<div class="dim">${esc(upd.status.slice(6))}</div>` : ""}</div>
  <button class="btn primary small" data-upd="install" ${upd.status === "installing" ? "disabled" : ""}>Update and restart</button><button class="btn small" data-upd="later">Later</button></div>`;
/** The agents Cue connects to, as Settings rows (also the first-launch setup screen's). */
function agentRows() {
  const c = state.connections || {};
  const conn = (ok) => `<span class="conn ${ok ? "ok" : ""}"><i></i>${ok ? "connected" : "not connected"}</span>`;
  // Not connected but installed: one click does what install.sh does.
  const connBtn = (h, ok, present) => (ok || !present ? conn(ok) : `<button class="btn primary" data-connect="${h}">Connect</button>`);
  return `${setRow("Claude Code", c.claude?.ok ? "Sessions already open when you connected: type <code>/hooks</code> in each once (or restart it with claude --resume)." : c.claude?.present === false ? "Not installed on this Mac." : "Not connected yet. Connect adds Cue's hooks to <code>~/.claude/settings.json</code> (backed up first; your other settings stay).", connBtn("claude", c.claude?.ok, c.claude?.present !== false))}
    ${setRow("Codex", c.codex?.ok ? "Hooks installed. In Codex, run <code>/hooks</code> once to trust them." : c.codex?.present ? "Not connected yet. Connect adds Cue's hooks to <code>~/.codex/hooks.json</code> (backed up first)." : "Not installed on this Mac.", connBtn("codex", c.codex?.ok, c.codex?.present))}
    ${setRow("Pi", c.pi?.ok ? "New Pi sessions load the Cue extension. Replies go straight into the session." : c.pi?.present ? "Not connected yet. Connect adds Cue's extension to Pi." : "Not installed on this Mac.", connBtn("pi", c.pi?.ok, c.pi?.present))}`;
}
/** First launch: connect the agents you use (until Done). */
function setupSheet() {
  const todo = ["claude", "codex", "pi"].filter((h) => { const c = state.connections?.[h]; return c && !c.ok && c.present; });
  return `<div class="sheet setup-sheet"><div class="sheet-head"><h3>Connect your agents</h3></div><div class="sheet-body">
    <p class="setup-lede">Cue shows you what your coding agents need from you: a finished turn, a permission, a question. Connect the ones you use. You can change this later in Settings.</p>
    ${agentRows()}
    <div class="setup-foot">${todo.length > 1 ? `<button class="btn" data-connect-all>Connect all</button>` : ""}<button class="btn primary" data-setup-done>Done</button></div>
  </div></div>`;
}
function settingsSheet() {
  const st = state.settings || {};
  const c = state.connections || {};
  const keep = st.history?.keep ?? 300;
  return `<div class="sheet"><div class="sheet-head"><h3>Settings</h3>${upd.current ? `<span class="set-ver">Cue ${esc(upd.current)}</span>` : ""}<span class="grow"></span><kbd>Esc</kbd></div><div class="sheet-body">
    <div class="set-group">Updates</div>
    ${updateRow()}
    <div class="set-group">Connected agents</div>
    ${agentRows()}
    ${extRows()}
    <div class="set-group">Usage</div>
    ${setRow("Show Claude Code's limits", "The 5-hour and weekly meters from your Claude plan. Off: only your usage file's meters show.", toggle("usage.claude", st.usage?.claude ?? true))}
    ${setRow("Usage file", `Your own meters beside Claude Code's: a CSV you keep up to date (a proxy's budget, credits, tokens). Header <code>label,spent,limit,unit,resets_at</code>, then up to 3 rows; only <code>spent</code> is required. Cue only reads it, whenever it changes.`, `<input class="path-in" id="usage-file" data-usage-file value="${esc(st.usage?.file || "~/.cue/usage.csv")}" spellcheck="false" aria-label="Usage file"/>`)}
    <div class="set-group">Look</div>
    ${setRow("Appearance", "Moss, light or dark. System follows macOS.", seg("appearance.mode", st.appearance?.mode ?? "system", [["system", "System"], ["light", "Light"], ["dark", "Dark"]]))}
    <div class="set-group">Quick phrases</div>
    ${setRow("Above the text box", `Click one to send it, after anything you've typed in the box. Up to 4. Empty a field to remove it.`, `<div class="quick-edit">${[0, 1, 2, 3].map((n) => `<input class="qp-in" data-quick-edit id="qp-${n}" value="${esc(qpEdit ? qpEdit[n] : phrases()[n] || "")}" maxlength="40" placeholder="${n < phrases().length ? "" : "Add one"}" aria-label="Quick phrase ${n + 1}"/>`).join("")}</div>`)}
    <div class="set-group">Steps</div>
    ${setRow("Steps in the chat", "What the agent did between your message and its reply, read from its transcript when you look. One line each: click a step for its output or diff.", seg("steps.mode", st.steps?.mode ?? "line", [["line", "One line each"], ["all", "Everything"], ["hidden", "Hidden"]]))}
    <div class="set-group">Context</div>
    ${setRow("Earlier in this session", "How many past exchanges (what it said or asked, what you answered) each card keeps.", seg("context.keep", st.context?.keep ?? 5, [[5, "5"], [10, "10"], [20, "20"]]))}
    ${setRow("When a Stop hook makes the agent continue, the card shows", "The answer: the agent's final reply, with the hook's output (e.g. a checklist) tucked underneath. The last message: whatever it wrote last, usually the hook's output.", seg("turn.mode", st.turn?.mode ?? "answer", [["answer", "The answer"], ["last", "The last message"]]))}
    <div class="set-group">History</div>
    ${setRow("Keep the last", `${state.history.length.toLocaleString()} saved now · ~/.cue/history.jsonl`, `<div style="display:flex;gap:8px;align-items:center">${seg("history.keep", keep, [[100, "100"], [300, "300"], [1000, "1,000"], [5000, "5,000"]])}<input class="num" type="number" min="10" step="50" value="${keep}" data-keep-custom aria-label="Custom history size"/></div>`)}
    <div class="set-group">Notifications</div>
    ${setRow("Test", "Sends one macOS notification now. If nothing appears, check System Settings → Notifications → Cue.", `<button class="btn" data-act="test-notify">Send a test notification</button>`)}
    ${setRow("When an agent finishes its turn", "Tells you which session is waiting on you.", toggle("notify.finished", st.notify?.finished ?? true))}
    ${setRow("When an agent needs a decision", "Permissions and questions.", toggle("notify.decisions", st.notify?.decisions ?? true))}
    ${setRow("Open at login", "Start Cue when you log in to your Mac. It's listed under System Settings → General → Login Items.", toggle("login.open", st.login?.open ?? false))}
    ${setRow("Show icon in menu bar", "Off: no icon up top; open Cue from the Dock. Alerts still arrive as notifications.", toggle("tray.show", st.tray?.show ?? true))}
    <div class="set-group">Agents driving agents</div>
    ${setRow("Show sessions another agent drives in Waiting", "Off: when a helper agent finishes (one another agent started and labelled with CUE_DRIVEN_BY), its lead handles it, so it stays in Working as \"waiting on its lead\" with no card or notification. Turn on to treat them like your own sessions.", toggle("agents.show_driven", st.agents?.show_driven ?? false))}
    <div class="set-group">Pi</div>
    ${setRow("Ask me before", "Pi doesn't ask on its own; Cue's extension adds the prompt.", seg("pi.gate", st.pi?.gate ?? "dangerous", [["dangerous", "Risky commands"], ["all", "Every command & edit"], ["off", "Never"]]))}
  </div></div>`;
}

// ---------- main render ----------
function applyTheme() {
  const mode = state.settings?.appearance?.mode ?? "system";
  if (mode === "system") document.documentElement.removeAttribute("data-theme");
  else document.documentElement.dataset.theme = mode;
}

/** Cards that move between redraws (back into Waiting, into Need to decide, or up to
 *  close a gap) glide from where they were instead of jumping, so you can follow where they went.
 *  Keyed by what they show: an ask or turn by its id, a session card by its session. */
const FLIP_SEL = ".board .col:not(.main) .nrow[data-big], .board .col:not(.main) .nrow[data-session], .board .col:not(.main) .working[data-session]";
const flipKey = (el) => (el.dataset.big ? `i:${el.dataset.big}` : `s:${el.dataset.session}`);
function flipRects() {
  const m = new Map();
  for (const el of document.querySelectorAll(FLIP_SEL)) m.set(flipKey(el), el.getBoundingClientRect());
  return m;
}
function flipPlay(from) {
  if (!from.size || matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  for (const el of document.querySelectorAll(FLIP_SEL)) {
    const was = from.get(flipKey(el));
    if (!was) continue;
    const now = el.getBoundingClientRect(), dx = was.left - now.left, dy = was.top - now.top;
    if (Math.abs(dx) < 1 && Math.abs(dy) < 1) continue;
    el.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: "none" }], { duration: 340, easing: "cubic-bezier(.2,.8,.2,1)" });
  }
}
function renderMain() {
  // You're dragging out a selection: a redraw would wipe it. It waits until you let go.
  if (dragSelecting()) { heldDraw = true; return; }
  const focused = document.activeElement;
  const flipFrom = flipRects();
  const focusKey = focused?.dataset?.text;
  const focusId = !focusKey && focused?.matches?.("input[id]") ? focused.id : null;   // e.g. a quick phrase field
  const caret = focusKey || focusId ? [focused.selectionStart, focused.selectionEnd] : null;
  const SCROLLERS = ".col, .lv-list, .sv-scroll, .sheet-body, .ap-chat, .sec-body, .card.recent, .hist, .hdetail";
  const scrolls = [...document.querySelectorAll(SCROLLERS)].map((el) => el.scrollTop);
  const onBefore = selectedCards();   // where the selected card sits in each list, and whether it's in full view
  const oldChat = document.querySelector("[data-chat]");
  const chatPinned = oldChat && oldChat.scrollHeight - oldChat.scrollTop - oldChat.clientHeight < 24;

  const counts = headerChips();
  document.getElementById("app").innerHTML = `<div class="dragbar" data-tauri-drag-region></div><div class="app">
    <div class="top" data-tauri-drag-region><span class="wordmark" data-tauri-drag-region role="img" aria-label="Cue">${CUE_MARK}</span><span class="hchips">${counts}${liveChip()}${starChip()}</span><span class="grow" data-tauri-drag-region></span>${updateCard()}<span class="grow" data-tauri-drag-region></span>
      <button class="top-btn icon" data-act="open-search" title="Search (⌘F or /)" aria-label="Search">${SEARCH_ICON}</button>
      ${usageChip()}
      <div class="switch-view"><button class="${view === "board" ? "on" : ""}" data-view="board">Board</button><button class="${view === "sessions" ? "on" : ""}" data-view="sessions">Sessions</button><button class="${view === "history" ? "on" : ""}" data-view="history">History</button></div>
      <button class="top-btn icon" data-act="open-settings" title="Settings" aria-label="Settings">${GEAR_ICON}</button></div>
    ${view === "history" ? historyView() : view === "sessions" ? sessionsView() : boardView()}
  </div>${lightbox ? `<div class="lightbox" data-act="close-lightbox"><img src="${esc(lightbox.srcs[lightbox.i])}" alt=""/>${lightbox.srcs.length > 1 ? `<div class="lb-count">${lightbox.i + 1} / ${lightbox.srcs.length} · ← →</div>` : ""}</div>` : ""}${sheet === "forward" && forward ? forwardPop() : ""}${sheet && sheet !== "forward" ? `<div class="scrim" data-act="close-sheet">${sheet === "search" ? searchSheet() : sheet === "convo" && convo ? convoSheet() : sheet === "setup" ? setupSheet() : settingsSheet()}</div>` : ""}${liveOpen && liveSpot ? `<div class="scrim spot-scrim"><span class="lv-wrap spot">${livePop(liveRows())}</span></div>` : ""}`;

  [...document.querySelectorAll(SCROLLERS)].forEach((el, i) => { if (scrolls[i] != null) el.scrollTop = scrolls[i]; });
  flipPlay(flipFrom);
  placeForward();
  saveSpot();
  // A new selection whose card is scrolled out of its column (under Recently answered, or above):
  // glide it into view, just enough to show it. Only when the selection changes, so it never fights you.
  // A conversation opened from search starts at the message you found (else at its end).
  if (sheet === "convo" && !convoScrolled) {
    convoScrolled = true;
    const body = document.querySelector(".convo-body"), hit = body?.querySelector(".cv-hit");
    if (hit) hit.scrollIntoView({ block: "center" }); else if (body) body.scrollTop = body.scrollHeight;
  }
  // The selected card, cut off at a list's edge, comes into full view: when you pick it, when it moves to
  // another list (you replied: it went from Waiting to Sessions), or when it outgrew the view while in full
  // view (a line added as it works). Not when you scrolled it part-way out yourself. It glides, just enough.
  const picked = `${active?.id}|${active?.sid}`;
  const repick = picked !== revealed;
  revealed = picked;
  for (const [list, { el, full }] of selectedCards()) {
    const was = onBefore.get(list);
    if (!full && (repick || !was || was.full)) glideIntoView(el);
  }
  // Something new in Waiting rises in from below with a brief ring, once.
  document.querySelectorAll(".nrow[data-big]").forEach((el) => {
    if (seenCards.has(el.dataset.big)) return;
    seenCards.add(el.dataset.big);
    if (cardsPrimed) el.classList.add("arrive");
  });
  cardsPrimed = true;
  // Stepped with ‹ › or ← →: the new item slides in from the side you moved toward.
  if (slideDir) {
    const pane = document.querySelector(".col.main .active-pane");
    pane?.classList.add(slideDir > 0 ? "slide-r" : "slide-l");
    slideDir = 0;
  }
  // A History row you just opened: its conversation starts at the end, where you replied, and the row comes into view.
  if (view === "history" && histOpen && histOpen !== histShown) {
    const ex = document.querySelector(".hexpand"), d = ex?.querySelector(".hdetail");
    if (d) d.scrollTop = d.scrollHeight;   // the end: your reply, under what it answered
    ex?.scrollIntoView({ block: "nearest" });
  }
  histShown = view === "history" ? histOpen : null;
  document.querySelectorAll("textarea[data-text]").forEach(grow);
  // The chat jumps to its newest message only when you open something else or a new message lands.
  const chat = document.querySelector("[data-chat]");
  const key = chat ? `${active?.id}|${active?.sid}|${chat.children.length}` : null;
  // New target or new message: jump to the end. Same chat, you were at the end: stay there
  // (an image added to the box, or a redraw, mustn't push the last lines out of view).
  if (chat && holdChat != null) { chat.scrollTop = chat.scrollHeight - holdChat; holdChat = null; }
  else if (chat && (key !== activeKey || chatPinned)) chat.scrollTop = chat.scrollHeight;
  activeKey = key;
  // Something new opened in Active: its text box takes the cursor, so you can just type. Done once it
  // has (at launch the first draw comes before the sessions do: no box yet, so the next draw tries again).
  const target = `${active?.id}|${active?.sid}`;
  if (target !== focusedTarget && !sheet && !focusKey && focusComposer()) focusedTarget = target;
  saveDrafts();
  if (focusKey) { const el = document.querySelector(`[data-text="${CSS.escape(focusKey)}"]`); if (el) { el.focus(); el.setSelectionRange(...caret); } }
  else if (focusId) { const el = document.getElementById(focusId); if (el) { el.focus(); el.setSelectionRange(...caret); } }
  // Something over the chat closed (Esc, a click outside, a pick): its text box takes the cursor back,
  // unless you closed it by clicking into another box.
  const over = !!(sheet || lightbox || liveOpen || menuFor || redirectFor || renaming || btwFor);
  if (overWas && !over && !document.activeElement?.matches?.("input, textarea, select")) focusComposer();
  overWas = over;
  fitHead();
  // By the way's panel drops from just under the chat's header (whatever height that has now).
  const bp = document.querySelector(".btw-pop"), ah = document.querySelector(".ap-head");
  if (bp && ah) bp.style.top = `${ah.offsetTop + ah.offsetHeight + 2}px`;
  showCopied();
  fitTop();
}
/** The top bar never pushes Settings off the edge (a usage file adds meters; chips come and go): when it
 *  doesn't fit, it gives up room step by step: the chips' extras ("· oldest 4m"), the meters' bars, the
 *  meters' labels, then the chips' words (the number and dot stay; the tooltip still says what they are). */
const TOP_STEPS = ["tfit1", "tfit2", "tfit3", "tfit4"];
function fitTop() {
  const h = document.querySelector(".app > .top");
  if (!h) return;
  h.classList.remove(...TOP_STEPS);
  // Fits: Settings (the last button) ends inside the bar. Measured by it, so an open drop-down doesn't count.
  const fits = () => !h.lastElementChild || h.lastElementChild.getBoundingClientRect().right <= h.getBoundingClientRect().right + 1;
  for (const step of TOP_STEPS) {
    if (fits()) return;
    h.classList.add(step);
  }
}
/** The chat header's buttons never get cut off: when they don't fit, it gives up room step by step
 *  (short labels, then the agent and model, then the "your turn" pill, then the role tag, and the
 *  name shortens). The CSS widths below can't know how many buttons a session has. */
const FIT_STEPS = ["fit1", "fit2", "fit3", "fit4"];
function fitHead() {
  const h = document.querySelector(".ap-head");
  if (!h) return;
  h.classList.remove(...FIT_STEPS);
  // Fits: the last button ends inside the header's right padding.
  const fits = () => {
    const last = [...h.children].filter((k) => k.offsetParent).at(-1);
    return !last || last.getBoundingClientRect().right <= h.getBoundingClientRect().right - parseFloat(getComputedStyle(h).paddingRight) + 1;
  };
  for (const step of FIT_STEPS) {
    if (fits()) return;
    h.classList.add(step);
  }
}
addEventListener("resize", () => { fitHead(); fitTop(); });

/** Cue's mark (the app icon without its tile): an open C around a dot. */
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
let escArmed = 0;           // when Esc was last pressed on a working session (twice within 1.5s stops it)
let focusedTarget = null;   // the Active target whose text box last got the cursor
let overWas = false;        // something covered the chat at the last draw (a sheet, the live list, an image…)
/** Put the cursor at the end of the Active pane's text box (opening something, Esc Esc). */
function focusComposer() {
  const el = document.querySelector(".active-pane textarea[data-text]");
  if (!el) return false;
  el.focus();
  el.setSelectionRange(el.value.length, el.value.length);
  return true;
}

// ---------- header chips: what's waiting, what's asking, what's working; click to open the oldest ----------
/** Each chip's list, oldest first: what a click steps through. Idle sessions aren't "working". */
function chipList(kind) {
  const { yours, decide, working } = groups();
  if (kind === "waiting") return yours.map((i) => ({ id: i.id, sid: i.session_id, project: i.project, since: i.created_ms }));
  if (kind === "asking") return decide.map((i) => ({ id: i.id, sid: i.session_id, project: i.project, since: i.created_ms }));
  return working.filter((s) => !idleNote(s)).sort((a, b) => a.since_ms - b.since_ms).map((s) => ({ id: null, sid: s.session_id, project: s.project, since: s.since_ms }));
}
function headerChips() {
  const chip = (kind, label, extra, cls, dot) => {
    const list = chipList(kind);
    if (!list.length) return "";
    // The chip you're stepping through: ‹ 2 of 3 waiting › (← → do the same).
    const at = chipNav === kind ? chipAt(list) : -1;
    if (at >= 0 && list.length > 1) return `<span class="hchip nav ${cls}"><button class="hstep" data-chip-step="${kind}:-1" title="Previous (←)" aria-label="Previous">‹</button><button class="hnav" data-chip="${kind}"><span class="hdot ${dot}"></span>${at + 1} of ${list.length}<span class="hlbl"> ${label}</span></button><button class="hstep" data-chip-step="${kind}:1" title="Next (→)" aria-label="Next">›</button></span>`;
    return `<button class="hchip ${cls}" data-chip="${kind}"><span class="hdot ${dot}"></span>${list.length}<span class="hlbl"> ${label}</span>${extra ? `<span class="hsub"> · ${extra}</span>` : ""}</button>`;
  };
  const waiting = chipList("waiting");
  const oldest = waiting[0] ? now() - waiting[0].since : 0;
  // Stronger the longer the oldest has waited: soft under 5 minutes, then half, then solid at 15.
  const heat = oldest >= 15 * 60000 ? "hot" : oldest >= 5 * 60000 ? "warm" : "";
  const html = chip("waiting", "waiting", waiting[0] ? `oldest ${ago(waiting[0].since)}` : "", heat, "z")
    + chip("asking", "asking you", "", "", "ask")
    + chip("working", "working", "", "", "w");
  return html || `<span class="hall" data-tauri-drag-region>all clear</span>`;
}
/** Where the Active pane sits in a chip's list (-1 when it's showing something else). */
const chipAt = (list) => list.findIndex((x) => (x.id ? x.id === active?.id : x.sid === active?.sid && !isPending(findItem(active?.id))));
/** A chip click opens the oldest of its kind; clicking again (or ›, →) steps to the next, ‹ or ← back; both wrap. */
function openFromChip(kind, dir = 1) {
  const list = chipList(kind);
  if (!list.length) return;
  const at = chipAt(list);
  const next = list[at < 0 ? (dir > 0 ? 0 : list.length - 1) : (at + dir + list.length) % list.length];
  chipNav = kind;
  slideDir = dir;
  setActive(next.id, next.sid);
}

/** Text boxes grow with their content, up to ~8 lines, then scroll. */
function grow(el) {
  // Growing the box shrinks the chat above it. If you were reading the end of the chat, keep the
  // end in view (the conversation moves up with the box) instead of letting the box cover it.
  const chat = el.closest(".active-pane")?.querySelector("[data-chat]");
  const pinned = chat && chat.scrollHeight - chat.scrollTop - chat.clientHeight < 24;
  el.style.height = "auto";
  el.style.height = `${Math.min(el.scrollHeight, 180)}px`;
  if (pinned) chat.scrollTop = chat.scrollHeight;
}
const picker = Object.assign(document.createElement("input"), { type: "file", accept: "image/*", multiple: true, hidden: true });
picker.addEventListener("change", () => attach(picker.dataset.key, [...picker.files]));
// Closed Finder's picker without one: back to the box you were adding to.
picker.addEventListener("cancel", () => document.querySelector(`[data-text="${CSS.escape(picker.dataset.key)}"]`)?.focus());
async function attach(key, files) {
  for (const f of files.slice(0, 6)) {
    if (f.size > 15 * 1024 * 1024) { toast(`${f.name} is over 15 MB`); continue; }
    const data = await new Promise((ok) => { const r = new FileReader(); r.onload = () => ok(r.result); r.readAsDataURL(f); });
    draft(key).images.push({ name: f.name || "pasted image", mime: f.type, data });
  }
  renderMain();
  document.querySelector(`[data-text="${CSS.escape(key)}"]`)?.focus();
}

/** Hover text that shows at once, inside Cue's window: the browser's own (from `title`) waits about a
 *  second and doesn't show unless Cue is the app in front. Anything with a title gets it; with
 *  `data-cut` (the hover is just the text you see), only when that text is cut off. */
function bindTips() {
  const tip = document.createElement("div");
  tip.className = "tip";
  document.body.append(tip);
  let on = null;
  const hide = () => { tip.classList.remove("show"); on = null; };
  document.addEventListener("mouseover", (e) => {
    const el = e.target.closest?.("[title], [data-tip]");
    if (!el) return hide();
    // Take the title, so the browser's own doesn't show as well.
    if (el.hasAttribute("title")) { el.dataset.tip = el.getAttribute("title"); el.removeAttribute("title"); }
    if (el === on || !el.dataset.tip) return;
    // Cut off: the text, or a part of it (a path's start, cut in the middle), doesn't fit.
    const cut = (x) => x.scrollWidth > x.clientWidth + 1 || x.scrollHeight > x.clientHeight + 1;
    if (el.hasAttribute("data-cut") && !cut(el) && ![...el.children].some(cut)) return hide();
    on = el;
    tip.textContent = el.dataset.tip;
    const r = el.getBoundingClientRect();
    tip.style.maxWidth = "320px";
    tip.classList.add("show");
    const w = tip.offsetWidth, h = tip.offsetHeight;
    const x = Math.min(Math.max(8, r.left + r.width / 2 - w / 2), innerWidth - w - 8);
    const below = r.bottom + 8 + h < innerHeight;
    tip.style.left = `${x}px`;
    tip.style.top = `${below ? r.bottom + 6 : r.top - h - 6}px`;
  });
  document.addEventListener("mousedown", hide);
  document.addEventListener("scroll", hide, true);
  addEventListener("blur", hide);
}
function bindMain() {
  bindTips();
  document.body.append(picker);
  document.addEventListener("scroll", placeForward, true);
  // Near the top of the chat: the next page of earlier messages comes in above.
  document.addEventListener("scroll", (e) => {
    if (!e.target?.matches?.("[data-chat]") || e.target.scrollTop > 60) return;
    const cur = current(), sid = cur?.s?.session_id || cur?.it?.session_id;
    if (sid) loadOlder(sid);
  }, true);   // the send-to menu follows its button
  const app = document.getElementById("app");
  // Where a click began: the live drop-down redraws every few seconds, so a click that began inside it can
  // end on a fresh copy and reach the page as a click "outside". Only a press that began outside closes it.
  let downInLive = false;
  document.addEventListener("mousedown", (e) => { downInLive = !!e.target.closest?.(".lv-wrap"); }, true);
  // Into a session's box: start reading its "/" commands now (that takes a few seconds the first
  // time in a folder), so they're usually in by the time you type "/".
  app.addEventListener("focusin", (e) => { const sid = sidOfKey(e.target.dataset?.text || ""); if (sid) cmdList(sid); });
  app.addEventListener("click", (e) => {
    const t = e.target;
    if (lightbox) { lightbox = null; return renderMain(); }
    // A queued message, sent now: stopping the turn (Esc) makes the agent read it straight away.
    // Clear: forget this session's side questions (they were only ever in this window).
    const bc = t.closest("[data-act=btw-clear]");
    if (bc) { btwLog.delete(bc.dataset.sid); renderMain(); return document.querySelector(`[data-text="${CSS.escape(`btw:${bc.dataset.sid}`)}"]`)?.focus(); }
    const bw = t.closest("[data-act=btw], [data-act=btw-close]");
    if (bw) { if (bw.dataset.act === "btw" && btwFor !== bw.dataset.sid) return openBtw(bw.dataset.sid); btwFor = null; return renderMain(); }
    const sn = t.closest("[data-act=send-now]");
    if (sn) return sendQueuedNow(sn.dataset.sid);
    if (t.closest("[data-setup-done]")) { sheet = null; setSetting("setup.done", true); return renderMain(); }
    if (t.closest("[data-connect-all]")) {
      const todo = ["claude", "codex", "pi"].filter((h) => { const c = state.connections?.[h]; return c && !c.ok && c.present; });
      (async () => { for (const h of todo) { try { toast(await invoke("connect_agent", { harness: h })); } catch (e) { toast(`Couldn't connect ${h}: ${e}`); } } })();
      return;
    }
    // The ⋯ menu: toggle it, run an item, or close it on a click anywhere else.
    const mb = t.closest("[data-more]");
    if (mb) { moreFor = moreFor === mb.dataset.more ? null : mb.dataset.more; handoffArm = null; return renderMain(); }
    const mc = t.closest("[data-cmd]");
    if (mc) {
      const { cmd, sid } = mc.dataset;
      if (cmd === "handoff" && handoffArm !== sid) { handoffArm = sid; return renderMain(); }
      moreFor = handoffArm = null;
      renderMain();
      invoke("session_command", { sessionId: sid, action: cmd }).then((m) => toast(m), (e) => toast(`Couldn't: ${e}`));
      return;
    }
    if (moreFor && !t.closest(".more-wrap")) { moreFor = handoffArm = null; renderMain(); }
    if (t.closest(".more-menu [data-park]")) moreFor = handoffArm = null;   // Decide later: done with the menu too
    const cb = t.closest("[data-connect]");
    if (cb) { cb.disabled = true; invoke("connect_agent", { harness: cb.dataset.connect }).then((m) => toast(m), (e) => { toast(`Couldn't connect: ${e}`); cb.disabled = false; }); return; }
    const ub = t.closest("[data-upd]");
    if (ub) { const a = ub.dataset.upd; if (a === "check") checkUpdate(); else if (a === "install") installUpdate(); else { upd = { ...upd, offer: false }; renderMain(); } return; }
    const tf = t.closest("[data-turn-fold]");
    if (tf) { const k = tf.dataset.turnFold; turnFold.set(k, tf.dataset.open !== "1"); renderMain(); return wantDetails(k.split("|")[0]); }
    const ta = t.closest("[data-turn-all]");
    if (ta) { const k = ta.dataset.turnAll; turnAll.has(k) ? turnAll.delete(k) : turnAll.add(k); renderMain(); return wantDetails(k.split("|")[0]); }
    const sl = t.closest("[data-step]");
    if (sl) { const k = sl.dataset.step; stepFlip.has(k) ? stepFlip.delete(k) : stepFlip.add(k); renderMain(); return wantDetails(k.split("|")[0]); }
    const qp = t.closest("[data-quick]");
    if (qp) {
      const key = qp.dataset.quick, d = draft(key);
      d.text = withPhrase(d.text, qp.dataset.phrase);
      saveDrafts();
      renderMain();
      // …and it goes, the way that box's Send would send it.
      document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`)?.closest(".composer")?.querySelector(".btn.send")?.click();
      return;
    }
    const cp = t.closest("[data-cmd-pick]");
    if (cp) return pickCmd(cp.dataset.cmdPick, cp.dataset.cmd, false);
    const st = t.closest("[data-astext]");
    if (st) {
      const key = st.dataset.astext, el = document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`);
      draft(key).text = ` ${draft(key).text.trimStart()}`;
      saveDrafts();
      st.parentElement.hidden = true;
      refreshCmd(key);
      if (el) { el.value = draft(key).text; el.focus(); el.selectionStart = el.selectionEnd = el.value.length; }
      return;
    }
    { const fd = t.closest("[data-fold]"); if (fd) return foldDrawer(fd.dataset.fold); }
    if (t.closest("[data-act=convo-back]")) { sheet = "search"; renderMain(); return document.querySelector(".search-in")?.focus(); }
    const co = t.closest("[data-act=convo-open]");
    if (co) { sheet = null; return setActive(null, co.dataset.sid); }
    const sm = t.closest("[data-srmore]");
    if (sm) { searchMore.add(sm.dataset.srmore); renderMain(); return document.querySelector(".search-in")?.focus(); }
    const sr = t.closest("[data-sr]");
    if (sr) return pickResult(+sr.dataset.sr);
    const starEl = t.closest("[data-star]");
    if (starEl) return toggleStar(starEl.dataset.star);
    if (t.closest("[data-star-chip]")) { starOpen = !starOpen; liveOpen = newOpen = false; return renderMain(); }
    const rn = t.closest("[data-rename]");
    if (rn) {
      renaming = rn.dataset.rename;
      draft(`rename:${renaming}`).text = sessionOf(renaming)?.name || state.about?.[renaming]?.title || "";
      renderMain();
      const el = document.querySelector(".rename-in");
      el?.focus(); el?.select();
      return;
    }
    const mic = t.closest("[data-mic]");
    if (mic) return toggleDictation(mic.dataset.mic);
    const pb = t.closest("[data-park]");
    if (pb) { const [sid, on] = pb.dataset.park.split(/:(?=[01]$)/); return setParked(sid, on === "1"); }
    const hide = t.closest("[data-hide-idle]");
    if (hide) return hideIdle(hide.dataset.hideIdle);
    const fwdBtn = t.closest("[data-fwd]");
    if (fwdBtn && fwdMsgs.has(fwdBtn.dataset.fwd)) { forward = { ...fwdMsgs.get(fwdBtn.dataset.fwd), key: fwdBtn.dataset.fwd, to: null }; sheet = "forward"; return renderMain(); }
    const fwdTo = t.closest("[data-fwd-to]");
    if (fwdTo && forward) { forward.to = fwdTo.dataset.fwdTo; renderMain(); return document.querySelector(".fwd-note")?.focus(); }
    if (t.closest("[data-act='send-forward']")) return sendForward();
    if (t.closest("[data-act='cancel-forward']")) { sheet = forward = null; return renderMain(); }
    const link = t.closest("a[data-href]");
    if (link) { e.preventDefault(); return invoke("open_link", { url: link.dataset.href }).catch((err) => toast(`Couldn't open: ${err}`)); }
    const llb = t.closest("[data-local-lb]");
    if (llb) {
      const { paths, i } = JSON.parse(llb.dataset.localLb);
      const srcs = paths.map((p) => localImgs.get(p)).filter(Boolean);
      if (srcs.length) { lightbox = { srcs, i: Math.min(i, srcs.length - 1) }; return renderMain(); }
      return;
    }
    const lb = t.closest("[data-lightbox]");
    if (lb) { lightbox = JSON.parse(lb.dataset.lightbox); return renderMain(); }
    const dlb = t.closest("[data-draft-lightbox]");
    if (dlb) { const [key, n] = dlb.dataset.draftLightbox.split(/:(?=\d+$)/); lightbox = { srcs: draft(key).images.map((im) => im.data), i: +n }; return renderMain(); }
    if (t.matches(".scrim")) { sheet = null; return renderMain(); }
    const us = t.closest("[data-usage]");
    if (us) {
      const k = us.dataset.usage;
      if (k === "resend") return resend(limitedSessions().filter((s) => lifted(s.limit)).map((s) => s.session_id));
      if (k === "resend-one") return resend([us.dataset.sid]);
      if (k === "out" || k === "model") {
        // Step through the paused sessions, oldest first.
        const list = limitedSessions().filter((s) => (k === "out") === (s.limit.scope === "all")).sort((a, b) => a.since_ms - b.since_ms);
        const at = list.findIndex((s) => s.session_id === active?.sid);
        const nx = list[(at + 1) % list.length];
        return nx && setActive(null, nx.session_id);
      }
      usageOpen = !usageOpen;
      return renderMain();
    }
    if (usageOpen && !t.closest(".usage-pop")) { usageOpen = false; renderMain(); }
    const hs = t.closest("[data-chip-step]");
    if (hs) { const [kind, dir] = hs.dataset.chipStep.split(":"); return openFromChip(kind, +dir); }
    const hc = t.closest("[data-chip]");
    if (hc) return openFromChip(hc.dataset.chip);
    const setBtn = t.closest("[data-set]");
    if (setBtn) return setSetting(setBtn.dataset.set, JSON.parse(setBtn.dataset.val));
    const at = t.closest("[data-attach]");
    if (at) { picker.dataset.key = at.dataset.attach; picker.value = ""; return picker.click(); }
    const un = t.closest("[data-unattach]");
    if (un) { const [key, n] = un.dataset.unattach.split(/:(?=\d+$)/); draft(key).images.splice(+n, 1); return renderMain(); }
    const fo = t.closest("[data-follow]");
    if (fo) { const id = fo.dataset.follow; openFollow.has(id) ? openFollow.delete(id) : openFollow.add(id); return renderMain(); }
    const stop = t.closest("[data-act=interrupt]");
    if (stop) return interrupt(stop.dataset.sid);
    const st_ = t.closest("[data-act=send-to]");
    if (st_) return sendTo(st_.dataset.sid);
    const lv = t.closest("[data-lv]");
    if (lv) return lvAct(lv.dataset.lv, lv.dataset);
    // A team button (auto, Diff, Review) inside a row that opens its session: the button, not the row.
    const ca = t.closest("[data-crew]");
    if (ca) return crewAct(ca.dataset.crewSid, ca.dataset.crew);
    // A button in the row (Allow / Deny in the starred list) does its own thing, not "open".
    const sv = t.closest("button[data-act]") ? null : t.closest("[data-sv]");
    if (sv) { if (liveOpen && sv.dataset.sv !== "close") liveOpen = false; starOpen = false; return svAct(sv.dataset.sv, sv.dataset.sid); }
    if (starOpen && !t.closest(".st-wrap")) { starOpen = false; renderMain(); }
    if (liveOpen && !downInLive && !t.closest(".lv-wrap")) { liveOpen = false; renderMain(); }
    const tr = t.closest("[data-act=trust]");
    if (tr) { invoke("trust_folder", { sessionId: tr.dataset.sid }).then(() => toast("Trusted: it's starting")).catch((e) => toast(`Couldn't answer it: ${e}. Use Go to tab.`)); return; }
    const goS = t.closest("[data-act=go-session]");
    if (goS) { invoke("focus_session_id", { sessionId: goS.dataset.sid }).then((r) => toast(`Jumped to ${r}`)).catch((e) => toast(`Couldn't jump: ${e}`)); return; }
    const ss = t.closest("[data-session]");
    if (ss) return setActive(null, ss.dataset.session);
    const vb = t.closest("[data-view]");
    if (vb) { view = vb.dataset.view; return renderMain(); }
    const hf = t.closest("[data-hfilter]");
    if (hf) { histFilter = hf.dataset.hfilter; histOpen = null; return renderMain(); }
    const hg = t.closest("[data-hgroup]");
    if (hg) { const k = hg.dataset.hgroup; histOpenGroups.has(k) ? histOpenGroups.delete(k) : histOpenGroups.add(k); return renderMain(); }
    const hr = t.closest("[data-hrow]");
    if (hr) { histOpen = histOpen === hr.dataset.hrow ? null : hr.dataset.hrow; return renderMain(); }
    const dt = t.closest("[data-detail]");
    if (dt) return setActive(dt.dataset.detail, null, true);
    const mo = t.closest("[data-msg]");
    if (mo) { const k = mo.dataset.msg; openMsgs.has(k) ? openMsgs.delete(k) : openMsgs.add(k); return renderMain(); }
    const big = t.closest("[data-big]");
    if (big && !t.closest("button")) return setActive(big.dataset.big);
    const qs_ = t.closest("[data-qstep]");
    if (qs_) { const [id, n] = qs_.dataset.qstep.split(/:(?=\d+$)/); qStep[id] = +n; return renderMain(); }
    const pk = t.closest("[data-pick]");
    if (pk) { const [qi, oi] = pk.dataset.pick.split(":").map(Number); const it = findItem(pk.dataset.id); return it && pick(it, qi, oi); }
    const al = t.closest("[data-always]");
    if (al) { const it = findItem(al.dataset.id); return it && respond(it, { behavior: "allow_always", permission: it.suggestions[+al.dataset.always] }); }
    const actEl = t.closest("[data-act]");
    const act = actEl?.dataset.act;
    if (act === "next") return goNext();
    if (act === "test-notify") return invoke("test_notification").then(toast).catch((e) => toast(`Couldn't send: ${e}`));
    if (act === "open-history") { view = "history"; sheet = null; return renderMain(); }
    if (act === "open-search") return openSearch();
    if (act === "load-older") return loadOlder(actEl.dataset.sid);
    if (act === "load-transcript") return loadOlder(actEl.dataset.sid, true);
    if (act === "open-settings") { sheet = sheet === "settings" ? null : "settings"; extSections = null; return renderMain(); }
    const it = actEl?.dataset.id ? findItem(actEl.dataset.id) : null;
    if (act && it) {
      if (it.status === "pending" && act !== "dismiss") active = { id: it.id, sid: it.session_id };
      if (act === "submit-answers") { const a = answersFor(it); return a && respond(it, { behavior: "allow", answers: a }); }
      if (act === "allow") return allow(it);
      if (act === "deny") return deny(it);
      if (act === "send") return submitText(it);
      if (act === "commit") return invoke("reply", { id: it.id, text: "Commit", images: [] }).then((r) => toast(`“Commit” ${r}`)).catch((e) => toast(`Couldn't send: ${e}`));
      if (act === "continue") return invoke("reply", { id: it.id, text: "continue", images: [] }).then((r) => toast(`“continue” ${r}`)).catch((e) => toast(`Couldn't send: ${e}`));
      if (act === "go") return goTo(it);
      if (act === "dismiss") { cleared.add(it.id); return invoke("dismiss", { id: it.id }); }
      if (act === "menu") { menuFor = menuFor === it.id ? null : it.id; return renderMain(); }
      if (act === "redirect") { redirectFor = it.id; renderMain(); return document.querySelector(`[data-text="${CSS.escape(it.id)}"]`)?.focus(); }
    }
  });
  app.addEventListener("input", (e) => {
    if (e.target.matches?.("[data-hsession]")) { histSession = e.target.value; histOpen = null; return renderMain(); }
    if (e.target.matches?.("[data-quick-edit]")) { qpEdit = [...document.querySelectorAll("[data-quick-edit]")].map((i) => i.value); return; }
    const id = e.target.dataset?.text;
    if (!id) return;
    draft(id).text = e.target.value;
    if (id === "search") return runSearch(e.target.value);
    if (id === "find-live") liveSel = 0;
    if (id === "find-sessions" || id === "find-live") return renderMain();
    saveDrafts();
    if (cmdShut === id) cmdShut = null;
    cmdSel = 0;
    refreshCmd(id);
    grow(e.target);
    const b = e.target.closest(".composer")?.querySelector(".btn.send");
    if (b && !b.dataset.held) b.disabled = !e.target.value.trim() && !draft(id).images.length;
  });
  // Images: 📎 picks a file; paste and drag-and-drop work on any box that takes them.
  app.addEventListener("paste", async (e) => {
    const key = e.target.closest?.("[data-drop]")?.dataset.drop;
    if (!key) return;
    const cd = e.clipboardData;
    const files = [...(cd?.items || [])].filter((i) => i.kind === "file" && i.type.startsWith("image/")).map((i) => i.getAsFile()).filter(Boolean);
    if (files.length) { e.preventDefault(); return attach(key, files); }
    // The web view sometimes hides a pasted image: when there's no text either, ask macOS directly.
    if (!cd?.getData("text/plain")) {
      e.preventDefault();
      const data = await invoke("clipboard_image");
      if (data) { draft(key).images.push({ name: "pasted image", mime: "image/png", data }); renderMain(); document.querySelector(`[data-text="${CSS.escape(key)}"]`)?.focus(); }
    }
  });
  app.addEventListener("dragover", (e) => { if (e.target.closest?.("[data-drop]")?.dataset.drop) e.preventDefault(); });
  app.addEventListener("drop", (e) => {
    const key = e.target.closest?.("[data-drop]")?.dataset.drop;
    if (!key) return;
    e.preventDefault();
    attach(key, [...(e.dataTransfer?.files || [])].filter((f) => f.type.startsWith("image/")));
  });
  app.addEventListener("change", (e) => {
    if (e.target.matches("[data-keep-custom]")) setSetting("history.keep", Math.max(10, Math.round(+e.target.value || 0)));
    if (e.target.matches("[data-usage-file]")) setSetting("usage.file", e.target.value.trim());
    if (e.target.matches("[data-quick-edit]")) { const list = [...document.querySelectorAll("[data-quick-edit]")].map((i) => i.value.trim()).filter(Boolean); qpEdit = null; setSetting("quick.phrases", list); }
  });
  // Coming back to Cue: the cursor goes to the text box unless you were somewhere else in it.
  window.addEventListener("focus", () => { if (!sheet && !lightbox && document.activeElement === document.body) focusComposer(); });
  document.addEventListener("keydown", (e) => {
    // The "/" menu under a box: arrows move, Tab fills, Enter runs, Esc closes it.
    const mk = e.target.dataset?.text;
    if (mk && !e.isComposing && ["ArrowUp", "ArrowDown", "Tab", "Enter", "Escape"].includes(e.key) && cmdMatches(mk).length && !e.shiftKey) {
      const m = cmdMatches(mk);
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") { cmdShut = mk; return refreshCmd(mk); }
      if (e.key === "ArrowUp" || e.key === "ArrowDown") { cmdSel = (cmdSel + (e.key === "ArrowDown" ? 1 : m.length - 1)) % m.length; return refreshCmd(mk); }
      return pickCmd(mk, m[cmdSel].name, e.key === "Enter");
    }
    if (renaming && e.key === "Escape") { renaming = null; return renderMain(); }
    // Esc in the New session form closes just the form (and back to the list), not the whole drop-down.
    if (starOpen && e.key === "Escape") { starOpen = false; return renderMain(); }
    if (liveOpen && e.key === "Escape") {
      if (newOpen && e.target.closest?.(".nf")) { newOpen = false; renderMain(); return document.querySelector('[data-text="find-live"]')?.focus(); }
      liveOpen = newOpen = false;
      return renderMain();
    }
    // ⌘, opens Settings, the Mac way (even while typing in a box).
    if (e.metaKey && e.key.toLowerCase() === "f") { e.preventDefault(); return sheet === "search" ? (sheet = null, renderMain()) : openSearch(); }
    if (e.metaKey && e.key === ",") { e.preventDefault(); sheet = sheet === "settings" ? null : "settings"; return renderMain(); }
    // ⌘K: every live session in the middle of the window, like Spotlight. ⌘L: the same, under its chip.
    if (e.metaKey && e.key.toLowerCase() === "k") { e.preventDefault(); sheet = lightbox = null; return lvAct("toggle", { spot: true }); }
    // ⌘L: every live session (the drop-down by "N live"), even while typing in a box.
    if (e.metaKey && e.key.toLowerCase() === "l") { e.preventDefault(); return lvAct("toggle", {}); }
    if (lightbox) {
      if (e.key === "ArrowRight") lightbox.i = (lightbox.i + 1) % lightbox.srcs.length;
      else if (e.key === "ArrowLeft") lightbox.i = (lightbox.i - 1 + lightbox.srcs.length) % lightbox.srcs.length;
      else if (e.key === "Escape") lightbox = null;
      e.preventDefault();
      return renderMain();
    }
    if (e.key === "Escape" && btwFor) { btwFor = null; return renderMain(); }
    if (e.key === "Escape") {
      // In History, Esc just closes the open row.
      if (view === "history") { histOpen = null; return renderMain(); }
      // Esc twice on a working session stops it (once only arms it, so a stray Esc can't).
      const s = !sheet && !menuFor && !redirectFor ? sessionOf(active?.sid) : null;
      // Stopping it is a pause to say something else, so the cursor stays in (or goes to) its box.
      if (s?.state === "working") {
        if (Date.now() - escArmed < 1500) { escArmed = 0; interrupt(s.session_id); }
        else { escArmed = Date.now(); toast(`Esc again to stop ${s.project}`); }
        renderMain();
        return focusComposer();
      }
      sheet = null; menuFor = redirectFor = null; document.activeElement?.blur(); return renderMain();
    }
    const typing = e.target.matches?.("input, textarea, select");
    if (typing) {
      if (e.target.dataset.text === "search" && (e.key === "ArrowDown" || e.key === "ArrowUp")) { e.preventDefault(); return stepSearch(e.key === "ArrowDown" ? 1 : -1); }
      if (e.target.dataset.text === "find-live" && (e.key === "ArrowDown" || e.key === "ArrowUp")) { e.preventDefault(); return stepLive(e.key === "ArrowDown" ? 1 : -1); }
      if (e.target.dataset.text === "find-live" && e.key === "Enter") { e.preventDefault(); return pickLive(); }
      if (e.key === "Enter" && !e.shiftKey && e.target.dataset.text) {
        e.preventDefault();
        const key = e.target.dataset.text;
        if (key.startsWith("btw:")) return askBtw(key.slice(4), draft(key).text);
        if (key === "fwd") return sendForward();
        if (key === "new-cwd" || key === "new-name" || key === "new-msg") return lvAct("start", {});
        if (key === "search") return pickResult(searchSel);
        if (key.startsWith("rename:")) return submitRename(key.slice(7));
        if (key.startsWith("s:")) {
          const sid = key.slice(2);
          const turn = state.items.find((i) => i.session_id === sid && i.kind === "waiting");
          // ⌘↵ with nothing new in the box: the message already queued goes now.
          const fresh = draft(key).text.trim() || draft(key).images.length;
          if ((e.ctrlKey || e.metaKey) && !fresh && sessionOf(sid)?.queued) return sendQueuedNow(sid);
          return turn ? reply(turn) : sendTo(sid, e.ctrlKey || e.metaKey);
        }
        const it = findItem(key);
        if (it) submitText(it);
      }
      return;
    }
    // ⌘↵ outside the box: the open session's queued message goes now.
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      const c = current(), sid = c?.s?.session_id || c?.it?.session_id;
      if (sid && sessionOf(sid)?.queued) { e.preventDefault(); return sendQueuedNow(sid); }
    }
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.key === "/") { e.preventDefault(); return openSearch(); }   // like GitHub and Gmail (only when not typing)
    if (e.key === "h") { view = view === "history" ? "board" : "history"; return renderMain(); }
    if (e.key === "l") return lvAct("toggle", {});
    if (e.key === ",") { sheet = sheet === "settings" ? null : "settings"; return renderMain(); }
    if (sheet) return;
    if (e.key === "n") return goNext();
    // ← → step through the chip you clicked (Waiting if you haven't).
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") { e.preventDefault(); return openFromChip(chipNav || "waiting", e.key === "ArrowRight" ? 1 : -1); }
    const it = current()?.it;
    if (!isPending(it)) {
      // Nothing pending on screen: R still jumps to the message box.
      if (e.key === "r") { e.preventDefault(); document.querySelector(".active-pane textarea[data-text]")?.focus(); }
      return;
    }
    if (e.key === "a" && it.kind === "permission") allow(it);
    else if (e.key === "d" && it.kind === "permission") deny(it);
    else if (e.key === "g") goTo(it);
    else if (e.key === "r") { e.preventDefault(); if (it.kind !== "waiting" && !inTerminal(it)) redirectFor = it.id; renderMain(); document.querySelector(`[data-text="${CSS.escape(it.id)}"]`)?.focus(); }
    else if (/^[1-9]$/.test(e.key) && it.kind === "question") pick(it, stepOf(it), +e.key - 1);
  });
}

// ---------- boot ----------
const LINGER_MS = 3000;
/** A "✓ answered" card's time is up: it fades and folds away, and the cards below close the gap. */
function leaveGhosts() {
  const gone = [...ghosts].filter(([, g]) => Date.now() - g.at >= LINGER_MS).map(([id]) => id);
  if (!gone.length) return;
  const finish = () => { gone.forEach((id) => ghosts.delete(id)); render(); };
  const els = gone.map((id) => document.querySelector(`.nrow.ghost[data-detail="${CSS.escape(id)}"]`)).filter(Boolean);
  if (!els.length || matchMedia("(prefers-reduced-motion: reduce)").matches) return finish();
  for (const el of els) {
    el.style.overflow = "hidden";
    // Down to nothing, eating the column's 10px gap too, so nothing below jumps at the end.
    el.animate([{ height: `${el.offsetHeight}px`, opacity: 0.6 }, { height: "0px", opacity: 0, paddingTop: "0px", paddingBottom: "0px", marginBottom: "-10px" }], { duration: 260, easing: "ease-in", fill: "forwards" });
  }
  setTimeout(finish, 270);
}
let setupShown = false;
function setState(s) {
  // Anything that just left Waiting lingers there as "✓ answered" for a moment instead of vanishing.
  const ids = new Set(s.items.map((i) => i.id));
  for (const it of state.items) if (!ids.has(it.id)) ghosts.set(it.id, { it, at: Date.now() });
  for (const [id, g] of ghosts) if (Date.now() - g.at > LINGER_MS || ids.has(id)) ghosts.delete(id);
  if (ghosts.size) setTimeout(leaveGhosts, LINGER_MS + 50);
  state = s;
  for (const [sid, on] of starNow) if (!!sessionOf(sid)?.starred_ms === on || !sessionOf(sid)) starNow.delete(sid);   // Cue says so now
  // First launch: an installed agent isn't connected yet, and you haven't closed the setup screen.
  if (!setupShown && s.settings && !s.settings.setup?.done && ["claude", "codex", "pi"].some((h) => s.connections?.[h]?.present && !s.connections[h].ok)) { setupShown = true; sheet = "setup"; }
  for (let n = outbox.length - 1; n >= 0; n--) if (outbox[n].via && landed(outbox[n])) outbox.splice(n, 1);
  if (s.now_ms) clockSkew = s.now_ms - Date.now();
  applyTheme();
  render();
}
const render = () => renderMain();

async function boot() {
  await loadDrafts();
  bindMain();
  setState(await invoke("get_state"));
  restoreSpot();
  await T.event.listen("state", (e) => setState(e.payload));
  await T.event.listen("dictation", (e) => onDictation(e.payload));
  await T.event.listen("select", (e) => {
    if (e.payload !== "usage") return setActive(e.payload);
    const s = limitedSessions().sort((a, b) => a.since_ms - b.since_ms)[0];
    if (s) setActive(null, s.session_id);
  });
  await T.event.listen("server-error", (e) => toast(`Cue can't listen: ${e.payload}`));
  moveOldParked();
  moveOldStars();
  // Which Cue this is (for Settings), without asking GitHub.
  T.app?.getVersion().then((v) => { upd = { ...upd, current: v }; }, () => {});
  await T.event.listen("update-ready", (e) => { upd = { ...upd, version: e.payload.version, notes: e.payload.notes || "", status: "found", offer: true }; renderMain(); });
  setInterval(render, 15000); // keep ages and bars moving
  // The open session's steps: every 0.5 s while it works, else every 2 s (only new lines are read;
  // nothing new answers with just a version).
  setInterval(() => {
    const cur = current();
    const q = quietOpen && (state.live || []).find((x) => x.session_id === quietOpen);
    const sid = q ? q.session_id : cur?.s?.session_id || cur?.it?.session_id;
    if (!sid || document.hidden) return;
    const f = stepFeeds.get(sid);
    const working = q ? q.status === "busy" : cur?.s?.state === "working";
    // Not "working" can still be working: a stop hook or a finished background task sends it back to
    // work without a new prompt. So it's checked every 2 s anyway (nothing new costs next to nothing).
    if (!f || working || Date.now() - f.at > 2000) loadSteps(sid);
  }, 500);
  // The live "what it's doing · 3s" counts every second, without redrawing everything.
  setInterval(() => document.querySelectorAll("[data-ago]").forEach((el) => { el.textContent = ago(+el.dataset.ago); }), 1000);
}
boot();
