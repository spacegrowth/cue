// Cue UI app — the main draw: the whole window rebuilt from a string, then put back as it was (the cursor, every
// list's scroll, the chat's place) and moved on where it changed (cards that glide, arrive, slide in).

/** The window's light or dark (Settings → Appearance). Away on a Desk keeps it (#459): only the head's mark says so
 *  (desk/away.js deskAwayHeadMark); `away-on` on the root for anything else that wants to know. */
function applyTheme() {
  // Only the Desk shown counts on its own page (#643): another Desk's Away doesn't mark this one.
  const away = typeof deskAwayShown === "function" && deskAwayShown().length > 0;
  const mode = state.settings?.appearance?.mode ?? "system";
  const root = document.documentElement;
  if (mode === "system") root.removeAttribute("data-theme");
  else if (root.dataset.theme !== mode) root.dataset.theme = mode;
  root.classList.toggle("away-on", away);
  // Text size: the chat's words (styles/base.css --type-chat).
  const text = textSize();
  if (root.dataset.text !== text) root.dataset.text = text;
  retheme();
}
/** The chat and color the window last said ("chat-open"), so they're said only when they change. */
let chatOpenTold;
// The lists a redraw puts back where they were scrolled (the chat keeps its element across draws, chat/patch.js liftChat /
// settleChat, so its scroll is its own). Each is known by its kind and its place among that kind, or by its
// data-scroll-key: a list new in this draw (a lead's review rows) is never handed another list's place.
const SCROLLERS = [".col", ".cp-scroll", ".cpv-scroll", ".cpm-body", ".ib-scroll", ".ib-brief", ".ib-page-body", ".lv-list", ".sv-scroll", ".sheet-body", ".sec-body", ".card.recent", ".hist", ".hdetail", ".itab", ".vp-list", ".rdv-scroll", ".rdv-list", ".rdv-toc", ".dk-scroll", ".dk-pane", ".dk-chat", ".dk-rd", ".dk-desks", ".dk-givemenu"];
/** The lists that keep their place while they're off the window, by key: the Desk's Items list, which an open item's
 *  column takes the place of till "‹ Items" (#418). */
const AWAY_KEPT = /^\.dk-scroll:desk-items/;
const scrollsAway = new Map();
function scrollerKeys() {
  const seen = {};
  return [...document.querySelectorAll(SCROLLERS.join(", "))].map((el) => {
    const kind = SCROLLERS.find((s) => el.matches(s));
    seen[kind] = (seen[kind] ?? -1) + 1;
    return [el.dataset.scrollKey ? `${kind}:${el.dataset.scrollKey}` : `${kind}#${seen[kind]}`, el];
  });
}

// Every other box that scrolls (#309): SCROLLERS above are the ones with their own rules, and anything not on it was
// built afresh at its start on every update (a Desk picker, an ask, the Inbox's side, a menu, a wide table scrolled
// sideways). Before each draw every element in the window that's scrolled is noted (a few hundred reads, well under
// a millisecond; a scroll event comes a frame late, and an update in between would miss it), and put back by where it
// sits: each step down from #app named by its tag, first class and data-* attributes (a different item's or session's
// box is a different place), and its count among like siblings. What's noted here is put back first; the lists with
// their own rules then have the last word.
/** Each child of `parent` with its step: tag, first class, data-* attributes, its count among children alike. */
function placeSteps(parent) {
  const seen = {};
  return [...parent.children].map((n) => {
    const data = Object.entries(n.dataset).sort().map(([k, v]) => `${k}=${v}`).join(",");
    const sig = `${n.tagName}.${n.classList[0] || ""}[${data}]`;
    seen[sig] = (seen[sig] ?? -1) + 1;
    return [`${sig}:${seen[sig]}`, n];
  });
}
/** Where an element sits in the window, as steps down from #app; null when it's outside it or is one with its own rule. */
function placeOf(el) {
  const root = document.getElementById("app");
  if (!root?.contains(el) || el === root || el.matches("[data-chat], textarea[data-text], input[data-text]")) return null;
  const steps = [];
  for (let n = el; n !== root; n = n.parentElement) steps.push(placeSteps(n.parentElement).find(([, c]) => c === n)[0]);
  return steps.reverse();
}
function atPlace(steps) {
  let n = document.getElementById("app");
  for (const step of steps) { n = n && placeSteps(n).find(([s]) => s === step)?.[1]; if (!n) return null; }
  return n;
}
function notePlaces() {
  const out = [];
  for (const el of document.getElementById("app")?.getElementsByTagName("*") ?? []) {
    if (!el.scrollTop && !el.scrollLeft) continue;
    const steps = placeOf(el);
    if (steps) out.push([steps, el.scrollTop, el.scrollLeft]);
  }
  return out;
}
function putPlacesBack(noted) {
  for (const [steps, top, left] of noted) {
    const el = atPlace(steps);
    if (!el) continue;
    el.scrollTop = top;
    el.scrollLeft = left;
  }
}

