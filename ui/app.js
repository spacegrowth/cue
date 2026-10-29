// Cue UI — everything that needs you, visible at once, on one board.
// Colors live in style.css (Moss, light + dark).
// Cue UI app — the main draw, the global event handlers, and boot().
addEventListener("wheel", () => { glide = null; }, { capture: true, passive: true });
/** The closed strip's line: the open session's screen, every 2 s (its last line with anything on it). */
setInterval(async () => {
  const sid = active?.sid;
  if (!sid || document.hidden || termSid === sid || !inTmux(sid)) return;
  try {
    const screen = await invoke("session_screen", { sessionId: sid });
    const last = (screen.split("\n").map((l) => l.trim()).filter(Boolean).at(-1) || "").slice(0, 200);
    if (termLast.get(sid) === last) return;
    termLast.set(sid, last);
    const el = document.querySelector(`.tstrip[data-sid="${CSS.escape(sid)}"] .tlast`);
    if (el) el.textContent = last;
  } catch {}
}, 2000);
setInterval(trimCaches, 5 * 60000);

// ---------- main render ----------
function applyTheme() {
  const mode = state.settings?.appearance?.mode ?? "system";
  if (mode === "system") document.documentElement.removeAttribute("data-theme");
  else document.documentElement.dataset.theme = mode;
  retheme();
}
/** The chat and color app.js last said ("chat-open"), so they're said only when they change. */
let chatOpenTold;
function renderMain() {
  projTheme();
  retheme();
  // You're dragging out a selection: a redraw would wipe it. It waits until you let go.
  if (dragSelecting() || folding || (mouseDown && onLink) || composing) { heldDraw = true; return; }
  const focused = document.activeElement;
  const flipFrom = flipRects();
  const focusKey = focused?.dataset?.text;
  const focusId = !focusKey && focused?.matches?.("input[id], select[id]") ? focused.id : null;   // e.g. a quick phrase field, the Model pick
  const caret = focusKey || (focusId && focused.setSelectionRange) ? [focused.selectionStart, focused.selectionEnd] : null;
  // The chat keeps its element across draws (chat.js: liftChat / settleChat), so its scroll is its own.
  const SCROLLERS = ".col, .lv-list, .sv-scroll, .sheet-body, .sec-body, .card.recent, .hist, .hdetail, .itab, .vp-list";
  const scrolls = [...document.querySelectorAll(SCROLLERS)].map((el) => el.scrollTop);
  // A background agent's steps (chat.js helperBlock), by agent: where you were, or at its newest.
  const bgScrolls = new Map([...document.querySelectorAll("[data-bg-scroll]")].map((el) => [el.dataset.bgScroll, el.scrollHeight - el.scrollTop - el.clientHeight < 4 ? null : el.scrollTop]));
  const trayScrolls = new Map([...document.querySelectorAll("[data-tray-scroll]")].map((el) => [el.dataset.trayScroll, el.scrollTop]));   // the agents tray, by session
  const onBefore = selectedCards();   // where the selected card sits in each list, and whether it's in full view
  const oldChat = document.querySelector("[data-chat]");
  const chatPinned = oldChat && oldChat.scrollHeight - oldChat.scrollTop - oldChat.clientHeight < 24;

  const counts = headerChips();
  const lifted = liftChat();
  const focusMode = focus && view === "board";
  const topBar = focusMode ? "" : `<div class="top" data-tauri-drag-region><span class="wordmark" data-tauri-drag-region role="img" aria-label="Cue">${CUE_MARK}</span><span class="hchips"><button class="hchip new-chip" data-lv="new" data-spot="1">+ New</button>${counts}${liveChip()}${projChip()}</span><span class="grow" data-tauri-drag-region></span>${updateCard()}<span class="grow" data-tauri-drag-region></span>
      <button class="top-btn icon" data-act="open-search" aria-label="Find (⌘K)">${SEARCH_ICON}</button>
      ${usageChip()}${shellsBtn()}${extPanelBtn()}
      <div class="switch-view"><button class="${view === "board" ? "on" : ""}" data-view="board">Board</button><button class="${view === "sessions" ? "on" : ""}" data-view="sessions">Sessions</button><button class="${view === "insights" ? "on" : ""}" data-view="insights">Insights</button><button class="${view === "history" ? "on" : ""}" data-view="history">History</button></div>
      <button class="top-btn icon" data-act="open-settings" title="Settings" aria-label="Settings">${GEAR_ICON}</button></div>`;
  document.getElementById("app").innerHTML = `${focusMode ? "" : `<div class="dragbar" data-tauri-drag-region></div>`}<div class="app ${focusMode ? "focus" : ""}">
    ${topBar}
    ${view === "history" ? historyView() : view === "insights" ? insightsTabView() : view === "sessions" ? sessionsView() : boardView()}
  </div>${moving ? movingDialog() : ""}${deleteAsk ? deleteDialog() : ""}${lightbox ? `<div class="lightbox" data-act="close-lightbox"><img src="${esc(lightbox.srcs[lightbox.i])}" alt=""/>${lightbox.srcs.length > 1 ? `<div class="lb-count">${lightbox.i + 1} / ${lightbox.srcs.length} · ← →</div>` : ""}</div>` : ""}${sheet === "forward" && forward ? forwardPop() : ""}${sheet && sheet !== "forward" ? `<div class="scrim" data-act="close-sheet">${sheet === "convo" && convo ? convoSheet() : sheet === "setup" ? setupSheet() : actSheet()}</div>` : ""}${liveOpen && liveSpot ? `<div class="scrim spot-scrim"><span class="lv-wrap spot">${livePop(liveRows())}</span></div>` : ""}${actOverlay()}`;

  const kept = settleChat(lifted);   // the same chat still open: its element is back in place, patched
  placePip();   // Cue's own terminal, back over the columns as they are now
  placeExt();   // the open session's extension tab, over the slot left for it
  placeActs();  // a chip's actions popover under its dots
  placeVoiceList();   // Settings → Voice: the open list under its picker
  const lc = current();
  listenKeep(lc?.s?.session_id || lc?.it?.session_id);   // a reply read out: its marks back on the redrawn chat
  [...document.querySelectorAll(SCROLLERS)].forEach((el, i) => { if (scrolls[i] != null) el.scrollTop = scrolls[i]; });
  document.querySelectorAll("[data-bg-scroll]").forEach((el) => { el.scrollTop = bgScrolls.get(el.dataset.bgScroll) ?? el.scrollHeight; });
  document.querySelectorAll("[data-tray-scroll]").forEach((el) => { el.scrollTop = trayScrolls.get(el.dataset.trayScroll) ?? 0; });
  // A redraw replaces every element, which would start each pulsing dot over: its phase comes from the
  // clock instead, so it carries on through the redraws that come every couple of seconds while steps
  // land. Not the kept chat's dots: those elements live on, and their pulse with them.
  const phase = `-${Date.now() % PULSE_MS}ms`;
  const liveChat = kept ? document.querySelector("[data-chat]") : null;
  document.querySelectorAll(".dot-live").forEach((el) => { if (!liveChat?.contains(el)) el.style.animationDelay = phase; });
  markMore();   // lists end on whole cards before anything measures where cards landed
  if (view === "sessions") fitSvRest();
  flipPlay(flipFrom);
  // A card just answered: its ✓ row eases from the card's height down to its own.
  for (const el of document.querySelectorAll(".nrow.ghost.fresh")) {
    const g = ghosts.get(el.dataset.detail);
    if (!g?.h || matchMedia("(prefers-reduced-motion: reduce)").matches) continue;
    const to = el.offsetHeight;
    if (Math.abs(g.h - to) > 2) el.animate([{ height: `${g.h}px`, overflow: "hidden" }, { height: `${to}px`, overflow: "hidden" }], { duration: 280, easing: "cubic-bezier(.2,.8,.2,1)" });
  }
  placeForward();
  saveSpot();
  // A new selection whose card is scrolled out of its column (under Recently answered, or above):
  // glide it into view, just enough to show it. Only when the selection changes, so it never fights you.
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
    const was = onBefore.get(list);
    if (!full && (repick || !was || was.full)) glideIntoView(el);
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
  document.querySelectorAll("textarea[data-text]").forEach(grow);
  // The chat jumps to its end when you open something else; a new row follows only if you were at the end.
  const chat = document.querySelector("[data-chat]");
  const key = chat ? `${active?.id}|${active?.sid}` : null;
  // New target: jump to the end. Same chat, you were at the end (within 24 px, measured before the patch):
  // stay there (an image added to the box, or a step landing, mustn't push the last lines out of view).
  // Scrolled up to read: a row arriving leaves your place alone.
  // Back from a background agent's or script's view (chat.js closeHelperView): the end, whatever the view's scroll was.
  if (chat && chatToEnd) { pinChatEnd(chat); holdChat = null; }
  // A reply just opened in the reader (chat.js readerPane): its start, wherever the chat was.
  else if (chat && !kept && chat.dataset.chatFor?.startsWith("r|")) { chat.scrollTop = 0; holdChat = null; }
  else if (chat && holdChat != null) { chat.scrollTop = chat.scrollHeight - holdChat; holdChat = null; }
  else if (chat && (key !== activeKey || chatPinned)) chat.scrollTop = chat.scrollHeight;
  // Drawn afresh for the same chat (its steps feed was replaced): where you were.
  else if (chat && !kept && oldChat?.dataset.chatFor === chat.dataset.chatFor && lifted.top != null) chat.scrollTop = lifted.top;
  activeKey = key;
  // A reply being read out: what it's reading stays in view, over the chat's jump to its end (chat.js listenFollow).
  if (listening) listenFollow(listening);
  // Which chat is open, and the window's color, told to an add-on (once each time either changes).
  const openNow = chat ? active?.sid || null : null, swatch = themeIdx(), hue = swatch === OWN ? ownHue() : null;
  const told = `${openNow}|${swatch}|${hue?.h}|${hue?.k}`;
  if (told !== chatOpenTold) { chatOpenTold = told; T.event.emit("chat-open", { sid: openNow, swatch, hue }).catch(() => {}); }
  // Something new opened in Active: its text box takes the cursor, so you can just type. Done once it
  // has (at launch the first draw comes before the sessions do: no box yet, so the next draw tries again).
  const target = `${active?.id}|${active?.sid}`;
  if (target !== focusedTarget && !sheet && !focusKey && focusComposer()) focusedTarget = target;
  saveDrafts();
  if (focusKey) { const el = document.querySelector(`[data-text="${CSS.escape(focusKey)}"]`); if (el) { el.focus(); el.setSelectionRange(...caret); } }
  else if (focusId) { const el = document.getElementById(focusId); if (el) { el.focus(); if (caret) el.setSelectionRange(...caret); } }
  // Something over the chat closed (Esc, a click outside, a pick): its text box takes the cursor back,
  // unless you closed it by clicking into another box.
  const over = !!(sheet || lightbox || liveOpen || menuFor || redirectFor || renaming || btwFor || deleteAsk);
  if (overWas && !over && !document.activeElement?.matches?.("input, textarea, select")) focusComposer();
  overWas = over;
  fitHead();
  // By the way's panel drops from just under the chat's header (whatever height that has now).
  const bp = document.querySelector(".btw-pop"), ah = document.querySelector(".ap-head");
  if (bp && ah) bp.style.top = `${ah.offsetTop + ah.offsetHeight + 2}px`;
  placeTerm(!!termHost && termHost.contains(focused));
  showCopied();
  fitTop();
  placeColors();
}
addEventListener("resize", () => { fitHead(); fitTop(); markMore(); });
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

