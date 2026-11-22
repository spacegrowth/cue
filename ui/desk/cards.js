// Cue UI desk — the Items card's Cards view (a List / Cards switch in its head, for now only so the two can be
// compared): at the left a filled card per item with a session (waiting on you and at work), the newest step first;
// at the right Up next, Done today and the calls while away, three quiet dashed boxes. A card or a row opens its item
// on the left as a List row does (data-desk-item, desk/act.js), lit in place. One column when the card is narrow.
// Reuses desk/b2.js's helpers; styles/desk-cards.css. Loaded after desk/b2.js (index.html, mini.html).

/** "list" or "cards", remembered in this window (a convenience: fine to lose). */
let deskCardsView = "list";
try { if (localStorage.getItem("cue.desk.itemsView") === "cards") deskCardsView = "cards"; } catch {}
const deskCardsOn = () => deskCardsView === "cards";
/** The calls box shows all its calls after "N more ›". */
let deskCardsAllCalls = false;
/** How many calls the box shows before "N more ›". */
const DESK_CARDS_CALLS = 5;

/** The List / Cards switch in the Items head. */
const deskCardsSwitch = () => `<span class="seg2 dkc-sw" role="group" aria-label="Items view">${[["list", "List"], ["cards", "Cards"]].map(([k, l]) => `<button class="seg2-b${deskCardsView === k ? " on" : ""}" data-desk-cards-view="${k}" aria-pressed="${deskCardsView === k}">${l}</button>`).join("")}</span>`;

/** Its kind as a word: Bug, Task, Feature. */
const deskCardsKind = (i) => (["bug", "task", "feature"].includes(i.kind) ? i.kind[0].toUpperCase() + i.kind.slice(1) : "");
/** When its newest step was stamped (its last change when it has none). */
const deskCardsStepAt = (i) => Math.max(0, ...deskProgStamps(i)) || deskMs(i.updated) || 0;
/** Lit while its page is open on the left. */
const deskCardsLit = (i) => deskOpen?.kind === "item" && deskIdIs(i, deskOpen.id);
/** "step 3 of 6 · Tests", or "not started". */
function deskCardsStep(i) {
  const at = Math.min(deskB2At(i), 5);
  return at < 0 ? "not started" : `step ${at + 1} of 6 · <b>${DESK_B2_STEPS[at]}</b>`;
}

/** One item with a session, as a filled card: #id, title and kind; ● its session · what it's doing; its six steps;
 *  its step and tests; what it's running (or Thinking…) and since when, else where it stands. */
