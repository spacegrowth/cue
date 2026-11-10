// ---------- agents: a session's teammates and background agents, as one ----------
// Two kinds of agent work for a session: a background agent (Claude Code's Agent tool) and a teammate (an agent
// started with a name, that stays for more). They show the same way everywhere: one list per session (agentsOf),
// one set of states (agentState), one row (agentRow) in the same places (the row above the box, the Agents tab, the
// title line, the board's count). (Background scripts are not agents: they keep their own rows and the Scripts tab.)

/** The words for the six states, and the look of each (the chip and dot classes the rest of the UI uses). */
const AGENT_WORDS = { needs: "needs you", working: "working", stuck: "stuck", done: "done", idle: "idle", stopped: "stopped" };
const AGENT_CLS = { needs: "need", working: "busy", stuck: "stuck", done: "done", idle: "", stopped: "" };
/** One state for any agent: needs · working · stuck (working but quiet) · done · idle (free to take more) · stopped.
 *  `x` is a helper from the steps feed ({ running, stuck, teammate, stopped }). */
function agentState(x) {
  if (x.asks) return "needs";
  if (x.running) return x.stuck ? "stuck" : "working";
  return x.teammate ? "idle" : x.stopped ? "stopped" : "done";
}
const agentLive = (a) => a.state === "working" || a.state === "stuck";

/** A session's agents, every kind, in one shape: { key, id, sid (whose), kind: agent|teammate, type (the word
 *  after the name: "Explore", "fork", "teammate"), name, state, started_ms, ended_ms, tokens, latest, ask, h (a helper) }.
 *  `helpers`: the steps feed's (the Agents tab passes the earlier ones too). */
function agentsOf(sid, helpers = helpersOf(sid)) {
  const asks = helperAsks(sid);
  // Its crew isn't here: a hosted lead's named agents are its crew, in the crew strip (one place each, never two).
  const crew = new Set((sessionOf(sid)?.team || []).map((t) => t.agent_id).filter(Boolean));
  return helpers.filter((h) => !h.script && !crew.has(h.id)).map((h) => helperAgent(sid, h, asks.find((i) => i.agent_id === h.id)));
}
/** What a session's background agents and teammates ask you: its pending asks that came from inside one of them
 *  (Claude Code names the agent on the hook: the item's agent_id, the helper's id). */
const helperAsks = (sid) => (state.items || []).filter((i) => i.session_id === sid && i.agent_id && i.kind !== "waiting");
/** A teammate or background agent of session `sid`, as an agent; `ask`: what it asks you, if it does. */
const helperAgent = (sid, h, ask = helperAsks(sid).find((i) => i.agent_id === h.id)) => ({
  key: `h:${h.id}`, id: h.id, sid, kind: h.teammate ? "teammate" : "agent", type: h.teammate ? "crew" : h.kind || "agent",
  name: bgName(h), state: agentState({ ...h, asks: !!ask }), started_ms: h.started_ms, ended_ms: h.finished_ms || h.last_ms || h.started_ms,
  tokens: h.tokens || 0, latest: bgLatest(h), ask: ask || null, h,
});
/** Grouped by what matters to you: needs you (oldest ask first), working (stuck too; oldest first), done (done, idle,
 *  stopped; newest first). */
function agentGroups(list) {
  const needs = list.filter((a) => a.state === "needs").sort((a, b) => (a.ask?.created_ms || 0) - (b.ask?.created_ms || 0));
  const working = list.filter(agentLive).sort((a, b) => a.started_ms - b.started_ms);
  const done = list.filter((a) => a.state !== "needs" && !agentLive(a)).sort((a, b) => b.ended_ms - a.ended_ms);
  return { needs, working, done };
}
/** The row above the box: what needs you, then what's at work; at most AGENT_ROWS, and how many more. */
const AGENT_ROWS = 4;
function agentTrayRows(list) {
  const g = agentGroups(list), all = [...g.needs, ...g.working];
  return { rows: all.slice(0, AGENT_ROWS), more: Math.max(0, all.length - AGENT_ROWS) };
}
/** The title line's: a chip for each agent that needs you. */
const agentChipsOf = (list) => ({ needs: agentGroups(list).needs });
/** How many agents a session has, and which need you: the one count every place says (the title's "N agents ›", the
 *  Agents tab, its cards). Its background agents and teammates as Cue counts them from the list its chat shows (the
 *  snapshot's `agents`: the live list, not the earlier ones Show earlier adds). `needs`: the names of those asking you. */
