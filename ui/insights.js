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
const CACHE_WARN_MS = 10 * 60000;   // the countdown shows in an hour's cache's last 10 minutes; a 5-minute one has
                                    // none (it runs out while you read; the Mac's heads-up goes by the same: hub cache_tick)
const CACHE_MIN_TOKENS = 50000;     // smaller contexts are cheap to rebuild: not worth a note
const CACHE_COLD_FOR_MS = 12 * 3600000;
// Compacting while it's still cached reads the context back at the cache's price and leaves a short summary,
// so a reply after you're back writes that, not all of it. Below this, the summary and its own reply cost
// about what writing the context again would.
const COMPACT_WARM_MIN = 100000;
// The auto-compact heads-up shows from this share of where Claude Code has lately compacted sessions on the
// model on its own (logs/steps.rs auto_compact_at): a turn or two before it does.
const AUTO_NEAR = 0.85;
// A hand-off note before the cache runs out (hub/activity.rs hand_off_ahead): a session's "Hand off if I don't
// come back" is offered on an hour's cache (a 5-minute one runs out while you read: Write hand-off now is yours
// to press), and from this much context it shows on the strip even before you tick it. Settings' every-session
// timer goes from the same (AHEAD_MIN_TOKENS there).
const AHEAD_MIN = 150000;
const AHEAD_TTL_MS = 30 * 60000;

/** The hand-off bar a session has to clear now: the lower one once its cache has run out. A session whose
 *  log doesn't say how long it caches (Codex, Pi) is taken as warm. */
function handoffBar(sid) {
  const c = cacheOf(sid), cold = !!c && c.until_ms < now();
  return { ...(cold ? HANDOFF_COLD : HANDOFF_WARM), cold };
}
const insOn = (rule) => state.settings?.insights?.[rule] !== false;
/** A hand-off note written before its cache ran out (by you, its "Hand off if I don't come back", or Settings),
 *  not continued from or replied past yet: its hand-off as the Mac keeps it, else null. */
const aheadOf = (sid) => { const ho = state.skill_handoffs?.[sid]; return ho?.ahead > 0 && !ho.next ? ho : null; };
const aheadWatched = (sid) => (state.ahead_watch || []).includes(sid);
/** Whether "Hand off if I don't come back" shows for a session: Claude Code on this Mac (the Mac's timer only
 *  reads its own cards' logs), an hour's cache, enough context to matter, and no note already written. Offered
 *  only while that cache is alive (once it's gone there's nothing to get ahead of until its next reply); once
 *  ticked it always shows, so you can see it's on and untick it. */
function aheadCan(sid) {
  const s = sessionOf(sid), c = cacheOf(sid);
  return !!s && insOn("handoff") && s.harness === "claude" && !s.machine && (c?.ttl || 0) >= AHEAD_TTL_MS && c.tokens >= CACHE_MIN_TOKENS && !aheadOf(sid) && (c.until_ms > now() || aheadWatched(sid));
}
/** Ticked on a session whose cache has already run out: it goes from your next reply. */
const aheadLater = (sid) => aheadWatched(sid) && !((cacheOf(sid)?.until_ms || 0) > now());
const watchText = (sid) => aheadLater(sid)
  ? "<b>Hands off if you don't come back,</b> from your next reply: its cache has run out, so there's nothing to get ahead of until then."
  : aheadWatched(sid)
  ? "<b>Hands off if you don't come back:</b> a minute before its cache runs out, it writes a hand-off note, and you choose when you're back."
  : "Away for a while? A minute before its cache runs out, it can write a hand-off note, and you choose when you're back.";
// A checkbox drawn as a button: a real one inside the bar would click twice (its label, then itself) and open the bar too.
const watchToggle = (sid) => `<button class="ins-check" role="checkbox" aria-checked="${aheadWatched(sid)}" data-act="ins-watch" data-sid="${esc(sid)}"><i></i>Hand off if I don't come back</button>`;
// `own`: on its own under the bar (ticked), lined up with the bar rather than inside its panel.
const watchLine = (sid, own = false) => `<div class="ins-line ${own ? "ins-own" : ""}">${BULB}<span>${watchText(sid)}</span>${watchToggle(sid)}</div>`;
// sid → the context size its hand-off insight waits for after "Remind me at …" (until Cue restarts). Nothing
// else hides an insight but aheadQuiet below: it shows whenever its numbers say so.
const insLater = new Map();
try { localStorage.removeItem("cue.insLater"); } catch {}   // what an earlier build kept
const insLaterOf = (sid) => insLater.get(sid) || 0;
// sid → its context when you pressed Continue here on a note written ahead: "Hand off if I don't come back"
// waits off the bar till it has grown AHEAD_MIN past that (until Cue restarts, as insLater). A compact since
// (its context now below that) starts it over: from the compact, it's AHEAD_MIN as for any session.
const aheadQuiet = new Map();
function aheadQuietOf(sid, tokens) {
  const q = aheadQuiet.get(sid) || 0;
  if (tokens < q) aheadQuiet.delete(sid);
  return tokens < q ? 0 : q;
}
const insShownOnce = new Set(); // "rule:sid": logged as shown in this window
const insCache = new Map();     // sid → { until_ms, tokens, start, last_ms, cold_x, cold_cents, auto_at }, for sessions whose log isn't open here
/** Compacting, or compacted since its latest reply: its log's context count is from before, so its
 *  insights wait for the next reply (the open session's hand-off asks the same). */
function insStale(s, sid) {
  const last = stepFeeds.get(sid)?.meta?.last_call_ms || insCache.get(sid)?.last_ms || 0;
  return !!s?.compacting_ms || (!!s?.compacted_ms && s.compacted_ms > last);
}
let insChoosing = "";           // Settings → Insights: the "host|agent" whose hand-off skill list is open
let insQuery = "";              // what's typed in that list's search
/** Where a hand-off skill choice is kept: per machine ("" is this Mac) and agent. */
const insSkillKey = (s) => `${s.machine || ""}|${s.harness}`;
const insSkills = () => state.settings?.insights?.skills || {};
const CUE_HANDOFF = "cue:handoff";   // Cue's own hand-off, as it's kept when chosen (ipc/insights.rs)
/** Whether a hand-off skill takes "resume" for the next session: its description says so. Otherwise
 *  Cue's own message starts the next one.
 *  SHORTCUT: reads the one-line description only; a skill that takes "resume" without saying so there gets
 *  Cue's message (which still works). Upgrade path: read the skill's own file on its host. */
const takesResume = (sid, name) => /\bresume\b/i.test(cmdList(sid).find((c) => c.name === name)?.desc || "");
// Every insight's mark: a lit bulb (rays around it), in the theme's own color (style.css .ins-bulb).
/** The bulb drawn small inside the cache's countdown ring (cacheLine). */
const RING_BULB = `<g class="ins-bulb" transform="translate(12 12.4) scale(0.5) translate(-12 -12)"><path d="M12 7a4.5 4.5 0 0 0-2.7 8.1c.5.4.8.9.8 1.5v.9h3.8v-.9c0-.6.3-1.1.8-1.5A4.5 4.5 0 0 0 12 7z"></path><path d="M10.3 21h3.4M12 2.2v1.6M5.4 4.9l1.1 1.1M18.6 4.9l-1.1 1.1M3 11.5h1.6M19.4 11.5H21"></path></g>`;
const BULB = `<svg class="ins-bulb" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 7a4.5 4.5 0 0 0-2.7 8.1c.5.4.8.9.8 1.5v.9h3.8v-.9c0-.6.3-1.1.8-1.5A4.5 4.5 0 0 0 12 7z"></path><path d="M10.3 21h3.4M12 2.2v1.6M5.4 4.9l1.1 1.1M18.6 4.9l-1.1 1.1M3 11.5h1.6M19.4 11.5H21"></path></svg>`;

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