/** Hover text that shows at once, inside Cue's window: the browser's own (from `title`) waits about a
 *  second and doesn't show unless Cue is the app in front. Anything with a title gets it; with
 *  `data-cut` (the hover is just the text you see), only when that text is cut off. */
function bindTips() {
  const tip = document.createElement("div");
  tip.className = "tip";
  document.body.append(tip);
  let on = null;
  const hide = () => { tip.classList.remove("show"); on = null; };
  document.addEventListener("mouseover", (e) => {
    const el = e.target.closest?.("[title], [data-tip]");
    if (!el) return hide();
    // Take the title, so the browser's own doesn't show as well.
    if (el.hasAttribute("title")) { el.dataset.tip = el.getAttribute("title"); el.removeAttribute("title"); }
    if (el === on || !el.dataset.tip) return;
    // Cut off: the text, or a part of it (a path's start, cut in the middle), doesn't fit.
    const cut = (x) => x.scrollWidth > x.clientWidth + 1 || x.scrollHeight > x.clientHeight + 1;
    if (el.hasAttribute("data-cut") && !cut(el) && ![...el.children].some(cut)) return hide();
    on = el;
    tip.textContent = el.dataset.tip;
    const r = el.getBoundingClientRect();
    tip.style.maxWidth = "320px";
    tip.classList.add("show");
    const w = tip.offsetWidth, h = tip.offsetHeight;
    const x = Math.min(Math.max(8, r.left + r.width / 2 - w / 2), innerWidth - w - 8);
    const below = r.bottom + 8 + h < innerHeight;
    tip.style.left = `${x}px`;
    tip.style.top = `${below ? r.bottom + 6 : r.top - h - 6}px`;
  });
  document.addEventListener("mousedown", hide);
  document.addEventListener("scroll", hide, true);
  addEventListener("blur", hide);
}
function bindMain() {
  bindTips();
  document.body.append(picker);
  document.addEventListener("scroll", placeForward, true);
  // Near the top of the chat: the next page of earlier messages comes in above. Only the chat itself (a
  // background agent's view fills the same element: its steps' top isn't the chat's), and only when it's
  // you scrolling: not the scroll events of a chat code just put at its end (chat.js pinChatEnd).
  document.addEventListener("scroll", (e) => {
    if (!e.target?.matches?.("[data-chat]") || e.target.scrollTop > 60 || /^[hr]\|/.test(e.target.dataset.chatFor || "") || Date.now() - chatPinnedAt < 400) return;
    const cur = current(), sid = cur?.s?.session_id || cur?.it?.session_id;
    if (sid) loadOlder(sid);
  }, true);   // the send-to menu follows its button
  // You scrolling the chat (the wheel or trackpad, or a press on its scroll bar, which lands on the chat itself):
  // a reply being read out stops scrolling it along (chat.js listenScrolled).
  document.addEventListener("wheel", (e) => { const c = e.target.closest?.("[data-chat]"); if (c) listenScrolled(c); }, { passive: true, capture: true });
  document.addEventListener("pointerdown", (e) => { if (e.target.matches?.("[data-chat]")) listenScrolled(e.target); }, true);
  const app = document.getElementById("app");
  // Where a click began: the live drop-down redraws every few seconds, so a click that began inside it can
  // end on a fresh copy and reach the page as a click "outside". Only a press that began outside closes it.
  let downInLive = false;
  document.addEventListener("mousedown", (e) => { downInLive = !!e.target.closest?.(".lv-wrap"); }, true);
  // Into a session's box: start reading its "/" commands now (that takes a few seconds the first
  // time in a folder), so they're usually in by the time you type "/".
  app.addEventListener("focusin", (e) => { const sid = sidOfKey(e.target.dataset?.text || ""); if (sid) cmdList(sid); });
  app.addEventListener("click", (e) => {
    const t = e.target;
    // An add-on's open menu closes on a click anywhere else.
    if (extPickOpen && !t.closest(".xp-menu, [data-ext-open]")) { extPickOpen = null; renderMain(); }
    if (lightbox) { lightbox = null; return renderMain(); }
    // A queued message, sent now: stopping the turn (Esc) makes the agent read it straight away.
    // Clear: forget this session's side questions (they were only ever in this window).
    const bc = t.closest("[data-act=btw-clear]");
    if (bc) { btwLog.delete(bc.dataset.sid); renderMain(); return document.querySelector(`[data-text="${CSS.escape(`btw:${bc.dataset.sid}`)}"]`)?.focus(); }
    // A project's dot: its swatches open under it; a swatch sets the color for that folder everywhere.
    const sw = t.closest("[data-pcpick]");
    if (sw) { setProjColor(sw.dataset.pcpick, +sw.dataset.pcidx); colorFor = null; return renderMain(); }
    // Your own Color: the Mac's color picker.
    if (t.closest("[data-pcown]")) return pickOwnColor(t.closest("[data-pcown]"));
    const pc = t.closest("[data-pcolor]");
    if (pc) { const { pcolor: p, pcwhere: w } = pc.dataset; colorFor = colorFor?.p === p && colorFor.w === w ? null : { p, w }; return renderMain(); }
    if (colorFor && !t.closest(".pc-wrap")) { colorFor = null; renderMain(); }
    // The Projects chip: its list, and the project it shows alone (empty: all of them).
    // Renaming a project in the list: its box takes clicks (not a pick); ✎ opens it.
    if (t.closest(".pj-name-in")) return;
    const renB = t.closest("[data-pjren]");
    if (renB) {
      pjRenaming = renB.dataset.pjren;
      draft(`pjname:${pjRenaming}`).text = (state.settings?.project?.names || {})[pjRenaming] || "";
      renderMain();
      const el = document.querySelector(".pj-name-in");
      el?.focus(); el?.select();
      return;
    }
    const pinB = t.closest("[data-pjpin]");
    if (pinB) { togglePin(pinB.dataset.pjpin); return renderMain(); }
    const pj = t.closest("[data-pj]");
    if (pj) {
      pjRenaming = null;
      if (pj.dataset.pj === "toggle") pjOpen = !pjOpen;
      else {
        const was = projFilter;
        projFilter = pj.dataset.project || ""; pjOpen = false; try { localStorage.setItem("cue.projFilter", projFilter); } catch {}
        if (projFilter !== was) openTopWaiting();
      }
      return renderMain();
    }
    if (pjOpen && !t.closest(".pj-wrap")) { pjOpen = false; pjRenaming = null; renderMain(); }
    const mach = t.closest("[data-mach]");
    if (mach) return machAct(mach.dataset.mach, mach.dataset.name);
    const tm = t.closest("[data-act=term], [data-act=term-open], [data-act=term-card]");
    if (tm) {
      if (tm.dataset.act === "term") return termSid === tm.dataset.sid ? closeTerm() : openTerm(tm.dataset.sid);
      // From a Waiting card: open its session, then its terminal.
      if (tm.dataset.act === "term-card") { const it = findItem(tm.dataset.id); setActive(tm.dataset.id); return it && openTerm(it.session_id); }
      return termSid === tm.dataset.sid ? termObj?.focus() : openTerm(tm.dataset.sid);
    }
    const bw = t.closest("[data-act=btw], [data-act=btw-close]");
    if (bw) { if (bw.dataset.act === "btw" && btwFor !== bw.dataset.sid) return openBtw(bw.dataset.sid); btwFor = null; return renderMain(); }
    // Catch me up above the box: asks once; Got it folds it away.
    const cu = t.closest("[data-act=catch-up], [data-act=catch-up-got]");
    if (cu) return cu.dataset.act === "catch-up" ? askCatchUp(cu.dataset.sid) : gotCatchUp(cu.dataset.sid);
    const wt = t.closest("[data-act=watch]");
    if (wt) {
      const on = wt.dataset.on === "1", s = sessionOf(wt.dataset.sid);
      if (s) s.watch_ms = on ? Date.now() : 0;   // shown now; Cue's next update says the same
      renderMain();
      return invoke("set_watch", { sessionId: wt.dataset.sid, on }).catch((e) => toast(`Couldn't: ${e}`));
    }
    const sn = t.closest("[data-act=send-now]");
    if (sn) return sendQueuedNow(sn.dataset.sid);
    const uq = t.closest("[data-act=unqueue-review]");
    if (uq) return invoke("unqueue_review", { exec: uq.dataset.exec }).then(() => toast("Taken off the queue")).catch((e) => toast(`Couldn't: ${e}`));
    const he = t.closest("[data-act=held-edit]");
    if (he) return pullHeld(he.dataset.sid);
    const hn = t.closest("[data-act=held-now]");
    if (hn) return sendHeldNow(hn.dataset.sid);
    if (t.closest("[data-setup-done]")) { sheet = null; setSetting("setup.done", true); return renderMain(); }
    if (t.closest("[data-connect-all]")) {
      const todo = ["claude", "codex", "pi"].filter((h) => { const c = state.connections?.[h]; return c && !c.ok && c.present; });
      (async () => { for (const h of todo) { try { toast(await invoke("connect_agent", { harness: h })); } catch (e) { toast(`Couldn't connect ${h}: ${e}`); } } })();
      return;
    }
    // The ⋯ menu: toggle it, run an item, or close it on a click anywhere else.
    const mb = t.closest("[data-more]");
    if (mb) { moreFor = moreFor === mb.dataset.more ? null : mb.dataset.more; handoffArm = newProjFor = null; movingProj = mcpPicking = false; return renderMain(); }
    if (t.closest("[data-move-ask]")) { movingProj = true; return renderMain(); }
    const np = t.closest("[data-new-proj]");
    if (np) { newProjFor = np.dataset.newProj; newProjNote = ""; draft(`newproj:${newProjFor}`).text = ""; renderMain(); return document.querySelector(".pm-new-in")?.focus(); }
    const mvp = t.closest("[data-move-proj]");
    if (mvp) { moreFor = handoffArm = null; movingProj = false; moveToProject(mvp.dataset.sid, mvp.dataset.moveProj); return renderMain(); }
    const rsb = t.closest("[data-restart]");
    if (rsb) {
      const sid = rsb.dataset.restart;
      moreFor = handoffArm = null;
      renderMain();
      // Its "/" commands are read again after (a skill added since shows up): the Mac forgets its list too.
      return invoke("restart_session", { sessionId: sid }).then((m) => { cmdLists.delete(sid); cmdMissed.delete(sid); if (m) toast(m); }, (e) => toast(`Couldn't restart: ${e}`));
    }
    const mc = t.closest("[data-cmd]");
    if (mc) {
      const { cmd, sid } = mc.dataset;
      if (cmd === "handoff" && handoffArm !== sid) { handoffArm = sid; return renderMain(); }
      moreFor = handoffArm = null;
      renderMain();
      invoke("session_command", { sessionId: sid, action: cmd }).then((m) => toast(m), (e) => toast(`Couldn't: ${e}`));
      return;
    }
    if (moreFor && !t.closest(".more-wrap")) { moreFor = handoffArm = newProjFor = null; movingProj = mcpPicking = false; renderMain(); }
    if (t.closest(".more-menu [data-park]")) moreFor = handoffArm = null;   // Decide later: done with the menu too
    const cb = t.closest("[data-connect]");
    if (cb) { cb.disabled = true; invoke("connect_agent", { harness: cb.dataset.connect }).then((m) => toast(m), (e) => { toast(`Couldn't connect: ${e}`); cb.disabled = false; }); return; }
    const ub = t.closest("[data-upd]");
    if (ub) { const a = ub.dataset.upd; if (a === "check") checkUpdate(); else if (a === "install") installUpdate(); else { upd = { ...upd, offer: false }; renderMain(); } return; }
    const tf = t.closest("[data-turn-fold]");
    if (tf) { const k = tf.dataset.turnFold; turnFold.set(k, tf.dataset.open !== "1"); renderMain(); return wantDetails(k.split("|")[0]); }
    const ta = t.closest("[data-turn-all]");
    if (ta) { const k = ta.dataset.turnAll; turnAll.has(k) ? turnAll.delete(k) : turnAll.add(k); renderMain(); return wantDetails(k.split("|")[0]); }
    const cc = t.closest("[data-copy-code]");
    if (cc) return copyCode(cc);
    const td = t.closest("[data-tray-done]");
    if (td) { const k = td.dataset.trayDone; trayDone.has(k) ? trayDone.delete(k) : trayDone.add(k); return renderMain(); }
    // A card's "N bg": open its session (its item, on a waiting card) with the newest still running in full.
    const bo = t.closest("[data-bg-open]");
    if (bo) { const sid = bo.dataset.bgOpen, it = bo.closest("[data-big]")?.dataset.big; helperView = { sid, id: null }; return it ? setActive(it) : openSession(sid); }
    const hb = t.closest("[data-helper]");
    if (hb) { const [sid, id] = hb.dataset.helper.split("|"); helperView = { sid, id }; return renderMain(); }
    // Back to the chat (Go to chat, ‹ Back to chat, the lead's chip): with the cursor in its box, ready to write.
    if (t.closest("[data-helper-back]")) { closeHelperView(); return focusComposer(); }
    const ls = t.closest("[data-listen]");
    if (ls) { const k = ls.dataset.listen; return listening?.key === k ? listenToggle() : listenPlay(k.slice(0, k.lastIndexOf(":")), k, !!ls.dataset.listenReader); }
    if (t.closest("[data-listen-toggle]")) return listenToggle();
    if (t.closest("[data-listen-stop]")) return listenStop();
    const lk = t.closest("[data-listen-skip]");
    if (lk) return listenSkip(+lk.dataset.listenSkip);
    const lsp = t.closest("[data-listen-speed]");
    if (lsp) return listenFaster(+lsp.dataset.listenSpeed);
    if (t.closest("[data-listen-then]")) return listenThenNext();
    if (t.closest("[data-voice-list]")) {
      voiceListOpen = !voiceListOpen;
      renderMain();
      // Opened: the voice in use in view, a line or two from the top.
      const list = document.querySelector(".vp-list"), on = list?.querySelector(".vp-line.on");
      if (on) list.scrollTop = on.offsetTop - 44;
      return;
    }
    const vs = t.closest("[data-voice-sample]");
    if (vs) return playSample(vs.dataset.voiceSample);
    const vg = t.closest("[data-voice-get]");
    if (vg) return getVoice(vg.dataset.voiceGet);
    const vp = t.closest("[data-voice-pick]");
    if (vp) { voiceListOpen = false; return pickVoice(vp.dataset.voicePick); }
    if (t.closest("[data-kokoro-remove]")) { kokoroTell({ remove: true }); return; }
    const lo = t.closest("[data-listen-open]");
    if (lo) return setActive(null, lo.dataset.listenOpen);
    if (!t.closest("button, a, input") && listenFrom(e)) return;
    const rd = t.closest("[data-read]");
    if (rd) { const k = rd.dataset.read; return openReader(k.slice(0, k.lastIndexOf(":")), k); }
    if (t.closest("[data-reader-back]")) return closeReader();
    const rt = t.closest("[data-reader-term]");
    if (rt) { closeReader(); return openTerm(rt.dataset.readerTerm); }
    if (t.closest("[data-reader-reply]")) return replyFromReader();
    if (t.closest("[data-reader-copy]")) return copyReader();
    const rs = t.closest("[data-reader-step]");
    if (rs) return stepReader(+rs.dataset.readerStep);
    const rz = t.closest("[data-reader-size]");
    if (rz) return sizeReader(+rz.dataset.readerSize);
    const sl = t.closest("[data-step]");
    if (sl) { const k = sl.dataset.step; stepFlip.has(k) ? stepFlip.delete(k) : stepFlip.add(k); renderMain(); return wantDetails(k.split("|")[0]); }
    const qp = t.closest("[data-quick]");
    if (qp) {
      const key = qp.dataset.quick, d = draft(key);
      d.text = withPhrase(d.text, qp.dataset.phrase);
      saveDrafts();
      renderMain();
      // …and it goes, the way that box's Send would send it.
      document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`)?.closest(".composer")?.querySelector(".btn.send")?.click();
      return;
    }
    const cr = t.closest("[data-cmd-refresh]");
    if (cr) { cmdList(sidOfKey(cr.dataset.cmdRefresh), true); renderMain(); return document.querySelector(`[data-text="${CSS.escape(cr.dataset.cmdRefresh)}"]`)?.focus(); }
    const cp = t.closest("[data-cmd-pick]");
    if (cp) return pickCmd(cp.dataset.cmdPick, cp.dataset.cmdName, false);
    const st = t.closest("[data-astext]");
    if (st) {
      const key = st.dataset.astext, el = document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`);
      draft(key).text = ` ${draft(key).text.trimStart()}`;
      saveDrafts();
      st.parentElement.hidden = true;
      refreshCmd(key);
      if (el) { el.value = draft(key).text; el.focus(); el.selectionStart = el.selectionEnd = el.value.length; }
      return;
    }
    { const fd = t.closest("[data-fold]"); if (fd) return foldDrawer(fd.dataset.fold); }
    if (t.closest("[data-fold-oldest]")) return foldOldest();
    // An ended Claude Code session, opened from search: back to work, as an older one resumes.
    if (t.closest("[data-act=convo-resume]")) return resumeOlder({ session_id: convo.sid, name: convo.name });
    if (t.closest("[data-act=convo-close]")) { sheet = null; return renderMain(); }
    if (t.closest("[data-act=convo-back]")) return openFind();
    const co = t.closest("[data-act=convo-open]");
    if (co) { sheet = null; return setActive(null, co.dataset.sid); }
    const sm = t.closest("[data-srmore]");
    if (sm) { searchMore.add(sm.dataset.srmore); renderMain(); return document.querySelector('[data-text="find-live"]')?.focus(); }
    // Resume on a closed session's row: back to work at once (the row itself opens its conversation).
    const rsm = t.closest("[data-resume]");
    if (rsm) return resumeOlder({ session_id: rsm.dataset.resume, name: rsm.dataset.name });
    const sr = t.closest("[data-sr]");
    if (sr) return pickResult(+sr.dataset.sr);
    const starEl = t.closest("[data-star]");
    if (starEl) return toggleStar(starEl.dataset.star);
    const rn = t.closest("[data-rename]");
    if (rn) {
      renaming = rn.dataset.rename;
      draft(`rename:${renaming}`).text = sessionOf(renaming)?.name || "";
      renderMain();
      const el = document.querySelector(".rename-in");
      el?.focus(); el?.select();
      return;
    }
    const mic = t.closest("[data-mic]");
    if (mic) return toggleDictation(mic.dataset.mic);
    const pb = t.closest("[data-park]");
    if (pb) { const [sid, on] = pb.dataset.park.split(/:(?=[01]$)/); return setParked(sid, on === "1"); }
    const fg = t.closest("[data-forget]");
    if (fg) return invoke("forget_session", { sessionId: fg.dataset.forget }).then(() => toast("Removed from Cue")).catch((e) => toast(`Couldn't: ${e}`));
    const hide = t.closest("[data-hide-idle]");
    if (hide) return hideIdle(hide.dataset.hideIdle);
    const fwdBtn = t.closest("[data-fwd]");
    if (fwdBtn && fwdMsgs.has(fwdBtn.dataset.fwd)) { forward = { ...fwdMsgs.get(fwdBtn.dataset.fwd), key: fwdBtn.dataset.fwd, to: null }; sheet = "forward"; return renderMain(); }
    const fwdTo = t.closest("[data-fwd-to]");
    if (fwdTo && forward) { forward.to = fwdTo.dataset.fwdTo; renderMain(); return document.querySelector(".fwd-note")?.focus(); }
    if (t.closest("[data-act='send-forward']")) return sendForward();
    if (t.closest("[data-act='cancel-forward']")) { sheet = forward = null; return renderMain(); }
    // A path: ⌘-click opens it (or shows it in Finder); a plain click stays what it was (text, or the row it's in).
    const pl = t.closest("a[data-path]");
    if (pl) { e.preventDefault(); if (e.metaKey || e.ctrlKey) return invoke("open_link", { url: pl.dataset.path }).catch((err) => toast(`Couldn't open: ${err}`)); }
    const link = t.closest("a[data-href]");
    if (link) { e.preventDefault(); return invoke("open_link", { url: link.dataset.href }).catch((err) => toast(`Couldn't open: ${err}`)); }
    const llb = t.closest("[data-local-lb]");
    if (llb) {
      const { paths, i } = JSON.parse(llb.dataset.localLb);
      const srcs = paths.map((p) => localImgs.get(p)).filter(Boolean);
      if (srcs.length) { lightbox = { srcs, i: Math.min(i, srcs.length - 1) }; return renderMain(); }
      return;
    }
    const lb = t.closest("[data-lightbox]");
    if (lb) { lightbox = JSON.parse(lb.dataset.lightbox); return renderMain(); }
    const dlb = t.closest("[data-draft-lightbox]");
    if (dlb) { const [key, n] = dlb.dataset.draftLightbox.split(/:(?=\d+$)/); lightbox = { srcs: draft(key).images.map((im) => im.data), i: +n }; return renderMain(); }
    if (t.matches(".scrim")) { sheet = null; if (t.matches(".spot-scrim")) liveOpen = newOpen = false; return renderMain(); }
    const us = t.closest("[data-usage]");
    if (us) {
      const k = us.dataset.usage;
      if (k === "resend") return resend(limitedSessions().filter((s) => lifted(s.limit)).map((s) => s.session_id));
      if (k === "resend-one") return resend([us.dataset.sid]);
      if (k === "out" || k === "model") {
        // Step through the paused sessions, oldest first.
        const list = limitedSessions().filter((s) => (k === "out") === (s.limit.scope === "all")).sort((a, b) => a.since_ms - b.since_ms);
        const at = list.findIndex((s) => s.session_id === active?.sid);
        const nx = list[(at + 1) % list.length];
        return nx && setActive(null, nx.session_id);
      }
      usageOpen = !usageOpen;
      return renderMain();
    }
    if (usageOpen && !t.closest(".usage-pop")) { usageOpen = false; renderMain(); }
    const hs = t.closest("[data-chip-step]");
    if (hs) { const [kind, dir] = hs.dataset.chipStep.split(":"); return openFromChip(kind, +dir); }
    const hc = t.closest("[data-chip]");
    if (hc) return openFromChip(hc.dataset.chip);
    const xp = t.closest("button[data-ext-pick]");
    if (xp) return setExtPick(xp.dataset.extPick, xp.dataset.val);
    const xo = t.closest("[data-ext-open]");
    if (xo) { extPickOpen = extPickOpen === xo.dataset.extOpen ? null : xo.dataset.extOpen; return renderMain(); }
    const setBtn = t.closest("[data-set]");
    if (setBtn) return setSetting(setBtn.dataset.set, JSON.parse(setBtn.dataset.val));
    const at = t.closest("[data-attach]");
    if (at) { picker.dataset.key = at.dataset.attach; picker.value = ""; return picker.click(); }
    const un = t.closest("[data-unattach]");
    if (un) { const [key, n] = un.dataset.unattach.split(/:(?=\d+$)/); draft(key).images.splice(+n, 1); return renderMain(); }
    const hk = t.closest("[data-hook-fold]");
    if (hk) { const k = hk.dataset.hookFold; hookOpen.has(k) ? hookOpen.delete(k) : hookOpen.add(k); return renderMain(); }
    const fo = t.closest("[data-follow]");
    if (fo) { const id = fo.dataset.follow; openFollow.has(id) ? openFollow.delete(id) : openFollow.add(id); return renderMain(); }
    const stop = t.closest("[data-act=interrupt]");
    if (stop) return interrupt(stop.dataset.sid);
    const st_ = t.closest("[data-act=send-to]");
    if (st_) return sendTo(st_.dataset.sid, !!st_.dataset.now);
    if (nfFolderOpen && !t.closest(".nf-fwrap")) { closeFolders(); renderMain(); }
    const lv = t.closest("[data-lv]");
    if (lv) return lvAct(lv.dataset.lv, lv.dataset);
    // A team button (auto, Diff, Review) inside a row that opens its session: the button, not the row.
    const ca = t.closest("[data-crew]");
    if (ca) return crewAct(ca.dataset.crewSid, ca.dataset.crew);
    // A remote executor's row in its lead's tree: its own terminal there (see openExec).
    const xt = t.closest("[data-exec-term]");
    if (xt) return openExec(xt.dataset.execTerm);
    // A button in the row (Allow / Deny in the starred list) does its own thing, not "open".
    const sv = t.closest("button[data-act], button[data-wake]") ? null : t.closest("[data-sv]");
    if (sv) { if (liveOpen && sv.dataset.sv !== "close") liveOpen = false; return svAct(sv.dataset.sv, sv.dataset.sid); }
    if (liveOpen && !downInLive && !t.closest(".lv-wrap")) { liveOpen = newOpen = false; renderMain(); }
    const rn2 = t.closest("[data-act=rename-again], [data-act=rename-dismiss]");
    if (rn2) {
      const sid = rn2.dataset.sid;
      if (rn2.dataset.act === "rename-again") return submitRename(sid, true);
      const x = sessionOf(sid);
      if (x) delete x.rename_error;
      renderMain();
      return invoke("rename_dismiss", { sessionId: sid }).catch(() => {});
    }
    const cs = t.closest("[data-act=continue-start]");
    if (cs) { invoke("continue_start", { sessionId: cs.dataset.sid }).then(() => toast("Pressed Enter: it's starting")).catch((e) => toast(`Couldn't: ${e}. Answer it in its terminal.`)); return; }
    const tr = t.closest("[data-act=trust]");
    if (tr) { invoke("trust_folder", { sessionId: tr.dataset.sid }).then(() => toast("Trusted: it's starting")).catch((e) => toast(`Couldn't answer it: ${e}. Answer it in its terminal.`)); return; }
    const goS = t.closest("[data-act=go-session]");
    if (goS) { invoke("focus_session_id", { sessionId: goS.dataset.sid }).then((r) => toast(`Jumped to ${r}`)).catch((e) => toast(`Couldn't jump: ${e}`)); return; }
    // Parked: Resume (↺ on its row, or the button), Resume & send, Unpark, Park, its earlier messages, fold.
    // ↺ on a row of one whose cache is gone and whose context is big opens it first, to say what resuming costs
    // (once: with it open, ↺ resumes).
    const wk = t.closest("[data-wake]");
    if (wk) return wk.closest(".active-pane") || active?.sid === wk.dataset.wake ? wakeParked(wk.dataset.wake) : insColdResume(wk.dataset.wake).then((cold) => (cold ? setActive(null, wk.dataset.wake) : wakeParked(wk.dataset.wake)));
    const wsd = t.closest("[data-wake-send]");
    if (wsd) return wakeParked(wsd.dataset.wakeSend, draft(`s:${wsd.dataset.wakeSend}`).text.trim());
    const svf = t.closest("[data-sv-fold]");
    if (svf) return svFold(svf.dataset.svFold);
    const sva = t.closest("[data-sv-all]");
    if (sva) return showAllSvRest(sva);
    const shw = t.closest("[data-shelf]");
    if (shw) return pickShelf(shw.dataset.shelf);
    const pkc = t.closest("[data-park-closed]");
    if (pkc) return parkClosed(pkc.dataset.parkClosed);
    const upk = t.closest("[data-unpark]");
    if (upk) return unparkSession(upk.dataset.unpark);
    const pks = t.closest("[data-park-session]");
    if (pks) { moreFor = handoffArm = null; return parkSession(pks.dataset.parkSession); }
    const dla = t.closest("[data-delete-ask]");
    if (dla) { moreFor = handoffArm = null; deleteAsk = dla.dataset.deleteAsk; document.activeElement?.blur(); return renderMain(); }
    const pka = t.closest("[data-park-all]");
    if (pka) { parkedAll = parkedAll === pka.dataset.parkAll ? null : pka.dataset.parkAll; return renderMain(); }
    const ss = t.closest("[data-session]");
    if (ss) return openSession(ss.dataset.session);
    const vb = t.closest("[data-view]");
    if (vb) { view = vb.dataset.view; return renderMain(); }
    const hf = t.closest("[data-hfilter]");
    if (hf) { histFilter = hf.dataset.hfilter; histOpen = null; return renderMain(); }
    const hg = t.closest("[data-hgroup]");
    if (hg) { const k = hg.dataset.hgroup; histOpenGroups.has(k) ? histOpenGroups.delete(k) : histOpenGroups.add(k); return renderMain(); }
    const hr = t.closest("[data-hrow]");
    if (hr) { histOpen = histOpen === hr.dataset.hrow ? null : hr.dataset.hrow; return renderMain(); }
    const dt = t.closest("[data-detail]");
    if (dt) return setActive(dt.dataset.detail, null, true);
    const mo = t.closest("[data-msg]");
    if (mo) { const k = mo.dataset.msg; openMsgs.has(k) ? openMsgs.delete(k) : openMsgs.add(k); return renderMain(); }
    const va = t.closest("[data-voice-ask]");
    if (va) { const k = va.dataset.voiceAsk; openAsks.has(k) ? openAsks.delete(k) : openAsks.add(k); return renderMain(); }
    const big = t.closest("[data-big]");
    if (big && !t.closest("button")) return setActive(big.dataset.big);
    const qs_ = t.closest("[data-qstep]");
    if (qs_) { const [id, n] = qs_.dataset.qstep.split(/:(?=\d+$)/); qStep[id] = +n; return renderMain(); }
    const tp = t.closest("[data-tpick]");
  if (tp) { const it = findItem(tp.dataset.id); if (it) active = { id: it.id, sid: it.session_id }; return invoke("answer_terminal", { id: tp.dataset.id, n: +tp.dataset.tpick }).then((r) => toast(r)).catch((e) => toast(`Couldn't answer it: ${e}`)); }
  const pk = t.closest("[data-pick]");
    if (pk) { const [qi, oi] = pk.dataset.pick.split(":").map(Number); const it = findItem(pk.dataset.id); return it && pick(it, qi, oi); }
    const al = t.closest("[data-always]");
    if (al) { const it = findItem(al.dataset.id); return it && respond(it, { behavior: "allow_always", permission: it.suggestions[+al.dataset.always] }); }
    const cbtn = t.closest("[data-collapse]");
    if (cbtn) return collapseSection(cbtn.dataset.collapse);
    const obtn = t.closest("[data-open-section]");
    if (obtn) return openSection(obtn.dataset.openSection);
    const crst = t.closest("[data-col-reset]");
    if (crst) return resetCol(crst.dataset.colReset);
    const actEl = t.closest("[data-act]");
    const act = actEl?.dataset.act;
    if (act?.startsWith("ins-")) return insAct(act, actEl.dataset);
    if (act === "next") return goNext();
    if (act === "set-tab") { setTab = actEl.dataset.tab; try { localStorage.setItem("cue.setTab", setTab); } catch {} const b = document.querySelector(".sheet-body"); if (b) b.scrollTop = 0; return renderMain(); }
    if (act === "test-notify") return invoke("test_notification").then(toast).catch((e) => toast(`Couldn't send: ${e}`));
    if (act === "open-history") { view = "history"; sheet = null; return renderMain(); }
    if (act === "open-search") return openFind();
    if (act === "shells") return toggleShells();
    if (act === "load-older") return loadOlder(actEl.dataset.sid);
    if (act === "load-transcript") return loadOlder(actEl.dataset.sid, true);
    if (act === "open-settings") { sheet = sheet === "settings" ? null : "settings"; extSections = null; return renderMain(); }
    // Buttons that name a session, not a card (no data-id): the "!" cards, Send again, Move to Cue.
    if (act === "move-cue") return actEl.dataset.why ? toast(`Move to Cue: ${actEl.dataset.why}`) : moveToCue(actEl.dataset.sid);
    if (act === "move-dismiss") { moving = null; return renderMain(); }
    if (act === "delete-yes" || act === "delete-no") return deleteAnswer(act === "delete-yes");
    if (act === "resend") return invoke("send_to_session", { sessionId: actEl.dataset.sid, text: actEl.dataset.resend, images: [], now: false }).then((r) => toast(`Sent ${r}`)).catch((x) => toast(`Couldn't send: ${x}`));
    if (act === "ran-again" || act === "ran-send" || act === "ran-term") {
      const who = actEl.dataset.who, e = (ranLog.get(who) || [])[+actEl.dataset.n];
      if (!e) return;
      if (act === "ran-again") return runLine(who, e.line, e.cwd, e.machine);
      if (act === "ran-send") return invoke("send_to_session", { sessionId: who, text: ranAsMessage(e), images: [], now: false }).then((r) => toast(`Sent ${r}`)).catch((x) => toast(`Couldn't send: ${x}`));
      return invoke("send_to_session", { sessionId: who, text: `! ${e.line}`, images: [], now: false }).then(() => toast("Typed into its terminal")).catch((x) => toast(`Couldn't: ${x}`));
    }
    const it = actEl?.dataset.id ? findItem(actEl.dataset.id) : null;
    if (act && it) {
      if (it.status === "pending" && act !== "dismiss" && act !== "back") active = { id: it.id, sid: it.session_id };
      if (act === "submit-answers") { const a = answersFor(it); return a && respond(it, { behavior: "allow", answers: a }); }
      if (act === "allow") return allow(it);
      if (act === "deny") return deny(it);
      if (act === "send") return submitText(it);
      if (act === "commit") return invoke("reply", { id: it.id, text: "Commit", images: [] }).then((r) => toast(`“Commit” ${r}`)).catch((e) => toast(`Couldn't send: ${e}`));
      if (act === "continue") return invoke("reply", { id: it.id, text: "continue", images: [] }).then((r) => toast(`“continue” ${r}`)).catch((e) => toast(`Couldn't send: ${e}`));
      if (act === "go") return goTo(it);
      if (act === "dismiss") { cleared.add(it.id); return invoke("dismiss", { id: it.id }); }
      if (act === "back") return invoke("send_to_back", { id: it.id }).catch((e) => toast(`Couldn't send to back: ${e}`));
      if (act === "menu") { menuFor = menuFor === it.id ? null : it.id; return renderMain(); }
      if (act === "redirect") { redirectFor = it.id; renderMain(); return document.querySelector(`[data-text="${CSS.escape(it.id)}"]`)?.focus(); }
    }
  });
  app.addEventListener("input", (e) => {
    if (e.target.matches?.("[data-hsession]")) { histSession = e.target.value; histOpen = null; return renderMain(); }
    if (e.target.matches?.("[data-quick-edit]")) { qpEdit = [...document.querySelectorAll("[data-quick-edit]")].map((i) => i.value); return; }
    const id = e.target.dataset?.text;
    if (!id) return;
    draft(id).text = e.target.value;
    if (id === "find-live") { liveSel = 0; return runSearch(e.target.value); }
    if (id === "find-sessions") return renderMain();
    if (id === "new-cwd") { nfFolderOpen = true; nfFolderQ = e.target.value; nfFolderSel = -1; saveDrafts(); return renderMain(); }
    if (id === "new-model-id") { newModel = e.target.value.replace(/\s+/g, ""); remember("newModel", newModel); remember("newModelOther", newModel); return; }
    saveDrafts();
    if (cmdShut === id) cmdShut = null;
    cmdSel = 0;
    refreshCmd(id);
    grow(e.target);
    const b = e.target.closest(".composer")?.querySelector(".btn.send");
    if (b && !b.dataset.held) b.disabled = !e.target.value.trim() && !draft(id).images.length;
  });
  // Images: 📎 picks a file; paste and drag-and-drop work on any box that takes them.
  app.addEventListener("paste", async (e) => {
    const key = e.target.closest?.("[data-drop]")?.dataset.drop;
    if (!key) return;
    const cd = e.clipboardData;
    const files = [...(cd?.items || [])].filter((i) => i.kind === "file" && i.type.startsWith("image/")).map((i) => i.getAsFile()).filter(Boolean);
    if (files.length) { e.preventDefault(); return attach(key, files); }
    // The web view sometimes hides a pasted image: when there's no text either, ask macOS directly.
    if (!cd?.getData("text/plain")) {
      e.preventDefault();
      const data = await invoke("clipboard_image");
      if (data) { draft(key).images.push({ name: "pasted image", mime: "image/png", data }); renderMain(); document.querySelector(`[data-text="${CSS.escape(key)}"]`)?.focus(); }
    }
  });
  app.addEventListener("dragover", (e) => { if (e.target.closest?.("[data-drop]")?.dataset.drop) e.preventDefault(); });
  app.addEventListener("drop", (e) => {
    const key = e.target.closest?.("[data-drop]")?.dataset.drop;
    if (!key) return;
    e.preventDefault();
    attach(key, [...(e.dataTransfer?.files || [])].filter((f) => f.type.startsWith("image/")));
  });
  app.addEventListener("change", (e) => {
    if (e.target.matches("[data-keep-custom]")) setSetting("history.keep", Math.max(10, Math.round(+e.target.value || 0)));
    if (e.target.matches("[data-usage-file]")) setSetting("usage.file", e.target.value.trim());
    if (e.target.matches("[data-model]")) {
      // Other…: the id you typed last time comes back, so a one-off pick of a listed model doesn't lose it.
      newModel = e.target.value === OTHER ? remembered("newModelOther") || "" : e.target.value;
      remember("newModel", newModel); renderMain();
      return e.target.value === OTHER && document.querySelector('[data-text="new-model-id"]')?.focus();
    }
    if (e.target.matches("[data-quick-edit]")) { const list = [...document.querySelectorAll("[data-quick-edit]")].map((i) => i.value.trim()).filter(Boolean); qpEdit = null; setSetting("quick.phrases", list); }
  });
  // Coming back to Cue: the cursor goes to the text box unless you were somewhere else in it.
  window.addEventListener("focus", () => { if (!sheet && !lightbox && document.activeElement === document.body) focusComposer(); });
  // While Cue asks whether to delete a session: Esc is no, Enter yes, before a terminal under it takes the key.
  document.addEventListener("keydown", (e) => {
    if (!deleteAsk || (e.key !== "Escape" && e.key !== "Enter")) return;
    e.preventDefault();
    e.stopPropagation();
    deleteAnswer(e.key === "Enter");
  }, true);
  document.addEventListener("keydown", (e) => {
    // Enter in + New's Add machine field adds it.
    if (e.key === "Enter" && e.target.dataset?.text === "mc-host") { e.preventDefault(); return machAct("add"); }
    if (e.key === "Enter" && e.target.dataset?.text === "mc-label" && machRenaming) { e.preventDefault(); return machAct("rename-save", machRenaming); }
    // ⌘J: Cue's own floating terminal, out or tucked away (from inside it too).
    if (e.metaKey && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "j") { e.preventDefault(); return toggleShells(true); }
    // ⌘+ / ⌘− / ⌘0: text size, a step bigger or smaller, or back to Default (from anywhere in Cue).
    if (e.metaKey && !e.altKey && !e.ctrlKey && ["=", "+", "-", "_", "0"].includes(e.key)) { e.preventDefault(); return stepZoom(e.key === "0" ? 0 : e.key === "-" || e.key === "_" ? -1 : 1); }
    // ⌘T: a new shell tab in it (from inside it too).
    if (e.metaKey && !e.shiftKey && !e.altKey && !e.ctrlKey && e.key.toLowerCase() === "t") { e.preventDefault(); return newShellTab(); }
    // Inside it, as in a terminal app: ⌘K clears it (not Cue's ⌘K), ⌘1–9 go to its tabs (⌘9 the last).
    if (e.target.closest?.(".pip") && e.metaKey && !e.shiftKey && !e.altKey && !e.ctrlKey) {
      if (e.key.toLowerCase() === "k") { e.preventDefault(); return clearShell(); }
      if (/^[1-9]$/.test(e.key)) { e.preventDefault(); return shellNumber(+e.key); }
    }
    // Inside it every key is its shell's; only ⌘ shortcuts stay Cue's.
    if (e.target.closest?.(".pip") && !e.metaKey) return;
    // ⌃`: the open session's terminal, open or closed (from inside it too).
    if (e.ctrlKey && e.key === "`") {
      e.preventDefault();
      if (termSid) return closeTerm();
      return active?.sid && inTmux(active.sid) ? openTerm(active.sid) : undefined;
    }
    // Inside the terminal every key is the terminal's (Esc included); only ⌘ shortcuts stay Cue's.
    if (e.target.closest?.(".term-host") && !e.metaKey) return;
    // The "/" menu under a box: arrows move, Tab fills, Enter runs, Esc closes it.
    const mk = e.target.dataset?.text;
    if (mk && !e.isComposing && ["ArrowUp", "ArrowDown", "Tab", "Enter", "Escape"].includes(e.key) && cmdMatches(mk).length && !e.shiftKey) {
      const m = cmdMatches(mk);
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") { cmdShut = mk; return refreshCmd(mk); }
      if (e.key === "ArrowUp" || e.key === "ArrowDown") { cmdSel = (cmdSel + (e.key === "ArrowDown" ? 1 : m.length - 1)) % m.length; return refreshCmd(mk); }
      return pickCmd(mk, m[cmdSel].name, e.key === "Enter");
    }
    // ↑ / ↓ in a session's box: back and forth through the last messages you sent it (chat.js recallSent).
    if (mk?.startsWith("s:") && (e.key === "ArrowUp" || e.key === "ArrowDown") && !e.shiftKey && !e.altKey && !e.metaKey && !e.ctrlKey && !e.isComposing && recallSent(e)) return e.preventDefault();
    if (e.target.dataset?.text === "new-cwd") {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); return stepFolder(e.key === "ArrowDown" ? 1 : -1); }
      // Enter here only ever picks a folder (the highlighted one, else the list's first), never starts the
      // session. Nothing to pick: on to the Message box.
      if (e.key === "Enter" && !e.isComposing) {
        e.preventDefault();
        const m = nfFolderOpen ? folderMatches(nfFolders()) : [], f = m[nfFolderSel] || m[0];
        if (f) return pickFolder(f);
        closeFolders(); renderMain();
        return document.querySelector('[data-text="new-msg"]')?.focus();
      }
      if (nfFolderOpen && e.key === "Escape") { e.preventDefault(); closeFolders(); return renderMain(); }
      if (nfFolderOpen && e.key === "Tab") { closeFolders(); renderMain(); }
    }
    if (renaming && e.key === "Escape") { renaming = null; return renderMain(); }
    if (pjRenaming && e.key === "Escape") { pjRenaming = null; return renderMain(); }
    if (newProjFor && e.key === "Escape") { newProjFor = null; newProjNote = ""; return renderMain(); }
    if ((pjOpen || colorFor) && e.key === "Escape") { pjOpen = false; colorFor = null; return renderMain(); }
    if (liveOpen && e.key === "Escape") {
      liveOpen = newOpen = false;
      return renderMain();
    }
    // ⌘, opens Settings, the Mac way (even while typing in a box).
    if (e.metaKey && !e.ctrlKey && e.key.toLowerCase() === "f") { e.preventDefault(); return openFind(); }
    if (e.metaKey && e.key === ",") { e.preventDefault(); sheet = sheet === "settings" ? null : "settings"; return renderMain(); }
    // ⌘K: every live session in the middle of the window, like Spotlight. ⌘L: the same, under its chip.
    if (e.metaKey && e.key.toLowerCase() === "k") { e.preventDefault(); return openFind(); }
    // ⌘L: every live session (the drop-down by "N live"), even while typing in a box.
    if (e.metaKey && e.key.toLowerCase() === "l") { e.preventDefault(); return lvAct("toggle", {}); }
    // ⌘N: + New session, in the middle of the window like ⌘K (even while typing in a box).
    if (e.metaKey && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "n") { e.preventDefault(); sheet = lightbox = null; liveSpot = true; return lvAct("new", {}); }
    // ⌘E: the open project's actions, to run one (actions.js); ⌘⇧E runs the last one again.
    if (e.metaKey && !e.altKey && !e.ctrlKey && e.key.toLowerCase() === "e") { e.preventDefault(); return e.shiftKey ? actAgain() : actPalToggle(); }
    if (lightbox) {
      if (e.key === "ArrowRight") lightbox.i = (lightbox.i + 1) % lightbox.srcs.length;
      else if (e.key === "ArrowLeft") lightbox.i = (lightbox.i - 1 + lightbox.srcs.length) % lightbox.srcs.length;
      else if (e.key === "Escape") lightbox = null;
      e.preventDefault();
      return renderMain();
    }
    if (e.key === "Escape" && btwFor) { btwFor = null; return renderMain(); }
    // Esc in a background agent's full view: back to the chat.
    if (e.key === "Escape" && !sheet && !lightbox && helperView && helperViewed(helperView.sid)) return closeHelperView();
    // …and in the reader.
    if (e.key === "Escape" && !sheet && !lightbox && readerShown()) return closeReader();
    if (e.key === "Escape") {
      // In History, Esc just closes the open row.
      if (view === "history") { histOpen = null; return renderMain(); }
      // Esc twice on a working session stops it (once only arms it, so a stray Esc can't).
      const s = !sheet && !menuFor && !redirectFor ? sessionOf(active?.sid) : null;
      // A message Cue kept for it (sent while it worked): Esc takes it back into the box first.
      if (s?.held) { escArmed = 0; return pullHeld(s.session_id); }
      // Stopping it is a pause to say something else, so the cursor stays in (or goes to) its box.
      if (s?.state === "working") {
        if (Date.now() - escArmed < 1500) { escArmed = 0; interrupt(s.session_id); }
        else { escArmed = Date.now(); toast(`Esc again to stop ${s.project}`); }
        renderMain();
        return focusComposer();
      }
      // Full screen: an Esc nothing else wanted leaves it (a working session's Esc arms Stop above instead).
      if (focus && !sheet && !menuFor && !redirectFor) { leaveFocus(); return renderMain(); }
      sheet = null; menuFor = redirectFor = null; document.activeElement?.blur(); return renderMain();
    }
    const typing = e.target.matches?.("input, textarea, select");
    if (typing) {
      if (e.target.dataset.text === "find-live" && (e.key === "ArrowDown" || e.key === "ArrowUp")) { e.preventDefault(); return stepLive(e.key === "ArrowDown" ? 1 : -1); }
      if (e.target.dataset.text === "find-live" && e.key === "Enter") { e.preventDefault(); return pickLive(); }
      if (e.key === "Enter" && !e.shiftKey && !e.isComposing && e.keyCode !== 229 && e.target.dataset.text) {
        e.preventDefault();
        const key = e.target.dataset.text;
        if (key.startsWith("btw:")) return askBtw(key.slice(4), draft(key).text);
        if (key === "fwd") return sendForward();
        if (key === "new-name" || key === "new-msg") return lvAct("start", {});
        if (key.startsWith("rename:")) return submitRename(key.slice(7));
        if (key.startsWith("pjname:")) return saveProjName(key.slice(7));
        if (key.startsWith("newproj:")) return submitNewProj(key.slice(8));
        // A parked session's box: Enter resumes it with the message (Resume & send).
        if (key.startsWith("s:") && !sessionOf(key.slice(2)) && (parkedOf(key.slice(2)) || closedOf(key.slice(2)))) return draft(key).text.trim() ? wakeParked(key.slice(2), draft(key).text.trim()) : undefined;   // as its button: nothing typed, nothing sent
        if (key.startsWith("s:")) {
          const sid = key.slice(2);
          const turn = state.items.find((i) => i.session_id === sid && i.kind === "waiting");
          // ⌘↵ with nothing new in the box: the message already waiting (queued, or kept in Cue) goes now.
          const fresh = draft(key).text.trim() || draft(key).images.length;
          if ((e.ctrlKey || e.metaKey) && !fresh && sendWaitingNow(sid)) return;
          return turn ? reply(turn) : sendTo(sid, e.ctrlKey || e.metaKey);
        }
        const it = findItem(key);
        if (it) submitText(it);
      }
      return;
    }
    // ⌘↵ outside the box: the open session's waiting message (queued, or kept in Cue) goes now.
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      const c = current(), sid = c?.s?.session_id || c?.it?.session_id;
      if (sid && sendWaitingNow(sid)) return e.preventDefault();
    }
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.key === "/") { e.preventDefault(); return openFind(); }   // like GitHub and Gmail (only when not typing)
    if (e.key === "h") { view = view === "history" ? "board" : "history"; return renderMain(); }
    if (e.key === "l") return lvAct("toggle", {});
    if (e.key === ",") { sheet = sheet === "settings" ? null : "settings"; return renderMain(); }
    if (sheet) return;
    if (e.key === "n") return goNext();
    // In the reader, ← → step through the session's replies.
    // …and Space reads it out, or pauses.
    if (readerShown() && e.key === " ") { e.preventDefault(); return listening ? listenToggle() : document.querySelector("[data-listen-reader]")?.click(); }
    if (readerShown() && (e.key === "ArrowLeft" || e.key === "ArrowRight")) { e.preventDefault(); return stepReader(e.key === "ArrowRight" ? 1 : -1); }
    // ← → step through the chip you clicked (Waiting if you haven't).
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") { e.preventDefault(); return openFromChip(chipNav || "waiting", e.key === "ArrowRight" ? 1 : -1); }
    const it = current()?.it;
    if (!isPending(it)) {
      // Nothing pending on screen: R still jumps to the message box.
      if (e.key === "r") { e.preventDefault(); document.querySelector(".active-pane textarea[data-text]")?.focus(); }
      return;
    }
    if (e.key === "a" && it.kind === "permission") allow(it);
    else if (e.key === "d" && it.kind === "permission") deny(it);
    else if (e.key === "g") goTo(it);
    else if (e.key === "r") { e.preventDefault(); if (it.kind !== "waiting" && !inTerminal(it)) redirectFor = it.id; renderMain(); document.querySelector(`[data-text="${CSS.escape(it.id)}"]`)?.focus(); }
    else if (/^[1-9]$/.test(e.key) && it.kind === "question") pick(it, stepOf(it), +e.key - 1);
  });
}
let setupShown = false;
function setState(s) {
  // Anything that just left Waiting lingers there as "✓ answered" for a moment instead of vanishing.
  const ids = new Set(s.items.map((i) => i.id));
  // Its card's height now, so the ✓ row can ease down from it instead of snapping shorter.
  // Answered here: its time runs from your click (though it shows as ✓ for a moment at least).
  for (const it of state.items) if (!ids.has(it.id)) {
    const clicked = answering.get(it.id);
    answering.delete(it.id);
    ghosts.set(it.id, { it, at: clicked ? Math.max(clicked, Date.now() - LINGER_MS + 400) : Date.now(), soft: !!clicked, h: document.querySelector(`.nrow[data-big="${CSS.escape(it.id)}"]`)?.offsetHeight });
  }
  for (const [id, g] of ghosts) if (Date.now() - g.at > LINGER_MS || ids.has(id)) ghosts.delete(id);
  for (const g of ghosts.values()) setTimeout(leaveGhosts, Math.max(0, g.at + LINGER_MS - Date.now()) + 50);
  actTurnEnds(s);   // actions put off until a turn ends: that session's ended, they run
  state = s;
  for (const [sid, c] of Object.entries(s.insight || {})) insCache.set(sid, c);   // the board's numbers, as of this change
  // A rename Cue's update doesn't have yet keeps showing (no flash of the old name).
  for (const [sid, r] of renamedNow) {
    const x = sessionOf(sid);
    if (!x || x.name === r.name || Date.now() - r.at > 5000) renamedNow.delete(sid);
    else { x.name = r.name; delete x.rename_error; }
  }
  // An agent that didn't take a rename: said once, as it happens (not for old ones at launch).
  for (const x of s.sessions) {
    const k = `${x.session_id}|${x.rename_error || ""}`;
    if (!x.rename_error) { for (const seen of renameErrSeen) if (seen.startsWith(`${x.session_id}|`)) renameErrSeen.delete(seen); continue; }
    if (!renameErrSeen.has(k) && renameErrPrimed) toast(`${nameOf(x.session_id, x.project)}: ${x.rename_error}`);
    renameErrSeen.add(k);
  }
  renameErrPrimed = true;
  for (const [sid, on] of starNow) if (!!sessionOf(sid)?.starred_ms === on || !sessionOf(sid)) starNow.delete(sid);   // Cue says so now
  // First launch: an installed agent isn't connected yet, and you haven't closed the setup screen.
  if (!setupShown && s.settings && !s.settings.setup?.done && ["claude", "codex", "pi"].some((h) => s.connections?.[h]?.present && !s.connections[h].ok)) { setupShown = true; sheet = "setup"; }
  for (let n = outbox.length - 1; n >= 0; n--) if (outbox[n].via && landed(outbox[n])) outbox.splice(n, 1);
  if (s.now_ms) clockSkew = s.now_ms - Date.now();
  applyTheme();
  render();
  extNotify();
}
// A draw that throws must not leave the window empty: what went wrong is shown where the board
// would be, so a bad build is seen as an error and not as "Cue didn't start".
function showRenderError(e) {
  console.error("[render]", e);
  const app = document.getElementById("app");
  if (!app || app.innerHTML.length > 0 && !app.querySelector(".render-error")) return; // an earlier good draw stays
  const detail = e && e.stack ? e.stack : String(e);
  app.innerHTML = `<div class="render-error" style="padding:24px 28px;font:13px/1.5 ui-monospace,Menlo,monospace;white-space:pre-wrap;user-select:text"><div style="font:600 15px system-ui;margin-bottom:10px">Cue can't draw its window</div>${esc(detail)}\n\nIf this came with a new build, reinstall the previous one or fix the error above and run ./install.sh --app again.</div>`;
}
const render = () => { try { renderMain(); } catch (e) { showRenderError(e); } };
window.addEventListener("error", (e) => showRenderError(e.error || e.message));
window.addEventListener("unhandledrejection", (e) => showRenderError(e.reason));

