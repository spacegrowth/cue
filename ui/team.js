// ---------- a hosted lead's team: its teammates as rows in its chat (docs/dev/cue-teams.md §4) ----------
// Cue keeps the team from the lead's stream (hub/team.rs: state.sessions[].team); this draws it. On the lead's title
// line, a chip for each crew member that needs you; inside the message box, top right, "Crew 3 · 1 ready ▴", which
// opens the crew above the box: one row per crew member (its mark, its name, its state, how long, its newest step or
// its summary, the buttons its state needs). A finished one is reviewed by itself (hub/team.rs review_on_finish).
// A committed or set-aside teammate leaves the team. Asks from a teammate are titled "<lead> › <teammate>".

/** The leads whose team is open under their title, and per teammate what's open under its row ("findings", "diff",
 *  "asked"); the teammate whose ⋯ menu is open; the diffs read for it. */
const mateOpenLeads = new Set();
const mateUnder = new Map();     // "sid|agent" → "findings" | "diff" | "asked" | "review" (its full review)
let mateMenu = null;             // "sid|agent"
const mateDiffs = new Map();     // "sid|agent" → the diff's text, or null while it's read
const mateNotes = new Map();     // "sid|agent" → why its last button didn't go through (Commit on main, say)
let mateShow = null;             // "sid|agent": what just opened under its row, scrolled into the box after the next draw

/** A lead's teammates still in its team (not committed or set aside). */
const mateList = (sid) => (sessionOf(sid)?.team || []).filter((t) => t.state !== "committed" && t.state !== "aside");
/** What a teammate asks you, if it does: the lead's pending ask from inside it. */
const mateAsk = (sid, t) => !!t.agent_id && (state.items || []).find((i) => i.session_id === sid && i.kind !== "waiting" && i.agent_id === t.agent_id) || null;
const mateKey = (sid, t) => `${sid}|${t.agent_id || t.call}`;

/** The teammates of a lead that wait on you: ready (reviewed, its verdict waiting for your Commit or Send fixes) or
 *  asking you something. */
const mateWaits = (sid) => mateList(sid).filter((t) => t.state === "reviewed" || mateAsk(sid, t));
/** Per lead, the teammates already seen waiting on you ("sid|agent"): a new one opens the teammates over the box once;
 *  closed after that, they stay closed until another starts waiting. */
// SHORTCUT: noticed while the tab or the rows are drawn (crewStrip, crewDock), not when the state arrives: fine while
// every draw of a lead's chat draws its tab; if one ever doesn't, it opens on the next draw that does. Upgrade path:
// call mateOpenOnWait for each lead from the "state" event handler and drop it from the two draws.
const mateSeenWaiting = new Set();
function mateOpenOnWait(sid) {
  for (const t of mateWaits(sid)) {
    const k = mateKey(sid, t);
    if (mateSeenWaiting.has(k)) continue;
    mateSeenWaiting.add(k);
    mateOpenLeads.add(sid);
  }
}
/** The line under the lead's chat while a teammate waits on you ("Waiting on you: docs is ready for review"),
 *  or while it waits on teammates at work ("Teammates working: mac-loops"); "" when neither. */
function crewWaitLine(sid) {
  const wait = mateWaits(sid);
  if (wait.length) {
    const t = wait[0], more = wait.length > 1 ? ` and ${wait.length - 1} more` : "";
    return `<span class="cv-what">Waiting on you: <b>${esc(t.name)}</b> ${mateAsk(sid, t) ? "asks you something" : "is ready for review"}${more}</span>`;
  }
  const working = mateList(sid).filter((t) => t.state === "working");
  return working.length ? `<span class="dot-live"></span><span class="cv-what">Teammates working: ${working.map((t) => esc(t.name)).join(", ")}</span>` : "";
}

/** Its state's word after its name. */
function mateWord(t, ask) {
  if (ask) return `<span class="tm-chip need small">needs you</span>`;
  const n = (t.findings || []).length;
  switch (t.state) {
    case "working": return `<span class="mt-st">working</span>`;
    case "queued": return `<span class="mt-st">review queued</span>`;
    case "reviewing": return `<span class="mt-st">reviewing…</span>`;
    case "reviewed": return `<span class="mt-st b">reviewed${n ? ` · ${n} finding${n === 1 ? "" : "s"}` : ""}</span>`;
    case "stopped": return `<span class="mt-st b">stopped</span>`;
    default: return `<span class="mt-st b">done</span>`;
  }
}

