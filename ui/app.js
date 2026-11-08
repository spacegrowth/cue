// Cue UI — everything that needs you, visible at once, on one board.
// Cue UI app — the window's state as Cue sends it (setState), a draw that fails shown as an error, and boot(). The draw
// is ui/app/render.js; its listeners ui/app/bind.js.
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
  for (const [id, g] of ghosts) if (Date.now() - g.at > LINGER_MS || ids.has(id)) ghosts.delete(id);
  watchGhosts();
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
    // "d:{sid, path}": a markdown file a reply named, in that session's reader (a link in the mini window).
    if (e.payload.startsWith("d:")) { const d = JSON.parse(e.payload.slice(2)); return openDoc(d.sid, d.path); }
    if (e.payload !== "usage") return setActive(e.payload);
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
