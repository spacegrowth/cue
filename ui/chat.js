// Cue UI chat — the Active pane: the steps feed, turns, parked threads, the terminal, the "/" menu, drafts and the message box.
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
  const acts = e.running || e.err ? "" : ` · <button class="q-now" data-act="ran-again" data-who="${esc(who)}" data-n="${n}">Run again</button>${who !== "new" ? ` · <button class="q-now" data-act="ran-send" data-who="${esc(who)}" data-n="${n}">Send to ${esc(agentName(sessionOf(who)?.harness))}</button> · <button class="q-now" data-act="ran-term" data-who="${esc(who)}" data-n="${n}">Run in its terminal</button>` : ""}`;
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
 *  looked at; every draw while the window is in front notes the time. Hidden, the window isn't being looked at. */
function noteLooked(sid) {
  if (!sid || document.hidden) return;
  if (sid !== lookedSid) { lookedSid = sid; catchUp = { sid, since: looked[sid] || 0, openedAt: now() }; }
  looked[sid] = now();
  const keys = Object.keys(looked);
  if (keys.length > 50) for (const k of keys.sort((a, b) => looked[a] - looked[b]).slice(0, keys.length - 50)) delete looked[k];
  try { localStorage.setItem(LOOKED, JSON.stringify(looked)); } catch {}
}
// Back in front: the chat open counts as opened again (looked at last when the window went away).
document.addEventListener("visibilitychange", () => { if (!document.hidden) { lookedSid = null; renderMain(); } });
/** The bar for the open chat, or "": only for a session you last looked at 15 minutes or more ago that had
 *  replied by the time you opened it. What comes in while it's open isn't "while you were away", and once you
 *  write to it you're caught up: the bar goes. */
function catchUpBar(sid, it, s) {
  const c = catchUp;
  if (!c || c.sid !== sid || c.gone || !c.since) return "";
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
// ---------- the built-in terminal: a session's real screen, docked above its reply box (sessions in tmux) ----------
/** Closed: one strip showing its screen's last line, amber while it waits on something only the terminal
 *  can answer. Open: xterm.js draws the real terminal, streamed from a tmux client of Cue's own (term.rs),
 *  and what you type goes straight to it. Its element lives outside the redraws: each one puts it back. */
const inTmux = (sid) => !!sessionOf(sid)?.tmux_pane || !!execTerm(sid);
let termSid = null;            // whose terminal is open
let termGen = 0;               // which opening its output belongs to (0: not known yet)
let termObj = null, termFit = null, termHost = null, termWatch = null;
let termQueue = [], termFrame = 0;   // output waiting for the next frame: one write per frame, however many chunks came
const termLast = new Map();    // sid -> its screen's last line, for the closed strip
const waitsInTerminal = (sid) => state.items.some((i) => i.session_id === sid && inTerminal(i));
const cssVar = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
/** The terminal app a session runs in, by name (from what its hooks reported). */
/** Where a session runs, as a small icon for the lists: Cue's mark (in Cue, tmux underneath) or a terminal. */
function placeIcon(sid) {
  const s = sessionOf(sid);
  if (!s) return "";
  return inTmux(sid) ? `<span class="lv-place" title="Runs in Cue">${CUE_MARK_SM}</span>` : `<span class="lv-place" title="Runs in ${esc(termAppName(s))}">${TERM_ICON}</span>`;
}
function termAppName(s) {
  const t = s?.term_program || "";
  return { "iTerm.app": "iTerm", Apple_Terminal: "Terminal", WezTerm: "WezTerm", ghostty: "Ghostty", vscode: "VS Code", kitty: "kitty" }[t] || (s?.kitty_window_id ? "kitty" : s?.wezterm_pane ? "WezTerm" : "its terminal");
}
/** Where a session runs, set into its reply box's top right corner: a terminal app's (a click goes to its
 *  tab), or Cue's own (a click opens its terminal above the box, as the strip does). */
function placeTab(sid) {
  const s = sid && sessionOf(sid);
  if (!s) return "";
  // In Cue: the same corner opens (or closes) its terminal above the box.
  if (inTmux(sid)) return `<button class="tplace-btn" data-act="term" data-sid="${esc(sid)}" aria-expanded="${termSid === sid}">${termSid === sid ? "Close" : "Open"} ${s.machine ? esc(machShort(s.machine)) : "Cue"} <span aria-hidden="true">${termSid === sid ? "▾" : "▴"}</span></button>`;
  // A terminal app's: its tab, and for Claude Code, Move to Cue (resumed in Cue's terminal).
  return `<span class="tplace">${moveBtn(sid, s.harness, s.state, "tplace-btn move")}<button class="tplace-btn" data-act="go-session" data-sid="${esc(sid)}">Open ${esc(termAppName(s))} <span aria-hidden="true">↗</span></button></span>`;
}
/** Move to Cue, for a Claude Code session in a terminal app's tab: ended there, resumed in Cue's terminal.
 *  Shown even when it can't run yet, greyed, and a click says why, so it never just isn't there. */
function moveBtn(sid, harness, st, cls) {
  if (harness !== "claude") return "";
  const why = !state.settings?.sessions?.tmux_installed ? "Needs tmux on this Mac (brew install tmux), then reopen Cue"
    : st === "working" ? "Available once it finishes what it's doing (or Esc it in its tab)"
    : st === "deciding" ? "Answer what it's asking first"
    : "";
  return `<button class="${cls}" data-act="move-cue" data-sid="${esc(sid)}" ${why ? `aria-disabled="true" data-why="${esc(why)}" title="${esc(why)}"` : ``}>Move to Cue</button>`;
}
/** "Open in iTerm ↗": where a terminal-app session's own tab is, for the places that have no reply box. */
const openInLabel = (s) => `Open in ${esc(s ? termAppName(s) : "its terminal")} <span aria-hidden="true">↗</span>`;
function termDock(sid) {
  if (!sid) return "";
  if (!inTmux(sid)) return "";   // where it runs instead: a tab on its reply box (placeTab)
  const open = termSid === sid, waiting = waitsInTerminal(sid);
  return `<div class="tdock ${open ? "open" : ""} ${waiting ? "waiting" : ""}">
    <button class="tstrip" data-act="term" data-sid="${esc(sid)}" aria-expanded="${open}" aria-label="${open ? "Close" : "Open"} its terminal (⌃\`)"><span class="tlive"></span><span class="tlbl">${waiting ? "Waiting in its terminal" : "Terminal"}</span><span class="tlast">${esc(termLast.get(sid) || "")}</span><span class="tchev">▴</span></button>
    ${open ? `<div class="tslot" data-term-slot></div>` : ""}</div>`;
}
async function openTerm(sid) {
  if (!inTmux(sid)) return;
  // Already open, or opening (its client's on the way): that one, not a second.
  if (termSid === sid && termObj) return termObj.focus();
  if (termSid) closeTerm(true);
  termSid = sid;
  termGen = 0;
  renderMain();                     // the slot appears
  const slot = document.querySelector("[data-term-slot]");
  if (!slot || typeof Terminal === "undefined") { termSid = null; return renderMain(); }
  termHost = document.createElement("div");
  termHost.className = "term-host";
  slot.appendChild(termHost);
  ({ term: termObj, fit: termFit } = xterm(termHost));
  termObj.onData((d) => { if (termSid) invoke("term_write", { sessionId: termSid, data: d }).catch(() => {}); });
  termObj.onResize(({ cols, rows }) => { if (termSid) invoke("term_resize", { sessionId: termSid, cols, rows }).catch(() => {}); });
  termWatch = new ResizeObserver(() => { try { termFit?.fit(); } catch {} });
  termWatch.observe(termHost);
  termObj.focus();
  try {
    const gen = await invoke("term_open", { sessionId: sid, cols: termObj.cols, rows: termObj.rows });
    if (termSid === sid) termGen = gen;
  } catch (e) { toast(`Couldn't open its terminal: ${e}`); if (termSid === sid) closeTerm(); }
}
/** A remote executor, from its row in its lead's tree: its chat when Cue holds it, else a pane of its
 *  own (execPane), and either way its terminal, attached to its own tmux session there. Once per
 *  executor: a second click on the same row keeps the terminal that's open. */
function openExec(sid, id = null) {
  if (sessionOf(sid)) setActive(id, sid);
  else if (quietOpen !== sid || view !== "board") {
    if (quietOpen !== sid) setActive(null, null);   // (not when it's this one: that would end its terminal)
    view = "board";
    quietOpen = sid;
    renderMain();
  }
  return openTerm(sid);
}
/** A session Cue holds, opened from any row, chip or list that names it: its chat, and for a remote
 *  executor in a tmux session of its own (one a lead on a machine started, whose hooks reached Cue too)
 *  also that terminal, as from its row in its lead's tree (openExec). `id`: the card to open it on. */
function openSession(sid, id = null) {
  if (sessionOf(sid) && execTerm(sid)) return openExec(sid, id);
  return setActive(id, sid);
}
/** Close it (the session keeps running in tmux). `quiet`: no redraw, another opening follows. */
function closeTerm(quiet = false) {
  const sid = termSid;
  termSid = null;
  termGen = 0;
  termWatch?.disconnect();
  cancelAnimationFrame(termFrame);
  termQueue = [];
  termFrame = 0;
  termObj?.dispose();
  termHost?.remove();
  termWatch = termObj = termFit = termHost = null;
  if (sid) invoke("term_close", { sessionId: sid }).catch(() => {});
  if (!quiet) { renderMain(); homeFocus(); }
}
function onTermOutput(p) {
  if (p.sid !== termSid || (termGen && p.gen !== termGen)) return;
  if (p.end) return closeTerm();
  termQueue.push(p.data);
  if (!termFrame) termFrame = requestAnimationFrame(flushTerm);
}
/** The terminals open now (the session's and Cue's own shells), to recolor when the window's colors change. */
const terms = new Set();
let termKey = "";
const rgbProbe = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
/** A color variable as [r, g, b]: xterm takes only plain colors, and a project's are oklch(). */
function rgbVar(name) {
  const el = document.createElement("i");
  el.style.color = `var(${name})`;
  document.body.append(el);
  const c = getComputedStyle(el).color;
  el.remove();
  rgbProbe.clearRect(0, 0, 1, 1);
  rgbProbe.fillStyle = c;
  rgbProbe.fillRect(0, 0, 1, 1);
  return [...rgbProbe.getImageData(0, 0, 1, 1).data.slice(0, 3)];
}
function termTheme() {
  const hex = (name) => "#" + rgbVar(name).map((v) => v.toString(16).padStart(2, "0")).join("");
  const [r, g, b] = rgbVar("--sel");
  return { background: hex("--slab"), foreground: hex("--on-slab"), cursor: hex("--sel"), cursorAccent: hex("--slab"), selectionBackground: `rgba(${r}, ${g}, ${b}, 0.35)` };
}
/** The open terminals in the window's colors now: the project's, light or dark. */
function retheme() {
  const root = document.documentElement;
  const key = [root.className, root.dataset.theme, root.style.getPropertyValue("--ph"), root.style.getPropertyValue("--pk"), matchMedia("(prefers-color-scheme: dark)").matches].join("|");
  if (key === termKey) return;
  termKey = key;
  if (!terms.size) return;
  const theme = termTheme();
  for (const t of terms) t.options.theme = theme;
}
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", retheme);

/** What arrived since the last frame, decoded and written as one. */
function flushTerm() {
  termFrame = 0;
  const chunks = termQueue;
  termQueue = [];
  if (!termObj || !chunks.length) return;
  termObj.write(b64Bytes(chunks));
}
/** An xterm.js terminal in `host`, in Cue's colours, fitted to it: drawn on the GPU where it can be (much
 *  faster than the default; if that's lost, back to the default). Also Cue's own shells' (shells.js).
 *  Its cursor blinks at Cue's pace, a 1.4s beat like the live dots: vendor/xterm/addon-webgl.js is patched
 *  from xterm's fixed 600ms to 700ms (keep that on an update), and the default renderer's in style.css. */
function xterm(host) {
  const term = new Terminal({
    fontFamily: cssVar("--mono") || "Menlo, monospace", fontSize: 12, lineHeight: 1.1, cursorBlink: true, scrollback: 5000, macOptionIsMeta: true,
    // ⌥-drag: a selection of the terminal's own, even with tmux taking the mouse.
    macOptionClickForcesSelection: true,
    theme: termTheme(),
  });
  terms.add(term);
  const dispose = term.dispose.bind(term);
  term.dispose = () => { terms.delete(term); dispose(); };
  const fit = new FitAddon.FitAddon();
  term.loadAddon(fit);
  term.open(host);
  try {
    const gl = new WebglAddon.WebglAddon();
    gl.onContextLoss(() => gl.dispose());
    term.loadAddon(gl);
  } catch {}
  try { fit.fit(); } catch {}
  // What tmux copies (drag over text: it selects, and on letting go hands the text to the terminal as
  // OSC 52, base64 after the ";"), and a ⌥-drag selection once you let go: on your clipboard.
  const copy = (text) => { if (text.trim()) invoke("copy_text", { text }).then(() => toast("Copied"), () => {}); };
  term.parser.registerOscHandler(52, (data) => {
    const b64 = data.slice(data.indexOf(";") + 1);
    if (b64 && b64 !== "?") try { copy(new TextDecoder().decode(Uint8Array.from(atob(b64), (c) => c.charCodeAt(0)))); } catch {}
    return true;
  });
  let picked = false;   // a new ⌥-drag selection, not copied yet (a later click doesn't copy it again)
  term.onSelectionChange(() => { picked = term.hasSelection(); });
  host.addEventListener("mouseup", () => { if (picked) { picked = false; copy(term.getSelection()); } });
  return { term, fit };
}
/** "term" events' chunks (base64), as the bytes to write. */
function b64Bytes(chunks) {
  const bin = chunks.map(atob).join("");
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}
/** After a redraw: the terminal back in its slot, sized, and focused again if it had the keyboard. */
function placeTerm(hadFocus) {
  if (!termHost) return;
  const slot = document.querySelector("[data-term-slot]");
  if (!slot) return;
  if (termHost.parentNode !== slot) slot.appendChild(termHost);
  try { termFit?.fit(); } catch {}
  if (hadFocus) termObj?.focus();
}
const reply = (it) => deliver(it.session_id, `s:${it.session_id}`, (text, images) => {
  insSent(it.session_id, text, false);
  answered(it.id);
  return invoke("reply", { id: it.id, text, images }).catch((e) => { answered(it.id, false); throw e; });
});
/** The text field: a reply on a finished card, "do this instead" on a permission, your own answer on a question. */
/** Dictation: the mic in a text box. Your words land in the box as you talk (after anything already
 *  typed); click the mic again, or send, to stop. Apple's on-device recognition, via cue-listen. */
function toggleDictation(key) {
  if (dictating?.key === key) { dictating.stopping = true; invoke("dictate_stop"); return renderMain(); }
  if (dictating) invoke("dictate_stop");
  dictating = { key, base: draft(key).text.trimEnd(), stopping: false };
  invoke("dictate_start").catch((e) => { dictating = null; toast(`Can't dictate: ${e}`); renderMain(); });
  renderMain();
  document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`)?.focus();
}
/** Sending ends dictation: what's in the box is what goes. */
function endDictation(key) {
  if (dictating?.key !== key) return;
  invoke("dictate_stop");
  dictating = null;
}
function onDictation(p) {
  if (p.error) { toast(p.error); dictating = null; return renderMain(); }
  if (p.done) { dictating = null; return renderMain(); }
  if (p.text == null || !dictating) return;
  const { key, base } = dictating;
  setBox(key, base ? `${base} ${p.text}` : p.text);
}
/** A text box's words, kept as its draft and put in the box in place (no full redraw), caret at the end. */
function setBox(key, text) {
  draft(key).text = text;
  saveDrafts();
  const el = document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`);
  if (el) {
    el.value = text;
    el.selectionStart = el.selectionEnd = text.length;
    grow(el);
    const b = el.closest(".composer")?.querySelector(".btn.send");
    if (b) b.disabled = !text.trim();
  }
  return el;
}
/** Text an add-on puts in a chat's box ("fill"), after what's typed there, to send or edit from. */
function onFill({ id, text }) {
  if (!id || !text) return;
  const had = draft(id).text.trimEnd();
  const el = setBox(id, had ? `${had} ${text}` : text);
  if (el) el.focus();
  else renderMain();
}
function submitText(it) {
  endDictation(it.id);
  const text = draft(it.id).text.trim();
  if (it.kind === "waiting") return reply(it);
  if (!text) return;
  if (isBang(text)) { delete drafts[it.id]; return runLine(it.session_id, text); }
  if (it.kind === "question") {
    const qs = questions(it);
    if (qs.length <= 1) return respond(it, { behavior: "allow", answers: Object.fromEntries(qs.map((q) => [q.question, text])) });
    // Several questions: your typed answer is for the one on screen only. Then on to the next
    // unanswered one; it's sent once every question has an answer.
    const cur = stepOf(it), q = qs[cur], d = draft(it.id);
    d.choices[q.question] = q.multiSelect ? new Set([text]) : text;
    d.text = "";
    const a = answersFor(it);
    if (a) return respond(it, { behavior: "allow", answers: a });
    const next = qs.findIndex((x, n) => n !== cur && !isAnswered(it, x));
    if (next >= 0) qStep[it.id] = next;
    return renderMain();
  }
  return deny(it, text);
}
/** Say something to a live session from the Active pane (it may be mid-turn). */
// now (Ctrl+Enter): if it's mid-turn, stop it first so this is handled right away.
/** Send now (the button on a queued message, or ⌘↵ with nothing new typed): stopping the turn makes the
 *  agent read what you queued straight away. */
function sendQueuedNow(sid) {
  invoke("send_queued_now", { sessionId: sid })
    .then(() => toast(`Sent now: ${nameOf(sid, sessionOf(sid)?.project || "it")} stopped its current turn and reads your message next`))
    .catch((e) => toast(`Couldn't send it now: ${e}`));
}
/** Send now on the message Cue kept for a working session (its button, or ⌘↵ as for a queued one): it
 *  stops the turn and the message goes straight away. */
const sendHeldNow = (sid) => invoke("send_held_now", { sessionId: sid }).then(() => toast("Sent now: it stopped its turn and reads your message next")).catch((e) => toast(`Couldn't send it now: ${e}`));
/** ⌘↵ with nothing new typed: what's waiting to go (queued in its agent, or kept in Cue) goes now. False when there's nothing. */
function sendWaitingNow(sid) {
  const s = sessionOf(sid);
  if (s?.queued) sendQueuedNow(sid);
  else if (s?.held) sendHeldNow(sid);
  else return false;
  return true;
}
const sendTo = (sid, now = false) => state.restarting?.[sid] ? toast("It's restarting. Send once it's back.") : sessionOf(sid)?.state === "deciding" ? toast("It's asking you something first. Answer that, then send.") : deliver(sid, `s:${sid}`, (text, images) => { insSent(sid, text, now); return invoke("send_to_session", { sessionId: sid, text, images, now }); });
/** The message Cue kept for a working session, back into its box to change (before anything you've typed
 *  there since), images and all. */
async function pullHeld(sid) {
  const h = await invoke("unhold_message", { sessionId: sid }).catch(() => null);
  if (!h) return;
  const key = `s:${sid}`, d = draft(key);
  d.text = d.text.trim() ? `${h.text}\n\n${d.text}` : h.text;
  for (const path of h.images || []) {
    const data = await invoke("image_data", { path }).catch(() => null);
    if (data) d.images.push({ name: path.split("/").pop(), mime: data.slice(5, data.indexOf(";")), data });
  }
  saveDrafts();
  renderMain();
  focusComposer();
}
/** Stop a working session mid-turn (Esc in its terminal; Pi aborts directly). */
async function interrupt(sid) {
  try { toast(`Stopped ${sessionOf(sid)?.project || "it"}: ${(await invoke("interrupt_session", { sessionId: sid })).replace(/^stopped via /, "via ")}`); }
  catch (e) { toast(`Couldn't stop it: ${e}`); }
}
const box_ = (key, placeholder, label, attrs, alt = "", extra = "") => box(key, placeholder, label, attrs, true, alt, extra);

/** A waiting message's tracing outline: the window redraws every few seconds, so each copy starts where
 *  the clock says the trace is, and it goes round smoothly instead of jumping back to the start. */
const qPhase = () => `--qd:-${Date.now() % 4000}ms`;
/** Remember how the last message went out ("typed via tmux", "queued"), shown in the Active pane. */
function sent(sid, text, images, via) {
  lastSent = { sid, text, images, via: String(via || "").replace(/^sent via /, ""), at: Date.now() };
  setTimeout(() => { if (lastSent && Date.now() - lastSent.at >= 8000) { lastSent = null; render(); } }, 8100);
}

