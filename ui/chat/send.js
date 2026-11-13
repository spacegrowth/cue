// Cue UI chat — sending a message, "!" shell lines, by-the-way questions, and Catch me up.
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
  for (const el of document.querySelectorAll(".cv-you.sending .cv-meta[data-sid]")) {
    if (el.dataset.sid === p.session_id) el.textContent = sendingMeta(p.session_id);
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
  return `<div class="cv-you cv-ran"><pre class="ran-out"><span class="ran-cmd">${esc(e.line)}</span>${e.output ? `\n${esc(e.output)}` : e.running || e.err ? "" : `\n<span class="ran-dim">(no output)</span>`}</pre><div class="cv-meta">You ran · in ${esc(homeless(e.cwd))}${e.machine ? ` on ${esc(machShort(e.machine))}` : ""} · ${status}${acts}</div></div>`;
}
/** The line and its output, as a message: what you checked becomes context for the agent. */
const ranAsMessage = (e) => `I ran \`${e.line}\` in ${homeless(e.cwd)}${e.machine ? ` on ${e.machine}` : ""}${e.code ? ` (exit ${e.code})` : ""}:\n\`\`\`\n${e.output || "(no output)"}\n\`\`\``;
async function deliver(sid, key, call) {
  endDictation(key);
  if (isParked(sid) && (draft(key).text.trim() || draft(key).images.length)) setParked(sid, false);   // you replied: decided
  const d = draft(key);
  const text = d.text.trim();
  // "/btw …": a side question, in its panel (the session never sees it).
  const bt = /^\/btw(?:\s+([\s\S]*))?$/.exec(text);
  if (bt && sessionOf(sid)?.harness === "claude") { delete drafts[key]; return openBtw(sid, bt[1] || ""); }
  // "/mcp" in a hosted Claude Code session: Cue's MCP servers list (chat/mcp.js), which headless Claude Code doesn't have.
  if (/^\/mcp(\s|$)/.test(text) && isHostedClaude(sid)) { delete drafts[key]; return openMcp(sid); }
  // "! …": a shell line, run by Cue in the session's folder (the card offers to send it on).
  if (isBang(text) && !d.images.length) { delete drafts[key]; return runLine(sid, text); }
  if (!text && !d.images.length) return;
  // " /…" (a leading space) is a message that starts with a slash; "/…" is a command. Keep the space.
  const typed = text.startsWith("/") && /^\s/.test(d.text) ? ` ${text}` : text;
  // A command this session doesn't have ("/reomte"): it would just fail in the terminal. Say so instead.
  if (typed.startsWith("/") && cmdOf(key) === null) return toast(`“${typed.split(/\s/)[0]}” isn't a command in this session. Use “Send as a message instead” under the box.`);
  const images = d.images.map(({ name, mime, data }) => ({ name, mime, data }));
  const entry = { sid, text, images, at: now(), via: null };
  outbox.push(entry);
  delete drafts[key];
  drawAtChatEnd();
  try {
    const r = await call(typed, images);
    entry.via = String(r || "sent").replace(/^sent via /, "");
    sent(sid, text, images.length, r);
    renderMain();
  } catch (e) {
    outbox.splice(outbox.indexOf(entry), 1);
    // Back in the box, ahead of anything you've typed there since (that's kept, not written over).
    const now_ = draft(key);
    drafts[key] = { ...now_, text: now_.text.trim() ? `${text}\n\n${now_.text}` : text, images: [...d.images, ...(now_.images || [])] };
    toast(`Couldn't send: ${e}`);
    renderMain();
  }
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
  if (/^kept in Cue/.test(o.via || "")) {
    if (s.held && msgStart(s.held.text).includes(want)) o.seenHeld = true;
    if (o.seenHeld || now() - o.at > 30000) return true;
  }
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
async function askBtw(sid, q) {
  q = q.trim();
  if (!q) return;
  const log = btwLog.get(sid) || btwLog.set(sid, []).get(sid);
  const e = { q, a: null, err: null };
  log.push(e);
  delete drafts[`btw:${sid}`];
  renderMain();
  try { e.a = await invoke("btw", { sessionId: sid, question: q }); } catch (x) { e.err = String(x); }
  renderMain();
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
const forkBtn = (sid, i) => `<button class="btw-fork" data-act="btw-fork" data-sid="${esc(sid)}" data-i="${i}" ${btwForking != null ? "disabled" : ""}>${btwForking === i ? "Forking…" : "Fork"}</button>`;
function btwPanel(sid) {
  if (!sid || btwFor !== sid) return "";
  const log = btwLog.get(sid) || [];
  const rows = log.map((e, i) => [e, i]).reverse().map(([e, i]) => `<div class="btw-q">${esc(e.q)}</div>${e.a != null ? `<div class="btw-a msg">${md(e.a)}</div>${forkBtn(sid, i)}` : e.err ? `<div class="btw-a bad">${esc(e.err)}</div>` : `<div class="btw-a dim"><span class="dot-live"></span>Thinking…</div>`}`).join("");
  return `<div class="btw-pop">
    <div class="btw-head">BY THE WAY · NOT PART OF THE CHAT${log.length ? `<button class="btw-clear" data-act="btw-clear" data-sid="${esc(sid)}">Clear</button>` : ""}<button class="btw-x" data-act="btw-close" aria-label="Close">×</button></div>
    <div class="btw-ask"><textarea data-text="btw:${esc(sid)}" rows="1" placeholder="Ask without interrupting it…  ↵" spellcheck="false">${esc(draft(`btw:${sid}`).text)}</textarea></div>
    <div class="btw-body">${rows || `<div class="dim btw-empty">A copy of this conversation answers, so the session never sees it. Each question costs about one turn.</div>`}</div></div>`;
}
// ---------- Catch me up: what it did while you weren't looking, above its box ----------
// Cue notes when this window last showed each session's chat (kept across restarts). Open one you last looked at
// 15 minutes or more ago, with replies since, and a bar takes the insights' place above the box: how long, what
// came in the meantime, and Catch me up. Nothing is asked until you click: then a small model reads only those
// messages (btw.rs catch_up) and the bar fills with the answer, Listen and Got it. Got it folds it away, and the
// insights come back, until you've been away again.
const LOOKED = "cue.looked";
const CATCH_UP_AFTER = 15 * 60000;
let looked = {};   // sid -> when this window last showed its chat (while in front)
try { looked = JSON.parse(localStorage.getItem(LOOKED) || "{}") || {}; } catch {}
let lookedSid = null;   // the session whose chat the last draw showed
let catchUp = null;     // this opening's strip: { sid, since, asking, got, err, gone }
/** The chat for `sid` is on screen: the first draw after opening it sets up its strip from when it was last
 *  looked at; every draw while the window is in front notes the time. Hidden, or behind another app, it isn't being
 *  looked at (a session's replies still redraw it). */
function noteLooked(sid) {
  if (!sid || document.hidden || !document.hasFocus()) return;
  if (sid !== lookedSid) { lookedSid = sid; catchUp = { sid, since: looked[sid] || 0, openedAt: now() }; }
  looked[sid] = now();
  const keys = Object.keys(looked);
  if (keys.length > 50) for (const k of keys.sort((a, b) => looked[a] - looked[b]).slice(0, keys.length - 50)) delete looked[k];
  try { localStorage.setItem(LOOKED, JSON.stringify(looked)); } catch {}
}
// Back in front (shown again, or back from another app): the chat open counts as opened again, looked at last
// when the window went away.
const backInFront = () => { if (!document.hidden && document.hasFocus()) { lookedSid = null; renderMain(); } };
document.addEventListener("visibilitychange", backInFront);
window.addEventListener("focus", backInFront);
/** The bar for the open chat, or "": only for a session you last looked at 15 minutes or more ago that had
 *  replied by the time you opened it. What comes in while it's open isn't "while you were away", and once you
 *  write to it you're caught up: the bar goes. */
/** Catch me up, at the end of the chat (under the latest reply, above the status line): never a bar on the box. */
function catchUpBar(sid, it, s) {
  const c = catchUp;
  if (!c || c.sid !== sid || c.gone || !c.since || aboutTracked(sid)) return "";
  const ex = conversation(it, s);
  if (ex.some((e) => e.role === "you" && e.at_ms > c.openedAt)) return "";
  const away = (at) => at > c.since && at <= c.openedAt;
  const replies = ex.filter((e) => e.role === "agent" && away(e.at_ms)).length;
  if (!replies || c.openedAt - c.since < CATCH_UP_AFTER) return "";
  // What came in the meantime, from what Cue has already: its replies, and the steps read for the chat.
  const steps = (stepFeeds.get(sid)?.turns || []).flatMap((t) => t.items).filter((x) => x.t === "step" && away(x.at_ms));
  const commands = steps.filter((x) => x.kind === "run").length, files = new Set(steps.filter((x) => x.kind === "edit").map((x) => x.subject)).size;
  const n = (k, one, many) => (k ? `${k} ${k === 1 ? one : many}` : "");
  const counts = [n(replies, "reply", "replies"), n(commands, "command", "commands"), n(files, "file", "files")].filter(Boolean).join(" · ");
  const tag = `<span class="cu-tag mono">${esc(ago(c.since).toUpperCase())} SINCE YOU LOOKED</span>`;
  if (!c.got && !c.err) {
    const act = c.asking ? `<span class="cu-busy"><span class="dot-live"></span>Catching up…</span>` : `<button class="btn primary" data-act="catch-up" data-sid="${esc(sid)}">Catch me up</button>`;
    return `<div class="cu-strip" data-key="catchup"><div class="cu-row">${tag}<span class="dim">${esc(counts)}</span><span class="grow"></span>${act}</div></div>`;
  }
  const key = `${sid}:catchup`, on = listening?.key === key;
  const listen = c.got ? `<button class="btn ${on ? "on" : ""}" data-listen="${esc(key)}">${!on ? SPEAKER_ICON + "Listen" : listening.playing ? PAUSE_ICON + "Pause" : PLAY_ICON + "Resume"}</button>` : "";
  const body = c.err ? `<div class="cu-err">Couldn't catch you up: ${esc(c.err)}</div>`
    : `<div class="msg cu-msg"><ul>${c.got.points.map((p) => `<li>${esc(p)}</li>`).join("")}</ul>${c.got.needs ? `<p class="cu-need"><b>Needs you: ${esc(c.got.needs)}</b></p>` : ""}</div>`;
  return `<div class="cu-strip" data-key="catchup"><div class="cu-row">${tag}<span class="grow"></span><span class="dim">${esc(counts)}</span></div>${body}
    <div class="cu-row cu-acts"><span class="grow"></span>${listen}<button class="btn" data-act="catch-up-got" data-sid="${esc(sid)}">Got it</button></div></div>`;
}
/** The click: asks once (a second click does nothing while it's on its way); the answer, or why there's none, fills the strip. */
async function askCatchUp(sid) {
  const c = catchUp;
  if (!c || c.sid !== sid || c.asking || c.got) return;
  c.asking = true;
  renderMain();
  try { const got = await invoke("catch_up", { sessionId: sid, seenMs: c.since }); if (catchUp === c) c.got = got; }
  catch (x) { if (catchUp === c) c.err = String(x); }
  if (catchUp === c) { c.asking = false; renderMain(); }
}
/** Got it: the strip goes, for this look at the chat (if it was being read out, that stops). */
function gotCatchUp(sid) {
  if (catchUp?.sid === sid) catchUp.gone = true;
  if (listening?.key === `${sid}:catchup`) listenStop();
  renderMain();
}