/** A window with its own draw: null in Cue's window. The mini window (mini.js) sets it as it loads to its paint, and
 *  then every redraw Cue's code asks for (renderMain, render) is that paint, and this file's own listeners stand
 *  aside (the mini window has no board, header or tips to fit). */
let windowDraw = null;

function renderMain() {
  if (windowDraw) return windowDraw();
  projTheme();
  applyTheme();
  // You're dragging out a selection: a redraw would wipe it. It waits until you let go.
  if (dragSelecting() || folding || onPress || composing) { heldDraw = true; return; }
  const was = beforeDraw();
  const lifted = drawWindow();
  const kept = settleChat(lifted);   // the same chat still open: its element is back in place, patched
  afterDraw(was, lifted, kept);
}

/** What a redraw must put back: the cursor and its selection, every list's and box's scroll, the Waiting card under
 *  the pointer, where the selected cards sit, and whether the chat was at its end. */
function beforeDraw() {
  const focused = document.activeElement;
  const focusKey = focused?.dataset?.text;
  const focusId = !focusKey && focused?.matches?.("input[id], select[id]") ? focused.id : null;   // e.g. a quick phrase field, the Model pick
  // A button that has the keyboard (Esc closed its menu, Tab reached it) is built afresh too: found again by what it
  // does, its data-* attributes, so the keyboard stays on it rather than falling to nothing at every redraw.
  const focusBtn = !focusKey && !focusId && focused?.matches?.("button, [role=button], a[href]") && focused.attributes ? `${focused.localName}${[...focused.attributes].filter((a) => a.name.startsWith("data-")).map((a) => `[${a.name}="${CSS.escape(a.value)}"]`).join("")}` : null;
  const was = { focused, flipFrom: flipRects(), focusKey, focusId, focusBtn };
  was.caret = focusKey || (focusId && focused.setSelectionRange) ? [focused.selectionStart, focused.selectionEnd] : null;
  // Each text box is built afresh too, scrolled to its top: a long message would jump there on every redraw.
  was.boxTops = new Map([...document.querySelectorAll("textarea[data-text]")].map((el) => [el.dataset.text, el.scrollTop]));
  // A one-line box is scrolled sideways once its words run past its end: built afresh it shows their start (#308).
  was.lineLefts = new Map([...document.querySelectorAll("input[data-text]")].map((el) => [el.dataset.text, el.scrollLeft]));
  was.places = notePlaces();
  was.scrolls = new Map(scrollerKeys().map(([key, el]) => [key, el.scrollTop]));
  for (const [key, top] of was.scrolls) if (AWAY_KEPT.test(key)) scrollsAway.set(key, top);
  was.mcpTops = new Map([...document.querySelectorAll(".mcp-list[data-mcp-key]")].map((el) => [el.dataset.mcpKey, el.scrollTop]));   // the MCP picker, redrawn on every tick
  // A background script's output in its full view (chat/helpers.js scriptPane): where you were, or at its newest.
  was.bgScrolls = new Map([...document.querySelectorAll("[data-bg-scroll]")].map((el) => [el.dataset.bgScroll, el.scrollHeight - el.scrollTop - el.clientHeight < 4 ? null : el.scrollTop]));
  // Waiting is newest first, so a card that comes in lands on top and pushes the rest down. While the pointer
  // is over the list, the card under it stays put: the new one comes in above, and the count says so.
  const waitBody = document.querySelector(".sec-waiting .sec-body:hover");
  was.waitAnchor = waitBody && [...waitBody.querySelectorAll(".nrow[data-big]")].map((el) => ({ id: el.dataset.big, top: el.getBoundingClientRect().top })).find((a) => a.top >= waitBody.getBoundingClientRect().top - 1);
  // Sessions keeps its place by the card at its top, not by a scroll number: a reply moves its card into the
  // list, and the old number would land partway into another card (cut off at the top until you scrolled).
  const sessBody = document.querySelector(".sec-sessions .sec-body");
  if (sessBody?.scrollTop > 0) {
    const t = sessBody.getBoundingClientRect().top;
    const el = [...sessBody.querySelectorAll(":scope > [data-session]")].find((c) => c.getBoundingClientRect().bottom > t + 1);
    if (el) was.sessAnchor = { id: el.dataset.session, off: el.getBoundingClientRect().top - t };
  }
  was.onBefore = selectedCards();   // where the selected card sits in each list, and whether it's in full view
  was.oldChat = document.querySelector("[data-chat]");
  was.chatPinned = boxAtEnd(was.oldChat);
  return was;
}

