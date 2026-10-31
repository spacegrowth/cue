// Cue UI — insights: small, evidence-first notes about how a session is going, from its own log.
// Each one shows what's happening (a chart, a countdown), what you could do with what this session's
// machine already has (its own skills and commands), and sends it when you pick one. Cue never sends
// anything you didn't click. Its one prompt of its own is the hand-off (ipc/insights.rs), for a host
// with no hand-off skill: yours always comes first. What each insight did, and what
// you did with it, goes to insight_log (shown / acted / later / dismissed) so Settings can count it.
//
// SHORTCUT: fixed thresholds below, the same for everyone. Upgrade path: each person's own baseline
// from their turns history (the turns table), tuned by what they act on and dismiss.

// When to offer a fresh session: its context has used this share of its model's window (the window as its
// status line said it; a log that doesn't say is taken as 200k) and grown this many times from its start, and
// it's at a break (handoffPick: its work landed, nothing edited since) or well into the window (`late`).
// Its cache doesn't lower the bar: a hand-off into a cold session pays the same cold write as any reply, so
// being away is no reason. The case for a fresh session is quality on a filling window, not what each reply
// re-reads: all models get less reliable as their input grows.
const HANDOFF = { share: 0.6, late: 0.8, growth: 3, window: 200000 };
const HANDOFF_CONTEXT = 150000;   // the least context at which a cold resume is worth a look first (insColdResume)
const CACHE_WARN_MS = 10 * 60000;   // the countdown shows in an hour's cache's last 10 minutes; a 5-minute one has
                                    // none (it runs out while you read; the Mac's heads-up goes by the same: hub cache_tick)
const CACHE_MIN_TOKENS = 50000;     // smaller contexts are cheap to rebuild: not worth a note
const CACHE_COLD_FOR_MS = 12 * 3600000;
// Compacting while it's still cached reads the context back at the cache's price and leaves a short summary,
// so a reply after you're back writes that, not all of it. Below this, the summary and its own reply cost
// about what writing the context again would.
const COMPACT_WARM_MIN = 100000;
// A hand-off note before the cache runs out (hub/activity.rs hand_off_ahead): a session's "Hand off if I don't
// come back" works on an hour's cache (a 5-minute one runs out while you read: Write hand-off now is yours to press).
const AHEAD_TTL_MS = 30 * 60000;

// One switch in Settings → Insights turns them all on (off until you do); config.rs fills in each one from it.
const insOn = (rule) => state.settings?.insights?.[rule] === true;
/** Hidden for a while by a nudge's "Hide for a day / a week": until then, no strip or card line asks for anything
 *  (a hand-off you started still shows what's next). */
const insSnoozed = () => (state.settings?.insights?.snooze || 0) > now();
const insNudge = (rule) => insOn(rule) && !insSnoozed();
/** A hand-off note written before its cache ran out (by you, its "Hand off if I don't come back", or Settings),
 *  not continued from or replied past yet: its hand-off as the Mac keeps it, else null. */
const aheadOf = (sid) => { const ho = state.skill_handoffs?.[sid]; return ho?.ahead > 0 && !ho.next ? ho : null; };
const aheadWatched = (sid) => (state.ahead_watch || []).includes(sid);
/** Whether a ticked "Hand off if I don't come back" still applies to a session: Claude Code on this Mac (the
 *  Mac's timer only reads its own cards' logs), an hour's cache, enough context to matter, and no note already
 *  written. It's no longer offered; one ticked before shows, so you can see it's on and untick it. */
