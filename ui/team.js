// ---------- a hosted lead's team: its crew as a tray on its message box (docs/dev/cue-teams.md §4) ----------
// Cue keeps the team from the lead's stream (hub/team.rs: state.sessions[].team); this draws it. On the lead's title
// line, a chip for each crew member that needs you. On the message box, the crew's tray: closed, one line ("● Cue Crew ·
// 6 · hub needs you · 2 ready · 2 working  Handle it ▾ ▴"); open, the same tray grows upward with the members grouped by
// what you'd do: Needs you (a member's ask, its Deny and Allow on its row), Ready (its review back, one button that
// agrees with its line), Working (one line each, a plain Stop), then the finished ones folded into "N done ›". A finished
// one is reviewed by itself (hub/team.rs review_on_finish). Cue clears members by itself (a committed or set-aside one
// leaves the team). A member's name opens its menu (what it was asked, its full review, its diff).

/** The leads whose tray is open, and per member what's open under its row ("asked", "review"); the member whose menu
 *  is open (by its name). */
const mateOpenLeads = new Set();
const mateUnder = new Map();     // "sid|agent" → "asked" | "review" (its full review)
let mateMenu = null;             // "sid|agent"
const mateNotes = new Map();     // "sid|agent" → why its last button didn't go through (Commit on main, say)
let mateShow = null;             // "sid|agent": what just opened under its row, scrolled into the tray after the next draw
/** The leads whose finished members are unfolded, one line each. */
const mateDoneOpen = new Set();

/** A lead's teammates still in its team (not committed or set aside). */
const mateList = (sid) => (sessionOf(sid)?.team || []).filter((t) => t.state !== "committed" && t.state !== "aside");
/** What a teammate asks you, if it does: the lead's pending ask from inside it. */
const mateAsk = (sid, t) => !!t.agent_id && (state.items || []).find((i) => i.session_id === sid && i.kind !== "waiting" && i.agent_id === t.agent_id) || null;
const mateKey = (sid, t) => `${sid}|${t.agent_id || t.call}`;

/** Needs you: it asks you something, or the hub says what it needs (`needs`: its lead didn't say it committed it). */
const mateNeeds = (sid, t) => !!mateAsk(sid, t) || !!t.needs;
/** The teammates of a lead that wait on you: ready (reviewed, its verdict waiting for your Commit or Send fixes), or
 *  asking or needing you. One whose files Cue never saw doesn't: Cue handles it (its review runs, then Handle it asks
 *  its lead to commit). */
const mateWaits = (sid) => mateList(sid).filter((t) => t.state === "reviewed" || mateNeeds(sid, t));
/** Per lead, what was already seen waiting on you ("sid|agent", and each ask by its id): a new one opens the tray
 *  once; closed after that, it stays closed until another starts waiting. */
// SHORTCUT: noticed while the tray is drawn (crewTray), not when the state arrives: fine while every draw of a lead's
// chat draws its tray. Upgrade path: call mateOpenOnWait for each lead from the "state" event handler.
const mateSeenWaiting = new Set();
function mateOpenOnWait(sid) {
  for (const t of mateWaits(sid)) {
    const ask = mateAsk(sid, t), k = ask ? `${mateKey(sid, t)}|${ask.id}` : mateKey(sid, t);
    if (mateSeenWaiting.has(k)) continue;
    mateSeenWaiting.add(k);
    mateOpenLeads.add(sid);
  }
}
/** The crew member's ask open in the pane (its card opened, or its chip on the title line): the tray opens for it,
 *  once per ask. */
let crewAskShown = null;
function crewOpenForAsk(it) {
  if (!it?.id || it.id === crewAskShown || !mateOfAsk(it)) return;
  crewAskShown = it.id;
  mateOpenLeads.add(it.session_id);
}
/** The line under the lead's chat while a teammate waits on you ("Waiting on you: docs is ready for review"),
 *  or while it waits on teammates at work ("Crew working: mac-loops"); "" when neither. */
function crewWaitLine(sid) {
  const wait = mateWaits(sid);
  if (wait.length) {
    const t = wait[0], more = wait.length > 1 ? ` and ${wait.length - 1} more` : "";
    return `<span class="cv-what">Waiting on you: <b>${esc(t.name)}</b> ${mateAsk(sid, t) ? "asks you something" : t.needs ? "needs you" : "is ready for review"}${more}</span>`;
  }
  const working = mateList(sid).filter((t) => t.state === "working");
  return working.length ? `<span class="dot-live"></span><span class="cv-what">Crew working: ${working.map((t) => esc(t.name)).join(", ")}</span>` : "";
}