/** The header: the chips; usage, search, the terminal, Extensions, Insights and Settings; then the views (Inbox, Board, Desk, the add-on's, Read) at the right edge. */
function topBarHtml(counts) {
  return `<div class="top" data-tauri-drag-region><span class="wordmark" data-tauri-drag-region role="img" aria-label="Cue">${CUE_MARK}</span><span class="hchips">${newChip()}${counts}${projChip()}</span><span class="grow" data-tauri-drag-region></span>${awayHead()}
      ${updateCard()}${usageChip()}<button class="top-btn icon top-find" data-act="open-search" aria-label="Find (⌘K)">${SEARCH_ICON}</button>${shellsBtn()}${extPanelBtn()}<button class="top-btn icon insights-btn" data-view="insights" aria-pressed="${view === "insights"}" aria-label="Insights">${INSIGHTS_ICON}</button><button class="top-btn icon" data-act="open-settings" aria-label="Settings">${GEAR_ICON}</button>
      <div class="switch-view">${inboxBtn()}<button class="${view === "board" ? "on" : ""}" data-view="board">Board</button><button class="${view === "desk" ? "on" : ""}" data-view="desk">Desk</button>${addOn.views.map((v) => `<button class="${view === v.id ? "on" : ""}" data-view="${v.id}">${v.label()}</button>`).join("")}<button class="${view === "read" ? "on" : ""}" data-view="read">Read</button></div></div>`;
}
/** While a Desk is away, its mark in the middle of the head (desk/away.js), with room either side. */
const awayHead = () => { const m = typeof deskAwayHeadMark === "function" ? deskAwayHeadMark() : ""; return m ? `${m}<span class="grow" data-tauri-drag-region></span>` : ""; };
/** Inbox in the header's switch, with how many need you ("Inbox 3"). */
const inboxBtn = () => { const n = inboxCount(); return `<button class="${view === "inbox" ? "on" : ""}" data-view="inbox">Inbox${n ? `<span class="ib-count">${n}</span>` : ""}</button>`; };
/** The view picked in the header. */
// A lead's crew on its own page (team_page.js) takes the board's place while it's open.
const viewHtml = () => (typeof crewPageShown === "function" && crewPageShown()) || (view === "history" ? historyView() : view === "inbox" ? inboxView() : view === "desk" ? deskView() : view === "insights" ? insightsTabView() : view === "sessions" ? sessionsView() : view === "read" ? readView() : view === "terminal" ? shellView() : addOn.views.find((v) => v.id === view)?.html() ?? boardView());
/** What sits over the window: Move to Cue, a delete to confirm, a picture full size, Send to, a sheet, the live
 *  list in the middle of the window. */