/** How long: at work, since it started; finished, how long it took. */
function mateSpan(t) {
  const end = t.state === "working" || !t.ended_ms ? now() : t.ended_ms, s = Math.max(0, Math.round((end - t.started_ms) / 1000));
  return s < 60 ? `${s}s` : s < 3600 ? `${Math.floor(s / 60)}m` : `${Math.floor(s / 3600)}h`;
}

/** Minutes since a teammate's last line. */
const mateQuietMins = (t) => Math.max(1, Math.floor((now() - (t.step_ms || t.started_ms)) / 60000));

/** The line under its name: "9m · Editing app.js · 3 files so far", or what it asks, or its summary. Nothing says
 *  "teammate": its place under its lead does. */
function mateLine(t, ask) {
  const files = (t.files || []).length, parts = [mateSpan(t)];
  // Files known only from reading its shell commands (in the lead's folder) say so.
  const shell = (t.via_shell || []).length, via = shell ? ` (${shell === files ? (files === 1 ? "" : "all ") : `${shell} `}via shell)` : "";
  if (ask) parts.push(`${verb(ask)}: ${plain(summary(ask))}`);
  // Quiet (src-tauri/src/sessions/quiet.rs): a long tool call left to it, or stuck (its lead was asked once).
  else if (t.state === "working" && t.quiet === "stuck") parts.push(`quiet for ${mateQuietMins(t)}m — looks stuck`);
  else if (t.state === "working" && t.quiet === "tool") parts.push(`${t.step || "Running a tool"} for ${mateQuietMins(t)}m`);
  else if (t.state === "working") { if (t.step) parts.push(t.step); if (files) parts.push(`${files} file${files === 1 ? "" : "s"} so far${via}`); }
  else if (t.state === "reviewing") parts.push("a fork reads its files' diff against what it was asked");
  else if (t.state === "reviewed") { if (t.recommendation) parts.push(`recommendation: ${t.recommendation}`); if (t.stat) parts.push(t.stat); }
  else { if (t.summary) parts.push(t.summary); if (t.stat) parts.push(t.stat); else if (files) parts.push(`${files} file${files === 1 ? "" : "s"}${via}`); }
  return parts.map(esc).join(" · ");
}

/** One button of a teammate's. */
const mateBtn = (sid, t, act, label, primary = false, held = false) =>
  `<button class="btn ${primary ? "primary" : ""} ${held ? "held" : ""}" data-mt-act="${act}" data-mt-sid="${esc(sid)}" data-mt-agent="${esc(t.agent_id || t.call)}" ${held ? `aria-disabled="true"` : ""}>${label}</button>`;
/** The buttons its state needs, in order (⋯ holds the rest). */
function mateActs(sid, t, ask) {
  if (ask) return "";
  switch (t.state) {
    case "working": return t.quiet === "stuck" ? mateBtn(sid, t, "resume", "Resume", true) + mateBtn(sid, t, "stop", "Stop") : mateBtn(sid, t, "stop", "Stop");
    case "queued": case "reviewing": return mateBtn(sid, t, "diff", "Diff");
    // Reviewed: the one its review recommends (the other is in ⋯); both when it said neither.
    case "reviewed": {
      const commit = mateBtn(sid, t, "commit", "Commit", true, (t.also || []).length > 0);
      const rec = mateRec(t);
      return rec === "fixes" ? mateBtn(sid, t, "fixes", "Send fixes", true) : rec === "commit" ? commit : commit + mateBtn(sid, t, "fixes", "Send fixes");
    }
    // Stopped (by you, or with its lead's interrupt): what it changed, if anything; nothing to review in nothing.
    case "stopped": return (t.files || []).length ? mateBtn(sid, t, "diff", "Diff") : mateBtn(sid, t, "aside", "Set aside");
    // Finished: its review is queued by itself (a loop's has Review in ⋯).
    default: return mateBtn(sid, t, "diff", "Diff");
  }
}

