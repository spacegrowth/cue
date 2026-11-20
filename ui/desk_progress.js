// Cue UI desk — Progress, the popover of Items (#409, mock J2 of the "Live progress" canvas page): the whole Desk, one row
// per open item, the six steps as columns named once, each row's marks joined by a line with how long each step took,
// and a Now column saying what it's doing or waits on (with its one button when it needs you).
// Its steps come from Cue (hub/desk_steps.rs `steps`): it redraws as Cue's state comes in, the current step's time
// counts up with the other live times (tickAges). Opens as a popover over the Desk from Progress in the Items head.
// Loaded after desk_list.js (index.html, mini.html).

const DESK_PROG_STEPS = ["Picked up", "Building", "Tests", "Commit", "Landing", "To check"];
/** The Progress popover (#409): open over the Desk while true; Esc, a click outside it or a row closes it. */
let deskProgShown = false;
function deskProgToggle(on = !deskProgShown) {
  deskProgShown = on;
  renderMain();
}
/** The Progress button in the Items head. */
const deskProgButtonHead = () => `<button class="btn small dk-prog-btn${deskProgShown ? " on" : ""}" data-desk-prog-toggle aria-haspopup="dialog" aria-expanded="${deskProgShown}">Progress</button>`;

/** A length of time, short: "40s", "6m", "2h", "3d". */
function deskProgDur(ms) {
  const s = Math.max(0, Math.round(ms / 1000));
  return s < 60 ? `${s}s` : s < 3600 ? `${Math.floor(s / 60)}m` : s < 86400 ? `${Math.floor(s / 3600)}h` : `${Math.floor(s / 86400)}d`;
}
/** Item `i`'s steps as Cue stamped them (ms, 0: not reached), six long. */
const deskProgStamps = (i) => DESK_PROG_STEPS.map((_, k) => +(i.steps || [])[k] || 0);
/** The step it's at: the furthest it reached, or -1 when it hasn't started. */
const deskProgAt = (i) => deskProgStamps(i).reduce((at, t, k) => (t ? k : at), -1);
/** How its current step looks: "work" (moving), "you" (asks you something), "check" (your turn to try it: calm, not an
 *  error, #443), "fail" (its check failed), "wait" (held up). */
function deskProgLook(i) {
  if (i.finish?.step === "check_failed") return "fail";
  if (i.state === "needs_you") return "you";
  if (i.state === "to_check" && !deskInstallWait(i)) return "check";
  if (i.state === "working" && !i.stopped && !i.waiting && !deskStalled(i)) return "work";
  return "wait";
}

/** What it's doing or waits on, in a few plain words (HTML). */
function deskProgNow(i) {
  const who = esc(deskHolderOf(i));
  if (i.state === "needs_you") return deskVerbHtml(i) || "Asks you something";
  if (i.state === "to_check") return deskVerbHtml(i);
  if (deskFinishing(i) && i.stopped) return `Stopped while landing · the Desk lead is on it`;
  if (deskFinishing(i)) return deskFinStep(i);
  if (i.state === "held") return "Parked: starts when you press Start";
  if (i.state === "paused") return `Paused by the Desk lead${i.paused?.[1] ? `: ${esc(i.paused[1])}` : ""}`;
  if (i.state === "needs_resume") return `${who || "Its session"} stopped before it was done`;
  if (i.state === "working") {
    if (i.stopped) return `${who} stopped · the Desk lead is on it`;
    if (i.waiting) return `Waits: ${esc(firstLine(i.waiting))}`;
    if (i.act && deskStalled(i)) return `Stalled · ${esc(i.act.was)} · <span data-ago="${i.act.at_ms}">${ago(i.act.at_ms)}</span> ago`;
    if (i.act) return `${who} · ${esc(i.act.now)}`;
    return who ? `${who} is on it` : "At work";
  }
  if (deskCopyWaits(i)) return esc(deskCopyWaitLine(i));
  if (!i.holder) return i.start_failed ? "Couldn't start a session for it" : "Nobody has it yet";
  return i.after ? `Queued with ${who}, after #${esc(i.after)}` : `Queued with ${who}`;
}
/** Its one button when it needs you: Answer (its page), Allow / No, Looks right. */
function deskProgButton(i) {
  if (i.state === "to_check" && !deskInstallWait(i) && !deskUncommitted(i) && !deskLooksSoon.has(i.id)) return `<button class="btn small primary" data-desk-act="looks_right" data-id="${esc(i.id)}">Looks right</button>`;
  if (i.state !== "needs_you") return "";
  const r = deskItemRequest(i);
  if (r?.kind === "permission") return deskAllowNo(r.id);
  return `<button class="btn small primary" data-desk-prog-open="${esc(i.id)}">Answer</button>`;
}

/** A step's mark, a circle: filled with a tick once done, lit while it's the one it's at, an empty circle ahead. */
function deskProgMark(state, look) {
  if (state === "done") return '<svg class="dk-grm" width="18" height="18" viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="7" fill="var(--accent)"/><path d="m4.6 8.2 2.2 2.2 4.6-4.8" fill="none" stroke="var(--on-accent)" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>';
  if (state === "todo") return '<svg class="dk-grm" width="18" height="18" viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="6.5" fill="var(--panel)" stroke="var(--line)" stroke-width="1.4"/></svg>';
  const c = look === "work" || look === "check" ? "var(--accent)" : look === "wait" ? "var(--muted)" : "var(--deny)";
  const inner = {
    work: `<circle class="dk-blink" cx="8" cy="8" r="3" fill="${c}"/>`,
    wait: `<path d="M8 4.8V8l2 1.4" fill="none" stroke="${c}" stroke-width="1.6" stroke-linecap="round"/>`,
    check: `<circle cx="8" cy="6.2" r="1.7" fill="${c}"/><path d="M4.9 11.2c.6-1.6 1.7-2.4 3.1-2.4s2.5.8 3.1 2.4" fill="none" stroke="${c}" stroke-width="1.6" stroke-linecap="round"/>`,
    you: `<path d="M8 4.4v4.2" stroke="${c}" stroke-width="1.7" stroke-linecap="round"/><circle cx="8" cy="11.2" r="1" fill="${c}"/>`,
    fail: `<path d="m5.4 5.4 5.2 5.2M10.6 5.4l-5.2 5.2" stroke="${c}" stroke-width="1.7" stroke-linecap="round"/>`,
  }[look];
  return `<svg class="dk-grm" width="18" height="18" viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="6.5" fill="var(--panel)" stroke="${c}" stroke-width="1.8"/>${inner}</svg>`;
}