// sid → { name, starts }: "Continue in a new session" pressed, the new session not started yet.
const insContinuing = new Map();
/** Over the old session's chat while its next one starts: what's happening, and what comes of it. */
function continueCover(sid) {
  const c = insContinuing.get(sid);
  if (!c) return "";
  const starts = c.starts ? `with <span class="mono">${esc(c.starts)}</span>` : "from its hand-off note";
  return `<div class="rs-cover"><div class="rs-card" role="status"><div class="rs-title">Continuing ${esc(c.name)} in a new session</div><div class="rs-bar"><i></i></div>
    <div class="rs-note">It starts ${starts} and opens here. ${c.ahead ? "This one stays as it is." : "This one closes (Closed keeps it)."}</div></div></div>`;
}

/** A hand-off sent and not continued yet (kept on the Mac, so it's here after a restart too): while it writes
 *  its note, say so; once it has, the one next step, a new session with "/<skill> resume" in its box. */
function handoffNext(sid, s, ho) {
  if (ho.ahead) return aheadCard(sid, s, ho);
  const starts = ho.resume ? `with <span class="mono">${esc(`/${ho.skill} resume`)}</span>` : "from its note";
  const body = s.state === "working"
    ? `<div class="ins-why">It's writing its hand-off note. Once it's done, continue in a new session from here.</div>`
    : `<div class="ins-why">Its hand-off note is written. Continue in a new session: it starts ${starts}, and this one closes (Closed keeps it).</div>
       <div class="ins-acts"><button class="btn primary" data-act="ins-continue" data-sid="${esc(sid)}">Continue in a new session</button></div>`;
  return `<div class="ins"><div class="ins-head">${BULB}<span class="ins-tag">INSIGHT</span><span class="ins-note">hand-off · ${esc(ago(ho.sent_ms))} ago</span></div>${body}</div>`;
}

/** A note written before its cache ran out: while it's written, say so; then both ways on, with what each costs.
 *  Continue here keeps all of its context (once the cache is gone, its next reply re-writes it); a new session
 *  from the note starts small, and this one stays as it is. Replying here is continuing here too. */
function aheadCard(sid, s, ho) {
  const c = cacheOf(sid), left = c ? c.until_ms - now() : 0;
  // Written once Cue has seen the note (its time, from its log): asking leave to write it isn't done yet.
  const head = `<div class="ins-head">${BULB}<span class="ins-tag">INSIGHT</span><span class="ins-note">hand-off · ${ho.note_ms ? "written" : "writing"} before its cache ran out · ${esc(ago(ho.sent_ms))} ago</span></div>`;
  if (!ho.note_ms) return `<div class="ins">${head}<div class="ins-why">It's writing a hand-off note before its cache runs out. Nothing else changes: this session stays as it is.</div></div>`;
  const fresh = `<button class="btn ${left > 0 ? "" : "primary"}" data-act="ins-continue" data-sid="${esc(sid)}">New session from the note</button>`;
  const here = `<button class="btn ${left > 0 ? "primary" : ""}" data-act="ins-keep" data-sid="${esc(sid)}">Continue here</button>`;
  const why = left > 0
    ? `<b>Hand-off note ready.</b> Its cache lasts ${Math.max(1, Math.ceil(left / 60000))} more min, so continuing here is still cheap.`
    : `<b>Cache expired · hand-off note ready.</b> Continuing here, ${c ? coldCost(c, "its next reply") : "its next reply re-writes all of its context"}. A new session from the note starts small, and this one stays as it is.`;
  return `<div class="ins">${head}<div class="ins-why">${why}</div><div class="ins-acts">${left > 0 ? here + fresh : fresh + here}</div></div>`;
}

/** A hand-off that already continued: where it carries on now, and Close for this one. Hand-off isn't offered here
 *  again: sending a hand-off skill again writes a new note, and Continue comes back. */
function handoffContinued(sid, ho) {
  const next = nameOf(ho.next, sessionOf(ho.next)?.project) || "its new session";
  const close = svCloseArm === sid
    ? `<button class="btn deny on" data-sv="close" data-sid="${esc(sid)}">Close?</button>`
    : `<button class="btn" data-sv="close" data-sid="${esc(sid)}">Close this one</button>`;
  return `<div class="ins"><div class="ins-head">${BULB}<span class="ins-tag">INSIGHT</span><span class="ins-note">hand-off · continued ${esc(ago(ho.continued_ms || ho.sent_ms))} ago</span></div>
    <div class="ins-why">It continued in <b>${esc(next)}</b>. Carry on there, and close this one when you're done with it (Closed keeps it).</div>
    <div class="ins-acts"><button class="btn primary" data-act="ins-goto" data-sid="${esc(ho.next)}">Go to ${esc(next)}</button>${close}</div></div>`;
}

// The open session's insights sit above its box as one bar: the most pressing one's sentence and its one
// action, "+N" for the rest. A click anywhere on it opens it: the bar rises like a title and the rest unfolds
// under it (chart, every action, the other insights as lines), the bar itself as it was. It takes the selected
// card's lime (Cue's color for what needs you) only when acting now saves something: past the hand-off point (until you act on it or put it off),
// or your turn with its cache in its last minutes.
const insOpen = new Set();      // sid: its insights opened, while Cue runs
const insHotSeen = new Set();   // sid: its hand-off has had its colored moment (acted on, put off)
// SHORTCUT: kept while Cue runs, so after a restart a session past its hand-off point colors once more. Upgrade
// path: keep it on the Mac with the hand-off state (db, like skill_handoffs), so both apps share it.
const HOT_CACHE_MS = 5 * 60000; // a cache this close to running out colors the strip
// sid → what you just tapped ("Handing off…"), until it's done: shown in place of the actions, so nothing
// goes twice and the tap shows at once, before the session's next state arrives.
const insBusy = new Map();

