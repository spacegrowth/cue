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
let chatPinnedAt = 0;     // when code last put the chat at its end: the scroll events that come of it aren't you scrolling up
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
  const chat = document.querySelector("[data-chat]");
  if (!chat) return;
  pinChatEnd(chat);
  const again = () => { if (chat.isConnected && (chat.scrollTop === chat._pinTop || chat.scrollHeight - chat.scrollTop - chat.clientHeight < 24)) pinChatEnd(chat); };
  requestAnimationFrame(again);
  setTimeout(again, 250);
}
/** The chat at its end, by code (not you): the loader of earlier messages leaves the scroll events of it alone. */
function pinChatEnd(chat) {
  chat.scrollTop = chat.scrollHeight;
  chat._pinTop = chat.scrollTop;
  chatPinnedAt = Date.now();
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
/** One agent in full, in place of the chat: who it is, every step Cue has of it, and once it's finished
 *  its reply (its last words). Read-only: there's no messaging a background agent, so no box; the foot
 *  says whose turn it runs in, and ‹ Back (or Esc) returns to the chat. Patched in place while it runs
 *  (chatBox keeps the element; each part has a key). */
function helperPane(sid, h) {
  if (h.script) return scriptPane(sid, h);
  const running = helperRunning(h);
  const model = helperModel(h);
  const steps = [], tail = [];
  // Finished: its last words are its reply, shown apart from the steps.
  const end = helperTail(h);
  h.items.slice(0, end).forEach((x) => steps.push(x.t === "step" ? stepLine(sid, x, false, true) : `<div class="stx-say msg small" data-key="${esc(x.id || `s:${x.at_ms}`)}">${md(x.text)}</div>`));
  h.items.slice(end).forEach((x) => tail.push(x.text));
  const tok = helperTokens(h.tokens);
  // The head every agent's view has (agents.js agentHead): ‹ Back, its mark, name, kind, state, whose it is.
  const bar = agentHead(helperAgent(sid, h), helperBack(sid));
  const body = `<div class="bg-view-meta" data-key="meta">${h.teammate ? `<span>${esc(h.desc)}</span>` : ""}${model ? `<span>${esc(model)}</span>` : ""}${tok ? `<span>${tok}</span>` : ""}<span>${running ? `<span data-helper-span="${h.started_ms}">${helperSpan(h)}</span> so far` : h.teammate ? "idle" : `${AGENT_WORDS[agentState(h)]} · ${helperSpan(h)}`}</span>${helperStuck(h) ? `<span>${helperQuiet(h)}</span>` : ""}<span>${h.items.filter((x) => x.t === "step").length + (h.more || 0)} steps</span></div>
    ${h.more ? `<div class="stx-more dim" data-key="more">${h.more} earlier steps not shown</div>` : ""}
    ${steps.length ? `<div class="stx-steps bg-view-steps" data-key="steps">${steps.join("")}</div>` : `<div class="dim" data-key="none">${running ? "Starting…" : "No steps were kept for it."}</div>`}
    ${tail.length ? `<div class="bg-view-reply" data-key="reply"><span class="bg-view-k">Its reply</span><div class="msg">${md(tail.join("\n\n"))}</div></div>` : ""}`;
  // Finished with a reply: a button puts it in the session's message box (only there; you send it).
  if (h.teammate && h.name) return { bar, body, foot: teammateFoot(sid, h) };
  const toBox = !running && tail.length ? `<button class="btn" data-helper-reply="${esc(`${sid}|${h.id}`)}">Put reply in the box</button>` : "";
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
