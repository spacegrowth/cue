// Cue UI desk — the chat card and its pages: Decisions, the lead's chat, a session's chat, one page pattern (deskPage) and its panel, an item page's parts (done when, mocks, checks, its ask, buttons, build log).
// Split from desk.js, which keeps the Desk's state and item basics; loaded after it (index.html, mini.html).
// ---------- the chat card: the lead's chat, Decisions, or what's open ----------

/** Decisions or steps as one quiet table: who | what | why (muted) | when | Undo (a small icon, shown on hover). The
 *  same line said again in a row is one row marked "×N" (its latest time and Undo). `about`: more for the muted line
 *  (the item a lead's decision was on). */
function deskLog(list, about = null) {
  const rows = [];
  for (const c of list) {
    const last = rows[rows.length - 1];
    if (last && last.c.text === c.text && (last.c.why || "") === (c.why || "") && last.c.call === c.call) { last.n++; if (deskMs(c.at) >= deskMs(last.c.at)) last.c = c; }
    else rows.push({ c, n: 1 });
  }
  return `<div class="dk-log">${rows.map(({ c, n }) => {
    const on = deskLogOpen.has(c.id), what = about?.(c) || "";
    return `<div class="dk-lr ${c.call ? "dk-dec" : "dk-step"}${on ? " open" : ""}" data-desk-log-row="${esc(c.id)}"><span class="dk-lby">${esc(c.by || "")}</span>`
      + `<span class="dk-d">${esc(c.text)}${n > 1 ? ` <span class="dk-x">×${n}</span>` : ""}${what ? ` <span class="dk-la">· ${what}</span>` : ""}</span>`
      + `<span class="dk-w">${c.why ? esc(c.why) : ""}</span><span class="dk-age">${ago(deskMs(c.at))} ago</span><span class="dk-lu">${deskTickBox(c)}${deskUndoLink(c)}</span></div>`;
  }).join("")}</div>`;
}
/** Rows of a Decisions or Steps table shown in full (a click on a row; long ones are cut at two lines). */
const deskLogOpen = new Set();
const DESK_UNDO_ICON = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/></svg>`;
/** Decisions you ticked to go back on together (their ids), whichever list they were ticked in. */
const deskTicked = new Set();
/** The lists showing housekeeping calls too (their keys): "show all" pressed there. */
const deskShowAll = new Set();
/** The list whose "Go back on these" box is open (its key), else null. */
let deskGoBackOpen = null;
const DESK_TICK_ICON = `<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5 10 17.5 19 7"/></svg>`;
/** A checkbox at a call's row end, to go back on it with others: only on a call that can still be undone. */
const deskTickBox = (c) => {
  if (!c.call || c.undo !== "can") return "";
  const on = deskTicked.has(c.id);
  return `<button class="dk-tick${on ? " on" : ""}" role="checkbox" aria-checked="${on}" aria-label="Go back on this" data-desk-tick="${esc(c.id)}">${on ? DESK_TICK_ICON : ""}</button>`;
};
/** Calls `list` drawn by `render` (deskLog or deskDecList) as a Decisions list `key`: key calls only (what gets built,
 *  how it looks or behaves, what's dropped), the housekeeping ones (DECIDED (minor): folders, routing, ordering,
 *  copies) behind a small "Show all"; and once some are ticked, "Go back on these", which asks for your new answer
 *  and sends each back to whoever decided it. `empty`: what shows with no calls at all. */
function deskCallsBox(list, key, render, empty) {
  if (!list.length) return empty;
  const all = deskShowAll.has(key), minor = list.filter((c) => c.minor).length;
  const shown = all ? list : list.filter((c) => !c.minor);
  const ticked = list.filter((c) => deskTicked.has(c.id) && c.undo === "can").length;
  const more = minor ? `<button class="dk-showall" data-desk-show-all="${esc(key)}">${all ? "Key decisions only" : `Show all · ${minor} more`}</button>` : "";
  const none = shown.length ? render(shown) : `<p class="dk-note">No key decisions yet.</p>`;
  return none + more + (ticked ? deskGoBackHtml(key, ticked) : "");
}
/** Under a Decisions list with `n` ticked: "Go back on these (n)", or, pressed, the box for your new answer. */
function deskGoBackHtml(key, n) {
  if (deskGoBackOpen !== key) return `<div class="dk-gb"><span class="dk-gb-n">${n} ticked</span><span class="grow"></span><button class="btn small" data-desk-go-back="clear">Clear</button><button class="btn primary small" data-desk-go-back="${esc(key)}">Go back on ${n === 1 ? "this" : `these (${n})`}</button></div>`;
  const label = "What should happen instead?";
  return boxHtml({ key: "goback", rows: 2, label, placeholder: "Your answer goes to each one with its decision", head: { label, cancel: 'data-desk-go-back="cancel"' }, sbClass: "dk-gb-box", drop: false, chips: false, mic: false, attach: false, send: { attrs: 'data-desk-go-back="send"' } });
}
/** Go back on every ticked call: each goes to whoever decided it, with your answer. */
async function deskGoBackSend() {
  const answer = draft("goback").text.trim();
  if (!answer) { toast("Say what should happen instead first", 4000); return document.querySelector('textarea[data-text="goback"]')?.focus(); }
  let sent = 0;
  for (const id of [...deskTicked]) {
    try { await invoke("desk_undo", { decisionId: id, answer }); deskTicked.delete(id); sent++; }
    catch (e) { toast(String(e), 5000); }
  }
  if (!deskTicked.size) { deskGoBackOpen = null; delete drafts.goback; saveDrafts(); }
  if (sent) toast(sent === 1 ? "Sent back with your answer" : `${sent} sent back with your answer`);
  renderMain();
}
/** Undo at a row's end: a small quiet icon shown while the pointer is on the row; "asked" once it was. */
const deskUndoLink = (c) => (c.undo === "can" ? `<button class="dk-undo-l" data-desk-undo="${esc(c.id)}" aria-label="Undo">${DESK_UNDO_ICON}</button>`
  : c.undo === "asked" ? `<span class="dk-u">Undo asked</span>` : "");
/** Lines Cue adds after your words on their way to the session (the hub's desk/hub/landing: a wait for a free copy,
 *  where its copy is, another machine; desk/mod.rs: your pictures): the session reads them, you don't see them. */
const DESK_CUE_TAIL = /\n(?:It waits for a free copy of |Work in its own copy of |You work on another machine: |Pictures you attached: )[\s\S]*$/;
/** Your words from an item's page as Cue sent them to a session or the lead (#605): "About #69 (its title)[, for x]: …"
 *  (a note, desk/hub/turn/act.rs) or "A question about Desk item #69 (its title), which is …: …\nAnswer it. …" (a
 *  question, desk/hub/turn/asked.rs), either maybe for a teammate. { id, title, words }: your words alone, Cue's around
 *  them left out. null for any other message. */
function deskYourWords(text) {
  const t = String(text || "").replace(/^For your teammate [^:\n]+: /, "");
  const m = /^(?:About|A question about Desk item) #([^\s(]+) \(/.exec(t);
  if (!m) return null;
  const ask = t.startsWith("A question"), rest = t.slice(m[0].length);
  const it = (state.desk?.projects || []).flatMap((d) => d.items).find((i) => deskIdIs(i, m[1]));
  // What closes the title: "): " (or "), for x: ") on a note, "), which is …: " on a question.
  const close = ask ? /^\), which is [^:\n]*: / : /^\)(?:, for [^:\n]+)?: /;
  // The title as sent; when the item is gone or renamed since, up to the first closing.
  let title = it && close.test(rest.slice(it.title.length)) && rest.startsWith(it.title) ? it.title : null;
  if (title == null) {
    const at = rest.search(ask ? /\), which is [^:\n]*: / : /\)(?:, for [^:\n]+)?: /);
    if (at < 0) return null;
    title = rest.slice(0, at);
  }
  let words = rest.slice(title.length).replace(close, "");
  if (ask) words = words.replace(/\nAnswer it\. If you change code for it, say DONE:[\s\S]*$/, "");
  // `crew`: for a teammate; `ask`: a question.
  const crew = t !== String(text || "") || !close.exec(rest.slice(title.length))?.[0].startsWith("): ");
  return { id: it?.id ?? m[1], it, title, ask, crew: !ask && crew, words: words.replace(DESK_CUE_TAIL, "").trim() };
}
/** Your words about an item (deskYourWords), or your send-back of it (cue_said.js cueBackOf, #688): a head over them
 *  with its kind's icon, its kind and number and its title (a click opens it, deskAboutBtn), and your words under it.
 *  null for any other words. */
function deskAboutHead(e) {
  const y = deskYourWords(e?.text) || (typeof cueBackOf === "function" && cueBackOf(e?.text));
  if (!y) return null;
  return { html: deskAboutBtn(y.id, y.it, y.title), text: y.words };
}
/** The head over your words about item `id` (`it` when it's still on the Desk; `title` as they named it). */
const deskAboutBtn = (id, it, title) => `<button class="dk-about" data-desk-chk-item="${esc(id)}">${deskKindIcon(it)}<b>${esc(it ? deskNum(it) : `#${id}`)}</b> · ${esc(title)}</button>`;
/** A chat the Desk shows: its earlier messages from Cue's log too (its thread keeps only the last few, all a restart
 *  starts with), loaded as it first shows, and Earlier messages above them, as in a session's chat (chat/thread.js). */