/** The open session's hand-off numbers, when its context has grown several times over: else null. */
function handoffDue(sid, s) {
  if (!insOn("handoff")) return null;
  const f = stepFeeds.get(sid), m = f?.meta;
  if (!m || insStale(s, sid)) return null;
  const bar = handoffBar(sid);
  if ((m.context || 0) < bar.context || insLaterOf(sid) > m.context) return null;
  // Grown from what the session started at (its first reply, read from the top of its log), not from the
  // first turns read here: a long session's log is only read from near its end, often just a few turns.
  const read = (f.turns || []).map((t) => t.context).filter((c) => c > 0);
  const early = m.start_context || median(read.slice(0, 3)), grew = early ? m.context / early : 0;
  if (grew < bar.growth) return null;
  // What this session's machine has: the hand-off skill you chose for this machine and agent, else one
  // named exactly /handoff (never another with "handoff" in its name: that may be something else); else
  // Cue's own hand-off. And compact.
  const cmds = cmdList(sid), chosen = insSkills()[insSkillKey(s)];
  // Relay's skills work only in a relay lead (its ⋯ menu's Relay handoff… asks the same), so never offer them elsewhere.
  const fits = (c) => !c.name.startsWith("relay:") || (crewOf(sid)?.role === "lead" && crewOf(sid)?.plugin === "relay");
  const handoffs = chosen === CUE_HANDOFF ? [] : (chosen ? cmds.filter((c) => c.name === chosen) : cmds.filter((c) => c.name === "handoff")).filter(fits);
  const opts = [
    ...handoffs.map((c) => ({ cmd: `/${c.name}` })),
    ...cmds.filter((c) => c.name === "compact").map(() => ({ cmd: "/compact" })),
  ];
  // The chart: its first reply, then each turn read here, then its latest reply; time across.
  const mark = Math.max(early * bar.growth, bar.context);   // where both halves of the bar are cleared
  const turnPts = (f.turns || []).filter((t) => t.context > 0 && t.at_ms).map((t) => ({ t: t.at_ms, c: t.context }));
  const startT = m.start_ms || (turnPts[0]?.t ?? now()) - 1;
  const pts = [{ t: startT, c: early }, ...turnPts.filter((p) => p.t > startT).slice(-40), { t: Math.max(m.last_call_ms || 0, turnPts.at(-1)?.t || 0, startT + 1), c: m.context }];
  // When it passed the hand-off point, if that happened within the turns read here.
  const crossed = pts.findIndex((p, i) => i > 0 && p.c >= mark && pts[i - 1].c < mark && !(i === 1 && pts.length > 2 && pts[1].t - pts[0].t > (pts.at(-1).t - pts[0].t) * 0.05));
  const since = (crossed > 0 ? ` It passed the hand-off point ${ago(pts[crossed].t)} ago.` : "") + (bar.cold ? " Its cache has run out, so its next reply writes all of it again: the cheapest moment to hand off." : "");
  const remind = Math.ceil((m.context * 1.25) / 50000) * 50000;
  const pick = opts.some((o) => o.cmd === "/compact") ? handoffPick(sid, s, f) : { compact: false, why: "" };
  return { m, early, grew, pts, mark, startT, since, handoffs, opts, remind, ...pick };
}

const COMMIT_RE = /\bgit\s+(commit|push)\b/;
const TEST_RE = /\b(test|tests|pytest|jest|vitest|cargo\s+(test|nextest)|go\s+test|run\.sh)\b/;
/** Hand off or compact first, from the session's own steps: a piece of work that just landed (committed,
 *  or its tests passed, with nothing edited since), a session that has compacted before, or one spread over
 *  several areas starts fresh better; one in the middle of a change keeps going better with /compact. Hand
 *  off when nothing says. `why`: the reason, for the opened card.
 *  SHORTCUT: only the turns read here (a long log is read from near its end) and the step's first line.
 *  Upgrade path: keep the last commit and compaction with the turns history (logs/turns.rs). */
function handoffPick(sid, s, f) {
  const steps = (f.turns || []).flatMap((t) => (t.items || []).filter((i) => i.t === "step"));
  let landed = "", editedSince = false, failing = false;
  for (let i = steps.length - 1; i >= 0 && !landed; i--) {
    const st = steps[i];
    if (st.kind === "edit") editedSince = true;
    if (st.kind !== "run" || !st.done) continue;
    if (COMMIT_RE.test(st.subject)) { if (!st.bad) landed = "committed"; }
    else if (TEST_RE.test(st.subject)) { if (st.bad) failing = failing || !editedSince; else landed = "tests"; }
  }
  const ctx = (f.turns || []).map((t) => t.context).filter((c) => c > 0);
  const compacted = !!s?.compacted_ms || ctx.some((c, i) => i > 0 && c < ctx[i - 1] * 0.5);
  const areas = insDelegate.get(sid)?.split?.length || 0;
  if (landed && !editedSince) return { compact: false, why: landed === "committed" ? "Its work is committed, so a fresh session starts clean from here." : "Its tests passed with nothing edited since, so this piece looks done." };
  if (compacted) return { compact: false, why: "It has compacted before: another summary of a summary loses more, so a written note is safer." };
  if (areas >= 3) return { compact: false, why: `It has worked across ${areas} areas: a fresh session for what's next keeps them apart.` };
  if (failing || editedSince) return { compact: true, why: failing ? "It's in the middle of a fix (its tests are failing), so /compact keeps it going in place." : "It's in the middle of a change, so /compact keeps it going in place." };
  return { compact: false, why: "" };
}

/** What a tap does, by its button: the hand-off skill (your click is the message), Cue's own hand-off when it
 *  has none it can use (or you chose Cue's), and /compact as Cue's own Compact (⋯ → Compact). */
function handoffButtons(sid, d) {
  // The pick (handoffPick) first and filled, the other after it.
  const first = d.compact ? "/compact" : d.handoffs.length ? d.opts[0].cmd : "";
  const btn = (o) => `<button class="btn ${o.cmd === first ? "primary" : ""}" data-act="${o.cmd === "/compact" ? "ins-compact" : "ins-fill"}" data-sid="${esc(sid)}" data-ins-cmd="${esc(o.cmd)}"><span class="mono">${esc(o.cmd)}</span></button>`;
  const cue = d.handoffs.length ? "" : `<button class="btn ${d.compact ? "" : "primary"}" data-act="ins-cue" data-sid="${esc(sid)}">Hand off</button>`;
  const opts = [...d.opts].sort((a, b) => (b.cmd === first) - (a.cmd === first));
  return d.compact ? opts.map(btn).join("") + cue : cue + opts.map(btn).join("");
}
/** The strip's one action: the pick (handoffPick), by the same route as the card's first button. Filled only
 *  when the strip is colored; on the quiet one it stays plain, so it doesn't pull the eye from the box's Send. */
function handoffMain(sid, d, hot) {
  const h = d.handoffs[0], cls = `btn ${hot ? "primary" : ""}`;
  if (d.compact) return `<button class="${cls}" data-act="ins-compact" data-sid="${esc(sid)}" data-ins-cmd="/compact">Compact</button>`;
  return h ? `<button class="${cls}" data-act="ins-fill" data-sid="${esc(sid)}" data-ins-cmd="/${esc(h.name)}">Hand off</button>`
    : `<button class="${cls}" data-act="ins-cue" data-sid="${esc(sid)}">Hand off</button>`;
}
// The bar's arrow: up to open (the bar rises, the rest unfolds under it), down to fold.
const CHEVRON_DOWN = `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 9l6 6 6-6"></path></svg>`;
const CHEVRON_UP = `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 15l6-6 6 6"></path></svg>`;
const insBusyNote = (sid) => `<span class="ins-busy" role="status">${esc(insBusy.get(sid))}</span>`;

