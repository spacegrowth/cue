// Cue UI chat — the reply box: sending, dictation, images, the "/" menu, recall and quick phrases.
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
    // Several questions: your typed answer is for the one whose tab is open (stepOf), and its tab stays open (#461);
    // it goes with the others when you press Send.
    const cur = stepOf(it), q = qs[cur], d = qDraft(it);
    d.choices[q.question] = q.multiSelect ? new Set([text]) : text;
    qStep[it.id] = cur;
    d.text = "";
    redirectFor = null;
    return renderMain();
  }
  return deny(it, text);
}
/** Say something to a live session from the Active pane (it may be mid-turn). */
// now (Ctrl+Enter): if it's mid-turn, stop it first so this is handled right away.
/** How the last message to `sid` went ("kept in Cue"), forgotten: it goes another way now. */
const forgetSent = (sid) => { if (lastSent?.sid === sid) lastSent = null; };
/** Send now (the button on a queued message, or ⌘↵ with nothing new typed): stopping the turn makes the
 *  agent read what you queued straight away. */
function sendQueuedNow(sid) {
  invoke("send_queued_now", { sessionId: sid })
    .then(() => toast(`Sent now: ${nameOf(sid, sessionOf(sid)?.project || "it")} stopped its current turn and reads your message next`))
    .catch((e) => toast(`Couldn't send it now: ${e}`));
}
/** Send now on the message Cue kept for a working session (its button, or ⌘↵ as for a queued one): it
 *  stops the turn and the message goes straight away. */
const sendHeldNow = (sid) => forgetSent(sid) || invoke("send_held_now", { sessionId: sid }).then(() => toast("Sent now: it stopped its turn and reads your message next")).catch((e) => toast(`Couldn't send it now: ${e}`));
/** ⌘↵ with nothing new typed: what's waiting to go (queued in its agent, or kept in Cue) goes now. False when there's nothing. */
function sendWaitingNow(sid) {
  const s = sessionOf(sid);
  if (s?.queued) sendQueuedNow(sid);
  else if (s?.held) sendHeldNow(sid);
  else return false;
  return true;
}
/** `key`: the draft that goes (the session's box, or one of its own). */
const sendTo = (sid, now = false, key = `s:${sid}`) => deliver(sid, key, boxToSession(sid, { now }), true);
/** The message Cue kept for a working session, back into its box to change (before anything you've typed
 *  there since), images and all. Your words only: the Desk's summary Cue put before a message to a Desk's lead
 *  (desk/hub/lead/send.rs desk_message) isn't yours, and would go twice if sent again (#498). */
async function pullHeld(sid, key = `s:${sid}`) {
  forgetSent(sid);
  const h = await invoke("unhold_message", { sessionId: sid }).catch(() => null);
  if (!h) return;
  // Taken back: this window's copy says so now, so an Esc before Cue's next update arms Stop rather than pulling again.
  const s = sessionOf(sid);
  if (s) delete s.held;
  boxPutBack(key, deskSaid(h.text), await boxHeldImages(h.images), { imagesLast: true, focus: focusComposer });
}
/** Cancel on the message Cue kept for a working session: dropped, never sent (taken back as Edit does, not into the box). */
const cancelHeld = (sid) => forgetSent(sid) || invoke("unhold_message", { sessionId: sid }).then((h) => toast(h ? "It won't be sent" : "Too late: it had already gone")).catch((e) => toast(`Couldn't cancel it: ${e}`));
/** Stop a working session mid-turn (Esc in its terminal; Pi aborts directly). */
async function interrupt(sid) {
  try { toast(`Stopped ${sessionOf(sid)?.project || "it"}: ${(await invoke("interrupt_session", { sessionId: sid })).replace(/^stopped via /, "via ")}`); }
  catch (e) { toast(`Couldn't stop it: ${e}`); }
}
const box_ = (key, placeholder, label, attrs, alt = "", extra = "", quick = true) => box(key, placeholder, label, attrs, true, alt, extra, quick);

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

