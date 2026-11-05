// ---------- agents: a session's teammates, background agents and relay agents, as one ----------
// Three kinds of agent work for a session: a background agent (Claude Code's Agent tool), a teammate (an agent
// started with a name, that stays for more), and a relay agent (a lead's executor: a full session of its own).
// They show the same way everywhere: one list per session (agentsOf), one set of states (agentState), one row
// (agentRow) in the same places (the row above the box, the Agents tab, the title line, the board's count).
// The one difference: a relay agent is a session, so it opens as its chat (it has a box) and, done, has Diff and
// Review on its row. (Background scripts are not agents: they keep their own rows and the Scripts tab.)

/** The words for the six states, and the look of each (the chip and dot classes the rest of the UI uses). */
const AGENT_WORDS = { needs: "needs you", working: "working", stuck: "stuck", done: "done", idle: "idle", stopped: "stopped" };
const AGENT_CLS = { needs: "need", working: "busy", stuck: "stuck", done: "done", idle: "", stopped: "" };
/** One state for any agent: needs · working · stuck (working but quiet) · done · idle (free to take more) · stopped.
 *  `x` is a helper from the steps feed ({ running, stuck, teammate, stopped }), or a relay agent
 *  ({ relay: true, status, asks, quiet }: relay's status, whether it's asking you, whether its session went quiet). */
function agentState(x) {
  if (x.asks) return "needs";
  if (x.relay) {
    if (x.status === "reported" || x.status === "idle") return "done";
    if (x.status === "busy") return x.quiet ? "stuck" : "working";
    if (x.status === "stalled") return "stuck";
    if (x.status === "closed" || x.status === "superseded" || x.status === "dead") return "stopped";
    return "idle";
  }
  if (x.running) return x.stuck ? "stuck" : "working";
  return x.teammate ? "idle" : x.stopped ? "stopped" : "done";
}
const agentLive = (a) => a.state === "working" || a.state === "stuck";

/** A session's agents, every kind, in one shape: { key, id, sid (whose), kind: agent|teammate|relay, type (the word
 *  after the name: "Explore", "fork", "teammate", "relay"), name, state, started_ms, ended_ms, tokens, latest, ask,
 *  h (a helper) | e (a relay agent) }. `helpers`: the steps feed's (the Agents tab passes the earlier ones too). */
function agentsOf(sid, helpers = helpersOf(sid)) {
  const asks = helperAsks(sid);
  const out = helpers.filter((h) => !h.script).map((h) => helperAgent(sid, h, asks.find((i) => i.agent_id === h.id)));
  const m = crewOf(sid);
  if (m?.role === "lead") for (const e of m.executors || []) {
    if (e.session_id === sid) continue;
    const s = sessionOf(e.session_id), ask = asksOf(e.session_id) || null;
    const state = agentState({ relay: true, status: e.status, asks: !!ask || !!preStart(e) || !!preStart(s), quiet: !!s?.stuck_ms });
    const now = liveStep(e.session_id);
    out.push({
      key: `r:${e.session_id}`, id: e.session_id, sid, kind: "relay", type: "relay", name: e.name || e.session_id, state,
      started_ms: s?.since_ms || 0, ended_ms: s?.since_ms || 0, tokens: 0,
      latest: now?.what || (state === "done" ? e.outcome : "") || e.goal || "", ask, e,
    });
  }
  return out;
}
/** What a session's background agents and teammates ask you: its pending asks that came from inside one of them
 *  (Claude Code names the agent on the hook: the item's agent_id, the helper's id). */
