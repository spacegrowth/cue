// Cue UI — insights: small, evidence-first notes about how a session is going, from its own log.
// Each one shows what's happening (a chart, a countdown), what you could do with what this session's
// machine already has (its own skills and commands), and sends it when you pick one. Cue never sends
// anything you didn't click. Its one prompt of its own is the hand-off (ipc/insights.rs), for a host
// with no hand-off skill: yours always comes first. What each insight did, and what
// you did with it, goes to insight_log (shown / acted / later / dismissed) so Settings can count it.
//
// SHORTCUT: fixed thresholds below, the same for everyone. Upgrade path: each person's own baseline
// from their turns history (the turns table), tuned by what they act on and dismiss.

// When to offer a hand-off: its context at least this much, and this many times what it started with.
// Warm, a hand-off pays off only once a session is big (its turns read the cache at a tenth of the input
// price); cold, its next reply writes the whole context again anyway, so the bar is lower. Set from 30 days
// of real sessions: warm at 150k paid off in only about half of them; 300k and 4× kept nearly all the savings.
const HANDOFF_WARM = { context: 300000, growth: 4 };
const HANDOFF_COLD = { context: 150000, growth: 3 };
const HANDOFF_CONTEXT = HANDOFF_COLD.context;   // the least any hand-off insight shows at
const CACHE_WARN_MS = 10 * 60000;   // the countdown shows in an hour's cache's last 10 minutes,
const CACHE_WARN_SHORT_MS = 2 * 60000;   // a 5-minute one's last 2 (the Mac's heads-up goes by the same: hub cache_tick)
const CACHE_MIN_TOKENS = 50000;     // smaller contexts are cheap to rebuild: not worth a note
const CACHE_COLD_FOR_MS = 12 * 3600000;

/** The hand-off bar a session has to clear now: the lower one once its cache has run out. A session whose
 *  log doesn't say how long it caches (Codex, Pi) is taken as warm. */
function handoffBar(sid) {
  const c = cacheOf(sid), cold = !!c && c.until_ms < now();
  return { ...(cold ? HANDOFF_COLD : HANDOFF_WARM), cold };
}
const insOn = (rule) => state.settings?.insights?.[rule] !== false;
// sid → the context size its hand-off insight waits for after "Remind me at …" (until Cue restarts). Nothing
// else hides an insight: it shows whenever its numbers say so.
const insLater = new Map();
try { localStorage.removeItem("cue.insLater"); } catch {}   // what an earlier build kept
const insLaterOf = (sid) => insLater.get(sid) || 0;
const insShownOnce = new Set(); // "rule:sid": logged as shown in this window
const insCache = new Map();     // sid → { until_ms, tokens, start, last_ms }, for sessions whose log isn't open here
/** Compacting, or compacted since its latest reply: its log's context count is from before, so its
 *  insights wait for the next reply (the open session's hand-off asks the same). */
function insStale(s, sid) {
  const last = stepFeeds.get(sid)?.meta?.last_call_ms || insCache.get(sid)?.last_ms || 0;
  return !!s?.compacting_ms || (!!s?.compacted_ms && s.compacted_ms > last);
}
const insChoosing = new Set();  // sid: its hand-off insight's skill drop-up is open
let insQuery = "";              // what's typed in that drop-up's search
/** Where a hand-off skill choice is kept: per machine ("" is this Mac) and agent. */
const insSkillKey = (s) => `${s.machine || ""}|${s.harness}`;
const insSkills = () => state.settings?.insights?.skills || {};
const CUE_HANDOFF = "cue:handoff";   // Cue's own hand-off, as it's kept when chosen (ipc/insights.rs)
/** Whether a hand-off skill takes "resume" for the next session: its description says so. Otherwise
 *  Cue's own message starts the next one.
 *  SHORTCUT: reads the one-line description only; a skill that takes "resume" without saying so there gets
 *  Cue's message (which still works). Upgrade path: read the skill's own file on its host. */
const takesResume = (sid, name) => /\bresume\b/i.test(cmdList(sid).find((c) => c.name === name)?.desc || "");
const LENS = `<svg class="ins-lens" viewBox="0 0 24 24" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5"></circle><circle cx="10.5" cy="10.5" r="1.6" class="dot"></circle><path d="M20 20l-4.6-4.6"></path></svg>`;