function deskCardsCard(i) {
  const s = sessionOf(deskHolderSid(i));
  const work = i.state === "working" && !i.waiting && !deskFinishing(i);
  const doing = work ? s?.doing || i.act?.now || "" : "";
  let foot;
  if (work) {
    const run = s?.compacting_ms ? "Compacting…" : s?.running ? `Running: ${s.running}` : "Thinking…";
    const at = s?.compacting_ms || s?.doing_ms || s?.since_ms || i.act?.at_ms || 0;
    foot = `<div class="dkc-run mono">${esc(run)} · <span data-ago="${at || now()}">${ago(at || now())}</span></div>`;
  } else {
    const [st, mono] = deskB2Status(i);
    foot = `<div class="dkc-run${mono ? " mono" : ""}">${st}</div>`;
  }
  return `<div class="dkc-card${deskCardsLit(i) ? " on" : ""}${i.state === "needs_you" ? " ask" : ""}" role="button" tabindex="0" data-desk-item="${esc(i.id)}">`
    + `<div class="dkc-h"><span class="dkc-n">#${esc(i.id)}</span><b class="dkc-t">${esc(i.title)}</b><span class="dkc-k">${deskCardsKind(i)}</span></div>`
    + `<div class="dkc-who"><i class="dkc-dot" aria-hidden="true"></i><b>${esc(deskHolderOf(i))}</b>${doing ? `<span> · ${esc(deskOneLine(doing))}</span>` : ""}</div>`
    + deskB2Track(i)
    + `<div class="dkc-st"><span>${deskCardsStep(i)}</span><span>${deskB2Tests(i)}</span></div>${foot}</div>`;
}
/** Why a queued item waits, in a few words: "next for cue-12", "after #4", "waits for other #4", "new". */
function deskCardsWhy(i) {
  if (i.state === "held") return "parked";
  if (i.state === "new") return "new";
  if (i.after) { const on = deskAfterName(i); return on.startsWith("#") ? `after ${on}` : `waits for ${on}`; }
  if (deskCopyWaits(i)) return "starts when a copy is free";
  const nf = i.holder ? deskHolderOf(i) : i.next_for ? deskHolderOf({ holder: i.next_for }) : "";
  return nf ? `next for ${nf}` : "any free session";
}
/** A quiet row in a box: #id, its title, and a word at the right. */
const deskCardsRow = (i, right) => `<div class="dkc-row${deskCardsLit(i) ? " on" : ""}" role="button" tabindex="0" data-desk-item="${esc(i.id)}"><span class="dkc-n">#${esc(i.id)}</span><span class="dkc-rt">${esc(i.title)}</span><span class="dkc-rr">${esc(right)}</span></div>`;
/** A dashed box: its head and its rows. */
const deskCardsBox = (key, head, rows) => `<section class="dkc-box" data-desk-cards-box="${key}"><div class="dkc-bh">${head}</div>${rows}</section>`;
/** The calls while away: how many, since and till, "N more ›"; a row each with its time, what was decided and Undo. */
function deskCardsCalls(d) {
  const a = d.away, calls = deskAwayCalls(d);
  const shown = deskCardsAllCalls ? calls : calls.slice(0, DESK_CARDS_CALLS), more = calls.length - shown.length;
  const when = [a.since ? `since ${deskHm(deskMs(a.since))}` : "", a.until ? `till ${deskHm(a.until)}` : ""].filter(Boolean).join(" · ");
  const btn = more ? `<button class="dkc-more" data-desk-cards-calls>${more} more ›</button>` : deskCardsAllCalls && calls.length > DESK_CARDS_CALLS ? `<button class="dkc-more" data-desk-cards-calls>Fewer</button>` : "";
  const head = `<b>${calls.length} call${calls.length === 1 ? "" : "s"} while away</b><span class="dkc-bw">${esc(when)}${when && btn ? " · " : ""}</span>${btn}`;
  const rows = shown.map((c) => {
    const undo = c.undo === "can" ? `<button class="dkc-undo" data-desk-undo="${esc(c.id)}">Undo</button>` : c.undo === "asked" ? `<span class="dkc-rr">Undo asked</span>` : "<span></span>";
    return `<div class="dkc-row dkc-call" data-desk-call="${esc(c.id)}"><span class="dkc-n">${c.at ? esc(deskHm(deskMs(c.at))) : ""}</span><span class="dkc-rt">${esc(c.text)}</span>${undo}</div>`;
  }).join("");
  return deskCardsBox("calls", head, rows || `<div class="dkc-none">No calls yet.</div>`);
}

/** The Items card as Cards: its head as the List's (Items, the switch, Add an item…, Away, Pause); then With sessions
 *  at the left and Up next, Done today and the calls while away at the right. */
function deskCards(project, taskApprovals) {
  const g = deskGroups(project, taskApprovals), d = g.d;
  const mine = [...g.asks, ...g.check, ...g.landing, ...g.install, ...g.working, ...g.resume, ...g.paused]
    .filter((i, k, all) => i.holder && all.indexOf(i) === k).sort((a, b) => deskCardsStepAt(b) - deskCardsStepAt(a));
  const next = [...g.queued, ...g.held], done = deskB2DoneToday(d);
  const left = `<div class="dkc-col dkc-cards"><div class="dkc-gh">WITH SESSIONS · ${mine.length}<span>NEWEST STEP FIRST</span></div>${mine.map(deskCardsCard).join("") || `<div class="dkc-none">No item has a session now.</div>`}</div>`;
  const up = deskCardsBox("next", `<b>Up next</b><i>${next.length}</i>`, next.map((i) => deskCardsRow(i, deskCardsWhy(i))).join("") || `<div class="dkc-none">Nothing up next.</div>`);
  const dn = deskCardsBox("done", `<b>Done today</b><i>${done.length}</i>`, done.map((i) => deskCardsRow(i, `landed ${deskHm(deskMs(i.done_at || i.updated))}`)).join("") || `<div class="dkc-none">Nothing done today.</div>`);
  const right = `<div class="dkc-col dkc-side"><div class="dkc-gh">UP NEXT · DONE · CALLS</div>${up}${dn}${d?.away?.on ? deskCardsCalls(d) : ""}</div>`;
  return `<div class="dk-card dk-items dk2b-list dkc-list"><div class="dk2b-ih"><b>Items</b>${deskCardsSwitch()}${deskQuickAdd()}${deskAwaySwitch(d)}${deskPausedTag(d)}${deskPauseButton(d)}</div><div class="dk-scroll" data-scroll-key="desk-cards:${esc(project)}"><div class="dkc-grid">${left}${right}</div></div></div>`;
}

deskClicks({
  deskCardsView: (el) => {
    deskCardsView = el.dataset.deskCardsView === "cards" ? "cards" : "list";
    try { localStorage.setItem("cue.desk.itemsView", deskCardsView); } catch {}
    renderMain();
  },
  deskCardsCalls: () => { deskCardsAllCalls = !deskCardsAllCalls; renderMain(); },
});
