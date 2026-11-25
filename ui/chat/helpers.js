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
  return `<button class="stx-line bg-row ${end === "failed" ? "bad" : ""}" data-key="h:${esc(h.id)}" data-helper="${esc(`${sid}|${h.id}`)}">${bgIcon(h)}<span class="bg-name">${esc(bgName(h))}</span><span class="bg-meta">${times || end === "failed" ? "" : "· script "}· ${when}${helperStuck(h) ? ` · ${helperQuiet(h)}` : ""}</span>${running ? liveDot(h) : ""}<span class="bg-tail">${esc(bgLatest(h))}</span><span class="stx-chev">›</span></button>`;
}

// ---------- what still runs once a session's turn is over (#867): its scripts, its agents, Cue's own update ----------
/** A session's background work as its card and Desk row count it: `scripts` running, `agents` running (the
 *  backend's `background`, every running helper in its feed, teammates too (steps running_background), less its
 *  scripts and its crew at work, who have their own rows), and `failed`: a script of
 *  it that failed since its turn ended (`task_failed_ms`, kept by Cue till its next turn starts). */
function bgCounts(s) {
  if (!s) return { scripts: 0, agents: 0, failed: false };
  const scripts = +s.scripts || 0;
  const crew = (s.team || []).filter((t) => t.agent_id && (t.state === "working" || t.state === "reviewing")).length;
  const agents = s.background != null ? Math.max(0, (+s.background || 0) - scripts - crew) : s.state === "agent" ? Math.max(0, (+s.helpers || 0) - crew) : 0;
  const failed = !!s.task_failed_ms && s.state !== "working" && (!s.since_ms || s.task_failed_ms >= s.since_ms);
  return { scripts, agents, failed };
}
/** Session `sid`'s scripts that failed since its turn ended (its chat's full rows): they stay in its tray, red, and
 *  the Scripts tab counts them, till its next turn starts. */
function bgFailedScripts(sid) {
  const s = sessionOf(sid);
  if (!s || s.state === "working") return [];
  const since = s.since_ms || 0;
  return helpersOf(sid).filter((h) => h.script && !helperRunning(h) && h.failed && !h.stopped && (h.finished_ms || h.last_ms || h.started_ms || 0) >= since);
}
/** Its pills, on a Board card and a Desk row: red "1 failed", "1 script" / "N scripts", "2 agents" (running only: they
 *  go when they're done). Never Cue's update: that's the Desk's own row (#891). `button`: each opens
 *  its session on that tab (a card); a Desk row is a button itself, so there they're plain. "" when it has none. */
function bgPills(s, button = true) {
  if (!s?.session_id) return "";
  const sid = s.session_id, c = bgCounts(s), out = [];
  const pill = (text, tab, bad = false) => button && tab
    ? `<button class="bg-pill${bad ? " bad" : ""}" data-bg-open="${esc(sid)}" data-bg-open-tab="${tab}">${text}</button>`
    : `<span class="bg-pill${bad ? " bad" : ""}">${text}</span>`;
  // Cue updating itself is no session's background work: it shows only in the Desk's own update row (#891).
  if (c.failed) out.push(pill("1 failed", "scripts", true));
  if (c.scripts) out.push(pill(`${c.scripts} script${c.scripts === 1 ? "" : "s"}`, "scripts"));
  if (c.agents) out.push(pill(`${c.agents} agent${c.agents === 1 ? "" : "s"}`, "agents"));
  return out.join("");
}
/** "2 failed · 410 passed" said short, for the status line: "2 of 412"; any other line cut to a few words. */
function bgFailShort(line) {
  const t = String(line || "").trim(), f = t.match(/(\d+)\s+failed/i), p = t.match(/(\d+)\s+passed/i);
  if (f && p) return `${f[1]} of ${+f[1] + +p[1]}`;
  return t.length > 40 ? t.slice(0, 40).replace(/\s+\S*$/, "") + "…" : t;
}
/** The status line of a session whose turn is over while its own work runs on (#867), "" otherwise: "Your turn ·
 *  npm test runs on · 4m" (live, from the script's start). Failed since its turn ended, nothing running: "Your turn · npm test failed · 2 of 412 ·
 *  2:08pm", red, with the waiting stripe. Its background agents keep their own line ("Waiting on …"). */
