// Cue UI — everything that needs you, visible at once, on one board.
// Cue UI app — the window's state as Cue sends it (setState), and boot(). The draw (and a draw that fails, shown as an
// error) is ui/app/render.js; its listeners ui/app/bind.js.
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
  // Your own words on Cue's Yes / No card went to the lead, and the card stays open (#651): answerable again.
  for (const it of s.items) {
    const said = it.status === "pending" && answering.has(it.id) && it.tool_input?.desk_said;
    if (Array.isArray(said) && said.length && said[said.length - 1].at >= answering.get(it.id) - 5000) answering.delete(it.id);
  }
  for (const [id, g] of ghosts) if (Date.now() - g.at > LINGER_MS || ids.has(id)) ghosts.delete(id);
  watchGhosts();
  if (typeof deskGoneApply === "function") deskGoneApply(s);   // a Desk deleted here stays gone (desk/home.js)
  if (typeof landsHeldApply === "function") landsHeldApply(s);   // a Desk's settings just saved stay shown (lands.js)
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
  dropLanded();
  if (s.now_ms) clockSkew = s.now_ms - Date.now();
  applyTheme();
  // An add-on's error mustn't keep the window from drawing the new state.
  try { addOn.stateChanged(); } catch (e) { console.error(e); }
  render();
  extNotify();
}
window.addEventListener("error", (e) => showRenderError(e.error || e.message));
window.addEventListener("unhandledrejection", (e) => showRenderError(e.reason));

/** What the app itself says (the menu, its server, quitting, updates). */
async function listenApp() {
  // ⌘W (the menu's Close): inside Cue's terminal, the tab you're in; elsewhere the window hides.
  await T.event.listen("close-key", () => closeKey());
  await T.event.listen("server-error", (e) => toast(`Cue can't listen: ${e.payload}`));
  // Quitting (⌘Q, the Dock) when it would end sessions Cue runs itself: it asks (startup/window.rs).
  await T.event.listen("quit-ask", (e) => { quitAsk = { why: "quit", list: e.payload || [] }; renderMain(); });
  // Which Cue this is (for Settings), without asking GitHub.
  T.app?.getVersion().then((v) => { upd = { ...upd, current: v }; }, () => {});
  await T.event.listen("update-ready", (e) => { upd = { ...upd, version: e.payload.version, notes: e.payload.notes || "", status: "found", offer: true }; renderMain(); });
}
async function boot() {
  await loadDrafts();
  bindMain();
  setState(await fetchState());
  loadExtensions();
  addOn.boot();
  loadMachines();   // their names (and what you renamed them to) for every label, before + New is opened
  restoreSpot();
  // Cue only says the state changed; it's fetched here once the last is drawn, then Cue's told it's drawn. States
  // sent whole, faster than they're drawn, waited in line: your clicks behind them (#587), and Cue holding each
  // one until it was read, growing all day (#794; hub/redraw.rs).
  let pulling = false, again = false;
  const pull = async () => {
    if (pulling) { again = true; return; }
    pulling = true;
    try {
      do { again = false; try { setState(await fetchState()); } catch (e) { console.error(e); } } while (again);
    } finally { pulling = false; invoke("state_drawn").catch(() => {}); }
  };
  await T.event.listen("state-changed", pull);
  pull();   // a change between the first fetch and listening
  await T.event.listen("dictation", (e) => onDictation(e.payload));
  await T.event.listen("speak", (e) => kokoroLine(e.payload));
  await T.event.listen("voice-status", () => extReload());
  // A natural voice picked: which ones are here, now, so Listen reads in it from the start (asked only by
  // Settings → Voice before, so after a restart Listen used the Mac's voice until Settings was opened).
  if (voicePick()) kokoroStatus();
  await T.event.listen("fill", (e) => onFill(e.payload));
  await T.event.listen("upload", (e) => onUpload(e.payload));
  await listenPageLook(T);
  await T.event.listen("select", (e) => {
    // "s:<session>": that session where it lives (#492): on its Desk when it's on one, else its chat on the Board,
    // whether or not a card of it waits.
    if (e.payload.startsWith("s:")) return openWhereItLives(null, e.payload.slice(2));
    // "d:{sid, path}": a file a reply named, as a click on it opens it here (a link in the mini window): a markdown
    // file in that session's Read, any other in the pop-up viewer.
    if (e.payload.startsWith("d:")) { const d = JSON.parse(e.payload.slice(2)); return isDocPath(d.path) ? openDoc(d.sid || null, d.path) : fileView(d.path, d.sid || null); }
    // "k:{desk, item}": a Desk, on its item when one is named (the mini window's Desk tab).
    if (e.payload.startsWith("k:")) { const k = JSON.parse(e.payload.slice(2)); return openInDesk(k.desk, k.item || ""); }
    // "c:{id, sid}": a notification clicked, with the session it's about (its card may be gone by now, #388).
    if (e.payload.startsWith("c:")) { const c = JSON.parse(e.payload.slice(2)); return openNotice(c.id, c.sid); }
    if (e.payload !== "usage") return openNotice(e.payload);
    const s = limitedSessions().sort((a, b) => a.since_ms - b.since_ms)[0];
    if (s) setActive(null, s.session_id);
  });
  await T.event.listen("term", (e) => { onTermOutput(e.payload); onShellOutput(e.payload); });
  await T.event.listen("shell", (e) => onShellEvent(e.payload));
  restoreShells();   // the shells still running in tmux, back in their tabs
  await listenApp();
  moveOldParked();
  moveOldStars();
  setInterval(render, 15000); // keep ages and bars moving
  setInterval(followSteps, 500);
  setInterval(tickAges, 1000);
}
boot().catch(showRenderError);