function deskChatRows(s, page) {
  const sid = s.session_id;
  if (!older.has(sid)) queueMicrotask(() => loadOlder(sid));
  const any = (s.thread || []).length || older.get(sid)?.msgs.length;
  // Nothing in its thread yet: only what you sent from the Desk's box (`page.pending`), drawn as the chat draws it.
  if (!any) return (page?.pending || []).map((o) => outboxRow(o, s, o.key || `p:${o.at}`, page)).join("");
  // Its Desk lines out, a count line for each run of them (desk/runs.js, #688). A lead's chat draws Cue's notes as its
  // short lines (#616) wherever it shows, an item's page too, so one message is drawn one way.
  return any ? chatHtml(null, s, s.harness || "claude", true, { lines: deskLineHtml, you: (state.desk?.projects || []).some((x) => deskLead(x)?.sid === sid) ? deskLeadNote : deskCueNote, youHead: deskAboutHead, noActs: true, ...page }) : "";
}
/** For ↑ ↓ in a Desk's box (chat/compose.js recallSent): the session it goes to ({ s, said }: your words as its chat
 *  shows them), the lead's for the lead's box; null for a box of no session's. */
function deskRecall(key) {
  const to = deskBoxes.get(key);
  if (!to) return null;
  if (to.sid) { const s = sessionOf(to.sid); return s ? { s } : null; }
  const d = deskOf(to.project), s = deskLead(d) && !deskLeadEnded(d) ? deskLeadSession(d) : null;
  return s ? { s } : null;
}
/** The session whose chat the Desk shows, when it's `sid`: its lead's (a parked one's kept messages too), or the one
 *  opened from the sessions on the left. Null otherwise. */
function deskShownChat(sid) {
  if (!deskSelection()) return null;
  if (deskOpen?.kind === "chat") return deskOpen.project === deskSelection() && deskOpen.sid === sid ? sessionOf(sid) : null;
  // An item's page shows its holder's chat from when it took the item (#575): that chat loads earlier messages till
  // that time is in (thread.js deskLoadStint).
  if (deskOpen?.kind === "item") { const i = deskItem(deskSelection(), deskOpen.id); return i && deskHolderSid(i) === sid && (i.stints || []).some((t) => t.sid === sid) ? sessionOf(sid) : null; }
  if (deskOpen || deskTab !== "lead" || deskPick === deskSelection()) return null;
  const d = deskOf(deskSelection());
  return deskLead(d)?.sid === sid && !deskLeadEnded(d) ? deskLeadSession(d) : null;
}
/** The session whose chat the Desk shows now (deskShownChat's), for talking to it by voice (app/render.js
 *  tellChatOpen, #617): its lead's (a parked one's too: sending resumes it), a session opened from the left, or an item's
 *  holder on its page. Null when it shows none. */
function deskShownSid() {
  const p = deskSelection();
  if (!p) return null;
  if (deskOpen?.kind === "chat") return deskOpen.project === p ? deskOpen.sid || null : null;
  if (deskOpen?.kind === "item") { const i = deskItem(p, deskOpen.id), sid = i && deskHolderSid(i); return sid && deskShownChat(sid) ? sid : null; }
  if (deskOpen || deskTab !== "lead" || deskPick === p) return null;
  const d = deskOf(p);
  return deskLead(d) && !deskLeadEnded(d) ? deskLead(d).sid : null;
}
/** Read on a reply in a Desk chat (chat/reader.js): the reader in the chat's place, its bar, the reply set larger at
 *  the size picked, and its foot (Listen's player; Back to chat while `ask` waits). Null when no reply of `s`'s is
 *  open in it. Not a .dk-chat: the Desk's chat-end scroll (deskSettle) leaves it alone. */
function deskReader(s, ask = null) {
  const sid = s?.session_id, rv = sid && readerViewed(sid, ask, s);
  if (!rv) return null;
  readerView.drawn = { it: ask, s };   // what ‹ › and Copy step through (chat/reader.js readerOf)
  const p = readerPane(sid, ask, s, s.harness || "claude", rv);
  return `${p.bar}<div class="dk-rd" data-scroll-key="rd:${esc(rv.all[rv.at].key)}">${p.body}</div>${p.foot}`;
}
/** The lead's chat: its session's messages, drawn as any session's chat is (chat/render.js), your words without the
 *  Desk's summary; then what you sent that it doesn't show yet, Sending… until its session has it: in its thread
 *  (delivered), or kept or queued in it (those rows say so, the moment its state has it, even before the send returns:
 *  never twice, #439). Given up ten minutes after the send returned, in case its
 *  words never match. */
function deskChat(d) {
  const s = deskLeadSession(d);
  const thread = s?.thread || [];
  const has = (e, p) => !!e && String(e.text || "").includes(p.text.slice(0, 200));
  const pending = (deskSent.get(d.project) || []).filter((p) => p.failed || !(p.sent && now() - p.sent > 600000)
    && !thread.some((e) => e.role === "you" && e.at_ms >= p.at - 60000 && has(e, p)) && !has(s?.held, p) && !has(s?.queued, p));
  deskSent.set(d.project, pending);
  // What you sent that it doesn't show yet: among its chat's own rows, drawn as the chat's outbox is (#688). One that
  // couldn't go: red, why, and Send again, as in a session's chat.
  const to = deskLeadName(d);
  const again = (p) => sendAgainBtn(`data-desk-act="lead_retry" data-id="${esc(d.project)}" data-at="${p.at}"`);
  const mine = pending.map((p) => ({ text: p.text, images: p.images, at: p.at, key: `p:${p.at}`, m: p.failed ? { st: "failed", why: esc(p.failed), acts: again(p), to } : { st: "sending", to } }));
  // The lead's own Desk lines drawn as lines, not raw text.
  const rows = s ? deskChatRows(s, { extra: deskCheckRows(d).concat(deskActRows(d)), noSteps: true, you: deskLeadNote, pending: mine })
    : mine.map((o) => outboxRow(o, null, o.key)).join("");
  // Where the lead is now (working on what, its latest step, stuck, compacting), as under any session's chat. Not while
  // a message of yours is on its way: "Waiting on you for your reply" under it would be wrong (#587).
  const sending = pending.some((p) => !p.failed);
  const where = s && sessionOf(s.session_id) && !sending ? whereNow(null, s, false) : "";
  // Each part keyed: the chat is kept in place across draws, brought up to date by key (chat/patch.js drawAround).
  const empty = !rows && !where ? `<div class="dk-empty" data-key="empty">Tell ${esc(deskLeadName(d))} about a bug, a feature or a task. It adds them here, hands each to a session, and says why.</div>` : "";
  // Ask it now, by the way (under a message Cue keeps for it): its panel over the chat, as in a session's chat.
  return `${s ? btwPanel(s.session_id) : ""}<div class="dk-chat" data-desk-chat="${esc(d.project)}">${empty}${rows}${where ? keyed(where, "where") : ""}</div>`;
}
/** What the lead did on its own (#360: closed, started, sent back, moved up, took back a copy, answered a session; #420: its
 *  routing and housekeeping calls, already in its chat as its own DECIDED lines), newest first. */
const deskActs = (d) => (d?.decisions || []).filter((x) => x.act && x.sid === deskLead(d)?.sid).sort((a, b) => b.at - a.at);
/** One of those as a line: its words, the item's number opening its page (the family's chip with `chip`, #501), and why
 *  (left out with `bare`). */
function deskActLine(x, d, bare, chip) {
  const id = x.item_id || "", it = id && deskItem(d.project, id), at = x.text.indexOf(`#${id}`);
  const num = chip ? `<button class="mg-id" data-desk-chk-item="${esc(id)}">${it ? deskKindIcon(it) : ""}#${esc(id)}</button>`
    : `<button class="dk-act-it" data-desk-chk-item="${esc(id)}">${it ? deskKindIcon(it) : ""}<b class="dk-inum">#${esc(id)}</b></button>`;
  const text = id && at >= 0 ? `${esc(x.text.slice(0, at))}${num}${esc(x.text.slice(at + id.length + 1))}` : esc(x.text);
  return bare ? text : `${text}${x.why ? `<span class="dk-act-why"> · ${esc(x.why)}</span>` : ""}`;
}
/** Those lines for the lead's chat (chat/render.js chatHtml `extra`), each at its time, as its other lines are
 *  (desk/lead.js deskLineHtml): one short line at the left ("Reopened #417 for cue-12 · 3m", #433), its why under it cut
 *  to two lines with More (#452: a reopen's feedback was its whole twelve lines). A call (#420) isn't repeated: the
 *  lead already said it in its reply. */
const deskActRows = (d) => deskActs(d).filter((x) => x.act !== "call").map((x) => {
  const key = `act:${x.id}`;
  const head = `<div class="mg-line"><span class="mg-lh">${deskActLine(x, d, true, true)}</span><span class="mg-age">${ago(x.at * 1000)}</span></div>`;
  return { at: x.at * 1000, html: keyed(`<div class="dk-act dk-ln dk-sgl" data-dk="${esc(x.act || "act")}" data-dk-id="${esc(x.item_id || "")}">${deskSign(x.act === "start" ? "start" : "act")}${head}${x.why ? deskCut(key, "mg-full", esc(x.why), x.why) : ""}${deskLoopCard(d, x)}</div>`, key) };
});
/** A loop the lead drafted (#583, its LOOP: line): the add-on's loop card for it (Start loop, Edit, Not now), as a
 *  chat's drafted loop shows; "" without the add-on. Drawn as the Desk's own, not the lead's chat's: Start gives the
 *  loop a session of its own in that project's folder, never the lead, which only dispatches. */
