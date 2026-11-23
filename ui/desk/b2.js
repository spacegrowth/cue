// Cue UI desk — parts first made for B2 of #652 (#666) that the Desk's layout (desk/c.js) still uses: the strip over
// the lead's chat when its panel is hidden (Now and Waits on you; the Working pill unfolds the sessions at work in
// place, a line each), where an item stands in a few words (deskB2Status), Done today's list, the folded groups, and
// Desk settings. styles/desk-b2.css. Loaded before desk/c.js (index.html, mini.html).

/** The Working pill in the strip is unfolded. */
let deskB2Unfold = false;
const DESK_B2_CARET = `<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m6 9 6 6 6-6"/></svg>`;

/** Desk `d`'s open items, as the lead's line and the All tab count them. */
const deskB2Open = (d) => (d?.items || []).filter((i) => i.state !== "done" && i.state !== "dropped").length;
/** How many wait on you on Desk `d`, the same in the strip's words and on the Waits on you tab: what that tab lists (its
 *  questions, what's ready to check, the steps and asks held while away) and the lead's own ask. */
function deskB2WaitN(d, taskApprovals) {
  const g = deskGroups(d.project, taskApprovals), ask = deskLeadEnded(d) ? null : deskLeadAsk(d);
  return g.asks.length + g.check.length + deskAwayWaiting(d, taskApprovals).n + (ask ? 1 : 0);
}
/** Desk `d`'s finished items done today: the Done today tab's rows and its count. */
const deskB2DoneToday = (d) => deskDoneList(d).filter(deskDoneToday);
/** The way back to the list: "Items 40". */
const deskB2ItemsWord = (d) => `Items ${deskB2Open(d)}`;
/** "a", "a and b", "a, b and c". */
const deskB2Names = (ns) => (ns.length < 2 ? ns.join("") : `${ns.slice(0, -1).join(", ")} and ${ns.at(-1)}`);

// ---------- the strip over the lead's chat ----------

/** Now or Waits on you, one of them (#666, so the strip never runs out of room): while anything waits on you, how many,
 *  a chip per item (the first 5, +N › for the rest) and Check on the first; with nothing waiting, the Working pill,
 *  unfolding a line per session at work and who else waits on you or has work queued. */
function deskB2Strip(d, waits, working, ask, taskApprovals) {
  const n = deskB2WaitN(d, taskApprovals);
  if (n || ask || waits.length) {
    const shown = waits.slice(0, ask ? 4 : 5);
    const words = `<span class="dk2b-wn">${n} wait${n === 1 ? "s" : ""} on you</span>`;
    const lead = ask ? `<button class="dk2b-chip ask" data-desk-tab="lead">${esc(deskLeadName(d))} asks</button>` : "";
    const chips = shown.map((i) => `<button class="dk2b-chip" data-desk-chk-item="${esc(i.id)}">#${esc(i.id)}</button>`).join("");
    const more = waits.length > shown.length ? `<button class="dk2b-more" data-desk2b-tab="you">+${waits.length - shown.length} ›</button>` : "";
    const check = waits.length ? `<button class="btn small primary dk2b-check" data-desk-chk-item="${esc(waits[0].id)}">Check #${esc(waits[0].id)}</button>` : "";
    return `<section class="dk2b-strip"><div class="dk2b-sr">${words}<span class="dk2b-chips">${lead}${chips}</span>${more}${check}</div></section>`;
  }
  const pill = `<button class="dk2b-work${deskB2Unfold ? " on" : ""}" data-desk2b-work aria-expanded="${deskB2Unfold}"><i class="dk2b-wdot" aria-hidden="true"></i>Working ${working.length}${DESK_B2_CARET}</button>`;
  return `<section class="dk2b-strip${deskB2Unfold ? " open" : ""}"><div class="dk2b-sr">${pill}</div>${deskB2Unfold ? deskB2Working(d, working) : ""}</section>`;
}
/** The unfolded Working list: a line per session at work (its name, its item, what it's running and since when, the
 *  item's steps); a click opens its item on the right (its chat when it holds none). Then a line naming the sessions
 *  that wait on you or have work queued, and All N sessions. */