/** How long: at work, since it started; finished, how long it took. */
function mateSpan(t) {
  const end = t.state === "working" || !t.ended_ms ? now() : t.ended_ms, s = Math.max(0, Math.round((end - t.started_ms) / 1000));
  return s < 60 ? `${s}s` : s < 3600 ? `${Math.floor(s / 60)}m` : `${Math.floor(s / 3600)}h`;
}

/** Minutes since a teammate's last line. */
const mateQuietMins = (t) => Math.max(1, Math.floor((now() - (t.step_ms || t.started_ms)) / 60000));
const plural = (n, word) => `${n} ${word}${n === 1 ? "" : "s"}`;

/** A working (or finished) member's line after its name: "9m · Editing app.js · 3 files so far", what the hub says it
 *  needs, or its summary. */
function mateLine(t) {
  const files = (t.files || []).length, parts = [mateSpan(t)];
  // Files known only from reading its shell commands (in the lead's folder) say so.
  const shell = (t.via_shell || []).length, via = shell ? ` (${shell === files ? (files === 1 ? "" : "all ") : `${shell} `}via shell)` : "";
  if (t.needs) parts.push(t.needs);
  // Quiet (src-tauri/src/sessions/quiet.rs): a long tool call left to it, or stuck (its lead was asked once).
  else if (t.state === "working" && t.quiet === "stuck") parts.push(`quiet for ${mateQuietMins(t)}m — looks stuck`);
  else if (t.state === "working" && t.quiet === "tool") parts.push(`${t.step || "Running a tool"} for ${mateQuietMins(t)}m`);
  else if (t.state === "working") { if (t.step) parts.push(t.step); if (files) parts.push(`${files} file${files === 1 ? "" : "s"} so far${via}`); }
  else if (t.state === "queued") parts.push("review queued");
  else if (t.state === "reviewing") parts.push("reviewing its diff");
  else if (t.state === "committed") parts.push(t.commit ? `committed ${t.commit.slice(0, 7)}` : "committed");
  else { if (t.summary) parts.push(t.summary); if (t.stat) parts.push(t.stat); else if (files) parts.push(`${files} file${files === 1 ? "" : "s"}${via}`); }
  if (mateUnknownFiles(t)) parts.push(UNKNOWN_FILES);
  return parts.map(esc).join(" · ");
}

/** Finished, and Cue never saw which files it changed (it wrote them with shell commands outside its lead's folder):
 *  nothing Cue can diff; its Commit asks its lead to commit them (hub/team.rs). */
const mateUnknownFiles = (t) => ["done", "reviewed"].includes(t.state) && !(t.files || []).length && !t.worktree && !t.stat;
const UNKNOWN_FILES = "Cue doesn't know which files it changed: Commit asks its lead to commit them";
/** One button of a member's: `look` "primary" (filled) or "" (outlined), both one fixed width so a group's line up, or
 *  "link" (a plain word, on one-line rows). Diff only for one with files Cue knows (or a worktree of its own). */
const mateBtn = (sid, t, act, label, look = "", held = false) => act === "diff" && !(t.files || []).length && !t.worktree ? "" :
  `<button class="${look === "link" ? "mt-lk" : `btn mt-b ${look}`} ${held ? "held" : ""}" data-mt-act="${act}" data-mt-sid="${esc(sid)}" data-mt-agent="${esc(t.agent_id || t.call)}" ${held ? `aria-disabled="true"` : ""}>${label}</button>`;
/** Commit for a member, held while another member has uncommitted changes in one of its files. */
const mateCommit = (sid, t, look = "primary") => mateBtn(sid, t, "commit", "Commit", look, (t.also || []).length > 0);

/** What a reviewed teammate's review recommends: "fixes", "commit", or "" when it said neither. */
// A commit that lists findings (notes too) calls for its fixes first: nothing it found is dropped.
const mateRec = (t) => { const k = verdictKind(t.recommendation); return k === "commit" && (t.findings || []).length ? "fixes" : k; };
/** A review's verdict as what it calls for: "fixes" (send fixes), "commit", or "" (neither). */
const verdictKind = (verdict) => /fix/.test(verdict || "") ? "fixes" : /^commit/.test(verdict || "") ? "commit" : "";
/** What a reviewed member's one button is: its review's call; said neither, its findings go back if it has any, else
 *  Commit. The other is in its menu. */
