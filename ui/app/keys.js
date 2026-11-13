// Cue UI app — the window's keys. Each step below takes a keydown (true: done with it) or leaves it to the next,
// in order: Cue's ⌘ shortcuts first, then keys that belong to whatever has the cursor, then Esc, then the bare
// letters of the board.

const cmdOnly = (e) => e.metaKey && !e.shiftKey && !e.altKey && !e.ctrlKey;

/** ⌘ shortcuts that work from anywhere, a terminal included; Enter in + New's machine fields. */
function keyAnywhere(e) {
  const k = e.key?.toLowerCase();
  // Enter in + New's Add machine field adds it.
  if (e.key === "Enter" && e.target.dataset?.text === "mc-host") { e.preventDefault(); machAct("add"); return true; }
  if (e.key === "Enter" && e.target.dataset?.text === "mc-label" && machRenaming) { e.preventDefault(); machAct("rename-save", machRenaming); return true; }
  // ⌘J: Cue's own floating terminal, out or tucked away (from inside it too).
  if (e.metaKey && !e.shiftKey && !e.altKey && k === "j") { e.preventDefault(); tipUsed("shell"); toggleShells(true); return true; }
  // ⌘D: tell me when the open session's turn is done. Works while you type.
  if (cmdOnly(e) && k === "d") { e.preventDefault(); tipUsed("tell"); watchTurn(); return true; }
  // ⌘+ / ⌘− / ⌘0: text size, a step bigger or smaller, or back to Default (from anywhere in Cue).
  if (e.metaKey && !e.altKey && !e.ctrlKey && ["=", "+", "-", "_", "0"].includes(e.key)) { e.preventDefault(); stepZoom(e.key === "0" ? 0 : e.key === "-" || e.key === "_" ? -1 : 1); return true; }
  // ⌘T: a new shell tab in it (from inside it too).
  if (cmdOnly(e) && k === "t") { e.preventDefault(); newShellTab(); return true; }
  return false;
}

/** Inside Cue's own terminal, as in a terminal app: ⌘K clears it (not Cue's ⌘K), ⌘1–9 go to its tabs (⌘9 the
 *  last), and every other key but a ⌘ shortcut is its shell's. A session's terminal takes every key but those too. */
function keyInTerminal(e) {
  const inPip = !!e.target.closest?.(".pip");
  if (inPip && cmdOnly(e)) {
    if (e.key.toLowerCase() === "k") { e.preventDefault(); clearShell(); return true; }
    if (/^[1-9]$/.test(e.key)) { e.preventDefault(); shellNumber(+e.key); return true; }
  }
  if (inPip && !e.metaKey) return true;
  return !!e.target.closest?.(".term-host") && !e.metaKey;
}

/** In a box: its "/" menu (arrows move, Tab fills, Enter runs, Esc closes it), and ↑ / ↓ in a session's box back
 *  and forth through the last messages you sent it (chat/compose.js recallSent). */
function keyInBox(e) {
  const mk = e.target.dataset?.text;
  const m = mk && !e.isComposing && ["ArrowUp", "ArrowDown", "Tab", "Enter", "Escape"].includes(e.key) ? cmdMatches(mk) : [];
  if (m.length && !e.shiftKey) {
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") { cmdShut = mk; refreshCmd(mk); }
    else if (e.key === "ArrowUp" || e.key === "ArrowDown") { cmdSel = (cmdSel + (e.key === "ArrowDown" ? 1 : m.length - 1)) % m.length; refreshCmd(mk); }
    else pickCmd(mk, m[cmdSel].name, e.key === "Enter");
    return true;
  }
  if (mk?.startsWith("s:") && e.key === "Tab" && !e.shiftKey && !e.altKey && !e.metaKey && !e.ctrlKey && !e.isComposing && takeSuggestion(mk)) { e.preventDefault(); return true; }
  if (mk?.startsWith("s:") && (e.key === "ArrowUp" || e.key === "ArrowDown") && !e.shiftKey && !e.altKey && !e.metaKey && !e.ctrlKey && !e.isComposing && recallSent(e)) { e.preventDefault(); return true; }
  return false;
}