/** The names of a session's own agents (background agents, teammates) asking you. */
const helperNeeds = (sid) => helperAsks(sid).map((i) => i.agent_name || "an agent");
function agentCount(sid) {
  // Its crew at work is counted in its strip, not here.
  const s = sessionOf(sid), crew = (s?.team || []).filter((t) => t.state === "working").length;
  return { n: Math.max(0, (s?.agents || 0) - crew), needs: helperNeeds(sid) };
}

// ---------- how they look ----------
/** Its mark: an agent A on moss, a teammate a person in a ring. */
const agentMark = (a) => bgIcon(a.h);
/** How long, or how it ended: "4m 12s", "done · 2m", "idle", "stopped". `times` (the Agents tab): with clock times. */
function agentWhen(a, times) {
  const h = a.h, live = agentLive(a);
  const span = live ? `<span data-helper-span="${h.started_ms}">${helperSpan(h)}</span>` : helperSpan(h);
  if (!times) return live || a.state === "needs" ? span : a.state === "idle" ? "idle" : `${AGENT_WORDS[a.state]} · ${span}`;
  return `started ${bgClock(h.started_ms)} · ${live ? `running ${span}` : a.state === "idle" ? "idle" : `${AGENT_WORDS[a.state]} ${bgClock(h.finished_ms || h.last_ms || h.started_ms)} · took ${span}`}`;
}
/** Where a click on it goes: it opens in full in place of the chat. */
const agentGo = (a) => `data-helper="${esc(`${a.sid}|${a.id}`)}"`;
/** One agent, one row, any kind: its mark, its name, its kind and how long in grey, the live dot (amber when stuck),
 *  and its newest step. Needs you: what it asks, with Deny / Allow for a permission. Else ›. */
function agentRow(a, times = false) {
  const tok = times ? helperTokens(a.tokens) : "";
  const quiet = a.state === "stuck" ? ` · ${helperQuiet(a.h)}` : a.h?.quiet === "tool" ? ` · ${helperLongRun(a.h)}` : "";
  const dot = agentLive(a) ? `<span class="dot-live ${a.state === "stuck" ? "stuck" : ""}"></span>` : "";
  const meta = `<span class="bg-meta">· ${esc(a.type)} · ${agentWhen(a, times)}${tok ? ` · ${tok}` : ""}${quiet}</span>`;
  const head = `${agentMark(a)}<span class="bg-name">${esc(a.name)}</span>`;
  if (a.state === "needs") {
    const ask = a.ask;
    const what = ask ? `${esc(verb(ask))} <span class="${isBash(ask) ? "mono" : ""}">${esc(plain(summary(ask)))}</span>` : esc("needs you");
    const acts = ask?.kind === "permission" ? `<span class="ag-acts"><button class="btn small deny" data-act="deny" data-id="${esc(ask.id)}">Deny</button><button class="btn small primary" data-act="allow" data-id="${esc(ask.id)}">Allow</button></span>` : `<span class="stx-chev">›</span>`;
    return `<div class="stx-line bg-row ag-row need" data-key="${esc(a.key)}" role="button" ${ask ? `data-big="${esc(ask.id)}"` : agentGo(a)}>${head}<span class="bg-meta">· ${esc(a.type)} · <span class="ag-need">${esc(AGENT_WORDS.needs)}</span></span><span class="bg-tail ag-ask">${what}</span>${acts}</div>`;
  }
  // Stuck: its session is asked to wake it (Resume) or stop it, as you'd type it there.
  const wake = a.state === "stuck" ? `<span class="ag-acts">${["resume", "stop"].map((w) => `<button class="btn small ${w === "stop" ? "deny" : ""}" data-ag-wake="${w}" data-ag-sid="${esc(a.sid)}" data-ag-id="${esc(a.id)}" data-ag-name="${esc(a.name)}">${w === "stop" ? "Stop" : "Resume"}</button>`).join("")}</span>` : "";
  const acts = wake || `<span class="stx-chev">›</span>`;
  const idle = a.state === "idle" || a.state === "stopped";
  return `<div class="stx-line bg-row ag-row ${idle ? "idle" : ""}" data-key="${esc(a.key)}" role="button" ${agentGo(a)}>${head}${meta}${dot}<span class="bg-tail">${esc(a.latest)}</span>${acts}</div>`;
}
/** Resume or Stop on a stuck agent's row: its session is asked, at once (its turn waits on this very agent). */
function agentWake(d) {
  const text = d.agWake === "stop"
    ? `Stop your background agent “${d.agName}” now (TaskStop, task id ${d.agId}), and say so.`
    : `Your background agent “${d.agName}” has gone quiet. Ask it where it is and to carry on, with SendMessage (to: "${d.agId}").`;
  return invoke("send_to_session", { sessionId: d.agSid, text, images: [], now: true }).then(() => toast(d.agWake === "stop" ? `Asked its session to stop ${d.agName}` : `Asked its session to wake ${d.agName}`), (e) => toast(`Couldn't ask its session: ${e}`));
}
/** Above the message box: the session's agents that need you or are at work, any kind, at most AGENT_ROWS (then
 *  "N more · Agents ›"); then its scripts still running. "" when nothing is. */