/** A picture opened full size (← → between its set), over the window; "" when none is. */
const lightboxHtml = () => !lightbox ? "" : `<div class="lightbox" data-act="close-lightbox"><img src="${esc(lightbox.srcs[lightbox.i])}" alt=""/>${lightbox.srcs.length > 1 ? `<div class="lb-count">${lightbox.i + 1} / ${lightbox.srcs.length} · ← →</div>` : ""}</div>`;

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
      if (list && JSON.stringify(list) !== JSON.stringify(cmdLists.get(sid))) { cmdLists.set(sid, list); cmdMissed.delete(sid); refreshCmdOf(sid); }
      if (list && !reading) return;
      if (n < 40) setTimeout(() => ask(n + 1), 600); else if (!list) missed();
    }).catch(missed);
    ask(0);
  }
  const list = cmdLists.get(sid) || [];
  // Cue's own: a side question, answered in a panel (Claude Code's /btw answers only in its terminal).
  // A hosted lead's /team (team_send.js).
  return sessionOf(sid)?.harness === "claude" ? [BTW_CMD, ...teamCmds(sid, list), ...list.filter((c) => c.name !== "btw")] : list;
}
/** The session box `key` writes to: a session's own box ("s:<sid>"), or a box of the window's own drawn as one (BOX_OWNERS
 *  sid: the Desk lead's, desk/view.js), so its "/" menu, Send at and suggestion are that session's. "" for none. */
const sidOfKey = (key) => {
  if (key.startsWith("s:")) return key.slice(2);
  for (const o of BOX_OWNERS) { const sid = o.sid?.(key); if (sid) return sid; }
  return "";
};
// ---------- ↑ / ↓ in a session's box: the last messages you sent it, like a shell's history ----------
// ↑ on the box's first line puts the message before into the box (older with each press, the last ten);
// ↓ on the last line comes forward again and ends on what you were typing. Typing, or sending, starts over.
const RECALL = 10;
let recall = null;   // { key, list, idx, typed, set }: where you are in the list, and the text the last step put in
/** Is the caret on the box's first shown row (up) or its last (down)? Rows as wrapped on screen, not "\n"s:
 *  a copy of the box, laid out the same, says which row the caret sits on. */