/** + New's folder field: arrows move through the folders, Enter picks one (the highlighted one, else the list's
 *  first; it never starts the session: nothing to pick, on to the Message box), Tab completes one as a shell does
 *  (on into its own folders), Esc closes the list. */
function keyInFolder(e) {
  if (e.target.dataset?.text !== "new-cwd") return false;
  if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); stepFolder(e.key === "ArrowDown" ? 1 : -1); return true; }
  if (e.key === "Enter" && !e.isComposing) {
    e.preventDefault();
    const m = nfFolderOpen ? folderMatches(nfFolders()) : [], f = m[nfFolderSel] || m[0];
    if (f) { pickFolder(f); return true; }
    closeFolders(); renderMain();
    document.querySelector('[data-text="new-msg"]')?.focus();
    return true;
  }
  if (nfFolderOpen && e.key === "Escape") { e.preventDefault(); closeFolders(); renderMain(); return true; }
  if (e.key === "Tab" && !e.shiftKey && nfFolderOpen && completeFolder()) { e.preventDefault(); return true; }
  if (nfFolderOpen && e.key === "Tab") { closeFolders(); renderMain(); }
  return false;
}

/** Esc closes what's open over the board first: a rename, a project's box, the Projects list, the live list. */
function keyCloseOpen(e) {
  if (e.key !== "Escape") return false;
  if (renaming) renaming = null;
  else if (pjRenaming) pjRenaming = null;
  else if (newProjFor) { newProjFor = null; newProjNote = ""; }
  else if (pjNewOpen) { pjNewOpen = false; pjNewNote = ""; }
  else if (pjOpen || colorFor) { pjOpen = false; colorFor = null; }
  else if (liveOpen) liveOpen = newOpen = false;
  else return false;
  renderMain();
  return true;
}

/** Cue's ⌘ shortcuts that work even while you type in a box. */
function keyCommands(e) {
  const k = e.key?.toLowerCase();
  if (e.metaKey && !e.ctrlKey && k === "f") { e.preventDefault(); tipUsed("find"); openFind(); return true; }
  // ⌘, opens Settings, the Mac way.
  if (e.metaKey && e.key === ",") { e.preventDefault(); sheet = sheet === "settings" ? null : "settings"; renderMain(); return true; }
  // ⌘K: every live session in the middle of the window, like Spotlight.
  if (e.metaKey && k === "k") { e.preventDefault(); tipUsed("find"); openFind(); return true; }
  // ⌘L: every live session (the drop-down by "N live").
  if (e.metaKey && k === "l") { e.preventDefault(); lvAct("toggle", {}); return true; }
  // ⌘N: + New session, in the middle of the window like ⌘K.
  if (e.metaKey && !e.shiftKey && !e.altKey && k === "n") { e.preventDefault(); tipUsed("new"); sheet = lightbox = null; liveSpot = true; lvAct("new", {}); return true; }
  // ⌘E: the open project's actions, to run one (actions.js); ⌘⇧E runs the last one again.
  if (e.metaKey && !e.altKey && !e.ctrlKey && k === "e") { e.preventDefault(); if (e.shiftKey) actAgain(); else actPalToggle(); return true; }
  return false;
}

/** A picture shown full size takes every key: ← → step through its set, Esc closes it. */
function keyLightbox(e) {
  if (!lightbox) return false;
  if (e.key === "ArrowRight") lightbox.i = (lightbox.i + 1) % lightbox.srcs.length;
  else if (e.key === "ArrowLeft") lightbox.i = (lightbox.i - 1 + lightbox.srcs.length) % lightbox.srcs.length;
  else if (e.key === "Escape") lightbox = null;
  e.preventDefault();
  renderMain();
  return true;
}

/** Esc twice on a working session stops it (once only arms it, so a stray Esc can't); a message Cue kept for it
 *  (sent while it worked) comes back into the box first. Stopping it is a pause to say something else, so the
 *  cursor stays in (or goes to) its box. False when the open session has neither. */
