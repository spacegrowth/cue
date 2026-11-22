// Cue UI chat — sending a message, "!" shell lines, and by-the-way questions.
/**
 * Send from a text box: your message shows in the chat right away ("Sending…"), the box clears,
 * and it settles to "Sent · via …" once delivered. On failure the text goes back in the box.
 */
const outbox = [];          // { sid, text, images, at, via } until the session's thread shows it
// Images on their way to a session's machine, by session: { sent, total, count, machine } while it copies.
const uploading = new Map();
const fmtBytes = (n) => n < 1024 * 1024 ? `${n ? Math.max(1, Math.round(n / 1024)) : 0} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`;
/** "Sending…", or how far the images' copy to its machine has got. */
function sendingMeta(sid) {
  const u = uploading.get(sid);
  if (!u) return "Sending…";
  const what = u.count > 1 ? `${u.count} images` : "image";
  return `Copying ${what} to ${machName(u.machine)} · ${fmtBytes(u.sent)} of ${fmtBytes(u.total)}`;
}
/** Cue's word on a copy ("upload"): the sending bubble's line follows it, without redrawing the pane. */
function onUpload(p) {
  if (p.done) uploading.delete(p.session_id);
  else uploading.set(p.session_id, p);
  for (const el of document.querySelectorAll(".msg-line.ml-sending[data-sid] .ml-words")) {
    if (el.parentElement.dataset.sid === p.session_id) el.textContent = sendingMeta(p.session_id);
  }
}
// ---------- "!": a shell line Cue runs for you, in the session's folder, shown as a card (shell.rs) ----------
// Never sent to the agent: the card offers that. For the + New session box, in its folder ("new").
const ranLog = new Map();   // sid | "new" -> [{ line, cwd, output, code, ms, at, running, err }]: this window's, oldest first
// "! " (a space after it) makes it one; "!!!" or "!important" is still a message.
const isBang = (text) => /^!\s+\S/.test(text.trimStart());
/** Already a "!" line as you type it (the hint and the Run button show once you've typed "! "). */
const bangTyped = (text) => /^!\s/.test(text.trimStart());
async function runLine(who, line, cwd = null, machine = null) {
  line = line.replace(/^\s*!\s+/, "").trim();
  if (!line) return;
  const log = ranLog.get(who) || ranLog.set(who, []).get(who);
  const e = { line, cwd: cwd || sessionOf(who)?.cwd || "", machine: machine ?? (who === "new" ? newMachine : sessionOf(who)?.machine || ""), output: "", code: 0, ms: 0, at: now(), running: true, err: null };
  log.push(e);
  renderMain();
  try {
    const r = await invoke("run_line", who === "new" ? { cwd, line, machine: e.machine } : { sessionId: who, line });
    Object.assign(e, r, { running: false });
  } catch (x) { e.err = String(x); e.running = false; }
  renderMain();
}
/** The card: the line, what it printed, and what to do with it. */
function ranCard(who, e, n) {
  const status = e.running ? `<span class="dot-live"></span>Running…` : e.err ? esc(e.err) : `${e.timed_out ? "stopped after 2 min" : e.code === 0 ? "ok" : `exit ${e.code}`} · ${e.ms < 1000 ? `${e.ms} ms` : `${(e.ms / 1000).toFixed(1)} s`}`;
  const acts = e.running || e.err ? "" : ` · <button class="q-now" data-act="ran-again" data-who="${esc(who)}" data-n="${n}">Run again</button> · <button class="q-now" data-act="ran-copy" data-who="${esc(who)}" data-n="${n}">Copy</button>${who !== "new" ? ` · <button class="q-now" data-act="ran-send" data-who="${esc(who)}" data-n="${n}">Send to ${esc(agentName(sessionOf(who)?.harness))}</button> · <button class="q-now" data-act="ran-term" data-who="${esc(who)}" data-n="${n}">Run in its terminal</button>` : ""}`;
  return `<div class="cv-you mg-you cv-ran"><pre class="ran-out"><span class="ran-cmd">${esc(e.line)}</span>${e.output ? `\n${esc(e.output)}` : e.running || e.err ? "" : `\n<span class="ran-dim">(no output)</span>`}</pre><div class="mg-f"><span class="mg-st">You ran · in ${esc(homeless(e.cwd))}${e.machine ? ` on ${esc(machShort(e.machine))}` : ""} · ${status}${acts}</span></div></div>`;
}
/** The line and its output, as a message: what you checked becomes context for the agent. */
const ranAsMessage = (e) => `I ran \`${e.line}\` in ${homeless(e.cwd)}${e.machine ? ` on ${e.machine}` : ""}${e.code ? ` (exit ${e.code})` : ""}:\n\`\`\`\n${e.output || "(no output)"}\n\`\`\``;
/** Send box `key` to session `sid` through `call` (ui/box/send.js boxSend): "/btw", "/mcp" and "!" lines are dealt with
 *  here, a slash command this session doesn't have is refused, and your message shows in the chat while it goes (the
 *  outbox). `guard`: the restarting / asking-first guards look at `sid` (a reply to a card goes without them).
 *  boxSend's outcome. */