async function goTo(it) {
  try { toast(`Jumped to ${await invoke("focus_session", { id: it.id })}`); }
  catch (e) {
    // A "Runs in Cue" session has no tab: its terminal is the one here.
    if (inTmux(it.session_id)) { setActive(it.id); return openTerm(it.session_id); }
    toast(`Couldn't jump: ${e}`);
  }
}

/** Thumbnails of sent images; click to open full size (← → between them). */
function thumbs(paths) {
  if (!paths?.length) return "";
  const srcs = paths.map(fileSrc);
  return `<div class="thumbs">${srcs.map((src, n) => `<button class="thumb" data-lightbox='${esc(JSON.stringify({ srcs, i: n }))}' aria-label="Open image ${n + 1}"><img src="${esc(src)}" alt=""/></button>`).join("")}</div>`;
}

/** Images a message names by absolute path (`in backticks`, which may hold spaces, or bare),
 *  shown under it. The window can't load them itself; image_data hands them over. */
const localImgs = new Map();   // path -> data: URL, null while loading, false if unreadable
const IMAGES_PER_MSG = 12;                 // past it a message shows the path instead
const IMAGE_HOLD_BYTES = 64 * 2 ** 20;     // no more loaded while this much is held (trimCaches thins it)
const imgHeld = () => [...localImgs.values()].reduce((n, v) => n + sizeOf(v), 0);
function imagePaths(text) {
  const IMG = String.raw`\.(?:png|jpe?g|gif|webp)`;
  const found = [...String(text ?? "").matchAll(new RegExp(String.raw`\`(\/[^\`\n]+?${IMG})\`|(?:^|[\s(])(\/[^\s\`'"<>()]+${IMG})(?![\w/])`, "gi"))].map((m) => m[1] || m[2]);
  return [...new Set(found)];
}
function localImages(text) {
  const all = imagePaths(text).filter((p) => localImgs.get(p) !== false);
  // The first dozen load (while what's held fits); the rest are named, never fetched.
  const paths = all.slice(0, IMAGES_PER_MSG).filter((p) => localImgs.has(p) || imgHeld() < IMAGE_HOLD_BYTES);
  const named = all.filter((p) => !paths.includes(p));
  for (const p of paths) {
    if (localImgs.has(p)) continue;
    localImgs.set(p, null);
    invoke("image_data", { path: p }).then((d) => localImgs.set(p, d), () => localImgs.set(p, false)).then(render);
  }
  if (!all.length) return "";
  const shots = paths.length ? `<div class="thumbs shots">${paths.map((p, n) => `<button class="thumb" data-local-lb='${esc(JSON.stringify({ paths, i: n }))}'>${localImgs.get(p) ? `<img src="${localImgs.get(p)}" alt=""/>` : ""}</button>`).join("")}</div>` : "";
  return shots + (named.length ? `<div class="shots-named">${named.map((p) => `<code>${esc(p)}</code>`).join(" ")}</div>` : "");
}

/** The "/" menu: the commands Claude Code has enabled for a session (its built-ins, your skills,
 *  plugins'), read by the Mac per folder. sid -> list; null while it's being read. */
const cmdLists = new Map();
const cmdMissed = new Map();   // sid -> when its list last failed to come: not asked again for a minute
const CMD_RETRY_MS = 60000;
const BTW_CMD = { name: "btw", kind: "cue", desc: "Ask on the side: answered in a panel here, never added to the chat" };
let cmdSel = 0, cmdShut = null;           // highlighted row; the box whose menu Esc closed (until you type)
/** The session's "/" commands, read once per session (Cue keeps a folder's list a week). `fresh`: you
 *  pressed Refresh, so every list the window holds goes and this one is read from Claude Code again.
 *  While Cue reads a newer list it gives the one it had, and the newer one takes its place when it's in. */
function cmdList(sid, fresh) {
  if (!sid) return [];
  if (fresh) { cmdLists.clear(); cmdMissed.clear(); }
  if (!cmdLists.has(sid) && Date.now() - (cmdMissed.get(sid) || 0) > CMD_RETRY_MS) {
    cmdLists.set(sid, null);
    const missed = () => { cmdLists.delete(sid); cmdMissed.set(sid, Date.now()); };
    const ask = (n) => invoke("session_commands", { sessionId: sid, fresh: !!fresh && n === 0 }).then(({ list, reading }) => {
      if (list && JSON.stringify(list) !== JSON.stringify(cmdLists.get(sid))) { cmdLists.set(sid, list); cmdMissed.delete(sid); refreshCmd(`s:${sid}`); }
      if (list && !reading) return;
      if (n < 40) setTimeout(() => ask(n + 1), 600); else if (!list) missed();
    }).catch(missed);
    ask(0);
  }
  const list = cmdLists.get(sid) || [];
  // Cue's own: a side question, answered in a panel (Claude Code's /btw answers only in its terminal).
  return sessionOf(sid)?.harness === "claude" ? [BTW_CMD, ...list.filter((c) => c.name !== "btw")] : list;
}
const sidOfKey = (key) => (key.startsWith("s:") ? key.slice(2) : "");
// ---------- ↑ / ↓ in a session's box: the last messages you sent it, like a shell's history ----------
// ↑ on the box's first line puts the message before into the box (older with each press, the last ten);
// ↓ on the last line comes forward again and ends on what you were typing. Typing, or sending, starts over.
const RECALL = 10;
let recall = null;   // { key, list, idx, typed, set }: where you are in the list, and the text the last step put in
function recallSent(e) {
  const ta = e.target, key = ta.dataset.text, sid = sidOfKey(key);
  if (!sid) return false;
  const up = e.key === "ArrowUp";
  const v = ta.value, at = ta.selectionStart;
  // Only from the box's first line (↑) or last line (↓): elsewhere the arrows move the caret as usual.
  if (up ? v.lastIndexOf("\n", at - 1) !== -1 : v.indexOf("\n", ta.selectionEnd) !== -1) return false;
  if (!recall || recall.key !== key || recall.set !== v) {
    const cur = current();
    if ((cur?.s?.session_id || cur?.it?.session_id) !== sid) return false;
    const seen = new Set();
    const list = conversation(cur.it, cur.s).filter((x) => x.role === "you" && x.text.trim()).map((x) => x.text).filter((t) => !seen.has(t) && seen.add(t)).slice(-RECALL);
    if (!list.length) return false;
    recall = { key, list, idx: list.length, typed: v, set: v };
  }
  const { list } = recall;
  if (up ? recall.idx === 0 : recall.idx >= list.length) return up;   // nothing older: the key is spent; nothing newer: the caret moves
  recall.idx += up ? -1 : 1;
  const text = recall.idx === list.length ? recall.typed : list[recall.idx];
  recall.set = text;
  ta.value = text;
  draft(key).text = text;
  saveDrafts();
  grow(ta);
  ta.setSelectionRange(text.length, text.length);
  const b = ta.closest(".composer")?.querySelector(".btn.send");
  if (b && !b.dataset.held) b.disabled = !text.trim() && !draft(key).images.length;
  refreshCmd(key);
  return true;
}
/** "/comp": the command name being typed (no space yet), else null. A leading space means a message. */
const typingCmd = (text) => (/^\/[^\s]*$/.test(text) ? text.slice(1).toLowerCase() : null);
/** "/model so": the command and the value being typed for it (one word, no space after yet), else null. */
const typingArg = (text) => { const m = /^\/(\S+) (\S*)$/.exec(text); return m ? { name: m[1], q: m[2].toLowerCase() } : null; };
/** Matches for what's typed: names that start with it first, then names that contain it. After a command
 *  that takes a set of values (/model, /effort…), those values, as "model sonnet" (what follows the "/"). */
/** A command's values to pick. /model's are each model with its 1M-token context first (`opus[1m]`, as
 *  + New's 1M context passes it), then Claude Code's list. No 1M one for Default, a model that already
 *  carries a bracket, or one the list names itself. */
function argsOf(c) {
  const args = c?.args || [];
  if (c?.name !== "model") return args;
  const big = args.filter((v) => v.value !== "default" && !/\[.*\]$/.test(v.value) && !args.some((w) => w.value === `${v.value}[1m]`));
  return [...big.map((v) => ({ value: `${v.value}[1m]`, label: `${v.label || v.value}, 1M context`, desc: "" })), ...args];
}
function cmdMatches(key) {
  if (cmdShut === key) return [];
  const list = cmdList(sidOfKey(key));
  const a = typingArg(draft(key).text);
  if (a) {
    const c = list.find((x) => x.name === a.name);
    const hits = argsOf(c).filter((v) => [v.value, v.label].some((t) => t.toLowerCase().includes(a.q)));
    // Typed out in full: nothing left to pick.
    if (hits.length === 1 && hits[0].value.toLowerCase() === a.q) return [];
    return hits.map((v) => ({ name: `${c.name} ${v.value}`, label: v.label !== v.value ? v.label : "", desc: v.desc, arg: true }));
  }
  const q = typingCmd(draft(key).text);
  if (q === null) return [];
  const starts = list.filter((c) => c.name.toLowerCase().startsWith(q) || c.name.toLowerCase().split(":").pop().startsWith(q));
  return [...starts, ...list.filter((c) => !starts.includes(c) && c.name.toLowerCase().includes(q))];
}
function cmdMenu(key) {
  const m = cmdMatches(key);
  // The session's list is still being read (the first "/" in a session): say so rather than show nothing.
  if (!m.length && typingCmd(draft(key).text) !== null && cmdShut !== key && cmdLists.get(sidOfKey(key)) === null) return `<div class="cmd-menu"><div class="cmd-row cmd-wait">Loading commands…</div></div>`;
  if (!m.length) return "";
  cmdSel = Math.min(cmdSel, m.length - 1);
  // A command that takes a value: its hint ("[name]"); one with values to pick says so (→ shows them).
  const after = (c) => c.arg ? (c.label ? ` <em>${esc(c.label)}</em>` : "") : c.args?.length ? ` <em>…</em>` : c.hint ? ` <em>${esc(c.hint)}</em>` : "";
  // Under the rows: Claude Code's list is read once and kept; Refresh asks again (a new skill or plugin).
  // Pi's comes from its extension; Refresh asks it again. Codex's is Codex's own: nothing to refresh.
  const h = sessionOf(sidOfKey(key))?.harness;
  const foot = h === "claude" || h === "pi" ? `<div class="cmd-foot"><span>${h === "pi" ? "From Pi" : "Read once, kept a week"}</span><button data-cmd-refresh="${esc(key)}">Refresh</button></div>` : "";
  return `<div class="cmd-menu" role="listbox">${m.map((c, n) => `<button class="cmd-row ${n === cmdSel ? "on" : ""}" data-cmd-pick="${esc(key)}" data-cmd-name="${esc(c.name)}" role="option"><b>/${esc(c.name)}${after(c)}</b>${c.desc ? `<span>${esc(c.desc)}</span>` : ""}${c.kind === "skill" ? `<i>skill</i>` : ""}</button>`).join("")}${foot}</div>`;
}
/** The command a box's text runs ("/compact now" -> compact), from the session's list; undefined when
 *  it isn't a command, null when it's one the session doesn't have (or the list isn't in yet: then unknown). */
function cmdOf(key) {
  const t = draft(key).text;
  if (!t.startsWith("/")) return undefined;
  const name = t.slice(1).split(/\s/)[0];
  const sid = sidOfKey(key), list = sid ? cmdList(sid) : [];
  if (!list.length) return { name, desc: "" };
  return list.find((c) => c.name === name) || null;
}
/** Under the box: what the command does, or that it isn't one here (with Send as a message). */
function slashHint(key) {
  const t = draft(key).text;
  if (bangTyped(t)) { const s = sessionOf(sidOfKey(key)); const c = s?.cwd; return `<div class="slash-hint"><b>!</b> Cue runs this in ${esc(homeless(c || draft("new-cwd").text || "the folder"))}${s?.machine || newMachine ? ` on ${esc(machShort(s?.machine || newMachine))}` : ""}, in your shell. The agent doesn't see it unless you send it the result.</div>`; }
  if (!t.startsWith("/") || cmdMatches(key).length || cmdLists.get(sidOfKey(key)) === null) return `<div class="slash-hint" hidden></div>`;
  const c = cmdOf(key), name = t.slice(1).split(/\s/)[0];
  const asText = `<button data-astext="${esc(key)}">Send as a message instead</button>`;
  if (c === null) return `<div class="slash-hint bad">“/${esc(name)}” isn't a command in this session. ${asText}</div>`;
  return `<div class="slash-hint">${c.desc ? `<b>/${esc(c.name)}${c.hint ? ` ${esc(c.hint)}` : ""}</b>: ${esc(c.desc)}` : `Starts with “/”, so the agent runs it as a command. ${asText}`}</div>`;
}
/** Redraw just a box's menu and hint (typing never rebuilds the box, so the caret stays put). */
function refreshCmd(key) {
  const el = document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`);
  const f = el?.closest(".field");
  if (!f) return;
  const menu = f.querySelector(".cmd-slot");
  if (menu) menu.innerHTML = cmdMenu(key);
  f.querySelector(".slash-hint")?.replaceWith(document.createRange().createContextualFragment(slashHint(key)));
  // The button beside it says Run for a "!" line (and its own word again once that's gone).
  f.classList.toggle("bang", bangTyped(draft(key).text));
  const btn = f.querySelector(".btn.send");
  if (btn) { btn.dataset.label ??= btn.textContent; btn.textContent = bangTyped(draft(key).text) ? "Run" : btn.dataset.label; }
  f.querySelector(".cmd-row.on")?.scrollIntoView({ block: "nearest" });
}
/** Pick a command: Tab or a click puts "/name " in the box for its arguments; Enter runs it. One with
 *  values to pick (/model) never runs bare (that opens a picker in its terminal): it shows its values.
 *  A value ("model sonnet"): Tab or a click puts it in the box, Enter runs it. */
function pickCmd(key, name, run) {
  const c = cmdList(sidOfKey(key)).find((x) => x.name === name);
  if (c?.args?.length) run = false;
  draft(key).text = `/${name}${run || name.includes(" ") ? "" : " "}`;
  cmdSel = 0;
  saveDrafts();
  const el = document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`);
  if (el) { el.value = draft(key).text; el.focus(); el.selectionStart = el.selectionEnd = el.value.length; }
  refreshCmd(key);
  if (run) el?.closest(".composer")?.querySelector(".btn.send")?.click();
}
/** Quick phrases (Settings → Quick phrases): chips that send their text, after anything typed in the box. */
const phrases = () => state.settings?.quick?.phrases || [];
/** The quick phrases, with (at the right) where the session runs when it's a terminal app's. */
const quickRow = (attrs, sid = "") => {
  const place = placeTab(sid);
  return phrases().length || place ? `<div class="quick">${phrases().map((p) => `<button class="qp" ${attrs} data-phrase="${esc(p)}">${esc(p)}</button>`).join("")}${place}</div>` : "";
};
/** Settings → Quick phrases as you type them (redraws keep them); saved when you leave a field. */
let qpEdit = null;
const withPhrase = (text, p) => (text.trim() ? `${text.trimEnd()} ${p}` : p);
/** A box's images, before they're sent: each opens large, × takes it off. */
function draftChips(key) {
  const d = draft(key);
  return d.images.length ? `<div class="chips">${d.images.map((im, n) => `<span class="chip"><button class="chip-img" data-draft-lightbox="${esc(key)}:${n}" aria-label="Open ${esc(im.name)}"><img src="${im.data}" alt="${esc(im.name)}"/></button><button class="chip-x" data-unattach="${esc(key)}:${n}" aria-label="Remove image">×</button></span>`).join("")}${d.images.length > 1 ? `<span class="chip-count">${d.images.length} images</span>` : ""}</div>` : "";
}
/** A text box that grows as you type (Shift+Enter = new line, Enter = send), optionally with images.
 *  The boxes that message a session (the ones with images) also get the quick phrases.
 *  `alt` shows beside Send once something's typed; `extra` always does. */
function box(key, placeholder, label, sendAttrs, images, alt = "", extra = "") {
  const d = draft(key);
  const ready = d.text.trim() || (images && d.images.length);
  if (bangTyped(d.text) && !d.images.length) label = "Run";
  const chips = images ? draftChips(key) : "";
  // Send sits inside the box, at the end of its row, so the box spans the full width (as the terminal dock does).
  return `<div class="composer"><div class="field ${images ? "drop" : ""} ${bangTyped(d.text) ? "bang" : ""}" data-drop="${images ? esc(key) : ""}">${sidOfKey(key) ? `<div class="cmd-slot">${cmdMenu(key)}</div>` : ""}${images ? quickRow(`data-quick="${esc(key)}"`, sidOfKey(key)) : ""}${chips}<div class="field-row">
    <textarea rows="2" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off" data-text="${esc(key)}" placeholder="${esc(placeholder)}">${esc(d.text)}</textarea>
    <button class="icon-btn mic ${dictating?.key === key ? "on" : ""}" data-mic="${esc(key)}" title="${dictating?.key === key ? "Stop dictating" : "Dictate: talk and it types here"}" aria-label="Dictate">${MIC_ICON}</button>
    ${images ? `<button class="icon-btn" data-attach="${esc(key)}" title="Add an image (or paste one with ⌘V)" aria-label="Add image">${IMAGE_ICON}</button>` : ""}
    ${extra}${ready ? alt : ""}<button class="btn primary send" ${sendAttrs} ${ready ? "" : "disabled"}>${label}</button></div>${slashHint(key)}</div></div>`;
}

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
  o.busy = false;
  const chat = document.querySelector("[data-chat]");
  if (chat) holdChat = chat.scrollHeight - chat.scrollTop;
  renderMain();
}
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
/** Your message's bubble. Said by voice: a mic beside it, and inside it the line Cue added for the agent (or will
 *  add, `later`: kept in Cue or queued, it hasn't gone yet); its + opens the line word for word (`key`). */
function yourBubble(e, key, style = "", later = false) {
  const said = yourWords(e);
  const open = said.voice && openAsks.has(key);
  const note = !said.voice ? "" : `<div class="cv-voice-note"><button class="cv-voice-ask" data-voice-ask="${esc(key)}" aria-expanded="${open}">${open ? "−" : "+"} Cue ${later ? "will ask" : "asked"} for a short spoken answer</button>${open ? `<div class="cv-voice-line">${esc(VOICE_ASK_LINE)}</div>` : ""}</div>`;
  const bubble = `<div class="cv-you-text"${style ? ` style="${style}"` : ""}>${linkify(esc(said.text)).replace(/\n/g, "<br>")}${note}</div>`;
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
// ---------- background agents: Claude Code's Agent tool, run in the background ----------
// Each runs inside its session's process, with no terminal and no way to message it: not a session,
// so no row or card of its own. Its steps come with the session's (from its own log). While it runs
// its block sits above the message box, so chatting on doesn't scroll it away; once it's finished the
// block folds under the Delegating step that started it.
const trayDone = new Set();     // sessions whose tray lists its finished agents too
const trayFolded = new Set();   // sessions whose tray is folded to its head row
let helperView = null;          // { sid, id }: the agent whose full view fills the chat area (helperPane); id null: the newest still running, once its feed is in
const helpersOf = (sid) => stepFeeds.get(sid)?.helpers || [];
const helperRunning = (h) => !!h.running;
/** A teammate (started with a name) goes by its name and stays, idle, after its task; a background agent
 *  goes by its type and is finished. */
const helperName = (h) => (h.teammate && h.name) || h.kind || "agent";
const helperTag = (h) => h.teammate ? `<span class="bgtag team">TEAMMATE</span>` : `<span class="bgtag">BACKGROUND</span>`;
/** "1m 12s" of work: until it finished, or so far. */
const helperSpan = (h) => { const ms = (h.finished_ms || Date.now()) - h.started_ms; const m = Math.floor(ms / 60000), sec = Math.floor((ms % 60000) / 1000); return m ? `${m}m ${sec}s` : `${sec}s`; };
const helperModel = (h) => (h.model || "").replace(/^claude-/, "").replace(/-\d.*$/, "");
/** "12k tokens" an agent has used so far; "" until it has used any (a script uses none). */
const helperTokens = (n) => !(n > 0) ? "" : n < 1000 ? `${n} tokens` : `${(n / 1000).toFixed(n < 10000 ? 1 : 0)}k tokens`;
/** Running but quiet for minutes (the feed says so): shown amber, with how long since it last did anything. */
const helperStuck = (h) => helperRunning(h) && !!h.stuck;
const helperQuiet = (h) => `<span class="bg-quiet">quiet · ${Math.max(1, Math.floor((Date.now() - (h.last_ms || h.started_ms)) / 60000))}m</span>`;
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
 *  (`chatToEnd`, the path a newly opened chat takes in app.js renderMain), then it's pinned again a frame
 *  later and once more when settled: whatever grows after the draw (the box growing to a draft, an image,
 *  a table laid out late) would otherwise leave the end just out of view. */
let chatToEnd = false;    // the next chat draw lands at its end; app.js renderMain consumes it
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
  return helpersOf(sid).find((h) => h.id === helperView.id) || null;
}
/** One agent's block, in the tray or under the Delegating step that started it: who it is and how long;
 *  a running one in the tray shows its steps too, four rows tall, the rest scrolled to inside it (it follows
 *  the newest unless you scrolled up; app.js renderMain keeps where you were). Its head opens the full
 *  view (helperPane). */