function escSession() {
  const s = !sheet && !menuFor && !redirectFor ? sessionOf(active?.sid) : null;
  if (s?.held) { escArmed = 0; pullHeld(s.session_id); return true; }
  if (s?.state !== "working") return false;
  if (Date.now() - escArmed < 1500) { escArmed = 0; interrupt(s.session_id); }
  else { escArmed = Date.now(); toast(`Esc again to stop ${s.project}`); }
  renderMain();
  focusComposer();
  return true;
}
/** Esc: the side question's panel, a background agent's view, the reader, History's open row, the open session
 *  (escSession), full screen; else whatever sheet or menu is open, and the cursor leaves its box. */
function keyEscape(e) {
  if (e.key !== "Escape") return false;
  if (btwFor) { btwFor = null; renderMain(); return true; }
  if (!sheet && !lightbox && helperView && helperViewed(helperView.sid)) { closeHelperView(); return true; }
  if (!sheet && !lightbox && readerShown()) { closeReader(); return true; }
  if (view === "history") { histOpen = null; renderMain(); return true; }
  if (escSession()) return true;
  // Full screen: an Esc nothing else wanted leaves it (a working session's Esc arms Stop above instead).
  if (focus && !sheet && !menuFor && !redirectFor) { leaveFocus(); renderMain(); return true; }
  sheet = null; menuFor = redirectFor = null; document.activeElement?.blur(); renderMain();
  return true;
}

/** Enter in a session's box: an answer to its waiting turn, else a message to it; ⌘↵ with nothing new in the box
 *  sends the message already waiting (queued, or kept in Cue) now. A parked or closed session's box resumes it
 *  with the message (Resume & send; nothing typed, nothing sent, as its button). */
function enterInSessionBox(e, key) {
  const sid = key.slice(2);
  if (!sessionOf(sid) && (parkedOf(sid) || closedOf(sid))) { if (draft(key).text.trim()) wakeParked(sid, draft(key).text.trim()); return; }
  const turn = state.items.find((i) => i.session_id === sid && i.kind === "waiting");
  const fresh = draft(key).text.trim() || draft(key).images.length;
  if ((e.ctrlKey || e.metaKey) && !fresh && sendWaitingNow(sid)) return;
  if (turn) reply(turn); else sendTo(sid, e.ctrlKey || e.metaKey);
}
/** Enter in a box: what that box is for. */
function enterInBox(e, key) {
  if (key.startsWith("btw:")) return askBtw(key.slice(4), draft(key).text);
  if (key === "fwd") return sendForward();
  if (key === "new-name" || key === "new-msg") return lvAct("start", {});
  if (key.startsWith("rename:")) return submitRename(key.slice(7));
  if (key.startsWith("pjname:")) return saveProjName(key.slice(7));
  if (key.startsWith("newproj:")) return submitNewProj(key.slice(8));
  if (key === "pjnew") return submitPjNew();
  if (key.startsWith("s:")) return enterInSessionBox(e, key);
  const it = findItem(key);
  if (it) submitText(it);
}
/** While you type in a field, the rest of the keys are the field's, but for the live search's arrows and Enter,
 *  and Enter (without ⇧) in a box, which sends. */
function keyTyping(e) {
  if (!e.target.matches?.("input, textarea, select")) return false;
  const key = e.target.dataset.text;
  if (key === "find-live" && (e.key === "ArrowDown" || e.key === "ArrowUp")) { e.preventDefault(); stepLive(e.key === "ArrowDown" ? 1 : -1); }
  else if (key === "find-live" && e.key === "Enter") { e.preventDefault(); pickLive(); }
  else if (e.key === "Enter" && !e.shiftKey && !e.isComposing && e.keyCode !== 229 && key) { e.preventDefault(); enterInBox(e, key); }
  return true;
}

/** The open card's own keys, while it's pending: A allow, D deny, G go to it, R reply another way, 1–9 a choice.
 *  Nothing pending on screen: R still jumps to the message box. */