const mateReadyAct = (t) => mateRec(t) || ((t.findings || []).length ? "fixes" : "commit");
/** Fix rounds Cue sends back by itself while it handles reviews (hub/team.rs AUTO_FIX_ROUNDS). */
const AUTO_FIX_ROUNDS = 2;

/** A finding as a list item (its number is the list's): where, its words, its kind after them. */
function mateFinding(line) {
  const m = String(line).match(/^(?:\d+\.)?\s*(\S+:\d+\S*)?\s*(?:[—-]\s*)?(?:(blocker|must-fix|should-fix|nit|note|question)\s*[—-]\s*)?(.*)$/i);
  if (!m) return `<li>${esc(line)}</li>`;
  return `<li>${m[1] ? `<code>${esc(m[1])}</code> ` : ""}${esc(m[3])}${m[2] ? ` <em>${esc(m[2].toLowerCase().replace("-", " "))}</em>` : ""}</li>`;
}

/** What's under a row: why its last button didn't go through, then what you opened from its menu (its full review,
 *  or what it was asked). Commit refused on main (or a detached HEAD) offers Make a branch and commit. */
function mateUnderHtml(sid, t) {
  const k = mateKey(sid, t), note = mateNotes.get(k), offer = /Make a branch and commit/.test(note || "");
  const branch = offer ? `<div class="mt-note-acts">${mateBtn(sid, t, "branch", "Make a branch and commit", "primary")}</div>` : "";
  const open = mateUnder.get(k);
  const opened = open === "review" && t.review ? `<div class="mt-under mt-review msg" data-mt-for="${esc(k)}">${md(t.review)}</div>`
    : open === "asked" ? `<div class="mt-under mt-asked" data-mt-for="${esc(k)}">${esc(t.asked || t.what || "")}</div>` : "";
  return (note ? `<div class="mt-under mt-note ${offer ? "offer" : ""}" data-mt-for="${esc(k)}"><div>${esc(note)}</div>${branch}</div>` : "") + opened;
}

/** A member's name: it opens its menu. */
const mateName = (sid, t) => { const k = mateKey(sid, t); return `<button class="mt-name ${mateMenu === k ? "on" : ""}" data-mt-more="${esc(k)}" aria-expanded="${mateMenu === k}">${esc(t.name)}</button>`; };

/** Needs you: what it asks, with ↪, Deny and Allow on its row (a command, an edit), or the ask's own answers under it (a
 *  question, a form, a plan); else what the hub says it needs, and the one button its state has. */
function mateNeedRow(sid, t) {
  const ask = mateAsk(sid, t), k = mateKey(sid, t);
  if (!ask) return `<div class="mt-row" data-key="mt:${esc(k)}"><div class="mt-n">${mateName(sid, t)}<i>${esc(t.needs)}</i></div><div class="mt-btns">${mateOneAct(sid, t, "")}</div>${mateUnderHtml(sid, t)}</div>`;
  const id = esc(ask.id), yesNo = ask.kind === "permission" && !inTerminal(ask) && tool(ask) !== "exitplanmode";
  // ↪ tells it what to do instead, as on the ask's own card: its box then opens under the ask (decisionButtons).
  const redirecting = typeof redirectFor !== "undefined" && redirectFor === ask.id;
  const btns = yesNo ? `<button class="btn mt-redir" data-act="redirect" data-id="${id}" aria-label="Tell it what to do instead">↪</button><button class="btn mt-b deny" data-act="deny" data-id="${id}">Deny</button><button class="btn mt-b primary" data-act="allow" data-id="${id}">Allow</button>` : "";
  return `<div class="mt-row need" data-key="mt:${esc(k)}"><div class="mt-n">${mateName(sid, t)}<i>${esc(verb(ask))}</i></div><div class="mt-btns">${btns}</div><div class="mt-ask">${requestBody(ask)}${yesNo && !redirecting ? "" : decisionButtons(ask)}</div>${mateUnderHtml(sid, t)}</div>`;
}

/** Ready: its line says what its review found and what its one button does ("2 findings · fixes go back, round 1 of 2"
 *  → Send fixes; "clean review · commit it" → Commit; "commits after rows does" while rows holds it). Its findings as a
 *  plain numbered list under it; its files under a clean one. */
