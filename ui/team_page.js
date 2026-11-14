// ---------- a lead's crew on a page of its own (team.js has the drawer this grows from) ----------
// "Open full ›" on the crew's tray line opens it across the window, in the board's and the chat's place (the top bar and
// tabs stay): ‹ Back to the board (or Esc, or Open chat ›) goes back to the lead's chat. It fits the window: a head line;
// the goal on one line (All of it › opens its four boxes in place); the crew, one lane per member still in it, what it
// did as small steps, where its work is and its links, the finished ones folded into "N done ›" (the drawer's fold);
// at the bottom the lead's stream (what it said and what it decided on its own, newest last, your own messages small on
// the right, the lead's box under it: the chat's draft, so what you type carries over) beside Needs you. The lanes and
// the stream scroll inside themselves. The optional add-on may add to it (core.js addOn.crewPage*): its own head line,
// a band under it, rows under the goal, rows in Needs you and decisions in the stream.

/** The lead whose crew's page is open; null when none is. It closes when that session goes away, or another opens. */
let crewPageSid = null;

/** Open the crew's page for lead `sid`. */
function crewPageOpen(sid) {
  crewPageSid = sid;
  mateOpenLeads.delete(sid);
  mateMenu = null;
  if (typeof crewMenuFor !== "undefined") crewMenuFor = null;
  view = "board";
  renderMain();
}
/** The page for lead `sid` from elsewhere (the add-on's): its chat becomes the one open first, so Back returns to it. */
addOn.openCrewPage = (sid) => {
  if (!sessionOf(sid)) return;
  if (active?.sid !== sid) setActive(null, sid);
  crewPageOpen(sid);
};
/** Back to the board, the lead's box keeping the cursor. */
function crewPageClose() {
  crewPageSid = null;
  mateMenu = null;
  renderMain();
  focusComposer();
}
/** The page across the board's place (columns and chat pane); null when it isn't open, or its lead is gone or no
 *  longer the open one: then it closes. Another tab shows that tab; back on Board, the page is still there. */
function crewPageShown() {
  if (!crewPageSid || view !== "board") return null;
  if (!sessionOf(crewPageSid) || (active?.sid && active.sid !== crewPageSid)) { crewPageSid = null; return null; }
  return `<div class="board cp-board">${crewPageView(crewPageSid)}</div>`;
}

/** The latest of a member's decisions in the crew's log: by its Agent call when the entry has one (a member started
 *  again under the same name is another call), else by its name. */
const crewEntryOf = (sid, t) => (sessionOf(sid)?.crew_log || []).filter((d) => d.call ? d.call === t.call : d.who === t.name).sort((a, b) => (a.at_ms || 0) - (b.at_ms || 0)).at(-1) || null;
/** What its review found, in a few words: "2 should-fix", "1 note", "clean". */
function crewFound(t) {
  const n = (t.findings || []).length, fix = fixCount(t.findings);
  return [fix ? `${fix} should-fix` : "", n - fix ? plural(n - fix, "note") : ""].filter(Boolean).join(", ") || "clean";
}
/** A member's steps so far, oldest first, as short words (`code` in backticks): its decision's head and what came of
 *  it, when the log has one; else what its row in the drawer says (at work, reviewed: what it found, what's next). */
function crewSteps(sid, t) {
  const d = crewEntryOf(sid, t);
  if (d?.head) return [d.head, ...(d.steps || [])];
  const out = [];
  if (t.state === "working") out.push(t.fix_rounds ? "sent back" : "", t.fix_rounds ? "fixing" : "working");
  else if (t.state === "queued") out.push("review waits for the lead");
  else if (t.state === "reviewing") out.push("reviewing");
  else if (t.state === "reviewed") {
    out.push(`reviewed: ${crewFound(t)}`);
    if (mateNeeds(sid, t)) out.push("waits for you");
    else if (mateCueHandles(sid, t)) out.push(mateReadyAct(t) === "fixes" ? "Cue sends it back" : "Cue commits it");
    else if (mateOnLead(t)) out.push(mateFixesKept(t) ? "fixes wait for the lead" : "its lead commits it");
    else out.push("waits for you");
  } else if (t.state === "committed") out.push(t.commit ? `committed \`${t.commit.slice(0, 7)}\`` : "committed");
  else if (t.state === "aside") out.push("set aside");
  else if (t.state === "stopped") out.push("stopped");
  else out.push("done");
  if (mateAsk(sid, t)) out.push("asks you");
  return out.filter(Boolean);
}
/** A member's state in a word, and its pill's look: "need" (red), "live" (dark: at work, or what Cue does next). */
function crewStateWord(sid, t) {
  if (mateAsk(sid, t)) return ["asks you", "need"];
  if (mateNeeds(sid, t)) return ["needs you", "need"];
  const words = { working: ["working", "live"], queued: ["queued", ""], reviewing: ["reviewing", "live"], reviewed: ["reviewed", ""], committed: ["committed", ""], aside: ["set aside", ""], stopped: ["stopped", ""] };
  return words[t.state] || ["done", ""];
}
/** Where its work is: at work, its line as in the drawer (how long, what it's on, its files so far); else its files
 *  and +/−, the commit it went into, and how long ago it finished. */