/** A finding as a line: its number and words, the kind it is muted after. */
function mateFinding(line) {
  const m = String(line).match(/^(\d+\.)\s+(\S+:\d+\S*)?\s*(?:[—-]\s*)?(?:(blocker|must-fix|should-fix|nit|note|question)\s*[—-]\s*)?(.*)$/i);
  if (!m) return `<div>${esc(line)}</div>`;
  return `<div>${m[1]} ${m[2] ? `<span class="mono">${esc(m[2])}</span> · ` : ""}${esc(m[4])}${m[3] ? ` <span class="rv-k">${esc(m[3].toLowerCase())}</span>` : ""}</div>`;
}

/** What's under its row: why its last button didn't go through, then what you opened (its review's findings, its
 *  diff, or what it was asked). Commit refused on main (or a detached HEAD) offers Make a branch and commit. */
function mateUnderHtml(sid, t) {
  const k = mateKey(sid, t), note = mateNotes.get(k), offer = /Make a branch and commit/.test(note || "");
  const branch = offer ? `<div class="mt-note-acts">${mateBtn(sid, t, "branch", "Make a branch and commit", true)}</div>` : "";
  return (note ? `<div class="mt-under mt-note ${offer ? "offer" : ""}" data-mt-for="${esc(k)}"><div>${esc(note)}</div>${branch}</div>` : "") + mateOpenHtml(sid, t);
}
/** What you opened under its row. */
function mateOpenHtml(sid, t) {
  const k = mateKey(sid, t), open = mateUnder.get(k);
  if (open === "findings" && (t.findings || []).length) return `<div class="mt-under" data-mt-for="${esc(k)}">${t.findings.map(mateFinding).join("")}</div>`;
  if (open === "review" && t.review) return `<div class="mt-under" data-mt-for="${esc(k)}"><div class="msg mt-review">${md(t.review)}</div></div>`;
  if (open === "asked") return `<div class="mt-under" data-mt-for="${esc(k)}"><div class="mt-asked">${esc(t.asked || t.what || "")}</div></div>`;
  if (open === "diff") {
    const d = mateDiffs.get(k);
    return `<div class="mt-under" data-mt-for="${esc(k)}"><pre class="mt-diff">${d == null ? "Reading its diff…" : esc(d || "No changes.")}</pre></div>`;
  }
  return "";
}

/** One teammate's row: its mark (a person in a ring), its name (opens its steps), its state, its line, its buttons. */
function mateRow(sid, t) {
  const ask = mateAsk(sid, t), k = mateKey(sid, t);
  const also = (t.also || []).length ? `<span class="mt-also">also changed by ${esc(t.also.join(", "))}</span>` : "";
  const fold = t.state === "reviewed" && (t.findings || []).length;
  const word = fold ? `<button class="mt-fold" data-mt-fold="${esc(k)}">${mateWord(t, ask)}<span class="rv-chev">${mateUnder.get(k) === "findings" ? "⌄" : "›"}</span></button>` : mateWord(t, ask);
  const name = t.agent_id ? `<button class="mt-name" data-helper="${esc(`${sid}|${t.agent_id}`)}">${esc(t.name)}</button>` : `<b class="mt-name">${esc(t.name)}</b>`;
  const click = ask ? ` data-big="${esc(ask.id)}" role="button"` : "";
  return `<div class="mt-row ${ask ? "need" : ""}" data-key="mt:${esc(k)}"${click}><i class="stx-ic k-team">${PERSON_ICON}</i><div class="mt-mid"><div class="mt-top">${name}${word}${also}</div><div class="mt-line">${mateLine(t, ask)}</div></div>${mateActs(sid, t, ask)}<button class="rv-more ${mateMenu === k ? "on" : ""}" data-mt-more="${esc(k)}" aria-label="More">⋯</button></div>${mateUnderHtml(sid, t)}`;
}

/** The ⋯ menu: Diff when the row doesn't show it, what it was asked, Set aside. */
/** What a reviewed teammate's review recommends: "fixes", "commit", or "" when it said neither. */
// A commit that lists findings (notes too) calls for its fixes first: nothing it found is dropped.
const mateRec = (t) => { const k = verdictKind(t.recommendation); return k === "commit" && (t.findings || []).length ? "fixes" : k; };
/** A review's verdict as what it calls for: "fixes" (send fixes), "commit", or "" (neither). */
const verdictKind = (verdict) => /fix/.test(verdict || "") ? "fixes" : /^commit/.test(verdict || "") ? "commit" : "";