function bgWaitLine(it, s) {
  // Only for a turn that's simply over: a question or a permission it waits on keeps its own line.
  if (s?.state !== "waiting" || !s.session_id || waitingFor(it, s) !== "your reply") return "";
  const sid = s.session_id, tip = tipFor(it, s, "waiting");
  const live = (what, n, from) => `<div class="mg-status cv-status"><span class="dot-live"></span><span class="cv-what">Your turn · ${esc(what)} ${n > 1 ? "run" : "runs"} on</span>${from ? `<span class="dim"> · <span data-ago="${from}">${ago(from)}</span></span>` : ""}${tip}</div>`;
  const failed = (what, tail, at) => `<div class="mg-status cv-status"><i class="sw z live"></i><span class="cv-what">Your turn · <span class="bg-bad">${esc(what)} failed</span></span><span class="dim">${tail ? ` · ${esc(tail)}` : ""}${at ? ` · ${clockAt(at)}` : ""}</span>${tip}</div>`;
  const run = helpersOf(sid).filter((h) => h.script && helperRunning(h)).sort((a, b) => a.started_ms - b.started_ms);
  const n = Math.max(run.length, +s.scripts || 0);
  if (n) return live(run[0] ? `${bgName(run[0])}${n > 1 ? ` and ${n - 1} more` : ""}` : `${n} script${n === 1 ? "" : "s"}`, n, run[0]?.started_ms || s.since_ms);
  if (bgCounts(s).failed) return failed(s.task_failed || bgFailedScripts(sid)[0]?.desc || "A script", bgFailShort(s.task_failed_line), s.task_failed_ms);
  return "";
}

/** Cue updating itself (`state.desk.install`: built from main, swapped in, restarted), while it builds, installs or
 *  restarts (`run`). Null otherwise: how it ended is in the Desk's Activity. */
const CUE_INSTALL_STEPS = { building: "Building", installing: "Installing", starting: "Restarting" };
function cueInstallNow() {
  const x = state.desk?.install;
  if (!x?.step) return null;
  // Only while it runs: once it's installed or failed it's a row of the Desk's Activity instead (#867).
  return x.step in CUE_INSTALL_STEPS ? { ...x, run: true } : null;
}
/** The update as session `sid` shows it: only a Desk lead's (its pill, its tray, its status line). */
function cueInstallFor(sid) {
  const x = sid && cueInstallNow();
  if (!x) return null;
  return (state.desk?.projects || []).some((d) => d.lead?.sid === sid && (!x.project || x.project === d.project)) ? x : null;
}
/** The step it failed at, in a word ("build"); a rolled-back one failed at its restart. */
const cueInstallFailStep = (x) => ({ building: "build", installing: "install", starting: "restart" })[x.failed_step] || (x.step === "rolled_back" ? "restart" : "");
/** Its row in the lead's tray: the C mark, "Updating Cue", what it carries and where it is ("· #805, #807 · build ·
 *  3m"), the live dot, its newest log line; installed: "· installed · 6m", "Cue restarted on #807"; failed: red,
 *  "· failed at build · 7m", its last log line, Show why. "" when it's not this session's to show. */
function cueInstallRow(sid) {
  const x = cueInstallFor(sid);
  if (!x) return "";
  const items = (x.items || []).map((i) => `#${i}`).join(", ");
  const took = tookText(Math.max(0, (x.at || now()) - (x.began || x.at || now())));
  let meta, tail, live = "", why = "";
  if (x.run) {
    const from = x.began || x.at;
    meta = `${items ? `· ${esc(items)} ` : ""}· ${CUE_INSTALL_STEPS[x.step]} · <span data-ago="${from}">${ago(from)}</span>`;
    live = `<span class="dot-live"></span>`;
    tail = x.line || "";
  } else if (x.ok) {
    meta = `· installed · ${took}`;
    tail = x.items?.length ? `Cue restarted on #${x.items.at(-1)}` : x.commit ? `Cue restarted on ${String(x.commit).slice(0, 7)}` : "Cue restarted";
  } else {
    const at = cueInstallFailStep(x);
    meta = `· <span class="bad">failed</span>${at ? ` at ${at}` : ""} · ${took}`;
    tail = String(x.log || x.line || "").trimEnd().split("\n").at(-1) || "";
    why = `<button class="bg-go" data-cue-install-why>Show why</button>`;
  }
  return `<div class="stx-line bg-row cu-row ${x.bad ? "bad" : ""}" data-key="cue-install"><i class="stx-ic k-cue">C</i>${live}<span class="bg-name">Updating Cue</span><span class="bg-meta">${meta}</span><span class="bg-tail">${esc(tail)}</span>${why}</div>`;
}
/** The lead's tray where no other tray is drawn (the Desk's own chat of its lead): just the update's row. */
const cueInstallTray = (sid) => { const r = cueInstallRow(sid); return r ? `<div class="bg-rows" data-key="bg-rows">${r}</div>` : ""; };
/** The Desk's top line chip while Cue updates: "● Updating Cue · build · 3m"; failed: red "Cue update failed · Show
 *  why". Either opens the update's sheet (cueInstallSheet). "" on other views, and once it's installed. */