const helperAsks = (sid) => (state.items || []).filter((i) => i.session_id === sid && i.agent_id && i.kind !== "waiting");
/** A teammate or background agent of session `sid`, as an agent; `ask`: what it asks you, if it does. */
const helperAgent = (sid, h, ask = helperAsks(sid).find((i) => i.agent_id === h.id)) => ({
  key: `h:${h.id}`, id: h.id, sid, kind: h.teammate ? "teammate" : "agent", type: h.teammate ? "teammate" : h.kind || "agent",
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
 *  snapshot's `agents`: the live list, not the earlier ones Show earlier adds), plus a lead's relay agents. `needs`: the
 *  names of those asking you, any kind. */
function agentCount(sid) {
  const m = crewOf(sid), ex = m?.role === "lead" ? (m.executors || []).filter((e) => e.session_id !== sid) : [];
  const helperNeeds = helperAsks(sid).map((i) => i.agent_name || "an agent");
  return { n: (sessionOf(sid)?.agents || 0) + ex.length, needs: [...helperNeeds, ...ex.filter((e) => asksOf(e.session_id)).map((e) => e.name || e.session_id)] };
}

// ---------- how they look ----------
/** Its mark: an agent A on moss, a teammate a person in a ring, a relay agent R on dark. */
const agentMark = (a) => a.kind === "relay" ? `<i class="stx-ic k-relay">R</i>` : bgIcon(a.h);
/** How long, or how it ended: "4m 12s", "done · 2m", "idle", "stopped". `times` (the Agents tab): with clock times. */
function agentWhen(a, times) {
  if (a.kind === "relay") {
    const since = a.started_ms ? `<span data-ago="${a.started_ms}">${ago(a.started_ms)}</span>` : "";
    if (a.state === "done") return [pkt(a.e.packet), "done"].filter(Boolean).join(" · ");
    return agentLive(a) || a.state === "needs" ? since : AGENT_WORDS[a.state];
  }
  const h = a.h, live = agentLive(a);
  const span = live ? `<span data-helper-span="${h.started_ms}">${helperSpan(h)}</span>` : helperSpan(h);
  if (!times) return live || a.state === "needs" ? span : a.state === "idle" ? "idle" : `${AGENT_WORDS[a.state]} · ${span}`;
  return `started ${bgClock(h.started_ms)} · ${live ? `running ${span}` : a.state === "idle" ? "idle" : `${AGENT_WORDS[a.state]} ${bgClock(h.finished_ms || h.last_ms || h.started_ms)} · took ${span}`}`;
}
/** Where a click on it goes: a helper opens in full in place of the chat; a relay agent opens its chat (a remote
 *  one's own terminal too). */
const agentGo = (a) => a.kind === "relay" ? exGo(a.id) : `data-helper="${esc(`${a.sid}|${a.id}`)}"`;
/** One agent, one row, any kind: its mark, its name, its kind and how long in grey, the live dot (amber when stuck),
 *  and its newest step. Needs you: what it asks, with Deny / Allow for a permission. A done relay agent: Diff and
 *  Review. Else ›. */
function agentRow(a, times = false) {
  const tok = times ? helperTokens(a.tokens) : "";
  const quiet = a.kind !== "relay" && a.state === "stuck" ? ` · ${helperQuiet(a.h)}` : a.state === "stuck" ? ` · <span class="bg-quiet">stuck</span>` : "";
  const dot = agentLive(a) ? `<span class="dot-live ${a.state === "stuck" ? "stuck" : ""}"></span>` : "";
  const meta = `<span class="bg-meta">· ${esc(a.type)} · ${agentWhen(a, times)}${tok ? ` · ${tok}` : ""}${quiet}</span>`;
  const head = `${agentMark(a)}<span class="bg-name">${esc(a.name)}</span>`;
  if (a.state === "needs") {
    const ask = a.ask;
    const what = ask ? `${esc(verb(ask))} <span class="${isBash(ask) ? "mono" : ""}">${esc(plain(summary(ask)))}</span>` : esc(preStart(a.e) || preStart(sessionOf(a.id)) || "needs you");
    const acts = ask?.kind === "permission" ? `<span class="ag-acts"><button class="btn small deny" data-act="deny" data-id="${esc(ask.id)}">Deny</button><button class="btn small primary" data-act="allow" data-id="${esc(ask.id)}">Allow</button></span>` : `<span class="stx-chev">›</span>`;
    return `<div class="stx-line bg-row ag-row need" data-key="${esc(a.key)}" role="button" ${ask ? `data-big="${esc(ask.id)}"` : agentGo(a)}>${head}<span class="bg-meta">· ${esc(a.type)} · <span class="ag-need">${esc(AGENT_WORDS.needs)}</span></span><span class="bg-tail ag-ask">${what}</span>${acts}</div>`;
  }
  const relayDone = a.kind === "relay" && a.state === "done";
  const acts = relayDone ? `<span class="ag-acts">${diffBtn(a.id, a.e)}${reviewWord(a.id) ? `<span class="dim">${esc(reviewWord(a.id))}</span>` : `${missedWake(a.id) ? `<span class="dim">not picked up</span>` : ""}${crewBtn(a.id, "review", "Review", true)}`}</span>` : `<span class="stx-chev">›</span>`;
  const idle = a.state === "idle" || a.state === "stopped";
  return `<div class="stx-line bg-row ag-row ${idle ? "idle" : ""}" data-key="${esc(a.key)}" role="button" ${agentGo(a)}>${head}${meta}${dot}<span class="bg-tail">${esc(a.latest)}</span>${acts}</div>`;
}
/** Above the message box: the session's agents that need you or are at work, any kind, at most AGENT_ROWS (then
 *  "N more · Agents ›"); then its scripts still running. "" when nothing is. */
function agentTray(sid) {
  const { rows, more } = agentTrayRows(agentsOf(sid));
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
 *  Agents tab. A relay agent's own chat has its RELAY tag and "for <lead> ›" instead (teamChips). */
function agentChips(sid) {
  const { needs } = agentChipsOf(agentsOf(sid)), { n } = agentCount(sid);
  if (!n) return "";
  const need = needs.map((a) => `<button class="tm-chip need" ${a.ask ? `data-big="${esc(a.ask.id)}"` : agentGo(a)}><span class="tm-n">${esc(a.name)}</span> <span class="tm-sub">needs you</span></button>`).join("");
  const on = bgTabOn.get(sid) === "agents";
  return `<span class="tm-chips">${need}<button class="tm-chip ${on ? "on" : ""}" data-bg-tab="agents" data-sid="${esc(sid)}">${n} agent${n === 1 ? "" : "s"} <span class="tm-sub">›</span></button></span>`;
}
/** A card's count, any kind: "N agents" (opens its session on the Agents tab), or a relay agent that needs you by
 *  name. `count`: false on a lead's card, whose crew line has the count. */
function agentPill(sid, count = true) {
  if (!sid) return "";
  const c = agentCount(sid);
  if (c.needs.length) return `<span class="tm-chip need small"><span class="tm-n">${esc(c.needs[0])}</span> <span class="tm-sub">needs you</span></span>`;
  return count && c.n ? `<button class="bg-pill" data-bg-open="${esc(sid)}">${c.n} agent${c.n === 1 ? "" : "s"}</button>` : "";
}
/** The top of an agent's own view, any kind: ‹ Back, its mark, its name, its kind tag, its state, and the session
 *  it works for. */
function agentHead(a, back) {
  const forName = nameOf(a.sid, sessionOf(a.sid)?.project);
  const st = a.state === "needs" || agentLive(a) ? `<span class="pill soft ${AGENT_CLS[a.state]}">${agentLive(a) ? `<span class="dot-live ${a.state === "stuck" ? "stuck" : ""}"></span>` : ""}${AGENT_WORDS[a.state]}</span>` : `<span class="dim">${AGENT_WORDS[a.state]}</span>`;
  return `<div class="bg-view-head ag-view-head" data-key="head">${back}${agentMark(a)}<b class="ag-view-name">${esc(a.name)}</b><span class="role ag-kind">${esc(a.type.toUpperCase())}</span>${st}${sessionOf(a.sid) && a.kind === "relay" ? `<button class="tm-chip" data-session="${esc(a.sid)}"><span class="tm-n">for ${esc(forName)}</span> <span class="tm-sub">›</span></button>` : `<button class="tm-chip" data-helper-back="1"><span class="tm-n">for ${esc(forName)}</span> <span class="tm-sub">›</span></button>`}</div>`;
}
/** A relay agent's own chat starts its head as any agent's view does: ‹ Back (to its lead's Agents tab, when Cue has
 *  its lead) and its R mark. "" for any other session. */
const relayBack = (sid) => { const t = sid && execOf(sid) && teamOf(sid); return t && sessionOf(t.lead) ? `<button class="bg-back" data-agent-back="${esc(t.lead)}"><span class="bg-back-ch">‹</span>Back</button>` : ""; };
const relayMark = (sid) => sid && execOf(sid) ? `<i class="stx-ic k-relay">R</i>` : "";

document.addEventListener("click", (e) => {
  // ‹ Back from a relay agent's chat: its lead, on its Agents tab.
  const back = e.target.closest?.("[data-agent-back]");
  if (!back) return;
  e.stopPropagation();
  const lead = back.dataset.agentBack;
  bgTabOn.set(lead, "agents");
  openSession(lead);
}, true);