function onEdgeRow(ta, up) {
  const v = ta.value, at = up ? ta.selectionStart : ta.selectionEnd;
  if (up ? at === 0 : at === v.length) return true;
  if (up ? v.lastIndexOf("\n", at - 1) !== -1 : v.indexOf("\n", at) !== -1) return false;
  const cs = getComputedStyle(ta), m = document.createElement("div");
  for (const p of ["fontFamily", "fontSize", "fontWeight", "fontStyle", "letterSpacing", "wordSpacing", "lineHeight", "textTransform", "textIndent", "tabSize", "wordBreak", "overflowWrap"]) m.style[p] = cs[p];
  const width = ta.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
  Object.assign(m.style, { position: "absolute", visibility: "hidden", left: "-9999px", top: "0", boxSizing: "content-box", width: `${width}px`, padding: "0", border: "0", whiteSpace: "pre-wrap" });
  const mark = () => { const s = document.createElement("span"); s.textContent = "​"; return s; };
  const a = mark(), c = mark(), z = mark();
  m.append(a, v.slice(0, at), c, v.slice(at), z);
  document.body.appendChild(m);
  const rowH = parseFloat(cs.lineHeight) || parseFloat(cs.fontSize) * 1.2;
  const same = Math.abs((up ? a : z).offsetTop - c.offsetTop) < rowH / 2;
  m.remove();
  return same;
}
function recallSent(e) {
  const ta = e.target, key = ta.dataset.text;
  // A box of the window's own (box/send.js BOX_OWNERS: a Desk's): what you sent to the session it goes to, as its chat shows it.
  const desk = !key.startsWith("s:") ? boxOwner(key)?.recall?.(key) || null : null;
  const sid = key.startsWith("s:") ? sidOfKey(key) : desk?.s?.session_id;
  if (!sid) return false;
  const up = e.key === "ArrowUp";
  const v = ta.value;
  // Only from the box's first row (↑) or last row (↓), as you see them, wrapped lines too: elsewhere the
  // arrows move the caret as usual.
  if (!onEdgeRow(ta, up)) return false;
  if (!recall || recall.key !== key || recall.set !== v) {
    const cur = desk ? { it: null, s: desk.s } : current();
    if ((cur?.s?.session_id || cur?.it?.session_id) !== sid) return false;
    const seen = new Set();
    const list = conversation(cur.it, cur.s).map(deskSaidE).filter((x) => x.role === "you" && !askLnAnswer(x) && x.text.trim()).map((x) => x.text).filter((t) => !seen.has(t) && seen.add(t)).slice(-RECALL);
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
  if (c?.name !== "model" || c.kind === "cue") return args;
  const big = args.filter((v) => v.value !== "default" && !/\[.*\]$/.test(v.value) && !args.some((w) => w.value === `${v.value}[1m]`));
  return [...big.map((v) => ({ value: `${v.value}[1m]`, label: `${v.label || v.value}, 1M context`, desc: "" })), ...args];
}
function cmdMatches(key) {
  if (cmdShut === key) return [];
  const mates = atMatches(key);
  if (mates.length) return mates;
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
  // Pi's comes from its extension. Codex's model choices come from its app server.
  const h = sessionOf(sidOfKey(key))?.harness;
  const foot = ["claude", "pi", "codex"].includes(h) ? `<div class="cmd-foot"><span>${h === "codex" ? "From Codex" : h === "pi" ? "From Pi" : "Read once, kept a week"}</span><button data-cmd-refresh="${esc(key)}">Refresh</button></div>` : "";
  if (m[0].mate) return `<div class="cmd-menu at-menu" role="listbox">${m.map((c, n) => `<button class="cmd-row ${n === cmdSel ? "on" : ""}" data-cmd-pick="${esc(key)}" data-cmd-name="${esc(c.name)}" role="option"><b>${esc(c.name)}</b><span></span><i class="${c.look}">${esc(c.word)}</i></button>`).join("")}</div>`;
  return `<div class="cmd-menu" role="listbox">${m.map((c, n) => `<button class="cmd-row ${n === cmdSel ? "on" : ""}" data-cmd-pick="${esc(key)}" data-cmd-name="${esc(c.name)}" role="option"><b>/${esc(c.name)}${after(c)}</b>${c.desc ? `<span>${esc(c.desc)}</span>` : ""}${c.kind === "skill" ? `<i>skill</i>` : c.kind === "terminal" ? `<i>in a terminal</i>` : c.kind === "cue" ? `<i>in Cue</i>` : c.kind === "unavailable" ? `<i>not hosted</i>` : ""}</button>`).join("")}${foot}</div>`;
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
  const agent = agentName(sessionOf(sidOfKey(key))?.harness);
  if (c.kind === "terminal") return `<div class="slash-hint"><b>/${esc(c.name)}</b>: one of ${esc(agent)}'s own screens, which only its terminal shows. Cue says what to run there.</div>`;
  if (c.kind === "cue") return `<div class="slash-hint"><b>/${esc(c.name)}</b>: ${esc(c.desc)}</div>`;
  if (c.kind === "unavailable") return `<div class="slash-hint bad"><b>/${esc(c.name)}</b> is one of ${esc(agent)}'s own commands, which a hosted session doesn't have. Move to terminal to use it.</div>`;
  return `<div class="slash-hint">${c.desc ? `<b>/${esc(c.name)}${c.hint ? ` ${esc(c.hint)}` : ""}</b>: ${esc(c.desc)}` : `Starts with “/”, so the agent runs it as a command. ${asText}`}</div>`;
}
/** Redraw the menu and hint of every box drawn that writes to session `sid` (its own, and the Desk lead's box for a lead). */
const refreshCmdOf = (sid) => { const keys = new Set([...document.querySelectorAll("textarea[data-text]")].map((el) => el.dataset.text)); keys.forEach((k) => sidOfKey(k) === sid && refreshCmd(k)); };
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
  if (name.startsWith("@")) return atPick(key, name);
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
// ---------- "@" in a lead's box: its crew to pick from, as "@name " (it goes to the lead, which passes it on) ----------
/** The box's text field: the one you're typing in when it's this box's, else the first drawn. */
const atField = (key) => document.activeElement?.dataset?.text === key ? document.activeElement : document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`);
/** "@na" just before the caret (at the start or after a space): where its "@" is, where the name ends, and what's
 *  typed of it; else null. */
function atTyped(key) {
  const text = draft(key).text, el = atField(key);
  const at = el && el.value === text ? el.selectionEnd : text.length;
  const m = /(?:^|\s)@([\w.-]*)$/.exec(text.slice(0, at));
  return m ? { from: at - m[1].length - 1, to: at, q: m[1].toLowerCase() } : null;
}
/** The lead's teammates for the "@" being typed: names that start with it first, then names that contain it; the
 *  ones still in the crew before the committed and set-aside ones. None once a name is typed out in full. */
function atMatches(key) {
  const sid = sidOfKey(key), team = (sid && sessionOf(sid)?.team) || [];
  const a = team.length ? atTyped(key) : null;
  if (!a) return [];
  const seen = new Set(), gone = (t) => t.state === "committed" || t.state === "aside";
  const mates = [...team.filter((t) => !gone(t)), ...team.filter(gone)].filter((t) => t.name && !seen.has(t.name) && seen.add(t.name));
  const starts = mates.filter((t) => t.name.toLowerCase().startsWith(a.q));
  const hits = [...starts, ...mates.filter((t) => !starts.includes(t) && t.name.toLowerCase().includes(a.q))];
  if (hits.length === 1 && hits[0].name.toLowerCase() === a.q) return [];
  // Each one's word and look ("need": red) as team.js crewMark gives them everywhere else.
  return hits.map((t) => { const { word, look } = crewMark(sid, t); return { name: `@${t.name}`, word, look, mate: true }; });
}
/** A teammate picked (Enter, Tab or a click): "@name " in place of the "@" being typed, the caret after it. */
function atPick(key, name) {
  const a = atTyped(key), d = draft(key);
  if (!a) return;
  const rest = d.text.slice(a.to).replace(/^\S*/, "").replace(/^ /, "");
  d.text = `${d.text.slice(0, a.from)}${name} ${rest}`;
  cmdSel = 0;
  saveDrafts();
  const el = atField(key);
  if (el) { el.value = d.text; el.focus(); el.selectionStart = el.selectionEnd = a.from + name.length + 1; grow(el); }
  refreshCmd(key);
}
/** Quick phrases (Settings → Quick phrases): chips that send their text as its own message; the box keeps what's typed. */
const phrases = () => state.settings?.quick?.phrases || [];
/** The words a message asks you to reply with (Reply "commit and install", Reply “merge”): its last such line's, or "". */
function replyAsked(text) {
  const all = [...String(text || "").matchAll(/\breply\s+(?:with\s+)?["“]([^"”\n]{1,120})["”]/gi)];
  return all.length ? all[all.length - 1][1].trim() : "";
}
/** The suggested reply in a session's empty box once its turn has ended: the words its latest message asks you to
 *  reply with (#186), else Claude's own suggestion. A box of the window's own (BOX_OWNERS: an item page's "item:…") has its own. */
const suggestionFor = (sid) => {
  for (const o of BOX_OWNERS) { const own = o.suggestion?.(sid); if (own != null) return own; }
  const s = sid && sessionOf(sid);
  if (s?.state !== "waiting") return "";
  const last = [...(s.thread || [])].reverse().find((e) => e.role === "agent" || e.role === "you");
  return (last?.role === "agent" && replyAsked(last.text)) || s.suggestion || "";
};
/** The quick phrases, with (at the right) where the session runs when it's a terminal app's. */
const quickRow = (attrs, sid = "") => {
  const place = placeTab(sid);
  return phrases().length || place ? `<div class="quick">${phrases().map((p) => `<button class="qp" ${attrs} data-phrase="${esc(p)}">${esc(p)}</button>`).join("")}${place}</div>` : "";
};
/** Claude's suggestion laid over an empty box's first line: "Suggested", the text, a Tab keycap and `tail` (the Inbox's
 *  "or write your own"); `attrs` on the line (the Inbox's click takes it). Empty when there's none. */
const sugLineHtml = (sug, tail = "", attrs = "") => `<div class="sug-line" ${attrs} aria-hidden="true">${sug ? `<span class="sug-k">Suggested</span><span class="sug-t">${esc(sug)}</span><kbd>Tab</kbd>${tail}` : ""}</div>`;
/** Tab in an empty box with a suggestion: the suggestion, typed in for you (Enter sends it, or edit it first). `sid`
 *  for a box whose draft isn't the session's own (an Inbox answer to a question); `el` that box, when it's known. */
function takeSuggestion(key, sid = sidOfKey(key), el = null) {
  const sug = suggestionFor(sid);
  const d = draft(key);
  if (!sug || d.text.trim()) return false;
  d.text = sug;
  saveDrafts();
  el ||= document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`);
  if (el) { el.value = sug; el.selectionStart = el.selectionEnd = sug.length; el.dispatchEvent(new Event("input", { bubbles: true })); }
  return true;
}
/** Settings → Quick phrases as you type them (redraws keep them); saved as you type (app/inputs.js) and when you leave. */
let qpEdit = null;
let qpSaveAt = 0;
/** What's in the quick phrase fields, saved (empty ones left out); nothing while none is being edited. */
function saveQuickPhrases() {
  clearTimeout(qpSaveAt);
  if (!qpEdit) return;
  const list = qpEdit.map((p) => p.trim()).filter(Boolean);
  if (JSON.stringify(list) !== JSON.stringify(phrases())) setSetting("quick.phrases", list);
}
/** The quick phrase last sent, by box and phrase: a second click on it straight after (a double click) sends nothing. */
let phraseLast = { key: "", phrase: "", at: 0 };
/** True while a quick phrase is on its way from a box (the senders read it at once): what didn't go isn't put back
 *  in the box, and the box isn't emptied when it goes. What you'd typed there is the box's own. */