function helperBlock(sid, h, inTray) {
  const k = `${sid}|${h.id}`;
  const running = helperRunning(h);
  const model = helperModel(h);
  // A script: its tag is solid, its name the description it was given (else the command), and it ends "failed" when it did.
  const tag = h.script ? `<span class="bgtag script">SCRIPT</span><b>${esc(h.desc)}</b>` : `${helperTag(h)}<b>${esc(helperName(h))}</b><span class="dim-sep"> · ${esc(h.desc)}</span>`;
  const ended = h.script && h.failed ? `<span class="bad">failed</span>` : h.teammate ? "idle" : "finished";
  const tok = helperTokens(h.tokens);
  const head = `<button class="bg-head" data-helper="${esc(k)}">${tag}<span class="dim">${model ? ` · ${esc(model)}` : ""}${tok ? ` · ${tok}` : ""} · ${running ? `<span data-helper-span="${h.started_ms}">${helperSpan(h)}</span>${helperStuck(h) ? ` · ${helperQuiet(h)}` : ""}` : `${ended} · ${helperSpan(h)}`}</span>${running ? liveDot(h) : ""}<span class="stx-chev">›</span></button>`;
  if (!(inTray && running)) return `<div class="bg-agent folded" data-key="h:${esc(h.id)}">${head}</div>`;
  // A script's body: its last lines of output (four rows; it follows the newest unless you scrolled up).
  if (h.script) return `<div class="bg-agent" data-key="h:${esc(h.id)}">${head}${h.output.length ? `<div class="bg-out bg-scroll" data-bg-scroll="${esc(k)}">${h.output.map((l) => `<div>${esc(l)}</div>`).join("")}</div>` : ""}</div>`;
  const steps = h.items.map((x) => x.t === "step" ? stepLine(sid, x, false, true) : `<div class="stx-say msg small">${md(x.text)}</div>`).join("");
  return `<div class="bg-agent" data-key="h:${esc(h.id)}">${head}${h.more ? `<div class="stx-more dim">${h.more} earlier steps not shown</div>` : ""}<div class="stx-steps bg-scroll" data-bg-scroll="${esc(k)}">${steps}</div></div>`;
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
  const top = helperViewTop(sid, h);
  const body = top.hop + `<div class="bg-view-title" data-key="title">${helperTag(h)}<b>${esc(helperName(h))}</b>${running ? liveDot(h) : ""}</div>
    <div class="bg-view-meta" data-key="meta"><span>${esc(h.desc)}</span>${model ? `<span>${esc(model)}</span>` : ""}${tok ? `<span>${tok}</span>` : ""}<span>${running ? `<span data-helper-span="${h.started_ms}">${helperSpan(h)}</span> so far` : h.teammate ? "idle, can take more" : `finished · ${helperSpan(h)}`}</span>${helperStuck(h) ? `<span>${helperQuiet(h)}</span>` : ""}<span>${h.items.filter((x) => x.t === "step").length + (h.more || 0)} steps</span></div>
    ${h.more ? `<div class="stx-more dim" data-key="more">${h.more} earlier steps not shown</div>` : ""}
    ${steps.length ? `<div class="stx-steps bg-view-steps" data-key="steps">${steps.join("")}</div>` : `<div class="dim" data-key="none">${running ? "Starting…" : "No steps were kept for it."}</div>`}
    ${tail.length ? `<div class="bg-view-reply" data-key="reply"><span class="bg-view-k">Its reply</span><div class="msg">${md(tail.join("\n\n"))}</div></div>` : ""}`;
  // Finished with a reply: a button puts it in the session's message box (only there; you send it).
  if (h.teammate && h.name) return { bar: top.bar, body, foot: teammateFoot(sid, h) };
  const toBox = !running && tail.length ? `<button class="btn" data-helper-reply="${esc(`${sid}|${h.id}`)}">Put reply in the box</button>` : "";
  const foot = helperViewFoot(running, running ? `Running inside this session's turn. It reports to the session, not to you.` : `What it reported went into the session's next reply.`, toBox);
  return { bar: top.bar, body, foot };
}
/** The top of a full view: ‹ Back, and chips for the session's other background work (agents and scripts),
 *  running first then finished newest first: a click hops to that one. */
function helperViewTop(sid, h) {
  const others = helpersOf(sid).filter((x) => x.id !== h.id).sort((a, b) => (helperRunning(b) - helperRunning(a)) || ((b.finished_ms || b.started_ms) - (a.finished_ms || a.started_ms)));
  const chip = (x) => `<button class="bg-hop-b ${helperRunning(x) ? "live" : ""}" data-helper="${esc(`${sid}|${x.id}`)}">${helperRunning(x) ? liveDot(x) : ""}${x.script ? `<span class="bgtag script">SCRIPT</span><span class="dim">${esc(x.desc)}</span>` : `<b>${esc(helperName(x))}</b><span class="dim">${esc(x.desc)}</span>`}${helperStuck(x) ? helperQuiet(x) : ""}</button>`;
  const hop = others.length ? `<div class="bg-hop" data-key="hop">${others.map(chip).join("")}</div>` : "";
  const bar = `<div class="bg-view-head" data-key="head"><button class="bg-back" data-helper-back="1"><span class="bg-back-ch">‹</span>Back to chat</button>${others.length ? `<span class="dim bg-hop-n">· ${others.length} other${others.length === 1 ? "" : "s"}</span>` : ""}</div>`;
  return { bar, hop };
}
/** A teammate's foot: its messages reach its lead by themselves (and show in the lead's chat), so there's
 *  nothing to send from here. */
function teammateFoot(sid, h) {
  return helperViewFoot(helperRunning(h), `Its messages go to ${esc(nameOf(sid, sessionOf(sid)?.project))} by themselves.`);
}
/** A session's teammates, at the top of its chat, as relay's lead and executors are: the session (lead),
 *  then a chip per teammate, working (live dot) or idle. A chip opens that teammate in full;
 *  the lead's takes you back to the chat. */
function teammateBar(sid) {
  const tms = helpersOf(sid).filter((h) => h.teammate && h.name);
  if (!tms.length) return "";
  const open = helperView?.sid === sid ? helperView.id : null;
  const lead = `<button class="tm-chip ${open ? "" : "me"}" ${open ? `data-helper-back="1"` : ""}>${esc(nameOf(sid, sessionOf(sid)?.project))} <span class="tm-sub">lead</span></button>`;
  const chip = (h) => `<button class="tm-chip ${open === h.id ? "me" : ""}" data-helper="${esc(`${sid}|${h.id}`)}">${helperRunning(h) ? liveDot(h) : ""}${esc(h.name)} <span class="tm-sub">${helperRunning(h) ? "working" : "idle"}</span> <span class="tm-sub tm-role">teammate</span></button>`;
  return `<div class="team-bar">${lead}${tms.map(chip).join("")}</div>`;
}
const helperViewFoot = (live, text, extra = "") => `<div class="foot bg-view-foot">${live ? `<span class="dot-live"></span>` : ""}<span class="dim">${text}</span><span class="grow"></span>${extra}<button class="btn primary send" data-helper-back="1">Go to chat</button></div>`;
/** A background script in full: the command it ran and its output at full height (the tail Cue has of
 *  it, following the newest line), and once it ended, how. */
function scriptPane(sid, h) {
  const running = helperRunning(h);
  const shown = h.output.length, all = h.lines || shown;
  const top = helperViewTop(sid, h);
  const body = top.hop + `<div class="bg-view-title script" data-key="title"><span class="bgtag script">SCRIPT</span><span>${esc(h.desc)}</span>${running ? liveDot(h) : ""}</div>
    <div class="bg-view-meta" data-key="meta">${running ? `<span><span data-helper-span="${h.started_ms}">${helperSpan(h)}</span> so far</span>` : `<span class="${h.failed ? "bad" : ""}">${h.failed ? "failed" : "finished"} · ${helperSpan(h)}</span>`}${helperStuck(h) ? `<span>${helperQuiet(h)}</span>` : ""}<span>${all} line${all === 1 ? "" : "s"}${all > shown ? ` · last ${shown} shown` : ""}</span></div>
    ${h.command && h.command !== h.desc ? `<div class="bg-cmd" data-key="cmd">${esc(h.command)}</div>` : ""}
    ${shown ? `<div class="bg-out bg-view-out" data-key="out" data-bg-scroll="${esc(`${sid}|${h.id}|full`)}">${h.output.map((l) => `<div>${esc(l)}</div>`).join("")}</div>` : `<div class="dim" data-key="none">${running ? "No output yet." : "It wrote nothing."}</div>`}`;
  const foot = helperViewFoot(running, running ? `Running in the background. Its output goes to the session when it ends.` : h.failed ? `It ended with an error. The session was told, and its output is here.` : `It finished. The session was told, and its output is here.`);
  return { bar: top.bar, body, foot };
}
// ---------- Reader: one reply, set for reading ----------
// Read on a long reply: it fills the chat area in one column, larger type, ‹ › to its other replies, three
// text sizes (remembered). Read-only on purpose: no box, so permissions, questions, the queue and the rest
// stay in the chat alone. Reply goes to the chat's box; ‹ Chat (or Esc) back to the reply in the chat.
let readerView = null;   // { sid, key }: the reply open in the reader; key as chatHtml's ("<sid>:<at_ms>")
const READER_SIZES = [15, 17, 20];
let readerSize = 1;      // index into READER_SIZES
try { const n = localStorage.getItem("cue.readerSize"); if (n != null && READER_SIZES[+n]) readerSize = +n; } catch {}
/** The session's replies the reader steps through, oldest first, keyed as in the chat. */
function readerReplies(it, s) {
  const id = s?.session_id || it?.id;
  return conversation(it, s).filter((e) => e.role === "agent" && e.text.trim()).map((e) => ({ key: `${id}:${e.at_ms}`, text: e.text, at_ms: e.at_ms }));
}
/** The reply open in this session's reader, with where it sits among the others; null when none (or it's gone). */
function readerViewed(sid, it, s) {
  if (!readerView) return null;
  if (readerView.sid !== sid) { readerView = null; return null; }
  const all = readerReplies(it, s), at = all.findIndex((r) => r.key === readerView.key);
  return at < 0 ? null : { all, at };
}
/** The reader is on screen (not just remembered behind another view). */
const readerShown = () => !!readerView && !!document.querySelector(".rd-head");
function openReader(sid, key) {
  readerView = { sid, key };
  renderMain();
}
/** ‹ › (and ← →): the previous or next reply, if there is one. */
function stepReader(dir) {
  const cur = current(), sid = cur?.s?.session_id || cur?.it?.session_id;
  const rv = sid && readerViewed(sid, cur.it, cur.s);
  const next = rv && rv.all[rv.at + dir];
  if (!next) return;
  readerView = { sid, key: next.key };
  renderMain();
}
function sizeReader(dir) {
  const n = Math.max(0, Math.min(READER_SIZES.length - 1, readerSize + dir));
  if (n === readerSize) return;
  readerSize = n;
  try { localStorage.setItem("cue.readerSize", String(n)); } catch {}
  renderMain();
}
/** Back to the chat, at the reply you were reading (its start in view). */
function closeReader() {
  const key = readerView?.key;
  readerView = null;
  renderMain();
  const btn = key && document.querySelector(`[data-read="${CSS.escape(key)}"], [data-fwd="${CSS.escape(key)}"]`);
  btn?.closest(".cv-agent")?.scrollIntoView({ block: "start" });
}
/** Reply: to the chat's own box, at the chat's end, the cursor in it. */
function replyFromReader() {
  readerView = null;
  drawAtChatEnd();
  focusComposer();
}
function copyReader() {
  const cur = current(), sid = cur?.s?.session_id || cur?.it?.session_id;
  const rv = sid && readerViewed(sid, cur.it, cur.s);
  if (rv) invoke("copy_text", { text: rv.all[rv.at].text }).then(() => toast("Copied"), () => toast("Couldn't copy"));
}
/** The reader, in place of the chat: a bar (‹ Chat, ‹ n of m ›, text size, Copy, Reply), the reply in one
 *  column, and a foot only when the session is asking you something (then Back to chat answers it there). */
function readerPane(sid, it, s, harness, rv) {
  const r = rv.all[rv.at], n = rv.all.length;
  // The reader has the pane to itself (activePane leaves out the session's head, bar, tabs and terminal strip):
  // its one bar says which session it is, and that it's waiting in its terminal when it is (an amber Terminal, as
  // the strip goes amber; a click: the chat, with the terminal open).
  const who = `<span class="badge h-${esc(harness || "other")}">${agentLetter(harness)}</span><span class="proj rd-name">${esc(nameOf(sid, it?.project || s?.project))}</span>`;
  const term = inTmux(sid) && waitsInTerminal(sid) ? `<button class="rd-term" data-reader-term="${esc(sid)}" aria-label="Waiting in its terminal: open it"><span class="tlive"></span>Terminal</button>` : "";
  const bar = `<div class="bg-view-head rd-head" data-key="head"><button class="bg-back" data-reader-back="1"><span class="bg-back-ch">‹</span>Chat</button>${who}<span class="grow"></span>${term}
    <span class="rd-seg"><button data-reader-step="-1" aria-label="Previous reply" ${rv.at ? "" : "disabled"}>‹</button><span class="rd-of">${rv.at + 1} of ${n}</span><button data-reader-step="1" aria-label="Next reply" ${rv.at < n - 1 ? "" : "disabled"}>›</button></span>
    <span class="rd-seg"><button class="rd-a small" data-reader-size="-1" aria-label="Smaller text" ${readerSize ? "" : "disabled"}>A</button><button class="rd-a big" data-reader-size="1" aria-label="Larger text" ${readerSize < READER_SIZES.length - 1 ? "" : "disabled"}>A</button></span>
    ${listening?.key === r.key ? "" : `<button class="btn ls-btn" data-listen="${esc(r.key)}" data-listen-reader="1">${SPEAKER_ICON}Listen</button>`}<button class="btn" data-reader-copy="1">Copy</button>${s ? `<button class="btn primary" data-reader-reply="1">Reply</button>` : ""}</div>`;
  const body = `<div class="rd" data-key="rd:${esc(r.key)}:${readerSize}" data-rdkey="${esc(r.key)}" style="--rd-size: ${READER_SIZES[readerSize]}px"><div class="rd-by">${esc(agentName(harness))} · ${ago(r.at_ms)} ago · reply ${rv.at + 1} of ${n}</div><div class="msg">${md(r.text)}</div>${localImages(r.text)}</div>`;
  const asks = isPending(it) && it.kind !== "waiting";
  const foot = asks ? `<div class="foot rd-ask"><span class="rd-ask-dot"></span><span class="rd-ask-t"><b>${esc(nameOf(sid, it.project))} ${it.kind === "question" ? "is asking you" : "needs a decision"}</b>${it.kind === "question" ? "" : `<span class="dim"> · ${esc(verb(it))}</span>`}</span><span class="grow"></span><button class="btn primary" data-reader-back="1">Back to chat</button></div>` : "";
  return { bar, body, foot: listenPlayer(r.key) + foot };
}
// ---------- Listen: a reply read out loud ----------
// Listen under any reply (and in the reader) reads it out a sentence at a time in the Mac's voice, at 1× to 1.6× in
// small steps (remembered). The sentence it's on goes lime with the word it's saying underlined, what it read goes gray
// (CSS highlights over the reply as shown, so the chat's own HTML is untouched), and it scrolls along. A click on a
// sentence starts there. Code blocks are said as "Code block." once, a table a row at a time. In the chat one line
// above the box says it's listening (Pause, Stop); in the reader the foot is its player, and it goes on to the next
// reply when "Then the next reply" is on. Opening another session keeps it reading (from the reply as last shown);
// that session's line says whose reply it is, a click goes back, and there the marks are where it got to.
let listening = null;    // { sid, key, i (sentence), n, playing, reader (started there), html (the reply as last shown), all (the reader's replies), follow (the chat scrolls along), ahead (scrolled away, the reading above the mark) }
let listenRun = 0;       // bumped on every start, pause and stop: an old sentence's end is ignored
const LISTEN_SPEEDS = [1, 1.1, 1.25, 1.4, 1.6];
let listenSpeed = 0, listenThen = true;
try { listenSpeed = Math.max(0, LISTEN_SPEEDS.indexOf(Number(localStorage.getItem("cue.listenRate")))); listenThen = localStorage.getItem("cue.listenThen") !== "0"; } catch {}
const LISTEN_BLOCKS = "p, li, h1, h2, h3, h4, h5, h6, pre, tr, blockquote";
/** The reply `key` as shown: in the reader when it's open on it, else its bubble in the chat. */
function listenRoot(key) {
  return [...document.querySelectorAll(".rd[data-rdkey] .msg")].find((m) => m.closest(".rd").dataset.rdkey === key)
    || [...document.querySelectorAll("[data-listen]")].find((b) => b.dataset.listen === key)?.closest(".cv-agent, .cu-strip")?.querySelector(".msg") || null;
}
/** What it reads from: the reply on screen, else (its session not open) a copy of it as it was last shown. */
function listenSource(l) {
  const root = listenRoot(l.key);
  if (root) { l.html = root.innerHTML; return root; }
  if (l.html == null) return null;
  const el = document.createElement("div");
  el.innerHTML = l.html;
  return el;
}
/** A reply's sentences as shown, in reading order: each its text (what's said), and a Range over it when it's
 *  marked (not a table row or a code block: those keep their element, for following them). */