const deskLoopCard = (d, x) => (x.act === "loop" && x.draft ? addOn.reply(`desk:${d.project}`, `\`\`\`cue-loop\n${JSON.stringify(x.draft)}\n\`\`\``).after : "");
/** Activity (#374; "Dispatcher Actions" from #390 to #688): every move the lead made on its own, kept until you clear it. Today's come with the
 *  Desk's state; older ones load from Cue (desk_moves) as you scroll: { items, end, loading } a Desk each. */
const deskMovePages = new Map(), deskMovesShown = new Map();
const DESK_MOVES_STEP = 30;
/** Desk `d`'s moves, newest first: today's and those loaded, none from before you last cleared the list. */
function deskMovesList(d) {
  const from = d.moves_cleared || 0, byId = new Map();
  for (const x of (deskMovePages.get(d.project)?.items || []).concat((d.decisions || []).filter((x) => x.act))) if (x.at * 1000 > from) byId.set(x.id, x);
  return [...byId.values()].sort((a, b) => b.at - a.at || (a.id < b.id ? 1 : a.id > b.id ? -1 : 0));
}
/** The next moves older than the list's last. */
async function deskMovesFetch(project) {
  const pg = deskMovePages.get(project) || { items: [], end: false, loading: false };
  deskMovePages.set(project, pg);
  if (pg.end || pg.loading) return;
  pg.loading = true;
  const last = pg.items.at(-1);
  try {
    const got = await invoke("desk_moves", { project, beforeMs: last ? last.at * 1000 : null, beforeId: last?.id ?? null, take: DESK_MOVES_STEP * 2 });
    pg.items.push(...got);
    pg.end = got.length < DESK_MOVES_STEP * 2;
  } catch (e) { console.error(e); pg.end = true; }
  pg.loading = false;
  if (deskSelection() === project && view === "desk") renderMain();
}
/** The list's end came into view: more moves shown, the next ones fetched ahead. */
function deskMovesMore(project) {
  const pg = deskMovePages.get(project);
  const all = deskMovesList(deskOf(project)).length, n = deskMovesShown.get(project) || DESK_MOVES_STEP;
  if (all > n) { deskMovesShown.set(project, n + DESK_MOVES_STEP); renderMain(); }
  if (!pg?.end && all < n + DESK_MOVES_STEP * 2) deskMovesFetch(project);
}
let deskMovesSeen = null;
/** Watches the moves list's end (after each draw): in view, more. */
function deskMovesWatch() {
  deskMovesSeen?.disconnect();
  const end = document.querySelector(".dk-moves [data-desk-moves-more]"), box = end?.closest(".dk-pane");
  if (!end || typeof IntersectionObserver === "undefined") return;
  const project = end.dataset.deskMovesMore;
  deskMovesSeen = new IntersectionObserver((es) => { if (es.some((e) => e.isIntersecting)) { deskMovesSeen.disconnect(); deskMovesMore(project); } }, { root: box, rootMargin: "0px 0px 240px 0px" });
  deskMovesSeen.observe(end);
}
/** Clear pressed once: it asks before the list goes. */
let deskMovesClearing = null;
async function deskMovesClear(project) {
  if (deskMovesClearing !== project) { deskMovesClearing = project; renderMain(); return; }
  deskMovesClearing = null;
  try { await invoke("desk_moves_clear", { project }); } catch (e) { console.error(e); }
  deskMovePages.set(project, { items: [], end: true, loading: false });
  deskMovesShown.delete(project);
  renderMain();
}
/** Desk `d`'s Activity, newest first: the lead's own moves (deskMovesList, { x }) and every Desk line out of its chats
 *  (desk/runs.js dkActivityOf, { line }): the lead's, and each of its sessions', worked out from their messages. The
 *  lead's moves are in its chat as lines too (deskActRows): listed once, as moves; a routing call it said in its reply
 *  as a DECIDED line (#420) is listed once, as that line. Nothing from before you last cleared it. */
function deskActivity(d) {
  const lead = deskLead(d)?.sid, from = d.moves_cleared || 0;
  const sids = [...new Set([lead, ...(state.sessions || []).map((s) => s.session_id).filter((sid) => sid !== lead && deskHasSession(d, sid))])].filter(Boolean);
  const lines = sids.flatMap((sid) => dkActivityOf(sid).filter((r) => !/^act:/.test(r.key) && r.at > from).map((r) => ({ at: r.at, line: r })));
  // The calls its replies said (their DECIDED lines' words), so a call isn't listed again as a move.
  const said = new Set(lines.filter((r) => r.line.sid === lead).flatMap((r) => r.line.calls.map(deskCallWords)));
  const moves = deskMovesList(d).filter((x) => !(x.act === "call" && said.has(deskCallWords(esc(x.text)))));
  return moves.map((x) => ({ at: x.at * 1000, x })).concat(lines).sort((a, b) => b.at - a.at);
}
/** A call's words as its DECIDED line and its move both say them, to tell one from the other. */
const deskCallWords = (t) => String(t).replace(/\s+/g, " ").trim().replace(/[.;:]$/, "");
/** One Activity row in the panel (`n`: its index, for its key): what it was, on one line, and when. */
function deskActPanelRow(r, d) {
  const what = r.x ? deskActLine(r.x, d, true) : dkRowLine(r.line.html);
  const why = r.x?.why ? esc(r.x.why) : r.line && r.line.sid !== deskLead(d)?.sid ? `in ${esc(deskHolderName(r.line.sid))}'s chat` : "";
  return `<div class="dk-prow dk-pdid"><span class="dk-pdt">${what}</span><span class="dk-prs">${[why, r.at ? ago(r.at) : ""].filter(Boolean).join(" · ")}</span></div>`;
}
/** The panel's Activity (#688, in Just did's place, at its foot): the last 3 rows of `rows` (deskActivity's, or one
 *  chat's), then "N more in Activity ›" opening the tab. "" with none. */
function deskPanelActivity(rows, d) {
  if (!rows.length) return "";
  const more = rows.length > 3 ? `<button class="dk-pall" data-act="desk-activity">${rows.length - 3} more in Activity ›</button>` : `<button class="dk-pall" data-act="desk-activity">Activity ›</button>`;
  return deskPsec("Activity", `<div class="dk-prows">${rows.slice(0, 3).map((r) => deskActPanelRow(r, d)).join("")}${more}</div>`, "dk-pact");
}
/** The open page's own chat's Activity rows (an item's holder, a session's, a crew's lead), for its panel. */
function deskPageActivity() {
  const d = deskOpen && deskOf(deskOpen.project);
  if (!d) return "";
  const sid = deskOpen.kind === "item" ? deskHolderSid(deskItem(d.project, deskOpen.id)) : deskOpen.kind === "crew" ? deskOpen.lead : deskOpen.sid;
  return sid ? deskPanelActivity(dkActivityOf(sid).map((r) => ({ at: r.at, line: r })), d) : "";
}
/** The Activity tab (Dispatcher Actions until #688): what the lead did on its own and every Desk line taken out of the
 *  chats, newest first: when, the line, and why. */
function deskMoves(d) {
  if (!deskMovePages.has(d.project)) deskMovesFetch(d.project);
  const l = deskLead(d);
  const all = deskActivity(d), n = deskMovesShown.get(d.project) || DESK_MOVES_STEP, pg = deskMovePages.get(d.project);
  const asking = deskMovesClearing === d.project;
  const clear = all.length ? (asking ? `<span class="dk-mv-ask">Clear the whole list?</span><button class="btn small" data-desk-moves-keep>Keep</button><button class="btn small primary" data-desk-moves-clear="${esc(d.project)}">Clear</button>` : `<button class="btn small" data-desk-moves-clear="${esc(d.project)}">Clear</button>`) : "";
  const head = `<div class="dk-mv-h"><span class="dk-mv-t">Activity</span><span class="grow"></span>${clear}</div>`;
  const where = (r) => (r.line.sid !== l?.sid ? `<span class="dk-act-why">in ${esc(deskHolderName(r.line.sid))}'s chat</span>` : "");
  const rows = all.slice(0, n).map((r) => (r.x ? `<div class="dk-mv" data-key="mv:${esc(r.x.id)}"><span class="dk-mv-at">${esc(deskMoveWhen(r.at))}</span><span class="dk-mv-line">${deskActLine(r.x, d)}</span></div>`
    : `<div class="dk-mv dk-mv-ln" data-key="ln:${esc(r.line.sid)}:${esc(r.line.key)}"><span class="dk-mv-at">${r.at ? esc(deskMoveWhen(r.at)) : ""}</span><span class="dk-mv-line">${r.line.html}${where(r)}</span></div>`)).join("");
  const more = all.length > n || (pg && !pg.end) ? `<div class="dk-mv-more" data-desk-moves-more="${esc(d.project)}">${pg?.loading ? "Loading older actions…" : ""}</div>` : "";
  const empty = !all.length && !pg?.loading ? `<div class="dk-empty">Nothing yet. What ${esc(deskLeadName(d) || "the lead")} and Cue do on the Desk shows here: items given, added, closed, sent back or moved up, copies made, notes to sessions. It stays until you clear it.</div>` : "";
  return `<div class="dk-pane dk-moves" data-scroll-key="moves:${esc(d.project)}">${head}${empty}${rows}${more}</div>`;
}
/** When a move was: the time today, else the day and time. */
function deskMoveWhen(ms) {
  const t = new Date(ms), today = new Date(now());
  const hm = t.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  return t.toDateString() === today.toDateString() ? hm : `${t.toLocaleDateString([], { month: "short", day: "numeric" })}, ${hm}`;
}
/** A session's chat, opened from the sessions on the left: its name, Open its session, its messages and where it is
 *  now, as under any session's chat. */