function crewWhere(t) {
  if (t.state === "working") return mateLine(t);
  const files = (t.files || []).length;
  return [t.stat || (files ? plural(files, "file") : ""), t.ended_ms ? `${ago(t.ended_ms)} ago` : ""].filter(Boolean).map(esc).join(" · ");
}
/** A member's links: Review (its full review under its lane, or a review asked for once it's done), Diff, Talk (the
 *  chat's box with "@name " typed), and Stop (Resume first when stuck) at work, or Skip while its review waits. */
function crewLaneLinks(sid, t) {
  const k = mateKey(sid, t);
  const review = t.review ? `<button class="mt-lk ${mateUnder.get(k) === "review" ? "on" : ""}" data-mt-full="${esc(k)}">Review</button>`
    : ["done", "stopped"].includes(t.state) ? mateBtn(sid, t, "review", "Review", "link") : "";
  const live = ["working", "queued"].includes(t.state) ? mateOneAct(sid, t) : "";
  return review + mateBtn(sid, t, "diff", t.state === "working" ? "Diff so far" : "Diff", "link") + `<button class="mt-lk" data-cp-talk="${esc(t.name)}">Talk</button>` + live;
}
/** One member's lane: its name (its menu) and state; its steps, the one it's on now filled; where its work is; its
 *  links. What its menu opened (its full review, what it was asked) under it. A committed or set-aside one is dimmed. */
function crewLane(sid, t) {
  const [word, look] = crewStateWord(sid, t), past = ["committed", "aside"].includes(t.state);
  const steps = crewSteps(sid, t), on = !past && !["done", "stopped"].includes(t.state);
  const chips = steps.map((s, n) => `<span class="cp-step ${on && n === steps.length - 1 ? "now" : ""}">${crewCode(s)}</span>`).join(`<span class="cp-to" aria-hidden="true">→</span>`);
  return `<div class="cp-lane ${past ? "past" : ""}" data-key="cp:${esc(mateKey(sid, t))}"><div class="cp-who">${mateName(sid, t)}<span class="cp-st ${look}">${word}</span></div>`
    + `<div class="cp-steps">${chips}</div><div class="cp-where">${crewWhere(t)}</div><div class="cp-links">${crewLaneLinks(sid, t)}</div>${mateUnderHtml(sid, t)}</div>`;
}
/** The crew's line over its lanes: "The crew · 5", then "2 committed · 1 working · 1 needs you · 1h 20m since it
 *  started", and Handle it / Ask me (team_send.js crewMode, its menu opening downward here). */
function crewStatusLine(sid, needs) {
  const team = sessionOf(sid)?.team || [], count = (st) => team.filter((t) => t.state === st).length;
  const start = Math.min(...team.map((t) => t.started_ms || Infinity));
  const parts = [count("committed") ? `${count("committed")} committed` : "", count("working") ? `${count("working")} working` : "",
    needs ? `${needs} need${needs === 1 ? "s" : ""} you` : "", Number.isFinite(start) ? `${tookText(Math.max(0, now() - start))} since it started` : ""];
  return `<div class="cp-status"><span class="mt-grp">The crew · ${team.length}</span><span class="cp-count">${parts.filter(Boolean).map(esc).join(" · ")}</span><span class="grow"></span>${crewMode(sid)}</div>`;
}
/** The finished members (committed, set aside) at the end of the lanes: one line, "N done ›", folded until you click
 *  it (the drawer's fold, team.js mateDoneOpen, so it's open in both or neither); unfolded, their lanes, dimmed. */