function insLog(sid, rule, action, detail = "", tokens = 0) {
  invoke("insight_log", { sessionId: sid, rule, action, detail, tokens }).catch(() => {});
}
function insShown(sid, rule) {
  const k = `${rule}:${sid}`;
  if (!insShownOnce.has(k)) { insShownOnce.add(k); insLog(sid, rule, "shown"); }
}
const insShort = (t) => (t.length > 90 ? `${t.slice(0, 88).trimEnd()}…` : t);
const insTok = (n) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : `${Math.round(n / 1000)}k`);
const median = (xs) => { const v = [...xs].sort((a, b) => a - b); return v.length ? v[Math.floor(v.length / 2)] : 0; };

// ---------- hand off: each turn now costs many times what it did ----------

/** A line of the session's context over time, from its first reply to now, with the hand-off point as
 *  a dashed level. A long session's log is read from near its end, so the stretch between its start and
 *  the first turn read here is drawn dashed: Cue knows both ends, not the path between. */
function insChart(pts, mark) {
  const W = 600, H = 64, t0 = pts[0].t, t1 = Math.max(pts.at(-1).t, t0 + 1), top = Math.max(mark, ...pts.map((p) => p.c)) * 1.12;
  const X = (t) => ((t - t0) / (t1 - t0)) * W, Y = (c) => H - (c / top) * H;
  const xy = (p) => `${X(p.t).toFixed(1)},${Y(p.c).toFixed(1)}`;
  const gap = pts.length > 2 && pts[1].t - pts[0].t > (t1 - t0) * 0.05;
  const solid = gap ? pts.slice(1) : pts;
  const line = `M${solid.map(xy).join(" L")}`;
  const fill = `${line} L${X(solid.at(-1).t).toFixed(1)},${H} L${X(solid[0].t).toFixed(1)},${H} Z`;
  const markY = Y(mark), endY = Y(pts.at(-1).c);
  return `<div class="ins-chart">
    <svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="none" aria-hidden="true">
      <path class="fill" d="${fill}"></path>
      ${gap ? `<path class="gap" d="M${xy(pts[0])} L${xy(pts[1])}"></path>` : ""}
      <path class="line" d="${line}"></path>
      <line class="level" x1="0" x2="${W}" y1="${markY.toFixed(1)}" y2="${markY.toFixed(1)}"></line>
    </svg>
    <span class="ins-level-label mono" style="top:${((markY / H) * 100).toFixed(1)}%">hand-off point · ${esc(insTok(mark))}</span>
    <i class="ins-dot" style="top:${((endY / H) * 100).toFixed(1)}%"></i>
  </div>`;
}

/** A hand-off sent and not continued yet (kept on the Mac, so it's here after a restart too): while it writes
 *  its note, say so; once it has, the one next step, a new session with "/<skill> resume" in its box. */
function handoffNext(sid, s, ho) {
  const starts = ho.resume ? `with <span class="mono">${esc(`/${ho.skill} resume`)}</span>` : "from its note";
  const body = s.state === "working"
    ? `<div class="ins-why">It's writing its hand-off note. Once it's done, continue in a new session from here.</div>`
    : `<div class="ins-why">Its hand-off note is written. Continue in a new session: it starts ${starts}, and this one closes (Closed keeps it).</div>
       <div class="ins-acts"><button class="btn primary" data-act="ins-continue" data-sid="${esc(sid)}">Continue in a new session</button></div>`;
  return `<div class="ins"><div class="ins-head">${LENS}<span class="ins-tag">INSIGHT</span><span class="ins-note">hand-off · ${esc(ago(ho.sent_ms))} ago</span></div>${body}</div>`;
}

