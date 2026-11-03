// ---------- The About tab: what a session is about ----------
// Off until you follow a session (or Settings → Summaries follows every one). Then a small model reads the
// session's chat and says what it's about, where it stands and the session in a few sentences, and after each of
// its turns reads what was said since and adds a line for the turn (tasks.rs). The window's state carries every
// followed session's (state.tasks); the tab shows it, the cards show what it's about and where it stands, and the
// chat marks the turns since you looked. The session never sees any of it.
const aboutTabOn = new Set();     // sessions showing the About tab
const aboutTurnsOpen = new Set(); // sessions showing every turn, not just the newest 12

const aboutOf = (sid) => state.tasks?.[sid];
/** About is on (Settings → Summaries): off, the window shows none of it, followed or not. */
const aboutShown = () => state.settings?.tasks?.on === true;
/** The session's About while it's on; null otherwise. */
const aboutTracked = (sid) => (aboutShown() ? aboutOf(sid) || null : null);

/** The tab's button, last in the row: a live dot while it reads the chat. */
function aboutTabBtn(sid) {
  if (!aboutShown()) return "";
  const t = aboutOf(sid);
  return `<button class="stab ${aboutTabOn.has(sid) ? "on" : ""}" data-about-tab data-sid="${esc(sid)}">About${t?.busy ? `<span class="stab-n"><span class="dot-live"></span></span>` : ""}</button>`;
}
/** Where the tab's page goes, in place of the chat (extensions.js placeExt hides the chat under it). */
const aboutSlot = (sid) => (sid && aboutShown() && aboutTabOn.has(sid) ? `<div class="ac-slot" data-about-slot>${aboutPage(sid)}</div>` : "");

/** On a card: what a followed session is about (bold) and where it stands, in place of its last reply's opening;
 *  "" when it has neither yet. */
function aboutCardText(sid) {
  const t = aboutTracked(sid);
  if (!t?.about && !t?.now) return "";
  return `<div class="tk-card">${t.about ? `<div class="tk-about">${esc(t.about)}</div>` : ""}${t.now ? `<div class="tk-now">${esc(t.now)}</div>` : ""}</div>`;
}

/** When you last looked at a session's chat before this look (chat.js catchUp), 0 if Cue doesn't know. */
const aboutLookedAt = (sid) => (catchUp?.sid === sid ? catchUp.since : looked[sid]) || 0;

/** In the chat: for a followed session you've come back to (Catch me up's rule: 15 minutes or more away, with
 *  replies since), a divider where the new part starts, a line per turn since then. { at, html }, or null. */
function aboutSince(sid, ex) {
  const t = aboutTracked(sid), c = catchUp;
  if (!t || !c || c.sid !== sid || !c.since || c.openedAt - c.since < CATCH_UP_AFTER) return null;
  if (ex.some((e) => e.role === "you" && e.at_ms > c.openedAt)) return null;
  const turns = (t.turns || []).filter((u) => u.at_ms > c.since && u.at_ms <= c.openedAt);
  if (!turns.length && !ex.some((e) => e.role === "agent" && e.at_ms > c.since && e.at_ms <= c.openedAt)) return null;
  const rows = turns.map((u) => `<div class="tk-turn"><span class="tk-t">${esc(aboutClock(u.at_ms))}</span><span>${esc(u.line)}</span></div>`).join("");
  return { at: c.since + 1, html: `<div class="tk-since"><div class="tk-div"><i></i>${esc(ago(c.since).toUpperCase())} SINCE YOU LOOKED${turns.length ? ` · ${turns.length} TURN${turns.length === 1 ? "" : "S"}` : ""}<i></i></div>${rows}${rows ? `<div class="tk-div"><i></i></div>` : ""}</div>` };
}
const aboutClock = (ms) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

const aboutMoney = (usd) => (usd > 0 && usd < 0.01 ? "under 1¢" : `$${(usd || 0).toFixed(2)}`);

/** The About tab: what the session is about and, under it, the session in a few sentences; where it stands now, and
 *  a line per turn (newest first, the ones since you looked in bold; 12, then "N earlier"). Not followed yet: what
 *  following it does, and Start. */
