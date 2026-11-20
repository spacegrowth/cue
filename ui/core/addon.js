// Cue UI core: the add-on's slots in the window (ui/core/state.js has the rest of the window's state).
/** What the optional add-on adds to the window (addon.js, empty without it; a build with the add-on serves its own in
 *  that file's place). It sets these; as here, the window has none of it. */
const addOn = {
  /** Its views in the header's switch, after Board: { id, label() → the button's inside, html() → the view }. */
  views: [],
  /** A session's name as shown (the add-on may tidy it), and a mark drawn before it wherever its name is shown. */
  name: (sid, name) => name,
  nameMark: (sid) => "",
  /** A line on a session's card, under its title. */
  cardLine: (sid) => "",
  /** Its rows in Waiting, among the newest, and how many there are. */
  waiting: () => ({ html: "", count: 0 }),
  /** Its entries in the waiting chip's list, oldest first: { since, shown() → whether the window shows it now, open() }. */
  waitingChips: () => [],
  /** A notification clicked, `id` its key (a card's id, or one the add-on posted) and `sid` the card's session (null
   *  when it isn't a card): true when the add-on opened it in its own place, false to leave it to the window. */
  openNotice: (id, sid) => false,
  /** A card at the top of a session's chat; `crew`: the chat drawn on its lead's crew page. */
  chatTop: (sid, { crew = false } = {}) => "",
  /** A reply of session `sid` as drawn: its text (the add-on may take a block of its own out of it) and anything it
   *  draws after it (a card for that block). */
  reply: (sid, text) => ({ text, after: "" }),
  /** A reply's text with the add-on's own blocks taken out, for what reads or passes a reply on (Listen, Read, Send
   *  to…): no side effects, cheap. */
  plain: (text) => text,
  /** Items in a session's ⋯ menu. */
  chatMenu: (sid, s) => "",
  /** On lead `sid`'s crew page (team_page.js): its own head line in place of the lead's name (‹ Back to the board
   *  stays), "" for the usual one; a band under the head line; rows under the goal; asks of its own in Needs you
   *  (team_bar.js crewAsks): { items: [{ id, who, what, cmd, why, one_way, buttons, at_ms, html }], count }, each shown
   *  in the bar as who · what · cmd · why with `buttons` (a button with data-bar-key="a" / "d" takes ⌘⇧A / ⌘⇧D; none
   *  on a one-way ask), and `html` (optional) as its whole row on the Needs you page. An item with only `html` (or an
   *  older `{ html, count }`) is in the bar as "waits for you" with Open ›, its html whole on that page; and
   *  decisions of its own in the lead's chat, drawn as the crew
   *  log's are (team.js crewLogRow, big): { html, count }, or with `items: [{ at_ms, html }]` placed in time order
   *  among the lead's words (else `html` comes first, oldest). The page opens with addOn.openCrewPage(sid, { from, back }):
   *  within the add-on's view `from` (its tab stays selected, Back and Esc return to it), and closes with
   *  addOn.closeCrewPage(from) (team_page.js). */
  crewPageHead: (sid) => "",
  crewPageBand: (sid) => "",
  crewPageRows: (sid) => "",
  crewPageNeeds: (sid) => ({ items: [], count: 0 }),
  crewPageDecided: (sid) => ({ html: "", count: 0 }),
  /** The row under the head of lead `sid`'s crew page in place of its goal row (html), or null for the page's own. */
  crewPageGoal: (sid) => null,
  /** A page of its own on the left of lead `sid`'s crew page, in the lead's chat's place (the pane's head, Needs you
   *  and the box stay): { key: what it is (its scroll is kept by it), html }, or null. It shares the page's one left
   *  (team_page.js crewLeftOf) with a teammate and Needs you: while it's there it is what the left shows; picking a
   *  teammate or Needs you, The lead's row, ‹ The lead's chat or Esc closes it (crewPageLeftClose). */
  crewPageLeft: (sid) => null,
  /** Esc on the crew page, one step of its own (an edit it has open, then its page on the left): asked after the page's
   *  menus and btw, before the box gives up the cursor and before the page closes. True when it took the key. */
  crewPageEsc: (sid) => false,
  /** Its page on the left of lead `sid`'s crew page closes (the left goes to something else): true when one was open. */
  crewPageLeftClose: (sid) => false,
  /** A crew page for `key` when it is no session of the window's: the record of something whose lead is gone, or null.
   *  Non-null, addOn.openCrewPage(key, …) opens the page as the record: its head (crewPageHead), the rows under the goal
   *  (crewPageRows; there is no goal line of the page's own), the left (crewPageLeft, which must give one; no box, no
   *  Needs you), and the list: the band (crewPageBand), then `mates` (html: its crew by name). */
  crewPageRecord: (key) => null,
  /** The Desk (desk.js) of `project`: its holders beside the leads and sessions, each a group in the list:
   *  [{ key, name, sub (what it is, a few words), items (Desk items it holds) }]. */
  deskHolders: (project) => [],
  /** The pane for one of its holders (from deskHolders) when its head is clicked: HTML under "‹ <lead's name>", or "" for
   *  its name alone. */
  deskPane: (holder) => "",
  /** A live session found with ⌘K while one of its views is shown: opened there, true; false to show its conversation
   *  in a pop-up over the view (sheets.js openFound). */
  openFound: (sid) => false,
  /** What ⌘K finds while one of its views is shown, above that view's sessions (sessions.js findHere): { title, rows:
   *  [{ html, open }], sessions, sids, place, ask }, or null for the Board's ⌘K. */
  findHere: (q) => null,
  /** Click rules as app/delegate.js walks them ((target, event) → true when taken), tried before the Board's own. */
  clicks: [],
  /** The crew page opened, closed, or went to another lead (team_page.js crewPageSet): `sid` the lead's open now, or
   *  null. */
  crewPageChanged: (sid) => {},
  /** After each new state from Cue is in `state`, before the window draws it (app.js setState). */
  stateChanged: () => {},
  /** Once the window has its state (boot, app.js). */
  boot: () => {},
};