/** The open session's hand-off insight, when its context has grown several times over and it's your turn. */
function handoffInsight(sid, s) {
  // It stays while the session works too (like the background-agents tray), until you act on it.
  if (!insOn("handoff") || !sid || !s) return "";
  const pending = state.skill_handoffs?.[sid];
  if (pending && !pending.next) return handoffNext(sid, s, pending);
  const f = stepFeeds.get(sid), m = f?.meta;
  if (!m || insStale(s, sid)) return "";
  const bar = handoffBar(sid);
  if ((m.context || 0) < bar.context || insLaterOf(sid) > m.context) return "";
  // Grown from what the session started at (its first reply, read from the top of its log), not from the
  // first turns read here: a long session's log is only read from near its end, often just a few turns.
  const read = (f.turns || []).map((t) => t.context).filter((c) => c > 0);
  const early = m.start_context || median(read.slice(0, 3)), grew = early ? m.context / early : 0;
  if (grew < bar.growth) return "";
  insShown(sid, "handoff");
  // What this session's machine has: the hand-off skill you chose for this machine and agent, else any
  // command named for it; else Cue's own hand-off. And compact.
  const cmds = cmdList(sid), chosen = insSkills()[insSkillKey(s)];
  // Relay's skills work only in a relay lead (its ⋯ menu's Relay handoff… asks the same), so never offer them elsewhere.
  const fits = (c) => !c.name.startsWith("relay:") || (crewOf(sid)?.role === "lead" && crewOf(sid)?.plugin === "relay");
  const handoffs = chosen === CUE_HANDOFF ? [] : (chosen ? cmds.filter((c) => c.name === chosen) : cmds.filter((c) => /hand-?off/i.test(c.name))).filter(fits);
  const opts = [
    ...handoffs.map((c) => ({ cmd: `/${c.name}` })),
    ...cmds.filter((c) => c.name === "compact").map(() => ({ cmd: "/compact" })),
  ];
  const where = s.machine ? `on ${machShort(s.machine)}` : "on this Mac";
  // Choose the hand-off skill: a drop-up over its buttons, searchable, skills first; kept for this host and agent.
  const choices = insChoosing.has(sid) ? cmds.filter((c) => fits(c) && c.kind !== "cue" && c.name !== "compact").sort((a, b) => (a.kind === "skill" ? 0 : 1) - (b.kind === "skill" ? 0 : 1) || a.name.localeCompare(b.name)) : [];
  const q = insQuery.trim().toLowerCase().replace(/^\//, "");
  const shown = choices.filter((c) => !q || c.name.toLowerCase().includes(q) || (c.desc || "").toLowerCase().includes(q)).slice(0, 60);
  const cueRow = !insChoosing.has(sid) || (q && !"cue's hand-off handoff".includes(q)) ? "" : `<button class="ins-pop-row ${chosen === CUE_HANDOFF ? "on" : ""}" data-act="ins-skill" data-sid="${esc(sid)}" data-name="${CUE_HANDOFF}"><span>Cue's hand-off</span><span class="dim">Writes HANDOFF.md; Continue starts the next session from it. Any agent</span></button>`;
  const picker = !insChoosing.has(sid) ? "" : `<div class="ins-pop" role="dialog" aria-label="Choose the hand-off skill">
      <input id="ins-search" class="ins-search" data-ins-search="${esc(sid)}" value="${esc(insQuery)}" placeholder="Search ${esc(where === "on this Mac" ? "this Mac's" : `${machShort(s.machine)}'s`)} ${esc(agentName(s.harness))} commands…" autocomplete="off" spellcheck="false">
      <div class="ins-pop-list">${cueRow}${shown.length || cueRow ? shown.map((c) => `<button class="ins-pop-row ${c.name === chosen ? "on" : ""}" data-act="ins-skill" data-sid="${esc(sid)}" data-name="${esc(c.name)}"><span class="mono">/${esc(c.name)}</span><span class="dim">${esc(insShort(c.desc || ""))}</span></button>`).join("") : `<div class="ins-pop-empty dim">${choices.length ? "Nothing matches." : "Reading its commands…"}</div>`}</div>
      <div class="ins-pop-foot"><button class="linkish" data-act="ins-refresh" data-sid="${esc(sid)}">Added one just now? Look again</button><span class="grow"></span><span class="dim">Esc to close</span></div>
    </div>`;
  // The chart: its first reply, then each turn read here, then its latest reply; time across.
  const mark = Math.max(early * bar.growth, bar.context);   // where both halves of the bar are cleared
  const turnPts = (f.turns || []).filter((t) => t.context > 0 && t.at_ms).map((t) => ({ t: t.at_ms, c: t.context }));
  const startT = m.start_ms || (turnPts[0]?.t ?? now()) - 1;
  const pts = [{ t: startT, c: early }, ...turnPts.filter((p) => p.t > startT).slice(-40), { t: Math.max(m.last_call_ms || 0, turnPts.at(-1)?.t || 0, startT + 1), c: m.context }];
  // When it passed the hand-off point, if that happened within the turns read here.
  const crossed = pts.findIndex((p, i) => i > 0 && p.c >= mark && pts[i - 1].c < mark && !(i === 1 && pts.length > 2 && pts[1].t - pts[0].t > (pts.at(-1).t - pts[0].t) * 0.05));
  const since = (crossed > 0 ? ` It passed the hand-off point ${ago(pts[crossed].t)} ago.` : "") + (bar.cold ? " Its cache has run out, so its next reply writes all of it again: the cheapest moment to hand off." : "");
  const remind = Math.ceil((m.context * 1.25) / 50000) * 50000;
  // /compact runs as Cue's own Compact does (⋯ → Compact); a hand-off skill goes in the box for you to send.
  // No hand-off skill it can use (or you chose Cue's): Cue's own hand-off.
  const cue = handoffs.length ? "" : `<button class="btn primary" data-act="ins-cue" data-sid="${esc(sid)}">Hand off</button>`;
  const buttons = cue + opts.map((o, i) => `<button class="btn ${handoffs.length && i === 0 ? "primary" : ""}" data-act="${o.cmd === "/compact" ? "ins-compact" : "ins-fill"}" data-sid="${esc(sid)}" data-ins-cmd="${esc(o.cmd)}"><span class="mono">${esc(o.cmd)}</span></button>`).join("");
  return `<div class="ins">
    <div class="ins-head">${LENS}<span class="ins-tag">INSIGHT</span><span class="ins-note">context this session re-reads, over time</span><span class="grow"></span><button class="linkish" data-act="ins-choose" data-sid="${esc(sid)}">Choose the hand-off skill</button></div>
    ${insChart(pts, mark)}
    <div class="ins-axis mono"><span>start · ${esc(insTok(early))} · ${esc(ago(startT))} ago</span><b>now · ${esc(insTok(m.context))}</b></div>
    <div class="ins-why">Each reply now re-reads <b>${esc(insTok(m.context))}</b> of context, ${Math.round(grew)}× what it started with. ${since} A fresh session with a short summary starts back near ${esc(insTok(early))}.</div>
    <div class="ins-acts-wrap">${picker}
    <div class="ins-acts">${buttons}<button class="btn" data-act="ins-later" data-sid="${esc(sid)}" data-at="${remind}">Remind me at ${esc(insTok(remind))}</button></div></div>
  </div>`;
}

// ---------- the cache runs out: the next reply re-writes the whole context ----------

/** When a session's cached context runs out, and how much the next reply re-writes after: from its
 *  log when it's open here, else from the last insight_state poll. */
function cacheOf(sid) {
  const m = stepFeeds.get(sid)?.meta;
  if (m?.cache_ttl_s && m.last_call_ms && m.context) return { until_ms: m.last_call_ms + m.cache_ttl_s * 1000, ttl: m.cache_ttl_s * 1000, tokens: m.context };
  const c = insCache.get(sid);
  return c?.until_ms ? { ...c, ttl: c.last_ms ? c.until_ms - c.last_ms : 0 } : null;
}
/** A session waiting on you whose cache is in its last minutes (`left` > 0) or went cold a little while ago
 *  (`left` <= 0), and big enough to matter: its numbers, else null. Its card's line and the header's "going
 *  cold" chip both go by this, so they agree. */
function cacheWarn(sid) {
  if (!insOn("cache") || !sid) return null;
  const s = sessionOf(sid), c = cacheOf(sid);
  if (!s || s.state === "working" || insStale(s, sid) || !c || c.tokens < CACHE_MIN_TOKENS) return null;
  const left = c.until_ms - now(), warn = !c.ttl || c.ttl > CACHE_WARN_MS ? CACHE_WARN_MS : CACHE_WARN_SHORT_MS;
  return left > warn || -left > CACHE_COLD_FOR_MS ? null : { c, left, warn };
}
/** One line for a session that's waiting on you: a countdown ring in the cache's last minutes, a dashed
 *  one once it's gone. `ask`: it waits on a decision (your answer is what its next reply follows), not a
 *  reply. "" when it isn't worth saying. */
function cacheLine(sid, ask = false) {
  const cw = cacheWarn(sid);
  if (!cw) return "";
  const { c, left, warn } = cw;
  if (left > 0) {
    insShown(sid, "cache");
    const mins = Math.max(1, Math.ceil(left / 60000)), arc = Math.round((left / warn) * 56.5 * 10) / 10;
    return `<div class="ins-line"><svg class="ins-ring" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" class="track"></circle><circle cx="12" cy="12" r="9" class="left" stroke-dasharray="${arc} 56.5" transform="rotate(-90 12 12)"></circle></svg><span><b>Cached ${mins} more min.</b> After that, ${ask ? "answering" : "its next reply"} re-writes ${esc(insTok(c.tokens))} tokens.</span></div>`;
  }
  return `<div class="ins-line cold"><svg class="ins-ring" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" class="gone"></circle></svg><span>Cache expired · ${ask ? `answering re-writes ${esc(insTok(c.tokens))} tokens.` : `its next reply re-writes ${esc(insTok(c.tokens))} tokens. Starting something new? Hand off instead.`}</span></div>`;
}
/** On a waiting card: the session has grown several times past where it started. Open it to hand off. */
function handoffLine(sid) {
  if (!insOn("handoff") || !sid) return "";
  const s = sessionOf(sid), m = stepFeeds.get(sid)?.meta, c = insCache.get(sid);
  const pending = state.skill_handoffs?.[sid];
  if (s && pending && !pending.next) return `<div class="ins-line">${LENS}<span><b>Hand-off note written.</b> Open it to continue in a new session.</span></div>`;
  const ctx = m?.context || c?.tokens || 0, start = m?.start_context || c?.start || 0;
  const bar = handoffBar(sid);
  if (!s || s.state === "working" || insStale(s, sid) || ctx < bar.context || !start || ctx / start < bar.growth || insLaterOf(sid) > ctx) return "";
  return `<div class="ins-line">${LENS}<span><b>${Math.round(ctx / start)}× its starting context</b> (${esc(insTok(ctx))}). ${bar.cold ? "Its cache is gone: the cheapest moment to hand off." : "Open it to hand off."}</span></div>`;
}
/** Sessions waiting on you (a reply or a decision, or put aside in Need to decide) whose log isn't open here, and a parked or closed one
 *  open in Active: ask the Mac for their insights' numbers (every 30 s). The board's cards on this Mac also
 *  get theirs with every state change (setState); this catches parked ones and other machines'. */