function crewDoneLanes(sid) {
  const past = (sessionOf(sid)?.team || []).filter((t) => ["committed", "aside"].includes(t.state));
  if (!past.length) return "";
  const open = mateDoneOpen.has(sid);
  const line = `<button class="mt-lk mt-done-line" data-mt-done="${esc(sid)}" aria-expanded="${open}">${past.length} done ${open ? "⌄" : "›"}</button>`;
  return `<div class="cp-done ${open ? "open" : ""}" data-mt-for="done:${esc(sid)}">${line}${open ? past.map((t) => crewLane(sid, t)).join("") : ""}</div>`;
}
/** The leads whose goal is open in full (All of it ›): its four boxes in place of its line. */
const crewGoalOpen = new Set();
/** The goal on one line, its job first, cut at the end; All of it › opens the four boxes (team.js crewGoalParts). */
function crewGoalLine(sid, handles) {
  const open = crewGoalOpen.has(sid);
  const btn = `<button class="mt-lk cp-all" data-cp-goal="${esc(sid)}" aria-expanded="${open}">${open ? "Less ‹" : "All of it ›"}</button>`;
  if (open) return `<div class="cp-goal open"><span class="cp-lab">Goal</span>${crewGoalHtml(sid, handles)}${btn}</div>`;
  const words = crewGoalParts(sid, handles).map(([, short, text], n) => (n ? `${short}: ${crewCode(text)}.` : `<b>${crewCode(text.replace(/\.?$/, "."))}</b>`)).join(" ");
  return `<div class="cp-goal"><span class="cp-lab">Goal</span><span class="cp-gt">${words}</span>${btn}</div>`;
}

/** The lead's stream, oldest first: its replies and your messages from its chat (not Cue's own, nor the reviews it
 *  wrote for Cue, which the lanes have, nor other sessions'), each decision of the crew's log and the add-on's (`added`,
 *  crewPageDecided), in time order; then what you sent that hasn't gone in yet. */
function crewStream(sid, added) {
  const s = sessionOf(sid), who = esc(agentName(s.harness));
  // What the chat shows (chat/thread.js conversation): its earlier messages once loaded, and the reply it stopped on
  // when it waits on you (from its waiting card), else the lead's own ask.
  const own = state.items.filter((i) => i.session_id === sid && !mateOfAsk(i));
  const thread = conversation(own.find((i) => i.kind === "waiting") || own[0] || null, s);
  const { reviews } = reviewFolds(thread, s.team || []);
  const said = thread.filter((e, n) => e.text?.trim() && !reviews.has(n) && (e.role === "agent" || (e.role === "you" && !cueSaid(e.text)))).map((e) => ({
    at: e.at_ms || 0,
    html: e.role === "you"
      ? `<div class="cv-you" data-key="y:${e.at_ms}">${yourBubble(e, `${sid}:${e.at_ms}`)}<div class="cv-meta">You · ${ago(e.at_ms)} ago</div></div>`
      : `<div class="cv-agent" data-key="a:${e.at_ms}"><div class="cv-meta">${who} · ${ago(e.at_ms)} ago</div><div class="msg cv-text">${md(addOn.plain(stripStatus(e.text)))}</div></div>`,
  }));
  const decided = (s.crew_log || []).map((d) => ({ at: d.at_ms || 0, html: crewLogRow(sid, d, true) }));
  const theirs = added.items ? added.items.map((x) => ({ at: x.at_ms || 0, html: x.html })) : added.html ? [{ at: -Infinity, html: added.html }] : [];
  // Then, as under the chat, what waits to go: kept in Cue, sent at a time, queued in the lead.
  const later = [s.held ? heldRow(s) : "", s.send_at ? sendAtRow(s) : "", s.queued ? queuedRow(s) : ""].join("");
  return said.concat(decided, theirs).sort((a, b) => a.at - b.at).map((x) => x.html).join("") + later;
}
/** The stream's box: the lead's own ask's card when it asks you something (its teammates' asks are in Needs you), else
 *  the chat's box (the same draft, s:<sid>). */
function crewPageFoot(sid) {
  const s = sessionOf(sid), ask = state.items.find((i) => i.session_id === sid && i.kind !== "waiting" && !mateOfAsk(i));
  if (ask) return askFoot(ask, sid, s.harness);
  return `<div class="foot">${box_(`s:${sid}`, "Message the lead… or @name a teammate", s.state === "working" ? "Queue" : "Send", `data-act="send-to" data-sid="${esc(sid)}"`)}</div>`;
}

/** The page: its head (‹ Back to the board, the lead's name and project), the add-on's band, the goal line and the
 *  add-on's rows, the crew's line and lanes, then the lead's stream with its box beside Needs you (outlined red). */