function mateMenuHtml(sid, t) {
  const onRow = ["queued", "reviewing", "done", "stopped"].includes(t.state) && !mateAsk(sid, t);
  const a = esc(t.agent_id || t.call), s = esc(sid);
  const review = t.state === "done" || t.state === "stopped" ? `<button data-mt-act="review" data-mt-sid="${s}" data-mt-agent="${a}">Review</button>` : "";
  // Reviewed: the button its review didn't recommend.
  const rec = t.state === "reviewed" ? mateRec(t) : "";
  const other = rec === "fixes" ? `<button data-mt-act="commit" data-mt-sid="${s}" data-mt-agent="${a}" ${(t.also || []).length ? `aria-disabled="true"` : ""}>Commit anyway</button>` : rec === "commit" ? `<button data-mt-act="fixes" data-mt-sid="${s}" data-mt-agent="${a}">Send fixes</button>` : "";
  return `<div class="more-menu rv-menu mt-menu">${other}${review}${onRow ? "" : `<button data-mt-act="diff" data-mt-sid="${s}" data-mt-agent="${a}">Diff</button>`}${t.review ? `<button data-mt-full="${esc(mateKey(sid, t))}">Full review</button>` : ""}<button data-mt-asked="${esc(mateKey(sid, t))}">What it was asked</button><button class="mt-aside" data-mt-act="aside" data-mt-sid="${s}" data-mt-agent="${a}">Set aside</button></div>`;
}

/** "Crew 4 · 1 needs you · 1 ready ▴", which opens its rows above the box: `tab`, as the tab on the box's top-right
 *  corner (compose.js quickRow); else a pill of its own (above an ask's card). "" when it has no crew. */
function crewStrip(sid, tab = false) {
  mateOpenOnWait(sid);
  const list = mateList(sid);
  if (!list.length) return "";
  const open = mateOpenLeads.has(sid);
  const need = list.filter((t) => mateAsk(sid, t)).length, ready = list.filter((t) => t.state === "reviewed").length;
  // Any at work: a live dot and how many ("Teammates · 2 working"); none: how many there are.
  const working = list.filter((t) => t.state === "working").length;
  const words = [working ? `<span class="dot-live"></span>Teammates · ${working} working` : `Teammates ${list.length}`, need ? `<span class="crew-need">${need} needs you</span>` : "", ready ? `${ready} ready` : ""].filter(Boolean).join(" · ");
  return `<button class="${tab ? "tplace-btn crew-tab" : "crew-strip"} ${open ? "on" : ""}" data-mt-open="${esc(sid)}" aria-expanded="${open}">${words}<span class="tm-sub">${open ? "▾" : "▴"}</span></button>`;
}

/** Above the lead's message box, while its strip is open: its crew's rows in one group that scrolls past a few, and why
 *  a Commit is held. "" when it has no crew or it's closed. */
function crewDock(sid) {
  mateOpenOnWait(sid);
  const list = mateList(sid);
  if (!list.length || !mateOpenLeads.has(sid)) return "";
  const menu = list.find((t) => mateKey(sid, t) === mateMenu);
  const held = list.find((t) => t.state === "reviewed" && (t.also || []).length);
  const heldLine = held ? `<div class="mt-held">Commit on <b>${esc(held.name)}</b> is held: <b>${esc(held.also[0])}</b> has uncommitted changes in one of its files.</div>` : "";
  return `<div class="crew-dock open" data-key="crew-dock"><div class="mt-wrap"><div class="mt-rows" data-scroll-key="mt:${esc(sid)}">${list.map((t) => mateRow(sid, t)).join("")}</div>${menu ? mateMenuHtml(sid, menu) : ""}${heldLine}</div></div>`;
}

/** The open team's rows, when they have to scroll (a short pane gives the chat its room first): the box ends under the
 *  last row it shows whole, never through one. Measured afresh after each draw, so it grows back with the window. What
 *  just opened under a row (its diff, its findings, why Commit didn't go) is scrolled into the box, its row with it. */
