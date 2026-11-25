// Cue UI chat — the chat's HTML: turns, status, the meta line and the ⋯ menu.
/** One turn's steps, between your message and its reply, each its own row as in 0.1.86 (#897): what it did, the file
 *  and how it went, a failed one in red. The newest turn is open (and, while the next one works, the one before it);
 *  older ones fold to one chip ("▸ 6 steps · 2 commands, 1 read, 3 edits, 1 failed"), what it said between them left
 *  in view. While it works, its steps come in as they run, under "Working · N steps". "Show all" opens every step's
 *  output or diff; a step opens on its own when you click it. `live`: the turn it's working on now. `latest`: a turn
 *  open by default. `part`: when you wrote mid-turn, the turn is drawn in parts around your messages ({ from, to } ms);
 *  only the first part has the chip (its count is the whole turn's). `replied`: the chat has its reply (false when you
 *  cut it off: its last words then stay here, or they'd be nowhere). */
function stepsBlock(sid, turn, live, latest, part = { from: -Infinity, to: Infinity }, said = new Set(), replied = true, say = null) {
  // What a Stop hook made it do goes in its own fold at the end (hookFold); the rest is the turn.
  const hooked = turn.items.filter((x) => hookedPart(turn, x) && (x.t !== "say" || !said.has(String(x.text || "").trim())));
  let kept = turn.items.filter((x) => !hookedPart(turn, x));
  // A finished turn's last words are its reply, already in the chat; so are words the chat shows as a
  // reply mid-turn: not twice.
  if (!live && replied) { let end = kept.length; while (end && kept[end - 1].t === "say") end--; kept = kept.slice(0, end); }
  kept = kept.filter((x) => x.t !== "say" || !said.has(String(x.text || "").trim()));
  const steps = kept.filter((x) => x.t === "step");
  const first = part.from === -Infinity;
  const fold = part.to === Infinity ? hookFold(sid, turn, hooked) : "";
  if (!steps.length) return fold ? `<div class="stx" data-key="${esc(`t:${turn.at_ms}:${part.from}`)}">${fold}</div>` : "";
  const items = kept.map((x, i) => [x, i]).filter(([x]) => x.at_ms >= part.from && x.at_ms < part.to);
  const key = `${sid}|${turn.at_ms}`;
  const open = turnOpen(key, live || latest);
  const all = turnAllOpen(key);
  const count = `${steps.length}${turn.more ? "+" : ""} step${steps.length === 1 ? "" : "s"}`;
  const bk = `t:${turn.at_ms}:${part.from}`;
  const head = `<div class="stx-head" data-key="head"><button class="stx-fold" data-turn-fold="${esc(key)}" data-open="${open ? 1 : 0}" aria-expanded="${open}">${live ? `<span class="dot-live"></span>` : `<span class="stx-tri">${open ? "▾" : "▸"}</span>`}<b>${live ? `Working · ${count}` : count}</b>${open ? "" : `<span class="stx-sum"> · ${esc(turnSummary(steps))}</span>`}</button>${open ? `<button class="stx-all" data-turn-all="${esc(key)}">${all ? "Show less" : "Show all"}</button>` : ""}</div>`;
  if (!items.length) return first || fold ? `<div class="stx" data-key="${esc(bk)}">${first ? head : ""}${fold}</div>` : "";
  let body = "", run = [], runKey = "";
  // A run of steps is keyed by its first step: a paragraph between steps starts the next run.
  const flush = () => { if (run.length) body += `<div class="stx-steps" data-key="run:${esc(runKey)}">${run.join("")}</div>`; run = []; };
  // Over: under the step that started it. A teammate stays in the list above the box instead.
  const finished = new Map(helpersOf(sid).filter((h) => !helperRunning(h) && !h.teammate).map((h) => [h.step, h]));
  items.forEach(([x, i]) => {
    // Folded: only its steps go; what it said to you stays (a fold never hides its words).
    if (x.t !== "say") { if (!open) return; if (!run.length) runKey = x.id; run.push(stepLine(sid, live || x.done ? x : { ...x, stopped: true }, false, all)); const h = finished.get(x.id); if (h) run.push(bgRow(sid, h)); return; }
    flush();
    // New words unroll top to bottom, quicker for short ones.
    const ms = Math.min(REVEAL_MS, 250 + x.text.length * 2);
    body += `<div class="stx-say msg" data-key="${esc(itemKey(turn, x, i))}" style="--rv:${ms}ms">${md(say ? say(x.text) : x.text)}</div>`;
  });
  flush();
  if (!first) return body || fold ? `<div class="stx" data-key="${esc(bk)}">${body}${fold}</div>` : "";
  return `<div class="stx" data-key="${esc(bk)}">${head}${open && turn.more ? `<div class="stx-more dim" data-key="more">${turn.more} earlier steps not shown</div>` : ""}${body}${fold}</div>`;
}
/** Whether a turn's steps are open: your choice, else open for the newest turn (`fresh`), or every turn with
 *  Settings → Steps "Everything"; "Hidden" draws none. */
const turnOpen = (key, fresh = false) => (turnFold.has(key) ? turnFold.get(key) : stepsMode() === "all" || (stepsMode() === "line" && fresh));
/** Whether a turn's steps each show their output or diff ("Show all", or Settings → Steps "Everything"). */
const turnAllOpen = (key) => stepsMode() === "all" || turnAll.has(key);
/** A turn's "After a stop hook": folded, what the hook made it do and write (a checklist, the commands it
 *  ran for it), so it never reads as the answer, live or after. "" when no hook sent it back. */
function hookFold(sid, turn, hooked) {
  if (!hooked.length) return "";
  const key = `${sid}|${turn.at_ms}`, open = hookOpen.has(key);
  const busy = hooked.some((x) => x.t === "step" && !x.done);
  const body = open ? hooked.map((x, i) => x.t === "step" ? `<div class="stx-steps">${stepLine(sid, x)}</div>` : `<div class="stx-say msg" data-key="${esc(itemKey(turn, x, i))}">${md(x.text)}</div>`).join("") : "";
  return `<div class="follow stx-hook" data-key="hook"><button class="thread-toggle" data-hook-fold="${esc(key)}">${busy ? `<span class="dot-live"></span>` : ""}After a stop hook ${open ? "▴" : "▾"}</button>${open ? `<div class="follow-body">${body}</div>` : ""}</div>`;
}
/** A turn since your last message (`since`) has its "After a stop hook" in the chat, so the reply's own copy
 *  of it, from its card, isn't drawn as well. Since, not the last turn only: a background task reporting
 *  after the hook starts a turn of its own, and the hook's part stays in the one before. */
const hookInChat = (sid, since) => stepsMode() !== "hidden" && !!stepFeeds.get(sid)?.turns?.some((t) => t.hooked_ms && t.hooked_ms >= since);
/** The chat with each turn's steps put in by time: after your message, before the reply. `extra`:
 *  more lines to place by time (what you chose on a decision, when you chose it). `hides(at)`: a turn's steps that
 *  aren't drawn (a page's folded turns, chatHtml `page.folds`). `say(text)`: the words being written as a page shows
 *  them (chatHtml `page.say`). */
function withSteps(sid, s, ex, rows, extra = [], hides = null, say = null) {
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
      const next = turns[turns.indexOf(t) + 1]?.at_ms ?? Infinity;
      const replied = ex.some((e) => e.role === "agent" && !askLnOf(e) && e.at_ms >= start && e.at_ms < next);
      // The newest turn, and while that one works, the one before it too (cut off, its last step stays in view).
      const latest = t === lastTurn || (s?.state === "working" && t === turns.at(-2));
      const cuts = ex.map((e) => e.at_ms).concat(extra.filter((x) => x.cut).map((x) => x.at)).filter((a) => a > start && a <= end).sort((a, b) => a - b);
      const bounds = [-Infinity, ...cuts, Infinity];
      return cuts.concat([null]).map((_, k) => ({
        at: k === 0 ? start : cuts[k - 1] + 0.5,
        html: stepsBlock(sid, t, t === lastTurn && s?.state === "working", latest, { from: bounds[k], to: bounds[k + 1] }, said, replied, say),
      }));
    })
    .filter((b) => !hides?.(b.at))
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
/** "Not sent" for one of the agent's own screens, which Cue says to run in a terminal: a button that does, in Cue's
 *  own terminal in the session's folder (on this Mac only: one on a machine is run there). */