function agentTray(sid) {
  // A hosted lead's teammates have their rows in its team (team.js), not here too.
  const mates = new Set(mateList(sid).map((t) => t.agent_id));
  const { rows, more } = agentTrayRows(agentsOf(sid).filter((a) => !mates.has(a.id)));
  const scripts = helpersOf(sid).filter((h) => h.script && helperRunning(h)).sort((a, b) => a.started_ms - b.started_ms).map((h) => bgRow(sid, h)).join("");
  if (!rows.length && !scripts) return "";
  const head = rows.length ? `<div class="ag-head" data-key="ag-head"><span>AGENTS</span><span class="grow"></span>${more ? `<button class="ag-more" data-bg-tab="agents" data-sid="${esc(sid)}">${more} more · Agents ›</button>` : ""}</div>` : "";
  return `<div class="bg-rows" data-key="bg-rows">${head}${rows.map((a) => agentRow(a)).join("")}${scripts}</div>`;
}
/** The Agents tab's page: every agent of the session, any kind, grouped (needs you, working, done), with clock times
 *  and tokens. `helpers`: the live feed's and the earlier ones read from its history. */
function agentsPage(sid, helpers) {
  const g = agentGroups(agentsOf(sid, helpers));
  const part = (label, list) => list.length ? `<div class="ag-group">${label}</div>${list.map((a) => agentRow(a, true)).join("")}` : "";
  return part("NEEDS YOU", g.needs) + part("WORKING", g.working) + part("DONE", g.done);
}
/** On the title line: a chip for each agent that needs you ("<name> needs you"), then "N agents ›", which opens the
 *  Agents tab. */
function agentChips(sid) {
  // A hosted lead's crew member that needs you, by name (team.js); the crew itself is in its strip above the box.
  const crewNeed = mateChips(sid);
  const { needs } = agentChipsOf(agentsOf(sid)), { n } = agentCount(sid);
  if (!n) return crewNeed ? `<span class="tm-chips">${crewNeed}</span>` : "";
  const need = crewNeed + needs.filter((a) => a.kind !== "teammate").map((a) => `<button class="tm-chip need" ${a.ask ? `data-big="${esc(a.ask.id)}"` : agentGo(a)}><span class="tm-n">${esc(a.name)}</span> <span class="tm-sub">needs you</span></button>`).join("");
  const on = bgTabOn.get(sid) === "agents";
  return `<span class="tm-chips">${need}<button class="tm-chip ${on ? "on" : ""}" data-bg-tab="agents" data-sid="${esc(sid)}">${n} agent${n === 1 ? "" : "s"} <span class="tm-sub">›</span></button></span>`;
}
/** A card's count, any kind: "N agents" (opens its session on the Agents tab), or an agent that needs you by name. */
function agentPill(sid) {
  if (!sid) return "";
  const c = agentCount(sid);
  if (c.needs.length) return `<span class="tm-chip need small"><span class="tm-n">${esc(c.needs[0])}</span> <span class="tm-sub">needs you</span></span>`;
  return c.n ? `<button class="bg-pill" data-bg-open="${esc(sid)}">${c.n} agent${c.n === 1 ? "" : "s"}</button>` : "";
}
/** The top of an agent's own view, any kind: ‹ Back, its mark, its name, its kind tag, its state, and the session
 *  it works for. */
function agentHead(a, back) {
  const forName = nameOf(a.sid, sessionOf(a.sid)?.project);
  const st = a.state === "needs" || agentLive(a) ? `<span class="pill soft ${AGENT_CLS[a.state]}">${agentLive(a) ? `<span class="dot-live ${a.state === "stuck" ? "stuck" : ""}"></span>` : ""}${AGENT_WORDS[a.state]}</span>` : `<span class="dim">${AGENT_WORDS[a.state]}</span>`;
  return `<div class="bg-view-head ag-view-head" data-key="head">${back}${agentMark(a)}<b class="ag-view-name">${esc(a.name)}</b><span class="role ag-kind">${esc(a.type.toUpperCase())}</span>${st}<button class="tm-chip" data-helper-back="1"><span class="tm-n">for ${esc(forName)}</span> <span class="tm-sub">›</span></button></div>`;
}
