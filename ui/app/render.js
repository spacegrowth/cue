// Cue UI app — the main draw: the whole window rebuilt from a string, then put back as it was (the cursor, every
// list's scroll, the chat's place) and moved on where it changed (cards that glide, arrive, slide in).

function applyTheme() {
  const mode = state.settings?.appearance?.mode ?? "system";
  if (mode === "system") document.documentElement.removeAttribute("data-theme");
  else document.documentElement.dataset.theme = mode;
  retheme();
}
/** The chat and color the window last said ("chat-open"), so they're said only when they change. */
let chatOpenTold;
// The lists a redraw puts back where they were scrolled (the chat keeps its element across draws, chat/patch.js liftChat /
// settleChat, so its scroll is its own). Each is known by its kind and its place among that kind, or by its
// data-scroll-key: a list new in this draw (a lead's review rows) is never handed another list's place.
const SCROLLERS = [".col", ".lv-list", ".sv-scroll", ".sheet-body", ".sec-body", ".card.recent", ".hist", ".hdetail", ".itab", ".vp-list", ".rv-rows", ".rdv-scroll", ".rdv-list", ".rdv-toc"];
function scrollerKeys() {
  const seen = {};
  return [...document.querySelectorAll(SCROLLERS.join(", "))].map((el) => {
    const kind = SCROLLERS.find((s) => el.matches(s));
    seen[kind] = (seen[kind] ?? -1) + 1;
    return [el.dataset.scrollKey ? `${kind}:${el.dataset.scrollKey}` : `${kind}#${seen[kind]}`, el];
  });
}

function renderMain() {
  projTheme();
  retheme();
  // You're dragging out a selection: a redraw would wipe it. It waits until you let go.
  if (dragSelecting() || folding || (mouseDown && onPress) || composing) { heldDraw = true; return; }
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
  const was = { focused, flipFrom: flipRects(), focusKey, focusId };
  was.caret = focusKey || (focusId && focused.setSelectionRange) ? [focused.selectionStart, focused.selectionEnd] : null;
  // Each text box is built afresh too, scrolled to its top: a long message would jump there on every redraw.
  was.boxTops = new Map([...document.querySelectorAll("textarea[data-text]")].map((el) => [el.dataset.text, el.scrollTop]));
  was.scrolls = new Map(scrollerKeys().map(([key, el]) => [key, el.scrollTop]));
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
  was.chatPinned = was.oldChat && was.oldChat.scrollHeight - was.oldChat.scrollTop - was.oldChat.clientHeight < 24;
  return was;
}

/** The header: the chips, the views (Board, the add-on's, Read, Insights, Terminal), search, usage, the terminal and Settings. */
function topBarHtml(counts) {
  return `<div class="top" data-tauri-drag-region><span class="wordmark" data-tauri-drag-region role="img" aria-label="Cue">${CUE_MARK}</span><span class="hchips"><button class="hchip new-chip" data-lv="new" data-spot="1">+ New</button>${counts}${liveChip()}${projChip()}</span><span class="grow" data-tauri-drag-region></span>${updateCard()}<span class="grow" data-tauri-drag-region></span>
      <button class="top-btn icon" data-act="open-search" aria-label="Find (⌘K)">${SEARCH_ICON}</button>
      ${usageChip()}${shellsBtn()}${extPanelBtn()}
      <div class="switch-view"><button class="${view === "board" ? "on" : ""}" data-view="board">Board</button>${addOn.views.map((v) => `<button class="${view === v.id ? "on" : ""}" data-view="${v.id}">${v.label()}</button>`).join("")}<button class="${view === "read" ? "on" : ""}" data-view="read">Read</button><button class="${view === "insights" ? "on" : ""}" data-view="insights">Insights</button><button class="sv-term ${view === "terminal" ? "on" : ""}" data-view="terminal">Terminal<span class="sh-live ${shellsLive()}">${shellsLive() === "done" ? "✓" : ""}</span></button></div>
      <button class="top-btn icon" data-act="open-settings" title="Settings" aria-label="Settings">${GEAR_ICON}</button></div>`;
}
/** The view picked in the header. */
const viewHtml = () => (view === "history" ? historyView() : view === "insights" ? insightsTabView() : view === "sessions" ? sessionsView() : view === "read" ? readView() : view === "terminal" ? shellView() : addOn.views.find((v) => v.id === view)?.html() ?? boardView());
/** What sits over the window: Move to Cue, a delete to confirm, a picture full size, Send to, a sheet, the live
 *  list in the middle of the window, an action's output. */