/** The open session's insights above its box: handed off (what's next), else the strip, or the card opened. */
function handoffInsight(sid, s) {
  if (!sid || !s) return "";
  const pending = insOn("handoff") && state.skill_handoffs?.[sid];
  if (pending && !pending.next) return handoffNext(sid, s, pending);
  if (pending?.next) return handoffContinued(sid, pending);
  // It stays while the session works too (like the background-agents tray), until you act on it.
  const d = handoffDue(sid, s);
  const it = state.items.find((i) => i.session_id === sid), ask = !!it && it.kind !== "waiting";
  const cw = cacheWarn(sid), cache = cw ? cacheLine(sid, ask, true, !d?.opts.some((o) => o.cmd === "/compact")) : "", del = delegateLine(sid);
  // Near where it auto-compacts: its own line, unless the hand-off card offers /compact already.
  const au = autoDue(sid, true), auto = au ? autoLine(sid, !d?.opts.some((o) => o.cmd === "/compact")) : "";
  // Hand off if I don't come back: in the opened panel whenever it can (ticking it while it works is fine), on the
  // bar itself once it's on, or when a big session waits on your reply (the only kind the Mac's timer acts on): not
  // while it works on what you just sent, asks you something, or is out of usage; nor, after Continue here, till
  // it has grown AHEAD_MIN past that (aheadQuiet).
  // Ticked, it's its own line under the bar, always in sight (never one of the bar's "+N").
  const can = aheadCan(sid), ticked = can && aheadWatched(sid), watch = can && !ticked ? watchLine(sid) : "";
  const tokens = cacheOf(sid)?.tokens || 0;
  const watchBar = can && !ticked && s.state === "waiting" && !ask && !insStale(s, sid) && tokens >= aheadQuietOf(sid, tokens) + AHEAD_MIN;
  if (!d && !cache && !del && !auto && !watchBar && !ticked) return "";
  if (!d && !cache && !del && !auto && !watchBar) return watchLine(sid, true);
  if (d) insShown(sid, "handoff");
  const busy = insBusy.has(sid), isOpen = insOpen.has(sid);
  const open = `data-act="ins-open" data-sid="${esc(sid)}"`;
  // The bar: the most pressing one. A cache about to run out comes first (replying now is what saves it). It
  // looks the same folded or opened (only its arrow turns), and a click anywhere on it opens or folds it.
  const hotCache = !!cw && cw.left > 0 && cw.left <= HOT_CACHE_MS && !ask;
  const hot = hotCache || (!!d && !insHotSeen.has(sid));
  const handoffText = d && (insHotSeen.has(sid)
    ? `<b>Each reply re-reads ${esc(insTok(d.m.context))},</b> ${Math.round(d.grew)}× its start.`
    : `<b>Past its hand-off point.</b> Each reply re-reads ${esc(insTok(d.m.context))}, ${Math.round(d.grew)}× its start.`);
  const shown = [hotCache && cacheText(cw, ask), handoffText, au && autoText(au), cw && !hotCache && cacheText(cw, ask), delegateText(sid), watchBar && watchText(sid)].filter(Boolean);
  const text = shown[0], rest = shown.length - 1;
  // Opened, the panel never repeats the bar's own line; with nothing else to show, the bar doesn't open at all.
  const lead = hotCache ? "cache" : d ? "handoff" : au ? "auto" : cw ? "cache" : delegateText(sid) ? "del" : "watch";
  const lines = [lead !== "auto" && auto, lead !== "cache" && cache, lead !== "del" && del, lead !== "watch" && watch].filter(Boolean).join("");
  const opens = !!d || !!lines;
  // A cache in its last minutes leading the bar carries its own ways out (its line isn't repeated when opened).
  const warmActs = lead === "cache" && cw.left > 0 && !ask && !d ? `${insOn("handoff") && !s.machine ? `<button class="btn" data-act="ins-ahead" data-sid="${esc(sid)}">Write hand-off now</button>` : ""}${compactWarm(sid, cw, ask) ? `<button class="btn" data-act="ins-compact-warm" data-sid="${esc(sid)}">Compact now</button>` : ""}` : "";
  // Gone, leading the bar: Start fresh, as its sentence says.
  const coldActs = lead === "cache" && cw.left <= 0 && !ask && !d ? `<button class="btn" data-act="ins-fresh" data-sid="${esc(sid)}">Start fresh</button>` : "";
  const act = busy ? insBusyNote(sid) : d ? handoffMain(sid, d, hot) : au && canCompact(sid) ? `<button class="btn" data-act="ins-compact-auto" data-sid="${esc(sid)}">Compact</button>` : warmActs || coldActs || (lead === "watch" ? watchToggle(sid) : "");
  const arrow = opens ? `<button class="ins-arrow" ${open} aria-expanded="${isOpen}" aria-label="${isOpen ? "Fold its insights" : "Show all of its insights"}">${isOpen ? CHEVRON_DOWN : CHEVRON_UP}</button>` : "";
  // With its own ways out on it, the cache's sentence wraps instead of losing what the next reply costs.
  const bar = `<div class="ins-strip ${hot ? "hot" : ""} ${opens ? "" : "whole"} ${(warmActs || coldActs) && !busy ? "wrap" : ""}" ${opens ? open : ""}>${BULB}<span class="ins-strip-text">${text}</span>${rest && !isOpen ? `<span class="ins-n">+${rest}</span>` : ""}${act}${arrow}</div>`;
  const below = ticked ? watchLine(sid, true) : "";
  if (!isOpen || !opens) return bar + below;
  // Opened: the bar rises like a title and the rest unfolds under it, on the panel: the other insights the bar's
  // +N counts first (one line each), then the chart and every action.
  const acts = d && !busy ? `<div class="ins-acts">${handoffButtons(sid, d)}<button class="btn" data-act="ins-later" data-sid="${esc(sid)}" data-at="${d.remind}">Remind me at ${esc(insTok(d.remind))}</button></div>` : "";
  const body = d ? `${insChart(d.pts, d.mark)}
    <div class="ins-axis mono"><span>start · ${esc(insTok(d.early))} · ${esc(ago(d.startT))} ago</span><b>now · ${esc(insTok(d.m.context))}</b></div>
    <div class="ins-why">Each reply now re-reads <b>${esc(insTok(d.m.context))}</b> of context, ${Math.round(d.grew)}× what it started with. ${d.since} A fresh session with a short summary starts back near ${esc(insTok(d.early))}.${d.why ? ` ${esc(d.why)}` : ""}</div>
    ${acts}` : "";
  return `${bar}${below}<div class="ins ins-detail">${lines ? `<div class="ins-lines">${lines}</div>` : ""}${body}</div>`;
}

// ---------- the cache runs out: the next reply re-writes the whole context ----------

/** When a session's cached context runs out, and how much the next reply re-writes after: from its
 *  log when it's open here, else from the last insight_state poll. */
function cacheOf(sid) {
  const m = stepFeeds.get(sid)?.meta;
  if (m?.cache_ttl_s && m.last_call_ms && m.context) return { until_ms: m.last_call_ms + m.cache_ttl_s * 1000, ttl: m.cache_ttl_s * 1000, tokens: m.context, cold_x: m.cold_x, cold_cents: m.cold_cents };
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
  if (c.ttl && c.ttl <= CACHE_WARN_MS) return null;
  const left = c.until_ms - now(), warn = CACHE_WARN_MS;
  return left > warn || -left > CACHE_COLD_FOR_MS ? null : { c, left, warn };
}
/** One line for a session that's waiting on you: the bulb in a ring counting down the cache's last minutes
 *  (amber in its last 5, when replying now is what saves it), the bulb alone once it's gone. `ask`: it waits on a decision (your answer is what its next reply follows), not a
 *  reply. `open`: the open session's line, which also offers Write hand-off now (a note to come back to, its
 *  session kept as it is), and with `compact` Compact now (compactWarm). "" when it isn't worth saying. */
