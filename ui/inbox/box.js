// Cue UI Inbox (ui/inbox/list.js says what it is): the brief's main column.
// ---------- the brief: its words, the question, the box ----------

/** Text in paragraphs, split at blank lines (a code block stays whole). */
function inboxParas(text) {
  const out = [];
  let cur = [], fence = false;
  for (const l of String(text || "").split("\n")) {
    if (/^\s*```/.test(l)) fence = !fence;
    if (!fence && !l.trim()) { if (cur.length) out.push(cur.join("\n")); cur = []; } else cur.push(l);
  }
  if (cur.length) out.push(cur.join("\n"));
  return out;
}
/** The sentence where it says what it does next ("Next I'd tune the stop distance."), as written; "" when none. */
function inboxThen(text) {
  const all = String(text || "").split("\n").flatMap((l) => l.split(/(?<=[.!?])\s+/));
  return all.filter((x) => { const p = plain(x); return p && !/\?["'”)]*$/.test(p) && INBOX_THEN.test(p); }).at(-1) || "";
}
/** Its words before the question (markdown), `cut` (the question, the sentence on what's next) taken out: whole, unless
 *  it's very long (over INBOX_SAID_LONG; `all` keeps it whole anyway): then its last paragraphs up to INBOX_SAID_MAX
 *  characters; one long paragraph is cut from its start. */
function inboxSaid(text, cut = [], all = false) {
  let t = String(text || "");
  for (const c of cut) { const at = c ? t.lastIndexOf(c) : -1; if (at >= 0) t = t.slice(0, at) + t.slice(at + c.length); }
  const paras = inboxParas(t).filter((p) => plain(p)), out = [];
  if (all || plain(t).length <= INBOX_SAID_LONG) return paras.join("\n\n").trim();
  for (const p of paras.reverse()) {
    if (out.length && out.join("").length + p.length > INBOX_SAID_MAX) break;
    out.unshift(p);
  }
  const one = out[0] || "";
  if (out.length === 1 && one.length > INBOX_SAID_MAX * 1.3 && !one.includes("```")) {
    const tail = one.slice(-INBOX_SAID_MAX), from = tail.search(/(?<=[.!?]\s)\S/);
    out[0] = `…${tail.slice(from > 0 ? from : 0)}`;
  }
  return out.join("\n\n").trim();
}
/** Its latest words in its steps this turn (what it said between steps), "" when there are none. */
const inboxFeedSaid = (sid) => (inboxTurn(sid)?.items || []).filter((x) => x.t === "say").at(-1)?.text || "";
/** Its own words for the brief: a finished turn's reply; an ask's last words before it (as Cue caught them with the
 *  ask, else from its steps); else what it said last. */
function inboxWords(r) {
  if (r.it?.kind === "waiting") return finishedText(r.it);
  if (r.it && r.kind === "ask") return (r.it.context || []).filter((x) => x.role !== "user").at(-1)?.text || inboxFeedSaid(r.sid);
  return inboxFeedSaid(r.sid) || lastSaid(r.s || {});
}
/** A choice's label without its "(Recommended)", and whether it had one: [label, recommended]. */
const inboxLabel = (label) => choiceLabel(label);
/** A plain box to write in (pictures pasted or dropped on it go with it), and its button (ui/box/markup.js boxHtml's
 *  "inbox" look): Send, then talk and pictures, as the chat's box has them (the mic types what you say into the box; the
 *  picture button adds one, ⌘V pastes one, or drop it on the box). Enter sends too (app/keys.js, by its draft key).
 *  With `sid`, Claude's suggestion for that session shows in the box while it's empty (inboxSuggest). */
const inboxBox = (key, placeholder, label, attrs, extra = "", sid = "", phr = "") => {
  const sug = sid ? suggestionFor(sid) : "";
  return boxHtml({ look: "inbox", key, placeholder, sug, sugSid: sid, sugLine: inboxSuggest(sug, key, sid), drop: inboxPics(key), extra: extra + phr, send: { attrs, label } });
};
/** Whether pictures go with what's written in this box: a message to the session (its "s:" box) does; an answer to its
 *  question is words only. */
const inboxPics = (key) => key.startsWith("s:");
/** The reply, as in mail: you write right under the message (no frame or line round it), Claude's suggestion in the
 *  empty box, then one row: Send, talk, pictures and Open chat on the left, your quick phrases on the right (those that
 *  don't fit go in a menu that opens upward). */
function inboxReply(sid, key, ph, label, attrs) {
  return inboxBox(key, ph, label, attrs, `<button class="btn" data-inbox-full="${esc(sid)}">Open chat</button>`, sid, inboxPhrases(sid, key));
}
/** Claude's own suggested reply, in the box's first line while it's empty (the chat box's own line, with "or write your
 *  own"): Tab or a click puts it in the box, to send as it is or change; anything typed hides it (CSS). */
const inboxSuggest = (sug, key, sid) => sugLineHtml(sug, `<span class="sug-or">or write your own</span>`, `data-inbox-sug data-key="${esc(key)}" data-sid="${esc(sid)}"`);
/** The quick phrase menu open, by its box's draft key ("" when none is). */
let inboxPhrOpen = "";
/** Your quick phrases (Settings → Quick phrases; a click sends it on its own, the box kept as it is) at the right of the
 *  box's row; inboxFitPhrases hides those that don't fit and lists them in More's menu. */
function inboxPhrases(sid, key) {
  const ps = phrases();
  if (!ps.length) return "";
  const k = `data-key="${esc(key)}" data-sid="${esc(sid)}"`, open = inboxPhrOpen === key;
  const chip = (p, i) => `<button class="ib-sug" data-inbox-phrase="${esc(p)}" data-i="${i}" ${k}>${esc(p)}</button>`;
  return `<div class="ib-phr"><div class="ib-phr-row">${ps.map(chip).join("")}</div>`
    + `<span class="ib-phr-more" hidden><button class="ib-sug" data-inbox-phr-more="${esc(key)}" aria-expanded="${open}">More<span class="ib-caret">▴</span></button>`
    + `${open ? `<span class="ib-phr-menu" role="menu">${ps.map((p, i) => `<button role="menuitem" data-inbox-phrase="${esc(p)}" data-i="${i}" ${k}>${esc(p)}</button>`).join("")}</span>` : ""}</span></div>`;
}
/** After each draw: each phrase row shows the phrases that fit (in order), More shows when some don't, and its menu
 *  lists just those. */
function inboxFitPhrases() {
  for (const w of document.querySelectorAll(".inbox-page .ib-phr")) {
    const row = w.querySelector(".ib-phr-row"), more = w.querySelector(".ib-phr-more"), chips = [...row.children];
    chips.forEach((c) => { c.hidden = false; });
    more.hidden = true;
    if (row.scrollWidth <= row.clientWidth + 1) continue;
    more.hidden = false;
    const end = row.getBoundingClientRect().right + 1, fits = (c) => c.getBoundingClientRect().right <= end;
    let n = chips.findIndex((c) => !fits(c));
    if (n < 0) n = chips.length;
    chips.forEach((c, i) => { c.hidden = i >= n; });
    w.querySelectorAll(".ib-phr-menu [data-i]").forEach((b) => { b.hidden = +b.dataset.i < n; });
  }
}
/** A quick phrase clicked: sent on its own, as the box's Send would (compose.js sendPhrase); the box keeps what's typed. */
const inboxSendPhrase = (el) => sendPhrase(el.dataset.key, el.dataset.inboxPhrase, (k) => document.querySelector(`.inbox-page .ib-box textarea[data-text="${CSS.escape(k)}"]`)?.closest(".ib-box")?.querySelector(".btn.primary"));
/** Claude's suggestion clicked: into the box (as Tab does), the cursor at its end. */
function inboxTakeSuggestion(el) {
  const ta = el.closest(".ta-wrap")?.querySelector("textarea");
  if (!ta || !takeSuggestion(el.dataset.key, el.dataset.sid, ta)) return;
  ta.focus();
  ta.selectionStart = ta.selectionEnd = ta.value.length;
}
/** A question ask: [the whole question card (board/question.js questionHtml: every question, its suggestions picked, what
 *  will happen); a box for your own answer to the question you last picked in; Decline and one Send for all]. */
function inboxQuestions(it) {
  const n = questions(it).length, cur = stepOf(it);
  const card = `<div class="ib-card on ib-q">${questionHtml(it, "data-inbox-pick")}</div>`;
  const box = inboxReply(it.session_id, it.id, n > 1 ? `Your own answer to question ${cur + 1}…` : "Or say something else…", n > 1 ? "Use answer" : "Send", `data-act="send" data-id="${esc(it.id)}"`);
  return [card, box, `<button class="btn deny ib-decline" data-act="deny" data-id="${esc(it.id)}">Decline</button>${qSendBtn(it)}`];
}
/** Any other ask (a command to run, an edit, a plan, a form, its terminal's): the Board's own card body and buttons. */
const inboxAskCard = (it) => `<div class="ib-card on"><div class="ib-qn">${esc(verb(it).replace(/^./, (c) => c.toUpperCase()))}</div>${requestBody(it)}</div>${decisionButtons(it)}`;
/** A finished turn's question (or that it was interrupted) as a card. */
function inboxTurnCard(r, q) {
  const what = r.it.interrupted ? "Interrupted" : "It asks";
  return `<div class="ib-card on"><div class="ib-qn">${what}</div><div class="ib-qt">${esc(q || "It stopped mid-turn. Say “continue”, or what to do instead.")}</div></div>`;
}
/** Getting full: how full, and Compact now, Hand off (its hand-off skill, else Cue's: as the hand-off insight does),
 *  or Remind me later (Snooze). */
function inboxChoreCard(r) {
  const { sid, s } = r, h = handoffSkills(sid, s)[0];
  const hand = h ? `data-act="ins-fill" data-ins-cmd="/${esc(h.name)}"` : `data-act="ins-cue"`;
  const acts = insBusy.has(sid) ? insBusyNote(sid) : `<button class="btn primary" data-act="ins-compact" data-sid="${esc(sid)}">Compact now</button><button class="btn" ${hand} data-sid="${esc(sid)}">Hand off to a fresh session</button>`
    + `<span class="ib-snz-wrap"><button class="btn" data-inbox-snooze="${esc(sid)}" data-at="card" aria-expanded="${inboxSnoozeAt === "card"}">Remind me later</button>${inboxSnoozeAt === "card" ? inboxSnoozeMenu(sid) : ""}</span>`;
  return `<div class="ib-card on chore"><div class="ib-full">${inboxMeter(s, "big")}<span>${inboxCtx(s)}% of its context used</span></div>
    <div class="ib-qt">Getting full: compact or hand off?</div>
    <div class="ib-why">Compact sums up its chat in place and keeps this session. A hand-off writes a note of where things stand and starts a fresh session from it; this one closes (Closed keeps it).</div>
    <div class="ib-foot-acts">${acts}</div></div>`;
}
/** Three dots that count 1, 2, 3 after a line still going ("Thinking"), in a fixed width so nothing moves. */
const INBOX_DOTS = `<span class="ib-dots" aria-hidden="true"><i>.</i><i>.</i><i>.</i></span>`;
/** A working session: what it's on now (one line), its latest steps, and its bar. The same size all along. */
function inboxRunCard(r) {
  // What waits to go shows in its own block (inboxPending), not here; sent and not at work yet, it's starting.
  const [, said] = inboxRunLines(r, true), line = said === "Sent" ? "Starting…" : said;
  const m = /^(.*?)(?:…|\.\.\.)?$/.exec(line), dots = /(…|\.\.\.)$/.test(line) || line === "Thinking";
  const { agents, scripts } = inboxLive(r), also = [agents.length && `${agents.length} agent${agents.length === 1 ? "" : "s"}`, scripts.length && `${scripts.length} script${scripts.length === 1 ? "" : "s"}`].filter(Boolean).join(" · ");
  return `<div class="ib-card run"><div class="ib-qn">Now</div><div class="ib-qt">${esc(dots ? m[1] : line)}${dots ? INBOX_DOTS : ""}${also ? `<span class="ib-also">+ ${also} at work</span>` : ""}</div>${inboxRunSteps(r)}${inboxBar(r.s)}</div>`;
}
/** The run card's step lines: the latest INBOX_RUN_STEPS of this turn, the ones before folded into one line above them,
 *  in a space of that many lines and the fold's from the start (empty until its first step), so it never grows. */
const INBOX_RUN_STEPS = 3;
function inboxRunSteps(r) {
  const steps = inboxTurnSteps(r), lines = inboxStepLines(steps), shown = lines.slice(-INBOX_RUN_STEPS), more = lines.length - shown.length;
  const fold = more ? `<div class="ib-step-more">${more} earlier</div>` : "";
  return `<div class="ib-steps ib-run-steps" style="--ib-run-n:${INBOX_RUN_STEPS + 1}">${fold}${shown.map(inboxStepHtml).join("")}</div>`;
}
/** Its steps in the turn it's on: none while the latest turn read is still the one before what you sent (its steps
 *  feed is read every few seconds). */
function inboxTurnSteps(r) {
  const turn = inboxTurn(r.sid), sent = Math.max(inboxGoing(r.sid).at(-1)?.at || 0, (r.s?.thread || []).filter((e) => e.role === "you").at(-1)?.at_ms || 0);
  return turn && (turn.at_ms || 0) >= sent - 5000 ? (turn.items || []).filter((x) => x.t === "step") : [];
}
/** Whether it takes a written reply here: a question, a finished turn, one at work or done (not a permission or the
 *  context row, which have their own buttons). */
const inboxCanReply = (r) => !!r && (["turn", "crew", "review", "run", "done"].includes(r.kind) || (r.kind === "ask" && r.it.kind === "question"));
/** The line under the box: the keys that work here (1–N for the question lit), then `acts` at
 *  its end. */
function inboxHint(r, acts = "") {
  const it = r.kind === "ask" && r.it.kind === "question" ? r.it : null, n = it ? Math.min(9, questions(it)[stepOf(it)]?.options?.length || 0) : 0;
  const keys = [n ? `<span><span class="ib-kbd">1</span>–<span class="ib-kbd">${n}</span> pick</span>` : ""].filter(Boolean);
  return keys.length || acts ? `<div class="ib-hint">${keys.join("")}<span class="grow"></span>${acts}</div>` : "";
}
/** A lead's crew that waits on you, a row each with its own buttons, as on its crew's page (team.js): a teammate's ask
 *  with Deny and Allow, a review ready with Commit or Send fixes. Empty for any other session. */
function inboxCrewHtml(sid) {
  const waits = inboxCrewWaits(sid).filter((t) => !mateNeeds(sid, t) || inboxCrewActions(sid).includes(t));
  if (!waits.length) return "";
  const need = inboxCrew(sid);
  return `<div class="ib-crew"><div class="ib-k">${need ? `Its crew · ${need} need${need === 1 ? "s" : ""} you` : "Crew reviews"}</div>${waits.map((t) => (mateNeeds(sid, t) ? mateNeedRow(sid, t) : mateReadyRow(sid, t))).join("")}${inboxMateMenu(sid, waits)}</div>`;
}
/** The sessions whose latest reply shows whole, not just its last paragraphs. */
const inboxWhole = new Set();
/** Earlier messages opened in full (`${sid}|${at_ms}|${role}`), and the sessions showing all their earlier ones. */
const inboxMsgOpen = new Set(), inboxAllEarlier = new Set();
/** How many earlier messages show (folded) before "Show N earlier". */
const INBOX_EARLIER = 6;
/** The conversation before its latest reply, as mail shows a thread: each message folded to a line (who, when, its
 *  first words); a click opens it in full. Its latest reply (shown below as It says) is left out. */
/** Its thread as mail's: { msgs: the earlier ones, said: its latest message (shown whole; null when the thread hasn't
 *  it), yours: what you wrote after that (at work: your reply, shown whole under it) }. */
/** `asks`: the asks it made after that message (an ask line the hub noted, with your answer: askLnPair), and decisions
 *  with no ask before them, shown under it. An ask is never its message, and an answer ("allowed") never your reply. */
function inboxThread(r, words) {
  const latest = plain(words || "").trim(), yours = [], asks = [];
  let msgs = askLnPair((r.s?.thread || []).filter((e) => e.text?.trim()));
  // An ask still open is the card below: not again as a row.
  if (r.kind === "ask" && msgs.at(-1)?.ask && !msgs.at(-1).answer) msgs = msgs.slice(0, -1);
  for (;;) {
    const e = msgs.at(-1);
    if (e?.ask || e?.decided) asks.unshift(e);
    else if (r.kind === "run" && e?.role === "you") yours.unshift(e);
    else break;
    msgs = msgs.slice(0, -1);
  }
  // What you just sent and its thread doesn't show yet goes where it will land, after the rest of yours: from the
  // moment you click Send the page is the one it will be at work, and the real message takes its place as it arrives.
  if (r.kind === "run") yours.push(...inboxGoing(r.sid).map((o) => ({ role: "you", text: o.text, at_ms: o.at, sent: o.images })));
  const last = msgs.at(-1);
  let said = null;
  if (last && last.role !== "you" && (r.kind === "run" ? yours.length || plain(last.text).trim() === latest : !latest || plain(last.text).trim() === latest || r.kind === "turn")) { said = last; msgs = msgs.slice(0, -1); }
  // Its message not the latest one (or none): the asks after it stay with the earlier ones, in time order.
  else { msgs = msgs.concat(asks); asks.length = 0; }
  return { msgs, said, yours, asks };
}
/** What you wrote after its latest message, whole, as your reply under it in a mail thread. */
/** A message's head as mail's: who on the left, when on the right ("2:14pm · 5m ago"; nothing when not known). */
const inboxHead = (who, ms) => `<div class="ib-mhead"><span class="ib-k">${who}</span>${ms ? `<span class="ib-when">${clockAt(ms)} · ${ago(ms)} ago</span>` : ""}</div>`;
function inboxYours(sid, yours) {
  // As its chat draws them (#422): the bubble, and its one line: Sending… while on its way, else delivered. Under
  // "Yours", with what still waits to go after them (inboxPending).
  const to = nameOf(sid, sessionOf(sid)?.project);
  return yours.map((e, n) => `<div class="ib-yours">${n ? "" : '<div class="ib-k">Yours</div>'}${msgRow(e.sent ? { st: "sending", to, sid } : { st: "delivered", to, at: msgTaken(e) }, `<div class="mg-text ib-yours-text">${esc(e.text).replace(/\n/g, "<br>")}</div>${e.sent ? inboxSentPics(e.sent) : thumbs(e.images)}`)}</div>`).join("");
}
/** The pictures of a message still on its way: what you picked, as they are (not files yet), as its chat shows them. */
const inboxSentPics = (images) => (images.length ? `<div class="thumbs">${images.map((im) => `<span class="thumb"><img src="${esc(im.data)}" alt=""/></span>`).join("")}</div>` : "");
/** What you sent it from Cue that its thread doesn't show yet and that goes now (not one kept till its turn ends, or one
 *  that failed: those wait to go, in their own block). */
const inboxGoing = (sid) => outbox.filter((o) => o.sid === sid && !landed(o) && !/^(kept in Cue|not sent)/.test(o.via || ""));
/** sid → { text, at }: the reply you last answered here, kept for its brief at work once Cue has moved it off Waiting
 *  (before its thread has it, it would otherwise go blank for a moment). */
const inboxAnswered = new Map();
function inboxEarlier(r, msgs) {
  if (!msgs.length) return "";
  const all = inboxAllEarlier.has(r.sid), hidden = all ? 0 : Math.max(0, msgs.length - INBOX_EARLIER);
  const more = hidden ? `<button class="ib-link ib-earlier-more" data-inbox-earlier="${esc(r.sid)}">Show ${hidden} earlier</button>` : "";
  return `<div class="ib-earlier"><div class="ib-k">Earlier</div>${more}${msgs.slice(hidden).map((e) => inboxMsgHtml(r.sid, e)).join("")}</div>`;
}
/** One earlier message: folded to a line, or open in full. */
function inboxMsgHtml(sid, e) {
  if (e.ask) return inboxAskMsgHtml(sid, e);
  // A decision with no ask before it (a terminal dialog's choice): one line, "You · Chose 1. Yes"; nothing to open.
  if (e.decided) return `<div class="ib-msg you decided"><span class="ib-msg-who">You</span><span class="ib-msg-at">${e.at_ms ? ago(e.at_ms) : ""}</span><span class="ib-msg-line mg-plain ib-ask-ans ${e.decided.no ? "no" : ""}">${esc(e.decided.text)}</span></div>`;
  const k = `${sid}|${e.at_ms}|${e.role}`, open = inboxMsgOpen.has(k), you = e.role === "you";
  const who = `<span class="ib-msg-who">${you ? "You" : "It"}</span><span class="ib-msg-at">${e.at_ms ? ago(e.at_ms) : ""}</span>`;
  // Folded: two lines of it, and "more" when that cuts it (a click opens it whole; "less" folds it again).
  const line = plain(e.text).replace(/\s+/g, " ").trim(), cut = line.length > INBOX_MSG_CUT || /\n\s*\S/.test(String(e.text).trim());
  const tag = cut ? `<span class="ib-msg-more">${open ? "less" : "more"}</span>` : "";
  const body = open ? `<div class="ib-msg-body ${you ? "" : "ib-md msg"}">${you ? esc(e.text).replace(/\n/g, "<br>") : md(e.text, { pathsOf: sid })}${thumbs(e.images)}${you ? "" : localImages(e.text)}${tag}</div>` : `<span class="ib-msg-fold"><span class="ib-msg-line mg-plain">${esc(line.slice(0, 600))}</span>${thumbs(e.images)}${tag}</span>`;
  return `<div class="ib-msg ${you ? "you" : "it"} ${open ? "open" : ""}" role="button" data-inbox-msg="${esc(k)}">${who}${body}</div>`;
}
/** An ask it made and your answer, as one row: "It · Asked to run `cmd…` · Allowed"; opened, the command whole, in code. */
function inboxAskMsgHtml(sid, e) {
  const { ask, answer } = e, k = `${sid}|${e.at_ms}|ask`, open = inboxMsgOpen.has(k);
  const line = ask.code || (ask.tool ? "" : ask.what), flat = line.replace(/\s+/g, " ").trim();
  const ans = answer ? ` · <span class="ib-ask-ans ${answer.no ? "no" : ""}">${esc(answer.text)}</span>` : "";
  const head = `${esc(ask.head)}${ask.tool ? "" : ":"}`;
  const tag = flat.length > 60 || line.includes("\n") ? `<span class="ib-msg-more">${open ? "less" : "more"}</span>` : "";
  const body = open
    ? `<div class="ib-msg-body"><div class="ib-ask-line">${head}${ans}</div>${ask.code ? `<pre class="mg-code"><code>${ask.tool === "Bash" ? "$ " : ""}${esc(ask.code)}</code></pre>` : line ? `<div>${esc(line)}</div>` : ""}${tag}</div>`
    : `<span class="ib-msg-fold"><span class="ib-msg-line mg-plain ib-ask-line">${head}${flat ? ` <code>${esc(flat.length > 60 ? `${flat.slice(0, 60)}…` : flat)}</code>` : ""}${ans}</span>${tag}</span>`;
  return `<div class="ib-msg it ask ${open ? "open" : ""}" role="button" data-inbox-msg="${esc(k)}"><span class="ib-msg-who">It</span><span class="ib-msg-at">${e.at_ms ? ago(e.at_ms) : ""}</span>${body}</div>`;
}
/** The asks it made after its message (and how you answered), and your decisions, under it: a row each, as in Earlier. */
const inboxAsksHtml = (sid, asks) => (asks.length ? `<div class="ib-asks">${asks.map((e) => inboxMsgHtml(sid, e)).join("")}</div>` : "");
/** What you've written to it that hasn't reached it yet, as its chat shows it (chat/render.js): on its way, queued in
 *  it till its next step (Send now), kept in Cue till its turn ends (Edit, Send now), or to go at a time (Edit,
 *  Cancel, Send now). Empty when nothing waits. */
function inboxPending(r, after = false) {
  const going = r.kind === "run" ? inboxGoing(r.sid) : [];   // at work, those show as yours in the thread
  const s = r.s, rows = outbox.filter((o) => o.sid === r.sid && !landed(o) && !going.includes(o)).map((o) => outboxRow(o, s, `ib-out:${o.at}`));
  if (s?.held) rows.push(heldRow(s));
  if (s?.send_at) rows.push(sendAtRow(s));
  if (s?.queued) rows.push(queuedRow(s));
  // Under the rest of yours as more of them, each its bubble and its one line (#422): "Yours" over them when nothing
  // of yours is above.
  return rows.length ? `<div class="ib-pending${after ? " after" : ""}">${after ? "" : '<div class="ib-k">Yours</div>'}${rows.join("")}</div>` : "";
}
/** What waits to go to it, in a line for its row, in its message's words (#422: "Queued for cue-418: …"); "" when
 *  nothing does. `html`: as markup, the message family's plain line (.mg-plain, #501), Cue's own message in its
 *  words ("Queued for <who>: Asked <who> · #461 not done…"). */
function inboxPendingLine(s, html = false) {
  const to = s ? nameOf(s.session_id, s.project) : "";
  const w = s?.queued ? ["Queued for", s.queued.text] : s?.held ? ["Not sent yet", s.held.text] : s?.send_at ? ["Sends later", s.send_at.text] : null;
  if (!w) return "";
  const head = w[0] === "Queued for" ? `Queued for ${html ? `<span class="mg-who">${esc(to)}</span>` : to}` : w[0];
  if (!html) return `${head}: ${cueMsgPlain(w[1], to) ?? `“${plain(w[1] || "")}”`}`;
  return `<span class="mg-plain">${head}: ${cueMsgPlainHtml(w[1], to) ?? `“${esc(plain(w[1] || ""))}”`}</span>`;
}
/** The brief's main column: its words before the question, the question with its choices, what it plans next, then
 *  the box to answer in (at the foot). */
function inboxMain(r) {
  const thread = inboxThread(r, inboxWords(r));
  // At work after your reply, its message is the one you answered (the thread's, or the ask's while it's still there,
  // or as it was when you answered it). That last is kept only until the thread has its message, or the session is
  // no longer at work, so it can't stand in for a later turn's.
  if (r.it?.kind === "waiting") inboxAnswered.set(r.sid, { text: finishedText(r.it), at: r.it.created_ms });
  else if (r.kind !== "run" || thread.said) inboxAnswered.delete(r.sid);
  const kept = inboxAnswered.get(r.sid);
  const words = r.kind === "run" && thread.yours.length ? thread.said?.text || (r.it?.kind === "waiting" ? finishedText(r.it) : kept?.text || "") : inboxWords(r), q = r.kind === "turn" && !r.it.interrupted ? inboxEndQuestion(words) : "";
  const then = r.kind === "run" ? "" : inboxThen(words), said = inboxSaid(words, [q, then]);
  const label = r.kind === "run" ? "It said last" : r.kind === "ask" || r.kind === "turn" ? "It says" : "It said";
  const whole = inboxWhole.has(r.sid), cut = said !== inboxSaid(words, [q, then], true);
  const shown = whole ? inboxSaid(words, [q, then], true) : said, more = cut ? `<button class="ib-link" data-inbox-whole="${esc(r.sid)}">${whole ? "Show less" : "Show the whole reply"}</button>` : "";
  const saidHtml = shown ? `<div class="ib-said">${inboxHead(label, thread.said?.at_ms || r.it?.created_ms || (r.kind === "run" && kept?.text === words ? kept.at : 0))}<div class="ib-md msg">${md(shown, { pathsOf: r.sid })}</div>${thumbs(thread.said?.images)}${localImages(shown)}${more}</div>` : "";
  const thenHtml = then ? `<div class="ib-then">It plans next: <b>${esc(plain(then))}</b></div>` : "";
  const reply = (ph) => inboxReply(r.sid, `s:${r.sid}`, ph, "Send", r.it ? `data-act="send" data-id="${esc(r.it.id)}"` : `data-inbox-send="${esc(r.sid)}"`);
  let top = "", foot = "", acts = "";
  if (r.kind === "ask" && r.it.kind === "question") [top, foot, acts] = inboxQuestions(r.it);
  else if (r.kind === "ask") top = inboxAskCard(r.it);
  else if (r.kind === "turn") { top = q || r.it.interrupted ? inboxTurnCard(r, q) : ""; foot = reply("Reply…"); }
  else if (r.kind === "chore") top = inboxChoreCard(r);
  else if (r.kind === "run") { top = inboxRunCard(r); foot = reply("Tell it something (it reads it when it can)…"); }
  else foot = reply("Tell it what's next…");
  const crew = inboxCrewHtml(r.sid);
  // Oldest at the top, newest at the foot, as a mail thread: earlier, its message, your reply, what waits to go, then
  // what it's doing now, right over the box. (Housekeeping and a crew's asks lead with what they need of you.) The
  // thread scrolls; the box is pinned to the foot of the pane, so nothing drawn above it ever moves it.
  const earlier = inboxEarlier(r, thread.msgs), asksHtml = inboxAsksHtml(r.sid, thread.asks);
  const first = r.kind === "run" ? `${saidHtml}${asksHtml}${inboxYours(r.sid, thread.yours)}${inboxPending(r, thread.yours.length > 0)}${crew}${top}`
    : `${r.kind === "chore" || r.kind === "crew" ? `${crew}${top}${saidHtml}${asksHtml}` : `${saidHtml}${asksHtml}${top}${crew}`}${thenHtml}${inboxPending(r)}`;
  return `<div class="ib-main"><div class="ib-page-body ib-thread" data-scroll-key="inbox-main" data-sid="${esc(r.sid)}" data-kind="${r.kind}">${earlier}${first}</div><div class="ib-foot">${foot}${inboxHint(r, acts)}</div></div>`;
}
/** Whether the open session's thread is at its newest (its foot) as it's drawn again: it stays there, unless you've
 *  scrolled up. Read off the page before the draw replaces it; a session or shape new to the page (just opened, just
 *  answered) and a message just sent start there too. */
function inboxAtEnd(sid, kind) {
  const el = document.querySelector(".inbox-page .ib-thread");
  if (!el || el.dataset.sid !== sid || el.dataset.kind !== kind || (typeof chatToEnd !== "undefined" && chatToEnd)) return true;
  const { box, end } = inboxThreadEnd(el);
  return box === el ? boxAtEnd(el) : Math.abs(end - box.scrollTop) < 24;
}
/** What scrolls the open thread, and where it stands at its newest. With room, the thread scrolls itself and its newest
 *  is its foot. Narrow, the whole brief scrolls (the thread, then the side under it), and the newest is where the main
 *  column ends right at the pane's foot, its last message just over the box (not the brief's own foot, the side's). */
function inboxThreadEnd(el) {
  if (getComputedStyle(el).overflowY !== "visible") return { box: el, end: el.scrollHeight - el.clientHeight };
  const box = el.closest(".ib-brief"), main = el.closest(".ib-main");
  const end = main.getBoundingClientRect().bottom - box.getBoundingClientRect().top + box.scrollTop - box.clientHeight;
  return { box, end: Math.max(0, end) };
}
/** Set as the brief is drawn: the thread goes to its newest after the draw (inboxKeepEnd). */
let inboxToEnd = false;
/** After each draw: the open thread at its newest, when it was there (or is new). */
function inboxKeepEnd() {
  const el = inboxToEnd && document.querySelector(".inbox-page .ib-thread");
  if (!el) return;
  const { box, end } = inboxThreadEnd(el);
  if (box === el) boxStickToEnd(el, false); else box.scrollTop = end;
}
