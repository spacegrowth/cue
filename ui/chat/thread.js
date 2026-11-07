// Cue UI chat — the conversation: earlier messages, your bubbles and the agent's steps.
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
  // Past the start of Cue's log: one message from its transcript says whether anything came before. A session that
  // started in Cue has nothing, so it ends here, without offering to load what isn't there (a failed look keeps it).
  // (The transcript's own copy of that first message can be stamped a moment earlier: it doesn't count.)
  if (!fromTranscript && o.done && (cur.it?.harness || cur.s?.harness) === "claude") {
    const top = o.msgs[0] || conversation(cur.it, cur.s)[0];
    const probe = await invoke("transcript_page", { sessionId: sid, beforeMs: top.at_ms, limit: 2 }).catch(() => null);
    o.tdone = Array.isArray(probe) && probe.every((e) => sameMessage(e, top));
  }
  o.busy = false;
  const chat = document.querySelector("[data-chat]");
  if (chat) holdChat = chat.scrollHeight - chat.scrollTop;
  renderMain();
}
/** The same message, from two records of it (Cue's log and the transcript): who said it and what. */
const sameMessage = (a, b) => a.role === b.role && String(a.text || "").trim() === String(b.text || "").trim();
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
  // Only the same words, or a long enough message inside more: a short one ("ok") or none at all (just an image)
  // would hide every message of yours that minute.
  const isQueued = (e) => { const q = s.queued.text.trim(); return !!q && e.role === "you" && Math.abs(e.at_ms - s.queued.at_ms) < 60000 && (e.text.trim() === q || (q.length >= 20 && e.text.includes(q))); };
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
/** The line Cue adds after what you said by voice for a session whose hook can't add it (talk.rs ASK). */
const VOICE_ASK = /\n\n\(Voice: I'm listening, not reading\.[^\n]*\)$/;
/** That line, word for word, as the agent got it (talk.rs ASK): shown when you open it under a voice message. */
const VOICE_ASK_LINE = "(Voice: I'm listening, not reading. Answer in one or two short, plain sentences: no lists, code or file paths unless I ask for them. If there's a link I'll need, put it at the end: I'll see it on screen.)";
/** A message of yours as the chat shows it: said by voice (Cue added a line for the agent), and its words. */
function yourWords(e) {
  const asked = VOICE_ASK.test(e.text);
  return { voice: !!e.voice || asked, text: asked ? e.text.replace(VOICE_ASK, "") : e.text };
}
/** A small note under your message: "+ <label>", which opens to `line`, what Cue (or a prompt hook) added for the
 *  agent, word for word. `key` says which note is open (openAsks). */
function addedNote(key, label, line) {
  const open = openAsks.has(key);
  return `<div class="cv-voice-note"><button class="cv-voice-ask" data-ask-open="${esc(key)}" aria-expanded="${open}">${open ? "−" : "+"} ${label}</button>${open ? `<div class="cv-voice-line">${esc(line)}</div>` : ""}</div>`;
}
/** Your message's bubble. Said by voice: a mic beside it, and inside it the line Cue added for the agent (or will
 *  add, `later`: kept in Cue or queued, it hasn't gone yet). A status list asked for by a prompt hook (`status_ask`)
 *  gets its own note. Each + opens its text word for word (`key`). */
function yourBubble(e, key, style = "", later = false) {
  const said = yourWords(e);
  const notes = (said.voice ? addedNote(key, `Cue ${later ? "will ask" : "asked"} for a short spoken answer`, VOICE_ASK_LINE) : "")
    + (e.status_ask ? addedNote(`${key}:status`, "Status list asked for", e.status_ask) : "");
  const bubble = `<div class="cv-you-text"${style ? ` style="${style}"` : ""}>${linkify(esc(said.text)).replace(/\n/g, "<br>")}${notes}</div>`;
  return said.voice ? `<div class="cv-you-row"><span class="cv-mic" role="img" aria-label="Said by voice">${MIC_ICON}</span>${bubble}</div>` : bubble;
}
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
const hookOpen = new Set();    // "sid|turn": its "After a stop hook" part, opened
/** A turn a Stop hook sent back to work (steps.rs `hooked_ms`): what it did and wrote from then on is the
 *  hook's follow-up, not its answer. */
const hookedPart = (turn, x) => !!turn.hooked_ms && x.at_ms >= turn.hooked_ms;
const turnAll = new Set();     // turns where "Show all" opened every step
const stepDetail = new Map();  // "sid|id" -> { full, output | diff } | "loading"
// Words and steps that come in while you watch appear smoothly, once: patchChat gives a new line the
// `reveal` class as it goes in (a full draw gives nothing that class). This is the longest a new
// paragraph takes to unroll (--rv).
const REVEAL_MS = 900;
/** The working light along the chat's top edge: brisk while steps land (a brighter flick for each new
 *  one, from patchChat), calmer after a quiet spell (a long think), still and amber once Cue thinks it's
 *  stuck. The element outlives the redraws (settleChat carries it over), so its pass never starts over. */
const SWEEP_MS = 1600, CALM_MS = 4200, QUIET_MS = 30000;
/** The pulsing dot's cycle (.dot-live in the CSS). */
const PULSE_MS = 1400;
function sweep(sid, s) {
  if (!(s?.state === "working" || s?.compacting_ms)) return "";
  if ((s.stuck_ms && !s.compacting_ms) || preStart(s)) return `<i class="sweep stalled"></i>`;
  const lastStep = stepFeeds.get(sid)?.turns?.at(-1)?.items?.at(-1)?.at_ms || 0;
  const calm = now() - Math.max(s.doing_ms || 0, lastStep, s.compacting_ms || 0, s.since_ms || 0) > QUIET_MS;
  return `<i class="sweep ${calm ? "calm" : ""}" style="animation-duration:${calm ? CALM_MS : SWEEP_MS}ms" data-sweep></i>`;
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
    // Its first feed: what was already there when the chat opened, not steps landing while you watch.
    stepFeeds.set(sid, { version: r.version, turns: r.turns ?? f?.turns ?? [], meta: r.turns ? r.meta : f?.meta, helpers: (r.turns ? r.helpers : f?.helpers) || [], at: Date.now(), fresh: !f });
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
  let got = null;
  try { got = await invoke("step_detail", { sessionId: sid, ids: want }); } catch {}
  // Couldn't be read now: asked again next time, not kept as "No output".
  want.forEach((id) => (got ? stepDetail.set(`${sid}|${id}`, got[id] || {}) : stepDetail.delete(`${sid}|${id}`)));
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
/** The chat's rows and a turn's items each carry a `data-key`, so patchChat can tell what's new, what
 *  changed and what's untouched between draws: this puts the key on `html`'s outer element. */
const keyed = (html, k) => html.replace(/^<([a-z]+)/i, `<$1 data-key="${esc(String(k))}"`);
/** Keys for one chat: a second row with the same key gets a suffix, so no two rows share one. */
const keyer = () => { const seen = new Map(); return (k) => { const n = (seen.get(k) || 0) + 1; seen.set(k, n); return n > 1 ? `${k}#${n}` : k; }; };
function stepLine(sid, x, all, plain = false) {
  const k = `${sid}|${x.id}`;
  const open = !plain && x.done && all !== stepFlip.has(k);
  return `<button class="stx-line ${x.bad ? "bad" : ""} ${open ? "open" : ""}" data-key="${esc(x.id)}" ${x.done && !plain ? `data-step="${esc(k)}"` : "disabled"}>
    <i class="stx-ic k-${esc(x.kind)}">${STEP_ICON[x.kind] || "•"}</i><span class="stx-what">${esc(x.verb)}${x.subject ? ` <span class="${x.mono ? "mono" : ""}">${esc(x.subject)}</span>` : ""}</span>
    <span class="stx-res">${stepResult(x)}</span><span class="stx-chev">${x.done && !plain ? (open ? "⌄" : "›") : ""}</span></button>${open ? keyed(stepDetailHtml(k, x), `d:${x.id}`) : ""}`;
}