let phraseSending = false;
/** A quick phrase clicked under box `key`: sent at once as its own message, the way that box's Send sends (#477).
 *  Anything typed in the box stays there as it was; a repeat click on the same phrase straight after is ignored.
 *  `button` finds the box's Send when it isn't a Desk box (the Inbox has its own). */
function sendPhrase(key, phrase, button = (k) => document.querySelector(`textarea[data-text="${CSS.escape(k)}"]`)?.closest(".composer")?.querySelector(".btn.send")) {
  const t = Date.now();
  if (phraseLast.key === key && phraseLast.phrase === phrase && t - phraseLast.at < 1500) return;
  phraseLast = { key, phrase, at: t };
  const typed = drafts[key], mine = { text: phrase, choices: {}, images: [] };
  drafts[key] = mine;
  phraseSending = true;
  try {
    const own = boxOwner(key);
    if (own) own.send(key);
    else {
      renderMain();
      const b = button(key);
      // A box held for an answer first ("Answer first"): nothing goes yet.
      if (!b || b.matches("[data-held]")) toast("Answer above first, then send.");
      else { b.disabled = false; b.click(); }
    }
  } finally {
    phraseSending = false;
    if (typed) drafts[key] = typed; else delete drafts[key];
    saveDrafts();
    renderMain();
  }
}
/** A box's images, before they're sent: each opens large, × takes it off. */
function draftChips(key) {
  const d = draft(key);
  return d.images.length ? `<div class="chips">${d.images.map((im, n) => `<span class="chip"><button class="chip-img" data-draft-lightbox="${esc(key)}:${n}" aria-label="Open ${esc(im.name)}"><img src="${im.data}" alt="${esc(im.name)}"/></button><button class="chip-x" data-unattach="${esc(key)}:${n}" aria-label="Remove image">×</button></span>`).join("")}${d.images.length > 1 ? `<span class="chip-count">${d.images.length} images</span>` : ""}</div>` : "";
}
/** A text box that grows as you type (Shift+Enter = new line, Enter = send), optionally with images.
 *  The boxes that message a session (the ones with images) also get the quick phrases, unless `quick` is false (the
 *  crew page with a teammate on its left: they belong to the chat). `alt` shows beside Send once something's typed;
 *  `extra` always does. */