function placeTeamRows() {
  for (const box of document.querySelectorAll(".mt-rows")) {
    box.style.maxHeight = "";
    if (box.scrollHeight <= box.clientHeight) continue;
    const ends = [...box.querySelectorAll(":scope > .mt-row, :scope > .mt-under")].map((r) => r.offsetTop + r.offsetHeight).filter((b) => b <= box.clientHeight);
    if (ends.length) box.style.maxHeight = `${Math.max(...ends)}px`;
  }
  const show = mateShow && [...document.querySelectorAll(".mt-rows [data-mt-for]")].filter((e) => e.dataset.mtFor === mateShow);
  mateShow = null;
  if (!show?.length) return;
  const box = show[0].closest(".mt-rows"), row = show[0].previousElementSibling || show[0], last = show[show.length - 1];
  const bottom = last.offsetTop + last.offsetHeight;
  if (bottom > box.scrollTop + box.clientHeight) box.scrollTop = Math.min(row.offsetTop, bottom - box.clientHeight);
}
if (typeof window !== "undefined") window.addEventListener("resize", () => { if (typeof document !== "undefined") placeTeamRows(); });

/** On the lead's title line: a chip for each crew member that needs you ("hub · needs you"); "" when none does (the
 *  crew itself is in its strip above the box, crewDock). */
function mateChips(sid) {
  return mateList(sid).map((t) => [t, mateAsk(sid, t)]).filter(([, a]) => a).map(([t, a]) => `<button class="tm-chip need" data-big="${esc(a.id)}"><span class="tm-n">${esc(t.name)}</span> <span class="tm-sub">· needs you</span></button>`).join("");
}

// ---------- Cue's own words to a lead, and its crew's reviews, in the chat ----------
// What Cue sends a lead for its crew (hub/team.rs, team_out.rs) is drawn as Cue's bubble on the right, never as yours; the lead's
// reply to a review is one line (its verdict, its button, Open ›), its findings under the crew member's row.

/** What Cue sends a lead, by how it starts: its few words for it (plain), from the crew member's name and,
 *  for a task handed out, the task's first line. The cases both windows are held to: tests/fixtures/crew-review.json. */
