// Cue UI board/desk_cards — each Desk on the Board as its lead's card (#825, option B of #823). While anything on a
// Desk waits on you, that Desk is one card in Waiting: its lead's name and the Desk's tag, how many items are at work
// and to check, then what waits on you inside it (the first ask, answered from the card, then the items, a line
// each). With nothing waiting it folds to one line under Sessions. Its sessions get no cards of their own on the
// Board: their finished turns are the lead's to pick up, their asks are in their Desk's card. styles/cards.css .dkw.

/** How many rows the card lists under its ask before "+ N more · Open the Desk". */
const DKW_ROWS = 3;

/** The Desks the Board shows: every Desk, or with a project picked in the header, that project's. */
const boardDesks = () => (state.desk?.projects || []).filter((d) => inProj(deskWorkspace(d)) || inProj(d.project));

/** Whether session `sid` works for a Desk, so it shows on the Board only through its Desk's card or line. */
const onDeskCard = (sid) => typeof deskForSession === "function" && !!deskForSession(sid);

/** What Desk `d` has waiting on you, from `needs` (the Board's asks, needsYou order): `asks`, its sessions' asks
 *  (the lead's too), newest first; `items`, its items that ask you something with no ask of theirs among those, then
 *  those to check; `n`, all of them; `atWork` and `toCheck`, its counts; `at`, when the newest of them came. */
function deskWaitsOf(d, needs) {
  const asks = needs.filter((i) => i.kind !== "waiting" && deskForSession(i.session_id)?.project === d.project)
    .sort((a, b) => b.created_ms - a.created_ms);
  const g = deskGroups(d.project, deskTaskApprovals(d));
  const askIds = new Set(asks.map((i) => i.id));
  const items = [...g.asks.filter((i) => !askIds.has(i.ask?.item_id)), ...g.check];
  const at = Math.max(0, ...asks.map((i) => i.created_ms || 0), ...items.map((i) => deskMs(i.updated) || 0));
  return { d, asks, items, n: asks.length + items.length, atWork: g.working.length, toCheck: g.check.length, at };
}

/** The item ask `it` is for on Desk `d`, or null. */
const dkwItemOf = (d, it) => (d.items || []).find((i) => i.ask?.item_id === it.id) || null;

/** The card's ask: who asks and for which item, what, and its answers (Allow and Deny, Choose…); a click on the
 *  rest opens it in Active, as a Waiting row does. */
function dkwAsk(w, it, open) {
  const item = dkwItemOf(w.d, it);
  const from = `<b>${esc(nameOf(it.session_id, it.project))}</b> ${esc(verb(it))}${item ? ` · for <button class="dkw-for" data-desk-hand="${esc(w.d.project)}" data-id="${esc(item.id)}">#${esc(item.id)}</button>` : ""}`;
  const text = isBash(it) ? cmdWords(plain(summary(it))) : esc(plain(summary(it)));
  return `<div class="dkw-ask ${open ? "on" : ""} ${answering.has(it.id) ? "answering" : ""}" data-big="${esc(it.id)}"><div class="dkw-from">${from}</div>`
    + `<div class="nrow-text ${isBash(it) ? "mono" : ""}">${text}</div>${needQuick(it)}</div>`;
}
/** A row under the ask: an item (its number, its title, asks or to check), which opens on its Desk; or another ask,
 *  which opens in Active. */
function dkwRow(w, { req, item: x }) {
  if (req) return `<div class="dkw-ir" role="button" tabindex="0" data-big="${esc(req.id)}"><span class="dkw-id">${esc(nameOf(req.session_id, req.project))}</span><span class="dkw-tt">${esc(plain(summary(req)))}</span><span class="dkw-st ask">asks</span></div>`;
  const asks = x.state === "needs_you";
  return `<button class="dkw-ir" data-desk-hand="${esc(w.d.project)}" data-id="${esc(x.id)}"><span class="dkw-id">#${esc(x.id)}</span><span class="dkw-tt">${esc(x.title || "")}</span><span class="dkw-st ${asks ? "ask" : ""}">${asks ? "asks" : "to check"}</span></button>`;
}
/** The tag after the lead's name: the tray, "Desk", its name; outlined, as the machine pills are. */
const dkwTag = (d) => `<span class="dkw-tag">${deskKindIcon("tray")}Desk · ${esc(deskName(d))}</span>`;
/** "3 at work · 5 to check" (either left out at 0). */
const dkwCounts = (w) => [w.atWork ? `<b>${w.atWork}</b> at work` : "", w.toCheck ? `<b>${w.toCheck}</b> to check` : ""].filter(Boolean).join(" · ");

/** Desk `w.d`'s card in Waiting (`w` from deskWaitsOf). Its head (the lead, the Desk's tag) opens the Desk. */
function deskLeadCard(w, isOpen) {
  const d = w.d, lead = deskLead(d), s = lead ? sessionOf(lead.sid) : null;
  const [first, ...more] = w.asks;
  const rows = [...more.map((req) => ({ req })), ...w.items.map((item) => ({ item }))];
  const shown = rows.slice(0, DKW_ROWS), left = rows.length - shown.length;
  const who = `${s ? badge(s.harness) : ""}<span class="proj">${esc(lead ? deskLeadName(d) : deskName(d))}</span>${dkwTag(d)}`;
  const counts = dkwCounts(w);
  return `<div class="nrow dkw" data-dkw="${esc(d.project)}">`
    + `<div class="nrow-top"><span class="nrow-who" data-desk-hand="${esc(d.project)}">${who}</span><span class="nrow-side"><span class="age">${ago(w.at)}</span></span></div>`
    + (counts ? `<div class="dkw-cnt">${counts}</div>` : "")
    + `<div class="dkw-in"><div class="dkw-ih">Waits on you · ${w.n}</div>${first ? dkwAsk(w, first, isOpen(first.session_id)) : ""}${shown.map((r) => dkwRow(w, r)).join("")}`
    + (left > 0 ? `<button class="dkw-more" data-desk-hand="${esc(d.project)}">+ ${left} more · Open the Desk ›</button>` : "")
    + `</div></div>`;
}

/** Desk `w.d`'s line under Sessions while nothing on it waits on you: the tray, the lead, the Desk, its counts, ›. */
function deskLine(w) {
  const d = w.d, counts = dkwCounts(w);
  return `<button class="dkw-line" data-desk-hand="${esc(d.project)}">${deskKindIcon("tray")}<b class="dkw-nm">${esc(deskLeadName(d) || deskName(d))}</b><span class="dkw-tag">Desk · ${esc(deskName(d))}</span>`
    + `<span class="dkw-line-n">${counts || "nothing at work"}</span><span aria-hidden="true">›</span></button>`;
}

/** The Board's Desks split by whether anything waits on you: `cards` for Waiting, `lines` for Sessions. A Desk with
 *  no lead and nothing open is left out. */
function boardDeskSplit(needs) {
  const all = boardDesks().map((d) => deskWaitsOf(d, needs));
  const cards = all.filter((w) => w.n > 0);
  const lines = all.filter((w) => w.n === 0 && (deskLead(w.d) || w.atWork || deskB2Open(w.d)));
  return { cards, lines };
}
