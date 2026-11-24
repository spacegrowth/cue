// Cue UI chat — background work: a session's agents, teammates and scripts, and their full views.
// ---------- background work: agents, teammates and scripts a session runs in the background ----------
// An agent (Claude Code's Agent tool, run in the background) runs inside its session's process, with no
// terminal and no way to message it: not a session, so no card of its own. A teammate is an agent started
// with a name that stays after its task. A script is a shell command run in the background. Each is one row
// (bgRow): in the list above the message box while it runs, so chatting on doesn't scroll it away; under
// the step that started it once it's over; and all of it in the session's Agents and Scripts tabs (background.js).
let helperView = null;          // { sid, id }: the agent whose full view fills the chat area (helperPane); id null: the newest still running, once its feed is in
const helpersOf = (sid) => stepFeeds.get(sid)?.helpers || [];
const helperRunning = (h) => !!h.running;
/** "1m 12s" of work: until it finished (an agent whose log ended without its notice: its last step), or so far. */
const helperSpan = (h) => { const ms = (h.finished_ms || (h.running === false && h.last_ms) || Date.now()) - h.started_ms; const m = Math.floor(ms / 60000), sec = Math.floor((ms % 60000) / 1000); return m ? `${m}m ${sec}s` : `${sec}s`; };
const helperModel = (h) => (h.model || "").replace(/^claude-/, "").replace(/-\d.*$/, "");
/** "12k tokens" an agent has used so far; "" until it has used any (a script uses none). */
const helperTokens = (n) => !(n > 0) ? "" : n < 1000 ? `${n} tokens` : n < 1e6 ? `${(n / 1000).toFixed(n < 10000 ? 1 : 0)}k tokens` : `${(n / 1e6).toFixed(1)}M tokens`;
/** Quiet (src-tauri/src/sessions/quiet.rs, as the feed judged it): stuck (nothing open, nothing said), shown amber
 *  with how long; or a long tool call open ("running npm test for 20m"), left to it. Minutes since it last did
 *  anything (a long run reads as its step: "Running: npm test for 20m"). */
const helperStuck = (h) => helperRunning(h) && !!h.stuck;
const helperMins = (h) => Math.max(1, Math.floor((Date.now() - (h.last_ms || h.started_ms)) / 60000));
const helperQuiet = (h) => `<span class="bg-quiet">quiet for ${helperMins(h)}m — looks stuck</span>`;
const helperLongRun = (h) => `${esc(h.quiet_what || "running a tool")} for ${helperMins(h)}m`;
/** The live dot: pulsing, or amber and still when it's quiet. */
const liveDot = (h) => `<span class="dot-live ${helperStuck(h) ? "stuck" : ""}"></span>`;
/** Where an agent's steps end and its reply begins: a finished one's last words are its reply. */
const helperTail = (h) => { let end = h.items.length; if (!helperRunning(h)) while (end && h.items[end - 1].t === "say") end--; return end; };
/** A finished agent's reply, into the session's message box (after what's typed there): nothing is sent,
 *  the text is in the box for you to send. Closes the agent's view. */
function helperReplyToBox(sid, id) {
  const h = helpersOf(sid).find((x) => x.id === id);
  if (!h || helperRunning(h) || h.script) return;
  const reply = h.items.slice(helperTail(h)).map((x) => x.text).join("\n\n").trim();
  if (!reply) return;
  const key = `s:${sid}`, d = draft(key);
  const text = `From the ${h.kind || "agent"} agent (${h.desc}):\n\n${reply}`;
  d.text = d.text.trim() ? `${d.text.replace(/\s+$/, "")}\n\n${text}` : text;
  saveDrafts();
  closeHelperView();
  focusComposer();
}
/** Back from an agent's or script's full view (‹ Back, Esc, "Put reply in the box"): the chat again, at its
 *  end (the newest message), not wherever a fresh chat element happens to start. The draw lands it there
 *  (`chatToEnd`, the path a newly opened chat takes in app/render.js renderMain), then it's pinned again a frame
 *  later and once more when settled: whatever grows after the draw (the box growing to a draft, an image,
 *  a table laid out late) would otherwise leave the end just out of view. */
let chatToEnd = false;    // the next chat draw lands at its end; app/render.js renderMain consumes it
function closeHelperView() {
  helperView = null;
  drawAtChatEnd();
}
/** Draw, landing the chat at its end (and keeping it there while what's below settles). Also after you
 *  send: your message is the newest, wherever you had scrolled to. */