async function insPoll() {
  if (document.hidden || (!insOn("cache") && !insOn("handoff"))) return;
  const shelved = active?.sid && !sessionOf(active.sid) && (parkedOf(active.sid) || closedOf(active.sid)) ? [active.sid] : [];
  const sids = [...new Set([...state.items.map((i) => i.session_id), ...parkedIds(), ...shelved])].filter((sid) => sid && !stepFeeds.get(sid)?.meta);
  if (!sids.length) return;
  try {
    const r = (await invoke("insight_state", { sessionIds: sids })) || {};
    // Merged, not replaced: a state change may have pushed newer numbers while this was asked.
    for (const sid of sids) {
      const c = r[sid], had = insCache.get(sid);
      if (!c) insCache.delete(sid);
      else if (!had || (c.last_ms || 0) >= (had.last_ms || 0)) insCache.set(sid, c);
    }
  } catch {}
}
/** ↺ on a parked or closed session's row: true when its cache is gone and its context is big enough that
 *  resuming is worth a look first (its pane says what it costs, beside Start fresh). Asks the Mac when this
 *  window doesn't know yet; anything unknown resumes as before. */
async function insColdResume(sid) {
  if (!insOn("cache") || !sid) return false;
  let c = insCache.get(sid);
  if (!c) {
    try { c = (await invoke("insight_state", { sessionIds: [sid] }))?.[sid]; } catch {}
    if (c) insCache.set(sid, c);
  }
  // No cache lifetime in its log (Codex, Pi): not known to be cold, so it resumes as before.
  return !!c?.until_ms && c.tokens >= HANDOFF_CONTEXT && c.until_ms < now();
}
const insAsked = new Set();   // parked or closed sessions asked about once, as their pane opened
/** A parked or closed session open in Active: what resuming it costs. While its cache lasts, resuming
 *  re-reads its context cheaply; once it's gone, the first reply re-writes all of it, and a new session
 *  started from its last reply costs a fraction. "" until the numbers are in, or when it's small. */