function overlaysHtml() {
  return `${moving ? movingDialog() : ""}${deleteAsk ? deleteDialog() : ""}${quitAsk ? quitDialog() : ""}${lightboxHtml()}${sheet === "forward" && forward ? forwardPop() : ""}${sheet && sheet !== "forward" ? `<div class="scrim" data-act="close-sheet">${sheet === "convo" && convo ? convoSheet() : sheet === "setup" ? setupSheet() : settingsSheet()}</div>` : ""}${liveOpen ? `<div class="scrim spot-scrim"><span class="lv-wrap spot">${livePop(liveRows())}</span></div>` : ""}${trustAsk ? trustDialog() : ""}${askBeforeHtml()}${typeof landsHtml === "function" ? landsHtml() : ""}`;
}
// Each text box with its words kept through a redraw, not built afresh: its undo stays with it (chat/patch.js moveTo,
// morphAttrs). Matched by its draft key, so another draft's undo never carries over.
function liftDraftEditors(root) {
  const editors = new Map();
  for (const el of root.querySelectorAll("textarea[data-text]")) {
    const key = el.dataset.text;
    if (!editors.has(key)) editors.set(key, []);
    editors.get(key).push(el);
    moveTo(document.body, el);
  }
  return editors;
}
function settleDraftEditors(root, editors) {
  for (const fresh of root.querySelectorAll("textarea[data-text]")) {
    const old = editors.get(fresh.dataset.text)?.shift();
    if (!old) continue;
    morphAttrs(old, fresh);
    if (old.value !== fresh.value) old.value = fresh.value;
    moveBack(fresh.parentNode, old, fresh);
    fresh.remove();
  }
  for (const remaining of editors.values()) for (const el of remaining) el.remove();
}
/** The window drawn afresh, the open chat lifted out first so it can go back in (chat/patch.js liftChat). */
function drawWindow() {
  const counts = headerChips();
  const lifted = liftChat();
  const focusMode = focus && view === "board";
  const topBar = focusMode ? "" : topBarHtml(counts);
  const html = `${focusMode ? "" : `<div class="dragbar" data-tauri-drag-region></div>`}<div class="app ${focusMode ? "focus" : ""}">
    ${topBar}
    ${viewHtml()}
  </div>${overlaysHtml()}`;
  const root = document.getElementById("app");
  const editors = liftDraftEditors(root);
  const lists = typeof deskListsLift === "function" ? deskListsLift() : null;   // the Desk's Items and Progress, kept (desk/list.js)
  drawAround(root, html);   // the Desk's open chat kept where it is, drawn around (chat/patch.js)
  if (lists) deskListsSettle(lists);
  settleDraftEditors(root, editors);
  return lifted;
}

/** Everything after the draw, in order: floating parts placed, scrolls back, motion, the chat's place, the cursor. */
function afterDraw(was, lifted, kept) {
  placeFloating();
  restoreScrolls(was);
  replayMotion(was, kept);
  placeForward();
  saveSpot();
  revealAfterDraw(was);
  document.querySelectorAll("textarea[data-text]").forEach(grow);
  const chat = document.querySelector("[data-chat]");
  scrollChat(chat, was, lifted, kept);
  // A reply being read out: what it's reading stays in view, over the chat's jump to its end (chat/listen.js listenFollow).
  if (listening) listenFollow(listening);
  tellChatOpen(chat);
  restoreFocus(was);
  fitAfterDraw(was.focused);
  reqFit();
}
/** What floats over the window, back under what it belongs to. */
function placeFloating() {
  placePip();   // Cue's own terminal: under its button as the peek, or filling the Terminal view
  placeExt();   // the open session's extension tab, over the slot left for it
  placeDiff();  // the Diff tab's page in its slot (diff.js)
  placeTeamRows();   // a lead's open team: its scrolling box ends on a whole row
  crewListPlace();   // a crew page's list: its rows' form, the chip for an ask out of view (team_page.js)
  placeVoiceList();   // Settings → Voice: the open list under its picker
  if (typeof deskGivePlace === "function") deskGivePlace();   // Give to…'s menu sized before its scroll is put back (#308)
  const lc = current();
  listenKeep(lc?.s?.session_id || lc?.it?.session_id);   // a reply read out: its marks back on the redrawn chat
}
/** Sessions scrolled: the card that was at its top back where it was; that card gone, the list starts on the
 *  whole card now at its top (never one cut off). */