/** What every chat on the Desk is (chat type mark, pick O): one solid label in the theme color, the kind's icon (the
 *  same icon the item list and About bubbles use) and its word, at the left of the header before the chat's name.
 *  `kind`: "desk" (the Desk's dispatcher), "session", "crew", "lead", "task", "bug" or "feature"; "" for none. `name`: the
 *  name's html. The same on every chat's head: the Dispatcher's, a session's, a holder's and a crew member's pane (#170).
 *  The left list keeps its own corner mark (Type marks pick 8, deskSessRow), not this one. */
const DESK_MARKS = { desk: "Dispatcher", session: "Session", crew: "Crew", lead: "Lead", task: "Task", bug: "Bug", feature: "Feature" };
/** Desk screens, one pattern (#200, pick B): every page in the middle card (an item, a session's chat, a crew member,
 *  a lead and its crew, the Dispatcher) is one header (back and the page's buttons, then its kind's icon, "Bug #197 · "
 *  quiet and its name, then one quiet line), the page's own column at the left (its chat or thread, the box under it)
 *  and a panel at the right: NOW (its state said once, a sentence, its buttons), then what else it is (DONE WHEN, WHO…).
 *  Hide panel folds the panel into one line under the header (its NOW: dot, word, counts, buttons), remembered by kind
 *  of page; a narrow card folds it on its own (desk-page.css). */
const DESK_PANEL_ICON = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M15 4v16"/></svg>`;
/** Kinds of page whose panel you hid ("item", "session", "crew", "lead", "dispatcher"), kept across restarts. */
const deskPanelHid = (() => { try { return new Set(JSON.parse(localStorage.getItem("cue.deskPanelHidden") || "[]")); } catch { return new Set(); } })();
/** On today's Desk page (B2, desk/b2.js) the lead's panel starts shown (#666); Hide panel puts its Now and Waits on you
 *  in the strip over the chat, remembered as "dispatcher-hidden". Every other page as before. */
const deskPanelKey = (page) => (page === "dispatcher" && deskB2On() ? "dispatcher-hidden" : page);
const deskPanelHidden = (page) => deskPanelHid.has(deskPanelKey(page)) || (page === "dispatcher" && deskB2On() && deskB2Narrow());
/** B2: the lead's chat card is too narrow for its panel (under 641px, desk-page.css's own rule), so the panel can't show
 *  and the strip takes its place, as with Hide panel. Read off the card in the chat's place as drawn last (else the
 *  board's width at the chat's share); a watcher on that card redraws when its width crosses the line. */
let deskB2NarrowDrawn = null;
const deskB2Sizer = typeof ResizeObserver === "function" ? new ResizeObserver((es) => {
  const w = es.at(-1)?.contentRect.width;
  if (view === "desk" && w > 0 && deskB2NarrowDrawn !== null && (w < 641) !== deskB2NarrowDrawn) renderMain();
}) : null;
function deskB2Narrow() {
  const card = document.querySelector("#app .dk-board.dk-b2 > .dk-card:is(.dk2b-chat, .dk2b-open)");
  const board = document.querySelector("#app .board");
  const w = card ? card.getBoundingClientRect().width : board ? (board.clientWidth - 28) * 1.9 / 3.75 : 0;
  deskB2NarrowDrawn = w > 0 && w < 641;
  // Watch the card this draw puts there (a new element each draw).
  if (deskB2Sizer) requestAnimationFrame(() => {
    const next = document.querySelector("#app .dk-board.dk-b2 > .dk-card.dk2b-chat");
    deskB2Sizer.disconnect();
    if (next) deskB2Sizer.observe(next);
  });
  return deskB2NarrowDrawn;
}
function deskPanelToggle(page) {
  if (!deskPanelHid.delete(page)) deskPanelHid.add(page);
  try { localStorage.setItem("cue.deskPanelHidden", JSON.stringify([...deskPanelHid])); } catch { /* kept for this window only */ }
  renderMain();
}
/** A panel section: its small head, then `html` ("" leaves it out). */
const deskPsec = (head, html, cls = "") => (html ? `<section class="dk-psec${cls ? ` ${cls}` : ""}"><h4 class="dk-psh">${head}</h4>${html}</section>` : "");
/** The panel's NOW: the state's dot (`dot`: "ask", "work", "stop", "done") and word, once; one plain sentence; its
 *  buttons. `fold`: the counts said after the word only when the panel is folded into a line. */
const deskPnow = ({ dot, word, say = "", acts = "", fold = "", box = false }) => {
  const body = `<div class="dk-pnr"><i class="dk-pdot ${dot}" aria-hidden="true"></i><b class="dk-pword">${esc(word)}</b>${fold ? `<span class="dk-pfc"> · ${fold}</span>` : ""}</div>${say ? `<div class="dk-psay">${say}</div>` : ""}${acts ? `<div class="dk-pacts">${acts}</div>` : ""}`;
  // `box`: all of it in a filled box at the panel's top (an item to check, #526); folded, the same one line.
  return `<section class="dk-psec dk-pnow"><h4 class="dk-psh">Now</h4>${box ? `<div class="dk-pbox">${body}</div>` : body}</section>`;
};
/** A WHO line: a quiet label, then its value. */
const deskPwho = (label, html) => (html ? `<div class="dk-pw"><span class="dk-pwl">${label}</span><span class="dk-pwv">${html}</span></div>` : "");
/** Item `i`'s number as a link to its page ("#191"). */
const deskItemLink = (i) => `<button class="dk-sch-t" data-desk-chk-item="${esc(i.id)}">${deskKindIcon(i)}#${esc(i.id)}</button>`;
/** Session `sid`'s name, opening its chat when Cue has it; else just the name, bold. */
const deskChatLink = (sid, name, d) => (sessionOf(sid) ? `<button class="dk-pwa" ${deskChatAttrs(sid, d)}>${esc(name)}</button>` : `<b>${esc(name)}</b>`);
/** One page: `page` the kind of page (what Hide panel remembers), `kind` its icon and mark (DESK_MARKS), `mark` the
 *  quiet words before its name ("Bug #197 · "), `title` its name's html, `meta` its quiet line, `now` and `panel` the
 *  panel's sections, `main` its own column and `foot` the box under it. */
function deskPage({ page, kind, back, acts = "", mark, title, meta = "", now = "", panel = "", main, foot = "", aside = "", activity = null }) {
  // `aside`: more attributes on its panel (an item's names the item, deskItemSide). `activity`: the panel's Activity at
  // its foot (#688); left out, the open page's own chat's (deskPageActivity).
  panel += activity ?? deskPageActivity();
  const hid = deskPanelHidden(page);
  const tog = `<button class="btn small dk-ptog" data-desk-panel="${deskPanelKey(page)}" aria-pressed="${!hid}">${DESK_PANEL_ICON}<span>${hid ? "Show panel" : "Hide panel"}</span></button>`;
  return `<div class="dk-pg${hid ? " hid" : ""}" data-desk-page="${page}"><div class="dk-ph"><div class="dk-ph1">${back}<span class="grow"></span>${acts}${tog}</div>`
    + `<h1 class="dk-ph2"><span class="dk-hk"${["desk", "session", "crew", "lead"].includes(kind) ? ` data-desk-mark="${kind}"` : ""}>${deskKindIcon(kind)}${esc(mark.replace(/ · $/, ""))}</span><span class="dk-hsep"> · </span><span class="dk-tname">${title}</span></h1>${meta ? `<div class="dk-ph3">${meta}</div>` : ""}</div>`
    + `<aside class="dk-panel"${aside}>${now}${panel}</aside><div class="dk-pgm">${main}${foot}</div></div>`;
}
/** A session's dot and word for the panel, from deskSessState. */
const deskPstate = (s) => { const [dot, word] = s ? deskSessWithHelpers(s) : ["idle", "idle"]; return [dot === "ask" ? "ask" : dot === "idle" ? "stop" : "work", word[0].toUpperCase() + word.slice(1)]; };
/** What session `sid` is on Desk `d`, as the left list's corner mark says it: "desk" (it runs the Desk: its
 *  Dispatcher, one name wherever it shows), "lead" (the Desk gave it a lead's work or it has a crew), else "session". */
