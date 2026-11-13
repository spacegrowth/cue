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
const sendTo = (sid, now = false, key = `s:${sid}`) => state.restarting?.[sid] ? toast("It's restarting. Send once it's back.") : sessionOf(sid)?.state === "deciding" ? toast("It's asking you something first. Answer that, then send.") : deliver(sid, key, (text, images) => { insSent(sid, text, now); return invoke("send_to_session", { sessionId: sid, text, images, now }); });
/** The message Cue kept for a working session, back into its box to change (before anything you've typed
 *  there since), images and all. */
async function pullHeld(sid) {
  forgetSent(sid);
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
  // A hosted lead's /team (team_send.js).
  return sessionOf(sid)?.harness === "claude" ? [BTW_CMD, ...teamCmds(sid, list), ...list.filter((c) => c.name !== "btw")] : list;
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
/** What Claude suggests you type next (a hosted session's prompt suggestion), while it waits on you; "" when none. */
const suggestionFor = (sid) => {
  const s = sid && sessionOf(sid);
  return s?.state === "waiting" ? s.suggestion || "" : "";
};
/** The quick phrases, with (at the right) where the session runs when it's a terminal app's. */
const quickRow = (attrs, sid = "") => {
  const place = placeTab(sid);
  return phrases().length || place ? `<div class="quick">${phrases().map((p) => `<button class="qp" ${attrs} data-phrase="${esc(p)}">${esc(p)}</button>`).join("")}${place}</div>` : "";
};
/** Tab in an empty box with a suggestion: the suggestion, typed in for you (Enter sends it, or edit it first). */
function takeSuggestion(key) {
  const sug = suggestionFor(sidOfKey(key));
  const d = draft(key);
  if (!sug || d.text.trim()) return false;
  d.text = sug;
  saveDrafts();
  const el = document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`);
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
const withPhrase = (text, p) => (text.trim() ? `${text.trimEnd()} ${p}` : p);
/** A phrase sent from box `key`: after anything typed in it, and sent the way that box's Send would send it (a
 *  quick phrase). */
function sendPhrase(key, phrase) {
  const d = draft(key);
  d.text = withPhrase(d.text, phrase);
  saveDrafts();
  renderMain();
  document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`)?.closest(".composer")?.querySelector(".btn.send")?.click();
}
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
  // A Claude Code session's box: its ring ahead of the mic (status.js), while the status list is on in Settings.
  const sess = images && sidOfKey(key) && sessionOf(sidOfKey(key));
  const status = sess?.harness === "claude" && statusShown() ? statusChip(null, sess) : "";
  // Claude's suggestion in the empty box: "Suggested", the text and a Tab keycap, laid over the box (a placeholder
  // can't style its parts) and hidden by CSS once anything's typed; Tab types it in. The wrapper and the line are
  // always drawn, so a redraw never swaps out the box you're typing in.
  const sug = images ? suggestionFor(sidOfKey(key)) : "";
  const sugLine = `<div class="sug-line" aria-hidden="true">${sug ? `<span class="sug-k">Suggested</span><span class="sug-t">${esc(sug)}</span><kbd>Tab</kbd>` : ""}</div>`;
  // Send sits inside the box, at the end of its row, so the box spans the full width (as the terminal dock does).
  return `<div class="composer"><div class="field ${images ? "drop" : ""} ${bangTyped(d.text) ? "bang" : ""}" data-drop="${images ? esc(key) : ""}">${sidOfKey(key) ? `<div class="cmd-slot">${cmdMenu(key)}</div>` : ""}${images ? quickRow(`data-quick="${esc(key)}"`, sidOfKey(key)) : ""}${chips}<div class="field-row">
    <div class="ta-wrap"><textarea rows="2" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off" data-text="${esc(key)}"${sug ? ` class="sug" aria-label="${esc(`Suggested: ${sug}. Tab types it in.`)}"` : ""} placeholder="${esc(sug ? " " : placeholder)}">${esc(d.text)}</textarea>${sugLine}</div>
    ${status}<button class="icon-btn mic ${dictating?.key === key ? "on" : ""}" data-mic="${esc(key)}" title="${dictating?.key === key ? "Stop dictating" : "Dictate: talk and it types here"}" aria-label="Dictate">${MIC_ICON}</button>
    ${images ? `<button class="icon-btn" data-attach="${esc(key)}" title="Add an image (or paste one with ⌘V)" aria-label="Add image">${IMAGE_ICON}</button>` : ""}
    ${extra}${ready ? alt : ""}<button class="btn primary send" ${sendAttrs} ${ready ? "" : "disabled"}>${label}</button></div>${slashHint(key)}</div></div>`;
}

