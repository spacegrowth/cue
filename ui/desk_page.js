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
  return `<div class="dk-sb dk-gb-box"><div class="dk-sb-h"><span class="dk-sb-l">What should happen instead?</span><button class="dk-sb-x" data-desk-go-back="cancel">Cancel</button></div><div class="dk-box dk-sb-box"><div class="dk-box-row"><textarea rows="2" data-text="goback" aria-label="What should happen instead?" placeholder="Your answer goes to each one with its decision" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off">${esc(draft("goback").text)}</textarea><button class="btn primary small" data-desk-go-back="send">Send</button></div></div></div>`;
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
/** Your words about an item ("About #69 (its title): …", as deskSendBox sends them): a head over them with its kind's
 *  icon, its kind and number and its title (a click opens it), and your words under it. null for any other words. */
function deskAboutHead(e) {
  const m = /^About #([^\s(]+) \(/.exec(e?.text || "");
  if (!m) return null;
  const it = (state.desk?.projects || []).flatMap((d) => d.items).find((i) => deskIdIs(i, m[1]));
  // The title as sent; when the item is gone or renamed since, up to the first "): ".
  const rest = e.text.slice(m[0].length), title = it && rest.startsWith(`${it.title}): `) ? it.title : rest.split("): ")[0];
  if (rest.length === title.length) return null;
  const html = `<button class="dk-about" data-desk-chk-item="${esc(it?.id ?? m[1])}">${deskKindIcon(it)}<b>${esc(it ? deskNum(it) : `#${m[1]}`)}</b> · ${esc(title)}</button>`;
  return { html, text: rest.slice(title.length + 3) };
}
/** A chat the Desk shows: its earlier messages from Cue's log too (its thread keeps only the last few, all a restart
 *  starts with), loaded as it first shows, and Earlier messages above them, as in a session's chat (chat/thread.js). */