function drawAtChatEnd() {
  chatToEnd = true;
  renderMain();
  chatToEnd = false;
  boxStickToEnd(document.querySelector("[data-chat]"));
}
/** The agent whose full view is open in this session's chat area, if it's still in the feed. Opening
 *  another session's chat ends the view: coming back lands in the chat, not the agent. */
function helperViewed(sid) {
  if (!helperView) return null;
  if (helperView.sid !== sid) { helperView = null; return null; }
  if (!helperView.id) {
    const newest = helpersOf(sid).filter(helperRunning).sort((a, b) => b.started_ms - a.started_ms)[0];
    if (newest) helperView.id = newest.id;
    else if (stepFeeds.has(sid)) helperView = null;   // nothing running any more: the chat
    if (!newest) return null;
  }
  // Not in the live feed: an earlier one opened from the Agents or Scripts tab, once it's read.
  return helpersOf(sid).find((h) => h.id === helperView.id) || bgFull(sid, helperView.id);
}
/** The line Claude Code ends a background command's output file on: "[exited with code N]", or
 *  "[killed]" when it was stopped. */
const SCRIPT_END = /^\[(exited with code -?\d+|killed)\]$/;
/** A script's newest line that says something (not its end line). */
const scriptTail = (h) => [...(h.output || [])].reverse().map((l) => l.trim()).find((l) => l && !SCRIPT_END.test(l)) || "";
/** How a script ended, in a word; "" while it runs. */
const scriptEnd = (h) => helperRunning(h) ? "" : h.stopped ? "stopped" : h.failed ? "failed" : "finished";
/** Its mark, from the step lines' vocabulary: a script $ on dark (as "Ran"), an agent A on moss, a teammate a
 *  person in a ring. */
const PERSON_ICON = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="8" r="4"/><path d="M4 21c1.5-4 4.5-6 8-6s6.5 2 8 6"/></svg>`;
const bgIcon = (h) => h.script ? `<i class="stx-ic k-run">$</i>` : h.teammate ? `<i class="stx-ic k-team">${PERSON_ICON}</i>` : `<i class="stx-ic k-bgagent">A</i>`;
/** What it goes by: a teammate its name; an agent or a script the task it was given. */
const bgName = (h) => (h.teammate && h.name) || h.desc || h.kind || "agent";
/** The newest thing it did, in a few words: a script's last line, an agent's last step (else the first line of what it said). */
function bgLatest(h) {
  if (h.latest != null) return h.latest;   // an earlier one from the session's history comes with it said
  if (h.script) return scriptTail(h);
  const x = h.items.at(-1);
  return !x ? "" : x.t === "step" ? `${x.verb} ${x.subject || ""}`.trim() : String(x.text || "").split("\n")[0];
}
/** A past moment as a clock time, "2:41pm"; with its weekday this week ("Fri 2:41pm"), else its date ("Oct 29 2:41pm"). */
function bgClock(ms) {
  const d = new Date(ms), h = d.getHours() % 12 || 12, ap = d.getHours() < 12 ? "am" : "pm";
  const t = `${h}:${String(d.getMinutes()).padStart(2, "0")}${ap}`;
  if (d.toDateString() === new Date().toDateString()) return t;
  return `${d.toLocaleDateString(undefined, now() - ms < 6 * 86400000 ? { weekday: "short" } : { month: "short", day: "numeric" })} ${t}`;
}
/** One piece of background work as one row, drawn like a step line. An agent or a teammate is an agent's row
 *  (agents.js agentRow); a script: its mark, what it ran, how long (or how it ended) in grey, the live dot (amber,
 *  with how long it's been quiet), and its newest line in grey at the right. A click opens it in full. `times`
 *  (the Agents and Scripts tabs): when it started, when it ended, and how long it took. */
function bgRow(sid, h, times = false) {
  if (!h.script) return agentRow(helperAgent(sid, h), times);
  const running = helperRunning(h), end = scriptEnd(h);
  const span = running ? `<span data-helper-span="${h.started_ms}">${helperSpan(h)}</span>` : helperSpan(h);
  const endWord = end === "failed" ? `<span class="bad">failed</span>` : end;
  const when = !times
    ? (running ? span : `${endWord} · ${span}`)
    : `started ${bgClock(h.started_ms)} · ${running ? `running ${span}` : `${endWord} ${bgClock(h.finished_ms || h.last_ms || h.started_ms)} · took ${span}`}`;
  return `<button class="stx-line bg-row ${end === "failed" ? "bad" : ""}" data-key="h:${esc(h.id)}" data-helper="${esc(`${sid}|${h.id}`)}">${bgIcon(h)}<span class="bg-name">${esc(bgName(h))}</span><span class="bg-meta">${times ? "" : "· script "}· ${when}${helperStuck(h) ? ` · ${helperQuiet(h)}` : ""}</span>${running ? liveDot(h) : ""}<span class="bg-tail">${esc(bgLatest(h))}</span><span class="stx-chev">›</span></button>`;
}
/** The runs of an agent's steps opened from their fold line (agentChatHtml), each "sid|agent|its first step". */
const agentRunOpen = new Set();
/** The leads' briefs opened in full (agentChatHtml), each "sid|agent". */
const agentBriefOpen = new Set();
/** A brief's first sentence, as one plain line: where it starts, what it's asked to do (a first sentence of a word or
 *  two, "Read-only task.", takes the next one with it). A dot inside a path or "e.g." doesn't end it. */
function briefLine(text) {
  const p = plain(text), END = /[.!?](?=\s+["'(`A-Z0-9]|\s*$)/g;
  let m, cut = p.length;
  while ((m = END.exec(p))) {
    if (/\b(?:e\.g|i\.e|etc|vs|cf)\.$/i.test(p.slice(0, m.index + 1))) continue;
    if (m.index + 1 < 24) continue;
    cut = m.index + 1;
    break;
  }
  return p.slice(0, cut).trim();
}
/** A lead's brief as markdown that reads like a short document, its words unchanged: a run of "(1) … (2) …" steps in
 *  a line becomes a numbered list; a paragraph that starts with a label ("Steps:", "Rules:", "Your files (only
 *  these):") gets the label as its head, and under Steps / Rules / Report its "; "-parted or sentence-parted body
 *  becomes a list. Fenced code stays as it is. */