function cacheLine(sid, ask = false, open = false, compact = false) {
  const cw = cacheWarn(sid);
  if (!cw) return "";
  const { left, warn } = cw;
  if (left > 0) {
    insShown(sid, "cache");
    const arc = Math.round((left / warn) * 66 * 10) / 10;
    // Write hand-off now: a session on this Mac (another machine's note is seen by its log copy's clock, which can drift).
    const away = open && compact && compactWarm(sid, cw, ask), note = open && !ask && insOn("handoff") && !sessionOf(sid)?.machine;
    const why = note && away ? " Away longer? A hand-off note now gives you a cheap way back; Compact now leaves a short summary in place."
      : note ? " Away longer? A hand-off note now gives you a cheap way back, and this session stays as it is."
      : away ? " Away longer? Compacting now leaves a short summary to come back to." : "";
    const acts = `${note ? `<button class="btn small" data-act="ins-ahead" data-sid="${esc(sid)}">Write hand-off now</button>` : ""}${away ? `<button class="btn small" data-act="ins-compact-warm" data-sid="${esc(sid)}">Compact now</button>` : ""}`;
    return `<div class="ins-line"><svg class="ins-ring ${left <= HOT_CACHE_MS ? "soon" : ""}" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="10.5" class="track"></circle><circle cx="12" cy="12" r="10.5" class="left" stroke-dasharray="${arc} 66" transform="rotate(-90 12 12)"></circle>${RING_BULB}</svg><span>${cacheText(cw, ask)}${why}</span>${acts}</div>`;
  }
  // Gone: the open session's line offers what its sentence says (a new session from its last reply, this one kept).
  const fresh = open && !ask ? `<button class="btn small" data-act="ins-fresh" data-sid="${esc(sid)}">Start fresh</button>` : "";
  return `<div class="ins-line cold">${BULB}<span>${cacheText(cw, ask)}</span>${fresh}</div>`;
}
/** Whether to offer Compact now on a cache in its last minutes: it waits for a reply (not a decision), it's
 *  big enough that a summary is much smaller, and it can compact (Claude Code's /compact). */
function compactWarm(sid, cw, ask) {
  return !ask && cw.left > 0 && cw.c.tokens >= COMPACT_WARM_MIN && canCompact(sid);
}
const canCompact = (sid) => cmdList(sid).some((c) => c.name === "compact");
/** What a reply after the cache runs out costs, `what` doing it ("its next reply", "answering"): as many warm
 *  replies as it's worth when the model is priced (prices::cold_x), with the tokens it re-writes, and dollars
 *  only on the API: Claude Code's 5-hour and weekly limits mean a subscription, where nothing is billed. */
function coldCost(c, what) {
  const u = state.usage, api = !(u && (u.five_hour || u.seven_day));
  const usd = api && c.cold_cents >= 1 ? `, about $${(c.cold_cents / 100).toFixed(2)}` : "";
  return c.cold_x > 1 ? `${what} costs as much as <b>${c.cold_x} warm replies</b> (it re-writes ${esc(insTok(c.tokens))} tokens${usd})`
    : `${what} re-writes ${esc(insTok(c.tokens))} tokens${usd}`;
}
/** cacheWarn's sentence, for its line and the open session's strip. */
function cacheText({ c, left }, ask) {
  const what = ask ? "answering" : "its next reply";
  if (left > 0) return `<b>Cached ${Math.max(1, Math.ceil(left / 60000))} more min.</b> After that, ${coldCost(c, what)}.`;
  return `Cache expired · ${coldCost(c, what)}.${ask ? "" : " Starting something new? Start fresh from its last reply instead: no re-write."}`;
}
/** On a waiting card: the session has grown several times past where it started. Open it to hand off. */
function handoffLine(sid) {
  if (!insOn("handoff") || !sid) return "";
  const s = sessionOf(sid), m = stepFeeds.get(sid)?.meta, c = insCache.get(sid);
  const pending = state.skill_handoffs?.[sid];
  if (s && pending?.ahead && !pending.next) {
    if (!pending.note_ms) return `<div class="ins-line">${BULB}<span><b>Writing a hand-off note</b> before its cache runs out.</span></div>`;
    const c = cacheOf(sid), left = c ? c.until_ms - now() : 0;
    return `<div class="ins-line">${BULB}<span>${left > 0 ? `<b>Hand-off note ready.</b> Still cached ${Math.max(1, Math.ceil(left / 60000))} more min.` : "<b>Cache expired · hand-off note ready.</b> Open it to continue here or start fresh from the note."}</span></div>`;
  }
  if (s && pending && !pending.next) return `<div class="ins-line">${BULB}<span><b>Hand-off note written.</b> Open it to continue in a new session.</span></div>`;
  if (s && pending?.next) return `<div class="ins-line">${BULB}<span><b>Continued in ${esc(nameOf(pending.next, sessionOf(pending.next)?.project) || "a new session")}.</b> Close this one when you're done with it.</span></div>`;
  const ctx = m?.context || c?.tokens || 0, start = m?.start_context || c?.start || 0;
  const bar = handoffBar(sid);
  if (!s || s.state === "working" || insStale(s, sid) || ctx < bar.context || !start || ctx / start < bar.growth || insLaterOf(sid) > ctx) return "";
  return `<div class="ins-line">${BULB}<span><b>${Math.round(ctx / start)}× its starting context</b> (${esc(insTok(ctx))}). ${bar.cold ? "Its cache is gone: the cheapest moment to hand off." : "Open it to hand off."}</span></div>`;
}
/** Sessions waiting on you (a reply or a decision, or put aside in Need to decide) whose log isn't open here, and a parked or closed one
 *  open in Active: ask the Mac for their insights' numbers (every 30 s). The board's cards on this Mac also
 *  get theirs with every state change (setState); this catches parked ones and other machines'. */
async function insPoll() {
  if (document.hidden || (!insOn("cache") && !insOn("handoff") && !insOn("autocompact"))) return;
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
  if (left > 0) return `<div class="ins-line" data-key="cost">${BULB}<span><b>Cached ${Math.max(1, Math.ceil(left / 60000))} more min.</b> Resuming now re-reads its ${esc(insTok(c.tokens))} of context from the cache.</span></div>`;
  insShown(sid, "cache");
  const fresh = c.start ? ` A new session from its last reply starts near ${esc(insTok(c.start))}.` : "";
  return `<div class="ins-line cold" data-key="cost">${BULB}<span>Its cache is gone: ${coldCost(c, "its first reply after resuming")}.${fresh}</span><button class="btn small" data-act="ins-fresh" data-sid="${esc(sid)}">Start fresh</button></div>`;
}
setInterval(insPoll, 30000);
setTimeout(insPoll, 3000);   // and soon after the window opens, not 30 s in

// ---------- near where Claude Code compacts on its own ----------

/** A session within reach of where Claude Code has lately compacted sessions on its model on its own (the
 *  least of its last few, logs/steps.rs auto_compact_at: the point moves between versions, so it's a range,
 *  not a promise): its context and that point, else null. `open`: its strip, shown while it works too;
 *  a card's waits for its turn to end. */