function cueInstallChip() {
  const x = view === "desk" ? cueInstallNow() : null;
  if (!x || x.ok) return "";
  if (x.bad) return `<button class="hchip cu-chip bad" data-cue-install-why>Cue update failed<span class="hsub"> · Show why</span></button>`;
  const from = x.began || x.at;
  return `<button class="hchip cu-chip" data-cue-install-why><span class="dot-live"></span>Updating Cue<span class="hsub"> · ${CUE_INSTALL_STEPS[x.step]} · <span data-ago="${from}">${ago(from)}</span></span></button>`;
}
/** Show why: the update's log in a plain sheet (as Settings' are), its newest lines in view. Esc, a click on Esc or
 *  outside closes it. */
let cueInstallWhy = false;
function cueInstallSheet() {
  const x = cueInstallWhy ? cueInstallNow() : null;
  if (!x) { cueInstallWhy = false; return ""; }
  const at = x.bad ? cueInstallFailStep(x) : "";
  const title = x.run ? `Updating Cue · ${CUE_INSTALL_STEPS[x.step]}` : x.ok ? "Cue updated" : `Cue update failed${at ? ` at ${at}` : ""}`;
  const meta = [(x.items || []).map((i) => `#${i}`).join(", "), x.commit ? String(x.commit).slice(0, 7) : "", x.began ? `started ${clockAt(x.began)}` : "", !x.run && x.at ? `${x.bad ? "ended" : "done"} ${clockAt(x.at)}` : ""].filter(Boolean).join(" · ");
  const log = String(x.log || x.line || "").trimEnd();
  return `<div class="scrim cu-scrim" data-cue-install-scrim><div class="sheet cu-sheet" role="dialog" aria-labelledby="cu-title">
    <div class="sheet-head"><h3 class="move-title" id="cu-title">${esc(title)}</h3><span class="grow"></span><kbd class="ld-esc" data-cue-install-close role="button" aria-label="Close">Esc</kbd></div>
    <div class="sheet-body">${meta ? `<div class="dim cu-meta">${esc(meta)}</div>` : ""}${log ? `<pre class="dk-flog open cu-log">${esc(log)}</pre>` : `<div class="dim">No log yet.</div>`}</div></div></div>`;
}
document.addEventListener("click", (e) => {
  const t = e.target;
  if (t.closest?.("[data-cue-install-why]")) {
    e.stopPropagation(); e.preventDefault();
    cueInstallWhy = true;
    renderMain();
    requestAnimationFrame(() => { const l = document.querySelector(".cu-log"); if (l) l.scrollTop = l.scrollHeight; });
    return;
  }
  if (cueInstallWhy && (t.closest?.("[data-cue-install-close]") || t.matches?.("[data-cue-install-scrim]"))) {
    e.stopPropagation(); e.preventDefault();
    cueInstallWhy = false;
    renderMain();
  }
}, true);
document.addEventListener("keydown", (e) => {
  if (e.key !== "Escape" || !cueInstallWhy) return;
  e.stopPropagation(); e.preventDefault();
  cueInstallWhy = false;
  renderMain();
}, true);
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
    // Its steps as 0.1.86 listed an agent's (#897): one row each, in place between its words.
    const list = r.steps || r.lines, first = r.steps ? r.steps[0].id : `l${n}`;
    const line = (x) => (r.steps ? stepLine(sid, x, false, true) : `<div class="stx-line ${x.ask ? "bad" : ""}"><span class="stx-what">${x.html}</span></div>`);
    out.push(`<div class="stx" data-key="run:${esc(first)}"><div class="stx-steps">${list.map(line).join("")}</div></div>`);
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