const BRIEF_LIST = /^(steps?|rules?|report|reporting|don'?t|do not|done when|constraints?|requirements?|checks?|notes?)$/i;
function briefMd(text) {
  // Split outside `code` only: a "; " or ". " inside a command isn't a break between items.
  const parts = (s, re) => {
    const out = [];
    let at = 0, code = false;
    for (let i = 0; i < s.length; i++) {
      if (s[i] === "`") code = !code;
      if (code) continue;
      re.lastIndex = i;
      const m = re.exec(s);
      if (m && m.index === i && !/\b(?:e\.g|i\.e|etc|vs|cf)$/i.test(s.slice(at, i))) { out.push(s.slice(at, i + (m[1] || "").length).trim()); at = i + m[0].length; i = at - 1; }
    }
    out.push(s.slice(at).trim());
    return out.filter(Boolean);
  };
  const steps = (l) => {
    // "(1) a (2) b (3) c": numbered in order from 1, at least two, else it's a plain line.
    const at = [...l.matchAll(/(^|\s)\((\d{1,2})\)\s+/g)];
    if (at.length < 2 || at.some((m, n) => +m[2] !== n + 1)) return null;
    const lead = l.slice(0, at[0].index).trim();
    const items = at.map((m, n) => l.slice(m.index + m[0].length, n + 1 < at.length ? at[n + 1].index : l.length).trim());
    return [...(lead ? [lead] : []), ...items.map((x, n) => `${n + 1}. ${x}`)];
  };
  const para = (p) => {
    const lines = p.split("\n"), first = lines[0];
    const lab = /^\s*([A-Za-z][A-Za-z' ]{0,28}?)(\s*\([^)]{1,40}\))?:(?:\s+(.*))?$/.exec(first);
    const words = lab ? lab[1].trim().split(/\s+/).length : 0;
    const out = [];
    let body = lines;
    if (lab && words <= 4 && !/^(https?|file)$/i.test(lab[1])) {
      out.push(`**${lab[1].trim()}${lab[2] || ""}:**`);
      body = [lab[3] || "", ...lines.slice(1)].filter((l) => l.trim());
      // Under a list label, one line of items parted by "; " or by sentences becomes a list.
      if (BRIEF_LIST.test(lab[1].trim()) && body.length === 1 && !steps(body[0]) && !/^\s*([-*•]|\d+[.)])\s/.test(body[0])) {
        const semi = parts(body[0], /;\s+/g), sent = parts(body[0], /([.!?])\s+(?=["'(`A-Z])/g);
        const items = semi.length > 1 ? semi : sent.length > 1 ? sent : null;
        if (items) return [...out, ...items.map((x) => `- ${x}`)].join("\n");
      }
    }
    for (const l of body) out.push(...(steps(l) || [l]));
    return out.join("\n");
  };
  return String(text ?? "").replace(/\r/g, "").split(/```/).map((chunk, i) => (i % 2 ? chunk : chunk.split(/(\n\s*\n)/).map((p, n) => (n % 2 ? p : para(p))).join(""))).join("```");
}
/** A brief drawn whole (md over briefMd): a long run of code (a command, a path) set apart in a box of its own
 *  instead of running on in the sentence. */
const briefHtml = (text, runIn = null) => md(briefMd(text), { pathsOf: runIn }).replace(/<code>([\s\S]*?)<\/code>/g, (all, c) => (c.replace(/<[^>]+>/g, "").length > 48 ? `<code class="brief-long">${c}</code>` : all));
/** What a lead asked its crew member, at the head of its chat: folded to its first sentence with Show the full brief
 *  under it; opened, the whole brief (briefHtml). A brief of one sentence shows whole, with no button. */
function agentBriefHtml(sid, key, ask, by) {
  const k = `${sid}|${key}`, open = agentBriefOpen.has(k), line = briefLine(ask);
  const whole = line.length >= plain(ask).length;
  const body = whole || open ? `<div class="msg ag-brief-md">${briefHtml(ask, sid)}</div>` : `<p class="ag-brief-line">${esc(line)}</p>`;
  const btn = whole ? "" : `<button class="mt-lk ag-brief-tg" data-agent-brief="${esc(k)}" aria-expanded="${open}">${open ? "Show less" : "Show the full brief"}</button>`;
  return `<div class="bg-view-reply ag-ask ag-brief${open ? " open" : ""}" data-key="ask:${open ? 1 : 0}"><span class="bg-view-k">${esc(by || "Its lead")} asked it</span>${body}${btn}</div>`;
}
/** A crew member of lead `sid` by its agent id, if the agent is one. */
const crewMateById = (sid, id) => (sessionOf(sid)?.team || []).find((t) => t.agent_id === id) || null;
/** An agent's chat: the one chat both the board's view of it (helperPane) and the Desk's crew page (desk/thread.js
 *  deskCrewPane) draw, so the two read the same. What its lead asked it first (`ask`, under `by`'s name); then what it
 *  did in order: its words as messages, and each run of steps between them folded into one line ("6 steps · 4
 *  commands, 2 reads ▸") that opens them; the run it's on now with the live dot and its newest step under it; once it
 *  finished, its last words as its answer at the end (else `answer`, what Cue kept of it). `h`: its helper from the
 *  steps feed, or null when Cue has no feed of it (`steps` then: crewMateSteps' lines, one fold). `key`: its fold's. */
function agentChatHtml(sid, h, { ask = "", by = "", answer = "", steps = [], key = h?.id || "" } = {}) {
  const running = h ? helperRunning(h) : false, items = h?.items || [];
  const end = h && !running ? helperTail(h) : items.length;
  // Its answer is only its own words (chat/who.js): anything else in its last run is drawn among its runs below.
  const own = (x) => chatPlace({ role: "agent", ...x }, "crew") === "reply";
  const tail = h ? items.slice(end) : [], words = tail.filter(own);
  const reply = h ? words.map((x) => x.text).join("\n\n").trim() || answer : answer;
  // Anything else in that last run, by its time: what came before its answer among its runs, what came after it under
  // it (`later`), as every chat places a row.
  const replyAt = words.length ? words[0].at_ms : Infinity;
  const others = tail.filter((x) => !own(x)), sooner = others.filter((x) => !(x.at_ms >= replyAt)), later = others.filter((x) => x.at_ms >= replyAt);
  const out = [];
  // Send to… under its words and its answer (render.js fwdBtn), keyed under its lead's id so it lasts as the lead does.
  const from = sessionOf(sid)?.project || by || "crew member";
  const fwd = (id, text) => `<div class="mg-f cv-acts ag-acts">${fwdBtn(`${sid}:${`ag-${key}-${id}`.replace(/:/g, "-")}`, text, from, sid)}</div>`;
  if (ask) out.push(agentBriefHtml(sid, key, ask, by));
  // Its steps and words, a run of steps at a time between its words.
  const runs = [];
  if (h) items.slice(0, end).concat(sooner).forEach((x) => (x.t === "step" ? (runs.at(-1)?.steps || runs[runs.push({ steps: [] }) - 1].steps).push(x) : runs.push({ say: x })));
  else if (steps.length) runs.push({ lines: steps });
  // Who said it, from its tags (chat/who.js, #837): its own words (no role: the agent's), yours as your bubble, Cue's
  // as Cue's quiet line, never your bubble. "" for its own words.
  const notOwn = (x) => {
    const place = chatPlace({ role: "agent", ...x }, "crew");
    if (place === "bubble") return `<div class="cv-you mg-you" data-key="${esc(x.id || `y:${x.at_ms}`)}">${yourBubble(x, `${sid}:ag:${key}:${x.at_ms}`)}</div>`;
    return place !== "reply" && typeof cueQuietHtml === "function" ? keyed(cueQuietHtml(x, sessionOf(sid), `${sid}:ag:${key}:${x.at_ms}:cueq`), x.id || `c:${x.at_ms}`) : "";
  };
  runs.forEach((r, n) => {
    const theirs = r.say && notOwn(r.say);
    if (theirs) { out.push(theirs); return; }
    if (r.say) { const id = r.say.id || `s:${r.say.at_ms}`; out.push(`<div class="stx-say msg" data-key="${esc(id)}">${md(r.say.text)}${fwd(id, r.say.text)}</div>`); return; }
    const list = r.steps || r.lines, first = r.steps ? r.steps[0].id : `l${n}`, k = `${sid}|${key}|${first}`, open = agentRunOpen.has(k);
    const live = running && n === runs.length - 1;
    const count = `${list.length}${!n && h?.more ? "+" : ""} step${list.length === 1 ? "" : "s"}`, sum = r.steps ? turnSummary(r.steps) : "";
    const head = `<div class="stx-head"><button class="stx-fold" data-agent-run="${esc(k)}" aria-expanded="${open}">${live ? `<span class="dot-live"></span>` : ""}<b>${live ? `Working · ${count}` : count}</b>${sum ? `<span class="stx-sum"> · ${esc(sum)}</span>` : ""}<span class="stx-tri">${open ? "▾" : "▸"}</span></button></div>`;
    const line = (x) => (r.steps ? stepLine(sid, x, true) : `<div class="stx-line ${x.ask ? "bad" : ""}"><span class="stx-what">${x.html}</span></div>`);
    // Folded, the run it's on now still says what it's doing: its newest step under the fold.
    const body = open ? list.map(line).join("") : live ? line(list.at(-1)) : "";
    out.push(`<div class="stx" data-key="run:${esc(first)}">${head}${body ? `<div class="stx-steps">${body}</div>` : ""}</div>`);
  });
  if (!runs.length && !reply) out.push(`<div class="dim" data-key="none">${running ? "Starting…" : "No steps were kept for it."}</div>`);
  if (reply) out.push(`<div class="bg-view-reply" data-key="reply"><span class="bg-view-k">Its answer</span><div class="msg">${md(reply)}</div>${fwd("reply", reply)}</div>`);
  for (const x of later) if (x.t !== "step") out.push(notOwn(x));
  return `<div class="ag-chat">${out.join("")}</div>`;
}
/** One agent in full, in place of the chat: who it is, and its chat (agentChatHtml: what it was asked, what it did,
 *  its answer). Read-only: there's no messaging a background agent, so no box; the foot says whose turn it runs in,
 *  and ‹ Back (or Esc) returns to the chat. Patched in place while it runs (chatBox keeps the element; each part has
 *  a key). */
function helperPane(sid, h) {
  if (h.script) return scriptPane(sid, h);
  const running = helperRunning(h);
  const model = helperModel(h);
  const tok = helperTokens(h.tokens);
  // A crew member: what its lead asked it heads its chat, as on the Desk's crew page.
  const t = crewMateById(sid, h.id);
  const tail = !running && h.items.slice(helperTail(h)).length;
  // The head every agent's view has (agents.js agentHead): ‹ Back, its mark, name, kind, state, whose it is.
  const bar = agentHead(helperAgent(sid, h), helperBack(sid));
  const body = `<div class="bg-view-meta" data-key="meta">${h.teammate ? `<span>${esc(h.desc)}</span>` : ""}${model ? `<span>${esc(model)}</span>` : ""}${tok ? `<span>${tok}</span>` : ""}<span>${running ? `<span data-helper-span="${h.started_ms}">${helperSpan(h)}</span> so far` : h.teammate ? "idle" : `${AGENT_WORDS[agentState(h)]} · ${helperSpan(h)}`}</span>${helperStuck(h) ? `<span>${helperQuiet(h)}</span>` : ""}<span>${h.items.filter((x) => x.t === "step").length + (h.more || 0)} steps</span></div>
    ${h.more ? `<div class="stx-more dim" data-key="more">${h.more} earlier steps not shown</div>` : ""}
    ${agentChatHtml(sid, h, { ask: (t?.asked || "").trim(), by: nameOf(sid, sessionOf(sid)?.project), answer: t?.summary || "" })}`;
  // Finished with a reply: a button puts it in the session's message box (only there; you send it).
  if (h.teammate && h.name) return { bar, body, foot: teammateFoot(sid, h) };
  const toBox = !running && tail ? `<button class="btn" data-helper-reply="${esc(`${sid}|${h.id}`)}">Put reply in the box</button>` : "";
  const foot = helperViewFoot(running, running ? `Running inside this session's turn. It reports to the session, not to you.` : `What it reported went into the session's next reply.`, toBox);
  return { bar, body, foot };
}
/** The top of a full view: ‹ Back, to the Agents or Scripts tab it was opened from, else the chat (the tab
 *  lists the session's other background work). */
const helperBack = (sid) => `<button class="bg-back" data-helper-back="1"><span class="bg-back-ch">‹</span>Back to ${bgTabName(sid) || "chat"}</button>`;
const helperViewBar = (sid) => `<div class="bg-view-head" data-key="head">${helperBack(sid)}</div>`;
/** A teammate's foot: its messages reach its lead by themselves (and show in the lead's chat), so there's
 *  nothing to send from here. */
function teammateFoot(sid, h) {
  return helperViewFoot(helperRunning(h), `Its messages go to ${esc(nameOf(sid, sessionOf(sid)?.project))} by themselves.`);
}
const helperViewFoot = (live, text, extra = "") => `<div class="foot bg-view-foot">${live ? `<span class="dot-live"></span>` : ""}<span class="dim">${text}</span><span class="grow"></span>${extra}<button class="btn primary send" data-helper-back="1">Go to chat</button></div>`;
/** A background script in full: the command it ran and its output at full height (the tail Cue has of
 *  it, following the newest line), and once it ended, how. */
function scriptPane(sid, h) {
  const running = helperRunning(h);
  const shown = h.output.length, all = h.lines || shown;
  const bar = helperViewBar(sid);
  const body = `<div class="bg-view-title script" data-key="title">${bgIcon(h)}<span>${esc(h.desc)}</span><span class="bg-view-kind">script</span>${running ? liveDot(h) : ""}</div>
    <div class="bg-view-meta" data-key="meta">${running ? `<span><span data-helper-span="${h.started_ms}">${helperSpan(h)}</span> so far</span>` : `<span class="${h.failed ? "bad" : ""}">${scriptEnd(h)} · ${helperSpan(h)}</span>`}${helperStuck(h) ? `<span>${helperQuiet(h)}</span>` : ""}<span>${all} line${all === 1 ? "" : "s"}${all > shown ? ` · last ${shown} shown` : ""}</span></div>
    ${h.command && h.command !== h.desc ? `<div class="bg-cmd" data-key="cmd">${esc(h.command)}</div>` : ""}
    ${shown ? `<div class="bg-out bg-view-out" data-key="out" data-bg-scroll="${esc(`${sid}|${h.id}|full`)}">${h.output.map((l) => `<div>${esc(l)}</div>`).join("")}</div>` : `<div class="dim" data-key="none">${running ? "No output yet." : "It wrote nothing."}</div>`}`;
  const foot = helperViewFoot(running, running ? `Running in the background. Its output goes to the session when it ends.` : h.stopped ? `It was stopped before it ended. Its output up to then is here.` : h.failed ? `It ended with an error. The session was told, and its output is here.` : `It finished. The session was told, and its output is here.`);
  return { bar, body, foot };
}
