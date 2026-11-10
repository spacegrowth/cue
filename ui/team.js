// ---------- a hosted lead's team: its teammates as rows in its chat (docs/dev/cue-teams.md §4) ----------
// Cue keeps the team from the lead's stream (hub/team.rs: state.sessions[].team); this draws it. On the lead's title
// line, a chip for each crew member that needs you; above the message box, a one-line strip ("Crew 3 · 1 ready ▴")
// that opens the crew above it: one row per crew member (its mark, its name, its state, how long, its newest step or
// its summary, the buttons its state needs). A finished one is reviewed by itself (hub/team.rs review_on_finish).
// A committed or set-aside teammate leaves the team. Asks from a teammate are titled "<lead> › <teammate>".

/** The leads whose team is open under their title, and per teammate what's open under its row ("findings", "diff",
 *  "asked"); the teammate whose ⋯ menu is open; the diffs read for it. */
const mateOpenLeads = new Set();
const mateUnder = new Map();     // "sid|agent" → "findings" | "diff" | "asked"
let mateMenu = null;             // "sid|agent"
const mateDiffs = new Map();     // "sid|agent" → the diff's text, or null while it's read
const mateNotes = new Map();     // "sid|agent" → why its last button didn't go through (Commit on main, say)
let mateShow = null;             // "sid|agent": what just opened under its row, scrolled into the box after the next draw

/** A lead's teammates still in its team (not committed or set aside). */
const mateList = (sid) => (sessionOf(sid)?.team || []).filter((t) => t.state !== "committed" && t.state !== "aside");
/** What a teammate asks you, if it does: the lead's pending ask from inside it. */
const mateAsk = (sid, t) => !!t.agent_id && (state.items || []).find((i) => i.session_id === sid && i.kind !== "waiting" && i.agent_id === t.agent_id) || null;
const mateKey = (sid, t) => `${sid}|${t.agent_id || t.call}`;

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
    case "reviewed": return mateBtn(sid, t, "commit", "Commit", true, (t.also || []).length > 0) + mateBtn(sid, t, "fixes", "Send fixes");
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
function mateMenuHtml(sid, t) {
  const onRow = ["queued", "reviewing", "done", "stopped"].includes(t.state) && !mateAsk(sid, t);
  const a = esc(t.agent_id || t.call), s = esc(sid);
  const review = t.state === "done" || t.state === "stopped" ? `<button data-mt-act="review" data-mt-sid="${s}" data-mt-agent="${a}">Review</button>` : "";
  return `<div class="more-menu rv-menu mt-menu">${review}${onRow ? "" : `<button data-mt-act="diff" data-mt-sid="${s}" data-mt-agent="${a}">Diff</button>`}<button data-mt-asked="${esc(mateKey(sid, t))}">What it was asked</button><button class="mt-aside" data-mt-act="aside" data-mt-sid="${s}" data-mt-agent="${a}">Set aside</button></div>`;
}

/** Above the lead's message box: the strip, "Crew 3 · 1 needs you · 1 ready ▴", and while it's open the crew above it,
 *  its rows in one group that scrolls past a few, and why a Commit is held. "" when it has no crew. */
function crewDock(sid) {
  const list = mateList(sid);
  if (!list.length) return "";
  const open = mateOpenLeads.has(sid);
  const need = list.filter((t) => mateAsk(sid, t)).length, ready = list.filter((t) => t.state === "reviewed").length;
  const words = [`Crew ${list.length}`, need ? `<span class="crew-need">${need} needs you</span>` : "", ready ? `${ready} ready` : ""].filter(Boolean).join(" · ");
  const strip = `<button class="crew-strip ${open ? "on" : ""}" data-mt-open="${esc(sid)}" aria-expanded="${open}">${words}<span class="tm-sub">${open ? "▾" : "▴"}</span></button>`;
  if (!open) return `<div class="crew-dock" data-key="crew-dock">${strip}</div>`;
  const menu = list.find((t) => mateKey(sid, t) === mateMenu);
  const held = list.find((t) => t.state === "reviewed" && (t.also || []).length);
  const heldLine = held ? `<div class="mt-held">Commit on <b>${esc(held.name)}</b> is held: <b>${esc(held.also[0])}</b> has uncommitted changes in one of its files.</div>` : "";
  return `<div class="crew-dock open" data-key="crew-dock"><div class="mt-wrap"><div class="mt-rows" data-scroll-key="mt:${esc(sid)}">${list.map((t) => mateRow(sid, t)).join("")}</div>${menu ? mateMenuHtml(sid, menu) : ""}${heldLine}</div>${strip}</div>`;
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
    const el = t.closest?.("[data-mt-open], [data-mt-act], [data-mt-more], [data-mt-fold], [data-mt-asked]");
    if (closeMenu) mateMenu = null;
    if (!el) { if (closeMenu) renderMain(); return; }
    e.stopPropagation();
    const d = el.dataset;
    if (d.mtOpen) { mateOpenLeads.has(d.mtOpen) ? mateOpenLeads.delete(d.mtOpen) : mateOpenLeads.add(d.mtOpen); return renderMain(); }
    if (d.mtMore) { mateMenu = closeMenu || mateMenu === d.mtMore ? null : d.mtMore; return renderMain(); }
    const fold = d.mtFold || d.mtAsked, what = d.mtFold ? "findings" : "asked";
    if (fold) { if (mateUnder.get(fold) === what) mateUnder.delete(fold); else { mateUnder.set(fold, what); mateShow = fold; } return renderMain(); }
    if (el.getAttribute("aria-disabled") === "true") return toast("Commit is held: someone else in the crew has uncommitted changes in one of its files");
    renderMain();
    mateAct(d.mtSid, d.mtAgent, d.mtAct);
  }, true);
}