function mateReadyRow(sid, t) {
  const k = mateKey(sid, t), n = (t.findings || []).length, files = t.files || [];
  const after = (t.also || []).length ? `commits after ${t.also.join(", ")} does` : "commit it";
  let line, btn, under = "";
  if (mateReadyAct(t) === "fixes") {
    const round = (t.fix_rounds || 0) + 1;
    line = `${n ? plural(n, "finding") : "fixes asked"} · fixes go back, round ${round}${round <= AUTO_FIX_ROUNDS ? ` of ${AUTO_FIX_ROUNDS}` : ""}`;
    btn = mateBtn(sid, t, "fixes", "Send fixes", "primary");
    if (n) under = `<ol class="mt-fx">${t.findings.map(mateFinding).join("")}</ol>`;
  } else {
    line = `${verdictKind(t.recommendation) ? "clean review" : "no verdict, nothing found"} · ${after}`;
    btn = mateCommit(sid, t);
    under = files.length ? `<div class="mt-l">${esc(`${t.stat || plural(files.length, "file")} · ${files.slice(0, 3).join(", ")}${files.length > 3 ? ", …" : ""}`)}</div>`
      : mateUnknownFiles(t) ? `<div class="mt-l">${esc(UNKNOWN_FILES)}</div>` : "";
  }
  return `<div class="mt-row" data-key="mt:${esc(k)}"><div class="mt-n">${mateName(sid, t)}<i>${esc(line)}</i></div><div class="mt-btns">${btn}</div>${under}${mateUnderHtml(sid, t)}</div>`;
}

/** The one action a working or finished member has, as `look`: Stop at work (Resume first when it looks stuck), Diff
 *  while its review runs or once it's done, Commit when Cue never saw its files. */
function mateOneAct(sid, t, look = "link") {
  switch (t.state) {
    case "working": return (t.quiet === "stuck" ? mateBtn(sid, t, "resume", "Resume", look) : "") + mateBtn(sid, t, "stop", "Stop", look);
    case "stopped": return mateBtn(sid, t, "diff", "Diff", look);
    default: return mateUnknownFiles(t) ? mateCommit(sid, t, look) : mateBtn(sid, t, "diff", "Diff", look);
  }
}
/** Working (or its review running), or finished: one line, its name, how long and what it's on, one plain action. */
const mateOneRow = (sid, t) => `<div class="mt-one" data-key="mt:${esc(mateKey(sid, t))}">${mateName(sid, t)}<span class="mt-line">${mateLine(t)}</span>${mateOneAct(sid, t)}</div>${mateUnderHtml(sid, t)}`;

/** Which group a member's row is in: "need", "ready" (its review back, or done with files Cue never saw while you
 *  handle reviews, so only your Commit settles it), "working" (at work, or its review is), "done". */
function mateGroup(sid, t) {
  if (mateNeeds(sid, t)) return "need";
  if (t.state === "reviewed") return "ready";
  if (["working", "queued", "reviewing"].includes(t.state)) return "working";
  return "done";
}
/** Committed members still show among the finished ones for this long after they finished; older ones leave the tray
 *  (the team's list keeps every one, so "N done" would only grow). */
const DONE_SHOWN_MS = 3600000;
/** The finished members (done, stopped, committed in the last hour) out of the way: one line at the bottom, "N done ›", folded until
 *  you click it; unfolded, one line each with its Diff. */
function mateDoneHtml(sid, done) {
  const all = done.concat((sessionOf(sid)?.team || []).filter((t) => t.state === "committed" && now() - (t.ended_ms || 0) < DONE_SHOWN_MS));
  if (!all.length) return "";
  const open = mateDoneOpen.has(sid);
  const line = `<button class="mt-lk mt-done-line" data-mt-done="${esc(sid)}" aria-expanded="${open}">${all.length} done ${open ? "⌄" : "›"}</button>`;
  return `<div class="mt-done ${open ? "open" : ""}" data-mt-for="done:${esc(sid)}">${line}${open ? all.map((t) => mateOneRow(sid, t)).join("") : ""}</div>`;
}

/** The menu its name opens: its steps, the button its review didn't pick, Review, Diff when its row doesn't show it,
 *  its full review, what it was asked. */