function resumeCost(sid) {
  if (!insOn("cache") || !sid) return "";
  const c = insCache.get(sid);
  if (!c && !insAsked.has(sid)) { insAsked.add(sid); invoke("insight_state", { sessionIds: [sid] }).then((r) => { if (r?.[sid]) { insCache.set(sid, r[sid]); renderMain(); } }).catch(() => {}); }
  if (!c?.tokens || !c.until_ms || c.tokens < CACHE_MIN_TOKENS) return "";
  const left = c.until_ms - now();
  if (left > 0) return `<div class="ins-line" data-key="cost">${LENS}<span><b>Cached ${Math.max(1, Math.ceil(left / 60000))} more min.</b> Resuming now re-reads its ${esc(insTok(c.tokens))} of context from the cache.</span></div>`;
  insShown(sid, "cache");
  const fresh = c.start ? ` A new session from its last reply starts near ${esc(insTok(c.start))}.` : "";
  return `<div class="ins-line cold" data-key="cost"><svg class="ins-ring" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" class="gone"></circle></svg><span>Its cache is gone: resuming re-writes <b>${esc(insTok(c.tokens))}</b> tokens on its first reply.${fresh}</span><button class="linkish" data-act="ins-fresh" data-sid="${esc(sid)}">Start fresh from its last reply</button></div>`;
}
setInterval(insPoll, 30000);
setTimeout(insPoll, 3000);   // and soon after the window opens, not 30 s in