function autoDue(sid, open = false) {
  if (!insOn("autocompact") || !sid) return null;
  const s = sessionOf(sid), m = stepFeeds.get(sid)?.meta, c = insCache.get(sid);
  const ctx = m?.context || c?.tokens || 0, at = m?.auto_at || c?.auto_at || 0;
  if (!s || (!open && s.state === "working") || insStale(s, sid) || !at || ctx < at * AUTO_NEAR) return null;
  return { ctx, at };
}
/** autoDue's sentence, for its line and the open session's strip. */
function autoText({ ctx, at }) {
  return `<b>Near where it compacts on its own.</b> Claude Code has lately auto-compacted sessions on this model from ${esc(insTok(at))}; it's at ${esc(insTok(ctx))}. Compact or hand off now, on your terms.`;
}
/** On a waiting card, and in the open session's lines (`act`: with Compact now). */
function autoLine(sid, act = false) {
  const au = autoDue(sid, act);
  if (!au) return "";
  insShown(sid, "autocompact");
  return `<div class="ins-line">${BULB}<span>${autoText(au)}</span>${act && canCompact(sid) ? `<button class="btn small" data-act="ins-compact-auto" data-sid="${esc(sid)}">Compact now</button>` : ""}</div>`;
}

// ---------- could have delegated: looking around itself, or many areas in one long session ----------

// sid → what it could have delegated (ipc insights_tab delegate_of), for the board's cards; whether relay is on.
// From the turns history, which is read every 10 minutes: asked every minute is plenty.
let insDelegate = new Map(), insRelay = false;
async function delPoll() {
  if (document.hidden || !insOn("delegate")) return;
  const sids = [...new Set(state.items.map((i) => i.session_id))].filter(Boolean);
  if (!sids.length) return;
  try {
    const r = (await invoke("insight_delegate", { sessionIds: sids })) || {};
    const next = new Map(Object.entries(r.sessions || {}));
    const changed = insRelay !== !!r.relay || JSON.stringify([...next]) !== JSON.stringify([...insDelegate]);
    insDelegate = next;
    insRelay = !!r.relay;
    if (changed) renderMain();
  } catch {}
}
setInterval(delPoll, 60000);
setTimeout(delPoll, 4000);
/** An area of a project as a short name: "." is its top level, a folder outside it its last part. */
const areaName = (a) => (a === "." ? "top level" : a.split("/").filter(Boolean).at(-1) || a);
/** On a waiting card: this session looked around itself where an agent could have, or ran long across
 *  several areas where each could have had a session (relay, when it's on) or an agent of its own. */
function delegateLine(sid) {
  const text = delegateText(sid);
  return text ? `<div class="ins-line">${BULB}<span>${text}</span></div>` : "";
}
/** A board card's insights as one line, the most pressing (a cache in its last minutes, else the hand-off,
 *  what it could have delegated, the cache), with "+N" for the rest: they're all in its open strip.
 *  `ask`: it waits on a decision, so only its cache counts. */
function insCardLine(sid, ask = false) {
  // A note written ahead says where its cache stands itself.
  const cw = cacheWarn(sid), cache = aheadOf(sid) ? "" : cacheLine(sid, ask);
  const lines = ask ? [cache] : cw && cw.left > 0 && cw.left <= HOT_CACHE_MS ? [cache, handoffLine(sid), autoLine(sid), delegateLine(sid)] : [handoffLine(sid), autoLine(sid), delegateLine(sid), cache];
  const [top, ...rest] = lines.filter(Boolean);
  // Ticked "Hand off if I don't come back": said in words on its own line, under the rest, never folded into "+N".
  const watching = !ask && aheadWatched(sid) && aheadCan(sid) ? `<div class="ins-line">${BULB}<span><b>Hands off if you don't come back</b> ${aheadLater(sid) ? "from your next reply" : "before its cache runs out"}.</span></div>` : "";
  return (top && rest.length ? top.replace(/<\/div>$/, `<span class="ins-n">+${rest.length}</span></div>`) : top || "") + watching;
}
/** delegateLine's sentence, for its line and the open session's strip: "" when there's nothing to say. */
function delegateText(sid) {
  const s = sessionOf(sid), d = insDelegate.get(sid);
  if (!insOn("delegate") || !s || s.state === "working" || !d) return "";
  insShown(sid, "delegate");
  if (d.search) {
    const later = d.search.later ? `its ${d.search.later} later repl${d.search.later === 1 ? "y" : "ies"} re-read ${esc(insTok(d.search.tokens))} of it` : "every reply after re-reads it";
    return `<b>Could have sent an agent to look.</b> ${d.search.looks} reads and searches grew it ${esc(insTok(d.search.growth))}; ${later}.`;
  }
  const next = insRelay ? "relay, an executor per area" : "a background agent per area, each in its own worktree";
  return `<b>${d.split.length} areas in one long session</b> (${esc(d.split.slice(0, 3).map(areaName).join(", "))}${d.split.length > 3 ? ", …" : ""}). Next time: ${next}.`;
}

// ---------- queue, don't interrupt ----------

/** By the box while a session works and you've typed something: Now stops it, Queue keeps the work. */
function queueNote(sid, s) {
  if (!insOn("queue") || s?.state !== "working" || !draft(`s:${sid}`).text.trim()) return "";
  insShown(sid, "queue");
  return `<div class="ins-line ins-queue">${BULB}<span>Now stops its current step and what it was doing. Queue keeps the work and sends this when it's free.</span><button class="btn small" data-act="ins-queue-off">Got it</button></div>`;
}

// ---------- what you did with them ----------