function listenSents(root) {
  const out = [];
  const els = [...root.querySelectorAll(LISTEN_BLOCKS)].filter((el) => !(el.matches("blockquote") && el.querySelector(LISTEN_BLOCKS)));
  for (const el of els.length ? els : [root]) {
    if (el.matches("pre")) { if (!el.parentElement.closest("pre")) out.push({ say: "Code block.", el }); continue; }
    if (el.matches("tr")) { const say = [...el.cells].map((c) => c.textContent.trim()).filter(Boolean).join(", "); if (/\w/.test(say)) out.push({ say, el }); continue; }
    // This block's own text (not a nested list's or a code block's), as one string over its text nodes.
    const nodes = [], walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, { acceptNode: (n) => (n.parentElement.closest(LISTEN_BLOCKS) === el ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT) });
    let text = "";
    for (let n; (n = walk.nextNode());) { nodes.push({ n, at: text.length }); text += n.data; }
    const pos = (i) => { let k = nodes.length - 1; while (k > 0 && nodes[k].at > i) k--; return [nodes[k].n, Math.min(i - nodes[k].at, nodes[k].n.data.length)]; };
    for (const m of text.matchAll(/(?:[^.!?\n]|[.!?]+(?=[^\s.!?"')\]]))+(?:[.!?]+["')\]]*(?=\s|$)|\n|$)/g)) {
      const a = m.index + (m[0].length - m[0].trimStart().length), b = m.index + m[0].trimEnd().length;
      if (b <= a || !/\w/.test(m[0])) continue;
      const range = document.createRange();
      range.setStart(...pos(a)); range.setEnd(...pos(b));
      out.push({ say: text.slice(a, b).replace(/\s/g, " "), range, pos, a });
    }
  }
  return out;
}
/** The marks: read in gray, the sentence it's on in lime, the word at `w` ([char, length] in it) underlined. */
function listenMark(sents, i, w) {
  if (!CSS.highlights) return;
  const done = sents.slice(0, i).filter((x) => x.range).map((x) => x.range), on = sents[i];
  CSS.highlights.set("cue-said", new Highlight(...done));
  CSS.highlights.set("cue-say", new Highlight(...(on?.range ? [on.range] : [])));
  CSS.highlights.delete("cue-word");
  const r = listenWord(on, w);
  if (r) CSS.highlights.set("cue-word", new Highlight(r));
}
/** A Range over the word at `w` ([char, length]) in sentence `x`, when it's marked. */
function listenWord(x, w) {
  if (!x?.range || !w) return null;
  const r = document.createRange();
  r.setStart(...x.pos(x.a + w[0])); r.setEnd(...x.pos(x.a + w[0] + w[1]));
  return r;
}
function listenUnmark() { if (CSS.highlights) ["cue-said", "cue-say", "cue-word"].forEach((h) => CSS.highlights.delete(h)); }
/** The chat scrolls along with what's read: the line being heard (the word, else the sentence's first line, a code
 *  block or table row as a whole) is held a third down, the chat moving up a line as the reading goes down one.
 *  Above that mark it's left where it is (the start of a reply near the top), unless it's out of view. A step is
 *  smooth, a jump (more than a screen) instant. Scroll the chat yourself and it stops following (`l.follow`) until
 *  the reading comes down to the mark from above, or you skip, or click a sentence. */
function listenFollow(l) {
  const v = l && listenView(l), x = v?.sents[l.i], chat = v?.root.closest("[data-chat]");
  if (!x || !chat) return;
  const r = (listenWord(x, l.w) || x.range)?.getClientRects()[0] || x.el?.getBoundingClientRect();
  if (!r) return;
  const box = chat.getBoundingClientRect(), mark = box.top + box.height / 3;
  const over = r.top < box.top + 24 || r.bottom > box.bottom - 40, below = r.top > mark + 4;
  // Scrolled away: on again once the reading, above the mark (you went on ahead), comes down to it. Below the
  // mark (you went back) it's left there.
  if (l.follow === false) {
    if (!over && r.top < mark) l.ahead = true;
    if (over || r.top < mark || !l.ahead) return;
    l.follow = true;
  }
  if (!over && !below) return;
  // A glide of Cue's own (core.js glideTo), not the browser's smooth scroll: this runs in every redraw, and in
  // WebKit a smooth scroll started then ran from the chat's top.
  const by = r.top - mark;
  if (Math.abs(by) < box.height) return glideTo(() => listenRoot(l.key)?.closest("[data-chat]"), chat.scrollTop + by);
  glide = null;
  chat.scrollTop += by;
}
/** You scrolled the chat being read yourself (app.js: the wheel or trackpad, or its scroll bar): it stops
 *  following, so it doesn't pull you back while you look elsewhere. */
function listenScrolled(chat) {
  const root = listening && listenRoot(listening.key);
  if (root && root.closest("[data-chat]") === chat) { listening.follow = false; listening.ahead = false; }
}
/** The reply being read as shown, with its sentences, worked out again only when asked (`fresh`: after a redraw)
 *  or when the reply on screen is another element: the word marks move many times a second. */
function listenView(l, fresh = false) {
  const root = listenRoot(l.key);
  if (!root) return (l.view = null);
  if (fresh || l.view?.root !== root) l.view = { root, sents: listenSents(root) };
  return l.view;
}
/** The marks where it is: the sentence `l.i` and the word in it (`l.w`, [at, length] in its words), if shown. */
function listenMarkNow(l, fresh = false) {
  const v = listenView(l, fresh);
  if (v) { listenMark(v.sents, l.i, l.w); listenFollow(l); }
}
/** Say the sentence it's on, then the next, to the end (and then the reader's next reply, when that's on). */
function listenSay() {
  const l = listening, root = l && listenSource(l);
  if (!root) return listenStop();
  const sents = listenSents(root), shown = root.isConnected;
  l.n = sents.length;
  if (l.i >= sents.length) return listenNext();
  const x = sents[l.i], run = ++listenRun;
  l.w = null;
  if (shown) { l.view = { root, sents }; listenMark(sents, l.i, null); listenFollow(l); } else listenUnmark();
  if (kokoroOn() && l.mac !== l.i) return kokoroSay(x, sents[l.i + 1], run);
  const u = new SpeechSynthesisUtterance(x.say);
  u.rate = LISTEN_SPEEDS[listenSpeed];
  u.onboundary = (e) => { if (run === listenRun && e.name === "word") { l.w = [e.charIndex, e.charLength || (x.say.slice(e.charIndex).match(/^\S+/)?.[0].length ?? 0)]; listenMarkNow(l); } };
  u.onend = () => { if (run !== listenRun) return; l.i++; listenSay(); };
  u.onerror = () => { if (run === listenRun) listenStop(); };
  speechSynthesis.cancel();
  speechSynthesis.speak(u);
  listenTell();
}
// ---------- Natural voices (Kokoro) ----------
// The voice Cue reads out in is a setting (voice.pick, Settings → Voice): "" the Mac's own (System Settings →
// Spoken Content), else a Kokoro voice. Each Kokoro voice is downloaded on its own (0.5 MB), the first with the
// voice engine they all use (about 125 MB), through cue-speak (Kokoro on this Mac, terminal/speak.rs). Listen
// then reads in it a sentence at a time, the next one made while this one plays; its marks follow where the
// sound is ({"at"}). If it fails or quits mid-sentence, Listen goes on in the Mac's voice from that sentence and
// says so once.
const voicePick = () => state.settings?.voice?.pick || "";
/** What cue-speak said of itself: the engine is here (`ready`), the voices here (`have`) and the space it all
 *  takes (`mb`), a macOS with the Apple bug (`risky`), none on this Mac (`none`: Intel); the voice being got and
 *  how far (`getting`, `progress`), how many sentences in a row it failed (`fails`) and that it gave up this run
 *  (`broke`). */
let kokoro = { known: false, ready: false, have: [], mb: 0, risky: false, none: false, getting: null, progress: 0, fails: 0, broke: false, bye: false };
const kokoroOn = () => !!voicePick() && kokoro.have.includes(voicePick()) && !kokoro.broke;
let kokoroAt = null;   // the sentence it's saying: { run, x, ms }
function kokoroTell(line) { invoke("speak_send", { line }).catch((e) => kokoroLine({ error: String(e), id: kokoroAt?.run, get: kokoro.getting })); }
function kokoroStatus(again = false) { if (again || !kokoro.known) { kokoro.known = true; kokoroTell({ status: true }); } }
function kokoroSay(x, next, run) {
  kokoroAt = { run, x, ms: 0, words: [], wi: -1 };
  kokoroTell({ say: x.say, voice: voicePick(), speed: LISTEN_SPEEDS[listenSpeed], id: run, ...(next ? { next: next.say } : {}) });
  listenTell();
}
function kokoroHush() { if (kokoroAt) { kokoroAt = null; kokoroTell({ stop: true }); } }
/** One of cue-speak's lines. */
function kokoroLine(v) {
  if (v.status) { Object.assign(kokoro, v.status); return renderMain(); }
  if (v.progress != null) { kokoro.progress = v.progress; return setProgress(); }
  // A voice got: it's the one Cue reads in now.
  if (v.got) { kokoro.getting = null; kokoro.ready = true; if (!kokoro.have.includes(v.got)) kokoro.have.push(v.got); pickVoice(v.got); kokoroStatus(true); return; }
  if (v.removed) { Object.assign(kokoro, { ready: false, have: [], mb: 0 }); pickVoice(""); return; }
  if (v.error && v.get) { kokoro.getting = null; toast(v.error); return renderMain(); }
  const a = kokoroAt, l = listening;
  if (v.playing != null && a?.run === v.playing) { a.ms = v.ms; a.words = v.words || []; kokoro.fails = 0; }
  else if (v.at != null && a?.run === v.at && a.ms && l) {
    // The word being heard: from when each starts (cue-speak's Words.swift, from Kokoro's own timings; the words
    // are the sentence's, split on spaces as here), else as far through its letters as through its sound.
    const words = [...a.x.say.matchAll(/\S+/g)], t = a.words;
    const wi = t.length === words.length ? Math.max(0, t.findLastIndex((ms) => ms <= v.ms))
      : Math.max(0, words.findLastIndex((m) => m.index <= (v.ms / a.ms) * a.x.say.length));
    if (wi === a.wi || !words[wi]) return;   // the same word: nothing moves
    a.wi = wi;
    l.w = [words[wi].index, words[wi][0].length];
    listenMarkNow(l);
  } else if (v.done != null && a?.run === v.done && v.end) { kokoroAt = null; if (l) { l.i++; listenSay(); } }
  else if (v.bye) kokoro.bye = true;
  // It retired after its 100 sentences (cue-speak): the sentence sent as it went, again to the fresh one.
  else if (v.gone && kokoro.bye) { kokoro.bye = false; if (a) kokoroSay(a.x, null, a.run); }
  else if (v.gone && kokoro.getting) { kokoro.getting = null; toast("The download stopped"); renderMain(); }
  else if (v.error || v.gone) {
    // Kokoro failing on a sentence: the Mac's voice says that one, and the natural voice the next. Three in a row:
    // the Mac's voice goes on from there.
    if (!a || (v.id != null && v.id !== a.run)) return;
    kokoroAt = null;
    if (++kokoro.fails >= 3) { kokoro.broke = true; toast("The natural voice stopped: going on in your Mac's voice"); }
    if (l) l.mac = l.i;
    if (l?.playing) listenSay();
  }
}
/** The voice Cue reads out in ("" the Mac's own). */
function pickVoice(id) {
  kokoro.broke = false;
  kokoro.fails = 0;
  invoke("set_setting", { key: "voice.pick", value: id }).catch((e) => toast(`Couldn't pick it: ${e}`));
  renderMain();   // the list closes now, not on the next redraw (none comes when it's the voice already in use)
}
/** Get a voice (and the engine, the first time). */
function getVoice(id) {
  kokoro.getting = id;
  kokoro.progress = 0;
  kokoroTell({ get: id });
  renderMain();
}
/** Kokoro's American English voices, as Settings lists them (each has a sample in ui/voices/). */
const KOKORO_VOICES = [["af_heart", "Heart", "woman"], ["af_bella", "Bella", "woman"], ["af_nicole", "Nicole", "woman"], ["af_sarah", "Sarah", "woman"],
  ["af_sky", "Sky", "woman"], ["af_alloy", "Alloy", "woman"], ["af_aoede", "Aoede", "woman"], ["af_jessica", "Jessica", "woman"], ["af_kore", "Kore", "woman"],
  ["af_nova", "Nova", "woman"], ["af_river", "River", "woman"], ["am_adam", "Adam", "man"], ["am_michael", "Michael", "man"], ["am_echo", "Echo", "man"],
  ["am_eric", "Eric", "man"], ["am_fenrir", "Fenrir", "man"], ["am_liam", "Liam", "man"], ["am_onyx", "Onyx", "man"], ["am_puck", "Puck", "man"]];
let voiceListOpen = false;
// The voice list closes on a click anywhere but the list and its button (before that click does its own thing).
document.addEventListener("click", (e) => { if (voiceListOpen && !e.target.closest?.(".vp-list, .vp-btn")) { voiceListOpen = false; renderMain(); } }, true);
/** The open voice list over the window, the sheet's edge included (a short sheet would cut it off): under its picker,
 *  as wide, as tall as fits down to the window's foot; over the picker when less than 160 px is left below. After
 *  each draw, and as the sheet scrolls under it. */
function placeVoiceList() {
  const list = document.querySelector(".vp-list"), btn = list && document.querySelector(".vp-btn");
  if (!btn) return;
  const r = btn.getBoundingClientRect(), below = innerHeight - r.bottom - 18, above = r.top - 18;
  const up = below < 160 && above > below, room = Math.min(324, up ? above : below);
  Object.assign(list.style, { left: `${r.left}px`, width: `${r.width}px`, maxHeight: `${room}px` });
  list.style.top = up ? `${r.top - 6 - Math.min(list.scrollHeight + 2, room)}px` : `${r.bottom + 6}px`;
}
document.addEventListener("scroll", (e) => { if (voiceListOpen && !e.target.matches?.(".vp-list")) placeVoiceList(); }, true);
addEventListener("resize", () => { if (voiceListOpen) placeVoiceList(); });
/** Settings → Voice: one Voice row, a picker showing the voice in use (▶ plays it); open, a short list: your Mac's
 *  voice, then Kokoro's women and men, one line each with ▶ and In use, ✓ (here: a click uses it) or ↓ 0.5 MB.
 *  Before the first one it says once that it brings the voice engine (125 MB); a download shows its bar. */
function voiceSettings() {
  kokoroStatus();
  const k = kokoro, pick = voicePick() && k.have.includes(voicePick()) ? voicePick() : "";
  const nameOf = (id) => (id ? KOKORO_VOICES.find(([v]) => v === id)?.[1] || id : "Your Mac's voice");
  const play = (id, name) => `<button class="vp-play" data-voice-sample="${id}" aria-label="Hear ${esc(name)}">${PLAY_ICON}</button>`;
  const btn = `<div class="vp-btn ${voiceListOpen ? "open" : ""}">${play(pick, nameOf(pick))}<button class="vp-open" data-voice-list aria-haspopup="listbox" aria-expanded="${voiceListOpen}"><span>${esc(nameOf(pick))}</span><span class="vp-caret">▾</span></button></div>`;
  const state = (id) => {
    if (k.getting === id) {
      const p = Math.round(k.progress * 100);
      return `<span class="vp-get"><span class="kk-bar"><span class="kk-fill" data-kokoro-progress style="width: ${p}%"></span></span><span class="dim" data-kokoro-pct>${k.ready ? `${p}%` : `${Math.round(k.progress * 125)} of 125 MB`}</span></span>`;
    }
    if (pick === id) return `<span class="vp-state">In use</span>`;
    if (!id || k.have.includes(id)) return `<button class="vp-use" data-voice-pick="${id}">${id ? "✓" : "Use"}</button>`;
    return `<button class="vp-down" data-voice-get="${id}" ${k.getting ? "disabled" : ""}>↓ ${k.ready ? "0.5 MB" : "Get"}</button>`;
  };
  const line = (id, name) => `<div class="vp-line ${pick === id ? "on" : ""}" role="option" aria-selected="${pick === id}" ${!id || k.have.includes(id) ? `data-voice-pick="${id}"` : ""}>${play(id, name)}<span class="vp-name">${esc(name)}</span>${state(id)}</div>`;
  const group = (who, title) => `<div class="vp-sec">${title}</div>${KOKORO_VOICES.filter(([, , w]) => w === who).map(([id, name]) => line(id, name)).join("")}`;
  const list = !voiceListOpen ? "" : `<div class="vp-list" role="listbox">
    <div class="vp-sec">Your Mac</div>${line("", "Your Mac's voice")}
    ${k.none ? `<div class="vp-intro">Natural voices need a Mac with Apple silicon.</div>`
      : `${k.ready ? "" : `<div class="vp-intro"><b>Natural voices</b>Kokoro, run on this Mac: nothing it reads leaves it. The first one brings the voice engine (125 MB, once); each after that is 0.5 MB. ▶ plays any of them first.</div>`}
    ${group("woman", "Natural · Women")}${group("man", "Natural · Men")}`}</div>`;
  const about = k.ready && k.have.length ? `<div class="vp-note">Natural voices (Kokoro) run on this Mac: nothing Cue reads out leaves it. ${k.mb} MB · <button class="vp-link" data-kokoro-remove>Remove</button></div>` : "";
  const risky = k.ready && k.risky ? `<div class="vp-note warn">macOS 26.4 and 26.5 have an Apple bug that can stop a natural voice now and then; Cue then goes on in your Mac's voice. macOS 26.6 fixes it.</div>` : "";
  return `<div class="set-group">Reading out loud</div>
    <div class="set-row vp-row"><div class="set-text"><div class="set-title">Voice</div><div class="set-sub">Listen reads replies out in it.</div>${about}${risky}</div><div class="vp">${btn}${list}</div></div>`;
}
let voiceSample = null;
/** ▶ beside a voice: its sample (shipped with Cue), again to stop it. */
function playSample(id) {
  const same = voiceSample?.id === id && !voiceSample.a.paused;
  voiceSample?.a.pause();
  voiceSample = null;
  if (window.speechSynthesis) speechSynthesis.cancel();
  if (same) return;
  // Your Mac's voice: said by it, the same words as the natural voices' samples.
  if (!id) { if (window.speechSynthesis) speechSynthesis.speak(new SpeechSynthesisUtterance("Packet three is done and staged. Here's what changed, and what I'd check before you commit.")); return; }
  const a = new Audio(`voices/${id}.m4a`);
  voiceSample = { id, a };
  a.play().catch(() => toast("Couldn't play the sample"));
}
/** A download's bar, without redrawing Settings. */
function setProgress() {
  const p = Math.round((kokoro.progress || 0) * 100);
  document.querySelectorAll("[data-kokoro-progress]").forEach((el) => { el.style.width = `${p}%`; });
  document.querySelectorAll("[data-kokoro-pct]").forEach((el) => { el.textContent = kokoro.ready ? `${p}%` : `${Math.round((kokoro.progress || 0) * 125)} of 125 MB`; });
}
/** Its count and button on screen, without redrawing the chat. */
function listenTell() {
  const l = listening;
  document.querySelectorAll("[data-listen-of]").forEach((el) => { el.textContent = l ? `${Math.min(l.i + 1, l.n)} of ${l.n}` : ""; });
  document.querySelectorAll(".ls-fill").forEach((el) => { el.style.width = l ? `${Math.round((Math.min(l.i + 1, l.n) / Math.max(1, l.n)) * 100)}%` : "0"; });
}
/** Done with this reply: on to the next when it was started in the reader with "Then the next reply" (its session
 *  open or not: then the next of its replies as they were when it was last open). */
function listenNext() {
  const l = listening, cur = current(), sid = cur?.s?.session_id || cur?.it?.session_id;
  if (sid === l.sid) l.all = readerReplies(cur.it, cur.s);
  const all = l.reader && listenThen ? l.all || [] : [];
  const next = all[all.findIndex((r) => r.key === l.key) + 1];
  if (!next) return listenStop();
  if (readerView?.key === l.key) readerView = { sid, key: next.key };
  listening = { ...l, key: next.key, i: 0, n: 0, html: md(next.text), follow: true };
  renderMain();
  listenSay();
}
function listenPlay(sid, key, reader = false) {
  if (!window.speechSynthesis && !kokoroOn()) return toast("This Mac can't read it out");
  openMsgs.add(key);   // a folded reply opens to be read
  const cur = current();
  listening = { sid, key, i: 0, n: 0, playing: true, reader, all: cur ? readerReplies(cur.it, cur.s) : [], follow: true };
  renderMain();
  listenSay();
}
// Pause holds the sentence where it is (a natural voice's sound pauses, and Resume goes on from that word); the
// Mac's voice starts the sentence again on Resume, as Safari's speech can't be relied on to pause.
function listenPause() {
  if (!listening) return;
  if (kokoroAt) kokoroTell({ pause: true });
  else { listenRun++; speechSynthesis.cancel(); }
  listening.playing = false;
  renderMain();
}
function listenResume() {
  if (!listening) return;
  listening.playing = true;
  renderMain();
  if (kokoroAt) kokoroTell({ resume: true });
  else listenSay();
}
const listenToggle = () => (listening?.playing ? listenPause() : listenResume());
function listenSkip(dir) {
  if (!listening) return;
  listening.i = Math.max(0, Math.min(listening.n - 1, listening.i + dir));
  listening.w = null;
  listening.follow = true;
  if (listening.playing) return listenSay();
  kokoroHush();   // paused: Resume starts the sentence skipped to
  listenMarkNow(listening);
  listenTell();
}
function listenStop() {
  listenRun++;
  if (window.speechSynthesis) speechSynthesis.cancel();
  kokoroHush();
  const was = listening;
  listening = null;
  listenUnmark();
  if (was) renderMain();
}
function listenFaster(n) {
  listenSpeed = n;
  try { localStorage.setItem("cue.listenRate", String(LISTEN_SPEEDS[n])); } catch {}
  if (listening?.playing) listenSay();
  else kokoroHush();   // paused: Resume says the sentence again at this speed
  renderMain();
}
function listenThenNext() {
  listenThen = !listenThen;
  try { localStorage.setItem("cue.listenThen", listenThen ? "1" : "0"); } catch {}
  renderMain();
}
/** A click in the reply being read, on a sentence (not a selection): it reads from there. */
function listenFrom(ev) {
  const l = listening, root = l && ev.target.closest(".msg");
  if (!root || root !== listenRoot(l.key) || !document.getSelection()?.isCollapsed) return false;
  const at = document.caretRangeFromPoint?.(ev.clientX, ev.clientY);
  const i = at ? listenSents(root).findIndex((x) => x.range?.isPointInRange(at.startContainer, at.startOffset)) : -1;
  if (i < 0) return false;
  l.i = i;
  l.follow = true;
  if (!l.playing) { l.playing = true; renderMain(); }
  listenSay();
  return true;
}
// The marks follow a redraw of the chat (and come back with its session).
function listenKeep(sid) {
  if (!listening || listening.sid !== sid) return;
  listenMarkNow(listening, true);
  listenTell();
}
const PLAY_ICON = `<svg viewBox="0 0 24 24" width="12" height="12" aria-hidden="true"><path d="M7 4l13 8-13 8z" fill="currentColor"/></svg>`;
const PAUSE_ICON = `<svg viewBox="0 0 24 24" width="12" height="12" aria-hidden="true"><path d="M6 4h4v16H6zM14 4h4v16h-4z" fill="currentColor"/></svg>`;
const BOOK_ICON = `<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 4h6a4 4 0 0 1 4 4v13a3 3 0 0 0-3-3H2zM22 4h-6a4 4 0 0 0-4 4v13a3 3 0 0 1 3-3h7z"/></svg>`;
/** Listen under a reply: the speaker and Listen; while it reads this one, ‖ Pause; paused on it, ▶ Resume (the
 *  same marks as the line above the box). Actions beside it are >_, so its ▶ isn't taken for one. */
function listenBtn(key) {
  const on = listening?.key === key;
  const [icon, label] = !on ? [SPEAKER_ICON, "Listen"] : listening.playing ? [PAUSE_ICON, "Pause"] : [PLAY_ICON, "Resume"];
  return `<button class="cv-pill ls-pill ${on ? "on" : ""}" data-listen="${esc(key)}">${icon}${label}</button>`;
}
/** Above the chat's box while a reply is read out: in its own session, how far; in another, whose (a click goes there). */
function listenLine(sid) {
  const l = listening;
  if (!l || !sid) return "";
  const where = l.sid === sid ? `sentence <span data-listen-of="1">${Math.min(l.i + 1, l.n)} of ${l.n}</span>` : `<button class="ls-where" data-listen-open="${esc(l.sid)}">${esc(nameOf(l.sid, sessionOf(l.sid)?.project))}</button>`;
  return `<div class="ls-line" data-key="listen"><b>${l.playing ? "Listening" : "Paused"}</b><span class="dim"> · ${where}${listenSpeed ? ` · ${LISTEN_SPEEDS[listenSpeed]}×` : ""}</span><span class="grow"></span>
    <button class="btn" data-listen-toggle="1" aria-label="${l.playing ? "Pause" : "Go on"}">${l.playing ? PAUSE_ICON : PLAY_ICON}</button><button class="btn" data-listen-stop="1">Stop</button></div>`;
}
/** The reader's foot while it reads the reply open there: a sentence back, pause, a sentence on, how far, speed,
 *  "Then the next reply", Stop. */
function listenPlayer(key) {
  const l = listening;
  if (!l || l.key !== key) return "";
  const of = Math.min(l.i + 1, l.n);
  return `<div class="foot ls-player">
    <span class="rd-seg"><button data-listen-skip="-1" aria-label="Back a sentence">«</button><button class="ls-pp" data-listen-toggle="1" aria-label="${l.playing ? "Pause" : "Go on"}">${l.playing ? PAUSE_ICON : PLAY_ICON}</button><button data-listen-skip="1" aria-label="Next sentence">»</button></span>
    <span class="dim ls-of" data-listen-of="1">${of} of ${l.n}</span>
    <span class="ls-track"><span class="ls-fill" style="width: ${Math.round((of / Math.max(1, l.n)) * 100)}%"></span></span>
    <span class="rd-seg ls-speeds">${LISTEN_SPEEDS.map((x, n) => `<button class="${n === listenSpeed ? "on" : ""}" data-listen-speed="${n}">${x}×</button>`).join("")}</span>
    <label class="ls-then"><input type="checkbox" data-listen-then="1" ${listenThen ? "checked" : ""}/>Then the next reply</label>
    <button class="btn" data-listen-stop="1">Stop</button></div>`;
}
/** The tray above the message box: every agent still at work, about one agent tall (the rest scrolled
 *  to inside it, app.js renderMain keeps where you were). "" when none. */
function helperTray(sid) {
  // Teammates have their own row at the top of the chat (teammateBar).
  const hs = helpersOf(sid).filter((h) => !h.teammate);
  const running = hs.filter(helperRunning);
  if (!running.length) return "";
  // The finished ones, on a click: newest first, each folded (open one for its steps). They're also in the chat,
  // under the step that started each.
  const done = hs.filter((h) => !helperRunning(h)).sort((a, b) => (b.finished_ms || b.started_ms) - (a.finished_ms || a.started_ms));
  const showDone = trayDone.has(sid) && done.length;
  const doneBtn = done.length ? `<button class="tray-done" data-tray-done="${esc(sid)}" aria-expanded="${!!showDone}">· ${done.length} finished <span class="stx-chev">${showDone ? "⌄" : "›"}</span></button>` : "";
  const na = running.filter((h) => !h.script).length, ns = running.length - na;
  const what = [na ? `${na} background agent${na === 1 ? "" : "s"}` : "", ns ? `${ns} script${ns === 1 ? "" : "s"}` : ""].filter(Boolean).join(" · ");
  // Quiet ones (running, nothing for minutes) are counted in amber; with several agents, their tokens are summed.
  const nq = running.filter(helperStuck).length;
  const tokens = na > 1 ? helperTokens(running.reduce((n, h) => n + (h.tokens || 0), 0)) : "";
  // The head folds the tray to itself (per session, until you unfold it); "· N finished" works either way.
  const folded = trayFolded.has(sid);
  const fold = (inner) => `<button class="tray-fold" data-tray-fold="${esc(sid)}" aria-expanded="${!folded}">${inner}</button>`;
  const head = `<div class="tray-head" data-key="tray-head">${fold(`<span class="dot-live ${nq ? "stuck" : ""}"></span><b>${what} running</b>${nq ? `<span class="bg-quiet"> · ${nq} quiet</span>` : ""}${tokens ? `<span class="dim"> · ${tokens}</span>` : ""}`)}${doneBtn}<span class="grow"></span>${fold(`<span class="stx-chev">${folded ? "›" : "⌄"}</span>`)}</div>`;
  const list = folded && !showDone ? "" : `<div class="tray-list" data-key="tray-list" data-tray-scroll="${esc(sid)}">${folded ? "" : running.map((h) => helperBlock(sid, h, true)).join("")}${showDone ? done.map((h) => helperBlock(sid, h, false)).join("") : ""}</div>`;
  return `<div class="tray ${folded ? "folded" : ""}" data-key="tray">${head}${list}</div>`;
}
/** One turn's steps, between your message and its reply. `live`: the turn it's working on now;
 *  `latest`: the newest turn (open by default, so you see what led to its reply). `part`: when you
 *  wrote mid-turn, the turn is drawn in parts around your messages ({ from, to } ms); only the first
 *  part has the header (its count is the whole turn's). */
function stepsBlock(sid, turn, live, latest, part = { from: -Infinity, to: Infinity }, said = new Set()) {
  const mode = stepsMode();
  // What a Stop hook made it do goes in its own fold at the end (hookFold); the rest is the turn.
  const hooked = turn.items.filter((x) => hookedPart(turn, x) && (x.t !== "say" || !said.has(String(x.text || "").trim())));
  let kept = turn.items.filter((x) => !hookedPart(turn, x));
  // A finished turn's last words are its reply, already in the chat; so are words the chat shows as a
  // reply mid-turn: not twice.
  if (!live) { let end = kept.length; while (end && kept[end - 1].t === "say") end--; kept = kept.slice(0, end); }
  kept = kept.filter((x) => x.t !== "say" || !said.has(String(x.text || "").trim()));
  const steps = kept.filter((x) => x.t === "step");
  const first = part.from === -Infinity;
  const fold = part.to === Infinity ? hookFold(sid, turn, hooked) : "";
  if (!steps.length) return fold ? `<div class="stx" data-key="${esc(`t:${turn.at_ms}:${part.from}`)}">${fold}</div>` : "";
  const items = kept.map((x, i) => [x, i]).filter(([x]) => x.at_ms >= part.from && x.at_ms < part.to);
  const key = `${sid}|${turn.at_ms}`;
  const open = turnFold.has(key) ? turnFold.get(key) : mode === "all" || (mode === "line" && (live || latest));
  const all = mode === "all" || turnAll.has(key);
  const count = `${steps.length}${turn.more ? "+" : ""} step${steps.length === 1 ? "" : "s"}`;
  const bk = `t:${turn.at_ms}:${part.from}`;
  const head = `<div class="stx-head" data-key="head"><button class="stx-fold" data-turn-fold="${esc(key)}" data-open="${open ? 1 : 0}">${live ? `<span class="dot-live"></span>` : `<span class="stx-tri">${open ? "▾" : "▸"}</span>`}<b>${live ? `Working · ${count}` : count}</b>${open ? "" : `<span class="stx-sum"> · ${esc(turnSummary(steps))}</span>`}</button>${open ? `<button class="stx-all" data-turn-all="${esc(key)}">${all ? "Show less" : "Show all"}</button>` : ""}</div>`;
  if (!open) return first || fold ? `<div class="stx" data-key="${esc(bk)}">${first ? head : ""}${fold}</div>` : "";
  if (!items.length) return first || fold ? `<div class="stx" data-key="${esc(bk)}">${first ? head : ""}${fold}</div>` : "";
  let body = "", run = [], runKey = "";
  // A run of steps is keyed by its first step: a paragraph between steps starts the next run.
  const flush = () => { if (run.length) body += `<div class="stx-steps" data-key="run:${esc(runKey)}">${run.join("")}</div>`; run = []; };
  const finished = new Map(helpersOf(sid).filter((h) => !helperRunning(h)).map((h) => [h.step, h]));
  items.forEach(([x, i]) => {
    if (x.t !== "say") { if (!run.length) runKey = x.id; run.push(stepLine(sid, x, all)); const h = finished.get(x.id); if (h) run.push(helperBlock(sid, h, false)); return; }
    flush();
    // New words unroll top to bottom, quicker for short ones.
    const ms = Math.min(REVEAL_MS, 250 + x.text.length * 2);
    body += `<div class="stx-say msg" data-key="${esc(itemKey(turn, x, i))}" style="--rv:${ms}ms">${md(x.text)}</div>`;
  });
  flush();
  if (!first) return `<div class="stx" data-key="${esc(bk)}">${body}${fold}</div>`;
  return `<div class="stx" data-key="${esc(bk)}">${head}${turn.more ? `<div class="stx-more dim" data-key="more">${turn.more} earlier steps not shown</div>` : ""}${body}${fold}</div>`;
}
/** A turn's "After a stop hook": folded, what the hook made it do and write (a checklist, the commands it
 *  ran for it), so it never reads as the answer, live or after. "" when no hook sent it back. */
function hookFold(sid, turn, hooked) {
  if (!hooked.length) return "";
  const key = `${sid}|${turn.at_ms}`, open = hookOpen.has(key);
  const busy = hooked.some((x) => x.t === "step" && !x.done);
  const body = open ? hooked.map((x, i) => x.t === "step" ? `<div class="stx-steps">${stepLine(sid, x, false)}</div>` : `<div class="stx-say msg" data-key="${esc(itemKey(turn, x, i))}">${md(x.text)}</div>`).join("") : "";
  return `<div class="follow stx-hook" data-key="hook"><button class="thread-toggle" data-hook-fold="${esc(key)}">${busy ? `<span class="dot-live"></span>` : ""}After a stop hook ${open ? "▴" : "▾"}</button>${open ? `<div class="follow-body">${body}</div>` : ""}</div>`;
}
/** A turn since your last message (`since`) has its "After a stop hook" in the chat, so the reply's own copy
 *  of it, from its card, isn't drawn as well. Since, not the last turn only: a background task reporting
 *  after the hook starts a turn of its own, and the hook's part stays in the one before. */
const hookInChat = (sid, since) => stepsMode() !== "hidden" && !!stepFeeds.get(sid)?.turns?.some((t) => t.hooked_ms && t.hooked_ms >= since);
/** The chat with each turn's steps put in by time: after your message, before the reply. `extra`:
 *  more lines to place by time (what you chose on a decision, when you chose it). */
function withSteps(sid, s, ex, rows, extra = []) {
  const f = sid && stepFeeds.get(sid);
  const first = ex[0]?.at_ms ?? 0;
  const turns = f?.turns || [];
  const lastTurn = turns.at(-1);
  // Its replies, already in the chat: a Stop hook that made it carry on leaves the answer mid-turn.
  const said = new Set(ex.filter((e) => e.role === "agent").map((e) => e.text.trim()));
  // A turn you wrote into while it ran is split at your messages, so the chat stays in time order.
  const blocks = turns
    .filter((t) => (t.items.at(-1)?.at_ms ?? t.at_ms) >= first || (!ex.length && t === lastTurn))
    .flatMap((t) => {
      const start = t.items[0]?.at_ms ?? t.at_ms, end = t.items.at(-1)?.at_ms ?? start;
      const cuts = ex.map((e) => e.at_ms).filter((a) => a > start && a <= end);
      const bounds = [-Infinity, ...cuts, Infinity];
      return cuts.concat([null]).map((_, k) => ({
        at: k === 0 ? start : cuts[k - 1] + 0.5,
        html: stepsBlock(sid, t, t === lastTurn && s?.state === "working", t === lastTurn, { from: bounds[k], to: bounds[k + 1] }, said),
      }));
    })
    .concat(extra)
    .filter((b) => b.html)
    .sort((a, b) => a.at - b.at);
  if (!blocks.length) return rows;
  const out = [];
  let b = 0;
  ex.forEach((e, n) => { while (b < blocks.length && blocks[b].at < e.at_ms) out.push(blocks[b++].html); out.push(rows[n]); });
  while (b < blocks.length) out.push(blocks[b++].html);
  return out;
}
/** Agent replies left open: the newest few, so the one you were reading doesn't fold the moment the next lands. */
const OPEN_REPLIES = 3;
/** Agent: full-width prose. You: a compact tinted bubble on the right. Older agent turns fold to 3 lines.
 *  Another session: a striped bubble saying which one (sent by its agent, or forwarded by you). */
/** `paged`: the Active pane's chat, which loads earlier messages as you scroll up. */
function chatHtml(it, s, harness, paged = false) {
  const ex = conversation(it, s);
  const lastAgent = ex.map((e) => e.role).lastIndexOf("agent");
  const agentAt = ex.flatMap((e, n) => (e.role === "agent" ? [n] : []));
  const recent = new Set(agentAt.slice(-OPEN_REPLIES));
  const project = s?.project || it?.project || "";
  const sid = s?.session_id || it?.session_id;
  // What you chose on a decision sits in the chat when you chose it (later steps come below it).
  const K = keyer();
  const chose = it && !isPending(it) && it.resolved_ms ? [{ at: it.resolved_ms, html: keyed(resultLine(it), "chose") }] : [];
  const ran = sid ? (ranLog.get(sid) || []).map((e, n) => ({ at: e.at, html: keyed(ranCard(sid, e, n), K(`ran:${e.at}`)) })) : [];
  const restarts = sid ? (state.restarted?.[sid] || []).map((r, n, all) => ({ at: r.at_ms, html: keyed(restartLine(sid, r, n === all.length - 1), K(`rs:${r.at_ms}`)) })) : [];
  const rows = withSteps(sid, s, ex, ex.map((e, n) => keyed((() => {
    if (e.role === "peer") return peerBubble(e.from || "another agent", "", e.at_ms, e.text);
    const fwd = e.role === "you" && FORWARDED.exec(e.text);
    if (fwd) return peerBubble(fwd[2], "forwarded by you", e.at_ms, fwd[1], fwd[3]);
    if (e.role === "you") {
      const how = lastSent && lastSent.sid === (s?.session_id || it?.session_id) && e.text.startsWith(lastSent.text) ? lastSent.via : "";
      // "queued via iTerm session: it reads this…" read as "via queued via…": say it plainly.
      const via = !how ? "" : /^queued/.test(how) ? " · queued, it reads this at its next step" : ` · via ${esc(how)}`;
      // Typed into its terminal, but it never became a message: say so where you'll see it.
      const unsent = e.unsent ? `<div class="cv-unsent">⚠ This didn't go through as a message. It may have run as a command, or still be in its box.${sid ? ` <button data-act="resend" data-sid="${esc(sid)}" data-resend="${esc(e.text)}">Send again</button>` : ""}</div>` : "";
      // Typed, and Cue hasn't yet seen it take it: a ring until it does (or until Cue can say it didn't).
      // Past half a minute it's no longer "any moment now": say what Cue knows, which is nothing yet.
      const wait = !e.pending ? "" : Date.now() - e.at_ms > 30_000 ? " · Cue hasn't seen it take this yet" : " · waiting for it to take this";
      // Sent now right after the one above, before it answered: the agent got both as one (sessions.rs note_with).
      const joined = e.joined ? " · went to the agent with your message above, as one" : "";
      return `<div class="cv-you ${e.unsent ? "unsent" : ""} ${e.pending ? "pending" : ""}">${yourBubble(e, `${sid}:${e.at_ms}`)}${thumbs(e.images)}<div class="cv-meta">You · ${ago(e.at_ms)} ago${via}${wait}${joined}</div>${unsent}</div>`;
    }
    const key = `${s?.session_id || it?.id}:${e.at_ms}`;
    const long = e.text.length > 280 || e.text.split("\n").length > 4;
    const folded = !recent.has(n) && long && !openMsgs.has(key);
    const follow = it?.followup && e.text === it.message && !hookInChat(sid, ex.findLast((y) => y.role === "you" && y.at_ms < e.at_ms)?.at_ms ?? 0) ? followHtml(it) : "";
    fwdMsgs.set(key, { text: e.text, from: project, fromSid: sid });
    return `<div class="cv-agent ${folded ? "folded" : ""} ${n === lastAgent ? "last" : ""}"><div class="cv-meta">${esc(agentName(harness))}${machTag(it?.machine || s?.machine)} · ${ago(e.at_ms)} ago</div>
      <div class="msg cv-text">${md(e.text)}</div>${thumbs(e.images)}${localImages(e.text)}
      <div class="cv-acts">${sid ? listenBtn(key) : ""}${long && paged && sid ? `<button class="cv-pill" data-read="${esc(key)}">${BOOK_ICON}Read</button>` : ""}${!recent.has(n) && long ? `<button class="cv-more" data-msg="${esc(key)}">${folded ? "Show all" : "Fold"}</button>` : ""}${n === lastAgent ? actReplyHtml(s, e.at_ms) : ""}<button class="cv-fwd" data-fwd="${esc(key)}">↗ Send to…</button></div>${follow}</div>`;
  })(), K(`${e.role[0]}:${e.at_ms}`))), chose.concat(ran, restarts));
  for (const o of outbox) if (o.sid === (s?.session_id || it?.session_id) && !landed(o)) rows.push(`<div class="cv-you ${o.via ? "" : "sending"}" data-key="${esc(K(`out:${o.at}`))}"><div class="cv-you-text">${esc(o.text).replace(/\n/g, "<br>")}</div>${o.images.length ? `<div class="thumbs">${o.images.map((im) => `<span class="thumb"><img src="${esc(im.data)}" alt=""/></span>`).join("")}</div>` : ""}<div class="cv-meta" data-sid="${esc(o.sid)}">${/^kept in Cue/.test(o.via || "") ? "Kept in Cue · it goes when this turn ends" : o.via ? `Sent · via ${esc(o.via)}` : esc(sendingMeta(o.sid))}</div></div>`);
  // Sent while it worked: Cue keeps it until the turn ends. Yours to take back (Edit, Esc) or send now.
  // Reviews you asked for while this lead was busy: each goes, in order, once it's free (× takes one back).
  for (const q of (s && state.review_queue?.[s.session_id]) || []) rows.push(`<div class="cv-you queued review-q" data-key="${esc(K(`rq:${q.exec}`))}"><div class="cv-you-text" style="${qPhase()}">Review ${esc(q.name)}</div><div class="cv-meta">Queued in Cue · it goes when this lead is free · <button class="q-now" data-act="unqueue-review" data-exec="${esc(q.exec)}">Remove</button></div></div>`);
  // A kept message that couldn't be sent says so (and why), rather than promising it goes when the turn ends.
  const heldWhy = s?.held?.unsent ? (s.held.delivery || []).map((d) => d.what).filter((w) => w.startsWith("couldn't send: ")).pop() : "";
  if (s?.held) rows.push(`<div class="cv-you queued held" data-key="held">${yourBubble(s.held, `${s.session_id}:held`, qPhase(), true)}${thumbs(s.held.images)}<div class="cv-meta">${heldWhy ? `Couldn't send it: ${esc(heldWhy.slice(15))}` : "Kept in Cue · it goes when this turn ends"} · <button class="q-now" data-act="held-edit" data-sid="${esc(s.session_id)}" title="Back into the box to change it (Esc)">Edit</button> · <button class="q-now" data-act="held-now" data-sid="${esc(s.session_id)}">Send now <kbd>⌘↵</kbd></button></div></div>`);
  if (s?.queued) rows.push(`<div class="cv-you queued" data-key="queued">${yourBubble(s.queued, `${s.session_id}:queued`, qPhase(), true)}${thumbs(s.queued.images)}<div class="cv-meta">Queued · it reads this when it finishes the current step · <button class="q-now" data-act="send-now" data-sid="${esc(s.session_id)}">Send now <kbd>⌘↵</kbd></button></div></div>`);
  return rows.length ? (paged && sid ? keyed(olderRow(sid, harness), "older") : "") + rows.join("") : ran.length ? ran.map((r) => r.html).join("") : `<div class="dim cv-empty" data-key="empty">No messages yet in this session.</div>`;
}
/** In the chat where a restart (after an MCP change) ended: back, with which servers and how long it took,
 *  or why it didn't finish. The latest one also says how each server did, once Claude Code has said
 *  (logs/mcp.rs with_live: after its next message); until then, just their names. */
function restartLine(sid, r, latest) {
  const when = `<span class="grow"></span><span class="dim nowrap">${ago(r.at_ms)} ago</span>`;
  if (r.error) return `<div class="cv-result no"><span class="sent-check">✕</span><span>The restart didn't finish: ${esc(r.error)}</span>${when}</div>`;
  const n = r.servers?.length;
  const title = r.servers ? `Restarted with ${n} MCP server${n === 1 ? "" : "s"}` : "Restarted with the folder's MCP servers";
  if (latest) mcpLoad(sid);
  const d = latest ? mcpCache.get(sid)?.data : null;
  const known = d?.live_known ? (d.servers || []).filter((m) => (r.servers ? r.servers.includes(m.name) : m.selected ?? m.enabled)) : null;
  const list = known?.length ? known.map((m) => `<span class="rs-sv-it"><b>${esc(m.name)}</b>${mcpLiveWord(m)}</span>`).join("")
    : r.servers?.length ? r.servers.map((s) => `<span class="rs-sv-it"><b>${esc(s)}</b></span>`).join("") : "";
  const later = latest && !known?.length ? `<div class="rs-sv">Whether each one connected shows after its next message</div>` : "";
  return `<div class="cv-result rs-line"><span class="sent-check">✓</span><span>${title}<span class="dim"> · ${r.secs}s</span></span>${when}${list ? `<div class="rs-sv">${list}</div>` : ""}${later}</div>`;
}
/** A turn's changed files, one per line, for the pill's tooltip. */
const changesTip = (ch) => ch.files.map((f) => `${f.path}  +${f.add} −${f.del}`).join("\n");
/** Under the chat: where the session is now, and while any of its background agents run, their tray, whatever
 *  state it's in (working on your message, your turn, waiting on them), so they never drop out of sight. */
function statusLine(it, s) {
  const tray = s?.session_id ? helperTray(s.session_id) : "";
  // Waiting on them: the tray says it all.
  if (tray && s.state === "agent" && s.helpers) return tray;
  return tellLine(s) + whereNow(it, s) + tray;
}
/** "Tell me" on this turn: a line says Cue is watching it, then that it's done and how long it took. */
function tellLine(s) {
  if (atWork(s) && s.watch_ms) return `<div class="cv-status watch-line">${tadaIcon(13, true)}<span>Telling you when this is done</span></div>`;
  if (s?.told_ms && !atWork(s)) return `<div class="cv-status watch-line">${tadaIcon(13, true)}<span><b>Done</b>${s.told_ms >= 1000 ? ` · took ${tookText(s.told_ms)}` : ""} · you asked to be told</span></div>`;
  return "";
}
/** How long a turn ran: "45s", "38m", "1h 52m" (as its notification says it). */
const tookText = (ms) => { const sec = Math.floor(ms / 1000); return sec < 60 ? `${sec}s` : sec < 3600 ? `${Math.floor(sec / 60)}m` : `${Math.floor(sec / 3600)}h ${Math.floor(sec % 3600 / 60)}m`; };
/** One line under the chat: where the session is now. */
function whereNow(it, s) {
  // Started in a folder Claude Code doesn't trust yet: it's asking in its terminal before it does anything.
  if (s?.trust_ms) return `<div class="cv-status stuck"><span class="lim-dot"></span><span><b>Claude Code is asking whether you trust ${esc(homeless(s.cwd) || "this folder")}.</b> It won't start until you answer.</span><button class="btn small primary" data-act="trust" data-sid="${esc(s.session_id)}">Trust folder</button></div>`;
  // Waiting for Enter on a screen before Claude Code starts ("Security notes … Press Enter to continue").
  if (s?.enter_ms) return `<div class="cv-status stuck"><span class="lim-dot"></span><span><b>Claude Code is waiting for Enter in its terminal before it starts.</b> Nothing runs until then.</span><button class="btn small primary" data-act="continue-start" data-sid="${esc(s.session_id)}">Continue</button></div>`;
  // Working, but nothing new from it in a while: a command waiting for input in its terminal, or hung.
  if (s?.state === "working" && s.stuck_ms) return `<div class="cv-status stuck"><span class="lim-dot"></span><span><b>No new output for <span data-ago="${s.stuck_ms}">${ago(s.stuck_ms)}</span>.</b> ${esc((s.doing || "Thinking").replace(/…$/, ""))}. If it's waiting for input, it's in its terminal.</span></div>`;
  // Compact (⋯) typed /compact: it's summarizing, until Claude Code says it's done.
  if (s?.compacting_ms) return `<div class="cv-status"><span class="dot-live"></span>Compacting: summarizing the conversation to free up context<span class="dim"> · <span data-ago="${s.compacting_ms}">${ago(s.compacting_ms)}</span></span></div>`;
  // It finished compacting (until the next turn starts).
  if (s?.compacted_ms && s.state !== "working") return `<div class="cv-status"><span class="ok-dot"></span>Compacted: the conversation was summarized and its context freed<span class="dim"> · <span data-ago="${s.compacted_ms}">${ago(s.compacted_ms)}</span> ago</span></div>`;
  // What it's doing right now (its latest step), else the prompt it's on.
  if (s?.state === "working" && s.doing) return `<div class="cv-status"><span class="dot-live"></span>${esc(s.doing)}<span class="dim"> · <span data-ago="${s.doing_ms}">${ago(s.doing_ms)}</span></span></div>`;
  // Nothing in its transcript yet (it's thinking; Claude Code writes a message once it's whole): a live
  // count since the turn started, so a long think doesn't look stuck.
  if (s?.state === "working") return `<div class="cv-status"><span class="dot-live"></span>Working${s.prompt ? ` on: ${esc(s.prompt.length > 120 ? s.prompt.slice(0, 120) + "…" : s.prompt)}` : ""}<span class="dim"> · <span data-ago="${s.since_ms}">${ago(s.since_ms)}</span></span></div>`;
  if (s?.state === "waiting" || s?.state === "deciding") return `<div class="cv-status"><i class="sw z live"></i>Waiting on you · ${ago(s.since_ms)}</div>`;
  if (s?.state === "agent") return `<div class="cv-status">${s.helpers ? `<span class="dot-live"></span>Waiting on ${esc(s.driven_by)}` : `Idle, waiting on ${esc(s.driven_by || "another agent")}`}</div>`;
  if (s?.state === "limited" && s.limit) {
    const l = s.limit, what = s.prompt ? `“${esc(s.prompt.length > 80 ? s.prompt.slice(0, 80) + "…" : s.prompt)}”` : "Your last message";
    if (lifted(l)) return `<div class="cv-status limit"><span class="lim-dot back"></span><span><b>${esc(scopeName(l))} is back.</b> ${what} didn't run.</span><button class="btn small" data-usage="resend-one" data-sid="${esc(s.session_id)}">Resend</button></div>`;
    const when = l.resets_ms ? ` · resets ${clockAt(l.resets_ms)}${l.resets_ms - now() < 20 * 3600000 ? `, in ${untilText(l.resets_ms - now())}` : ""}` : "";
    const head = l.scope === "all" ? "Out of usage" : `${esc(l.scope)} used up`;
    const tail = l.scope === "all" ? "Cue will tell you when it resets." : "Switch models with /model to keep going.";
    return `<div class="cv-status limit"><span class="lim-dot"></span><span><b>${head}</b>${when}. ${what} didn't run. ${tail}</span></div>`;
  }
  if (s?.state === "stopped") return `<div class="cv-status">Stopped by you · it's at its prompt</div>`;
  if (!s && it && !isPending(it)) return `<div class="cv-status">This session has ended.</div>`;
  return "";
}
/** After a decision: what you chose. Replies need no line, they're already in the chat. */
function resultLine(it) {
  if (!it || isPending(it) || !it.resolved_ms || it.kind === "waiting") return "";
  return `<div class="cv-result ${outcomeClass(it)}"><span class="sent-check">${outcomeClass(it) === "no" ? "✕" : "✓"}</span><span>${esc(outcomeText(it))}</span><span class="grow"></span><span class="dim nowrap">${ago(it.resolved_ms)} ago</span></div>`;
}
function nextBar() {
  const n = nextUp();
  if (!n) return "";
  // × only on a finished turn; a decision would just hide something the agent is still blocked on.
  const clear = n.kind === "waiting" ? `<button class="next-clear" data-act="dismiss" data-id="${esc(n.id)}" title="Take it off Waiting" aria-label="Take it off Waiting">×</button>` : "";
  return `<div class="next-bar" data-act="next" role="button" tabindex="0"><span class="next-label">Next</span>${badge(n.harness)}<span class="next-proj">${esc(n.project)}</span><span class="next-what">${esc(plain(summary(n)))}</span>${clear}<kbd>N</kbd></div>`;
}

/** The chat header's ⋯: what's used now and then. Compact (any agent's /compact), a relay lead's Relay
 *  handoff (asks once more first: the lead steps down), and Decide later / Back to Waiting. */
let moreFor = null, handoffArm = null, movingProj = false;
/** + New project… at the foot of Move to project: the session whose list has the name box open (draft
 *  `newproj:<sid>`), and what the box last said back (a name already taken). */
let newProjFor = null, newProjNote = "";
/** ⋯ → Move to project: the projects it can show under instead, its folder's first, then + New project…. */
function projMoveList(sid) {
  const own = folderProject(sid), now = sessionOf(sid)?.project ?? findItem(active?.id)?.project ?? own;
  const names = [...new Set([own, ...projectsKnown().map(([p]) => p)])].filter((p) => p && p !== now);
  const rows = names.map((p) => `<button data-move-proj="${esc(p)}" data-sid="${esc(sid)}">${projDot(p)}${esc(projLabel(p))}${p === own ? "<span>Its folder's own</span>" : ""}</button>`).join("");
  const add = newProjFor === sid
    ? `<div class="pm-new"><input class="pm-new-in" data-text="newproj:${esc(sid)}" maxlength="40" placeholder="Name the new project" spellcheck="false" value="${esc(draft(`newproj:${sid}`).text)}"/>${newProjNote ? `<div class="pm-note">${esc(newProjNote)}</div>` : ""}</div>`
    : `<button class="pm-add" data-new-proj="${esc(sid)}">+ New project…</button>`;
  return `<div class="more-menu proj-move"><div class="pm-title">Move to project</div><div class="pm-note">This session only</div>${rows}${add}</div>`;
}
/** Enter in + New project…'s box: this session goes to a project of that name. One that's already there (its
 *  name or the name you gave it) is said instead, so two never quietly become one. */
function submitNewProj(sid) {
  const name = draft(`newproj:${sid}`).text.trim();
  if (!name) return;
  const taken = takenProject(name, [folderProject(sid)]);
  if (taken) { newProjNote = `${projLabel(taken)} is already a project: pick it above`; return renderMain(); }
  draft(`newproj:${sid}`).text = "";
  newProjFor = null; newProjNote = ""; moreFor = handoffArm = null; movingProj = false;
  moveToProject(sid, name);
  // The window goes to the new project, the moved session still open in it.
  projFilter = name;
  try { localStorage.setItem("cue.projFilter", projFilter); } catch {}
  renderMain();
}
function moreMenu(sid, s, closable = false) {
  const open = moreFor === sid;
  if (open && movingProj) return `<span class="more-wrap"><button class="btn more-btn" data-more="${esc(sid)}" aria-label="More" aria-expanded="true">⋯</button>${projMoveList(sid)}</span>`;
  if (open && mcpPicking) return `<span class="more-wrap"><button class="btn more-btn" data-more="${esc(sid)}" aria-label="More" aria-expanded="true">⋯</button>${mcpPicker(sid)}</span>`;
  const busy = s?.state === "working";
  const m = crewOf(sid);
  const lead = m?.role === "lead" && m?.plugin === "relay";
  const later = isParked(sid) ? `<button data-park="${esc(sid)}:0">Back to Waiting</button>` : `<button data-park="${esc(sid)}:1">Decide later<span>Move it to Need to decide while you think it over</span></button>`;
  const items = !open ? "" : `<div class="more-menu">
      <button data-cmd="compact" data-sid="${esc(sid)}" ${busy ? "disabled" : ""}>Compact<span>${busy ? "When this turn ends" : "Summarize the conversation to free up context"}</span></button>
      ${lead ? `<button data-cmd="handoff" data-sid="${esc(sid)}" ${busy ? "disabled" : ""} class="${handoffArm === sid ? "armed" : ""}">${handoffArm === sid ? "Relay handoff? Click again" : "Relay handoff…"}<span>${busy ? "When this turn ends" : "Through relay (/relay:handoff): it writes its notes, relay opens a new lead and steps this one down"}</span></button>` : ""}
      <button data-move-ask="${esc(sid)}">Move to project…<span>Show its folder's sessions under another project in Cue; the agent stays in its folder</span></button>
      ${mcpItem(sid)}
      ${restartItem(sid, s)}
      ${forkItem(sid, s)}
      ${later}${parkMenuItem(sid, s, closable)}
      <button data-delete-ask="${esc(sid)}" ${busy ? "disabled" : ""}>Delete…<span>${busy ? "When this turn ends" : "Gone for good: from Cue, and its conversation, so it can't be resumed"}</span></button></div>`;
  return `<span class="more-wrap"><button class="btn more-btn" data-more="${esc(sid)}" aria-label="More" aria-expanded="${open}">⋯</button>${items}</span>`;
}
/** ⋯ → Restart session: stopped and resumed with the same conversation, so it loads what it reads only at
 *  start (a skill added since, MCP servers, settings). Covered while it restarts, as an MCP restart is. */
function restartItem(sid, s) {
  if (!s || !["claude", "codex", "pi"].includes(s.harness) || !s.cwd || crewOf(sid)?.role === "executor") return "";
  const busy = s.state === "working";
  return `<button data-restart="${esc(sid)}" ${busy ? "disabled" : ""}>Restart session<span>${busy ? "When this turn ends" : "Same conversation, fresh start: picks up new skills, MCP servers and settings"}</span></button>`;
}
/** ⋯ → Fork, for a Claude Code session: its conversation goes on as a new session too (forkBtw). */
function forkItem(sid, s) {
  if (s?.harness !== "claude") return "";
  return `<button data-fork="${esc(sid)}" ${btwForking != null ? "disabled" : ""}>Fork<span>Its conversation goes on as a new session too, with -fork on its name; this one carries on as it is</span></button>`;
}
/** ⋯ → MCP servers…, for a Claude Code session on this Mac: how many it runs with. */
function mcpItem(sid) {
  const s = sessionOf(sid);
  if (s?.harness !== "claude" || s?.machine) return "";
  mcpLoad(sid);
  const n = mcpOnCount(sid);
  return `<button data-mcp-ask="${esc(sid)}">${n == null ? "MCP servers…" : `MCP servers · ${n} on`}<span>Pick which it runs with; it restarts with them</span></button>`;
}
/** "claude-opus-5-5" → "Opus 5.5", "gpt-5.6-terra" → "GPT-5.6 Terra", "deepseek-v4-pro" → "DeepSeek V4 Pro". */
function prettyModel(id) {
  const m = String(id || "").replace(/\[.*\]$/, "").replace(/^.*\//, "");
  const claude = /^claude-([a-z]+)-(\d+)(?:-(\d+))?/.exec(m);
  if (claude) return `${claude[1][0].toUpperCase()}${claude[1].slice(1)} ${claude[2]}${claude[3] ? `.${claude[3]}` : ""}`;
  return m.split("-").map((w) => /^gpt$/i.test(w) ? "GPT" : /^deepseek$/i.test(w) ? "DeepSeek" : /^v\d/i.test(w) ? w.toUpperCase() : w[0] ? w[0].toUpperCase() + w.slice(1) : w)
    .join(" ").replace(/^GPT (\d)/, "GPT-$1");
}
/** The open session's model, how full its context is, and its cost so far
 *  (where the agent logs it), on the chat's title line after Claude's title; context as a small gauge. From its log. */
function metaInline(sid) {
  const m = sid && stepFeeds.get(sid)?.meta;
  if (!m || (!m.model && !m.context)) return "";
  const pct = m.window ? Math.min(100, Math.round((m.context / m.window) * 100)) : null;
  const lvl = pct === null ? "" : pct >= 95 ? "full" : pct >= 80 ? "high" : "";
  const k = (n) => (n >= 1e6 ? `${(n / 1e6).toFixed(n % 1e6 ? 2 : 0)}M` : `${Math.round(n / 1000)}k`);
  const money = (c) => `$${c >= 100 ? Math.round(c) : c.toFixed(2)}`;
  const tip = [m.model && `Model: ${m.model}`, m.window && `Context: ${k(m.context)} of ${k(m.window)} tokens (${pct}%)`, m.cost != null && `Cost so far: ${money(m.cost)}`].filter(Boolean).join("\n");
  // No share until Cue knows the window (Claude Code's status line says it): the tokens alone, not a guess.
  // Just compacted: its log's last count is from before, so don't show it until a new reply brings a fresh one.
  const s = sessionOf(sid);
  const freed = s?.compacted_ms && s.state !== "working";
  const parts = [m.model ? `<b>${esc(prettyModel(m.model))}</b>` : "", freed ? `<span class="mi-ctx">context freed</span>` : pct !== null ? `<span class="mi-ctx ${lvl}"><span class="ctx-bar"><i style="width:${pct}%"></i></span>${pct}% context</span>` : m.context ? `<span class="mi-ctx">${k(m.context)} context</span>` : "", m.cost != null ? money(m.cost) : ""].filter(Boolean);
  // Context high and it's not mid-turn: Compact right here (the same as ⋯ → Compact).
  const chip = pct !== null && pct >= 80 && s && s.state !== "working" && !s.compacting_ms && !freed ? `<button class="mi-compact" data-cmd="compact" data-sid="${esc(sid)}">Compact</button>` : "";
  return `<span class="ap-meta">${parts.join(" · ")}</span>${chip}`;
}
/** A quiet session open in Active: running, but started before Cue was connected, so it never
 *  reports to Cue. Read-only: what it's been doing, from its transcript, and how to make it a full one. */
let quietOpen = null;
function quietPane() {
  const x = execTerm(quietOpen);
  if (x) return execPane(quietOpen, x);
  const q = (state.live || []).find((x) => x.session_id === quietOpen);
  if (!q) { quietOpen = null; return activePane(); }   // it ended (or reported in): back to the usual
  const f = stepFeeds.get(q.session_id);
  const busy = q.status === "busy";
  const turns = (f?.turns || []).slice(-8);
  const chat = turns.map((t, i) => quietTurn(q.session_id, t, busy && i === turns.length - 1, i === turns.length - 1, q.harness)).join("");
  return `<div class="active-pane ${busy ? "busy" : ""}">
    <div class="ap-head">${projBtn(q.project || projOf(q.cwd), "ap")}${badge(q.harness)}<span class="proj">${esc(bareName(q.name) || baseName(q.cwd) || "session")}</span><span class="pill soft">${busy ? "working" : "quiet"}</span><span class="grow"></span>
      ${moveBtn(q.session_id, q.harness, busy ? "working" : "idle", "btn primary")}<button class="btn" data-sv="tab" data-sid="${esc(q.session_id)}">${openInLabel(null)}</button></div>
    ${subHead(q.cwd, "", "", q.session_id)}
    <div class="quiet-note">It started before Cue was connected, so Cue can show what it's doing but can't answer it yet. In its terminal, type <code>/hooks</code> once to pick up Cue's hooks (or restart it with <code>claude --resume</code>; the conversation carries on), and you can reply, allow and answer from Cue.</div>
    ${chatBox(`q|${q.session_id}`, chat || `<div class="dim cv-empty" data-key="empty">${f ? "Nothing in its transcript yet." : "Reading its transcript…"}</div>`)}</div>`;
}
/** A remote executor Cue doesn't hold (its hooks never reached Cue): its team, its packet as its lead's
 *  listing has it, and its own terminal, which fills the pane. */
function execPane(sid, e) {
  const st = exState(e);
  return `<div class="active-pane exec-pane">
    <div class="ap-head">${badge(e.plugin === "pilead" ? "pi" : "claude")}<span class="proj">${esc(e.name || sid)}</span><span class="role executor">EXEC</span>${e.model ? `<span class="dim">${esc(shortModel(e.model))}</span>` : ""}<span class="mach">${esc(machShort(e.machine))}</span>${st.word ? `<span class="pill soft">${esc(st.word)}</span>` : ""}<span class="grow"></span></div>
    ${teamBar(sid)}${packetCard(sid, e)}
    ${termDock(sid)}</div>`;
}
/** One turn of a quiet session: your message, its steps, its reply (all from the transcript). */
function quietTurn(sid, t, live, latest, harness = "claude") {
  const you = t.prompt ? `<div class="cv-you" data-key="y:${t.at_ms}"><div class="cv-you-text">${linkify(esc(t.prompt)).replace(/\n/g, "<br>")}</div><div class="cv-meta">You · ${ago(t.at_ms)} ago</div></div>` : "";
  // Its reply: the words after its last step (the steps leave a finished turn's last words to the chat).
  const tail = [];
  if (!live) for (let k = t.items.length - 1; k >= 0 && t.items[k].t === "say"; k--) tail.unshift(t.items[k]);
  const reply = tail.length ? `<div class="cv-agent" data-key="a:${t.at_ms}"><div class="cv-meta">${esc(agentName(harness))} · ${ago(tail.at(-1).at_ms)} ago</div><div class="msg cv-text">${md(tail.map((x) => x.text).join("\n\n"))}</div></div>` : "";
  return you + stepsBlock(sid, t, live, latest) + reply;
}
// ---------- Parked: sessions you stopped, to come back to ----------
// Park ends a session's agent (as Close does) and keeps it in the Parked drawer; Resume runs its
// agent's own resume command and it picks up where it left off. Nothing wakes it but you.
const PARKABLE = new Set(["claude", "codex", "pi"]);
const waking = new Set();   // parked sessions on their way back
let parkedAll = null;       // the parked session whose whole kept thread shows (else its last 2 turns)
/** The parking sign: a small rounded square with a bold P, wherever a session is parked. */
const P_SIGN = `<i class="p-sign" aria-hidden="true">P</i>`;
/** How long ago it was parked: "just now" for the first minute. */
const parkedAgo = (p) => { const at = p.at ?? p.parked_ms; return now() - at < 60000 ? "just now" : `${ago(at)} ago`; };
/** Its last `n` turns: from your n-th last message to the end (all of it if it has fewer). */
function lastTurns(thread, n) {
  const yours = thread.flatMap((e, i) => (e.role === "you" ? [i] : []));
  return yours.length > n ? thread.slice(yours.at(-n)) : thread;
}
/** Park, in the Active header's ⋯ menu: where Close is offered (not while it works or asks you, not a
 *  lead with executors still open), for an agent Cue can bring back. Not an executor: relay closes those. */
function parkMenuItem(sid, s, closable) {
  if (!closable || !s || !PARKABLE.has(s.harness) || !s.cwd || crewOf(sid)?.role === "executor") return "";
  return `<button data-park-session="${esc(sid)}">Park<span>Stop it and keep it under Parked; Resume brings it back whole</span></button>`;
}
/** Delete, from the ⋯ menu: asked first, in a dialog (Esc no, Enter yes). `deleteAsk`: the session it asks about. */
let deleteAsk = null;
function deleteDialog() {
  const sid = deleteAsk, name = nameOf(sid, sessionOf(sid)?.project || "this session");
  const m = crewOf(sid), n = m?.role === "lead" && m?.plugin === "relay" ? (m.executors || []).length : 0;
  return `<div class="move-scrim"><div class="move-dialog end-dialog" role="alertdialog" aria-labelledby="del-title">
    <div class="move-title" id="del-title">Delete ${esc(name)}?</div>
    <div class="end-text">${sessionOf(sid) ? `It stops${n ? `, with its ${n} executor${n === 1 ? "" : "s"},` : ""} and its tab closes. ` : ""}Cue forgets it, and its conversation is deleted, so it can't be resumed. This can't be undone.</div>
    <div class="row-btns"><span class="grow"></span><button class="btn" data-act="delete-no">Cancel <kbd>Esc</kbd></button><button class="btn deny" data-act="delete-yes">Delete <kbd>↵</kbd></button></div>
  </div></div>`;
}
async function deleteAnswer(yes) {
  const sid = deleteAsk;
  deleteAsk = null;
  renderMain();
  if (!yes || !sid) return;
  const name = nameOf(sid, sessionOf(sid)?.project || "it");
  try { toast(`${await invoke("delete_session", { sessionId: sid })}: ${name}`); }
  catch (e) { toast(`Couldn't delete ${name}: ${e}`); }
}
async function parkSession(sid) {
  const name = nameOf(sid, sessionOf(sid)?.project || "it");
  try { await invoke("park_session", { sessionId: sid }); toast(`Parked ${name}: its agent stopped and its terminal closed`); }
  catch (e) { toast(`Couldn't park ${name}: ${e}`); }
}
/** Resume (↺, the button, or Resume & send with what's in its box), a parked or a closed one: back in
 *  Sessions, open in Active. */
async function wakeParked(sid, message = "") {
  const parked = parkedOf(sid), p = parked || closedOf(sid);
  if (!p || waking.has(sid)) return;
  waking.add(sid);
  renderMain();
  try {
    const r = await invoke(parked ? "resume_parked" : "resume_closed", { sessionId: sid, message });
    // Until Cue's update has it live, Active keeps showing it here (else it would move on to something else).
    for (let n = 0; n < 30 && !sessionOf(sid); n++) await new Promise((ok) => setTimeout(ok, 100));
    if (!parked) closedList = (closedList || []).filter((c) => c.session_id !== sid);
    if (message) { draft(`s:${sid}`).text = ""; saveDrafts(); }
    toast(`${p.name || p.project} is back: ${r.replace(/^Resumed/, "resumed")}`);
    setActive(null, sid);
  } catch (e) {
    toast(`Couldn't resume ${p.name || p.project}: ${e}`);
  } finally {
    waking.delete(sid);
    renderMain();
  }
}
/** Park, on a closed one: kept under Parked (it has ended already, so nothing stops). */
async function parkClosed(sid) {
  const c = closedOf(sid);
  try { await invoke("park_closed", { sessionId: sid }); }
  catch (e) { return toast(`Couldn't park it: ${e}`); }
  // Shown parked now (Cue's update says the same), so Active stays on it.
  if (c && !parkedOf(sid)) state.parked = [{ ...c, parked_ms: Date.now() }, ...(state.parked || [])];
  closedList = (closedList || []).filter((x) => x.session_id !== sid);
  toast(`Parked ${c?.name || c?.project || "it"}`);
  renderMain();
}
/** Unpark: off the list, nothing started. Its conversation stays in History and ⌘F. */
async function unparkSession(sid) {
  try { await invoke("unpark_session", { sessionId: sid }); }
  catch (e) { return toast(`Couldn't unpark it: ${e}`); }
  if (active?.sid === sid) active = null;
  toast("Unparked: its conversation is still in History");
}
/** The last drawer (below Recently answered), headed "▸ PARKED 8 · CLOSED 30": each word opens its list,
 *  Parked (the latest parked first) or Closed (what ended in the last 30 days, at most 30, to find the
 *  ones that got lost). One slim line per session; a list too long for its room ends on "N more". */
const pkTab = () => { try { return localStorage.getItem("cue.pkTab") === "closed" ? "closed" : "parked"; } catch { return "parked"; } };
/** A parked or closed session in Active: its last two turns (more on request), where it stopped, Resume
 *  (and Unpark for a parked one, Park for a closed one). */
function parkedPane(p) {
  const sid = p.session_id, harness = p.harness, name = p.name || baseName(p.cwd) || p.project || "session", w = waking.has(sid), closed = p.kind === "closed";
  const thread = p.thread || [];
  const shown = parkedAll === sid ? thread : lastTurns(thread, 2);
  const hidden = thread.length - shown.length;
  const more = hidden > 0 ? `<button class="park-more" data-key="more" data-park-all="${esc(sid)}">▴ ${hidden} earlier message${hidden === 1 ? "" : "s"}</button>`
    : parkedAll === sid && lastTurns(thread, 2).length < thread.length ? `<button class="park-more" data-key="more" data-park-all="${esc(sid)}">▾ Just the last two turns</button>` : "";
  const K = keyer();
  const rows = shown.map((e) => keyed(e.role === "you"
    ? `<div class="cv-you"><div class="cv-you-text">${linkify(esc(e.text)).replace(/\n/g, "<br>")}</div>${thumbs(e.images)}<div class="cv-meta">You · ${ago(e.at_ms)} ago</div></div>`
    : `<div class="cv-agent"><div class="cv-meta">${esc(e.role === "peer" ? e.from || "another agent" : agentName(harness))}${machTag(p.machine)} · ${ago(e.at_ms)} ago</div><div class="msg cv-text">${md(e.text)}</div></div>`, K(`${e.role[0]}:${e.at_ms}`))).join("");
  const where = p.machine ? ` on ${esc(machShort(p.machine))}` : "";
  const line = w
    ? `<div class="cv-status" data-key="line"><span class="dot-live"></span>Resuming${where}…</div>`
    : `<div class="park-line" data-key="line">${closed ? "" : P_SIGN}${closed ? "Closed" : "Parked"} ${parkedAgo(p)} · ${esc(agentName(harness))} stopped${where} · Resume picks up right here</div>`;
  return `<div class="active-pane parked">
    <div class="ap-head">${projBtn(p.project, "ap")}${badge(harness)}<span class="proj">${esc(name)}</span>${machTag(p.machine)}<span class="pill soft park-pill">${w ? "resuming…" : `${closed ? "" : P_SIGN}${p.kind} · ${now() - p.at < 60000 ? "now" : ago(p.at)}`}</span>${closed ? "" : starBtn(sid)}<span class="grow"></span>
      ${closed ? `<button class="btn" data-park-closed="${esc(sid)}" ${w ? "disabled" : ""}>Park</button>` : `<button class="btn" data-unpark="${esc(sid)}" ${w ? "disabled" : ""}>Unpark</button>`}
      <button class="btn primary" data-wake="${esc(sid)}" ${w ? "disabled" : ""}>${w ? "Resuming…" : "Resume"}</button></div>
    ${subHead(p.cwd, "", "", "")}
    ${chatBox(`p|${sid}`, `${more}${rows || `<div class="dim cv-empty" data-key="empty">Cue kept no messages from it.</div>`}${line}${w ? "" : resumeCost(sid)}`)}
    <div class="foot">${box(`s:${sid}`, `Message ${name}… sending resumes it first`, w ? "Resuming…" : "Resume & send", `data-wake-send="${esc(sid)}"`, false)}</div></div>`;
}

/** Over a restarting session's chat and box: its steps (stop, start, load; hub/mod.rs Restart), the one it's on. */
function restartCover(rs, name) {
  const steps = [["stop", "Stop"], ["start", "Start"], ["load", !rs.mcp ? "Load skills and settings" : rs.servers == null ? "Load MCP servers" : `Load ${rs.servers} MCP server${rs.servers === 1 ? "" : "s"}`]];
  const at = steps.findIndex(([k]) => k === rs.step);
  return `<div class="rs-cover"><div class="rs-card" role="status"><div class="rs-title">Restarting ${esc(name)}</div><div class="rs-bar"><i></i></div>
    <div class="rs-steps">${steps.map(([, l], n) => `<span class="${n < at ? "done" : n === at ? "now" : ""}">${n < at ? "✓ " : ""}${esc(l)}</span>`).join("")}</div>
    <div class="rs-note">Usually a few seconds. The conversation carries on where it was.</div></div></div>`;
}

/** "Tell me", beside Send: told when this turn is done, even with Cue in front. It shows once the turn
 *  has run 30 seconds (most are done by then), or while it waits on its background agents (its turn goes
 *  on when they report), and stays pressed until the turn ends. */
const TELL_AFTER_MS = 30000;
const atWork = (s) => s?.state === "working" || s?.state === "agent";
function tellBtn(s) {
  if (!atWork(s) || !(s.watch_ms || s.state === "agent" || Date.now() - s.since_ms > TELL_AFTER_MS)) return "";
  const on = !!s.watch_ms;
  return `<button class="btn tell-btn ${on ? "on" : ""}" data-act="watch" data-sid="${esc(s.session_id)}" data-on="${on ? 0 : 1}" aria-pressed="${on}">${tadaIcon(14, on)}${on ? "Telling you" : "Tell me"}</button>`;
}
/** Where the project's dot is, while you're waiting to be told: the tada, in the project's color. */
const watchMark = (project) => `<span class="watch-mark" style="${projStyle(project)}" role="img" aria-label="Telling you when this turn is done">${tadaIcon(14, true)}</span>`;

function activePane() {
  if (quietOpen) return quietPane();
  const pk = parkedActive();
  if (pk) return parkedPane(pk);
  const cur = current();
  if (!cur && state.connections && !state.connections.claude?.ok) return `<div class="active-pane empty"><div class="quiet-big">Connect Cue to Claude Code</div><div class="dim">Cue adds its hooks to Claude Code's settings (backed up first), then sessions that need you show up here.</div><button class="btn primary" data-connect="claude">Connect Claude Code</button></div>`;
  if (!cur) return `<div class="active-pane empty"><div class="quiet-big">Nothing waiting.</div><div class="dim">Pick anything on the right to read it or message the session.</div></div>`;
  const { it, s } = cur;
  const pending = isPending(it);
  const harness = it?.harness || s?.harness;
  const project = it?.project || s?.project;
  const sid = s?.session_id || it?.session_id;
  noteLooked(sid);
  // A finished turn that changed files: the pill says what changed ("4 files · +98 −9"; hover for which), and Commit sits beside it.
  const ch = pending && it.kind === "waiting" && !it.interrupted ? s?.changes : null;
  const turnWord = ch ? `${ch.files.length} file${ch.files.length === 1 ? "" : "s"} · +${ch.add} −${ch.del}` : it?.interrupted ? "interrupted" : "your turn";
  const pill = pending
    ? `<span class="pill ${it.interrupted ? "intr" : ""}">${it.kind === "waiting" ? turnWord : it.kind === "question" ? "asks you" : inTerminal(it) ? "asks in its terminal" : "needs a decision"} · ${ago(it.created_ms)}${it.kind === "waiting" ? `<button class="pill-x" data-act="dismiss" data-id="${esc(it.id)}" title="Take it off Waiting" aria-label="Take it off Waiting">×</button>` : ""}</span>`
    : `<span class="pill soft">${s ? { working: "working", waiting: "your turn", deciding: "deciding", agent: s.helpers ? "on its agents" : "on its lead", stopped: "stopped", limited: s.limit && lifted(s.limit) ? "usage is back" : "out of usage" }[s.state] || s.state : "answered"}</span>`;
  let foot;
  if (pending && it.kind !== "waiting") {
    // In its terminal: what Claude Code said ("A sandboxed command needs network access"), then the call it's about.
    const head = (inTerminal(it) ? esc(it.message || `${agentName(harness)} ${verb(it)}`) : `${esc(agentName(harness))} ${esc(verb(it))}`) + machTag(it.machine);
    foot = `<div class="act-card"><div class="act-head">${head}</div>${inTerminal(it) && !it.tool_name ? "" : requestBody(it)}${it.kind !== "question" ? whyLine(it) : ""}${decisionButtons(it)}</div>`;
    // You were typing to this session when it asked: your box stays (cursor and text intact),
    // and sending waits until you've answered, so nothing gets typed into its prompt.
    const key = `s:${sid}`;
    if (document.activeElement?.dataset?.text === key || draft(key).text.trim() || draft(key).images.length) {
      foot += `<div class="foot held">${box_(key, "Answer above first; your message waits here…", "Answer first", `data-held="1" disabled`)}</div>`;
    }
  } else if (pending) {
    foot = `<div class="foot">${field(it, `Reply to ${project} (typed into its terminal)…`, "Send")}</div>`;
  } else if (s) {
    const busy = s.state === "working";
    // Working: what you send waits for its current step (Queue, ↵), or stops it and goes now (Now, ⌘↵):
    // with something typed, both are there by the box, so you see you have the choice.
    // Pi takes it straight into its queue (read at its next step); for the others Cue keeps it until the turn ends.
    const after = harness === "pi" ? "it reads this after its current step" : "Cue keeps it and sends it when this turn ends";
    const now = busy ? `<button class="btn send-now" data-act="send-to" data-now="1" data-sid="${esc(sid)}">Now <kbd>⌘↵</kbd></button>` : "";
    foot = `${queueNote(sid, s)}<div class="foot">${box_(`s:${sid}`, busy ? `Message ${nameOf(sid, project)}… ${after}` : `Message ${project}…`, busy ? "Queue" : "Send", `data-act="send-to" data-sid="${esc(sid)}"`, now, tellBtn(s))}</div>`;
  } else foot = "";
  const cm = crewOf(sid);
  // A background agent opened in full (helperPane): it takes the chat area and the box, under the same head.
  const hv = sid ? helperViewed(sid) : null, hvp = hv && helperPane(sid, hv);
  // A reply open in the reader (readerPane): the same place, read-only, the box and insight bar out of the way.
  const rv = sid && !hvp ? readerViewed(sid, it, s) : null, rvp = rv && readerPane(sid, it, s, harness, rv);
  const full = hvp ? { ...hvp, forKey: `h|${sid}|${hv.id}` } : rvp ? { ...rvp, forKey: `r|${sid}|${rv.all[rv.at].key}` } : null;
  // The badge already says which agent: no "Claude Code" beside it. An executor's relay name and a crew's model stay.
  const who = cm?.role === "executor" ? `${esc(cm.name)}${cm.model ? ` · ${esc(shortModel(cm.model))}` : ""}` : cm?.role === "lead" && cm.model ? esc(shortModel(cm.model)) : "";
  const onMachine = machTag(s?.machine || it?.machine);
  // Restarting (after an MCP change): covered until it's back, with nothing to press meanwhile.
  const rs = sid && state.restarting?.[sid];
  if (rs) return `<div class="active-pane restarting">
    <div class="ap-head">${projBtn(project, "ap")}${badge(harness)}${nameHead(sid, project)}${roleTag(sid, true)}${onMachine}<span class="pill soft">restarting</span><span class="grow"></span></div>
    ${subHead(s?.cwd || it?.cwd, "", sid)}
    ${chatBox(`m|${sid}`, chatHtml(it, s, harness, true))}${foot}${restartCover(rs, nameOf(sid, project))}</div>`;
  // A reply in the reader: the pane is the reader's (its bar names the session), the box out of the way.
  if (rvp) return `<div class="active-pane reading ${s?.state === "working" || s?.compacting_ms ? "busy" : ""}">${sweep(sid, s)}
    ${full.bar + chatBox(full.forKey, full.body)}${extSlot(sid)}${full.foot}${continueCover(sid)}</div>`;
  // At work (a turn, or compacting): a light sweeps along the top edge, as in its terminal tab. Not while it waits.
  return `<div class="active-pane ${s?.state === "working" || s?.compacting_ms ? "busy" : ""}">${sweep(sid, s)}
    <div class="ap-head">${s?.watch_ms ? watchMark(project) : projBtn(project, "ap")}${badge(harness)}${nameHead(sid, project)}${roleTag(sid, true)}${who ? `<span class="dim">${who}</span>` : ""}${onMachine}${pill}${ch ? `<button class="btn small commit-btn" data-act="commit" data-id="${esc(it.id)}">Commit</button>` : ""}<span class="grow"></span>
      ${sid ? moreMenu(sid, s, svClosable({ sid, st: pending && it.kind !== "waiting" ? "asks" : s?.state || "idle" })) : ""}
      ${s && harness === "claude" ? `<button class="btn btw-btn ${btwFor === sid ? "on" : ""}" data-act="btw" data-sid="${esc(sid)}">btw</button>` : ""}
      ${s?.state === "working" ? `<button class="btn deny" data-act="interrupt" data-sid="${esc(sid)}" title="Stop it mid-turn (Esc twice)">Stop</button>` : ""}
      ${sid && svClosable({ sid, st: pending && it.kind !== "waiting" ? "asks" : s?.state || "idle" }) ? closeBtn(sid) : ""}${focus && s?.state !== "working" ? `<span class="esc-hint">esc</span>` : ""}</div>
    ${btwPanel(sid)}
    ${subHead(s?.cwd || it?.cwd, "", sid)}
    ${sid ? teamBar(sid) + teammateBar(sid) + leadCard(sid) + packetCard(sid) : ""}
    ${sid ? `<div class="ap-bar">${barLabels(sid)}${bar(sid)}${barKey(sid)}</div>` : ""}
    ${sessTabs(sid)}
    ${full ? full.bar + chatBox(full.forKey, full.body) : chatBox(`m|${sid || it?.id}`, chatHtml(it, s, harness, true) + keyed(renameNotice(s), "rename") + keyed(statusLine(it, s), "status"))}${extSlot(sid)}${actSlot(sid)}
    ${"" /* The Next bar, hidden for now (it read as part of the terminal tabs and the box; N still works): pending ? "" : nextBar() */}
    ${sid && s && !full ? catchUpBar(sid, it, s) || handoffInsight(sid, s) : ""}${termDock(sid)}${full ? full.foot : listenLine(sid) + foot}${sid ? continueCover(sid) : ""}</div>`;
}
// ---------- MCP servers: ⋯ → MCP servers… ----------
// Every server Claude Code knows for the session's folder (mcp_for), each with a checkbox: checked =
// it runs with it (`selected`). Restart session with these pins the session to the checked ones (mcp_apply: Cue
// restarts it with them, conversation kept, and every later resume keeps them); "Back to the folder's
// servers" drops that (mcp_apply_clear). The + New session form shows the same list for the folder typed.
const mcpCache = new Map();   // sid or "f:<folder>" → { at, data, loading, err }
const MCP_FRESH = 30000;
function mcpLoad(sid, force = false) {
  const c = mcpCache.get(sid) || { at: 0, data: null, loading: false };
  mcpCache.set(sid, c);
  if (c.loading || (!force && now() - c.at < MCP_FRESH)) return;
  c.loading = true;
  // A key "f:<folder>" is a folder with no session yet (the + New session form): its config alone.
  (sid.startsWith("f:") ? invoke("mcp_for_folder", { cwd: sid.slice(2) }) : invoke("mcp_for", { sessionId: sid })).then((v) => { c.data = v || { servers: [] }; c.err = ""; c.at = now(); c.loading = false; renderMain(); }, (x) => { c.loading = false; c.at = now(); c.err = String(x || "couldn't read them"); renderMain(); });
}
let mcpPicking = false;      // the ⋯ menu shows the MCP picker instead of its items
const mcpPick = new Map();   // key → Set of checked names, once you've changed a box (else what it has now)
const mcpToolsOpen = new Set();   // "<key>|<server>": its tools unfolded in the list
const mcpPickable = (m) => m.scope !== "session";   // any server with a definition can be passed; Claude Code's own can't
function mcpChecked(key) {
  if (mcpPick.has(key)) return mcpPick.get(key);
  const servers = mcpCache.get(key)?.data?.servers || [];
  return new Set(servers.filter((m) => mcpPickable(m) && (m.selected ?? m.enabled)).map((m) => m.name));
}
/** How many it runs with, for the ⋯ item (null until read). */
function mcpOnCount(sid) {
  const d = mcpCache.get(sid)?.data;
  return d ? (d.servers || []).filter((m) => mcpPickable(m) && (m.selected ?? m.enabled)).length : null;
}
/** Where a server comes from, in a word or two. */
function mcpScopeWord(m) {
  if (m.scope === "user") return "always";
  if (m.scope === "local") return "this folder";
  if (m.scope === "project") return "project";
  if (m.scope === "session") return "Claude Code's";
  const from = String(m.from || "");
  if (/\.claude\.json$/.test(from)) return "this folder";   // turned off here, kept by Cue
  if (/\/mcp\/[^/]+\.json$/.test(from) && !/\.mcp\.json$/.test(from)) return "picked before";
  const dir = from.replace(/ \(local\)$/, "").replace(/\/\.mcp\.json$/, "").replace(/\/+$/, "");
  return `from ${dir.split("/").pop() || dir}`;
}
/** The checklist: pickable servers first, then Claude Code's own (shown, not checkable). */
function mcpList(key, compact = false) {
  mcpLoad(key);
  const c = mcpCache.get(key);
  if (!c?.data) return `<div class="mcp-none">${c?.err ? esc(`Couldn't read them: ${c.err}`) : "Reading…"}</div>`;
  const servers = c.data.servers || [];
  const on = mcpChecked(key);
  const item = (m) => {
    const can = mcpPickable(m);
    const n = m.used ? `<span class="mcp-n ${m.failed > 0 ? "bad" : ""}">· ${m.used} call${m.used === 1 ? "" : "s"}</span>` : "";
    // The tools the running session loaded from it, by name (logs/mcp.rs with_live), with the calls it
    // made to each: a click unfolds them.
    const tools = m.loaded || [], open = mcpToolsOpen.has(`${key}|${m.name}`);
    const calls = (t) => (m.tools || []).find((u) => u.name === t)?.calls || 0;
    const fold = tools.length ? `<button class="mcp-tfold" data-mcp="tools" data-sid="${esc(key)}" data-name="${esc(m.name)}" aria-expanded="${open}">· ${tools.length} tool${tools.length === 1 ? "" : "s"} ${open ? "▴" : "▾"}</button>` : "";
    const list = open && tools.length ? `<div class="mcp-tools mono">${tools.map((t) => `<span>${esc(t)}${calls(t) ? `<span class="mcp-tn"> · ${calls(t)}</span>` : ""}</span>`).join("")}</div>` : "";
    return `<label class="mcp-it ${can ? "" : "fixed"}"><input type="checkbox" data-mcp-pick="${esc(key)}" value="${esc(m.name)}" ${(can ? on.has(m.name) : m.selected ?? m.enabled) ? "checked" : ""} ${can ? "" : "disabled"}/><span class="mcp-nm">${esc(m.name)}</span><span class="mcp-sc">${esc(mcpScopeWord(m))}</span>${mcpLiveWord(m)}${n}${fold}</label>${list}`;
  };
  const mine = servers.filter(mcpPickable), theirs = servers.filter((m) => !mcpPickable(m));
  if (!mine.length && !theirs.length) return `<div class="mcp-none">No MCP servers are set up in Claude Code</div>`;
  return `<div class="mcp-list ${compact ? "compact" : ""}">${mine.map(item).join("")}${theirs.length ? `<div class="mcp-sub">Claude Code adds these itself</div>${theirs.map(item).join("")}` : ""}</div>`;
}
/** What the running session really has from a server (logs/mcp.rs with_live), so a tick never passes
 *  for working: connected, waiting for sign-in, connecting, failed, or (one it should have) not loaded. */
function mcpLiveWord(m) {
  const w = { on: ["connected", "ok"], "sign-in": ["needs sign-in: /mcp in its terminal", "warn"], connecting: ["connecting", ""], failed: ["didn't connect", "bad"] }[m.live]
    || (m.live === "off" && (m.selected ?? m.enabled) ? ["not loaded", "bad"] : null);
  return w ? `<span class="mcp-live ${w[1]}">· ${w[0]}</span>` : "";
}
/** ⋯ → MCP servers…: the checklist, Restart session with these, Cancel, and (pinned) back to the folder's. */
function mcpPicker(sid) {
  const busy = sessionOf(sid)?.state === "working";
  const d = mcpCache.get(sid)?.data;
  const back = d?.pinned ? `<button class="mcp-back" data-mcp="clear" data-sid="${esc(sid)}" ${busy ? "disabled" : ""}>Back to the folder's servers</button>` : "";
  // Claude Code only records what connected once it's had a turn: just restarted, it can't be told yet.
  const unknown = d?.running && !d.live_known ? `<div class="mcp-note">Whether each one connected shows after its next message</div>` : "";
  return `<div class="more-menu mcp-pick"><div class="pm-title">MCP servers</div>${mcpList(sid)}${unknown}
    ${busy ? `<div class="mcp-note">It's working; apply when it's idle</div>` : `<div class="mcp-note">Only this session restarts, with the ones checked; its conversation stays</div>`}
    <div class="mcp-acts">${back}<span class="grow"></span><button class="btn small" data-mcp="cancel">Cancel</button><button class="btn small primary" data-mcp="apply" data-sid="${esc(sid)}" ${busy || !d ? "disabled" : ""}>Restart session with these</button></div></div>`;
}
function mcpClose(sid) { moreFor = handoffArm = null; mcpPicking = false; if (sid) mcpPick.delete(sid); }
document.addEventListener("change", (e) => {
  const cb = e.target.closest?.("[data-mcp-pick]");
  if (!cb) return;
  const key = cb.dataset.mcpPick;
  const set = new Set(mcpChecked(key));
  cb.checked ? set.add(cb.value) : set.delete(cb.value);
  mcpPick.set(key, set);
});
document.addEventListener("click", async (e) => {
  const ask = e.target.closest("[data-mcp-ask]");
  if (ask) { mcpPicking = true; mcpPick.delete(ask.dataset.mcpAsk); mcpLoad(ask.dataset.mcpAsk, true); return renderMain(); }
  const el = e.target.closest("[data-mcp]");
  if (!el) return;
  const k = el.dataset.mcp, sid = el.dataset.sid;
  if (k === "cancel") { mcpClose(moreFor); return renderMain(); }
  if (k === "tools") { e.preventDefault(); const t = `${sid}|${el.dataset.name}`; mcpToolsOpen.has(t) ? mcpToolsOpen.delete(t) : mcpToolsOpen.add(t); return renderMain(); }
  if (k === "apply" || k === "clear") {
    const names = [...mcpChecked(sid)];
    mcpClose(sid);
    renderMain();
    try { toast(await (k === "apply" ? invoke("mcp_apply", { sessionId: sid, names }) : invoke("mcp_apply_clear", { sessionId: sid }))); }
    catch (x) { toast(`Couldn't: ${x}`); }
    return mcpLoad(sid, true);
  }
});
// ---------- the chat's DOM lives on between draws ----------
// renderMain rebuilds the whole window from a string; the chat (the Active pane's `.ap-chat`) is the one
// part that would show it: steps land every half second while a session works, and a rebuilt chat
// flickers, starts every fade-in over, drops your text selection and loses its scroll. So the chat's
// element is kept: while the same chat stays open, activePane() leaves a slot where it goes, settleChat
// puts the live element back into that slot, and patchChat brings it up to date by key: a row or step
// whose key is new goes in (in order), one whose HTML changed is updated in place, the rest are left
// alone. The whole chat is drawn afresh only when another chat opens (or the steps feed shrank: the
// transcript was replaced).
// Setting the window's HTML would take the chat out of the document, and an element put back after
// that starts its animations over and loses the selection in it. So liftChat moves it (and the sweep)
// aside first, with moveBefore where the engine has it: a move that keeps the element's state. Without
// it the element is kept but a fade-in mid-way would replay, so there the `reveal` of earlier lines is
// taken off at the next draw. A selection in the chat collapses on either move, so it's noted and put back.
const KEEPS_STATE = typeof Element.prototype.moveBefore === "function";
const moveTo = (parent, node, before = null) => (KEEPS_STATE ? parent.moveBefore(node, before) : parent.insertBefore(node, before));
let chatPending = null;   // what the slot stands for: the chat's HTML, until settleChat patches it in
/** Before the window's HTML is set: the chat, the sweep and a flick move aside, out of what's replaced. */
function liftChat() {
  const kept = ["[data-chat]", "[data-sweep]", "[data-flick]"].map((q) => document.querySelector(q)).filter(Boolean);
  const chat = kept.find((el) => el.matches("[data-chat]"));
  const top = chat?.scrollTop;
  const sel = getSelection();
  const picked = chat && sel.rangeCount && !sel.isCollapsed && chat.contains(sel.getRangeAt(0).commonAncestorContainer) ? [sel.anchorNode, sel.anchorOffset, sel.focusNode, sel.focusOffset] : null;
  for (const el of kept) moveTo(document.body, el);
  return { kept, top, picked };
}
/** The `.ap-chat` for a pane: the real thing on a full draw, else a slot for the element that's there. */
function chatBox(forKey, html) {
  const old = document.querySelector("[data-chat]");
  const turns = stepFeeds.get(forKey.slice(2))?.turns.length ?? 0;
  if (old && old.dataset.chatFor === forKey && turns >= +(old.dataset.turns || 0)) {
    chatPending = { forKey, html, turns };
    return `<div class="ap-chat" data-chat data-chat-slot></div>`;
  }
  chatPending = null;
  const f = stepFeeds.get(forKey.slice(2));
  if (f) f.fresh = false;   // whatever is in this draw was there when the chat was opened
  return `<div class="ap-chat" data-chat data-chat-for="${esc(forKey)}" data-turns="${turns}">${html}</div>`;
}
/** After renderMain set the window's HTML: the live chat (and the working sweep over it) go back where
 *  the slot is, then the chat is patched. `old`: the chat element from before the draw. Returns true when
 *  the chat was kept, false when this draw made a new one. */
function settleChat({ kept, top, picked }) {
  const slot = document.querySelector("[data-chat-slot]");
  const old = kept.find((el) => el.matches("[data-chat]"));
  if (!slot || !old || !chatPending) { chatPending = null; kept.forEach((el) => el.remove()); return false; }
  const { forKey, html, turns } = chatPending;
  chatPending = null;
  const pane = slot.closest(".active-pane");
  moveTo(slot.parentNode, old, slot);
  slot.remove();
  if (top != null && old.scrollTop !== top) old.scrollTop = top;
  // The sweep carries on where it was (its class and speed follow the new draw); so does a flick mid-pass.
  const sw = pane?.querySelector("[data-sweep]"), oldSw = kept.find((el) => el.matches("[data-sweep]"));
  if (sw && oldSw) { oldSw.className = sw.className; oldSw.style.cssText = sw.style.cssText; moveTo(sw.parentNode, oldSw, sw); sw.remove(); }
  else oldSw?.remove();
  const oldFlick = kept.find((el) => el.matches("[data-flick]"));
  if (sw && oldFlick) moveTo(pane, oldFlick, (pane.querySelector("[data-sweep]") || pane.firstChild).nextSibling);
  else oldFlick?.remove();
  if (!KEEPS_STATE) old.querySelectorAll(".reveal").forEach((el) => el.classList.remove("reveal"));
  const f = stepFeeds.get(forKey.slice(2));
  const fresh = !!f?.fresh;
  if (f) f.fresh = false;
  const tpl = document.createElement("template");
  tpl.innerHTML = html;
  const landed = patchKids(old, tpl.content, !fresh);
  old.dataset.turns = turns;
  if (landed && sw) flick(pane);
  // What you had selected, unless the patch replaced it.
  if (picked && picked[0].isConnected && picked[2].isConnected) try { getSelection().setBaseAndExtent(...picked); } catch {}
  return true;
}
/** A step just landed: one quick flick along the top edge, over the sweep. It outlives the draws that
 *  come while it plays (settleChat carries it over). */
function flick(pane) {
  pane.querySelector("[data-flick]")?.remove();
  const el = document.createElement("i");
  el.className = "sweep flick";
  el.dataset.flick = "1";
  el.addEventListener("animationend", () => el.remove());
  (pane.querySelector("[data-sweep]") || pane).after(el);
}
/** The chat's rows (or a turn's items) brought up to date, by key. A key not in the DOM goes in where it
 *  comes in order; one whose HTML differs from the last draw's is updated in place (morph); one that's
 *  gone is removed; the rest are left exactly as they are. A `.stx` block and a run of steps inside it
 *  are patched the same way, one level down, so a landing step only adds its own line. `reveal`: a new
 *  step or paragraph fades in. Returns how many step lines went in. */
const PATCH_INSIDE = ".stx, .stx-steps";
function patchKids(live, fresh, reveal) {
  const want = new Set(), old = new Map();
  for (const c of fresh.children) want.add(c.dataset.key);
  for (const c of [...live.children]) { const k = c.dataset.key; if (k == null || old.has(k) || !want.has(k)) c.remove(); else old.set(k, c); }
  let landed = 0, cursor = live.firstElementChild;
  const seen = new Set();
  for (const f of [...fresh.children]) {
    const k = f.dataset.key, h = f.outerHTML;
    const node = k != null && !seen.has(k) ? old.get(k) : null;
    if (!node) {
      live.insertBefore(f, cursor);
      f._html = h;
      const lines = [...f.querySelectorAll(".stx-line"), ...(f.matches(".stx-line") ? [f] : [])];
      landed += lines.length;
      if (reveal) for (const el of [...lines, ...f.querySelectorAll(".stx-say"), ...(f.matches(".stx-say") ? [f] : [])]) el.classList.add("reveal");
      continue;
    }
    seen.add(k);
    if (node === cursor) cursor = cursor.nextElementSibling;
    else live.insertBefore(node, cursor);   // out of order (rare: rows sort by time)
    if (node._html === h) continue;
    node._html = h;
    if (node.matches(PATCH_INSIDE) && f.matches(PATCH_INSIDE)) { morphAttrs(node, f); landed += patchKids(node, f, reveal); }
    else morph(node, f);
  }
  return landed;
}
/** `a` made to match `b` in place where their shapes agree (same tags, same number of children: the text
 *  and attributes are set), replaced where they don't. A line mid-fade-in keeps its `reveal`. */
function morph(a, b) {
  if (a.nodeType !== b.nodeType || a.nodeName !== b.nodeName) { a.replaceWith(b); return; }
  if (a.nodeType !== 1) { if (a.nodeValue !== b.nodeValue) a.nodeValue = b.nodeValue; return; }
  morphAttrs(a, b);
  if (a.childNodes.length !== b.childNodes.length) { a.replaceChildren(...b.childNodes); return; }
  const bc = [...b.childNodes];
  [...a.childNodes].forEach((c, i) => morph(c, bc[i]));
}
function morphAttrs(a, b) {
  if (a.classList.contains("reveal")) b.classList.add("reveal");
  for (const { name, value } of [...b.attributes]) if (a.getAttribute(name) !== value) a.setAttribute(name, value);
  for (const { name } of [...a.attributes]) if (!b.hasAttribute(name)) a.removeAttribute(name);
}
let escArmed = 0;           // when Esc was last pressed on a working session (twice within 1.5s stops it)
let focusedTarget = null;   // the Active target whose text box last got the cursor
let overWas = false;        // something covered the chat at the last draw (a sheet, the live list, an image…)
/** Put the cursor at the end of the Active pane's text box (opening something, Esc Esc). */
function focusComposer() {
  const el = document.querySelector(".active-pane textarea[data-text]");
  if (!el) return false;
  el.focus();
  el.setSelectionRange(el.value.length, el.value.length);
  return true;
}

/** Focus that has fallen to nothing goes home, to the open session's message box: what had it went away
 *  (a terminal tab you closed, a terminal put away) or let it go. Never over a choice of yours: not
 *  while something sits over the chat (a sheet, a menu, a dialog), not when Cue isn't the app you're in,
 *  and not after a click, where you picked where to be (and may be selecting text, which focusing the
 *  box would clear). A frame later, so whatever takes focus next has done so first. */
let pointedAt = 0;
document.addEventListener("pointerdown", () => { pointedAt = Date.now(); }, true);
// The tray's fold and an agent's "Put reply in the box": theirs alone, so they're handled here.
document.addEventListener("click", (e) => {
  const tf = e.target.closest("[data-tray-fold]");
  if (tf) { const k = tf.dataset.trayFold; trayFolded.has(k) ? trayFolded.delete(k) : trayFolded.add(k); return renderMain(); }
  const hr = e.target.closest("[data-helper-reply]");
  if (hr) { const [sid, id] = hr.dataset.helperReply.split("|"); helperReplyToBox(sid, id); }
});
function homeFocus() {
  requestAnimationFrame(() => {
    const a = document.activeElement;
    if ((a && a !== document.body) || !document.hasFocus() || Date.now() - pointedAt < 500) return;
    if (sheet || lightbox || liveOpen || menuFor || redirectFor || renaming || btwFor || deleteAsk || moving) return;
    focusComposer();
  });
}
// Focus let go to nothing (Tab past the last control, an element that blurs as it closes): home.
document.addEventListener("focusout", (e) => { if (!e.relatedTarget) homeFocus(); });

/** Text boxes grow with their content, up to ~8 lines, then scroll. */
const BOX_MAX = 180;   // the text box's tallest (px); past it, it scrolls (style.css says the same)
function grow(el) {
  // Already as tall as it gets and still overflowing: nothing to measure. (Measuring collapses it for a
  // moment, which moved what you were reading and jolted the chat above on every key in a long message.)
  if (el.clientHeight >= BOX_MAX - 1 && el.scrollHeight > el.clientHeight) {
    if (el.selectionEnd >= el.value.length) el.scrollTop = el.scrollHeight;   // typing at the end: the end in view
    return;
  }
  // Growing the box shrinks the chat above it. If you were reading the end of the chat, keep the
  // end in view (the conversation moves up with the box) instead of letting the box cover it.
  const chat = el.closest(".active-pane")?.querySelector("[data-chat]");
  const pinned = chat && chat.scrollHeight - chat.scrollTop - chat.clientHeight < 24;
  const top = el.scrollTop, atEnd = el.selectionEnd >= el.value.length;
  el.style.height = "auto";
  el.style.height = `${Math.min(el.scrollHeight, BOX_MAX)}px`;
  // The moment collapsed resets where it was scrolled (WebKit doesn't always put it back): with the cursor
  // at the end, the end stays in view; else where you were.
  el.scrollTop = atEnd ? el.scrollHeight : top;
  if (pinned) chat.scrollTop = chat.scrollHeight;
}
const picker = Object.assign(document.createElement("input"), { type: "file", accept: "image/*", multiple: true, hidden: true });
async function attach(key, files) {
  for (const f of files.slice(0, 6)) {
    if (f.size > 15 * 1024 * 1024) { toast(`${f.name} is over 15 MB`); continue; }
    const data = await new Promise((ok) => { const r = new FileReader(); r.onload = () => ok(r.result); r.readAsDataURL(f); });
    draft(key).images.push({ name: f.name || "pasted image", mime: f.type, data });
  }
  renderMain();
  document.querySelector(`[data-text="${CSS.escape(key)}"]`)?.focus();
}