const CUE_SAID = [
  [/^Review your teammate (.+?)['’]s work with a fork/, (n) => `Cue asked for ${n}'s review`, "review"],
  [/^Send your teammate (.+?) its review['’]s findings with SendMessage/, (n) => `Cue sent ${n}'s findings back to it`],
  [/^Your teammate (.+?) \(carried over from the session before you\) can['’]t be messaged/, (n) => `Cue asked to start ${n} again with its fixes`],
  [/^Stop your teammate (.+?) now/, (n) => `Cue asked to stop ${n}`],
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
 *  the exact text (addedNote, opened by `key`). `meta`: what follows "Cue ·" (when it went, or that it waits). */
function cueSaidLine(said, key, text, meta) {
  return `<div class="cv-you cue-you"><div class="cv-you-text">${cueWords(said.html)}${text ? addedNote(key, "What Cue sent", text) : ""}</div><div class="cv-meta">Cue · ${meta}</div></div>`;
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
/** A review's one line, in parts (tests/fixtures/crew-review.json "lines"): its plain words, the button it calls for
 *  ("fixes", "commit" or "": only on that member's current review — the reply is its saved review — while it's
 *  reviewed), whether Open › shows (while it's still in the crew), and the member (the latest of that name). */
function reviewLineParts(team, name, text) {
  const t = [...(team || [])].reverse().find((m) => m.name === name) || null;
  const { verdict, counts } = reviewSays(text);
  const current = !!t && t.state === "reviewed" && (t.review || "").trim() === String(text).trim();
  // Send fixes for a verdict with "fix" in it, Commit for one that starts with "commit", nothing on a guess.
  // A commit that lists anything (notes too) calls for its fixes first, as the row does (mateRec).
  const kind = verdictKind(verdict), button = !current ? "" : kind === "commit" && counts ? "fixes" : kind;
  const suffix = !t ? "not one of its teammates now" : t.state === "committed" ? "committed" : t.state === "aside" ? "set aside" : "";
  const words = [`${name} reviewed: ${verdict || "no verdict"}`, counts, suffix].filter(Boolean).join(" · ");
  return { words, verdict, counts, suffix, button, open: !!t && !suffix, member: t };
}
/** The lead's reply that is <name>'s review: one line with its verdict, its button, and Open › (its findings under
 *  its row). */
function reviewLine(sid, name, text) {
  const p = reviewLineParts(sessionOf(sid)?.team, name, text), t = p.member;
  const btn = p.button === "fixes" ? mateBtn(sid, t, "fixes", "Send fixes", true) : p.button === "commit" ? mateBtn(sid, t, "commit", "Commit", true, (t.also || []).length > 0) : "";
  const open = p.open ? `<button class="btn" data-crew-open="${esc(mateKey(sid, t))}">Open ›</button>` : "";
  // Amber for a verdict with "fix" in it, green for commit, plain for anything else.
  const tone = { fixes: "rv-fix", commit: "rv-ok" }[verdictKind(p.verdict)] || "";
  return `<div class="cv-review"><i class="stx-ic k-team">${PERSON_ICON}</i><span class="cv-review-w"><b>${esc(name)}</b> reviewed: <b class="${tone}">${esc(p.verdict || "no verdict")}</b>${p.counts ? ` · ${esc(p.counts)}` : ""}${p.suffix ? ` · <span class="dim">${esc(p.suffix)}</span>` : ""}</span>${btn}${open}</div>`;
}

/** The teammate an ask comes from, by name, when its session's team has it: "" otherwise. */
const mateOfAsk = (it) => it?.agent_id ? (sessionOf(it.session_id)?.team || []).find((t) => t.agent_id === it.agent_id)?.name || "" : "";
/** After the lead's name on an ask's card and in Waiting: "› <teammate>" (the space before it is the caller's: a
 *  card's top line spaces its parts itself). */
const mateTitle = (it) => { const n = mateOfAsk(it); return n ? `<span class="mt-of">› <b>${esc(n)}</b></span>` : ""; };

/** A teammate's button: Review, Diff (read, then shown under its row), Commit, Send fixes, Stop, Set aside. */
async function mateAct(sid, agent, action) {
  const k = `${sid}|${agent}`;
  if (action === "diff") {
    if (mateUnder.get(k) === "diff") { mateUnder.delete(k); return renderMain(); }
    mateUnder.set(k, "diff"); mateDiffs.set(k, null); mateShow = k; renderMain();
    try { mateDiffs.set(k, await invoke("team_action", { sessionId: sid, agent, action })); }
    catch (e) { mateDiffs.set(k, `Couldn't read its diff: ${e}`); }
    mateShow = k;
    return renderMain();
  }
  // Refused (Commit on main, say): the reason stays under its row until a button of its goes through.
  try { toast(await invoke("team_action", { sessionId: sid, agent, action })); mateNotes.delete(k); }
  catch (e) { mateNotes.set(k, String(e)); mateShow = k; }
  renderMain();
}

if (typeof document !== "undefined") {
  document.addEventListener("click", (e) => {
    const t = e.target;
    // The ⋯ menu closes on any click but its own button (one of its items runs as it closes).
    const closeMenu = mateMenu && !t.closest?.("[data-mt-more]");
    const el = t.closest?.("[data-mt-open], [data-mt-act], [data-mt-more], [data-mt-fold], [data-mt-asked], [data-mt-full], [data-crew-open]");
    if (closeMenu) mateMenu = null;
    if (!el) { if (closeMenu) renderMain(); return; }
    e.stopPropagation();
    const d = el.dataset;
    if (d.mtOpen) { mateOpenLeads.has(d.mtOpen) ? mateOpenLeads.delete(d.mtOpen) : mateOpenLeads.add(d.mtOpen); return renderMain(); }
    if (d.mtMore) { mateMenu = closeMenu || mateMenu === d.mtMore ? null : d.mtMore; return renderMain(); }
    // A review's line in the chat: Open › opens the crew above the box with its findings under its row.
    if (d.crewOpen) { const lead = d.crewOpen.split("|")[0]; mateOpenLeads.add(lead); mateUnder.set(d.crewOpen, "findings"); mateShow = d.crewOpen; return renderMain(); }
    const fold = d.mtFold || d.mtAsked || d.mtFull, what = d.mtFold ? "findings" : d.mtFull ? "review" : "asked";
    if (fold) { if (mateUnder.get(fold) === what) mateUnder.delete(fold); else { mateUnder.set(fold, what); mateShow = fold; } return renderMain(); }
    if (el.getAttribute("aria-disabled") === "true") return toast("Commit is held: another teammate has uncommitted changes in one of its files");
    renderMain();
    mateAct(d.mtSid, d.mtAgent, d.mtAct);
  }, true);
}