function deskB2Working(d, working) {
  const o = deskOpen;
  const rows = working.map(({ sid, name, on }) => {
    const s = sessionOf(sid);
    const doing = s?.compacting_ms ? "Compacting…" : s?.running ? `Running: ${s.running}` : s?.doing || "Thinking…";
    const at = s?.compacting_ms || s?.doing_ms || s?.since_ms || 0;
    const lit = on ? o?.kind === "item" && deskIdIs(on, o.id) : o?.kind === "chat" && o.sid === sid;
    const go = on ? `data-desk-chk-item="${esc(on.id)}"` : deskChatAttrs(sid, d);
    const step = on && on.state !== "done" ? Math.max(0, deskProgAt(on)) + 1 : 0;
    return `<button class="dk2b-wrow${lit ? " on" : ""}" ${go} data-desk2b-sid="${esc(sid)}"><i class="dk2b-wdot" aria-hidden="true"></i><b class="dk2b-wnm">${esc(name)}</b><span class="dk2b-wid">${on ? `#${esc(on.id)}` : ""}</span><span class="dk2b-wdo">${esc(doing)}${at ? ` · <span data-ago="${at}">${ago(at)}</span>` : ""}</span>${on ? deskB2Track(on) : "<span></span>"}<span class="dk2b-wst">${step ? `${step}/6` : ""}</span></button>`;
  }).join("");
  // Who else: the sessions whose items wait on you, and those with an item queued for them.
  const waitNames = [...new Set(d.items.filter(deskWaits).map((i) => (i.holder ? deskHolderOf(i) : "")).filter(Boolean))];
  const queued = d.items.filter((i) => i.state === "queued" && (i.holder || i.next_for)).slice(0, 2).map((i) => `${i.holder ? deskHolderOf(i) : deskHolderOf({ holder: i.next_for })} has #${i.id} queued`);
  const say = [waitNames.length ? `${deskB2Names(waitNames.slice(0, 3))}${waitNames.length > 3 ? ` and ${waitNames.length - 3} more` : ""} ${waitNames.length === 1 ? "waits" : "wait"} on you` : "", ...queued].filter(Boolean).join(" · ");
  const all = deskB2SessionCount(d);
  return `<div class="dk2b-wlist">${rows || `<div class="dk2b-wnone">No session is at work right now.</div>`}<div class="dk2b-wfoot"><span class="dk2b-wsay">${esc(say)}</span><span class="grow"></span><button class="dk2b-alls" data-desk2b-sessions>All ${all} session${all === 1 ? "" : "s"} ›</button></div></div>`;
}
/** The sessions All N sessions lists: the lead and the Desk's sessions in its folder. */
const deskB2SessionCount = (d) => (state.sessions || []).filter((s) => s.project === deskWorkspace(d) && s.session_id && deskHasSession(d, s.session_id)).length;

// ---------- the list: tabs, groups, a row per item in four columns ----------

/** The six steps an item shows on today's page, its bar and its page alike. */
const DESK_B2_STEPS = ["Picked up", "Working", "Tests", "Landing", "Ready to check", "Done"];
/** The step item `i` is at on those six (-1: not started; 6: done, all filled). Cue stamps Picked up, Building,
 *  Tests, Commit, Landing, To check (desk/progress.js): Commit and Landing are both Landing here. */
function deskB2At(i) {
  if (i.state === "done") return 6;
  if (i.state === "to_check") return 4;
  const at = [-1, 0, 1, 2, 3, 3, 4][deskProgAt(i) + 1];
  // Finishing is its DONE on the landing line, on main or waiting to install: at Landing, whatever it stamped.
  if (deskFinishing(i)) return Math.max(at, 3);
  return i.state === "working" ? Math.max(at, 1) : at;
}
/** The class for step `k` of an item at step `at`: done, cur (dotted; its dots move while a session works on it right
 *  now, #729: not while it waits its turn, on you, or to be checked), or none. */
const deskB2StepCls = (i, at, k) => (k < at ? "done" : k === at ? (deskProgLook(i) === "work" ? "cur live" : "cur") : "");
/** After a draw (app/render.js replayMotion): every moving step's dots keep time with the page's clock, so a step drawn
 *  afresh carries on where it was, never starting over, and a kept one stays as it runs. */
function deskB2Drift() {
  for (const el of document.querySelectorAll(".dk2b-trk i.cur.live, .dk2b-step.cur.live i, .dkn-kp.cur.live i")) for (const a of el.getAnimations?.() || []) if (a.startTime !== 0) a.startTime = 0;
}
/** An item's six steps as six short bars: done filled, the one it's at dotted, the rest faint. */
function deskB2Track(i) {
  const at = deskB2At(i);
  return `<span class="dk2b-trk" aria-hidden="true">${DESK_B2_STEPS.map((_, k) => `<i class="${deskB2StepCls(i, at, k)}"></i>`).join("")}</span>`;
}
/** The line under a row's title: where it stands in a few words, the old row's words cut short (#666: the row has no
 *  other line, so every state says itself here; answering and Looks right are on its page). [html, whether it's a
 *  session's step (mono)]. */