function crewPageView(sid) {
  const s = sessionOf(sid), handles = crewHandles(sid), groups = crewGroups(sid), needs = crewNeedRows(sid, groups, true);
  // The add-on's part, each empty without it: its head line, its band, rows under the goal, rows in Needs you, decisions.
  const head = addOn.crewPageHead?.(sid) || "", band = addOn.crewPageBand?.(sid) || "", rowsAdded = addOn.crewPageRows?.(sid) || "";
  const needsAdded = addOn.crewPageNeeds?.(sid) || { html: "", count: 0 }, decidedAdded = addOn.crewPageDecided?.(sid) || { html: "", count: 0 };
  const needCount = needs.list.length + (needsAdded.count || 0), needRows = needs.rows + (needsAdded.count ? needsAdded.html : "");
  const decided = (s.crew_log || []).length + (decidedAdded.count || 0);
  // Still in the crew, in its order; committed and set aside folded at the end.
  const lanes = mateList(sid).map((t) => crewLane(sid, t)).join("") + crewDoneLanes(sid);
  const needBox = `<div class="cp-needs ${needCount ? "red" : ""}">${needCount ? `<div class="mt-grp red">Needs you · ${needCount}</div>${needRows}` : `<div class="mt-grp">Needs you · nothing now</div><div class="crew-none">${CREW_NONE}</div>`}</div>`;
  const items = crewStream(sid, decidedAdded);
  const stream = `<div class="cp-stream"><div class="cp-sh"><span class="mt-grp">The lead <span>what it said and decided, newest last · ${decided} decided</span></span><span class="grow"></span><button class="mt-lk" data-cp-chat>Open chat ›</button></div>
    <div class="cp-items" data-scroll-key="cps:${esc(sid)}"><div class="cp-items-in">${items || `<div class="crew-none">Nothing yet: what the lead says, and what it settles by itself, shows here.</div>`}</div></div>${crewPageFoot(sid)}</div>`;
  return `<div class="crew-page" data-crew-page="${esc(sid)}">
    <div class="cp-head"><button class="cp-back" data-cp-back>‹ Back to the board</button>${head || `<span class="cp-name">${esc(nameOf(sid, s.project))}</span><span class="grow"></span>${s.project && s.project !== nameOf(sid, s.project) ? `<span class="cp-proj">${esc(s.project)}</span>` : ""}`}</div>
    <div class="cp-body" data-scroll-key="cp:${esc(sid)}">${band}
      <div class="cp-top">${crewGoalLine(sid, handles)}${rowsAdded}</div>
      <div class="cp-crew">${crewStatusLine(sid, needCount)}<div class="cp-scroll" data-scroll-key="cpl:${esc(sid)}"><div class="cp-lanes">${lanes}</div></div></div>
      <div class="cp-split">${stream}${needBox}</div>
    </div>${mateMenuOf(sid)}</div>`;
}

/** Talk on a lane: the lead's box (the stream's) with "@name " typed (after what's there), the cursor at its end. */
function crewTalk(name) {
  const key = `s:${crewPageSid}`, d = draft(key), at = `@${name} `;
  if (!d.text.includes(at)) d.text = d.text.trim() ? `${d.text.replace(/\s*$/, " ")}${at}` : at;
  saveDrafts();
  renderMain();
  const el = document.querySelector(`.crew-page textarea[data-text="${CSS.escape(key)}"]`);
  if (el) { el.focus(); el.setSelectionRange(el.value.length, el.value.length); }
}

/** Esc on the page: the mode menu or a member's menu closes first, then the page; not while you type in a box that
 *  has text (Esc is the box's there) or something is open over the window. True when it took the key. */
function crewPageEsc(e) {
  if (e.key !== "Escape" || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey || e.isComposing) return false;
  if (!crewPageSid || sheet || lightbox || !document.querySelector(".crew-page")) return false;
  const f = document.activeElement;
  if (f?.matches?.("textarea, input") && f.value) return false;
  if (typeof crewMenuFor !== "undefined" && crewMenuFor) { crewMenuFor = null; renderMain(); }
  else if (mateMenu) { mateMenu = null; renderMain(); }
  else crewPageClose();
  return true;
}

if (typeof document !== "undefined") {
  document.addEventListener("keydown", (e) => { if (crewPageEsc(e)) { e.preventDefault(); e.stopImmediatePropagation(); } }, true);
  document.addEventListener("click", (e) => {
    const el = e.target.closest?.("[data-cp-open], [data-cp-back], [data-cp-chat], [data-cp-talk], [data-cp-goal]");
    if (!el) return;
    e.stopPropagation();
    if (el.dataset.cpOpen) return crewPageOpen(el.dataset.cpOpen);
    if (el.matches("[data-cp-back], [data-cp-chat]")) return crewPageClose();
    if (el.dataset.cpGoal) { const sid = el.dataset.cpGoal; if (!crewGoalOpen.delete(sid)) crewGoalOpen.add(sid); return renderMain(); }
    crewTalk(el.dataset.cpTalk);
  }, true);
}
