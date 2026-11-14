// ---------- a lead's crew on a page of its own (team.js has the drawer this grows from) ----------
// "Open full ›" on the crew's tray line opens it in the main area, where the board and the chat are, while the tabs and
// the top bar stay: ‹ Back to the chat (or Esc) goes back. Top to bottom: the crew's goal and one status line (its size,
// what's committed, at work, waiting on you, how long it has run, and Handle it / Ask me); one lane per member, what it
// did as small steps, where its work is and its links; what it decided on its own beside what needs you; and the
// lead's own message box (the chat's, its draft with it).

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
/** Back to the lead's chat, its box taking the cursor. */
function crewPageClose() {
  crewPageSid = null;
  mateMenu = null;
  renderMain();
  focusComposer();
}
/** The page drawn in place of the board, or null when it isn't open, or its lead is gone or no longer the open one:
 *  then it closes. Another tab shows that tab; back on Board, the page is still there. */
function crewPageShown() {
  if (!crewPageSid || view !== "board") return null;
  if (!sessionOf(crewPageSid) || (active?.sid && active.sid !== crewPageSid)) { crewPageSid = null; return null; }
  return crewPageView(crewPageSid);
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
 *  box below with "@name " typed), and Stop (Resume first when stuck) at work, or Skip while its review waits. */
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
/** Under the goal: "Crew of 5 · 2 committed · 1 working · 1 needs you · 1h 20m since it started", and Handle it / Ask
 *  me (team_send.js crewMode, its menu opening downward here). */
function crewStatusLine(sid, needs) {
  const team = sessionOf(sid)?.team || [], count = (st) => team.filter((t) => t.state === st).length;
  const start = Math.min(...team.map((t) => t.started_ms || Infinity));
  const parts = [`Crew of ${team.length}`, count("committed") ? `${count("committed")} committed` : "", count("working") ? `${count("working")} working` : "",
    needs ? `${needs} need${needs === 1 ? "s" : ""} you` : "", Number.isFinite(start) ? `${tookText(Math.max(0, now() - start))} since it started` : ""];
  return `<div class="cp-status"><span class="cp-count">${parts.filter(Boolean).map(esc).join(" · ")}</span><span class="grow"></span>${crewMode(sid)}</div>`;
}
/** The page: its head (‹ Back to the chat, the lead's name and project), then the goal, the status line, the lanes,
 *  Decided on its own beside Needs you (outlined red), and the lead's box. */
function crewPageView(sid) {
  const s = sessionOf(sid), team = s.team || [];
  const handles = crewHandles(sid), groups = crewGroups(sid), needs = crewNeedRows(sid, groups, true);
  // Still in the crew first, in its order; committed and set aside at the end.
  const lanes = [...mateList(sid), ...team.filter((t) => ["committed", "aside"].includes(t.state))].map((t) => crewLane(sid, t)).join("");
  const needBox = `<div class="cp-needs ${needs.rows ? "red" : ""}">${needs.rows ? `<div class="mt-grp red">Needs you · ${needs.list.length}</div>${needs.rows}` : `<div class="mt-grp">Needs you · nothing now</div><div class="crew-none">${CREW_NONE}</div>`}</div>`;
  const busy = s.state === "working";
  const box = box_(`s:${sid}`, "Tell the lead… or @name a teammate", busy ? "Queue" : "Send", `data-act="send-to" data-sid="${esc(sid)}"`);
  return `<div class="crew-page" data-crew-page="${esc(sid)}">
    <div class="cp-head"><button class="cp-back" data-cp-back>‹ Back to the chat</button><span class="cp-name">${esc(nameOf(sid, s.project))}</span><span class="grow"></span>${s.project && s.project !== nameOf(sid, s.project) ? `<span class="cp-proj">${esc(s.project)}</span>` : ""}</div>
    <div class="cp-scroll" data-scroll-key="cp:${esc(sid)}">
      ${crewGoalHtml(sid, handles)}${crewStatusLine(sid, needs.list.length)}
      <div class="mt-grp cp-crew-l">The crew · ${team.length} <span>what each did, where its work is</span></div>
      <div class="cp-lanes">${lanes}</div>
      <div class="cp-cols"><div class="cp-log">${crewLogHtml(sid, true)}</div>${needBox}</div>
    </div>
    <div class="foot cp-foot">${box}</div>${mateMenuOf(sid)}</div>`;
}

/** Talk on a lane: the lead's box below with "@name " typed (after what's there), the cursor at its end. */
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
    const el = e.target.closest?.("[data-cp-open], [data-cp-back], [data-cp-talk]");
    if (!el) return;
    e.stopPropagation();
    if (el.dataset.cpOpen) return crewPageOpen(el.dataset.cpOpen);
    if (el.matches("[data-cp-back]")) return crewPageClose();
    crewTalk(el.dataset.cpTalk);
  }, true);
}