// ---------- queue, don't interrupt ----------

/** By the box while a session works and you've typed something: Now stops it, Queue keeps the work. */
function queueNote(sid, s) {
  if (!insOn("queue") || s?.state !== "working" || !draft(`s:${sid}`).text.trim()) return "";
  insShown(sid, "queue");
  return `<div class="ins-line ins-queue">${LENS}<span>Now stops its current step and what it was doing. Queue keeps the work and sends this when it's free.</span><button class="linkish" data-act="ins-queue-off">Got it</button></div>`;
}

// ---------- what you did with them ----------

/** You sent `text` to a session (Queue when `now` is false): count what an insight led to. */
function insSent(sid, text, now_) {
  const c = cacheOf(sid), s = sessionOf(sid);
  if (c && c.tokens >= CACHE_MIN_TOKENS && c.until_ms > now() && insShownOnce.has(`cache:${sid}`)) {
    insLog(sid, "cache", "acted", "", c.tokens);
    toast(`Caught ${nameOf(sid, s?.project)} in time: ${insTok(c.tokens)} tokens not written again`, 5000);
  }
  // A hand-off skill sent bare (not "… resume", not relay's, which opens the next lead itself): remembered
  // on the Mac until it continues in a new session.
  const ho = /^\/(\S*hand-?off)\s*$/i.exec(text.trim());
  if (ho && !ho[1].startsWith("relay:")) invoke("handoff_sent", { sessionId: sid, skill: ho[1], resume: takesResume(sid, ho[1]) }).catch(() => {});
  if (s?.state === "working" && !now_ && insShownOnce.has(`queue:${sid}`)) insLog(sid, "queue", "acted");
}
function insAct(act, d) {
  const sid = d.sid;
  if (act === "ins-refresh") { cmdList(sid, true); return renderMain(); }
  if (act === "ins-choose") { insQuery = ""; insChoosing.has(sid) ? insChoosing.delete(sid) : insChoosing.add(sid); renderMain(); return document.getElementById("ins-search")?.focus(); }
  if (act === "ins-skill") { const s = sessionOf(sid); insChoosing.delete(sid); insQuery = ""; return s && setSetting("insights.skills", { ...insSkills(), [insSkillKey(s)]: d.name }); }
  if (act === "ins-find") { cmdLists.clear(); cmdMissed.clear(); insFindSkills(true); return renderMain(); }
  if (act === "ins-skill-forget") { const { [d.key]: _, ...rest } = insSkills(); return setSetting("insights.skills", rest); }
  if (act === "ins-later") { insLater.set(sid, +d.at || Math.round((stepFeeds.get(sid)?.meta?.context || 0) * 1.25)); insLog(sid, "handoff", "later"); return renderMain(); }
  if (act === "ins-queue-off") { insLog(active?.sid || "", "queue", "dismissed"); return setSetting("insights.queue", false); }
  if (act === "ins-continue") {
    return invoke("handoff_continue", { sessionId: sid }).then((r) => {
      if (r.session_id) setActive(null, r.session_id);
      toast(r.detail);
    }, (e) => toast(`Couldn't continue: ${e}`));
  }
  if (act === "ins-cue") {
    return invoke("handoff_cue", { sessionId: sid }).then((r) => toast(`Hand-off sent${r ? ` · ${String(r).replace(/^sent /, "")}` : ""}`), (e) => toast(`Couldn't send: ${e}`));
  }
  if (act === "ins-fresh") {
    insLog(sid, "cache", "acted", "fresh", insCache.get(sid)?.tokens || 0);
    return invoke("handoff_fresh", { sessionId: sid }).then((r) => {
      if (r.session_id) setActive(null, r.session_id);
      toast(r.detail);
    }, (e) => toast(`Couldn't start it: ${e}`));
  }
  if (act === "ins-compact") {
    insLog(sid, "handoff", "acted", "/compact");
    return invoke("session_command", { sessionId: sid, action: "compact" }).then((m) => toast(m), (e) => toast(`Couldn't: ${e}`));
  }
  if (act === "ins-fill") {
    // Your click on the command is the message: it goes as you'd send it, like Compact or a quick phrase.
    insSent(sid, d.insCmd, false);
    insLog(sid, "handoff", "acted", d.insCmd);
    return invoke("send_to_session", { sessionId: sid, text: d.insCmd, images: [], now: false }).then((r) => toast(`${d.insCmd} sent${r ? ` · ${String(r).replace(/^sent /, "")}` : ""}`), (e) => toast(`Couldn't send: ${e}`));
  }

}