const deskSessKind = (sid, d) => (sid === deskLead(d)?.sid ? "desk" : deskIsLead(sid, d.items.filter((i) => deskGroupKey(i.holder) === sid)) ? "lead" : "session");
/** The way back from a session's chat: to what opened it (an item's page, a holder's), else to the lead's chat. */
function deskChatBack(from, d) {
  const i = from?.kind === "item" && deskItem(d.project, from.id);
  const label = i ? deskNum(i) : from?.kind === "sessions" ? "All sessions" : from?.kind === "holder" ? deskHolderName(from.sid, d.items.find((x) => deskGroupKey(x.holder) === from.sid && x.state !== "done" && x.holder?.kind !== "crew")?.holder?.name) : deskB2On() ? deskB2ItemsWord(d) : deskLeadName(d) || "Back";
  return `<button class="dk-back" data-desk-chat-back>‹ ${esc(label)}</button>`;
}
function deskSessChat(sid, from, d, foot = "") {
  const s = sessionOf(sid);
  if (!s) return null;
  // A Desk's lead opened as a session: Cue's messages to it folded to a line, as in the Desk's chat (#616).
  const rows = deskChatRows(s, (state.desk?.projects || []).some((x) => deskLead(x)?.sid === sid) ? { you: deskLeadNote } : undefined);
  const where = whereNow(null, s);
  const empty = !rows && !where ? `<div class="dk-empty" data-key="empty">Nothing said yet.</div>` : "";
  const open = `<button class="btn small" data-desk-session="${esc(sid)}">Open its session</button>`;
  // One page (#200): back, Open its session; "Session · " (or Lead, Dispatcher: what the session is, never an item's
  // kind) and its name; quiet, the item it's on ("on #191 · its title", the number opening it). Its chat at the left,
  // its state, step and Stop in the panel, with every item it holds.
  const its = deskSessItems(d, sid), kind = deskSessKind(sid, d), on = its[0];
  const meta = on ? `on ${deskItemLink(on)} · ${esc(on.title)}${its.length > 1 ? ` · +${its.length - 1} more` : ""}` : s.since_ms ? `nothing on the Desk · since ${esc(clockAt(s.since_ms))}` : "nothing on the Desk";
  const [dot, word] = deskPstate(s);
  const say = dot === "ask" ? "It asks you something: answer under its chat." : dot === "work" ? (s.doing ? esc(s.doing) : "At work.") : "At its prompt: tell it what's next under its chat.";
  const stop = dot === "work" ? `<button class="btn small" data-act="interrupt" data-sid="${esc(sid)}">Stop</button>` : "";
  // The new Desk page (#628): back to the lead's chat, and the item it's on a click away.
  const v2 = deskV2 ? deskV2ChatBack(sid, from, d) : null;
  return deskPage({ page: kind === "desk" ? "dispatcher" : kind === "lead" ? "lead" : "session", kind, back: v2 ? v2.back : deskChatBack(from, d), acts: (v2 ? v2.its : "") + open,
    mark: `${DESK_MARKS[kind]} · `, title: deskSessName(sid, deskHolderName(sid), "dk-shn"), meta,
    now: deskPnow({ dot, word, say, acts: stop, fold: on ? `on #${esc(on.id)}` : "" }),
    panel: deskPsec("Who", deskPwho("On", its.length ? its.map(deskItemLink).join(", ") : "nothing yet")) + deskSessFiles(sid),
    main: `<div class="dk-chat" data-desk-chat="${esc(sid)}">${empty}${rows}${where ? keyed(where, "where") : ""}</div>`, foot });
}
/** The session whose steps the Desk follows live (chat/thread.js followSteps): its lead's, while its chat shows, or
 *  the session's whose chat is open. */
function deskFollowSid() {
  // An item's page: its holder's steps, for its chat (deskItemChat).
  if (deskOpen?.kind === "item" && deskOpen.project === deskSelection()) {
    const i = deskItem(deskOpen.project, deskOpen.id), sid = deskHolderSid(i), s = sid && sessionOf(sid);
    // Its holder's chat shows on it (#418): that chat's steps, as on the holder's own page.
    if (s) return sid;
  }
  if (deskOpen?.kind === "chat" && deskOpen.project === deskSelection()) return sessionOf(deskOpen.sid) ? deskOpen.sid : null;
  // A crew member's page: its lead's steps, where its own steps are (deskCrewPane's Doing).
  if (deskOpen?.kind === "crew" && deskOpen.project === deskSelection()) return sessionOf(deskOpen.lead) ? deskOpen.lead : null;
  if (!deskSelection() || deskOpen || deskTab !== "lead" || deskPick === deskSelection()) return null;
  const d = deskOf(deskSelection()), l = deskLead(d);
  return l && !deskLeadEnded(d) && sessionOf(l.sid) ? l.sid : null;
}
/** The Desk's judgment calls (DECIDED lines), the only decisions worth your look: routes, gives, sessions started,
 *  contexts kept and Cue's hand-off asks are steps, kept on their item (deskItemPane). */
const deskCalls = (d) => d.decisions.filter((c) => c.call);
/** The Desk's Decisions list (#420): a call on an item that's over (you checked it, or it was dropped) is settled, so it
 *  goes behind "Show all" with the housekeeping; it stays on its item's own page. */
const deskOpenCalls = (d) => deskCalls(d).map((c) => (!c.minor && ["done", "dropped"].includes(deskItem(d.project, c.item_id || "")?.state) ? { ...c, minor: true } : c));
function deskDecisions(d) {
  const calls = deskOpenCalls(d), empty = `<div class="dk-empty">No judgment calls yet. Routes and hand-offs are on each item.</div>`;
  if (!calls.length) return empty;
  return `<div class="dk-pane" data-scroll-key="decisions:${esc(d.project)}"><div class="dk-sec">${deskCallsBox(calls, `desk:${d.project}`, deskLog, "")}</div></div>`;
}
/** "Done when" as its points, one per line: its own lines (a "-", "*", "•" or "1." in front dropped), else its
 *  sentences and "; " parts. "e.g." / "i.e." / "etc." don't end one; a point's last "." goes. */