function box(key, placeholder, label, sendAttrs, images, alt = "", extra = "", quick = true) {
  const d = draft(key), sid = sidOfKey(key);
  const ready = d.text.trim() || (images && d.images.length);
  const bang = bangTyped(d.text) && !d.images.length;
  // Claude's suggestion in the empty box: "Suggested", the text and a Tab keycap, laid over the box (a placeholder
  // can't style its parts) and hidden by CSS once anything's typed; Tab types it in. The wrapper and the line are
  // always drawn, so a redraw never swaps out the box you're typing in. Send sits inside the box, at the end of its
  // row, so the box spans the full width (as the terminal dock does).
  return boxHtml({
    key, look: "composer", rows: 2, placeholder, drop: !!images,
    sug: images ? suggestionFor(sid) : "", sugSid: key.startsWith("s:") ? "" : sid,
    bang: bangTyped(d.text),
    cmd: sid ? `<div class="cmd-slot">${cmdMenu(key)}</div>` : "",
    top: images && quick ? quickRow(`data-quick="${esc(key)}"`, sid) : "",
    clock: images && sid ? `<button class="icon-btn ${sendAtFor === key ? "on" : ""}" data-send-at-open="${esc(key)}" aria-label="Send at a time">${CLOCK_ICON}</button>` : "",
    extra: `${extra}${ready ? alt : ""}`,
    send: { attrs: sendAttrs, label: bang ? "Run" : label, disabled: !ready },
    below: `${sendAtFor === key ? sendAtBar(key) : ""}${slashHint(key)}`,
  });
}
/** The box whose "Send at…" bar is open (its clock button), and the time picked there ("HH:MM"). */
let sendAtFor = null, sendAtTime = "08:01";
/** "Send at…" under a session's box: what's typed goes at that time (tomorrow if it has passed today), sent by Cue. */
const sendAtBar = (key) => `<div class="send-at-bar">Send at <input type="time" data-send-at-time value="${esc(sendAtTime)}"><span class="grow"></span><button class="btn sm" data-send-at-x>Cancel</button><button class="btn sm primary" data-send-at-go="${esc(key)}">Send ${esc(sendAtWhen(sendAtTime).words)}</button></div>`;
/** "HH:MM" as the next time it comes: today, or tomorrow once it's passed. */
function sendAtWhen(hm) {
  const [h, m] = hm.split(":").map(Number), at = new Date();
  at.setHours(h || 0, m || 0, 0, 0);
  if (at.getTime() <= Date.now()) at.setDate(at.getDate() + 1);
  const today = at.toDateString() === new Date().toDateString();
  return { ms: at.getTime(), words: `${today ? "today" : "tomorrow"} at ${hm}` };
}
/** "Send tomorrow at 08:01": the box's text goes to Cue, which sends it then; the box is emptied. */
function sendAtKeep(key) {
  const d = draft(key), sid = sidOfKey(key);
  if (!d.text.trim()) return toast("Type what to send first");
  if (d.images.length) return toast("Pictures can't wait for a time yet: send them now");
  const when = sendAtWhen(sendAtTime);
  invoke("send_at", { sessionId: sid, text: d.text, atMs: when.ms }).then(() => {
    d.text = ""; sendAtFor = null; saveDrafts(); renderMain(); toast(`Cue sends it ${when.words}`);
  }, (e) => toast(`Couldn't keep it: ${e}`));
}
/** The message for later: back into the box to change (Edit), dropped (Cancel), or sent now. */
async function sendAtEdit(sid) {
  const text = await invoke("send_at_drop", { sessionId: sid }).catch(() => null);
  if (text) boxPutBack(`s:${sid}`, text, [], { focus: false });
}