async function boot() {
  await loadDrafts();
  bindMain();
  setState(await invoke("get_state"));
  loadExtensions();
  loadMachines();   // their names (and what you renamed them to) for every label, before + New is opened
  restoreSpot();
  await T.event.listen("state", (e) => setState(e.payload));
  await T.event.listen("dictation", (e) => onDictation(e.payload));
  await T.event.listen("speak", (e) => kokoroLine(e.payload));
  await T.event.listen("voice-status", () => extReload());
  // A natural voice picked: which ones are here, now, so Listen reads in it from the start (asked only by
  // Settings → Voice before, so after a restart Listen used the Mac's voice until Settings was opened).
  if (voicePick()) kokoroStatus();
  await T.event.listen("fill", (e) => onFill(e.payload));
  await T.event.listen("upload", (e) => onUpload(e.payload));
  await T.event.listen("select", (e) => {
    // "s:<session>": that session's chat, whether or not a card of it waits.
    if (e.payload.startsWith("s:")) return setActive(null, e.payload.slice(2));
    if (e.payload !== "usage") return setActive(e.payload);
    const s = limitedSessions().sort((a, b) => a.since_ms - b.since_ms)[0];
    if (s) setActive(null, s.session_id);
  });
  await T.event.listen("term", (e) => { onTermOutput(e.payload); onShellOutput(e.payload); });
  await T.event.listen("shell", (e) => onShellEvent(e.payload));
  restoreShells();   // the shells still running in tmux, back in their tabs
  // ⌘W (the menu's Close): inside Cue's terminal, the tab you're in; elsewhere the window hides.
  await T.event.listen("close-key", () => closeKey());
  await T.event.listen("server-error", (e) => toast(`Cue can't listen: ${e.payload}`));
  moveOldParked();
  moveOldStars();
  // Which Cue this is (for Settings), without asking GitHub.
  T.app?.getVersion().then((v) => { upd = { ...upd, current: v }; }, () => {});
  await T.event.listen("update-ready", (e) => { upd = { ...upd, version: e.payload.version, notes: e.payload.notes || "", status: "found", offer: true }; renderMain(); });
  setInterval(render, 15000); // keep ages and bars moving
  // The open session's steps: every 0.5 s while it works, else every 2 s (only new lines are read;
  // nothing new answers with just a version).
  setInterval(() => {
    const cur = current();
    const q = quietOpen && (state.live || []).find((x) => x.session_id === quietOpen);
    const sid = q ? q.session_id : cur?.s?.session_id || cur?.it?.session_id;
    if (!sid || document.hidden) return;
    const f = stepFeeds.get(sid);
    const working = q ? q.status === "busy" : cur?.s?.state === "working";
    // Not "working" can still be working: a stop hook or a finished background task sends it back to
    // work without a new prompt. So it's checked every 2 s anyway (nothing new costs next to nothing).
    if (!f || working || Date.now() - f.at > 2000) loadSteps(sid);
  }, 500);
  // The live "what it's doing · 3s" counts every second, without redrawing everything.
  setInterval(() => {
    document.querySelectorAll("[data-ago]").forEach((el) => { el.textContent = ago(+el.dataset.ago); });
    // The going-cold chip counts down; once its soonest goes cold, the chip redraws without it.
    let coldDone = false;
    document.querySelectorAll("[data-cold-in]").forEach((el) => { if (+el.dataset.coldIn <= now()) coldDone = true; else el.textContent = coldIn(+el.dataset.coldIn); });
    if (coldDone) renderMain();
    document.querySelectorAll("[data-helper-span]").forEach((el) => { el.textContent = helperSpan({ started_ms: +el.dataset.helperSpan }); });
  }, 1000);
}
boot().catch(showRenderError);