function deskDoneWhenPoints(t) {
  // Numbered in a line ("(1) … (2) …"): a point each.
  const lines = String(t || "").split(/\n+|\s*(?:^|\s)\(\d+\)\s+/).map((l) => l.replace(/^\s*(?:[-*•]|\d+[.)])\s+/, "").trim()).filter(Boolean);
  const out = [];
  for (const l of lines) {
    let cur = "";
    for (const part of l.split(/(?<=[.!?;]["”')]?)\s+/)) {
      cur = cur ? `${cur} ${part}` : part;
      if (!/\b(?:e\.g|i\.e|etc|vs)\.$|\b[A-Z]\.$/i.test(cur) || /[;!?]$/.test(cur)) { out.push(cur); cur = ""; }
    }
    if (cur) out.push(cur);
  }
  return out.map((p) => p.replace(/(?<!\.)[.;]$/, "").trim()).filter(Boolean);
}
/** "Done when" as a short checklist of what you'll see: a quiet ring, then the point, a line each. */
const deskDoneWhenHtml = (t, base = null) => {
  const pts = deskDoneWhenPoints(t);
  return pts.length ? `<ul class="dk-dwl">${pts.map((p) => `<li><i aria-hidden="true"></i><span>${linkify(esc(p), base)}</span></li>`).join("")}</ul>` : "";
};
/** Its designs' canvas (the last DESIGNS: link a session gave, desk/hub/core/desk.rs designs_said): "Open the mocks ↗", a
 *  plain link in the accent colour (Lead chats pick 1 + D: no outlined button), in its page's header (#178). */
const deskDesignsLink = (i) => /^https?:\/\//.test(i.designs || "") ? `<div class="dk-dsg">${deskDesignsA(i.designs)}</div>` : "";
const deskDesignsA = (url) => `<a class="dk-dlink" href="#" data-href="${esc(url)}">Open the mocks ↗</a>`;
/** Its mocks' canvas: the DESIGNS: link its session gave, or for an item whose work was only mocks (#408, #436), the
 *  link it made. "" for none. */
const deskCanvas = (i) => /^https?:\/\//.test(i?.designs || "") ? i.designs : i?.finish?.mocks ? (i.made || []).find((m) => /^https?:\/\//.test(m)) || "" : "";
/** Where an item's options can be picked: waiting on you, to check, or still with its holder, at work or finishing
 *  (#667: a pick closes it whatever stage it's at). */
const DESK_PICK_STATES = new Set(["needs_you", "to_check", "working", "finishing"]);
/** An item waiting on you to pick an option, one with a canvas or with what follows each option (IF PICKED: lines,
 *  #543): the line of its holder's newest words that asks you to, plain, for under its page's header ("Pick one for
 *  working items: D, E or F"). Its question first while it asks you, then its latest reply. An earlier pick can
 *  already have been answered. */
function deskPickLine(i) {
  if (i.picked || (!deskCanvas(i) && !(i.next || []).length) || !DESK_PICK_STATES.has(i.state)) return "";
  for (const t of [i.state === "needs_you" ? deskQuestion(i) : "", i.said?.at(-1)?.text || ""]) {
    const line = deskPickLineIn(t);
    if (line) return line;
  }
  return "";
}
/** The line of `t` that asks you to pick (deskPickLine): one starting "Pick", "Choose" or "Which", else a question
 *  that says pick, choose or "which option", else the first that does (#539: "Which option do you want built: A, B or
 *  C?" says neither pick nor choose). Line by line before its markdown goes (plain() can run lines together), then
 *  sentence by sentence. "" for none. */
function deskPickLineIn(t) {
  const lines = String(t || "").split(/\n+/).flatMap((l) => plain(l).split(/(?<=[.?!])\s+/)).map((x) => x.trim())
    .filter((x) => /\b(pick|choose)\b|\bwhich\s+(option|one|mock|way)s?\b/i.test(x));
  const line = lines.find((x) => /^(pick|choose|which)\b/i.test(x)) || lines.find((x) => x.endsWith("?")) || lines[0];
  return line ? line.replace(/^(not done yet|still to do)\s*:\s*/i, "").replace(/^./, (c) => c.toUpperCase()) : "";
}
/** The options its newest reply lists, each on a line of its own, its key first ("A: compact rows", "**M2** — a
 *  strip", "- Option C) …", "4. a calm list"): Map key → its words, cut short (#445, #619). */
function deskOptionWords(i) {
  const out = new Map();
  for (const l of (i.said?.at(-1)?.text || "").split("\n")) {
    // Its key, then a note on it ("B (suggested)", #539), then its mark: a colon, dot, bracket or dash, or its bold's
    // end; a comma too inside bold ("**A, heads-up only:** …").
    const m = l.match(/^\s*(?:[-*•]\s+)?(\*\*|__)?(?:option\s+)?([A-Z]\d{0,2}|\d{1,2}(?!\d))(?:\s*\(([^)]{1,30})\))?(.*)$/i);
    if (!m || (m[2].length === 1 && m[2] !== m[2].toUpperCase())) continue;
    const mark = m[4].match(m[1] ? /^\s*(?:\*\*|__)?\s*[:.)\u2014\u2013,-]|^\s*(?:\*\*|__)/ : /^\s*[:.)\u2014\u2013-]/);
    if (!mark) continue;
    const words = plain(m[4].slice(mark[0].length)).replace(/^[\s:.)\u2014\u2013-]+/, "").trim();
    const note = m[3] && !words.toLowerCase().includes(m[3].toLowerCase()) ? ` (${m[3].trim()})` : "", max = 70 - note.length;
    const k = m[2].toUpperCase();
    if (words && !out.has(k)) out.set(k, `${words.length > max ? `${words.slice(0, max - 1).trimEnd()}…` : words}${note}`);
  }
  return out;
}
/** An option's key: A, B… M1, M2… or 1, 2… 10 (#619). */
const DESK_OPT = String.raw`(?:\b[A-Z]\d{0,2}\b|\b\d{1,2}\b)`;
/** The options a pick line runs through: "D, E or F", "4, 6, 8 and 9", else a range, "1 to 10", "A–F" (#619). */
function deskPickRun(line) {
  const run = line.match(new RegExp(String.raw`${DESK_OPT}(?:\s*(?:,|\/|\bor\b|\band\b)\s*(?:(?:or|and)\s+)?${DESK_OPT})+`));
  if (run) return [...new Set(run[0].match(new RegExp(DESK_OPT, "g")))];
  const range = line.match(new RegExp(String.raw`\b([A-Z]|\d{1,2})\s*(?:to|-|\u2013|\u2014)\s*([A-Z]|\d{1,2})\b`));
  if (range && /\d/.test(range[1]) === /\d/.test(range[2])) {
    const num = /\d/.test(range[1]), [a, b] = [range[1], range[2]].map((k) => (num ? +k : k.charCodeAt(0)));
    if (b > a && b - a < 20) return Array.from({ length: b - a + 1 }, (_, k) => (num ? String(a + k) : String.fromCharCode(a + k)));
  }
  return [];
}
/** The options an item asks you to pick from, A, B, C…, M1, M2… or 1, 2… (#445, #543, #619): those its pick line
 *  names ("… working items: D, E or F" → D, E, F; "Which option, 1 to 10?"); else, for one with a canvas or with
 *  what follows each option (IF PICKED: lines), the two or more its newest reply lists one to a line, or those its
 *  IF PICKED: lines name: those when the list shares none of them (a numbered list that isn't its options); none once
 *  picked. */
function deskPickOptions(i) {
  if (i.picked || !DESK_PICK_STATES.has(i.state)) return [];
  const run = deskPickRun(deskPickLine(i));
  if (run.length > 1) return run;
  if (!deskCanvas(i) && !(i.next || []).length) return [];
  const listed = [...deskOptionWords(i).keys()];
  const named = [...new Set((i.next || []).map((n) => String(n.option)).filter((o) => o && o !== "any"))];
  if (listed.length > 1 && (named.length < 2 || listed.some((o) => named.includes(o)))) return listed;
  return named.length > 1 ? named : [];
}
/** Its options as a short list, each with what it is (when its reply says) and a Pick (#281, #445, #543): Cue closes
 *  the item, then files and routes what its holder said follows the one you picked. */
function deskPickBtns(i, cls = "") {
  const opts = deskPickOptions(i);
  if (!opts.length) return "";
  const words = deskOptionWords(i);
  // Its name: what its reply says of it, else the title of what follows it (IF PICKED:, #619).
  const named = (o) => (i.next || []).find((n) => String(n.option) === o)?.title || "";
  const row = (o) => `<li><b class="dk-optk">${esc(o)}</b><span class="dk-optw">${esc(words.get(o) || named(o) || `Option ${o}`)}</span><button class="btn small" data-desk-act="pick" data-id="${esc(i.id)}" data-on="${esc(o)}" aria-label="Pick ${esc(o)}">Pick</button></li>`;
  return `<ul class="dk-opts${cls ? ` ${cls}` : ""}">${opts.map(row).join("")}</ul>`;
}
/** The reply on item `i`'s chat that asks you to pick (#539): the newest of its holder's replies `ex` with its pick
 *  line, or with no pick line its options one to a line. Its time, null for none. */
function deskPickReplyAt(i, ex) {
  if (!deskPickOptions(i).length) return null;
  const line = deskPickLine(i);
  const asks = (e) => (line ? deskPickLineIn(e.text) === line : deskOptionWords({ said: [{ text: e.text }] }).size > 1);
  return (ex || []).findLast((e) => e.role === "agent" && asks(e))?.at_ms ?? null;
}
/** Item `id` wherever the window has it: a Desk's open items, or the finished ones fetched for its Done list. */
const deskItemAnywhere = (id) => (state.desk?.projects || []).flatMap((p) => p.items || []).find((x) => x.id === id)
  || [...deskDonePages.values()].flatMap((pg) => pg.items).find((x) => x.id === id) || null;
/** A finished item and the item that carries it on, each linking to the other (#281, #606, #667): one line, "You
 *  picked B2 on #652; #666 builds it" (built: "#666 built it"; nothing filed: "nothing follows it"), or, not a pick,
 *  "Followed by #604"; on the other end "From your pick on #593", or "Follows #593". Empty when it has neither. */
function deskBuiltWords(i) {
  const a = (id) => `<a href="#" data-desk-item="${esc(id)}">#${esc(id)}</a>`;
  if (i.picked) {
    const b = i.built_as && deskItemAnywhere(i.built_as);
    const next = !i.built_as ? "nothing follows it" : `${a(i.built_as)} ${b?.state === "done" ? "built it" : "builds it"}`;
    return `You picked ${esc(i.picked)} on #${esc(i.id)}; ${next}`;
  }
  if (i.built_as) return `Followed by ${a(i.built_as)}`;
  if (!i.builds) return "";
  const from = i.from_pick || deskItemAnywhere(i.builds)?.picked;
  return from ? `From your pick on ${a(i.builds)}` : `Follows ${a(i.builds)}`;
}
const deskBuiltLink = (i) => {
  const words = deskBuiltWords(i);
  return words ? `<p class="dk-built">${words}</p>` : "";
};
const deskSec = (title, html) => (html ? `<div class="dk-sec"><h3>${title}</h3>${html}</div>` : "");
/** Item `i`'s latest DONE (`d.checks`), quiet, under What to check on its page: "Done by web-lead · 12m ago · 2 files"
 *  (a click on who opens its chat, as its row does), then whether the installed Cue has it ("In the installed Cue ·
 *  installed Mon 3:04 PM" / "Not installed yet: install Cue to try it"), or for a design item, its mocks. */
function deskCheckInfo(i, d) {
  const c = i.state === "to_check" || i.state === "done" ? (d.checks || []).find((x) => x.id === i.id && x.last) : null;
  if (!c) return "";
  const by = c.by || "its session", ses = (state.sessions || []).find((s) => s.name === c.by && s.project === deskWorkspace(d));
  const who = ses ? `<button class="dk-chk-a" ${deskChatAttrs(ses.session_id, d)}>${esc(by)}</button>` : `<b>${esc(by)}</b>`;
  const n = (c.files || []).length, files = n ? ` · ${n === 1 ? "1 file" : `${n} files`}` : "";
  // In it when Cue was installed after the work last changed (`edited`), not after the DONE: you install, then say DONE.
  const mocks = /^https?:\/\//.test(c.designs || "") ? `<a class="dk-dlink" href="#" data-href="${esc(c.designs)}">Open the mocks ↗</a>` : "";
  const inst = mocks || !c.cue ? mocks : deskInstallWait({ ...i, state: "to_check" }) ? "Not installed yet: install Cue to try it"
    : i.finish?.step === "checked" ? (deskCheckBox(i) ? "" : "Ready to verify in Cue")
    : `In the installed Cue · installed ${esc(deskChkWhen(state.desk?.installed_ms || 0))}`;
  return `<div class="dk-chk-info"><div class="dk-chk-w">Done by ${who} · ${ago(c.at)} ago${files}</div>${inst ? `<div class="dk-chk-in">${inst}</div>` : ""}</div>`;
}
const deskChkWhen = (ms) => new Date(ms).toLocaleString([], { weekday: "short", hour: "numeric", minute: "2-digit" });
/** Back to `to` (a holder's sid), else to what shows with nothing open: the lead's chat, or on today's page (B2,
 *  desk/b2.js) the item list ("‹ Items 40"). */