function deliver(sid, key, call, guard = false) {
  return boxSend({ sid }, key, {
    call, guard,
    before: (text, d) => {
      if (isParked(sid) && (text || d.images.length)) setParked(sid, false);   // you replied: decided
      // "/btw …": a side question, in its panel (the session never sees it).
      const bt = /^\/btw(?:\s+([\s\S]*))?$/.exec(text);
      if (bt && sessionOf(sid)?.harness === "claude") { delete drafts[key]; openBtw(sid, bt[1] || ""); return false; }
      // "/mcp" in a hosted Claude Code session: Cue's MCP servers list (chat/mcp.js), which headless Claude Code doesn't have.
      if (/^\/mcp(\s|$)/.test(text) && isHostedClaude(sid)) { delete drafts[key]; openMcp(sid); return false; }
      // "! …": a shell line, run by Cue in the session's folder (the card offers to send it on).
      if (isBang(text) && !d.images.length) { delete drafts[key]; runLine(sid, text); return false; }
      // " /…" (a leading space) is a message that starts with a slash; "/…" is a command. Keep the space.
      const typed = text.startsWith("/") && /^\s/.test(d.text) ? ` ${text}` : text;
      // A command this session doesn't have ("/reomte"): it would just fail in the terminal. Say so instead.
      if (typed.startsWith("/") && cmdOf(key) === null) { toast(`“${typed.split(/\s/)[0]}” isn't a command in this session. Use “Send as a message instead” under the box.`); return false; }
      return typed;
    },
    pending: (text, images) => { const entry = { sid, text, images, at: now(), via: null }; outbox.push(entry); return entry; },
    draw: drawAtChatEnd,
    sent: (r, entry) => {
      entry.via = String(r || "sent").replace(/^sent via /, "");
      sent(sid, entry.text, entry.images.length, r);
      renderMain();
    },
    // Out of the chat; boxSend puts the words back in the box (ahead of anything typed since) and says why.
    failed: (e, entry) => { outbox.splice(outbox.indexOf(entry), 1); return false; },
  });
}
/** Has the session's own thread (or queue) caught up with this outbox entry? Once Cue says it's sent
 *  (`via`), Cue has noted it: a message of yours since you pressed send is this one, whatever became of
 *  its text (spacing, image paths, or glued to one you stopped: then it shows what you typed, marked as
 *  gone with the one above). Before that, by how it starts: the first 300
 *  characters that aren't spaces (a terminal may join lines; a long one may be cut short). */