function aheadCan(sid) {
  const s = sessionOf(sid), c = cacheOf(sid);
  return !!s && insOn("handoff") && s.harness === "claude" && !s.machine && (c?.ttl || 0) >= AHEAD_TTL_MS && c.tokens >= CACHE_MIN_TOKENS && !aheadOf(sid) && (c.until_ms > now() || aheadWatched(sid));
}
/** Ticked on a session whose cache has already run out: it goes from your next reply. */
const aheadLater = (sid) => aheadWatched(sid) && !((cacheOf(sid)?.until_ms || 0) > now());
const watchText = (sid) => aheadLater(sid)
  ? "<b>Hands off if you don't come back,</b> from your next reply: its cache has run out, so there's nothing to get ahead of until then."
  : "<b>Hands off if you don't come back:</b> a minute before its cache runs out, it writes a hand-off note, and you choose when you're back.";
// sid → the context size its hand-off insight waits for after "Remind me at …" (until Cue restarts). Nothing
// else hides an insight: it shows whenever its numbers say so.
const insLater = new Map();
try { localStorage.removeItem("cue.insLater"); } catch {}   // what an earlier build kept
const insLaterOf = (sid) => insLater.get(sid) || 0;
const insShownOnce = new Set(); // "rule:sid": logged as shown in this window
const insCache = new Map();     // sid → { until_ms, tokens, start, window, last_ms, cold_x, cold_cents, auto_at }, for sessions whose log isn't open here
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

// sid: its hand-off's note is written and Cue has started continuing it by itself (once; a failure leaves the ⋯ item).
const insAutoTried = new Set();
const AUTO_CONTINUE_MS = 30 * 60000;   // a hand-off sent longer ago than this (before a restart, say) waits for your click
/** A hand-off sent and not continued yet (kept on the Mac, so it's here after a restart too): one status line under
 *  the chat with where it is: writing its note, then opening the fresh session here, then this one closes. Once the
 *  note is written (its turn ended), Cue continues by itself: nothing to press. A failed or old one offers it in ⋯. */
function handoffNext(sid, s, ho) {
  if (ho.ahead) return aheadLine(sid, s, ho);
  const writing = s.state === "working" || !!s.compacting_ms;
  const going = insContinuing.has(sid);
  const fresh = now() - ho.sent_ms < AUTO_CONTINUE_MS;
  if (!writing && !going && fresh && !insAutoTried.has(sid)) {
    insAutoTried.add(sid);
    queueMicrotask(() => insAct("ins-continue", { sid }));
  }
  const stuck = !writing && !going && (!fresh || insAutoTried.has(sid));
  const where = writing ? "writing its note…" : stuck ? "note written · <b>⋯ → Continue in a fresh session</b>" : "note written · opening the fresh session here…";
  return `<div class="cv-status ins-status">${BULB}<span><b>Continuing in a fresh session:</b> ${where}</span></div>`;
}
/** ⋯ → Continue in a fresh session, when a hand-off's note is written and Cue didn't continue it by itself. */
function handoffNextItem(sid, s, ho) {
  if (ho.ahead || !ho.note_ms && s.state === "working") return "";
  const stuck = s.state !== "working" && !s.compacting_ms && !insContinuing.has(sid) && (now() - ho.sent_ms >= AUTO_CONTINUE_MS || insAutoTried.has(sid));
  return stuck ? `<button data-act="ins-continue" data-sid="${esc(sid)}">Continue in a fresh session<span>From its hand-off note; this one closes (Closed keeps it)</span></button>` : "";
}

/** A note written before its cache ran out: one status line under the chat. While it's written, say so; then
 *  replying here carries on with all of its context (cheap while the cache lasts), or ⋯ → New session from the note
 *  starts small and leaves this one as it is. */