const deskBack = (label, to = "") => (deskB2On() && !to ? `<button class="dk-back dk2b-items" data-desk-back="">‹ ${esc(deskB2ItemsWord(deskOf(deskSelection())))}</button>` : `<button class="dk-back" data-desk-back="${esc(to)}">‹ ${esc(label)}</button>`);

/** What you answered an item's holder on its page, by item: { q, a, ask, at }. Shown there until its holder asks again
 *  goes back to work, or ten minutes pass (board/board.js respond calls deskAnswered). */
const deskAnswers = new Map();
/** Card `it` answered with `decision`: kept for the Desk item whose holder asked it, so its page says what you picked. */
function deskAnswered(it, decision) {
  const item = (state.desk?.projects || []).flatMap((p) => p.items || []).find((x) => x.ask?.item_id === it.id);
  if (!item) return;
  const picked = decision.answers ? Object.values(decision.answers).join("; ") : decision.message || "";
  const a = decision.behavior === "deny" ? (picked ? `Declined: ${picked}` : "Declined") : picked || (decision.behavior === "allow_always" ? "Always allowed" : "Allowed");
  deskAnswers.set(item.id, { q: item.ask.text, a, ask: it.id, at: now(), kind: item.ask.kind });
}
/** What item `i`'s holder asks you: its ask's question, or, for a turn that ended on it, the last paragraph of its last
 *  words (all of it, not a line cut short; the hub's latest when its session isn't here). */
function deskQuestion(i) {
  if (i.ask) return i.ask.text || "";
  // Cue's own, while it finishes it ("The build failed"): not its holder's words.
  if (i.finish?.question) return i.finish.question;
  const last = [...(sessionOf(i.holder?.sid)?.thread || [])].reverse().find((e) => e.role === "agent")?.text || "";
  // Its calls made on its own (DECIDED) and its Desk lines (DONE, ITEM…) aren't what it asks you: they're dropped.
  const asked = last.split("\n").filter((l) => !DECIDED_LINE.test(l) && !DESK_SAY.test(l.trim())).join("\n").trim();
  return asked.split(/\n\s*\n/).pop().trim() || (last.trim() ? "" : i.latest || "");
}
/** What you last answered on item `i`'s page, while it's fresh and its holder isn't asking something new; else null. */
function deskAnswerOf(i) {
  const said = deskAnswers.get(i.id);
  if (!said || now() - said.at > 600000 || (i.ask && i.ask.item_id !== said.ask)) return null;
  if (i.state === "needs_you" && deskQuestion(i) !== said.q) return null;
  // Once its holder is at work again (your answer went through, or you sent it more), the old ask is gone.
  const s = i.holder?.kind === "crew" ? null : sessionOf(i.holder?.sid);
  if (i.state !== "needs_you" && s && ["working", "deciding", "agent"].includes(s.state)) return null;
  return said;
}
/** What holds an item up: the item it waits for, then this one; or what its holder asks you (only the question: it's
 *  answered at the page's foot, deskItemFoot), then what you answered. */
function deskChain(i) {
  const said = deskAnswerOf(i);
  // A command you allowed or denied (#611): its first lines, folded, with more; not the whole script.
  const asked = (q) => said?.kind === "permission" ? reqCut(`said:${i.id}`, `<pre class="slab">${esc(q)}</pre>`, q) : `<div class="dk-lt dk-md">${deskMd(q)}</div>`;
  if (said) return `<div class="dk-chain"><div class="dk-cn dk-q"><div>${asked(said.q)}<div class="dk-ls dk-picked"><span class="sent-check">✓</span> You answered: ${esc(said.a)}</div></div></div></div>`;
  if (i.state === "needs_you") {
    // A tool it waits to run (#611): its card at the page's foot has the command, folded, with Deny and Allow. Its
    // whole command here as well put the full script above that card and pushed the card down.
    if (deskItemRequest(i)?.kind === "permission") return "";
    const q = deskQuestion(i);
    return q ? `<div class="dk-chain"><div class="dk-cn dk-q"><div class="dk-lt dk-md">${deskMd(q)}</div></div></div>` : "";
  }
  if (i.state !== "queued" || !i.after) return "";
  const first = deskItemAny(i.project, i.after);
  if (!first) return "";
  const who = first.holder?.kind === "crew" ? `${first.holder.name} (${deskHolderName(first.holder.lead_sid)}'s crew)` : first.crew && first.holder ? `${first.holder.name} → ${first.crew}` : first.holder?.name || "";
  const files = deskFilesShort(first.files);
  const sub = [who, first.latest, files ? `has ${files} open` : ""].filter(Boolean).join(" · ");
  return `<div class="dk-chain"><div class="dk-cn"><i></i><div><div class="dk-lt">${first.project !== i.project ? `${esc(deskAfterName(i))} · ` : ""}${esc(first.title)}</div><div class="dk-ls">${esc(sub)}</div></div></div>
    <div class="dk-cn"><i class="wait"></i><div><div class="dk-lt">This one</div><div class="dk-ls">starts when ${esc(deskShort(first.title))} is done${i.after_why ? `: ${esc(i.after_why)}` : ""}</div></div></div></div>`;
}
/** An item's files, short: a few names, the rest as "+N more" ("" for none). */
function deskFilesShort(files, sep = ", ") {
  const names = (files || []).map((f) => f.split("/").pop());
  const more = names.length > 3 ? ` +${names.length - 3} more` : "";
  return names.slice(0, 3).join(sep) + more;
}
/** Item `i`'s project as the hub sends it (its waits and circles in it). */
const deskProjOf = (i) => (state.desk?.projects || []).find((p) => p.project === i.project || (p.items || []).some((x) => x.id === i.id));
/** The circle item `i` is in, if any (desk/hub/core/waits.rs): its links in order, when it was found, whether its lead was told. */
const deskCircleOf = (i) => (deskProjOf(i)?.circles || []).find((c) => c.items.includes(i.id)) || null;
const deskSince = (ms) => (ms ? `Since ${clockAt(ms)} · ${ago(ms)}` : "");
const DESK_NTH = ["no", "one", "two", "three", "four", "five"];
/** What item `i` waits on, in a box, with the buttons that clear it (#250, the "How work lands" waits): a circle and
 *  its links to cut; or what stopped its landing, a failed check among them (Send back to fix, Show the output; Run the landing again only
 *  when Cue can name nothing in its way). "" when it waits on nothing. */