function runHereBtn(via, s) {
  const line = /run (.+?) in a terminal in /.exec(via)?.[1];
  if (!line || !s?.cwd || s.machine) return "";
  return ` <button class="mg-btn run-here" data-act="run-here" data-line="${esc(line)}" data-cwd="${esc(s.cwd)}">Run it in a terminal</button>`;
}

/** A crew member's review (the lead's reply to Cue's request) folds into that request's bubble: not a message of its
 *  own (its findings and buttons are on the member's row in the crew's tray, team.js). `reviews`: the replies folded
 *  (index → name); `reviewAt`: the request's index → its review. */
function reviewFolds(ex, team) {
  const reviews = reviewReplies(ex, (name) => [...team].reverse().find((m) => m.name === name)?.review || "");
  const reviewAt = new Map();
  for (const [k, name] of reviews) {
    for (let n = k - 1; n >= 0; n--) if (ex[n].role === "you" && cueSaid(ex[n].text)?.review) { reviewAt.set(n, { name, text: ex[k].text, team }); break; }
  }
  return { reviews, reviewAt };
}

/** `page`: a page that draws this chat as its own (the crew's page, team_page.js): `extra`, more rows of its own placed
 *  by time ({ at, html }, each keyed). It folds replies as the chat does. The Desk's lead chat (desk.js deskChat):
 *  `lines`, md's hook that draws the lead's own ITEM/ROUTE/DONE/CONTEXT lines; `noActs`, no Send to… on its messages (only a hand-off's inert tail, desk/earlier.js).
 *  `reply(e, key)`: { text, after } a reply drawn as the page splits it; `say(text)`: the words being written, likewise.
 *  `folds(ex)`: turns the page folds to a line of its own (an item's page, the holder's turns about other items):
 *  { skip: the messages not drawn, by index; rows: its fold lines, placed by time; hides(at): their steps }.
 *  `acts({ st, sid, held })`: a held or queued message's controls as the page has them (heldRow, queuedRow).
 *  `dkOut`: a Desk chat that doesn't draw the lead's lines (a session's, an item's): Cue's messages to it (hand-offs,
 *  sent back, landed…) leave it all the same, for Activity (#789).
 *  `pending`: what you sent from the page's own box that the thread doesn't show yet (the Desk's, #688), each drawn
 *  as the chat's own outbox is (outboxRow): { text, images, at, key, m: its msgLine state, head, picsHtml }. */
function chatHtml(it, s, harness, paged = false, page = null) {
  const original = conversation(it, s);
  const ex = original.map(deskSaidE);
  // An ask the hub noted ("Bash: …") is drawn as an ask with your answer beside it, not as its words (chatAsks).
  const { asks, answers, decided, hidden } = chatAsks(ex, asksUp(s?.session_id || it?.session_id));
  const lastAgent = ex.findLastIndex((e, n) => e.role === "agent" && !asks.has(n));
  const agentAt = ex.flatMap((e, n) => (e.role === "agent" && !asks.has(n) ? [n] : []));
  const recent = new Set(agentAt.slice(-OPEN_REPLIES));
  const project = s?.project || it?.project || "";
  const sid = s?.session_id || it?.session_id;
  // What you chose on a decision sits in the chat when you chose it (later steps come below it).
  const K = keyer();
  // …unless its ask line in the thread already shows that answer beside it. (An answer with no ask before it, a
  // terminal dialog's "chose 1. …", is no ask's: the result line says it, and that answer isn't drawn again.)
  const near = (at) => !!it?.resolved_ms && Math.abs(at - it.resolved_ms) < 5000;
  const shown = [...asks.values()].some((a) => a.answer && near(a.answer.at_ms));
  const chose = it && !isPending(it) && it.resolved_ms && !shown ? [{ at: it.resolved_ms, html: keyed(resultLine(it), "chose") }] : [];
  const ran = sid ? (ranLog.get(sid) || []).map((e, n) => ({ at: e.at, html: keyed(ranCard(sid, e, n), K(`ran:${e.at}`)) })) : [];
  const restarts = sid ? (state.restarted?.[sid] || []).map((r, n, all) => ({ at: r.at_ms, html: keyed(restartLine(sid, r, n === all.length - 1), K(`rs:${r.at_ms}`)) })) : [];
  const { reviews, reviewAt } = reviewFolds(ex, s?.team || []);
  const fold = page?.folds?.(ex);
  // Every chat draws its turns' steps (#897, the Desk lead's too), unless Settings → Steps hides them.
  const stepsOf = stepsMode() === "hidden" ? null : sid;
  // Which chat this is (chat/who.js chatPlace): the Desk's lead, session or item chat, else the Board's.
  const chat = page?.chat || (page?.lines ? "lead" : page?.dkOut ? "session" : "board");
  const c = { it, s, harness, paged, page, original, ex, asks, answers, decided, hidden, lastAgent, recent, project, sid, near,
    chose, reviews, reviewAt, fold, chat };
  const msgRows = ex.map((e, n) => keyed(chatRowHtml(c, e, n), K(`${e.role[0]}:${e.at_ms}`)));
  const drawn = withSteps(stepsOf, s, ex, msgRows, chose.concat(ran, restarts, page?.extra || [], fold?.rows || [], chatWaitRows(s, page, K)), fold?.hides, page?.say);
  // A Desk chat (#688, #789, desk/runs.js): the Desk's own rows leave it; Activity has them.
  const dkChat = (page?.lines || page?.dkOut) && typeof dkFoldRows === "function";
  const rows = dkChat ? dkFoldRows(drawn) : drawn;
  // …so a page of Cue's log can be all the Desk's (a run of its updates and the lead's answers to them, as after an
  // install, #869) and draw nothing: the chat loads the next page by itself until enough of the conversation shows,
  // and says Loading earlier messages… meanwhile, never "No messages yet" over messages still in the log.
  const filling = dkChat && paged && sid && fillOlder(sid, msgRows.filter((r) => r && !dkRow(r)).length);
  for (const o of outbox) if (o.sid === (s?.session_id || it?.session_id) && !landed(o)) rows.push(outboxRow(o, s, K(`out:${o.at}`), page));
  for (const o of page?.pending || []) rows.push(outboxRow(o, s, K(o.key || `p:${o.at}`), page));
  if (s?.send_at) rows.push(sendAtRow(s, page));
  return rows.length || filling ? (paged && sid && !page?.noOlder ? keyed(olderRow(sid, harness), "older") : "") + rows.join("") : ran.length ? ran.map((r) => r.html).join("") : `<div class="dim cv-empty" data-key="empty">No messages yet in this session.</div>`;
}
/** Message `e`, the `n`th of chatHtml's thread, as drawn there: an ask, a decision, a card of a page's or Cue's own, a
 *  bubble of yours or the agent's reply; "" for one not drawn. `c`: what chatHtml worked out for the whole thread. */
function chatRowHtml(c, e, n) {
  const { it, s, harness, page, asks, answers, decided, hidden, chose, near, reviews, reviewAt, fold, sid } = c;
  if (fold?.skip.has(n) || hidden.has(n)) return "";
  // Another Desk's band in the lead's chat goes to Activity (chat/who.js chatPlace, #837): marked the Desk's, it leaves.
  const out = (h) => (chatPlace(e, c.chat) === "activity" && typeof dkMark === "function" ? dkMark(h, "peer") : h);
  // From another Desk's session, in a lead's chat: a band across the chat (desk/lead.js, #472).
  if (e.role === "peer") return out((typeof deskPeerBand === "function" && deskPeerBand(sid, e)) || peerBubble(e.from || "another agent", "", e.at_ms, e.text));
  if (answers.has(n)) return "";
  if (decided.has(n)) return chose.length && near(e.at_ms) ? "" : chatDecisionLine(decided.get(n), e.at_ms);
  if (asks.has(n)) return chatAskHtml(`${s?.session_id || it?.id}:${e.at_ms}`, e, asks.get(n), harness, it?.machine || s?.machine);
  // Another Desk's lead, through Cue (TELL DESK, TELL DESKS, a HOLD, #540): the same band (desk/lead.js).
  const told = e.role === "you" && typeof deskToldBand === "function" && deskToldBand(sid, e);
  if (told) return chatFrom(e) === "cue" ? out(told) : told;
  // Who said it, from its tags (chat/who.js, #837): Cue's is its card, its quiet line, or Activity's in a Desk chat
  // (chatCueRow marks it so), never your bubble; its words only pick how it looks. A message from before the tags
  // goes on as it always did, below.
  const place = chatPlace(e, c.chat);
  if (place === "reply") return reviews.has(n) ? "" : chatAgentHtml(c, e, n);
  if (place !== "bubble") return chatCueRow(page, s, sid, e, n, reviewAt) || "";
  // Your send-back of an item ("Not done", #688): your bubble with the item's head, as your other words about it.
  if (e.role === "you" && youBack(e)) return chatYouHtml(c, e, n);
  // In a Desk chat, your words that went in one message with a session's words for the lead (#812: you wrote while it
  // was busy): your bubble with only yours, the lead's answer under it; the session's words are Activity's (runs.js).
  const mixed = e.role === "you" && (page?.lines || page?.dkOut) && typeof deskRelaySplit === "function" && deskRelaySplit(e.text);
  if (mixed?.yours) return chatYouHtml(c, { ...e, text: mixed.yours }, n);
  const cueRow = e.role === "you" && chatCueRow(page, s, sid, e, n, reviewAt);
  if (cueRow) return cueRow;
  if (reviews.has(n)) return "";
  const fwd = e.role === "you" && FORWARDED.exec(e.text);
  if (fwd) return peerBubble(fwd[2], "forwarded by you", e.at_ms, fwd[1], fwd[3]);
  if (e.role === "you") return chatYouHtml(c, e, n);
  return chatAgentHtml(c, e, n);
}
/** Your-side message `e` (the `n`th) when it's Cue's or a page's own, not your words: what Cue sent the lead for its
 *  crew, a page's own card, the Desk's hand-off, Cue's message to the session; "" for any other. Also how Activity
 *  draws them (desk/runs.js dkActivityRows), so the same message reads the same there. */