function aheadLine(sid, s, ho) {
  const c = cacheOf(sid), left = c ? c.until_ms - now() : 0;
  // Written once Cue has seen the note (its time, from its log): asking leave to write it isn't done yet.
  if (!ho.note_ms) return `<div class="cv-status ins-status">${BULB}<span><b>Writing a hand-off note</b> before its cache runs out. Nothing else changes.</span></div>`;
  const why = left > 0
    ? `Its cache lasts ${Math.max(1, Math.ceil(left / 60000))} more min, so replying here is still cheap.`
    : `Cache expired: replying here ${c ? coldCost(c, "").replace(/^ /, "") : "re-writes all of its context"}.`;
  return `<div class="cv-status ins-status">${BULB}<span><b>Hand-off note ready.</b> ${why} <b>⋯ → New session from the note</b> starts small and leaves this one as it is.</span></div>`;
}
/** ⋯ items for a note written ahead: New session from the note, and Continue here (sets the note aside). */
function aheadItems(sid, ho) {
  if (!ho.note_ms) return "";
  const c = cacheOf(sid), warm = !!c && c.until_ms > now();
  return `<button data-act="ins-continue" data-sid="${esc(sid)}">New session from the note<span>Starts small; this one stays as it is</span></button>
    <button data-act="ins-keep" data-sid="${esc(sid)}">Continue here<span>${warm ? "Still cached: its whole context, cheaply" : "Its next reply re-writes its whole context"}; the note is set aside</span></button>`;
}

/** A hand-off that already continued: a status line saying where it carries on now (Close is in the head). */
function handoffContinued(sid, ho) {
  const next = nameOf(ho.next, sessionOf(ho.next)?.project) || "its new session";
  return `<div class="cv-status ins-status">${BULB}<span>It continued in <b>${esc(next)}</b>. Carry on there, and close this one when you're done with it (Closed keeps it).</span><button class="btn small" data-act="ins-goto" data-sid="${esc(ho.next)}">Go to ${esc(next)}</button></div>`;
}

// The open session's hand-off is a chip by the box's quick phrases, "Continue in a fresh session", once the session
// is past its point (handoffDue). A click opens the plan above it: what will happen, in three lines, one button
// that does all of it, Remind me, and the ways to hide nudges. The header's meta line carries "4× its start" in
// amber meanwhile, and the cache's countdown (cacheMeta); the ways out are in ⋯ (handoffMenu). Nothing sits on
// the box for any of it: what's on its way is a status line under the chat (handoffStatus).
const insPlan = new Set();      // sid: its fresh-session plan open above the chip
const HOT_CACHE_MS = 5 * 60000; // a cache this close to running out colors the bar
// sid → what you just tapped ("Handing off…"), until it's done: shown in place of the actions, so nothing
// goes twice and the tap shows at once, before the session's next state arrives.
const insBusy = new Map();

/** The open session's fresh-session numbers, when its context has used most of its window and grown several
 *  times over, at a break or late in the window: else null. */
function handoffDue(sid, s) {
  if (!insNudge("handoff")) return null;
  const f = stepFeeds.get(sid), m = f?.meta;
  if (!m || insStale(s, sid)) return null;
  const window = m.window || HANDOFF.window, share = (m.context || 0) / window;
  if (share < HANDOFF.share || insLaterOf(sid) > m.context) return null;
  // Grown from what the session started at (its first reply, read from the top of its log), not from the
  // first turns read here: a long session's log is only read from near its end, often just a few turns.
  const read = (f.turns || []).map((t) => t.context).filter((c) => c > 0);
  const early = m.start_context || median(read.slice(0, 3)), grew = early ? m.context / early : 0;
  if (grew < HANDOFF.growth) return null;
  const pick = handoffPick(sid, s, f);
  if (!pick.brk && share < HANDOFF.late) return null;
  // What this session's machine has: the hand-off skill you chose for this machine and agent, else one
  // named exactly /handoff (never another with "handoff" in its name: that may be something else); else
  // Cue's own hand-off.
  const cmds = cmdList(sid), chosen = insSkills()[insSkillKey(s)];
  // Relay's skills work only in a relay lead (its ⋯ menu's Relay handoff… asks the same), so never offer them elsewhere.
  const fits = (c) => !c.name.startsWith("relay:") || (crewOf(sid)?.role === "lead" && crewOf(sid)?.plugin === "relay");
  const handoffs = chosen === CUE_HANDOFF ? [] : (chosen ? cmds.filter((c) => c.name === chosen) : cmds.filter((c) => c.name === "handoff")).filter(fits);
  const remind = Math.ceil((m.context * 1.25) / 50000) * 50000;
  return { m, early, grew, share, window, handoffs, remind, why: pick.why };
}
/** How many times the open session has grown from its start, when a fresh session is due (handoffDue): for the
 *  header's meta line. 0 otherwise. */