function deskWaitsOn(i) {
  const btn = (act, label, look = "", extra = "") => `<button class="btn small ${look}" data-desk-act="${act}" data-id="${esc(i.id)}"${extra}>${esc(label)}</button>`;
  const box = (text, sub, more = "", tag = "") => `<div class="dk-wo${tag ? " circle" : ""}"><div class="dk-wo-l">Waits on${tag ? `<span class="dk-wo-tag">${tag}</span>` : ""}</div><div class="dk-wo-t">${text}</div>${sub ? `<div class="dk-wo-s">${esc(sub)}</div>` : ""}${more}</div>`;
  const ref = (id) => { const it = deskItem(i.project, id); return `<a href="#" data-desk-item="${esc(id)}">#${esc(id)}</a>${it ? ` ${esc(deskShort(it.title))}` : ""}`; };
  // Send back, your words to its holder, only on an item that's yours to check (#435); "Send back to fix" is the same.
  const back = i.holder && i.state === "to_check" ? btn("send_back", "Send back") : "";
  const c = deskCircleOf(i);
  if (c) {
    const others = c.items.length - 1;
    const links = c.links.map((l) => `<div class="dk-wo-link"><span>#${esc(l.item)} waits on #${esc(l.on)} · ${esc(l.why)}</span><button class="btn small" data-desk-act="cut" data-id="${esc(l.item)}" data-on="${esc(l.on)}">Cut this link</button></div>`).join("");
    const found = c.at ? `Found at ${clockAt(c.at)} · ${c.told ? "the lead was told at once" : "no lead to tell: cut a link here"}` : "";
    return box(`Itself, through ${DESK_NTH[others] || others} other${others === 1 ? "" : "s"}. None can move until one link is cut.`, found, `<div class="dk-wo-loop">${esc(c.words)}</div>${links}${back ? `<div class="dk-acts">${back}</div>` : ""}`, "Circle");
  }
  // Queued after another item: "What's holding it up" on its page says so, with Start it now / Put it before here.
  const f = i.finish;
  // In review: its pull request, and a way to it. It lands once that's merged; closed unmerged, it goes back.
  if (f?.step === "review" && f.pr) {
    const sub = f.wait || deskSince(f.at);
    return box(`A review of its pull request · <a href="#" data-href="${esc(f.pr)}">${esc(f.pr.replace(/^https?:\/\//, ""))}</a>`, sub,
      `<div class="dk-acts"><a class="btn small primary" href="#" data-href="${esc(f.pr)}">Open the PR</a></div>`);
  }
  if (!f?.question) return "";
  const q = esc(f.question);
  // A check (the build, the new Cue starting) that failed: its holder fixes it; the output is its log. Or, when main
  // was what broke it and is fixed now, run again on main as it is (#343).
  if (f.step === "failed" || f.step === "rolled_back" || f.step === "check_failed") {
    const show = f.log ? btn("log_all", deskFinLogOpen.has(i.id) ? "Hide the output" : "Show the output") : "";
    return box(q, deskSince(f.at), `<div class="dk-acts">${btn("fix_back", "Send back to fix", "primary")}${btn("rerun", "Run the landing again")}${show}</div>`);
  }
  // Edits in the main folder already on its branch (#659): Clear and land, your one OK. Files that also hold another
  // session's edits are named and left alone.
  if (deskClearable(i)) {
    const names = (l) => l.length === 1 ? l[0] : `${l.slice(0, -1).join(", ")} and ${l[l.length - 1]}`;
    const one = f.clear.length === 1;
    const text = `Leftover edits in the main folder are in its way. ${esc(names(f.clear))} already ${one ? "has" : "have"} this item's changes.`;
    const keep = (f.clear_keep || []).map(([p, who]) => `${esc(p)} also has ${esc(who)}'s edits: left alone.`).join(" ");
    const how = `Clear and land saves ${one ? "its" : "each file's"} diff to work/, sets ${one ? "it" : "them"} back to main, and lands it.`;
    return box(text, deskSince(f.at), `${keep ? `<div class="dk-wo-s">${keep}</div>` : ""}<div class="dk-wo-s">${esc(how)}</div><div class="dk-acts">${btn("clear_land", "Clear and land", "primary")}${btn("fix_back", "Send back to fix")}</div>`);
  }
  // A word that mustn't go in, a clash with the main line: its holder's to fix.
  if (f.step === "word" || f.step === "conflict") return box(q, deskSince(f.at), `<div class="dk-acts">${btn("fix_back", "Send back to fix", "primary")}</div>`);
  // Its branch couldn't be pushed or its pull request opened (it says why), or the pull request was closed unmerged.
  if (f.step === "pr_failed" || f.step === "pr_closed") return box(q, deskSince(f.at), `<div class="dk-acts">${btn("rerun", f.step === "pr_failed" ? "Run the landing again" : "Open a new pull request")}${back}</div>`);
  // Nothing Cue can name: a plain retry, said so.
  return box(q, deskSince(f.at), `<div class="dk-acts">${btn("rerun", "Run the landing again")}${back}</div><div class="dk-wo-s">A plain retry: nothing known is in its way.</div>`);
}
/** An item's buttons, by where it is. */
const deskCanRestart = (i) => (i.state === "needs_resume" || i.stopped) && !!sessionOf(i.holder?.lead_sid || i.holder?.sid);

function deskItemActs(i) {
  const b = (act, label, look = "") => `<button class="btn small ${look}" data-desk-act="${act}" data-id="${esc(i.id)}">${esc(label)}</button>`;
  const first = i.after && deskItem(i.project, i.after);
  if (deskBackFor === i.id) return deskBackHtml(i.id);
  // A feature nobody holds is only its pieces (#282): they're given out, not it.
  if (deskFeat(i) && !i.holder) return "";
  // What it waits on, and what clears that (#250): in place of a plain Try again.
  const wo = deskWaitsOn(i);
  if (wo) return wo;
  // Waiting for install: nothing to try yet, so no Looks right (Send back stays: what it says can be wrong already).
  const acts = deskInstallWait(i) ? (i.state === "to_check" ? b("send_back", "Send back") : "") : i.state === "to_check" ? (deskLooksSoon.has(i.id) ? "" : b("send_back", "Send back"))
    : (i.state === "new" || i.state === "queued") && !i.holder && i.start_failed ? b("start_now", "Retry")
    // Parked (#385): Start queues it as a new item, for the lead to give out.
    : i.state === "held" ? b("start_now", "Start", "primary")
    // Waiting for a free working folder (#401): its session starts on it now, without one.
    : deskCopyWaits(i) ? b("start_now", "Start now", "primary") + b("park", "Park")
    : i.state === "queued" ? b("start_now", i.holder ? "Start it now" : "Start it now in a new session") + (first ? b("put_before", `Put it before ${deskShort(first.title)}`) : "") + b("park", "Park")
    : i.state === "new" ? b("start_now", "Start it now in a new session") + b("park", "Park")
    // Its holder's turn cut off (a crash, a quit) leaves it "working" with nobody on it: Resume wakes that same
    // session where it left off (#375).
    : deskCanRestart(i) ? b("start_now", "Resume")
    : deskFinishing(i) ? deskInstallNow(i, "btn small") + deskSkipInstall(i, "btn small")
    // Done by your pick on its mocks (#281): its build item says where it went (deskBuiltLink), nothing to undo here.
    // Reopen (#577): your words back to the session that did it, or a fresh one when it's closed or on other work.
    : i.state === "done" && !i.picked ? `<div class="dk-chk-after">${deskCommitLine(i)}</div>${i.holder ? b("send_back", "Reopen") : ""}` : "";
  const give = !i.holder && i.state !== "done" && i.state !== "held" ? deskGive(i, "pane") : "";
  return acts || give ? `<div class="dk-acts">${give}${acts}</div>` : "";
}
/** Looks right on an item's page: in its title area, always in reach however far its page is scrolled (#163); its
 *  few seconds to take it back there too. None while it waits for an install or you're writing why to send it back. */
function deskLooksTop(i) {
  if (i.state !== "to_check" || deskInstallWait(i) || deskBackFor === i.id) return "";
  // Committed when its session said DONE: Looks right only closes it (its state line says which commit). Not
  // committed yet: the same button commits it to main first, and says so.
  return deskLooksSoon.has(i.id) ? deskLooksSoonHtml(i.id) : `<button class="btn small primary" data-desk-act="looks_right" data-id="${esc(i.id)}">${deskLooksWord(i)}</button>`;
}
/** Looks right's words: "Commit to main" for an item whose files aren't committed; "Close" beside its check line (#526). */
const deskLooksWord = (i) => (deskUncommitted(i) ? "Commit to main" : deskCheckBox(i) ? "Close" : "Looks right");
/** Item `i` is to check and there's something to do with it now (#526): its check line shows in a box at the top of
 *  its panel, with Close and Send back. Not mocks (their pick), not changes left on its branch, not waiting for install. */
const deskCheckBox = (i) => i?.state === "to_check" && !deskInstallWait(i) && !i.finish?.unlanded && !i.finish?.mocks && !deskCanvas(i);
const DESK_LOG_FOLD = 6;
/** Which items' build logs are open in full (Show all), by id. */
const deskFinLogOpen = new Set();
/** A failed build's or start's log tail on its item's page: mono, folded to a few lines, Show all opens it (and it
 *  scrolls). Its last lines show folded. */
function deskFinLog(i) {
  const log = i.state === "needs_you" ? (i.finish?.log || "").trimEnd() : "";
  if (!log) return "";
  // Folded: its last lines, where a build says what went wrong.
  const lines = log.split("\n"), open = deskFinLogOpen.has(i.id), long = lines.length > DESK_LOG_FOLD;
  return `<pre class="dk-flog${open ? " open" : ""}">${esc(open ? log : lines.slice(-DESK_LOG_FOLD).join("\n"))}</pre>${long ? `<button class="dk-flog-more" data-desk-act="log_all" data-id="${esc(i.id)}">${open ? "Show less" : "Show all"}</button>` : ""}`;
}
/** An item's decisions as sentences: what was decided (its age at the right), why under it, then who and Undo. The
 *  same decision said again in a row is one entry marked "×N"; a click shows a long one in full. */
function deskDecList(list) {
  const rows = [];
  for (const c of list) {
    const last = rows[rows.length - 1];
    if (last && last.c.text === c.text && (last.c.why || "") === (c.why || "")) { last.n++; if (deskMs(c.at) >= deskMs(last.c.at)) last.c = c; }
    else rows.push({ c, n: 1 });
  }
  return `<div class="dk-decs">${rows.map(({ c, n }) => `<div class="dk-lr dk-dec dk-dl${deskLogOpen.has(c.id) ? " open" : ""}" data-desk-log-row="${esc(c.id)}">`
    + `<div class="dk-dl1"><span class="dk-d">${esc(c.text)}${n > 1 ? ` <span class="dk-x">×${n}</span>` : ""}</span><span class="dk-age">${ago(deskMs(c.at))} ago</span></div>`
    + (c.why ? `<div class="dk-w">${esc(c.why)}</div>` : "")
    + `<div class="dk-dl3">${c.by ? `<span class="dk-lby">by <b>${esc(c.by)}</b></span>` : ""}<span class="grow"></span><span class="dk-lu">${deskTickBox(c)}${deskUndoLink(c)}</span></div></div>`).join("")}</div>`;
}