function deskChatRows(s, page) {
  const sid = s.session_id;
  if (!older.has(sid)) queueMicrotask(() => loadOlder(sid));
  const any = (s.thread || []).length || older.get(sid)?.msgs.length;
  return any ? chatHtml(null, s, s.harness || "claude", true, { lines: deskLineHtml, you: deskCueNote, youHead: deskAboutHead, noActs: true, ...page }) : "";
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
  if (deskOpen || deskTab !== "lead" || deskPick === deskSelection()) return null;
  const d = deskOf(deskSelection());
  return deskLead(d)?.sid === sid && !deskLeadEnded(d) ? deskLeadSession(d) : null;
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
 *  Desk's summary; then what you sent that it doesn't show yet (given up a minute after the send returned). */
function deskChat(d) {
  const s = deskLeadSession(d);
  const thread = s?.thread || [];
  const pending = (deskSent.get(d.project) || []).filter((p) => !(p.sent && now() - p.sent > 60000)
    && !thread.some((e) => e.role === "you" && e.at_ms >= p.at - 60000 && e.text.includes(p.text)));
  deskSent.set(d.project, pending);
  // The lead's own Desk lines drawn as lines, not raw text.
  const rows = s ? deskChatRows(s, { extra: deskCheckRows(d).concat(deskActRows(d)), noSteps: true }) : "";
  const mine = pending.map((p) => { const hd = deskAboutHead(p); return `<div class="cv-you sending">${p.text ? `<div class="cv-you-text">${hd?.html || ""}${esc(hd ? hd.text : p.text).replace(/\n/g, "<br>")}</div>` : ""}${p.images?.length ? `<div class="thumbs">${p.images.map((im) => `<span class="thumb"><img src="${esc(im.data)}" alt=""/></span>`).join("")}</div>` : ""}<div class="cv-meta">Sending</div></div>`; }).join("");
  // Where the lead is now (working on what, its latest step, stuck, compacting), as under any session's chat.
  const where = s && sessionOf(s.session_id) ? whereNow(null, s) : "";
  const empty = !rows && !mine && !where ? `<div class="dk-empty">Tell ${esc(deskLeadName(d))} about a bug, a feature or a task. It adds them here, hands each to a session, and says why.</div>` : "";
  return `<div class="dk-chat" data-desk-chat="${esc(d.project)}">${empty}${rows}${mine}${where}</div>`;
}
/** What the lead did on its own (#360: closed, started, sent back, moved up, took back a copy, answered a session), newest first. */
const deskActs = (d) => (d?.decisions || []).filter((x) => x.act && x.sid === deskLead(d)?.sid).sort((a, b) => b.at - a.at);
/** One of those as a line: its words, the item's number opening its page, and why (left out with `bare`). */
function deskActLine(x, d, bare) {
  const id = x.item_id || "", it = id && deskItem(d.project, id), at = x.text.indexOf(`#${id}`);
  const num = `<button class="dk-act-it" data-desk-chk-item="${esc(id)}">${it ? deskKindIcon(it) : ""}<b class="dk-inum">#${esc(id)}</b></button>`;
  const text = id && at >= 0 ? `${esc(x.text.slice(0, at))}${num}${esc(x.text.slice(at + id.length + 1))}` : esc(x.text);
  return bare ? text : `${text}${x.why ? `<span class="dk-act-why"> · ${esc(x.why)}</span>` : ""}`;
}
/** Those lines for the lead's chat (chat/render.js chatHtml `extra`), each at its time. */
const deskActRows = (d) => deskActs(d).map((x) => ({ at: x.at * 1000, html: keyed(`<div class="dk-act dk-cl"><span class="dk-cl-t"><b>${esc(x.by)}</b> · ${deskActLine(x, d)} <span class="dk-cl-age">· ${ago(x.at * 1000)}</span></span></div>`, `act:${x.id}`) }));
/** How long the lead's last moves stay in the Dispatcher's panel, and how many. */
const DESK_JUST_DID_MS = 30 * 60 * 1000, DESK_JUST_DID = 3;
const deskJustDid = (d) => deskActs(d).filter((x) => now() - x.at * 1000 < DESK_JUST_DID_MS).slice(0, DESK_JUST_DID);
/** "Dispatcher Actions" (#374, named #390): every move the lead made on its own, kept until you clear it. Today's come with the
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
/** The Dispatcher Actions tab: what the lead did on its own, newest first: when, its words with the item's number, and why. */
function deskMoves(d) {
  if (!deskMovePages.has(d.project)) deskMovesFetch(d.project);
  const all = deskMovesList(d), n = deskMovesShown.get(d.project) || DESK_MOVES_STEP, pg = deskMovePages.get(d.project);
  const asking = deskMovesClearing === d.project;
  const clear = all.length ? (asking ? `<span class="dk-mv-ask">Clear the whole list?</span><button class="btn small" data-desk-moves-keep>Keep</button><button class="btn small primary" data-desk-moves-clear="${esc(d.project)}">Clear</button>` : `<button class="btn small" data-desk-moves-clear="${esc(d.project)}">Clear</button>`) : "";
  const head = `<div class="dk-mv-h"><span class="dk-mv-t">Dispatcher Actions</span><span class="grow"></span>${clear}</div>`;
  const rows = all.slice(0, n).map((x) => `<div class="dk-mv" data-key="mv:${esc(x.id)}"><span class="dk-mv-at">${esc(deskMoveWhen(x.at * 1000))}</span><span class="dk-mv-line">${deskActLine(x, d)}</span></div>`).join("");
  const more = all.length > n || (pg && !pg.end) ? `<div class="dk-mv-more" data-desk-moves-more="${esc(d.project)}">${pg?.loading ? "Loading older actions…" : ""}</div>` : "";
  const empty = !all.length && !pg?.loading ? `<div class="dk-empty">Nothing yet. When ${esc(deskLeadName(d) || "the lead")} closes, starts, sends back or moves up an item on its own, takes back a copy or answers a session, it's listed here and stays until you clear it.</div>` : "";
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
 *  of page; a narrow card folds it on its own (desk.css). */
const DESK_PANEL_ICON = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M15 4v16"/></svg>`;
/** Kinds of page whose panel you hid ("item", "session", "crew", "lead", "dispatcher"), kept across restarts. */
const deskPanelHid = (() => { try { return new Set(JSON.parse(localStorage.getItem("cue.deskPanelHidden") || "[]")); } catch { return new Set(); } })();
function deskPanelToggle(page) {
  if (!deskPanelHid.delete(page)) deskPanelHid.add(page);
  try { localStorage.setItem("cue.deskPanelHidden", JSON.stringify([...deskPanelHid])); } catch { /* kept for this window only */ }
  renderMain();
}
/** A panel section: its small head, then `html` ("" leaves it out). */
const deskPsec = (head, html, cls = "") => (html ? `<section class="dk-psec${cls ? ` ${cls}` : ""}"><h4 class="dk-psh">${head}</h4>${html}</section>` : "");
/** The panel's NOW: the state's dot (`dot`: "ask", "work", "stop", "done") and word, once; one plain sentence; its
 *  buttons. `fold`: the counts said after the word only when the panel is folded into a line. */
const deskPnow = ({ dot, word, say = "", acts = "", fold = "" }) => `<section class="dk-psec dk-pnow"><h4 class="dk-psh">Now</h4><div class="dk-pnr"><i class="dk-pdot ${dot}" aria-hidden="true"></i><b class="dk-pword">${esc(word)}</b>${fold ? `<span class="dk-pfc"> · ${fold}</span>` : ""}</div>${say ? `<div class="dk-psay">${say}</div>` : ""}${acts ? `<div class="dk-pacts">${acts}</div>` : ""}</section>`;
/** A WHO line: a quiet label, then its value. */
const deskPwho = (label, html) => (html ? `<div class="dk-pw"><span class="dk-pwl">${label}</span><span class="dk-pwv">${html}</span></div>` : "");
/** Item `i`'s number as a link to its page ("#191"). */
const deskItemLink = (i) => `<button class="dk-sch-t" data-desk-chk-item="${esc(i.id)}">${deskKindIcon(i)}#${esc(i.id)}</button>`;
/** Session `sid`'s name, opening its chat when Cue has it; else just the name, bold. */
const deskChatLink = (sid, name, d) => (sessionOf(sid) ? `<button class="dk-pwa" ${deskChatAttrs(sid, d)}>${esc(name)}</button>` : `<b>${esc(name)}</b>`);
/** One page: `page` the kind of page (what Hide panel remembers), `kind` its icon and mark (DESK_MARKS), `mark` the
 *  quiet words before its name ("Bug #197 · "), `title` its name's html, `meta` its quiet line, `now` and `panel` the
 *  panel's sections, `main` its own column and `foot` the box under it. */
function deskPage({ page, kind, back, acts = "", mark, title, meta = "", now, panel = "", main, foot = "" }) {
  const hid = deskPanelHid.has(page);
  const tog = `<button class="btn small dk-ptog" data-desk-panel="${page}" aria-pressed="${!hid}">${DESK_PANEL_ICON}<span>${hid ? "Show panel" : "Hide panel"}</span></button>`;
  return `<div class="dk-pg${hid ? " hid" : ""}" data-desk-page="${page}"><div class="dk-ph"><div class="dk-ph1">${back}<span class="grow"></span>${acts}${tog}</div>`
    + `<h1 class="dk-ph2"><span class="dk-hk"${["desk", "session", "crew", "lead"].includes(kind) ? ` data-desk-mark="${kind}"` : ""}>${deskKindIcon(kind)}${esc(mark.replace(/ · $/, ""))}</span><span class="dk-hsep"> · </span><span class="dk-tname">${title}</span></h1>${meta ? `<div class="dk-ph3">${meta}</div>` : ""}</div>`
    + `<aside class="dk-panel">${now}${panel}</aside><div class="dk-pgm">${main}${foot}</div></div>`;
}
/** A session's dot and word for the panel, from deskSessState. */
const deskPstate = (s) => { const [dot, word] = s ? deskSessState(s) : ["idle", "idle"]; return [dot === "ask" ? "ask" : dot === "idle" ? "stop" : "work", word[0].toUpperCase() + word.slice(1)]; };
/** What session `sid` is on Desk `d`, as the left list's corner mark says it: "desk" (it runs the Desk: its
 *  Dispatcher, one name wherever it shows), "lead" (the Desk gave it a lead's work or it has a crew), else "session". */
const deskSessKind = (sid, d) => (sid === deskLead(d)?.sid ? "desk" : deskIsLead(sid, d.items.filter((i) => deskGroupKey(i.holder) === sid)) ? "lead" : "session");
/** The way back from a session's chat: to what opened it (an item's page, a holder's), else to the lead's chat. */
function deskChatBack(from, d) {
  const i = from?.kind === "item" && deskItem(d.project, from.id);
  const label = i ? deskNum(i) : from?.kind === "holder" ? deskHolderName(from.sid, d.items.find((x) => deskGroupKey(x.holder) === from.sid && x.state !== "done" && x.holder?.kind !== "crew")?.holder?.name) : deskLeadName(d) || "Back";
  return `<button class="dk-back" data-desk-chat-back>‹ ${esc(label)}</button>`;
}
function deskSessChat(sid, from, d, foot = "") {
  const s = sessionOf(sid);
  if (!s) return null;
  const rows = deskChatRows(s);
  const where = whereNow(null, s);
  const empty = !rows && !where ? `<div class="dk-empty">Nothing said yet.</div>` : "";
  const open = `<button class="btn small" data-desk-session="${esc(sid)}">Open its session</button>`;
  // One page (#200): back, Open its session; "Session · " (or Lead, Dispatcher: what the session is, never an item's
  // kind) and its name; quiet, the item it's on ("on #191 · its title", the number opening it). Its chat at the left,
  // its state, step and Stop in the panel, with every item it holds.
  const its = deskSessItems(d, sid), kind = deskSessKind(sid, d), on = its[0];
  const meta = on ? `on ${deskItemLink(on)} · ${esc(on.title)}${its.length > 1 ? ` · +${its.length - 1} more` : ""}` : s.since_ms ? `nothing on the Desk · since ${esc(clockAt(s.since_ms))}` : "nothing on the Desk";
  const [dot, word] = deskPstate(s);
  const say = dot === "ask" ? "It asks you something: answer under its chat." : dot === "work" ? (s.doing ? esc(s.doing) : "At work.") : "At its prompt: tell it what's next under its chat.";
  const stop = dot === "work" ? `<button class="btn small" data-act="interrupt" data-sid="${esc(sid)}">Stop</button>` : "";
  return deskPage({ page: kind === "desk" ? "dispatcher" : kind === "lead" ? "lead" : "session", kind, back: deskChatBack(from, d), acts: open,
    mark: `${DESK_MARKS[kind]} · `, title: deskSessName(sid, deskHolderName(sid), "dk-shn"), meta,
    now: deskPnow({ dot, word, say, acts: stop, fold: on ? `on #${esc(on.id)}` : "" }),
    panel: deskPsec("Who", deskPwho("On", its.length ? its.map(deskItemLink).join(", ") : "nothing yet")) + deskSessFiles(sid),
    main: `<div class="dk-chat" data-desk-chat="${esc(sid)}">${empty}${rows}${where}</div>`, foot });
}
/** The session whose steps the Desk follows live (chat/thread.js followSteps): its lead's, while its chat shows, or
 *  the session's whose chat is open. */
function deskFollowSid() {
  // An item's page while its holder works: its steps, for the turn it's writing (deskLiveTurn).
  if (deskOpen?.kind === "item" && deskOpen.project === deskSelection()) {
    const i = deskItem(deskOpen.project, deskOpen.id), sid = deskHolderSid(i), s = sid && sessionOf(sid);
    if (s && i.holder.kind !== "crew" && s.state === "working") return sid;
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
function deskDecisions(d) {
  const calls = deskCalls(d), empty = `<div class="dk-empty">No judgment calls yet. Routes and hand-offs are on each item.</div>`;
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
/** Its designs' canvas (the last DESIGNS: link a session gave, hub/desk.rs designs_said): "Open the mocks ↗", a
 *  plain link in the accent colour (Lead chats pick 1 + D: no outlined button), in its page's header (#178). */
const deskDesignsLink = (i) => /^https?:\/\//.test(i.designs || "") ? `<div class="dk-dsg">${deskDesignsA(i.designs)}</div>` : "";
const deskDesignsA = (url) => `<a class="dk-dlink" href="#" data-href="${esc(url)}">Open the mocks ↗</a>`;
/** An item with mocks waiting on you to pick one: the line of its holder's newest words that asks you to, plain, for
 *  under its page's header ("Pick one for working items: D, E or F"). Its question first while it asks you, then its
 *  latest reply. An earlier pick can already have been answered. */
function deskPickLine(i) {
  if (!/^https?:\/\//.test(i.designs || "") || (i.state !== "needs_you" && i.state !== "to_check")) return "";
  const texts = [i.state === "needs_you" ? deskQuestion(i) : "", i.said?.at(-1)?.text || ""];
  for (const t of texts) {
    const lines = plain(t).split(/\n+|(?<=[.?!])\s+/).map((x) => x.trim()).filter((x) => /\b(pick|choose)\b/i.test(x));
    const line = lines.find((x) => /^(pick|choose|which)\b/i.test(x)) || lines[0];
    if (line) return line.replace(/^(not done yet|still to do)\s*:\s*/i, "").replace(/^./, (c) => c.toUpperCase());
  }
  return "";
}
/** The options a mocks item asks you to pick from, as its pick line names them ("… working items: D, E or F" → D, E,
 *  F), none once picked. */
function deskPickOptions(i) {
  if (i.picked) return [];
  const run = deskPickLine(i).match(/\b[A-Z]\b(?:\s*(?:,|\/|\bor\b|\band\b)\s*(?:(?:or|and)\s+)?[A-Z]\b)+/);
  return run ? [...new Set(run[0].match(/\b[A-Z]\b/g))] : [];
}
/** Pick D, Pick E…: a pick means build (#281): Cue closes the mocks and adds the item that builds the one you picked. */
const deskPickBtns = (i) => deskPickOptions(i).map((o) => `<button class="btn small" data-desk-act="pick" data-id="${esc(i.id)}" data-on="${esc(o)}">Pick ${esc(o)}</button>`).join("");
/** A picked mocks item and the item that builds its pick, each linking to the other (#281). */
const deskBuiltLink = (i) => {
  const a = (id) => `<a href="#" data-desk-item="${esc(id)}">#${esc(id)}</a>`;
  const words = i.picked && i.built_as ? `Picked ${esc(i.picked)} → being built as ${a(i.built_as)}` : i.builds ? `Builds what you picked on ${a(i.builds)}'s mocks` : "";
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
    : i.finish?.step === "checked" ? "Ready to verify in Cue"
    : `In the installed Cue · installed ${esc(deskChkWhen(state.desk?.installed_ms || 0))}`;
  return `<div class="dk-chk-info"><div class="dk-chk-w">Done by ${who} · ${ago(c.at)} ago${files}</div>${inst ? `<div class="dk-chk-in">${inst}</div>` : ""}</div>`;
}
const deskChkWhen = (ms) => new Date(ms).toLocaleString([], { weekday: "short", hour: "numeric", minute: "2-digit" });
const deskBack = (label, to = "") => `<button class="dk-back" data-desk-back="${esc(to)}">‹ ${esc(label)}</button>`;

/** What you answered an item's holder on its page, by item: { q, a, ask, at }. Shown there until its holder asks again
 *  goes back to work, or ten minutes pass (board.js respond calls deskAnswered). */
const deskAnswers = new Map();
/** Card `it` answered with `decision`: kept for the Desk item whose holder asked it, so its page says what you picked. */
function deskAnswered(it, decision) {
  const item = (state.desk?.projects || []).flatMap((p) => p.items || []).find((x) => x.ask?.item_id === it.id);
  if (!item) return;
  const picked = decision.answers ? Object.values(decision.answers).join("; ") : decision.message || "";
  const a = decision.behavior === "deny" ? (picked ? `Declined: ${picked}` : "Declined") : picked || (decision.behavior === "allow_always" ? "Always allowed" : "Allowed");
  deskAnswers.set(item.id, { q: item.ask.text, a, ask: it.id, at: now() });
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
  if (said) return `<div class="dk-chain"><div class="dk-ln dk-q"><div><div class="dk-lt dk-md">${deskMd(said.q)}</div><div class="dk-ls dk-picked"><span class="sent-check">✓</span> You answered: ${esc(said.a)}</div></div></div></div>`;
  if (i.state === "needs_you") {
    const q = deskQuestion(i);
    return q ? `<div class="dk-chain"><div class="dk-ln dk-q"><div class="dk-lt dk-md">${deskMd(q)}</div></div></div>` : "";
  }
  if (i.state !== "queued" || !i.after) return "";
  const first = deskItem(i.project, i.after);
  if (!first) return "";
  const who = first.holder?.kind === "crew" ? `${first.holder.name} (${deskHolderName(first.holder.lead_sid)}'s crew)` : first.crew && first.holder ? `${first.holder.name} → ${first.crew}` : first.holder?.name || "";
  const files = deskFilesShort(first.files);
  const sub = [who, first.latest, files ? `has ${files} open` : ""].filter(Boolean).join(" · ");
  return `<div class="dk-chain"><div class="dk-ln"><i></i><div><div class="dk-lt">${esc(first.title)}</div><div class="dk-ls">${esc(sub)}</div></div></div>
    <div class="dk-ln"><i class="wait"></i><div><div class="dk-lt">This one</div><div class="dk-ls">starts when ${esc(deskShort(first.title))} is done${i.after_why ? `: ${esc(i.after_why)}` : ""}</div></div></div></div>`;
}
/** An item's files, short: a few names, the rest as "+N more" ("" for none). */
function deskFilesShort(files, sep = ", ") {
  const names = (files || []).map((f) => f.split("/").pop());
  const more = names.length > 3 ? ` +${names.length - 3} more` : "";
  return names.slice(0, 3).join(sep) + more;
}
/** Item `i`'s project as the hub sends it (its waits and circles in it). */
const deskProjOf = (i) => (state.desk?.projects || []).find((p) => p.project === i.project || (p.items || []).some((x) => x.id === i.id));
/** The circle item `i` is in, if any (hub/desk_waits.rs): its links in order, when it was found, whether its lead was told. */
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
  // Send back, your words to its holder, on every box it waits on you in (#371); "Send back to fix" is the same.
  const back = i.holder ? btn("send_back", "Send back") : "";
  const c = deskCircleOf(i);
  if (c) {
    const others = c.items.length - 1;
    const links = c.links.map((l) => `<div class="dk-wo-link"><span>#${esc(l.item)} waits on #${esc(l.on)} · ${esc(l.why)}</span><button class="btn small" data-desk-act="cut" data-id="${esc(l.item)}" data-on="${esc(l.on)}">Cut this link</button></div>`).join("");
    const found = c.at ? `Found at ${clockAt(c.at)} · ${c.told ? "the lead was told at once" : "no lead to tell: cut a link here"}` : "";
    return box(`Itself, through ${DESK_NTH[others] || others} other${others === 1 ? "" : "s"}. None can move until one link is cut.`, found, `<div class="dk-wo-loop">${esc(c.words)}</div>${links}${back ? `<div class="dk-acts">${back}</div>` : ""}`, "Circle");
  }
  // Queued after another item: "What's holding it up" on its page says so, with Start it now / Put it before here.
  const f = i.finish;
  if (!f?.question) return "";
  const q = esc(f.question);
  // A check (the build, the new Cue starting) that failed: its holder fixes it; the output is its log. Or, when main
  // was what broke it and is fixed now, run again on main as it is (#343).
  if (f.step === "failed" || f.step === "rolled_back" || f.step === "check_failed") {
    const show = f.log ? btn("log_all", deskFinLogOpen.has(i.id) ? "Hide the output" : "Show the output") : "";
    return box(q, deskSince(f.at), `<div class="dk-acts">${btn("fix_back", "Send back to fix", "primary")}${btn("rerun", "Run the landing again")}${show}</div>`);
  }
  // A word that mustn't go in, a clash with the main line: its holder's to fix.
  if (f.step === "word" || f.step === "conflict") return box(q, deskSince(f.at), `<div class="dk-acts">${btn("fix_back", "Send back to fix", "primary")}</div>`);
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
  const acts = deskInstallWait(i) ? b("send_back", "Send back") : i.state === "to_check" ? (deskLooksSoon.has(i.id) ? "" : b("send_back", "Send back"))
    : (i.state === "new" || i.state === "queued") && !i.holder && i.start_failed ? b("start_now", "Retry")
    // Parked (#385): Start queues it as a new item, for the lead to give out.
    : i.state === "held" ? b("start_now", "Start", "primary")
    : i.state === "queued" ? b("start_now", "Start it now in a new session") + (first ? b("put_before", `Put it before ${deskShort(first.title)}`) : "") + b("park", "Park")
    : i.state === "new" ? b("start_now", "Start it now in a new session") + b("park", "Park")
    // Its holder's turn cut off (a crash, a quit) leaves it "working" with nobody on it: Resume wakes that same
    // session where it left off (#375).
    : deskCanRestart(i) ? b("start_now", "Resume")
    // Waiting on you for an answer (#371): Send back is always there too, your words to its holder.
    : i.state === "needs_you" && i.holder ? b("send_back", "Send back")
    : deskFinishing(i) ? deskInstallNow(i, "btn small")
    // Done by your pick on its mocks (#281): its build item says where it went (deskBuiltLink), nothing to undo here.
    : i.state === "done" && !i.picked ? `<div class="dk-chk-after">${deskCommitLine(i)}</div>` : "";
  const give = !i.holder && i.state !== "done" && i.state !== "held" ? deskGive(i, "pane") : "";
  return acts || give ? `<div class="dk-acts">${give}${acts}</div>` : "";
}
/** Looks right on an item's page: in its title area, always in reach however far its page is scrolled (#163); its
 *  few seconds to take it back there too. None while it waits for an install or you're writing why to send it back. */
function deskLooksTop(i) {
  if (i.state !== "to_check" || deskInstallWait(i) || deskBackFor === i.id) return "";
  // Committed when its session said DONE: Looks right only closes it (its state line says which commit). Not
  // committed yet: the same button commits it to main first, and says so.
  return deskLooksSoon.has(i.id) ? deskLooksSoonHtml(i.id) : `<button class="btn small primary" data-desk-act="looks_right" data-id="${esc(i.id)}">${deskUncommitted(i) ? "Commit to main" : "Looks right"}</button>`;
}
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