const handoffGrew = (sid) => handoffDue(sid, sessionOf(sid))?.grew || 0;

const COMMIT_RE = /\bgit\s+(commit|push)\b/;
const TEST_RE = /\b(test|tests|pytest|jest|vitest|cargo\s+(test|nextest)|go\s+test|run\.sh)\b/;
/** Whether the session is at a break, from its own steps: a piece of work just landed (committed, or its tests
 *  passed, with nothing edited since). `why`: what to say about it on the plan (a session that has compacted
 *  before, or spread over several areas, starts fresh better; one mid-change keeps going better with /compact).
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
  const brk = !!landed && !editedSince;
  if (brk) return { brk, why: landed === "committed" ? "Its work is committed, so a fresh session starts clean from here." : "Its tests passed with nothing edited since, so this piece looks done." };
  if (compacted) return { brk, why: "It has compacted before: another summary of a summary loses more, so a written note is safer." };
  if (areas >= 3) return { brk, why: `It has worked across ${areas} areas: a fresh session for what's next keeps them apart.` };
  if (failing || editedSince) return { brk, why: failing ? "It's in the middle of a fix (its tests are failing): ⋯ → Compact keeps it going in place instead." : "It's in the middle of a change: ⋯ → Compact keeps it going in place instead." };
  return { brk, why: "" };
}

/** By the box's quick phrases: Continue in a fresh session, once the session is past its point. A click opens
 *  the plan above it; nothing is sent until you click the plan's button. "" while a hand-off is on its way. */
function handoffChip(sid) {
  const s = sessionOf(sid);
  if (!s || state.skill_handoffs?.[sid]) return "";
  const d = handoffDue(sid, s);
  if (!d) return "";
  insShown(sid, "handoff");
  const open = insPlan.has(sid);
  return `<span class="ins-chip-wrap">${open ? handoffPlan(sid, d) : ""}<button class="qp qp-ins ${open ? "on" : ""}" data-act="ins-plan" data-sid="${esc(sid)}" aria-expanded="${open}">${BULB}Continue in a fresh session</button></span>`;
}
/** The plan above the chip: why, the three things that will happen, the one button that does them (the hand-off
 *  skill, your click being the message, or Cue's own ask when it has none), Remind me, and the ways to hide. */
function handoffPlan(sid, d) {
  const h = d.handoffs[0];
  const go = h ? `data-act="ins-fill" data-ins-cmd="/${esc(h.name)}"` : `data-act="ins-cue"`;
  const note = h ? `as its <span class="mono">/${esc(h.name)}</span> writes it` : `<span class="mono">HANDOFF.md</span>`;
  return `<div class="ins-pop ins-plan" role="dialog" aria-label="Continue in a fresh session">
    <div class="ins-why"><b>${Math.round(d.share * 100)}% of its window,</b> ${Math.round(d.grew)}× what it started with: a fresh session from a note stays sharp.${d.why ? ` ${esc(d.why)}` : ""}</div>
    <ol class="ins-plan-steps">
      <li>It writes a short note of where things stand, ${note}.</li>
      <li>A fresh session opens here from that note, back near ${esc(insTok(d.early))}.</li>
      <li>This one closes. Closed keeps it, chat and all.</li>
    </ol>
    ${insBusy.has(sid) ? `<div class="ins-acts">${insBusyNote(sid)}</div>` : `<div class="ins-acts"><button class="btn primary" ${go} data-sid="${esc(sid)}">Continue in a fresh session</button><button class="btn" data-act="ins-later" data-sid="${esc(sid)}" data-at="${d.remind}">Remind me at ${esc(insTok(d.remind))}</button><span class="dim">Nothing goes until you click.</span></div>`}
    ${insHideRow(sid)}
  </div>`;
}
const insBusyNote = (sid) => `<span class="ins-busy" role="status">${esc(insBusy.get(sid))}</span>`;