function keepSessionsTop(anchor) {
  const body = document.querySelector(".sec-sessions .sec-body");
  if (!body) return;
  const t = body.getBoundingClientRect().top;
  const cards = [...body.querySelectorAll(":scope > [data-session]")];
  const same = cards.find((c) => c.dataset.session === anchor.id);
  if (same) { body.scrollTop += same.getBoundingClientRect().top - t - anchor.off; return; }
  const top = cards.find((c) => c.getBoundingClientRect().bottom > t + 1);
  if (top && top.getBoundingClientRect().top < t - 1) body.scrollTop += top.getBoundingClientRect().top - t;
}
/** Every list where it was scrolled, the Waiting card under the pointer where it was, the MCP picker and a
 *  background script's output where they were. */
function restoreScrolls(was) {
  putPlacesBack(was.places);
  scrollerKeys().forEach(([key, el]) => { if (was.scrolls.has(key)) el.scrollTop = was.scrolls.get(key); else if (scrollsAway.has(key)) el.scrollTop = scrollsAway.get(key); });
  if (was.waitAnchor) {
    const el = [...document.querySelectorAll(".sec-waiting .sec-body .nrow[data-big]")].find((x) => x.dataset.big === was.waitAnchor.id);
    const dy = el ? el.getBoundingClientRect().top - was.waitAnchor.top : 0;
    if (Math.abs(dy) >= 1) el.closest(".sec-body").scrollTop += dy;
  }
  if (was.sessAnchor) keepSessionsTop(was.sessAnchor);
  document.querySelectorAll(".mcp-list[data-mcp-key]").forEach((el) => { const top = was.mcpTops.get(el.dataset.mcpKey); if (top) el.scrollTop = top; });
  document.querySelectorAll("[data-bg-scroll]").forEach((el) => { el.scrollTop = was.bgScrolls.get(el.dataset.bgScroll) ?? el.scrollHeight; });
}
/** Motion carried across the draw: the live dots' pulse, cards gliding from where they were, a ✓ row easing down. */
function replayMotion(was, kept) {
  // A redraw replaces every element, which would start each pulsing dot over: its phase comes from the
  // clock instead, so it carries on through the redraws that come every couple of seconds while steps
  // land (several a second just after a Send). Not the kept chat's dots: those elements live on, and
  // their pulse with them. The Inbox's "Thinking" dots and its live step too.
  const liveChat = kept ? document.querySelector("[data-chat]") : null;
  // A Desk chat kept in place (chat/patch.js drawAround): a dot it had already is left as it runs; a new one is set once.
  const inPlace = (el) => el._phased && keptInPlace.some((c) => c.contains(el));
  // Each looping animation a redraw would restart, and its length (as its CSS has it).
  for (const [sel, ms] of [[".dot-live, .pdot.live, .lv-chip .hdot", PULSE_MS], [".ib-dots i", 1200], [".ib-step.now i", 1600]]) {
    const phase = `-${Date.now() % ms}ms`;
    document.querySelectorAll(sel).forEach((el) => { if (!liveChat?.contains(el) && !inPlace(el)) { el.style.animationDelay = phase; el._phased = true; } });
  }
  markMore();   // lists end on whole cards before anything measures where cards landed
  if (view === "sessions") fitSvRest();
  flipPlay(was.flipFrom);
  // A card just answered: its ✓ row eases from the card's height down to its own.
  for (const el of document.querySelectorAll(".nrow.ghost.fresh")) {
    const g = ghosts.get(el.dataset.detail);
    if (!g?.h || matchMedia("(prefers-reduced-motion: reduce)").matches) continue;
    const to = el.offsetHeight;
    if (Math.abs(g.h - to) > 2) el.animate([{ height: `${g.h}px`, overflow: "hidden" }, { height: `${to}px`, overflow: "hidden" }], { duration: 280, easing: "cubic-bezier(.2,.8,.2,1)" });
  }
}
/** What a draw brings into view: a found message, the selected card, a new Waiting card, the Active pane stepped
 *  to, an opened History row. */