document.addEventListener("input", (e) => {
  if (!e.target.matches?.("[data-ins-search]")) return;
  insQuery = e.target.value;
  renderMain();
});
document.addEventListener("keydown", (e) => {
  if (!insChoosing.size) return;
  if (e.key === "Escape") { e.stopPropagation(); insChoosing.clear(); insQuery = ""; renderMain(); }
  else if (e.key === "Enter" && e.target.matches?.("[data-ins-search]")) { e.preventDefault(); document.querySelector(".ins-pop .ins-pop-row")?.click(); }
}, true);
document.addEventListener("mousedown", (e) => {
  if (insChoosing.size && !e.target.closest?.(".ins-pop, [data-act=ins-choose]")) { insChoosing.clear(); insQuery = ""; renderMain(); }
});

// ---------- Settings → Insights ----------
/** Settings → Insights: each insight on or off, the last 30 days, and the hand-off skill per host and agent. */
let insMachines = null;   // the machines you added (+ New → Machine), read once when this opens
let insFound = null;      // host → its hand-off skills for Claude Code (null while being read)
let insFoundAsks = 0;
/** Ask each host which hand-off skills it has; while any is still being read (or a newer list is on its
 *  way, the one it had shown till then), ask again shortly. */
function insFindSkills(fresh) {
  if (fresh) insFoundAsks = 0;
  invoke("insight_host_skills", { fresh: !!fresh }).then((r) => {
    insFound = r?.hosts || {};
    renderMain();
    if ((r?.reading || Object.values(insFound).some((v) => v === null)) && ++insFoundAsks < 40) setTimeout(() => insFindSkills(false), 1500);
  }).catch(() => {});
}
function insSettings(st) {
  if (insMachines === null) { insMachines = []; invoke("machines_list").then((l) => { insMachines = l || []; renderMain(); }).catch(() => {}); }
  if (insFound === null) { insFound = {}; insFindSkills(false); }
  const skills = insSkills();
  const hosts = [{ key: "", name: "This Mac" }, ...insMachines.map((m) => ({ key: m.name, name: m.label || machShort(m.name) }))];
  const agents = ["claude", "codex", "pi"];
  const hostRows = hosts.map((h) => {
    const lines = agents.map((a) => {
      const k = `${h.key}|${a}`, name = skills[k];
      // Claude Code: what the host has (Cue reads its commands there); the others' logs don't list theirs.
      const found = a === "claude" ? insFound?.[h.key] : undefined;
      const usable = (found || []).filter((n) => !n.startsWith("relay:"));
      const what = name ? `${name === CUE_HANDOFF ? "<span>Cue's hand-off</span>" : `<span class="mono">/${esc(name)}</span>`}<span class="dim">chosen</span><span class="grow"></span><button class="btn small" data-act="ins-skill-forget" data-key="${esc(k)}">Forget</button>`
        : found === null ? `<span class="dim">Looking…</span>`
        : usable.length ? `<span class="mono">${usable.map((n) => `/${esc(n)}`).join(", ")}</span><span class="dim">found</span>`
        : a === "claude" && found ? `<span class="dim">None found: Cue's hand-off</span>`
        : `<span class="dim">Any skill with “handoff” in its name, else Cue's hand-off</span>`;
      return `<div class="ins-skill-row"><span class="ins-skill-agent">${esc(agentName(a))}</span>${what}</div>`;
    }).join("");
    return setRow(esc(h.name), `<div class="ins-skill-rows">${lines}</div>`, "");
  }).join("");
  return `
    <div class="set-group">Insights</div>
    ${setRow("What they are", "Small notes from a session's own log, with the evidence. Each one offers what that session's machine already has, and sends it only when you click. Last 30 days: " + esc(insCountsLine()), "")}
    ${setRow("Hand off", "When a session's turns cost several times what its early ones did, offers its hand-off skill (else Cue's own: a note in HANDOFF.md, then Continue in a new session) or /compact.", toggle("insights.handoff", st.insights?.handoff ?? true))}
    ${setRow("Cache about to expire", "On a waiting card, the last minutes before its cached context runs out, and once it has. Claude Code says when; other agents' logs don't.", toggle("insights.cache", st.insights?.cache ?? true))}
    ${setRow("Queue instead of interrupting", "While a session works and you've typed something: Now stops what it's doing, Queue keeps it.", toggle("insights.queue", st.insights?.queue ?? true))}
    <div class="set-group">Hand-off skill, by host</div>
    ${setRow("How it's set", "Each host's agents have their own skills. Cue offers what a host has (any with “handoff” in its name, else its own hand-off), or the one you choose from a session's hand-off insight: kept for that host and agent.", `<button class="btn small" data-act="ins-find">Look again</button>`)}
    ${hostRows}`;
}
let insCounts = null;
function insCountsLine() {
  if (!insCounts) { insCounts = []; invoke("insight_counts").then((r) => { insCounts = r || []; renderMain(); }).catch(() => {}); }
  const by = Object.fromEntries(insCounts.map((r) => [r.rule, r]));
  const part = (rule, label) => (by[rule] ? `${label}: shown ${by[rule].shown}, acted ${by[rule].acted}` : "");
  const saved = by.cache?.tokens ? ` · ${insTok(by.cache.tokens)} tokens not re-written` : "";
  return [part("handoff", "Hand off"), part("cache", "Cache") + saved, part("queue", "Queue")].filter(Boolean).join(" · ") || "Nothing shown yet.";
}