function chatCueRow(page, s, sid, e, n, reviewAt) {
  // What Cue sent the lead for its crew: Cue's bubble on the right, never yours (team.js). A page's own drawing first.
  // Each of these is the Desk's own (data-dk, desk/runs.js): it leaves the chat for Activity, unless it's meant for you.
  // Your pick (#667, "… picked option B2 on #d5 …") and your words Cue passed on (on its question, #651) are yours,
  // not the Desk's: they stay.
  // Marked Cue's (#837, chat/who.js): its tag says whether it stays (your words on Cue's question) or leaves; an older
  // message without the mark, its words.
  const mine = () => (chatFrom(e) === "cue" ? (chatPlace(e, "session") === "activity" ? "cue" : "yours")
    : /\bpicked option \S+ on #/.test(e.text || "") ? "picked" : typeof cueMsgOf === "function" && cueMsgOf(e.text)?.tag === "Your words" ? "yours" : "cue");
  const dk = (h, kind = mine()) => ((page?.lines || page?.dkOut) && typeof dkMark === "function" ? dkMark(h, kind) : h);
  const own = e.role === "you" && page?.you?.(e, n);
  if (own) return dk(own);
  // The Desk's hand-off of an item: a card, Cue's standing words to the session left out (desk/lead.js).
  const hand = e.role === "you" && typeof deskHandHtml === "function" && deskHandHtml(e);
  if (hand) return dk(hand, "route");
  const cue = e.role === "you" && cueSaid(e.text);
  if (cue) return dk(cueSaidLine(cue, `${sid}:${e.at_ms}`, e.text, { st: "delivered", to: msgTo(s), at: msgTaken(e) }, reviewAt.get(n)));
  // Cue's own message to the session (a send-back, a reopen, a refusal…, #478): one short plain line, its full text
  // folded under it (desk/cue_said.js).
  const cm = e.role === "you" && typeof cueMsgHtml === "function" && cueMsgHtml(e, s, `${sid}:${e.at_ms}:cue`, e.unsent ? { st: "failed", why: e.no_wake ? "it didn't wake for this" : "it may have run as a command, or still be in its box", to: msgTo(s) } : e.pending ? { ...(typedQueued(s) || { st: "sending" }), to: msgTo(s) } : { st: "delivered", to: msgTo(s), at: msgTaken(e) });
  if (cm) return dk(cm);
  // Any other message Cue sent it, marked Cue's by the hub as it went (#836): one quiet line of Cue's, never your bubble.
  return chatFrom(e) === "cue" && typeof cueQuietHtml === "function" ? dk(cueQuietHtml(e, s, `${sid}:${e.at_ms}:cueq`)) : "";
}
/** A page's own head on your words `e` (the Desk: what item they're about) and the words without it. Anywhere else (a
 *  session's own chat) your words from an item's page show the same way, Cue's words around them left out (#605). */
const youHeadOf = (page, e) => (page?.youHead ? page.youHead(e) : typeof deskAboutHead === "function" ? deskAboutHead(e) : null);
/** Message `e` is your send-back of a Desk item (desk/cue_said.js cueBackOf): drawn as yours, never as Cue's card. */
const youBack = (e) => typeof cueBackOf === "function" && !!cueBackOf(e?.text);
/** Send again on a message of yours that didn't go (`attrs`: what sends it, the board's resend or a page's own). */
const sendAgainBtn = (attrs) => `<button class="q-now" ${attrs}>Send again</button>`;
/** Your message (`text`) that session `sid` didn't wake for (#857): Try again wakes it once more; Send to a fresh session
 *  starts one in its place, from its last reply, with this message (the old one closes, Closed keeps it). */
const noWakeActs = (sid, text) => sid ? `${sendAgainBtn(`data-act="resend" data-sid="${esc(sid)}" data-resend="${esc(text)}"`).replace(">Send again<", ">Try again<")} <button class="q-now" data-act="fresh-send" data-sid="${esc(sid)}" data-text="${esc(text)}">Send to a fresh session</button>` : "";
/** Your message `e` (the `n`th) in chatHtml's thread: your bubble, with how it went. */
function chatYouHtml(c, e, n) {
  const { it, s, page, original, sid } = c;
  const how = lastSent && lastSent.sid === (s?.session_id || it?.session_id) && e.text.startsWith(lastSent.text) ? lastSent.via : "";
  // Queued in its agent ("queued via iTerm session"): it reads it at its next step. Sent another way: which.
  const queued = /^queued/.test(how), via = how && !queued ? ` · via ${esc(how)}` : "";
  // Typed into its terminal, but it never became a message: say so where you'll see it. Typed, and Cue hasn't yet
  // seen it take it: Queued while it's mid-turn (#531), else Sending until it does (past half a minute, that Cue
  // hasn't seen it yet).
  const resend = sid ? sendAgainBtn(`data-act="resend" data-sid="${esc(sid)}" data-resend="${esc(original[n].text)}"`) : "";
  // Cue started it again for this, and it ended or never took it (#857): said plainly, the message kept.
  const m = e.unsent && e.no_wake ? { st: "failed", head: `${esc(msgTo(s) || "It")} didn't wake`, why: "your message is kept here", acts: noWakeActs(sid, original[n].text) }
    : e.unsent ? { st: "failed", why: "it may have run as a command, or still be in its box", acts: resend }
    : e.pending ? typedQueued(s) || { st: "sending", note: Date.now() - e.at_ms > 30_000 ? "Sending… · Cue hasn't seen it take this yet" : "" }
    : { st: queued ? "queued" : "delivered", at: msgTaken(e) };
  // Sent now right after the one above, before it answered: the agent got both as one (sessions.rs note_with).
  const joined = e.joined ? " · went to the agent with your message above, as one" : "";
  // A page's own head on your words (the Desk: what item they're about), and the words without it.
  const hd = youHeadOf(page, e);
  return msgRow({ ...m, to: msgTo(s), extra: via + joined }, yourBubble(hd ? { ...e, text: hd.text } : e, `${sid}:${e.at_ms}`, "", false, hd?.html || "") + thumbs(e.images), `${e.unsent ? "unsent" : ""} ${e.pending ? "pending" : ""}`.trim());
}
/** The agent's reply `e` (the `n`th) in chatHtml's thread: its words, Listen, Read, Fold and Send to…. */
function chatAgentHtml(c, e, n) {
  const { it, s, harness, paged, page, ex, lastAgent, recent, project, sid } = c;
  const key = `${s?.session_id || it?.id}:${e.at_ms}`;
  // Long by what it shows: Cue's lines in it (core/md.js cueLineOf) are left out of it.
  const shown = e.text.split("\n").filter((l) => !cueLineOf(l));
  const long = shown.join("\n").length > 280 || shown.length > 4;
  const foldable = !recent.has(n) && long, folded = foldable && !openMsgs.has(key);
  const follow = it?.followup && e.text === it.message && !hookInChat(sid, ex.findLast((y) => y.role === "you" && y.at_ms < e.at_ms)?.at_ms ?? 0) ? followHtml(it) : "";
  // Listen and Read go together: a reply worth hearing is worth reading set larger (Read shows wherever Listen does).
  // A page's own split of a reply (the Desk's item chat: its own lines out, a stop hook's checklist folded under it).
  const split = page?.reply?.(e, key);
  const said = addOn.reply(sid, split ? split.text : e.text), listen = sid && proseWords(said.text, page?.lines) >= LISTEN_WORDS;
  // A Desk chat (#688): its Desk lines taken out for the count line around it; a reply of nothing but those leaves whole.
  const draw = (run) => md(said.text, { runIn: s?.session_id, lines: page?.lines, refs: page?.refs, run });
  const talk = page?.lines && typeof dkReplyTake === "function" ? dkReplyTake(draw) : null;
  const body = talk ? talk.html : draw(null);
  // The lead's answer to a turn of only Cue's notices (#816): in Activity under the notice, unless it acted on it; then
  // here, the notices one quiet line over it.
  const notice = talk && typeof dkNoticeReply === "function" ? dkNoticeReply(s, ex, n, page) : null;
  const out = (talk?.all && !split?.after && !said.after && !e.images?.length) || (notice && !notice.stays);
  const quiet = notice?.stays ? dkNoticeQuiet(s, ex, notice.turn, page) : "";
  return `<div class="cv-agent mg-agent ${folded ? "folded" : ""} ${n === lastAgent ? "last" : ""}"${out ? " data-dkr" : ""} data-at="${e.at_ms}">${quiet}<div class="mg-h">${mgAgentHead(harness, it?.machine || s?.machine, e.at_ms)}${n === lastAgent ? toldMeta(s) : ""}</div>
      <div class="msg mg-text cv-text">${body}</div>${split?.after || ""}${said.after}${thumbs(e.images)}${localImages(e.text)}
      <div class="mg-f cv-acts">${listen ? listenBtn(key) : ""}${listen && paged ? `<button class="mg-btn rd-pill" data-read="${esc(key)}">${BOOK_ICON}Read</button>` : ""}${foldable ? `<button class="mg-lk cv-more" data-msg="${esc(key)}">${folded ? "Show all" : "Fold"}</button>` : ""}${page?.noActs ? "" : fwdBtn(key, e.text, project, sid)}</div>${follow}</div>`;
}
/** "↗ Send to…" on an agent's words, a pill like Listen and Read, always shown, at the right end of their row (as on the Board): the one button every chat that talks to a session draws (#854): this chat's, the
 *  Board's, the Desk's item and session chats, a crew member's (helpers.js agentChatHtml) and a parked session's
 *  (parked.js). It keeps what it sends (fwdMsgs; `key` starts with the session's id and ":", so window.js keeps it
 *  while the session lasts), and a click opens the one menu (sheets.js forwardPop) over the same list of sessions. */
function fwdBtn(key, text, from, fromSid) {
  fwdMsgs.set(key, { text: addOn.plain(text), from, fromSid });
  return `<button class="mg-btn fwd-pill cv-fwd" data-fwd="${esc(key)}">${FWD_ICON}Send to…</button>`;
}
/** Send to…'s icon, the Board's ↗ drawn as an icon like Listen's and Read's beside it: the one every chat's Send to
 *  shows (fwdBtn), never drawn anywhere else. */
const FWD_ICON = `<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 17 17 7M9 7h8v8"/></svg>`;
/** The head row over an agent's words (#501, messages.css .mg-h): "Claude Code · 6m ago", its machine beside the name. */
const mgAgentHead = (harness, machine, at) => `<span class="mg-who">${esc(agentName(harness))}</span>${machTag(machine)} · ${ago(at)} ago`;
/** Sent while it worked: kept in Cue till the turn ends (yours to take back, Edit or Esc; to drop, Cancel; or to send
 *  now), or queued in its agent. Each sits where you wrote it, among the rest by time, keyed as its thread copy will
 *  be once it goes (`y:` and when you wrote it, the hub's written_ms): the same bubble, only its line changing, never
 *  moving from the foot into the thread (#422). (What Cue itself will send its crew's lead once it's free waits on the
 *  member's row in the crew tray, team.js: the chat shows only what was sent.) */
function chatWaitRows(s, page, K) {
  const out = [];
  // `cut`: a turn's steps split at it, as at a message in the thread, so the steps around it stay put when it goes.
  // A session's words for a Desk lead kept with yours (#831): never in your bubble, and alone, not in the chat (they
  // go as a message of their own, Activity's once sent).
  const relay = s?.held && typeof deskRelaySplit === "function" && deskRelaySplit(s.held.text);
  const held = relay ? (relay.yours ? { ...s, held: { ...s.held, text: relay.yours, cue_parts: [] } } : null) : s;
  // Cue's own words kept with yours (#836, the hub's cue_parts): your bubble holds only yours; Cue's are its quiet line,
  // going after yours, a message of their own. All Cue's with a card of its own (cue_said.js, team.js): that card.
  const parts = held?.held?.cue_parts || [];
  const cueText = parts.join("\n\n"), at = held?.held?.at_ms;
  const card = parts.length && typeof cueQuietHtml === "function" && !cueWithout(held.held.text, parts) && (cueSaid(cueText) || (typeof cueMsgOf === "function" && cueMsgOf(cueText)));
  if (parts.length && typeof cueQuietHtml === "function" && !card) {
    const yours = cueWithout(held.held.text, parts);
    if (yours) out.push({ at, cut: true, html: heldRow({ ...held, held: { ...held.held, text: yours, cue_parts: [] } }, page, K(`y:${at}`)) });
    out.push({ at, cut: true, html: keyed(cueQuietHtml({ ...held.held, text: cueText }, s, `${s.session_id}:held:cueq`, { st: "held" }), K(`yc:${at}`)) });
  } else if (held?.held) out.push({ at, cut: true, html: heldRow(held, page, K(`y:${at}`)) });
  // A kept message sent now and queued: where you wrote it, as its copy is drawn once read (written_ms).
  const qAt = s?.queued && (s.queued.written_ms && s.queued.written_ms < s.queued.at_ms ? s.queued.written_ms : s.queued.at_ms);
  if (s?.queued) out.push({ at: qAt, cut: true, html: queuedRow(s, page, K(`y:${qAt}`)) });
  return out;
}
/** A message you sent that its thread doesn't show yet: sending, sent and how, kept in Cue till its turn ends, or not
 *  sent and why. `key`: its row's key in the chat. A page's own (`page.pending`, the Desk's box) brings its state as
 *  `o.m`, and may bring its head (`o.head`) and its pictures' line (`o.picsHtml`); else its head is the page's
 *  (youHeadOf), as the thread's copy of it will have. */
function outboxRow(o, s, key, page = null) {
  const pics = o.images?.length ? `<div class="thumbs">${o.images.map((im) => `<span class="thumb"><img src="${esc(im.data)}" alt=""/></span>`).join("")}</div>` : o.picsHtml || "";
  const hd = o.head ? { html: o.head, text: o.text } : youHeadOf(page, o);
  const words = hd ? hd.text : o.text;
  const inner = `${words || hd ? `<div class="cv-you-text mg-text">${hd?.html || ""}${esc(words || "").replace(/\n/g, "<br>")}</div>` : ""}${pics}`, attrs = ` data-key="${esc(key)}"`;
  // A "/model" line: Cue set the model, nothing went to the agent.
  if (!o.m && /^not sent: Model set to /.test(o.via || "")) return `<div class="cv-you mg-you"${attrs}>${inner}<div class="mg-f"><span class="mg-st">${esc(o.via.slice(10))}</span></div></div>`;
  const m = o.m ? o.m : !o.via ? { st: "sending", note: esc(sendingMeta(o.sid)) } : /^kept in Cue/.test(o.via) ? { st: "held" }
    : /^not sent: /.test(o.via) ? { st: "failed", why: esc(o.via.slice(10)), extra: runHereBtn(o.via, s) }
    : /^queued/.test(o.via) ? { st: "queued" } : { st: "delivered", at: o.at, extra: ` · via ${esc(o.via)}` };
  return msgRow({ to: msgTo(s), sid: o.sid, ...m }, inner, "", attrs);
}
/** Who a message to session `s` is for, by its name. */
const msgTo = (s) => (s ? nameOf(s.session_id, s.project) : "");
/** When a message in a session's thread was taken (its delivery says), else when it was written. */
const msgTaken = (e) => (e.delivery || []).find((x) => String(x.what || "").startsWith("taken"))?.at_ms || e.sent_ms || e.at_ms;
/** Not gone yet, but drawn as its page's own card once sent (the Desk lead's new-item note, an item's hand-off): the
 *  same card now, only the line under it saying it hasn't gone (#370). "" for any other message. */
function laterOwn(e, page, key, meta) {
  e = deskSaidE(e);
  const hand = typeof deskHandHtml === "function" && deskHandHtml(e, meta);
  if (hand) return hand.replace(/^<div class="mg-you mg-hand"/, `<div class="mg-you mg-hand" data-key="${key}"`);
  const own = page?.you?.(e);
  return own ? `<div class="cv-later" data-key="${key}">${own}${meta}</div>` : "";
}
/** A message queued in its agent, read at its next step, with Send now. */
function queuedRow(s, page, key = "queued") {
  const sid = esc(s.session_id), now = `<button class="q-now" data-act="send-now" data-sid="${sid}">Send now</button>`;
  const m = { st: "queued", to: msgTo(s), acts: page?.acts?.({ st: "queued", sid: s.session_id }) ?? now };
  // Your send-back of an item (#688): yours, drawn below as any of your words; one marked Cue's (#837) never is.
  if (chatFrom(s.queued) === "cue" || !youBack(s.queued)) {
    const own = laterOwn(s.queued, page, key, msgLine(m));
    if (own) return own;
    const cue = cueSaid(s.queued.text);
    if (cue) return keyed(cueSaidLine({ ...cue, html: cueWill(cue.html) }, `${s.session_id}:queued`, s.queued.text, m), key);
    const cm = typeof cueMsgHtml === "function" && cueMsgHtml(s.queued, s, `${s.session_id}:queued:cue`, m);
    if (cm) return keyed(cm, key);
    // Any other of Cue's (#836): its quiet line, never your bubble.
    if (chatFrom(s.queued) === "cue" && typeof cueQuietHtml === "function") return keyed(cueQuietHtml(s.queued, s, `${s.session_id}:queued:cueq`, m), key);
  }
  const hd = youHeadOf(page, s.queued);
  return msgRow(m, yourBubble(hd ? { ...s.queued, text: hd.text } : s.queued, `${s.session_id}:queued`, "", true, hd?.html || "") + thumbs(s.queued.images), "queued", ` data-key="${esc(key)}"`);
}
/** A message kept for a session at work, to go when its turn ends: Cue's own crew message as Cue's bubble in the
 *  future tense (never one that looks like yours), yours as not sent yet; one that couldn't be sent says why. */
function heldRow(s, page, key = "held") {
  const heldWhy = s.held.unsent ? (s.held.delivery || []).map((d) => d.what).filter((w) => w.startsWith("couldn't send: ")).pop() : "";
  const sid = esc(s.session_id), now = `<button class="q-now" data-act="held-now" data-sid="${sid}">Send now</button>`;
  const acts = `<button class="q-now" data-act="held-edit" data-sid="${sid}">Edit</button> · ${now} · <button class="q-now" data-act="held-x" data-sid="${sid}">Cancel</button>`;
  const m = heldWhy ? { st: "failed", why: esc(heldWhy.slice(15)), acts, to: msgTo(s) } : { st: "held", acts, to: msgTo(s) };
  const mine = page?.acts?.({ st: m.st, held: true, sid: s.session_id });
  if (mine != null) m.acts = mine;
  // Your send-back of an item (#688): yours, drawn below as any of your words; one marked Cue's (#837) never is.
  if (chatFrom(s.held) === "cue" || !youBack(s.held)) {
    const own = !heldWhy && laterOwn(s.held, page, key, msgLine(m));
    if (own) return own;
    const cue = cueSaid(s.held.text);
    if (cue && !heldWhy) return keyed(cueSaidLine({ ...cue, html: cueWill(cue.html) }, `${s.session_id}:held`, s.held.text, m), key);
    const cm = typeof cueMsgHtml === "function" && cueMsgHtml(s.held, s, `${s.session_id}:held:cue`, m);
    if (cm) return keyed(cm, key);
    // Any other of Cue's (#836, #837): its quiet line, never your bubble.
    if (chatFrom(s.held) === "cue" && typeof cueQuietHtml === "function") return keyed(cueQuietHtml(s.held, s, `${s.session_id}:held:cueq`, m), key);
  }
  const hd = youHeadOf(page, s.held);
  return msgRow(m, yourBubble(hd ? { ...s.held, text: hd.text } : s.held, `${s.session_id}:held`, "", true, hd?.html || "") + thumbs(s.held.images), "queued held", ` data-key="${esc(key)}"`);
}
/** A message Cue sends by itself later: at your time, or (Cue's carry-on) when the usage limit it stopped on resets.
 *  Its words under the page's head when they're about an item (youHeadOf), as your other words. */
function sendAtRow(s, page = null) {
  const l = s.send_at, sid = esc(s.session_id);
  const at = new Date(l.at_ms), hm = at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const day = at.toDateString() === new Date().toDateString() ? "today" : "tomorrow";
  const resets = s.limit?.resets_ms ? ` (${new Date(s.limit.resets_ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })})` : "";
  const when = l.failed ? `Couldn't send it: ${esc(l.failed)}` : l.at_ms ? `Not sent yet · Cue sends it ${day} at ${hm}` : `Not sent yet · Cue sends it when your usage limit resets${resets}`;
  const acts = `${l.at_ms ? `<button class="q-now" data-act="send-at-edit" data-sid="${sid}">Edit</button> · ` : ""}<button class="q-now" data-act="send-at-x" data-sid="${sid}">Cancel</button> · <button class="q-now" data-act="send-at-now" data-sid="${sid}">Send now</button>`;
  const e = { role: "you", text: l.text, at_ms: l.at_ms || Date.now() }, hd = youHeadOf(page, e);
  return `<div class="cv-you mg-you queued held" data-key="send-at">${yourBubble(hd ? { ...e, text: hd.text } : e, `${s.session_id}:send-at`, "", true, hd?.html || "")}<div class="mg-f"><span class="mg-st${l.failed ? " failed" : ""}">${when}</span><span class="mg-acts">${acts}</span></div></div>`;
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
  const list = known?.length ? known.map((m) => `<span class="rs-sv-it"><b>${esc(m.name)}</b>${mcpLiveWord(m, sid)}</span>`).join("")
    : r.servers?.length ? r.servers.map((s) => `<span class="rs-sv-it"><b>${esc(s)}</b></span>`).join("") : "";
  const later = latest && !known?.length ? `<div class="rs-sv">Whether each one connected shows after its next message</div>` : "";
  return `<div class="cv-result rs-line"><span class="sent-check">✓</span><span>${title}<span class="dim"> · ${r.secs}s</span></span>${when}${list ? `<div class="rs-sv">${list}</div>` : ""}${later}</div>`;
}
/** Under the chat: where the session is now, and while any of its background agents run, their tray, whatever
 *  state it's in (working on your message, your turn, waiting on them), so they never drop out of sight. */
function statusLine(it, s) {
  const tray = s?.session_id ? agentTray(s.session_id) : "";
  // Waiting on them: the tray says it all, unless it's compacting (or just did): that line shows above it.
  if (tray && s.state === "agent" && s.helpers && !s.compacting_ms && !s.compacted_ms) return tray;
  // A hand-off on its way, a note written ahead, a cache gone (insights.js): lines here, never a bar on the box.
  return (s?.session_id ? handoffStatus(s.session_id, s) : "") + whereNow(it, s) + tray;
}
/** "Tell me" on the turn that just ended: on its reply's meta line (no line of its own under the chat), the
 *  tada and how long it took, until the next turn starts. While it runs, the tada in place of the title's dot
 *  says Cue is watching; nothing else. */
function toldMeta(s) {
  if (!s?.told_ms || atWork(s)) return "";
  return ` · <span class="told-meta">${tadaIcon(11, true)}${s.told_ms >= 1000 ? `took ${tookText(s.told_ms)}` : "done"}</span>`;
}
/** How long a turn ran: "45s", "38m", "1h 52m" (as its notification says it). */
const tookText = (ms) => { const sec = Math.floor(ms / 1000); return sec < 60 ? `${sec}s` : sec < 3600 ? `${Math.floor(sec / 60)}m` : `${Math.floor(sec / 3600)}h ${Math.floor(sec % 3600 / 60)}m`; };
/** What a session waiting on you waits for (#410), from its open card: your answer to its question, your OK for a
 *  step, else your reply. */
function waitingFor(it, s) {
  // Drawn without its card (the Desk's chats): the session's newest open one.
  if (!it || it.status !== "pending") it = state.items.filter((i) => i.status === "pending" && i.session_id === s?.session_id).sort((a, b) => b.created_ms - a.created_ms)[0] || it;
  const cut = (t) => { t = plain(t); return t.length > 100 ? t.slice(0, 100).replace(/\s+\S*$/, "") + "…" : t; };
  if (it?.kind === "question") { const q = cut(summary(it)); return q && q !== "Question" ? `your answer: ${q}` : "your answer to its question"; }
  if (it?.kind === "permission") { const d = it.tool_input?.description; const w = cut(typeof d === "string" && d.trim() ? d : summary(it)); return w ? `your OK to run: ${w}` : "your OK on a step"; }
  if (it?.kind === "input") return "you to fill in its form";
  return "your reply";
}
/** One line under the chat: where the session is now. */
function whereNow(it, s, steps = true) {
  // Working, but Claude Code can't reach the API (or the proxy in front of it) and keeps trying: minutes of a
  // spinner in its terminal otherwise. What failed, which try; Stop ends the turn instead of waiting it out.
  if (s?.state === "working" && s.retry) {
    const r = s.retry, head = r.network ? "Can't reach the API." : "The API returned an error.";
    return `<div class="mg-status cv-status stuck"><span class="lim-dot"></span><span><b>${head}</b> ${esc(r.error)}. Claude Code is trying again${r.max ? ` (${r.attempt} of ${r.max})` : ""}; messages you send wait until it gets through or gives up.</span><button class="btn small deny" data-act="interrupt" data-sid="${esc(s.session_id)}">Stop</button></div>`;
  }
  // Working, but nothing new from it in a while: a command waiting for input in its terminal, or hung.
  if (s?.state === "working" && s.stuck_ms) return `<div class="mg-status cv-status stuck"><span class="lim-dot"></span><span><b>No new output for <span data-ago="${s.stuck_ms}">${ago(s.stuck_ms)}</span>.</b> ${esc((s.doing || "Thinking").replace(/…$/, ""))}. If it's waiting for input, it's in its terminal.</span></div>`;
  // Compact (⋯) typed /compact: it's summarizing, until Claude Code says it's done.
  if (s?.compacting_ms) return `<div class="mg-status cv-status"><span class="dot-live"></span>Compacting: summarizing the conversation to free up context<span class="dim"> · <span data-ago="${s.compacting_ms}">${ago(s.compacting_ms)}</span></span></div>`;
  // It finished compacting (until the next turn starts).
  if (s?.compacted_ms && s.state !== "working") return `<div class="mg-status cv-status"><span class="ok-dot"></span>Compacted: the conversation was summarized and its context freed<span class="dim"> · <span data-ago="${s.compacted_ms}">${ago(s.compacted_ms)}</span> ago</span></div>`;
  // What it's doing right now (its latest step), else the prompt it's on.
  if (s?.state === "working" && s.doing) return `<div class="mg-status cv-status"><span class="dot-live"></span><span class="cv-what">${esc(s.doing)}</span><span class="dim"> · <span data-ago="${s.doing_ms}">${ago(s.doing_ms)}</span></span>${tipFor(it, s, "working")}</div>`;
  // Nothing in its transcript yet (it's thinking; Claude Code writes a message once it's whole): a live
  // count since the turn started, so a long think doesn't look stuck.
  const prompt = (typeof cueMsgPlain === "function" && cueMsgPlain(s?.prompt, msgTo(s))) || deskSaid(s?.prompt);
  if (s?.state === "working") return `<div class="mg-status cv-status"><span class="dot-live"></span><span class="cv-what">Working${prompt ? ` on: ${esc(prompt.length > 120 ? prompt.slice(0, 120) + "…" : prompt)}` : ""}</span><span class="dim"> · <span data-ago="${s.since_ms}">${ago(s.since_ms)}</span></span>${tipFor(it, s, "working")}</div>`;
  // Its turn over while its scripts (or, for a Desk lead, Cue's update) run on, or one failed since (#867).
  const bgWait = bgWaitLine(it, s);
  if (bgWait) return bgWait;
  if (s?.state === "waiting" || s?.state === "deciding") return `<div class="mg-status cv-status"><i class="sw z live"></i><span class="cv-what">Waiting on you for ${esc(waitingFor(it, s))}</span><span class="dim"> · ${ago(s.since_ms)}</span>${tipFor(it, s, "waiting")}</div>`;
  // A lead whose teammates wait on you, or that waits on teammates at work: named, not "1 background agent".
  const crew = s?.state === "agent" && s.session_id ? crewWaitLine(s.session_id) : "";
  if (crew) return `<div class="mg-status cv-status">${/^<span class="cv-what">Waiting on you/.test(crew) ? `<i class="sw z live"></i>` : ""}${crew}${tipFor(it, s, "working")}</div>`;
  // Waiting on its background agents: named and timed like any other line ("Waiting on inbox-removal and 1 more · 12m").
  if (s?.state === "agent" && s.helpers) {
    const run = helpersOf(s.session_id).filter(helperRunning), first = run[0];
    const who = first ? `${bgName(first)}${run.length > 1 ? ` and ${run.length - 1} more` : ""}` : s.driven_by;
    return `<div class="mg-status cv-status"><span class="dot-live"></span><span class="cv-what">Waiting on ${esc(who)}</span>${s.since_ms ? `<span class="dim"> · <span data-ago="${s.since_ms}">${ago(s.since_ms)}</span></span>` : ""}${tipFor(it, s, "working")}</div>`;
  }
  if (s?.state === "agent") return `<div class="mg-status cv-status"><span class="cv-what">Idle, waiting on ${esc(s.driven_by || "another agent")}</span>${tipFor(it, s, "working")}</div>`;
  if (s?.state === "limited" && s.limit) {
    const l = s.limit, what = prompt ? `“${esc(prompt.length > 80 ? prompt.slice(0, 80) + "…" : prompt)}”` : "Your last message";
    if (lifted(l)) return `<div class="mg-status cv-status limit"><span class="lim-dot back"></span><span><b>${esc(scopeName(l))} is back.</b> ${what} didn't run.</span><button class="btn small" data-usage="resend-one" data-sid="${esc(s.session_id)}">Resend</button></div>`;
    const when = l.resets_ms ? ` · resets ${clockAt(l.resets_ms)}${l.resets_ms - now() < 20 * 3600000 ? `, in ${untilText(l.resets_ms - now())}` : ""}` : "";
    const head = l.scope === "all" ? "Out of usage" : `${esc(l.scope)} used up`;
    const tail = l.scope === "all" ? "Cue will tell you when it resets." : "Switch models with /model to keep going.";
    return `<div class="mg-status cv-status limit"><span class="lim-dot"></span><span><b>${head}</b>${when}. ${what} didn't run. ${tail}</span></div>`;
  }
  // Cue quit (restarted, reinstalled) while a session it hosts was mid-turn: that turn ended there, not by you.
  if (s?.state === "stopped" && s.cut_by_restart) return `<div class="mg-status cv-status">Cue restarted mid-turn, so that turn ended · your next message starts it again</div>`;
  if (s?.state === "stopped") return `<div class="mg-status cv-status">Stopped by you · it's at its prompt</div>`;
  if (!s && it && !isPending(it)) return `<div class="mg-status cv-status">This session has ended.</div>`;
  return "";
}
/** The ask lines in a chat's thread (askLnOf), each with the answer right after it: `asks` (index → { ask, answer },
 *  the answer with its at_ms), `answers` (the indexes of those answers, not drawn on their own), `decided` (index →
 *  answer: one with no ask before it, a terminal dialog's choice or one whose ask scrolled out, drawn as a decision
 *  line, not as a message of yours). `hidden` (#801): asks not drawn, each shown once elsewhere: one asked again before
 *  any answer (the hook lost Cue mid-wait and asked anew: the later copy stands for it), and the newest one while its
 *  session's ask is up (`live`: asksUp), whose card under the chat is that ask. */
function chatAsks(ex, live = new Set()) {
  const asks = new Map(), answers = new Set(), decided = new Map(), hidden = new Set();
  ex.forEach((e, n) => {
    const ask = askLnOf(e);
    if (!ask) {
      const a = !answers.has(n) && askLnAnswer(e);
      if (a) decided.set(n, a);
      return;
    }
    const answer = askLnAnswer(ex[n + 1]);
    asks.set(n, { ask, answer: answer ? { ...answer, at_ms: ex[n + 1].at_ms } : null });
    if (answer) answers.add(n + 1);
  });
  for (const [n, { answer }] of asks) {
    if (answer) continue;
    let k = n + 1;
    while (k < ex.length && ex[k].role !== "you" && !(asks.has(k) && ex[k].text.trim() === ex[n].text.trim())) k++;
    if (k < ex.length && ex[k].role !== "you") hidden.add(n);
  }
  const lastYou = ex.findLastIndex((e) => e.role === "you");
  for (const [n, { answer }] of asks) if (!answer && n > lastYou && live.has(ex[n].text.trim())) hidden.add(n);
  return { asks, answers, decided, hidden };
}
/** The asks session `sid` has up now, as the hub words them in its thread (hub/asks.rs asked_line, history.rs summary):
 *  each one's card under the chat (askFoot) is that ask, so the chat doesn't draw it again (#801). */
function asksUp(sid) {
  const up = new Set();
  for (const i of sid ? state.items : []) {
    if (i.session_id !== sid || (i.kind !== "permission" && i.kind !== "question")) continue;
    const t = i.tool_input || {}, tool = String(i.tool_name || "");
    const what = i.kind === "question" ? `asked: ${t.questions?.[0]?.question ?? t.question ?? ""}`
      : `${tool}: ${{ bash: t.command ?? "", webfetch: `Fetch ${t.url ?? ""}` }[tool.toLowerCase()] ?? (["edit", "write", "multiedit", "notebookedit"].includes(tool.toLowerCase()) ? `${tool} ${t.file_path ?? ""}` : tool)}`;
    up.add(what.trim());
  }
  return up;
}
/** A decision as a line in the chat, in the result line's look: "✓ Chose 1. Yes", "✕ Denied". */
const chatDecisionLine = ({ text, no }, at, cls = no ? "no" : "") => `<div class="cv-result ${cls}"><span class="sent-check">${no ? "✕" : "✓"}</span><span>${esc(text)}</span><span class="grow"></span><span class="dim nowrap">${ago(at)} ago</span></div>`;
/** An ask the agent made, as the chat draws it. A tool's ask (#801) is one quiet line once it's settled: how you answered
 *  ("✓ Allowed", "✕ Denied", or "Not answered here" when it went unanswered in Cue), what it asked and the command's first
 *  line; while it waits, the card under the chat is that ask and the chat draws none (chatAsks). A question (#501,
 *  messages.css .mg.ask) stays a card, its words whole: "Claude Code · ASKS YOU · 6m ago", what it asked, and your
 *  answer as its foot's status, the card back to plain. */
function chatAskHtml(key, e, { ask, answer }, harness, machine) {
  if (ask.tool) {
    const mark = answer ? `<span class="sent-check">${answer.no ? "✕" : "✓"}</span>` : `<span class="sent-check none" aria-hidden="true">–</span>`;
    const line = String(ask.code || "").split("\n").find((l) => l.trim()) || "";
    const said = answer ? answer.text : "Not answered here";
    return `<div class="cv-ask cv-result ask-done${answer?.no ? " no" : ""}${answer ? "" : " none"}">${mark}<span class="ask-said">${esc(said)}</span><span class="ask-head">${esc(ask.head)}</span>${line ? `<code class="ask-line">${esc(line)}</code>` : ""}<span class="grow"></span><span class="dim nowrap">${ago((answer || e).at_ms)} ago</span></div>`;
  }
  const what = ` <span class="ask-q">${esc(ask.what)}</span>`;
  const st = answer ? `<span class="mg-st ask-ans${answer.no ? " failed no" : ""}"><span class="mg-mark" aria-hidden="true">${answer.no ? "✕" : "✓"}</span> ${esc(answer.text)}</span>` : "";
  return `<div class="cv-ask mg ask${answer ? " answered" : ""}"><div class="mg-h">${mgAgentHead(harness, machine, e.at_ms).replace(/ · (.*)$/, `<span class="mg-tag">${answer ? "Asked you" : "Asks you"}</span><span class="mg-age">$1</span>`)}</div>
    <div class="mg-t whole"><b class="ask-head">${esc(ask.head)}:</b>${what}</div>${st ? `<div class="mg-f">${st}</div>` : ""}</div>`;
}
/** After a decision: what you chose. Replies need no line, they're already in the chat. */
function resultLine(it) {
  if (!it || isPending(it) || !it.resolved_ms || it.kind === "waiting") return "";
  return chatDecisionLine({ text: outcomeText(it), no: outcomeClass(it) === "no" }, it.resolved_ms, outcomeClass(it));
}
function nextBar() {
  const n = nextUp();
  if (!n) return "";
  // × only on a finished turn; a decision would just hide something the agent is still blocked on.
  const clear = n.kind === "waiting" ? `<button class="next-clear" data-act="dismiss" data-id="${esc(n.id)}" title="Take it off Waiting" aria-label="Take it off Waiting">×</button>` : "";
  return `<div class="next-bar" data-act="next" role="button" tabindex="0"><span class="next-label">Next</span>${badge(n.harness)}<span class="next-proj">${esc(n.project)}</span><span class="next-what">${esc(plain(summary(n)))}</span>${clear}<kbd>N</kbd></div>`;
}

/** The chat header's ⋯: what's used now and then. Compact (any agent's /compact), and Decide later / Back to Waiting. */
let moreFor = null, movingProj = false;
/** + New project… at the foot of Move to project: the session whose list has the name box open (draft
 *  `newproj:<sid>`), and what the box last said back (a name already taken). */
let newProjFor = null, newProjNote = "";
/** ⋯ → Move to project: the projects it can show under instead, its folder's first, then + New project…. */
function projMoveList(sid) {
  const own = folderProject(sid), now = sessionOf(sid)?.project ?? findItem(active?.id)?.project ?? own;
  const names = [...new Set([own, ...projectsKnown().map(([p]) => p)])].filter((p) => p && p !== now);
  const rows = names.map((p) => `<button data-move-proj="${esc(p)}" data-sid="${esc(sid)}">${esc(projLabel(p))}${p === own ? "<span>Its folder's own</span>" : ""}</button>`).join("");
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
  newProjFor = null; newProjNote = ""; moreFor = null; movingProj = false;
  moveToProject(sid, name);
  // The window goes to the new project, the moved session still open in it.
  setProjFilter(name);
  renderMain();
}
function moreMenu(sid, s, closable = false) {
  const open = moreFor === sid;
  if (open && movingProj) return `<span class="more-wrap"><button class="btn more-btn" data-more="${esc(sid)}" aria-label="More" aria-expanded="true">⋯</button>${projMoveList(sid)}</span>`;
  if (open && mcpPicking) return `<span class="more-wrap"><button class="btn more-btn" data-more="${esc(sid)}" aria-label="More" aria-expanded="true">⋯</button>${mcpPicker(sid)}</span>`;
  const busy = s?.state === "working";
  const later = isParked(sid) ? `<button data-park="${esc(sid)}:0">Back to Waiting</button>` : `<button data-park="${esc(sid)}:1">Decide later<span>Move it to Need to decide while you think it over</span></button>`;
  const items = !open ? "" : `<div class="more-menu">
      <button data-cmd="compact" data-sid="${esc(sid)}" ${busy ? "disabled" : ""}>Compact<span>${busy ? "When this turn ends" : "Summarize the conversation to free up context"}</span></button>
      ${tellItem(sid, s)}
      ${crewMoreItem(sid)}
      ${typeof deskMemberItem === "function" ? deskMemberItem(sid, s) : ""}
      ${handoffMenu(sid, s)}
      ${addOn.chatMenu(sid, s)}
      <button data-move-ask="${esc(sid)}">Move to project…<span>Show its folder's sessions under another project in Cue; the agent stays in its folder</span></button>
      ${mcpItem(sid)}
      ${restartItem(sid, s)}
      ${forkItem(sid, s)}
      ${moveOutItem(sid, s)}
      ${later}${parkMenuItem(sid, s, closable)}
      <button data-delete-ask="${esc(sid)}" ${busy ? "disabled" : ""}>Delete…<span>${busy ? "When this turn ends" : "Gone for good: from Cue, and its conversation, so it can't be resumed"}</span></button></div>`;
  return `<span class="more-wrap"><button class="btn more-btn" data-more="${esc(sid)}" aria-label="More" aria-expanded="${open}">⋯</button>${items}</span>`;
}
/** ⋯ → Move to terminal, for a session Cue hosts: resumed where you run agents
 *  yourself (ipc/move_out.rs). Only for moving away from Cue, so it's here, not a button, and says so. Not while it
 *  works or asks you something. */
function moveOutItem(sid, s) {
  if (!s || !s.hosted || isDeskLead(sid)) return "";   // never a Desk's lead (#842)
  const why = s.state === "working" ? "When this turn ends" : s.state === "deciding" ? "Once you've answered what it asks" : "";
  const where = s.hosted && s.machine ? `in tmux on ${machName(s.machine)}` : `in a ${termName()} tab`;
  return `<button data-act="move-term" data-sid="${esc(sid)}" ${why ? "disabled" : ""}>Move to terminal<span>${why || `Only if you're moving away from Cue: the conversation goes on ${esc(where)}, and Cue stops running it`}</span></button>`;
}
/** ⋯ → Restart session: stopped and resumed with the same conversation, so it loads what it reads only at
 *  start (a skill added since, MCP servers, settings). Covered while it restarts, as an MCP restart is. */
function restartItem(sid, s) {
  if (!s || !["claude", "codex", "pi"].includes(s.harness) || !s.cwd) return "";
  const busy = s.state === "working";
  return `<button data-restart="${esc(sid)}" ${busy ? "disabled" : ""}>Restart session<span>${busy ? "When this turn ends" : "Same conversation, fresh start: picks up new skills, MCP servers and settings"}</span></button>`;
}
/** ⋯ → Fork, for a Claude Code session: its conversation goes on as a new session too (forkBtw). */
function forkItem(sid, s) {
  if (s?.harness !== "claude") return "";
  return `<button data-fork="${esc(sid)}" ${btwForking != null ? "disabled" : ""}>Fork<span>Its conversation goes on as a new session too, with -fork on its name; this one carries on as it is</span></button>`;
}
/** ⋯ → MCP servers…, for a Claude Code session on this Mac: how many it runs with. A hosted one's (here or on a
 *  machine) is Claude Code's own list (chat/mcp_live.js). */
function mcpItem(sid) {
  const s = sessionOf(sid);
  if (isHostedClaude(sid)) return mcplItem(sid);
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
/** How full the session's context is, in % (null until Cue knows its window: Claude Code's status line says it). */
const ctxPct = (m) => (m?.window ? Math.min(100, Math.round((m.context / m.window) * 100)) : null);
/** Just compacted and not at work again: its log's last count is from before, until a new reply brings a fresh one. */
const justCompacted = (s) => !!s?.compacted_ms && s.state !== "working";
/** The open session's model, how full its context is, how far it has grown, its cache and its cost so far (where
 *  the agent logs it), on the line under the chat's title; context as a small gauge. From its log. */
function metaInline(sid) {
  const m = sid && stepFeeds.get(sid)?.meta;
  if (!m || (!m.model && !m.context)) return "";
  const pct = ctxPct(m);
  const lvl = pct === null ? "" : pct >= 95 ? "full" : pct >= 80 ? "high" : "";
  const k = (n) => (n >= 1e6 ? `${(n / 1e6).toFixed(n % 1e6 ? 2 : 0)}M` : `${Math.round(n / 1000)}k`);
  const money = (c) => `$${c >= 100 ? Math.round(c) : c.toFixed(2)}`;
  const s = sessionOf(sid);
  const freed = justCompacted(s);
  // No share until Cue knows the window: the tokens alone, not a guess.
  const ctx = freed ? `<span class="mi-ctx">context freed</span>` : pct !== null ? `<span class="mi-ctx ${lvl}"><span class="ctx-bar"><i style="width:${pct}%"></i></span>${pct}% context</span>` : m.context ? `<span class="mi-ctx">${k(m.context)} context</span>` : "";
  // A fresh session due (insights.js handoffDue): how far it has grown, in amber.
  const grew = !freed && !state.skill_handoffs?.[sid] ? handoffGrew(sid) : 0;
  // Its cache's countdown (insights.js cacheMeta): the ring and the minutes, amber in its last 5; "cache gone" after.
  const cache = freed ? "" : cacheMeta(sid);
  // On a tight line they give way from the right, the cost first.
  const parts = [m.model ? `<b>${esc(prettyModel(m.model))}</b>${m.fast ? ` <span class="mi-fast">Fast</span>` : ""}` : "", ctx, grew ? `<span class="mi-grow">${Math.round(grew)}× its start</span>` : "", cache, m.cost != null ? money(m.cost) : ""].filter(Boolean);
  return `<span class="ap-meta">${parts.join(" · ")}</span>`;
}
/** The one thing to do now, on the chat's title row beside its state: Compact once its context is 80% full (not
 *  mid-turn; the same as ⋯ → Compact), then insights.js handoffChips (one chip, Fresh session ▾, whatever the state). */
function titleChips(sid) {
  const s = sessionOf(sid), pct = ctxPct(stepFeeds.get(sid)?.meta);
  const compact = pct !== null && pct >= 80 && s && s.state !== "working" && !s.compacting_ms && !justCompacted(s) ? `<button class="mi-compact" data-cmd="compact" data-sid="${esc(sid)}">${BULB}Compact</button>` : "";
  return compact + handoffChips(sid, s);
}
/** A quiet session open in Active: running, but started before Cue was connected, so it never
 *  reports to Cue. Read-only: what it's been doing, from its transcript, and how to make it a full one. */
let quietOpen = null;
function quietPane() {
  const q = (state.live || []).find((x) => x.session_id === quietOpen);
  if (!q) { quietOpen = null; return activePane(); }   // it ended (or reported in): back to the usual
  const f = stepFeeds.get(q.session_id);
  const busy = q.status === "busy";
  const turns = (f?.turns || []).slice(-8);
  const chat = turns.map((t, i) => quietTurn(q.session_id, t, busy && i === turns.length - 1, i === turns.length - 1, q.harness)).join("");
  return `<div class="active-pane ${busy ? "busy" : ""}">
    <div class="ap-head">${badge(q.harness)}<span class="proj">${esc(q.name || baseName(q.cwd) || "session")}</span><span class="pill soft">${busy ? "working" : "quiet"}</span><span class="grow"></span>
      ${moveBtn(q.session_id, q.harness, busy ? "working" : "idle", "btn primary")}<button class="btn" data-sv="tab" data-sid="${esc(q.session_id)}">${openInLabel(null)}</button></div>
    ${subHead(q.cwd, "", q.session_id)}
    <div class="quiet-note">It started before Cue was connected, so Cue can show what it's doing but can't answer it yet. In its terminal, type <code>/hooks</code> once to pick up Cue's hooks (or restart it with <code>claude --resume</code>; the conversation carries on), and you can reply, allow and answer from Cue.</div>
    ${chatBox(`q|${q.session_id}`, chat || `<div class="dim cv-empty" data-key="empty">${f ? "Nothing in its transcript yet." : "Reading its transcript…"}</div>`)}</div>`;
}
/** One turn of a quiet session: your message, its steps, its reply (all from the transcript). */
function quietTurn(sid, t, live, latest, harness = "claude") {
  const you = t.prompt ? `<div class="cv-you mg-you" data-key="y:${t.at_ms}"><div class="cv-you-text mg-text">${linkify(esc(t.prompt)).replace(/\n/g, "<br>")}</div><div class="mg-f"><span class="mg-st">You · ${ago(t.at_ms)} ago</span></div></div>` : "";
  // Its reply: the words after its last step (the steps leave a finished turn's last words to the chat).
  const tail = [];
  if (!live) for (let k = t.items.length - 1; k >= 0 && t.items[k].t === "say"; k--) tail.unshift(t.items[k]);
  const said = tail.length ? addOn.reply(sid, tail.map((x) => x.text).join("\n\n")) : null;
  const reply = said ? `<div class="cv-agent mg-agent" data-key="a:${t.at_ms}"><div class="mg-h">${mgAgentHead(harness, null, tail.at(-1).at_ms)}</div><div class="msg mg-text cv-text">${md(said.text)}</div>${said.after}</div>` : "";
  return you + stepsBlock(sid, t, live, latest) + reply;
}

/** Fewer words than this to read out and a reply has no Listen or Read: "Hi." or a line drawn as a card isn't worth it. */
const LISTEN_WORDS = 20;
/** How many words a reply has of its own, the lines a page draws as cards of its own (`lines`, as md's) left out. */
function proseWords(text, lines) {
  const kept = String(text || "").split("\n").filter((l) => l.trim() && !cueLineOf(l) && !(lines && lines(l, (x) => x)));
  return kept.join("\n").split(/\s+/).filter((w) => /\w/.test(w)).length;
}