function revealAfterDraw(was) {
  // A conversation opened from search starts at the message you found (else at its end).
  if (sheet === "convo" && !convoScrolled) {
    convoScrolled = true;
    const body = document.querySelector(".convo-body"), hit = body?.querySelector(".cv-hit");
    if (hit) hit.scrollIntoView({ block: "center" }); else if (body) body.scrollTop = body.scrollHeight;
  }
  // The selected card, cut off at a list's edge, comes into full view: when you pick it, when it moves to
  // another list (you replied: it went from Waiting to Sessions), or when it outgrew the view while in full
  // view (a line added as it works). Not when you scrolled it part-way out yourself. It glides, just enough.
  const picked = `${active?.id}|${active?.sid}`;
  const repick = picked !== revealed;
  revealed = picked;
  for (const [list, { el, full }] of selectedCards()) {
    const before = was.onBefore.get(list);
    if (!full && (repick || !before || before.full)) glideIntoView(el);
  }
  // Something new in Waiting rises in from below with a brief ring, once.
  document.querySelectorAll(".nrow[data-big]").forEach((el) => {
    if (seenCards.has(el.dataset.big)) return;
    seenCards.add(el.dataset.big);
    if (cardsPrimed) el.classList.add("arrive");
  });
  cardsPrimed = true;
  // Stepped with ‹ › or ← →: the new item slides in from the side you moved toward.
  if (slideDir) {
    const pane = document.querySelector(".col.main .active-pane");
    pane?.classList.add(slideDir > 0 ? "slide-r" : "slide-l");
    slideDir = 0;
  }
  // A History row you just opened: its conversation starts at the end, where you replied, and the row comes into view.
  if (view === "history" && histOpen && histOpen !== histShown) {
    const ex = document.querySelector(".hexpand"), d = ex?.querySelector(".hdetail");
    if (d) d.scrollTop = d.scrollHeight;   // the end: your reply, under what it answered
    ex?.scrollIntoView({ block: "nearest" });
  }
  histShown = view === "history" ? histOpen : null;
}
/** Which chat is open, and the window's color, told to an add-on and to talking (voice/talk.rs: what you say goes to
 *  that chat), once each time either changes. On the Desk, the chat it shows: its lead's, or a session's (#617). */