/** On the open session's meta line: its cache's countdown, the bulb in a ring with the minutes left (amber in its
 *  last 5), or "cache gone" once it has run out (until replied to). "" when there's nothing to say (cacheWarn). */
function cacheMeta(sid) {
  const cw = cacheWarn(sid);
  if (!cw) return "";
  const s = sessionOf(sid);
  if (state.skill_handoffs?.[sid]?.ahead && !state.skill_handoffs[sid].next) return "";   // its note ahead says where the cache stands
  insShown(sid, "cache");
  const { left, warn } = cw;
  if (left <= 0) return `<span class="mi-cache cold">${BULB}cache gone</span>`;
  const arc = Math.round((left / warn) * 66 * 10) / 10;
  const soon = left <= HOT_CACHE_MS && s?.state !== "working";
  return `<span class="mi-cache ${soon ? "soon" : ""}"><svg class="ins-ring ${soon ? "soon" : ""}" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="10.5" class="track"></circle><circle cx="12" cy="12" r="10.5" class="left" stroke-dasharray="${arc} 66" transform="rotate(-90 12 12)"></circle>${RING_BULB}</svg>cached ${Math.max(1, Math.ceil(left / 60000))} min</span>`;
}
/** ⋯ → the ways out of a cache in its last minutes (Write hand-off now, Compact now), Start fresh once it's gone,
 *  a note written ahead's two ways on, a stuck hand-off's Continue, and the untick of "Hand off if I don't come back". */
function handoffMenu(sid, s) {
  if (!sid || !s) return "";
  const ho = insOn("handoff") && state.skill_handoffs?.[sid];
  if (ho && !ho.next) return ho.ahead ? aheadItems(sid, ho) : handoffNextItem(sid, s, ho);
  const it = state.items.find((i) => i.session_id === sid), ask = !!it && it.kind !== "waiting";
  const cw = cacheWarn(sid), busy = insBusy.has(sid);
  let items = "";
  if (cw && !ask && !busy) {
    const mins = Math.max(1, Math.ceil(cw.left / 60000));
    if (cw.left > 0 && insOn("handoff") && !s.machine) items += `<button data-act="ins-ahead" data-sid="${esc(sid)}">Write hand-off now<span>Cached ${mins} more min: a note to come back to, this session kept as it is</span></button>`;
    if (compactWarm(sid, cw, ask)) items += `<button data-act="ins-compact-warm" data-sid="${esc(sid)}">Compact now<span>Cached ${mins} more min: leaves a short summary in place, read at the cache's price</span></button>`;
    if (cw.left <= 0) items += `<button data-act="ins-fresh" data-sid="${esc(sid)}">Start fresh<span>Its cache is gone: a new session from its last reply, no re-write; this one stays</span></button>`;
  }
  if (aheadCan(sid) && aheadWatched(sid)) items += `<button data-act="ins-watch" data-sid="${esc(sid)}">Don't hand off on its own<span>It hands off if you don't come back before its cache runs out</span></button>`;
  return items;
}
/** Under the open session's chat: a hand-off on its way or continued, a note written ahead, what you just tapped,
 *  the cache's cost once it's gone, and a ticked "Hand off if I don't come back". Nothing sits above the box. */