/** One item's row: its title, kind and number, who holds it; a cell per step; what it's doing now. */
function deskProgRow(i) {
  const st = deskProgStamps(i), at = deskProgAt(i), look = deskProgLook(i);
  const cells = st.map((t, k) => {
    const state = at < 0 || k > at ? "todo" : k < at ? "done" : "cur";
    // How long the step took: till the next step it reached; the one it's at counts up (tickAges), unless it waits on
    // you to check it (the time it has been in your app isn't the step's work).
    // Picked up took till its first round's next step: a later round starts building afresh (#421).
    const next = (k === 0 && i.backs?.length ? i.backs[0].steps || [] : st).slice(k + 1).find(Boolean);
    const took = !t ? "" : next ? deskProgDur(next - t) : state === "cur" ? `<span data-ago="${t}">${ago(t)}</span>` : "";
    const l = k > 0 ? `<i class="dk-grl${at >= k ? " on" : ""}"></i>` : "", r = k < 5 ? `<i class="dk-grr${at > k ? " on" : ""}"></i>` : "";
    return `<span class="dk-grc">${l}${r}${deskProgMark(state, look)}<span class="dk-grt${state === "cur" ? ` cur ${look}` : ""}">${took}</span></span>`;
  }).join("");
  const who = deskHolderOf(i);
  return `<div class="dk-gr" role="button" tabindex="0" data-desk-prog-open="${esc(i.id)}" data-desk-prog-row="${esc(i.id)}"><span class="dk-grn"><span class="dk-grtl">${esc(i.title)}</span><span class="dk-grmeta"><span class="dk-kid">${deskKindIcon(i)}<span class="dk-id">${esc(deskNum(i))}</span></span>${who ? `<span>· ${esc(who)}</span>` : ""}<span class="dk-grmono">· ${ago(deskMs(i.created))}</span></span></span>${cells}<span class="dk-grnow"><span class="dk-grw ${look}">${deskProgNow(i)}</span>${deskProgButton(i) ? `<span class="dk-grb">${deskProgButton(i)}</span>` : ""}</span>${deskProgBack(i)}</div>`;
}

/** Reopened items whose full send-back note is shown (#444): a click on the line opens it, another folds it. */
const deskProgNoteOpen = new Set();
/** Under its marks when it went back to be redone (#421, #444): one plain line, the round it's on and why it went back
 *  (the latest time; the round counts them all): your note's first words, the whole note on a click. */
function deskProgBack(i) {
  const b = i.backs?.[i.backs.length - 1];
  if (!b) return "";
  const round = `<b>Round ${i.backs.length + 1}</b>`;
  const note = (b.why || "").trim();
  if (!note) return `<span class="dk-grbk"><span class="dk-grbk-t">${round} · went back to work from ${DESK_PROG_STEPS[+b.from] || "its finish"}</span></span>`;
  const open = deskProgNoteOpen.has(i.id);
  return `<span class="dk-grbk"><button class="dk-grbk-t${open ? " open" : ""}" data-desk-prog-note="${esc(i.id)}" aria-expanded="${open}">${round} · you sent it back: ${open ? esc(note) : esc(firstLine(note))}</button></span>`;
}

/** The Progress popover: every item under way (not the queued or parked ones, which haven't started) in the order the
 *  list has them (yours first, then landing, waiting to install, at work), under the step names. */
function deskProgress(project, taskApprovals) {
  const g = deskGroups(project, taskApprovals);
  const rows = [...g.waiting, ...g.review, ...g.landing, ...g.install, ...g.working, ...g.resume, ...g.paused];
  const you = rows.filter((i) => ["you", "check"].includes(deskProgLook(i))).length, later = g.queued.length + g.held.length;
  const head = `<div class="dk-gr dk-grh"><span>Item</span>${DESK_PROG_STEPS.map((s) => `<span class="dk-grhc">${s}</span>`).join("")}<span class="dk-grnow">Now</span></div>`;
  const body = rows.length ? head + rows.map(deskProgRow).join("") : `<div class="dk-empty">Nothing under way.</div>`;
  return `<div class="dk-prog" role="dialog" aria-label="Progress"><div class="dk-ch"><b>Progress</b><span class="dk-grcount">${rows.length} under way${you ? ` · ${you} need${you === 1 ? "s" : ""} you` : ""}${later ? ` · ${later} not started` : ""}</span><button class="btn small dk-prog-x" data-desk-prog-toggle aria-label="Close">Close</button></div>
    <div class="dk-scroll" data-scroll-key="desk-progress:${esc(project)}">${body}</div></div>`;
}
/** A row (or its Answer) closes the popover and opens the item's page. */
function deskProgOpen(id) {
  deskProgShown = false;
  deskShow({ kind: "item", id });
}