function deskB2Status(i) {
  const since = (ms) => (ms ? ` · since ${deskQHm(ms)}` : "");
  const cap = (t) => t.charAt(0).toUpperCase() + t.slice(1);
  // In a circle of waits (#250): none of it moves till a link is cut, on its page.
  if (i.state !== "done" && deskCircleOf(i)) return ["Stuck in a circle of waits", false];
  // A big feature nobody holds (#282): how many of its pieces are done, and a thin bar of it; its pieces are on its page.
  if (deskFeat(i) && ["work", "q"].includes(deskFeatState(i)) && i.state !== "needs_you" && i.state !== "to_check") return [deskFeatBar(i), false];
  if (i.state === "to_check") {
    if (deskInstallWait(i)) return ["Waiting for install", false];
    if (deskUncommitted(i)) return ["Not on main yet", false];
    // Changes left on its branch, its mocks' canvas, or in the app since Cue checked it there with what to try: the
    // old row's check line (#436, #448, #526).
    if (i.finish?.unlanded || deskCanvas(i) || i.finish?.mocks || i.finish?.step === "checked") return [deskWhereLine(i).html, false];
    return [`Ready to check${since(deskMs(i.updated))}`, false];
  }
  if (i.state === "needs_you") {
    const r = deskItemRequest(i), [q] = r?.kind === "question" ? questions(r) : [];
    // A tool to allow, by its request or else by its ask's own kind (its request can be another, older one).
    if (r?.kind === "permission" || i.ask?.kind === "permission") {
      const w = deskAllowLine(i), lc = /^[A-Z][a-z]/.test(w) ? w.charAt(0).toLowerCase() + w.slice(1) : w;
      return [esc(w ? `Asks: Allow ${lc}?` : "Asks to use a tool") + since(deskMs(i.updated)), false];
    }
    const ask = q?.question || deskQuestion(i) || deskVerb(i)?.[1];
    return [esc(ask ? `Asks: ${deskOneLine(ask)}` : "Asks you something") + since(deskMs(i.updated)), false];
  }
  // Done, with what carries it on or what it came from (#606): "You picked B2 on #652; #666 builds it".
  if (i.state === "done") { const b = deskBuiltWords(i); return [`Done · ${clockAt(deskMs(i.done_at || i.updated))}${b ? ` · <span class="dk-built">${b}</span>` : ""}`, false]; }
  if (i.state === "held") return ["Parked: starts when you press Start", false];
  if (i.state === "paused") return ["Paused by the Desk lead", false];
  if (i.state === "needs_resume") return ["Stopped before it was done", false];
  if (deskClearable(i)) return ["Stopped · leftover edits in the main folder", false];
  if (i.stopped) return ["Stopped · the Desk lead is on it", false];
  // Its landing step, already html (escaped, with its install's countdown).
  if (deskFinishing(i)) return [deskFinStep(i), false];
  if (i.state === "working") {
    if (i.waiting) return [`Waits: ${esc(firstLine(i.waiting))}`, false];
    // On another machine that isn't answering, or waits for something there: that, over its last step.
    if (i.remote?.quiet_since > 0 || i.remote?.wait) return [esc(cap(deskRemoteLine(i))), false];
    const s = sessionOf(deskHolderSid(i));
    const doing = s?.compacting_ms ? "Compacting…" : s?.running ? `Running: ${s.running}` : s?.doing || i.act?.now || "Thinking…";
    const at = s?.compacting_ms || s?.doing_ms || s?.since_ms || i.act?.at_ms || 0;
    return [`${esc(doing)}${at ? ` · <span data-ago="${at}">${ago(at)}</span>` : ""}`, true];
  }
  // Its new session wouldn't start (#172): its page has why and Retry.
  if (!i.holder && i.start_failed && (i.state === "new" || i.state === "queued")) return ["Couldn't start", false];
  if (deskCopyWaits(i)) return [esc(deskCopyWaitLine(i)), false];
  // Its hand-off failed: why, so it doesn't read as just not given out yet.
  // (Given out since, as next for a session, it isn't failed any more.)
  if (i.route_failed && !i.holder && !i.next_for) return [esc(`Couldn't give it out · ${firstLine(i.route_failed)}`), false];
  const q = deskQueueLine(i);
  if (q) return [esc(cap(q)), false];
  if (i.state === "queued" && i.after) { const on = deskAfterName(i); return [esc(on.startsWith("#") ? `After ${on}` : `Waits for ${on}`), false]; }
  if (!i.holder) return ["Nobody has it yet", false];
  return ["Queued", false];
}
/** The folded groups that are open, by key ("done", "away"), the same on every Desk; kept across restarts (#430). */
const deskB2Folds = (() => { try { return new Set(JSON.parse(localStorage.getItem("cue.desk2bFolds") || "[]")); } catch { return new Set(); } })();
/** A folded group, one line with its count and ›: Done today, Calls while away. Opened, its rows under it. */
function deskB2Fold(key, label, n, body) {
  const open = deskB2Folds.has(key);
  return `<button class="dk2b-fold${open ? " open" : ""}" data-desk2b-fold="${key}" aria-expanded="${open}">${label} <i>${n}</i><span aria-hidden="true">›</span></button>${open ? body : ""}`;
}


// ---------- clicks ----------

deskClicks({
  desk2bWork: () => { deskB2Unfold = !deskB2Unfold; renderMain(); },
  desk2bFold: (el) => {
    const k = el.dataset.desk2bFold;
    if (!deskB2Folds.delete(k)) deskB2Folds.add(k);
    try { localStorage.setItem("cue.desk2bFolds", JSON.stringify([...deskB2Folds])); } catch { /* kept for this window only */ }
    renderMain();
  },
  desk2bSessions: () => deskShow({ kind: "sessions" }),
});

/** Desk settings for `project` (the header's Desks menu, head_view.js): the settings screen (lands.js). */
function deskSettingsOpen(project) {
  const d = deskOf(project);
  // Every Desk the hub sends has how its work lands (desk/lands.rs desk_view), what it asks before on the same screen.
  landsChange(d?.id || d?.project || project);
}