function overlaysHtml() {
  return `${moving ? movingDialog() : ""}${deleteAsk ? deleteDialog() : ""}${quitAsk ? quitDialog() : ""}${lightbox ? `<div class="lightbox" data-act="close-lightbox"><img src="${esc(lightbox.srcs[lightbox.i])}" alt=""/>${lightbox.srcs.length > 1 ? `<div class="lb-count">${lightbox.i + 1} / ${lightbox.srcs.length} · ← →</div>` : ""}</div>` : ""}${sheet === "forward" && forward ? forwardPop() : ""}${sheet && sheet !== "forward" ? `<div class="scrim" data-act="close-sheet">${sheet === "convo" && convo ? convoSheet() : sheet === "setup" ? setupSheet() : actSheet()}</div>` : ""}${liveOpen && liveSpot ? `<div class="scrim spot-scrim"><span class="lv-wrap spot">${livePop(liveRows())}</span></div>` : ""}${actOverlay()}${trustAsk ? trustDialog() : ""}`;
}
/** The window drawn afresh, the open chat lifted out first so it can go back in (chat/patch.js liftChat). */
function drawWindow() {
  const counts = headerChips();
  const lifted = liftChat();
  const focusMode = focus && view === "board";
  const topBar = focusMode ? "" : topBarHtml(counts);
  document.getElementById("app").innerHTML = `${focusMode ? "" : `<div class="dragbar" data-tauri-drag-region></div>`}<div class="app ${focusMode ? "focus" : ""}">
    ${topBar}
    ${viewHtml()}
  </div>${overlaysHtml()}`;
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
}
/** What floats over the window, back under what it belongs to. */
function placeFloating() {
  placePip();   // Cue's own terminal: under its button as the peek, or filling the Terminal view
  placeExt();   // the open session's extension tab, over the slot left for it
  placeActs();  // a chip's actions popover under its dots
  placeReviewMenu();   // a Review row's ⋯ menu under its button
  placeTeamRows();   // a lead's open team: its scrolling box ends on a whole row
  placeVoiceList();   // Settings → Voice: the open list under its picker
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
  scrollerKeys().forEach(([key, el]) => { if (was.scrolls.has(key)) el.scrollTop = was.scrolls.get(key); });
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
  // land. Not the kept chat's dots: those elements live on, and their pulse with them.
  const phase = `-${Date.now() % PULSE_MS}ms`;
  const liveChat = kept ? document.querySelector("[data-chat]") : null;
  document.querySelectorAll(".dot-live").forEach((el) => { if (!liveChat?.contains(el)) el.style.animationDelay = phase; });
  markMore();   // lists end on whole cards before anything measures where cards landed
  statusPopPaint();   // an open status card stays where it was across the redraw
  statusPanelFit();   // an open status list ends above the pane's bottom, scrolling past that
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
/** The chat jumps to its end when you open something else; a new row follows only if you were at the end.
 *  New target: jump to the end. Same chat, you were at the end (within 24 px, measured before the patch): stay
 *  there (an image added to the box, or a step landing, mustn't push the last lines out of view). Scrolled up to
 *  read: a row arriving leaves your place alone. */
function scrollChat(chat, was, lifted, kept) {
  const key = chat ? `${active?.id}|${active?.sid}` : null;
  // Back from a background agent's or script's view (chat/helpers.js closeHelperView): the end, whatever the view's scroll was.
  if (chat && chatToEnd) { pinChatEnd(chat); holdChat = null; }
  // A reply just opened in the reader (chat/reader.js readerPane): its start, wherever the chat was.
  else if (chat && !kept && chat.dataset.chatFor?.startsWith("r|")) { chat.scrollTop = 0; holdChat = null; }
  else if (chat && holdChat != null) { chat.scrollTop = chat.scrollHeight - holdChat; holdChat = null; }
  else if (chat && (key !== activeKey || was.chatPinned)) chat.scrollTop = chat.scrollHeight;
  // Drawn afresh for the same chat (its steps feed was replaced): where you were.
  else if (chat && !kept && was.oldChat?.dataset.chatFor === chat.dataset.chatFor && lifted.top != null) chat.scrollTop = lifted.top;
  activeKey = key;
}
/** Which chat is open, and the window's color, told to an add-on (once each time either changes). */
function tellChatOpen(chat) {
  const openNow = chat ? active?.sid || null : null, swatch = themeIdx(), hue = swatch === OWN ? ownHue() : null;
  const told = `${openNow}|${swatch}|${hue?.h}|${hue?.k}`;
  if (told !== chatOpenTold) { chatOpenTold = told; T.event.emit("chat-open", { sid: openNow, swatch, hue }).catch(() => {}); }
}
/** The cursor back where it was, or into the text box of something new. */
function restoreFocus(was) {
  const { focusKey, focusId, caret } = was;
  // Something new opened in Active: its text box takes the cursor, so you can just type. Done once it
  // has (at launch the first draw comes before the sessions do: no box yet, so the next draw tries again).
  const target = `${active?.id}|${active?.sid}`;
  if (target !== focusedTarget && !sheet && !focusKey && focusComposer()) focusedTarget = target;
  saveDrafts();
  if (focusKey) { const el = document.querySelector(`[data-text="${CSS.escape(focusKey)}"]`); if (el) { el.focus(); el.setSelectionRange(...caret); } }
  else if (focusId) { const el = document.getElementById(focusId); if (el) { el.focus(); if (caret) el.setSelectionRange(...caret); } }
  // Where each box was scrolled, after the cursor is back (putting it back can scroll the box itself).
  document.querySelectorAll("textarea[data-text]").forEach((el) => { const top = was.boxTops.get(el.dataset.text); if (top != null) el.scrollTop = top; });
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
}
addEventListener("resize", () => { fitHead(); fitTips(); fitTop(); markMore(); });
document.addEventListener("scroll", (e) => { if (e.target instanceof Element) moreWoke.add(e.target); if (!moreFrame) moreFrame = requestAnimationFrame(() => { moreFrame = 0; markMore(false); }); }, true);
document.addEventListener("click", (e) => {
  const pill = e.target.closest?.(".more-below");
  if (!pill) return;
  e.stopPropagation();
  const sc = pill.list;
  sc?.scrollTo({ top: sc.scrollHeight, behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" });
}, true);
picker.addEventListener("change", () => attach(picker.dataset.key, [...picker.files]));
// Closed Finder's picker without one: back to the box you were adding to.
picker.addEventListener("cancel", () => document.querySelector(`[data-text="${CSS.escape(picker.dataset.key)}"]`)?.focus());