/** You sent `text` to a session (Queue when `now` is false): count what an insight led to. */
function insSent(sid, text, now_) {
  const c = cacheOf(sid), s = sessionOf(sid);
  if (c && c.tokens >= CACHE_MIN_TOKENS && c.until_ms > now() && insShownOnce.has(`cache:${sid}`)) {
    insLog(sid, "cache", "acted", "", c.tokens);
  }
  // A hand-off skill sent bare (not "… resume", not relay's, which opens the next lead itself): remembered
  // on the Mac until it continues in a new session.
  const ho = /^\/(\S*hand-?off)\s*$/i.exec(text.trim());
  if (ho && !ho[1].startsWith("relay:")) invoke("handoff_sent", { sessionId: sid, skill: ho[1], resume: takesResume(sid, ho[1]) }).catch(() => {});
  if (s?.state === "working" && !now_ && insShownOnce.has(`queue:${sid}`)) insLog(sid, "queue", "acted");
}
function insAct(act, d) {
  const sid = d.sid;
  if (act === "ins-choose") {
    insQuery = "";
    insChoosing = insChoosing === d.key ? "" : d.key;
    // Opened: its host's commands read again, so a skill added a moment ago is in the list.
    if (insChoosing) { cmdLists.clear(); cmdMissed.clear(); insFindSkills(true); }
    renderMain();
    return document.getElementById("ins-search")?.focus();
  }
  if (act === "ins-skill") { insChoosing = ""; insQuery = ""; return setSetting("insights.skills", { ...insSkills(), [d.key]: d.name }); }
  if (act === "ins-skill-forget") { const { [d.key]: _, ...rest } = insSkills(); return setSetting("insights.skills", rest); }
  // Opening keeps its colored moment (the bar stays as it was); acting on it, or Remind me, ends it.
  if (act === "ins-open") { insOpen.has(sid) ? insOpen.delete(sid) : insOpen.add(sid); return renderMain(); }
  if (act === "ins-later") { insHotSeen.add(sid); insOpen.delete(sid); insLater.set(sid, +d.at || Math.round((stepFeeds.get(sid)?.meta?.context || 0) * 1.25)); insLog(sid, "handoff", "later"); return renderMain(); }
  if (act === "ins-queue-off") { insLog(active?.sid || "", "queue", "dismissed"); return setSetting("insights.queue", false); }
  if (act === "ins-continue") {
    if (insContinuing.has(sid)) return;
    const s = sessionOf(sid), ho = state.skill_handoffs?.[sid];
    insContinuing.set(sid, { name: nameOf(sid, s?.project), starts: ho?.resume ? `/${ho.skill} resume` : "", ahead: ho?.ahead > 0 });
    renderMain();
    return invoke("handoff_continue", { sessionId: sid }).then((r) => {
      insContinuing.delete(sid);
      if (r.session_id) setActive(null, r.session_id);
      toast(r.detail);
      renderMain();
    }, (e) => { insContinuing.delete(sid); toast(`Couldn't continue: ${e}`); renderMain(); });
  }
  // The session a hand-off continued in.
  if (act === "ins-goto") return sessionOf(sid) ? openSession(sid) : setActive(null, sid);
  // Write hand-off now: Cue's hand-off before its cache runs out; its session stays as it is.
  if (act === "ins-ahead") {
    return insRun(sid, "Writing its hand-off note…", () => invoke("handoff_cue", { sessionId: sid, ahead: true })
      .then(() => toast("Hand-off note asked for: when you're back, continue here or start fresh from it"), (e) => toast(`Couldn't send: ${e}`)));
  }
  // Continue here, on a note written ahead: it's set aside, and the box is yours.
  if (act === "ins-keep") {
    aheadQuiet.set(sid, cacheOf(sid)?.tokens || 0);
    return invoke("handoff_keep", { sessionId: sid }).then(() => document.querySelector(`textarea[data-text="${CSS.escape(`s:${sid}`)}"]`)?.focus(), (e) => toast(`Couldn't: ${e}`));
  }
  if (act === "ins-watch") {
    const on = !aheadWatched(sid);
    return invoke("ahead_watch", { sessionId: sid, on }).then(() => toast(on ? "If you're not back a minute before its cache runs out, it writes a hand-off note" : "It won't hand off on its own"), (e) => toast(`Couldn't: ${e}`));
  }
  if (act === "ins-cue") {
    return insRun(sid, "Handing off…", () => invoke("handoff_cue", { sessionId: sid })
      .then((r) => toast(`Hand-off sent${r ? ` · ${String(r).replace(/^sent /, "")}` : ""}`), (e) => toast(`Couldn't send: ${e}`)));
  }
  if (act === "ins-fresh") {
    insLog(sid, "cache", "acted", "fresh", cacheOf(sid)?.tokens || insCache.get(sid)?.tokens || 0);
    return invoke("handoff_fresh", { sessionId: sid }).then((r) => {
      if (r.session_id) setActive(null, r.session_id);
      toast(r.detail);
    }, (e) => toast(`Couldn't start it: ${e}`));
  }
  // Compact, from the hand-off card, the cache's Compact now (counted as what saved it) or the auto-compact heads-up.
  if (act === "ins-compact" || act === "ins-compact-warm" || act === "ins-compact-auto") {
    if (act === "ins-compact") insLog(sid, "handoff", "acted", "/compact");
    else if (act === "ins-compact-warm") insLog(sid, "cache", "acted", "compact", cacheOf(sid)?.tokens || 0);
    else insLog(sid, "autocompact", "acted", "compact");
    // Mid-step, Cue's Compact is refused ("do this when its turn ends"): /compact goes as a queued message instead.
    const go = sessionOf(sid)?.state === "working"
      ? () => invoke("send_to_session", { sessionId: sid, text: "/compact", images: [], now: false }).then(() => toast("/compact queued: it compacts when this step ends"), (e) => toast(`Couldn't: ${e}`))
      : () => invoke("session_command", { sessionId: sid, action: "compact" }).then((m) => toast(m), (e) => toast(`Couldn't: ${e}`));
    return insRun(sid, "Compacting…", go);
  }
  if (act === "ins-fill") {
    // Your click on the command is the message: it goes as you'd send it, like Compact or a quick phrase.
    insSent(sid, d.insCmd, false);
    insLog(sid, "handoff", "acted", d.insCmd);
    return insRun(sid, `Sending ${d.insCmd}…`, () => invoke("send_to_session", { sessionId: sid, text: d.insCmd, images: [], now: false })
      .then((r) => toast(`${d.insCmd} sent${r ? ` · ${String(r).replace(/^sent /, "")}` : ""}`), (e) => toast(`Couldn't send: ${e}`)));
  }
}
/** A tap that sends something: says so in the insight at once (its buttons give way to `note`), then clears
 *  once the Mac has answered. By then the session's own state says what came of it (writing its hand-off
 *  note, compacting). A second tap while it's on its way does nothing. */
function insRun(sid, note, go) {
  if (insBusy.has(sid)) return;
  insHotSeen.add(sid);
  insBusy.set(sid, sessionOf(sid)?.state === "working" ? `${note.replace(/…$/, "")}: queued for when this step ends…` : note);
  renderMain();
  return go().finally(() => { insBusy.delete(sid); insOpen.delete(sid); renderMain(); });
}

document.addEventListener("input", (e) => {
  if (!e.target.matches?.("[data-ins-search]")) return;
  insQuery = e.target.value;
  renderMain();
});
document.addEventListener("keydown", (e) => {
  if (!insChoosing) return;
  if (e.key === "Escape") { e.stopPropagation(); insChoosing = ""; insQuery = ""; renderMain(); }
  else if (e.key === "Enter" && e.target.matches?.("[data-ins-search]")) { e.preventDefault(); document.querySelector(".ins-pop .ins-pop-row")?.click(); }
}, true);
document.addEventListener("mousedown", (e) => {
  if (insChoosing && !e.target.closest?.(".ins-pop, [data-act=ins-choose]")) { insChoosing = ""; insQuery = ""; renderMain(); }
});

// ---------- Settings → Insights ----------
/** Settings → Insights: each insight on or off, the last 30 days, and the hand-off skill per host and agent. */
let insMachines = null;   // the machines you added (+ New → Machine), read once when this opens
let insFound = null;      // host → its hand-off skills for Claude Code (null while being read)
let insHostCmds = {};     // host → every Claude Code command it has, to choose from (null while being read)
let insFoundAsks = 0;
/** Ask each host which hand-off skills it has; while any is still being read (or a newer list is on its
 *  way, the one it had shown till then), ask again shortly: for as long as a host's list is open (another
 *  machine's can take a while over SSH), else a minute at most. */