function aboutPage(sid) {
  const t = aboutOf(sid);
  if (!t) {
    return `<div class="tk-page"><div class="tk-intro">
      <p>What this session is about, where it stands, and a line for each turn.</p>
      <p class="dim">Start sends this session's chat to a small model (Haiku), which writes it. After each turn Cue sends it the new messages. The session never sees this. What it costs shows here.</p>
      <button class="btn primary" data-about-do="start" data-sid="${esc(sid)}">Start</button></div></div>`;
  }
  const seen = aboutLookedAt(sid), turns = [...(t.turns || [])].reverse();
  const shown = aboutTurnsOpen.has(sid) ? turns : turns.slice(0, 12);
  const unseen = turns.filter((u) => seen && u.at_ms > seen).length;
  const now = t.busy ? `Reading ${t.busy === "listing" ? "the chat" : "the last turn"}…` : t.err ? `Couldn't update: ${t.err}` : "";
  return `<div class="tk-page tk-aboutpage">${t.about ? `<div class="tk-about big">${esc(t.about)}</div>` : ""}${t.summary ? `<p class="tk-summary">${esc(t.summary)}</p>` : ""}
    ${t.now ? `<div class="tk-head">NOW</div><div class="tk-nowline">${esc(t.now)}</div>` : ""}
    ${turns.length ? `<div class="tk-head">TURNS${unseen ? ` · ${unseen} SINCE YOU LOOKED` : ""}</div>${shown.map((u) => `<div class="tk-turn ${seen && u.at_ms > seen ? "new" : ""}"><span class="tk-t">${esc(aboutClock(u.at_ms))}</span><span>${esc(u.line)}</span></div>`).join("")}${turns.length > shown.length ? `<button class="tk-more" data-about-do="turns" data-sid="${esc(sid)}">${turns.length - shown.length} earlier</button>` : ""}` : ""}
    ${!t.about && !t.summary && !t.now && !turns.length ? `<div class="ac-empty">${t.busy ? "Reading the chat…" : "Nothing yet: it fills in after the next turn."}</div>` : ""}
    <div class="tk-dock"><div class="tk-foot"><span class="tk-said ${t.err && !t.busy ? "tk-err" : ""}">${esc(now)}</span>
      <span class="dim">So far: ${aboutMoney(t.spent?.usd)} · ${esc(helperTokens(t.spent?.tokens || 0) || "0 tokens")}</span>
      <button class="btn" data-about-do="refresh" data-sid="${esc(sid)}" ${t.busy ? "disabled" : ""}>Refresh</button>
      <button class="btn" data-about-do="stop" data-sid="${esc(sid)}">Stop</button></div></div></div>`;
}

document.addEventListener("click", (e) => {
  const tab = e.target.closest?.("[data-about-tab]");
  if (tab) {
    e.stopPropagation();
    const sid = tab.dataset.sid;
    actTabOn.delete(sid);
    bgTabOn.delete(sid);
    extTabOf.delete(sid);
    aboutTabOn.add(sid);
    helperView = null;
    return renderMain();
  }
  const d = e.target.closest?.("[data-about-do]");
  if (!d) return;
  e.stopPropagation();
  const sid = d.dataset.sid, what = d.dataset.aboutDo;
  if (what === "turns") { aboutTurnsOpen.add(sid); return renderMain(); }
  const cmd = { start: "tasks_start", refresh: "tasks_refresh", stop: "tasks_stop" }[what];
  if (cmd) invoke(cmd, { sessionId: sid }).catch((x) => toast(String(x)));
}, true);

/** Settings → Progress: the status list each session ends its replies with (statuslist.rs), then About: what turning
 *  it on does, in plain words (a small model writes it, not Cue, and the session never sees it), which sessions get
 *  one, and what it has cost. */
function aboutSettings(st) {
  const on = st.tasks?.on === true;
  return `
    <div class="set-group">Where each session stands</div>
    ${setRow("Status list", "A session's replies end with what's done, what it's on and what's left: the ring beside the mic, a click for the whole list. Each session starts off: turn it on from its ring. The session writes it itself, about 0.2¢ a reply. Off here: no ring in any session.", toggle("status.on", st.status?.on !== false))}
    <div class="set-group">About each session</div>
    ${setRow("What each session is about", "An About tab in each session. Start it there and a small model (Haiku) reads the chat and writes what the session is about, where it stands and a line per turn; the cards show it too. The session never sees it.", toggle("tasks.on", on))}
    ${on ? setRow("For every session", "Without clicking Start.", toggle("tasks.auto", st.tasks?.auto === true)) : ""}
    ${aboutSpentRow()}`;
}
/** What this tab's two have cost, all sessions together: the status list (an estimate: it's part of each session's own
 *  bill, which doesn't count it apart; statuslist.rs spent) and About (exact: Cue's own calls, kept after one is
 *  stopped; tasks.rs spent). */
function aboutSpentRow() {
  const l = state.status_spent || {}, t = state.tasks_spent || {};
  const list = l.replies ? `about ${aboutMoney(l.usd)} · ~${helperTokens(l.tokens || 0) || "0 tokens"} over ${l.replies} message${l.replies === 1 ? "" : "s"} (an estimate: it's part of each session's own bill)` : "nothing yet";
  const about = t.calls ? `${aboutMoney(t.usd)} · ${helperTokens(t.tokens || 0) || "0 tokens"}${t.tracked ? ` · ${t.tracked} session${t.tracked === 1 ? "" : "s"} followed now` : ""}` : "nothing yet";
  return setRow("What it has cost", `<span class="set-line">Status list: ${esc(list)}</span><span class="set-line">About: ${esc(about)}</span>`, "");
}