function tellChatOpen(chat) {
  const openNow = chat ? active?.sid || null : view === "desk" && typeof deskShownSid === "function" ? deskShownSid() : null, swatch = themeIdx(), hue = swatch === OWN ? ownHue() : null;
  const told = `${openNow}|${swatch}|${hue?.h}|${hue?.k}`;
  if (told !== chatOpenTold) { chatOpenTold = told; T.event.emit("chat-open", { sid: openNow, swatch, hue }).catch(() => {}); }
}
/** The cursor back where it was, or into the text box of something new. */
function restoreFocus(was) {
  const { focusKey, focusId, focusBtn, caret } = was;
  // Something new opened in Active: its text box takes the cursor, so you can just type. Done once it
  // has (at launch the first draw comes before the sessions do: no box yet, so the next draw tries again).
  const target = `${active?.id}|${active?.sid}`;
  if (target !== focusedTarget && !sheet && !focusKey && focusComposer()) focusedTarget = target;
  if (view !== "desk" && typeof deskBoxShown !== "undefined") deskBoxShown = null;   // back on the Desk later, its open chat's box takes the cursor again (desk.js)
  saveDrafts();
  // preventScroll: the box is redrawn where it was, so nothing needs scrolling to show it. Without it
  // WebKit can scroll an ancestor (the Desk item card hides its overflow) to reveal the new box, and
  // nothing scrolls it back: the item's box moved at the first redraw after it opened.
  if (focusKey) {
    const el = document.querySelector(`[data-text="${CSS.escape(focusKey)}"]`);
    // Gone: on the Inbox (Allow moved it on to the next row), the box on screen now takes it.
    if (el) { el.focus({ preventScroll: true }); el.setSelectionRange(...caret); } else if (view === "inbox") focusComposer(undefined, { home: "inbox", end: false });
  }
  else if (focusId) { const el = document.getElementById(focusId); if (el) { el.focus({ preventScroll: true }); if (caret) el.setSelectionRange(...caret); } }
  else if (focusBtn && !was.focused.isConnected && document.activeElement === document.body) {
    const els = document.querySelectorAll(focusBtn);   // only one just like it: two would be a guess
    if (els.length === 1) els[0].focus({ preventScroll: true });
  }
  // Where each box was scrolled, after the cursor is back (putting it back can scroll the box itself).
  document.querySelectorAll("textarea[data-text]").forEach((el) => { const top = was.boxTops.get(el.dataset.text); if (top != null) el.scrollTop = top; });
  document.querySelectorAll("input[data-text]").forEach((el) => { const left = was.lineLefts.get(el.dataset.text); if (left) el.scrollLeft = left; });
  // Something over the chat closed (Esc, a click outside, a pick): its text box takes the cursor back,
  // unless you closed it by clicking into another box.
  const over = !!(sheet || lightbox || liveOpen || menuFor || redirectFor || renaming || btwFor || deleteAsk);
  if (overWas && !over && !document.activeElement?.matches?.("input, textarea, select")) focusComposer();
  overWas = over;
}
/** What's measured once the window is in place: the header, the tips, By the way's panel (just under the chat's
 *  header, whatever height that has now), the terminal, the copied pill, the cards' top lines, the color picker. */
function fitAfterDraw(focused) {
  fitHead();
  fitTips();
  const bp = document.querySelector(".btw-pop"), ah = document.querySelector(".ap-head");
  if (bp && ah) bp.style.top = `${ah.offsetTop + ah.offsetHeight + 2}px`;
  placeTerm(!!termHost && termHost.contains(focused));
  showCopied();
  fitTop();
  placeColors();
  if (typeof inboxFitPhrases === "function") inboxFitPhrases();
  if (typeof inboxKeepEnd === "function") inboxKeepEnd();   // the open Inbox thread at its newest, unless you scrolled up
}
// A draw that throws must not leave the window empty: what went wrong is shown where the board would be, so a bad
// build is seen as an error and not as "Cue didn't start".
function showRenderError(e) {
  console.error("[render]", e);
  const app = document.getElementById("app");
  if (!app || app.innerHTML.length > 0 && !app.querySelector(".render-error")) return; // an earlier good draw stays
  const detail = e && e.stack ? e.stack : String(e);
  app.innerHTML = `<div class="render-error" style="padding:24px 28px;font:13px/1.5 ui-monospace,Menlo,monospace;white-space:pre-wrap;user-select:text"><div style="font:600 15px system-ui;margin-bottom:10px">Cue can't draw its window</div>${esc(detail)}\n\nIf this came with a new build, reinstall the previous one or fix the error above and run ./install.sh --app again.</div>`;
}
/** A redraw, its error shown rather than thrown (a window with its own draw: that draw, as it is). */
const render = () => { if (windowDraw) return windowDraw(); try { renderMain(); } catch (e) { showRenderError(e); } };
addEventListener("resize", () => { if (windowDraw) return; fitHead(); fitTips(); fitTop(); markMore(); if (typeof inboxFitPhrases === "function") inboxFitPhrases(); });
document.addEventListener("scroll", (e) => { if (windowDraw) return; if (e.target instanceof Element) moreWoke.add(e.target); if (!moreFrame) moreFrame = requestAnimationFrame(() => { moreFrame = 0; markMore(false); }); }, true);
document.addEventListener("click", (e) => {
  if (windowDraw) return;
  const pill = e.target.closest?.(".more-below");
  if (!pill) return;
  e.stopPropagation();
  const sc = pill.list;
  sc?.scrollTo({ top: sc.scrollHeight, behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" });
}, true);