function handoffStatus(sid, s) {
  if (!sid || !s) return "";
  const pending = insOn("handoff") && state.skill_handoffs?.[sid];
  if (pending && !pending.next) return handoffNext(sid, s, pending);
  if (pending?.next) return handoffContinued(sid, pending);
  if (insBusy.has(sid)) return `<div class="cv-status ins-status">${BULB}<span role="status">${esc(insBusy.get(sid))}</span></div>`;
  const it = state.items.find((i) => i.session_id === sid), ask = !!it && it.kind !== "waiting";
  const cw = cacheWarn(sid);
  let out = "";
  // Gone: what the next reply costs, once (the meta line says "cache gone"; Start fresh is in ⋯).
  if (cw && cw.left <= 0) out += `<div class="cv-status ins-status">${BULB}<span>Cache expired: ${coldCost(cw.c, ask ? "answering" : "its next reply")}.${ask ? "" : " Starting something new? <b>⋯ → Start fresh</b> from its last reply instead."}</span></div>`;
  if (aheadCan(sid) && aheadWatched(sid)) out += `<div class="cv-status ins-status">${BULB}<span>${watchText(sid)}</span></div>`;
  return out;
}

/** At the foot of the plan: hide every nudge for a day or a week, or turn them off (Settings turns them back on). */
const insHideRow = (sid) => `<div class="ins-acts ins-hide"><span class="dim">Hide nudges</span><button class="btn small" data-act="ins-snooze" data-sid="${esc(sid)}" data-days="1">For a day</button><button class="btn small" data-act="ins-snooze" data-sid="${esc(sid)}" data-days="7">For a week</button><button class="btn small" data-act="ins-off" data-sid="${esc(sid)}">Turn off</button></div>`;

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
  if (!insNudge("cache") || !sid) return null;
  const s = sessionOf(sid), c = cacheOf(sid);
  if (!s || s.state === "working" || insStale(s, sid) || !c || c.tokens < CACHE_MIN_TOKENS) return null;
  if (c.ttl && c.ttl <= CACHE_WARN_MS) return null;
  const left = c.until_ms - now(), warn = CACHE_WARN_MS;
  return left > warn || -left > CACHE_COLD_FOR_MS ? null : { c, left, warn };
}
/** One line for a waiting card: the bulb in a ring counting down the cache's last minutes (amber in its last 5,
 *  when replying now is what saves it), the bulb alone once it's gone. `ask`: it waits on a decision (your answer is
 *  what its next reply follows), not a reply. `open` and `compact` are unused now (the open session's ways out are
 *  in ⋯: handoffMenu). "" when it isn't worth saying. */
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
/** On a waiting card: a hand-off on its way, or the session late in its window and grown several times over. */
function handoffLine(sid) {
  if (!insOn("handoff") || !sid) return "";
  const s = sessionOf(sid), m = stepFeeds.get(sid)?.meta, c = insCache.get(sid);
  const pending = state.skill_handoffs?.[sid];
  if (s && pending?.ahead && !pending.next) {
    if (!pending.note_ms) return `<div class="ins-line">${BULB}<span><b>Writing a hand-off note</b> before its cache runs out.</span></div>`;
    const c = cacheOf(sid), left = c ? c.until_ms - now() : 0;
    return `<div class="ins-line">${BULB}<span>${left > 0 ? `<b>Hand-off note ready.</b> Still cached ${Math.max(1, Math.ceil(left / 60000))} more min.` : "<b>Cache expired · hand-off note ready.</b> Open it to continue here or start fresh from the note."}</span></div>`;
  }
  if (s && pending && !pending.next) return `<div class="ins-line">${BULB}<span><b>Continuing in a fresh session</b>${s.state === "working" ? ": writing its note." : "."}</span></div>`;
  if (s && pending?.next) return `<div class="ins-line">${BULB}<span><b>Continued in ${esc(nameOf(pending.next, sessionOf(pending.next)?.project) || "a new session")}.</b> Close this one when you're done with it.</span></div>`;
  // A card can't see whether it's at a break: it says so only late in the window.
  const ctx = m?.context || c?.tokens || 0, start = m?.start_context || c?.start || 0, win = m?.window || c?.window || HANDOFF.window;
  if (!s || insSnoozed() || s.state === "working" || insStale(s, sid) || ctx / win < HANDOFF.late || !start || ctx / start < HANDOFF.growth || insLaterOf(sid) > ctx) return "";
  return `<div class="ins-line">${BULB}<span><b>${Math.round((ctx / win) * 100)}% of its window,</b> ${Math.round(ctx / start)}× its start. Open it for a fresh session.</span></div>`;
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
  if (!insNudge("cache") || !sid) return "";
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

// ---------- the areas a long session worked across ----------

// sid → what it could have delegated (ipc insights_tab delegate_of): no longer shown as an insight of its own, but
// the areas it worked across say why its hand-off goes to a fresh session. From the turns history, which is read
// every 10 minutes: asked every minute is plenty.
let insDelegate = new Map();
async function delPoll() {
  if (document.hidden || !insOn("handoff")) return;
  const sids = [...new Set(state.items.map((i) => i.session_id))].filter(Boolean);
  if (!sids.length) return;
  try {
    const r = (await invoke("insight_delegate", { sessionIds: sids })) || {};
    const next = new Map(Object.entries(r.sessions || {}));
    const changed = JSON.stringify([...next]) !== JSON.stringify([...insDelegate]);
    insDelegate = next;
    if (changed) renderMain();
  } catch {}
}
setInterval(delPoll, 60000);
setTimeout(delPoll, 4000);
/** An area of a project as a short name: "." is its top level, a folder outside it its last part. */
const areaName = (a) => (a === "." ? "top level" : a.split("/").filter(Boolean).at(-1) || a);
/** A board card's insights as one line, the most pressing (a cache in its last minutes, else the hand-off,
 *  the cache), with "+N" for the rest: they're all in its open strip.
 *  `ask`: it waits on a decision, so only its cache counts. */
function insCardLine(sid, ask = false) {
  // A note written ahead says where its cache stands itself.
  const cw = cacheWarn(sid), cache = aheadOf(sid) ? "" : cacheLine(sid, ask);
  const lines = ask ? [cache] : cw && cw.left > 0 && cw.left <= HOT_CACHE_MS ? [cache, handoffLine(sid)] : [handoffLine(sid), cache];
  const [top, ...rest] = lines.filter(Boolean);
  // Ticked "Hand off if I don't come back": said in words on its own line, under the rest, never folded into "+N".
  const watching = !ask && aheadWatched(sid) && aheadCan(sid) ? `<div class="ins-line">${BULB}<span><b>Hands off if you don't come back</b> ${aheadLater(sid) ? "from your next reply" : "before its cache runs out"}.</span></div>` : "";
  return (top && rest.length ? top.replace(/<\/div>$/, `<span class="ins-n">+${rest.length}</span></div>`) : top || "") + watching;
}

// ---------- what you did with them ----------

/** You sent `text` to a session (Queue when `now` is false): count what an insight led to. */
function insSent(sid, text, now_) {
  const c = cacheOf(sid), s = sessionOf(sid);
  if (c && c.tokens >= CACHE_MIN_TOKENS && c.until_ms > now() && insShownOnce.has(`cache:${sid}`)) {
    insLog(sid, "cache", "acted", "", c.tokens);
  }
  // A hand-off skill sent, bare or with your instructions after it (not "… resume", not relay's, which opens
  // the next lead itself): remembered on the Mac until it continues in a new session.
  const ho = /^\/(\S*hand-?off)(?:\s+(?!resume\b)[\s\S]*)?$/i.exec(text.trim());
  if (ho && !ho[1].startsWith("relay:")) invoke("handoff_sent", { sessionId: sid, skill: ho[1], resume: takesResume(sid, ho[1]) }).catch(() => {});
}
function insAct(act, d) {
  const sid = d.sid;
  if (typeof moreFor !== "undefined" && moreFor) moreFor = null;   // an item picked in ⋯ closes it
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
  if (act === "ins-plan") { insPlan.has(sid) ? insPlan.delete(sid) : insPlan.add(sid); return renderMain(); }
  if (act === "ins-later") { insPlan.delete(sid); insLater.set(sid, +d.at || Math.round((stepFeeds.get(sid)?.meta?.context || 0) * 1.25)); insLog(sid, "handoff", "later"); return renderMain(); }
  if (act === "ins-snooze") {
    const days = +d.days || 1;
    insPlan.delete(sid);
    insLog(sid, "handoff", "dismissed", `snooze ${days}d`);
    toast(`Nudges hidden for ${days === 1 ? "a day" : `${days} days`}: Settings → Insights shows them again`);
    return setSetting("insights.snooze", Date.now() + days * 86400000);
  }
  if (act === "ins-off") {
    insPlan.delete(sid);
    insLog(sid, "handoff", "dismissed", "off");
    toast("Nudges off: Settings → Insights turns them on again");
    return setSetting("insights.on", false);
  }
  if (act === "ins-unsnooze") return setSetting("insights.snooze", 0);
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
    return invoke("handoff_keep", { sessionId: sid }).then(() => document.querySelector(`textarea[data-text="${CSS.escape(`s:${sid}`)}"]`)?.focus(), (e) => toast(`Couldn't: ${e}`));
  }
  if (act === "ins-watch") {
    // Only a ticked one shows now, so a click unticks it.
    return invoke("ahead_watch", { sessionId: sid, on: false }).then(() => toast("It won't hand off on its own"), (e) => toast(`Couldn't: ${e}`));
  }
  if (act === "ins-cue") {
    insAutoTried.delete(sid);
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
  // Compact, from the hand-off card or the cache's Compact now (counted as what saved it).
  if (act === "ins-compact" || act === "ins-compact-warm") {
    if (act === "ins-compact") insLog(sid, "handoff", "acted", "/compact");
    else insLog(sid, "cache", "acted", "compact", cacheOf(sid)?.tokens || 0);
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
    insAutoTried.delete(sid);
    return insRun(sid, `Sending ${d.insCmd}…`, () => invoke("send_to_session", { sessionId: sid, text: d.insCmd, images: [], now: false })
      .then((r) => toast(`${d.insCmd} sent${r ? ` · ${String(r).replace(/^sent /, "")}` : ""}`), (e) => toast(`Couldn't send: ${e}`)));
  }
}
/** A tap that sends something: says so in the insight at once (its buttons give way to `note`), then clears
 *  once the Mac has answered. By then the session's own state says what came of it (writing its hand-off
 *  note, compacting). A second tap while it's on its way does nothing. */
function insRun(sid, note, go) {
  if (insBusy.has(sid)) return;
  insBusy.set(sid, sessionOf(sid)?.state === "working" ? `${note.replace(/…$/, "")}: queued for when this step ends…` : note);
  renderMain();
  return go().finally(() => { insBusy.delete(sid); insPlan.delete(sid); renderMain(); });
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
/** Settings → Insights: one switch for all of them, and while they're on, the hand-off skill per host and agent. */
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
/** The one switch, saying plainly what turning it on does. */
const insNudgesRow = (on) => setRow("Show nudges", "A chip by a session's quick phrases to continue in a fresh session once it has used most of its window, its cache's countdown on its meta line, and the ways out in ⋯. Nothing sits on the reply box. Cue also writes a hand-off note on its own just before a waiting session's cache runs out.", toggle("insights.on", on));
function insSettings(st) {
  if (!st.insights?.on) return `<div class="set-group">Nudges</div>${insNudgesRow(false)}`;
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
    <div class="set-group">Nudges</div>
    ${insNudgesRow(true)}
    ${insSnoozed() ? setRow("Hidden", `Until ${esc(new Date(st.insights.snooze).toLocaleString([], { weekday: "short", hour: "numeric", minute: "2-digit" }))}.`, `<button class="btn small" data-act="ins-unsnooze">Show again</button>`) : ""}
    <div class="set-group">Hand-off skill, by host</div>
    ${hostRows}`;
}