function mateMenuHtml(sid, t) {
  const a = esc(t.agent_id || t.call), s = esc(sid), k = esc(mateKey(sid, t));
  const steps = t.agent_id ? `<button data-helper="${esc(`${sid}|${t.agent_id}`)}">Its steps</button>` : "";
  const review = t.state === "done" || t.state === "stopped" ? `<button data-mt-act="review" data-mt-sid="${s}" data-mt-agent="${a}">Review</button>` : "";
  // Reviewed: the button its row doesn't show.
  const act = t.state === "reviewed" && !mateNeeds(sid, t) ? mateReadyAct(t) : "";
  const other = act === "fixes" ? `<button data-mt-act="commit" data-mt-sid="${s}" data-mt-agent="${a}" ${(t.also || []).length ? `aria-disabled="true"` : ""}>Commit anyway</button>` : act === "commit" ? `<button data-mt-act="fixes" data-mt-sid="${s}" data-mt-agent="${a}">Send fixes</button>` : "";
  const onRow = ["queued", "reviewing", "done", "stopped"].includes(t.state) && !mateNeeds(sid, t);
  const diff = onRow || (!(t.files || []).length && !t.worktree) ? "" : `<button data-mt-act="diff" data-mt-sid="${s}" data-mt-agent="${a}">Diff</button>`;
  return `<div class="more-menu rv-menu mt-menu" data-mt-menu="${k}">${steps}${other}${review}${diff}${t.review ? `<button data-mt-full="${k}">Full review</button>` : ""}<button data-mt-asked="${k}">What it was asked</button></div>`;
}

/** The tray's line: "● Cue Crew · 6 · hub needs you · 2 ready · 2 working", the dot live while any member works. */
function crewWords(sid, groups) {
  const need = groups.need, n = need.length;
  const needs = n === 1 ? `${esc(need[0].name)} needs you` : n ? `${n} need you` : "";
  const rest = [groups.ready.length ? `${groups.ready.length} ready` : "", groups.working.length ? `${groups.working.length} working` : ""].filter(Boolean);
  const live = mateList(sid).some((t) => t.state === "working");
  return `<span class="${live ? "dot-live" : "crew-dot"}"></span><span class="crew-w">Cue Crew · ${mateList(sid).length}${needs ? ` <span class="crew-need">· ${needs}</span>` : ""}${rest.length ? ` <span class="crew-mu">· ${rest.join(" · ")}</span>` : ""}</span>`;
}

/** The crew's tray on the lead's message box (the box's top corners go square under it): its line (a click on it, or
 *  its caret, opens and closes it; Esc closes it), with the mode ("Handle it ▾", team_send.js crewMode); open, the
 *  groups in a white body that scrolls inside past about half the pane. "" when it has no crew. */
function crewTray(sid) {
  const list = mateList(sid);
  if (!list.length) return "";
  mateOpenOnWait(sid);
  const open = mateOpenLeads.has(sid);
  const groups = { need: [], ready: [], working: [], done: [] };
  for (const t of list) groups[mateGroup(sid, t)].push(t);
  const head = `<div class="crew-head" data-mt-open="${esc(sid)}" role="button" aria-expanded="${open}">${crewWords(sid, groups)}<span class="grow"></span>${crewMode(sid)}<button class="crew-car" data-mt-open="${esc(sid)}" aria-label="${open ? "Close the crew" : "Open the crew"}">${open ? "▾" : "▴"}</button></div>`;
  if (!open) return `<div class="crew-tray" data-key="crew-tray" data-crew-tray="${esc(sid)}">${head}</div>`;
  const grp = (label, rows, cls = "") => rows ? `<div class="mt-grp ${cls}">${label}</div>${rows}` : "";
  const body = grp("Needs you", groups.need.map((t) => mateNeedRow(sid, t)).join(""), "red")
    + grp("Ready", groups.ready.map((t) => mateReadyRow(sid, t)).join(""))
    + grp("Working", groups.working.map((t) => mateOneRow(sid, t)).join(""))
    + mateDoneHtml(sid, groups.done);
  const menu = (sessionOf(sid)?.team || []).find((t) => mateKey(sid, t) === mateMenu);
  return `<div class="crew-tray open" data-key="crew-tray" data-crew-tray="${esc(sid)}">${head}<div class="crew-body" data-scroll-key="crew:${esc(sid)}">${body}</div>${menu ? mateMenuHtml(sid, menu) : ""}</div>`;
}

/** After each draw: the open tray's body at most about half its pane (it scrolls inside past that), the open menu under
 *  the name that opened it, and what just opened under a row scrolled into the body, its row with it. */