function insFindSkills(fresh) {
  if (fresh) insFoundAsks = 0;
  invoke("insight_host_skills", { fresh: !!fresh }).then((r) => {
    insFound = r?.hosts || {};
    insHostCmds = r?.commands || {};
    renderMain();
    if ((r?.reading || Object.values(insFound).some((v) => v === null)) && (++insFoundAsks < 40 || insChoosing)) setTimeout(() => insFindSkills(false), 1500);
  }).catch(() => {});
}
/** Settings → Insights: the list a host and agent's hand-off skill is chosen from, while it's open:
 *  searchable, skills first. Claude Code's are the host's own (your home folder's there, and each of its folders' Cue has read); the
 *  other agents' logs don't list theirs, so only Cue's hand-off is offered for them. */
function insPicker(k, h, a, chosen) {
  if (insChoosing !== k) return "";
  const all = a === "claude" ? insHostCmds[h.key] : [];
  // Every command it has (the list scrolls), as its "/" menu has them; relay's say they work only in a relay lead.
  const choices = (all || []).filter((c) => c.kind !== "cue" && c.name !== "compact")
    .sort((x, y) => (x.kind === "skill" ? 0 : 1) - (y.kind === "skill" ? 0 : 1) || x.name.localeCompare(y.name));
  const q = insQuery.trim().toLowerCase().replace(/^\//, "");
  const shown = choices.filter((c) => !q || c.name.toLowerCase().includes(q) || (c.desc || "").toLowerCase().includes(q));
  const cueRow = q && !"cue's hand-off handoff".includes(q) ? "" : `<button class="ins-pop-row ${chosen === CUE_HANDOFF ? "on" : ""}" data-act="ins-skill" data-key="${esc(k)}" data-name="${CUE_HANDOFF}"><span>Cue's hand-off</span><span class="dim">Writes HANDOFF.md; Continue starts the next session from it. Any agent</span></button>`;
  const empty = a !== "claude" ? "" : `<div class="ins-pop-empty dim">${all === null ? "Reading its commands…" : choices.length ? "Nothing matches." : "No commands found."}</div>`;
  return `<div class="ins-pop down" role="dialog" aria-label="Choose the hand-off skill">
      <input id="ins-search" class="ins-search" data-ins-search="${esc(k)}" value="${esc(insQuery)}" placeholder="Search ${esc(h.name)}'s ${esc(agentName(a))} commands…" autocomplete="off" spellcheck="false">
      <div class="ins-pop-list">${cueRow}${shown.length ? shown.map((c) => `<button class="ins-pop-row ${c.name === chosen ? "on" : ""}" data-act="ins-skill" data-key="${esc(k)}" data-name="${esc(c.name)}"><span class="mono">/${esc(c.name)}</span><span class="dim">${c.name.startsWith("relay:") ? "In a relay lead only. " : ""}${esc(insShort(c.desc || ""))}</span></button>`).join("") : empty}</div>
      <div class="ins-pop-foot"><span class="grow"></span><span class="dim">Esc to close</span></div>
    </div>`;
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
      // What it uses now: the one you chose, else its /handoff (that exact name), else Cue's own.
      const what = name ? `${name === CUE_HANDOFF ? "<span>Cue's hand-off</span>" : `<span class="mono">/${esc(name)}</span>`}<span class="dim">chosen</span>`
        : (found || []).includes("handoff") ? `<span class="mono">/handoff</span><span class="dim">its own</span>`
        : a === "claude" && found ? `<span>Cue's hand-off</span>`
        : `<span class="dim">/handoff if it has one, else Cue's hand-off</span>`;
      const acts = `<span class="grow"></span>${name ? `<button class="btn small" data-act="ins-skill-forget" data-key="${esc(k)}">Forget</button>` : ""}<button class="btn small" data-act="ins-choose" data-key="${esc(k)}">${name ? "Change" : "Choose"}</button>`;
      return `<div class="ins-skill-row">${insPicker(k, h, a, name)}<span class="ins-skill-agent">${esc(agentName(a))}</span>${what}${acts}</div>`;
    }).join("");
    return setRow(esc(h.name), `<div class="ins-skill-rows">${lines}</div>`, "");
  }).join("");
  return `
    <div class="set-group">Insights</div>
    ${setRow("What they are", "Small notes from a session's own log, with the evidence. Each one offers what that session's machine already has, and sends it only when you click, or where you turn on a hand-off before the cache runs out. Last 30 days: " + esc(insCountsLine()), "")}
    ${setRow("Hand off", "When a session's turns cost several times what its early ones did, offers its hand-off skill (else Cue's own: a note of its own, HANDOFF-<its id>.md, then Continue in a new session) or /compact.", toggle("insights.handoff", st.insights?.handoff ?? true))}
    ${setRow("Cache about to expire", "On a waiting card, the last minutes before its cached context runs out, and once it has. Claude Code says when; other agents' logs don't.", toggle("insights.cache", st.insights?.cache ?? true))}
    ${setRow("Hand off before the cache runs out", "Off by default: Cue would type into your sessions unasked. On, every Claude Code session on this Mac waiting on you with 150k or more of context and an hour's cache gets Cue's hand-off a minute before its cache runs out, unless something is typed in its box, it has background agents at work, or Cue can't see its box (it reads tmux panes, which Cue's own terminal uses). When you're back you choose: continue here, or a new session from the note. A session's own “Hand off if I don't come back” does this for that one alone.", toggle("insights.ahead", st.insights?.ahead ?? false))}
    ${setRow("Near auto-compact", "When a session nears where Claude Code has lately compacted sessions on its model on its own, so you can compact or hand off first. The point moves between versions: Cue goes by the least of your last few.", toggle("insights.autocompact", st.insights?.autocompact ?? true))}
    ${setRow("Could have delegated", "On a waiting card, when a session did a lot of reading and searching itself (a background agent would have sent back a summary), or ran long across several areas of its project (relay, when it's on, or an agent per area).", toggle("insights.delegate", st.insights?.delegate ?? true))}
    ${setRow("Queue instead of interrupting", "While a session works and you've typed something: Now stops what it's doing, Queue keeps it.", toggle("insights.queue", st.insights?.queue ?? true))}
    <div class="set-group">Hand-off skill, by host</div>
    ${setRow("How it's set", "Each host's agents have their own skills. Cue offers the one you choose here, kept for that host and agent; else a host's /handoff (only that exact name); else its own hand-off.", "")}
    ${hostRows}`;
}
let insCounts = null;
function insCountsLine() {
  if (!insCounts) { insCounts = []; invoke("insight_counts").then((r) => { insCounts = r || []; renderMain(); }).catch(() => {}); }
  const by = Object.fromEntries(insCounts.map((r) => [r.rule, r]));
  const part = (rule, label) => (by[rule] ? `${label}: shown ${by[rule].shown}, acted ${by[rule].acted}` : "");
  const saved = by.cache?.tokens ? ` · ${insTok(by.cache.tokens)} tokens not re-written` : "";
  // Notes before the cache ran out: written, then what you did when you came back (here or from the note).
  const ahead = by.ahead ? `Before cold: ${by.ahead.shown} notes, ${by.ahead.acted} came back to` : "";
  return [part("handoff", "Hand off"), part("cache", "Cache") + saved, ahead, part("autocompact", "Auto-compact"), part("queue", "Queue"), part("delegate", "Delegate")].filter(Boolean).join(" · ") || "Nothing shown yet.";
}