const msgStart = (t) => String(t ?? "").replace(/\s+/g, "").slice(0, 300);
function landed(o) {
  const s = sessionOf(o.sid);
  if (!s) return !!o.via;
  const want = msgStart(o.text);
  // Kept in Cue: shown as the kept bubble, once the session's state carries it (from a machine that can
  // take a moment; dropped before then, the message would vanish and come back). Once seen, it's that
  // bubble's, even if you take it back to edit; and it doesn't wait forever.
  // Kept before the send even returned (the session's state can come first): the kept bubble is this one, so it
  // never shows twice, "Not sent yet" and a Sending copy (#439).
  if (want && s.held && msgStart(s.held.text).includes(want) && (!o.via || /^kept in Cue/.test(o.via))) o.seenHeld = true;
  if (o.seenHeld) return true;
  if (/^kept in Cue/.test(o.via || "") && now() - o.at > 30000) return true;
  // Nothing typed (just an image): no words to tell it by, so only a message from the moment you sent.
  const since = (e) => e && e.role === "you" && e.at_ms >= o.at - (want ? 5000 : 1000);
  const mine = (e) => since(e) && msgStart(e.text).startsWith(want);
  if ((s.thread || []).some(mine) || mine({ role: "you", ...s.queued })) return true;
  return !!o.via && (s.thread || []).some((e) => e.role === "you" && e.at_ms >= o.at - 1000);
}
/** What a session's thread now shows, out of the outbox (with each new state, in both windows: app.js, mini.js). */
function dropLanded() {
  for (let n = outbox.length - 1; n >= 0; n--) if (outbox[n].via && landed(outbox[n])) outbox.splice(n, 1);
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
/** Ask `q` by the way (ui/box/send.js boxSend, no guards: the session is never written to): it shows in the panel at
 *  once, the panel's box emptied, and its answer, or why there's none, in its place. */
function askBtw(sid, q) {
  const key = `btw:${sid}`;
  return boxSend({}, key, {
    text: q, guard: false,
    pending: (text) => {
      const log = btwLog.get(sid) || btwLog.set(sid, []).get(sid);
      const e = { q: text, a: null, err: null };
      log.push(e);
      delete drafts[key];   // a "/btw …" from the chat's box too: the panel's box starts empty
      return e;
    },
    call: (text) => invoke("btw", { sessionId: sid, question: text }),
    sent: (a, e) => { e.a = a; renderMain(); },
    failed: (err, e) => { e.err = err; renderMain(); return true; },
  });
}
/** askBtw, and the log entry it made (null if it never got one). */
function askBtwEntry(sid, q) {
  const n = (btwLog.get(sid) || []).length;
  const p = askBtw(sid, q);
  const e = btwLog.get(sid)?.[n] || null;
  return Promise.resolve(p).then(() => e, () => e);
}
// Fork: the conversation copied into a new session that keeps going ("<name>-fork", then -fork-2…), in Active
// beside this one, which goes on untouched. From ⋯ it starts with nothing said; under a btw answer, that side
// question and its answer are its first message (the copy that answered was never saved), so it picks up there.
let btwForking = null;     // the fork being started: "menu", or the answer's index
async function forkBtw(sid, i) {
  const e = i == null ? null : btwLog.get(sid)?.[i];
  if (btwForking != null || (i != null && e?.a == null)) return;
  btwForking = i ?? "menu";
  renderMain();
  const message = e ? `On the side I asked: ${e.q}\n\nYou answered:\n\n${e.a.trim()}\n\nLet's take this further from here.` : "";
  try { toast(`Forked as ${(await invoke("fork_session", { sessionId: sid, message })).name}`); } catch (x) { toast(`Couldn't fork it: ${x}`); }
  btwForking = null;
  renderMain();
}
// ---------- by the way, offered on a message waiting for a busy session (#535) ----------
// Kept in Cue until its turn ends, a message can go as a side question instead, at once (out of Cue's keeping,
// answered by a copy that can't run anything), and back if that didn't help. Past 45 minutes it says it may wait a while.
const LONG_TURN_MS = 45 * 60_000;
const btwKept = new Set();   // `${sid}:${at_ms}`: Keep it queued, said for that message
/** When its current turn started, while it works (sessions/mod.rs turn_started): the first bar segment since it last
 *  waited on you. */
function turnStart(s) {
  if (s?.state !== "working" && s?.state !== "agent") return 0;
  const segs = s.segments || [], w = segs.findLastIndex((g) => g.kind === "waiting");
  return segs[w + 1]?.start_ms ?? s.since_ms ?? 0;
}
/** The offer under kept message `h` of session `s`, or "": only a Claude Code session's (by the way asks a copy of
 *  its conversation), and words alone (a side question takes no images). Its turn 45 minutes on: why it may wait. */
function btwOffer(s, h) {
  const t = turnStart(s), sid = s.session_id;
  if (s.harness !== "claude" || h.images?.length || !h.text.trim() || btwKept.has(`${sid}:${h.at_ms}`)) return "";
  const d = esc(sid), long = t && now() - t >= LONG_TURN_MS;
  return `<div class="btw-offer">${long ? `<div class="btw-offer-why">It's been at this ${esc(hm(now() - t))}, so this may wait a while.</div>` : ""}
    <div class="btw-offer-acts"><button class="btn primary" data-act="btw-offer" data-sid="${d}">Ask it now, by the way</button><button class="btn" data-act="btw-keep" data-sid="${d}" data-at="${h.at_ms}">Keep it queued</button></div>
    <div class="btw-offer-note">A side question gets an answer from what it already knows. It can't run anything or change what it's doing.</div></div>`;
}
/** "1h 04m", "52m". */
const hm = (ms) => { const m = Math.floor(ms / 60_000); return m >= 60 ? `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, "0")}m` : `${m}m`; };
/** Ask it now, by the way: the kept message out of Cue's keeping (too late if it already went), then asked on the
 *  side, its answer with Queue it again under it. */
async function btwFromKept(sid) {
  let h;
  try { h = await invoke("unhold_message", { sessionId: sid }); } catch (e) { return toast(`Couldn't take it back: ${e}`); }
  if (!h) return toast("Too late: it had already gone");
  forgetSent(sid);
  openBtw(sid);
  const e = await askBtwEntry(sid, h.text);
  if (e) { e.kept = h.text; renderMain(); }
}
/** Queue it again: the side question's words back to the session, kept in Cue till its turn ends as before. */
function btwRequeue(sid, i) {
  const e = btwLog.get(sid)?.[i];
  if (!e?.kept || e.requeued) return;
  e.requeued = true;
  renderMain();
  invoke("send_to_session", { sessionId: sid, text: e.kept, images: [], now: false })
    .then(() => toast("Queued again: it reads this when it finishes"), (x) => { e.requeued = false; renderMain(); toast(`Couldn't queue it: ${x}`); });
}
const requeueBtn = (sid, i, e) => e.kept ? (e.requeued ? `<span class="dim btw-requeued">Queued again</span>` : `<button class="btw-fork" data-act="btw-requeue" data-sid="${esc(sid)}" data-i="${i}">Queue it again</button>`) : "";
const forkBtn = (sid, i) => `<button class="btw-fork" data-act="btw-fork" data-sid="${esc(sid)}" data-i="${i}" ${btwForking != null ? "disabled" : ""}>${btwForking === i ? "Forking…" : "Fork"}</button>`;
function btwPanel(sid) {
  if (!sid || btwFor !== sid) return "";
  const log = btwLog.get(sid) || [];
  // On its crew's page (team_page.js) it is the lead it asks, and it says so.
  const lead = typeof crewPageOn === "function" && crewPageOn(sid);
  const rows = log.map((e, i) => [e, i]).reverse().map(([e, i]) => `<div class="btw-q">${esc(e.q)}</div>${e.a != null ? `<div class="btw-a msg">${md(e.a)}</div><div class="btw-a-acts">${requeueBtn(sid, i, e)}${forkBtn(sid, i)}</div>` : e.err ? `<div class="btw-a bad">${esc(e.err)}</div>${e.kept ? `<div class="btw-a-acts">${requeueBtn(sid, i, e)}</div>` : ""}` : `<div class="btw-a dim"><span class="dot-live"></span>Thinking…</div>`}`).join("");
  return `<div class="btw-pop">
    <div class="btw-head">${lead ? "BY THE WAY · ASKS THE LEAD, NOT PART OF ITS CHAT" : "BY THE WAY · NOT PART OF THE CHAT"}${log.length ? `<button class="btw-clear" data-act="btw-clear" data-sid="${esc(sid)}">Clear</button>` : ""}<button class="btw-x" data-act="btw-close" aria-label="Close">×</button></div>
    ${boxHtml({ look: "btw", key: `btw:${sid}`, placeholder: lead ? "Ask the lead without interrupting it…  ↵" : "Ask without interrupting it…  ↵" })}
    <div class="btw-body">${rows || `<div class="dim btw-empty">${lead ? "A copy of the lead's conversation answers, so neither the lead nor its crew sees it." : "A copy of this conversation answers, so the session never sees it."} Each question costs about one turn.</div>`}</div></div>`;
}