function placeTeamRows() {
  for (const tray of document.querySelectorAll(".crew-tray.open")) {
    const pane = tray.closest(".active-pane"), body = tray.querySelector(".crew-body");
    if (pane && body) body.style.maxHeight = `${Math.max(120, Math.round(pane.clientHeight / 2) - tray.querySelector(".crew-head").offsetHeight)}px`;
    const menu = tray.querySelector(".mt-menu"), name = menu && [...tray.querySelectorAll(".crew-body [data-mt-more]")].find((b) => b.dataset.mtMore === menu.dataset.mtMenu);
    if (menu && name) {
      const t = tray.getBoundingClientRect(), r = name.getBoundingClientRect();
      menu.style.left = `${Math.round(r.left - t.left)}px`;
      menu.style.top = `${Math.round(r.bottom - t.top + 4)}px`;
    }
  }
  const show = mateShow && [...document.querySelectorAll(".crew-body [data-mt-for]")].filter((e) => e.dataset.mtFor === mateShow);
  mateShow = null;
  if (!show?.length) return;
  const box = show[0].closest(".crew-body"), row = show[0].closest(".mt-row") || (show[0].matches(".mt-under") && show[0].previousElementSibling) || show[0], last = show[show.length - 1];
  const top = row.offsetTop - box.offsetTop, bottom = last.offsetTop - box.offsetTop + last.offsetHeight;
  if (bottom > box.scrollTop + box.clientHeight) box.scrollTop = Math.min(top, bottom - box.clientHeight);
}
if (typeof window !== "undefined") window.addEventListener("resize", () => { if (typeof document !== "undefined") placeTeamRows(); });

/** On the lead's title line: a chip for each crew member that needs you ("hub · needs you"); "" when none does (the
 *  crew itself is in its tray on the box, crewTray). */
function mateChips(sid) {
  return mateList(sid).map((t) => [t, mateAsk(sid, t)]).filter(([, a]) => a).map(([t, a]) => `<button class="tm-chip need" data-big="${esc(a.id)}"><span class="tm-n">${esc(t.name)}</span> <span class="tm-sub">· needs you</span></button>`).join("");
}

// ---------- Cue's own words to a lead, and its crew's reviews, in the chat ----------
// What Cue sends a lead for its crew (hub/team.rs, team_out.rs) is drawn as Cue's bubble on the right, never as yours.
// The lead's reply to a review request isn't a message of its own: it folds into that request's bubble ("+ docs
// reviewed: commit · 1 note" unfolds it); its findings and buttons are on the member's row in the tray.

/** What Cue sends a lead, by how it starts: its few words for it (plain), from the crew member's name and,
 *  for a task handed out, the task's first line. The cases both windows are held to: tests/fixtures/crew-review.json. */