function keyOnCard(e) {
  // The card under the chat (a crew member's open ask yields to the lead's own, chat/pane.js shownAsk).
  const it = shownAsk(current()?.it);
  if (!isPending(it)) {
    if (e.key === "r") { e.preventDefault(); document.querySelector(".active-pane textarea[data-text]")?.focus(); }
    return;
  }
  if (e.key === "a" && it.kind === "permission") allow(it);
  else if (e.key === "d" && it.kind === "permission") deny(it);
  else if (e.key === "g") goTo(it);
  else if (e.key === "r") { e.preventDefault(); if (it.kind !== "waiting" && !inTerminal(it)) redirectFor = it.id; renderMain(); document.querySelector(`[data-text="${CSS.escape(it.id)}"]`)?.focus(); }
  else if (/^[1-9]$/.test(e.key) && it.kind === "question") pick(it, stepOf(it), +e.key - 1);
}
/** Keys with nothing typed into: ⌘↵ sends the open session's waiting message, and the bare letters. */
function keyBoard(e) {
  // ⌘↵ outside the box: the open session's waiting message (queued, or kept in Cue) goes now.
  if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
    const c = current(), sid = c?.s?.session_id || c?.it?.session_id;
    if (sid && sendWaitingNow(sid)) return e.preventDefault();
  }
  if (e.metaKey || e.ctrlKey || e.altKey) return;
  if (e.key === "/") { e.preventDefault(); tipUsed("find"); return openFind(); }   // like GitHub and Gmail (only when not typing)
  if (e.key === "h") { view = view === "history" ? "board" : "history"; return renderMain(); }
  // The list focuses its search box: the key that opened it mustn't land there as typed text.
  if (e.key === "l") { e.preventDefault(); return lvAct("toggle", {}); }
  if (e.key === ",") { sheet = sheet === "settings" ? null : "settings"; return renderMain(); }
  if (sheet) return;
  if (e.key === "n") return goNext();
  // In the reader, Space reads it out, or pauses; ← → step through the session's replies.
  if (view === "read" && readDoc && e.key === " ") { e.preventDefault(); return listening ? listenToggle() : document.querySelector(".rdv-head [data-listen-reader]")?.click(); }
  if (readerShown() && e.key === " ") { e.preventDefault(); return listening ? listenToggle() : document.querySelector("[data-listen-reader]")?.click(); }
  if (readerShown() && (e.key === "ArrowLeft" || e.key === "ArrowRight")) { e.preventDefault(); return stepReader(e.key === "ArrowRight" ? 1 : -1); }
  // ← → step through the chip you clicked (Waiting if you haven't).
  if (e.key === "ArrowLeft" || e.key === "ArrowRight") { e.preventDefault(); return openFromChip(chipNav || "waiting", e.key === "ArrowRight" ? 1 : -1); }
  keyOnCard(e);
}

const KEY_STEPS = [keyAnywhere, keyInTerminal, keyInBox, keyInFolder, keyCloseOpen, keyCommands, keyLightbox, keyEscape, keyTyping];
/** A key pressed in the window: the first step that takes it, else the board's keys. */
function onKey(e) {
  if (!KEY_STEPS.some((step) => step(e))) keyBoard(e);
}
/** While Cue asks whether to delete a session: Esc is no, Enter yes, before a terminal under it takes the key. While
 *  it asks whether to quit when that ends sessions: Esc keeps it open (quitting takes a click). Esc says no to trusting a folder. */
function keyDeleteAsk(e) {
  if (quitAsk && e.key === "Escape") {
    e.preventDefault();
    e.stopPropagation();
    return quitAnswer(false);
  }
  if (trustAsk && e.key === "Escape") {
    e.preventDefault();
    e.stopPropagation();
    return trustAnswer(false);
  }
  if (!deleteAsk || (e.key !== "Escape" && e.key !== "Enter")) return;
  e.preventDefault();
  e.stopPropagation();
  deleteAnswer(e.key === "Enter");
}