const CUE_SAID = [
  [/^Review your teammate (.+?)['’]s work with a fork/, (n) => `Cue asked for ${n}'s review`, "review"],
  [/^Send your teammate (.+?) its review['’]s findings with SendMessage/, (n) => `Cue sent ${n}'s findings back to it`],
  [/^Your teammate (.+?) \(carried over from the session before you\) can['’]t be messaged/, (n) => `Cue asked to start ${n} again with its fixes`],
  [/^Stop your teammate (.+?) now/, (n) => `Cue asked to stop ${n}`],
  [/^Commit your teammate (.+?)['’]s work now/, (n) => `Cue asked to commit ${n}'s work`],
  [/^(?:\(Cue\) )?Your teammate (.+?) has (?:gone quiet\.|been quiet for)/, (n) => `Cue asked ${n} where it is`],
  [/^Hand this to a new teammate:/, (n, text) => { const task = String(text).split(/\n\s*\n/)[1]?.trim().split("\n")[0] || ""; return `Cue handed out a task${task ? ` · ${task}` : ""}`; }],
];
/** A message Cue sent the lead for its crew: { words (plain), html (the name in bold), name, review }; null for
 *  anything else (your own words stay yours). */
function cueSaid(text) {
  for (const [re, words, kind] of CUE_SAID) {
    const m = re.exec(String(text || "").trim());
    if (!m) continue;
    const name = m[1] || "", plain = words(name, text), i = name ? plain.indexOf(name) : -1;
    const html = i < 0 ? esc(plain) : `${esc(plain.slice(0, i))}<b>${esc(name)}</b>${esc(plain.slice(i + name.length))}`;
    return { words: plain, html, name, review: kind === "review" };
  }
  return null;
}
/** Cue's words for its own bubble, where "Cue" is already its name: "Asked for <b>docs</b>'s review". */
const cueWords = (html) => html.replace(/^Cue (.)/, (_, c) => c.toUpperCase());
/** Cue's message as a bubble on the right, where yours are, named Cue: its few words, and "+ What Cue sent" unfolds
 *  the exact text (addedNote, opened by `key`). `meta`: what follows "Cue ·" (when it went, or that it waits).
 *  `review`: for a review request, the lead's reply that is the review ({ name, text, team }), folded under its own
 *  "+ <its one line>". */
function cueSaidLine(said, key, text, meta, review = null) {
  const rv = review ? addedNote(`${key}:review`, esc(reviewLineParts(review.team, review.name, review.text).words), review.text) : "";
  return `<div class="cv-you cue-you"><div class="cv-you-text">${cueWords(said.html)}${text ? addedNote(key, "What Cue sent", text) : ""}${rv}</div><div class="cv-meta">Cue · ${meta}</div></div>`;
}
/** A review's Recommendation line: it starts the line ("**Recommendation**:" and "**Recommendation:**" both), and what
 *  follows it up to . ( , ; or * is the verdict. */
const RECOMMENDATION = /^\**recommendation\**\s*:\s*\**\s*([^.(,;*]*)/i;
const hasRecommendation = (text) => String(text || "").split("\n").some((l) => RECOMMENDATION.test(l.trim()));
/** A review's verdict and counts read off its words ("send fixes", "1 should-fix · 2 notes"): should-fix, must-fix and
 *  blocker count as should-fix; note, nit, question (or no kind) as notes; a zero group is left out. */
function reviewSays(text) {
  const lines = String(text || "").split("\n").map((l) => l.trim());
  const rec = lines.map((l) => RECOMMENDATION.exec(l)).find(Boolean);
  const kinds = lines.filter((l) => /^\d+\.\s/.test(l)).map((l) => (/ — (blocker|must-fix|should-fix|nit|note|question) — /i.exec(l) || [])[1]?.toLowerCase() || "note");
  const fix = kinds.filter((k) => ["should-fix", "must-fix", "blocker"].includes(k)).length, notes = kinds.length - fix;
  const counts = [fix ? `${fix} should-fix` : "", notes ? `${notes} note${notes === 1 ? "" : "s"}` : ""].filter(Boolean).join(" · ");
  return { verdict: rec ? rec[1].trim().toLowerCase() : "", counts };
}
/** Which of a chat's replies are a crew member's review (index → its name): after Cue's review request for X, up to
 *  your own next message or Cue's next review request (Cue's other messages don't end it), the reply that is X's saved review word for word, else the last one with a Recommendation
 *  line. `review(name)`: that member's saved review, "" when none. */
function reviewReplies(ex, review = () => "") {
  const out = new Map();
  ex.forEach((e, n) => {
    const asked = e.role === "you" && cueSaid(e.text);
    if (!asked?.review) return;
    const span = [];
    const ends = (y) => { if (y.role !== "you") return false; const c = cueSaid(y.text); return !c || c.review; };
    for (let k = n + 1; k < ex.length && !ends(ex[k]); k++) if (ex[k].role === "agent") span.push(k);
    const saved = review(asked.name).trim();
    const hit = (saved ? span.find((k) => ex[k].text.trim() === saved) : undefined) ?? span.filter((k) => hasRecommendation(ex[k].text)).at(-1);
    if (hit != null) out.set(hit, asked.name);
  });
  return out;
}
/** A review's one line, in parts (tests/fixtures/crew-review.json "lines", shared with another window that draws it
 *  with a button): its plain words (here, the label its fold has in Cue's bubble), the button it calls for ("fixes",
 *  "commit" or "": only on that member's current review — the reply is its saved review — while it's reviewed),
 *  whether it can open the member's row (while it's still in the crew), and the member (the latest of that name). */
function reviewLineParts(team, name, text) {
  const t = [...(team || [])].reverse().find((m) => m.name === name) || null;
  const { verdict, counts } = reviewSays(text);
  const current = !!t && t.state === "reviewed" && (t.review || "").trim() === String(text).trim();
  // Send fixes for a verdict with "fix" in it, Commit for one that starts with "commit", nothing on a guess.
  // A commit that lists anything (notes too) calls for its fixes first, as the row does (mateRec).
  const kind = verdictKind(verdict), button = !current ? "" : kind === "commit" && counts ? "fixes" : kind;
  const suffix = !t ? "not in the crew now" : t.state === "committed" ? "committed" : t.state === "aside" ? "set aside" : "";
  const words = [`${name} reviewed: ${verdict || "no verdict"}`, counts, suffix].filter(Boolean).join(" · ");
  return { words, verdict, counts, suffix, button, open: !!t && !suffix, member: t };
}

/** The teammate an ask comes from, by name, when its session's team has it: "" otherwise. */
const mateOfAsk = (it) => it?.agent_id ? (sessionOf(it.session_id)?.team || []).find((t) => t.agent_id === it.agent_id)?.name || "" : "";
/** After the lead's name on an ask's card and in Waiting: "› <teammate>" (the space before it is the caller's: a
 *  card's top line spaces its parts itself). */
const mateTitle = (it) => { const n = mateOfAsk(it); return n ? `<span class="mt-of">› <b>${esc(n)}</b></span>` : ""; };

/** A teammate's button: Review, Diff (the session's Diff tab on its files), Commit, Send fixes, Stop, Resume. */
async function mateAct(sid, agent, action) {
  const k = `${sid}|${agent}`;
  // Diff: the crew folds away for its files in the session's Diff tab (diff.js).
  if (action === "diff") { mateOpenLeads.delete(sid); return diffShowMember(sid, agent, (sessionOf(sid)?.team || []).find((t) => (t.agent_id || t.call) === agent)?.name || ""); }
  // Refused (Commit on main, say): the reason stays under its row until a button of its goes through.
  try { toast(await invoke("team_action", { sessionId: sid, agent, action })); mateNotes.delete(k); }
  catch (e) { mateNotes.set(k, String(e)); mateShow = k; }
  renderMain();
}

/** Esc on the open session's crew tray: its mode menu or a member's menu closes first, then the tray; not while you
 *  type in a box that has text (Esc is the box's there) or something is open over the window. True when it took the key. */
function crewEsc(e) {
  if (e.key !== "Escape" || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey || e.isComposing) return false;
  const sid = active?.sid;
  if (!sid || !mateOpenLeads.has(sid) || sheet || lightbox) return false;
  if (!document.querySelector(`.crew-tray.open[data-crew-tray="${CSS.escape(sid)}"]`)) return false;
  const f = document.activeElement;
  if (f?.matches?.("textarea, input") && f.value) return false;
  if (typeof crewMenuFor !== "undefined" && crewMenuFor) crewMenuFor = null;
  else if (mateMenu) mateMenu = null;
  else mateOpenLeads.delete(sid);
  renderMain();
  return true;
}

if (typeof document !== "undefined") {
  document.addEventListener("keydown", (e) => { if (crewEsc(e)) { e.preventDefault(); e.stopImmediatePropagation(); } }, true);
  document.addEventListener("click", (e) => {
    const t = e.target;
    // The mode on the tray's line is team_send.js's: it doesn't open or close the tray.
    if (t.closest?.(".crew-pick")) return;
    // A member's menu closes on any click but its name (one of its items runs as it closes).
    const closeMenu = mateMenu && !t.closest?.("[data-mt-more]");
    const el = t.closest?.("[data-mt-open], [data-mt-act], [data-mt-more], [data-mt-done], [data-mt-asked], [data-mt-full]");
    if (closeMenu) mateMenu = null;
    if (!el) { if (closeMenu) renderMain(); return; }
    e.stopPropagation();
    const d = el.dataset;
    if (d.mtOpen) { mateOpenLeads.has(d.mtOpen) ? mateOpenLeads.delete(d.mtOpen) : mateOpenLeads.add(d.mtOpen); return renderMain(); }
    if (d.mtMore) { mateMenu = closeMenu || mateMenu === d.mtMore ? null : d.mtMore; return renderMain(); }
    // Unfolded, the finished ones are scrolled into the tray's body.
    if (d.mtDone) { if (mateDoneOpen.has(d.mtDone)) mateDoneOpen.delete(d.mtDone); else { mateDoneOpen.add(d.mtDone); mateShow = `done:${d.mtDone}`; } return renderMain(); }
    const fold = d.mtAsked || d.mtFull, what = d.mtFull ? "review" : "asked";
    if (fold) { if (mateUnder.get(fold) === what) mateUnder.delete(fold); else { mateUnder.set(fold, what); mateShow = fold; } return renderMain(); }
    if (el.getAttribute("aria-disabled") === "true") return toast("Commit is held: another crew member has uncommitted changes in one of its files");
    renderMain();
    mateAct(d.mtSid, d.mtAgent, d.mtAct);
  }, true);
}
