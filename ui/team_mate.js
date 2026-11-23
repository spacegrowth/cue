// ---------- a teammate on the crew page's left (team_page.js has the page) ----------
// A row in the list picks its teammate: it lights, and the left shows it in the chat's place, the Needs you bar and the
// box staying under it; the page gives the left all but a narrow list (team_page.js). One head line (‹ The lead's chat,
// its name and state, what it's doing now, Talk, Stop); what it was asked on one line with its size and time; its files
// as chips (a click opens that file's whole diff over the page: diff_overlay.js); then the reading area, the left's whole width, with three tabs: Diff so far, Full review, Full task. Its
// steps and what its review found unfold under its own card in the list (crewMateExp). Esc, ‹ The lead's chat or The
// lead's row go back to the chat where you left it.

/** The teammate of lead `sid` by its key (team.js mateKey), still in the crew or finished. */
const crewMateOf = (sid, key) => (sessionOf(sid)?.team || []).find((t) => mateKey(sid, t) === key) || null;
/** When the left last left the lead's chat (team_page.js crewLeftSet): a reply after it lights The lead's dot. */
let crewLeftChatAt = 0;

/** Pick teammate `key` of lead `sid`: the left shows it (at Diff so far), the bar its ask if it has one, the cursor in
 *  the box (`stay`: it stays in the list, ↑ / ↓). Another picked while one is open swaps it. */
function crewMatePick(sid, key, stay = false) {
  const t = crewMateOf(sid, key);
  if (!t) return;
  crewLeftSet(sid, { kind: "mate", key, tab: "diff", file: "" });
  const ask = mateAsk(sid, t);
  crewBarShow(sid, ask ? `a:${ask.id}` : `m:${key}`);
  mateMenu = null;
  renderMain();
  if (!stay) focusComposer();
}
/** Its state as the head's pill: its word, red when it needs you, dark at work, grey otherwise (team.js crewMark). */
const crewMatePill = (sid, t) => { const { word, look } = crewMark(sid, t); return `<span class="cp-pill ${{ need: "need", now: "run" }[look] || "off"}">${esc(word)}</span>`; };
/** "5 files +212 −8 · 21m, since 14:18": its size (as Cue read it, else its files' count) and how long, since when. */
function crewMateSize(t) {
  const files = (t.files || []).length, size = t.stat || (files ? plural(files, "file") : "");
  return [size, t.started_ms ? `${mateSpan(t)}, since ${hhmm(t.started_ms)}` : ""].filter(Boolean).join(" · ");
}
/** Its steps, oldest first, each { at, html, now, ask }: its own feed when the lead's chat has it (each step's time and
 *  words), else what Cue keeps (started, its lines in the crew's log, what it's on now); its ask last, in red. */
function crewMateSteps(sid, t) {
  const h = t.agent_id && typeof helpersOf === "function" ? helpersOf(sid).find((x) => x.id === t.agent_id) : null;
  const steps = (h?.items || []).filter((x) => x.t === "step").map((x) => ({ at: x.at_ms, html: `${esc(x.verb || "")} ${x.subject ? (x.mono ? `<code>${esc(x.subject)}</code>` : esc(x.subject)) : ""}`.trim(), now: x.done === false }));
  if (!steps.length) {
    if (t.started_ms) steps.push({ at: t.started_ms, html: "Started" });
    for (const d of crewEntriesOf(sid, t)) steps.push({ at: d.at_ms, html: crewCode(d.what || d.head || "") });
    if (t.step) steps.push({ at: t.step_ms, html: crewCode(crewStepCode(t.step)), now: t.state === "working" });
    else if (t.ended_ms && t.state !== "working") steps.push({ at: t.ended_ms, html: esc(t.summary ? `Finished: ${t.summary}` : "Finished") });
    steps.sort((a, b) => (a.at || 0) - (b.at || 0));
  }
  const ask = mateAsk(sid, t);
  if (ask) steps.push({ at: ask.created_ms, html: `${esc(verb(ask).replace(/^./, (c) => c.toUpperCase()))} <code>${esc(summary(ask))}</code>`, ask: true });
  return steps;
}
/** Its diff as Cue read it (ipc/team.rs team_diff_stat, team_diff), per teammate: { sig, files: [{ path, add, del }]
 *  or null while it's read, err, text: path → its part of the diff (null while it's read) }. Read again when what it
 *  changed may have changed (its state, its latest step, its files). */
const crewDiffs = new Map();
function crewDiffOf(sid, t) {
  const k = mateKey(sid, t), agent = t.agent_id || t.call;
  const sig = [t.state, t.step_ms, t.ended_ms, t.fix_rounds, (t.files || []).join(",")].join("|");
  let d = crewDiffs.get(k);
  if (d?.sig === sig) return d;
  d = { sig, files: null, err: "", text: new Map() };
  crewDiffs.set(k, d);
  invoke("team_diff_stat", { sessionId: sid, agent }).then((f) => { d.files = f; }, (e) => { d.err = String(e); }).finally(() => crewDiffs.get(k) === d && renderMain());
  return d;
}
/** File `path`'s part of its diff, read once per reading of the diff; null while it's read. */
function crewDiffText(sid, t, d, path) {
  if (d.text.has(path)) return d.text.get(path);
  d.text.set(path, null);
  invoke("team_diff", { sessionId: sid, agent: t.agent_id || t.call, path }).then((x) => d.text.set(path, x), (e) => d.text.set(path, `Couldn't read it: ${e}`)).finally(() => renderMain());
  return null;
}
/** A file's diff drawn: its hunks' heads, the lines added and removed on their tints, the rest plain; git's own header
 *  lines left out. Each line after a hunk's head has its old and new line numbers (from the head's "-a,b +c,d") in a
 *  gutter that stays put as long lines scroll sideways; drawn by the styles from data-o / data-n, so they're not in the
 *  text you copy. */
function crewDiffHtml(text) {
  const lines = String(text).split("\n"), at = lines.findIndex((l) => l.startsWith("@@"));
  const body = (at < 0 ? lines.filter((l) => !/^(diff --git|index |--- |\+\+\+ |new file|deleted file|similarity|rename )/.test(l)) : lines.slice(at)).filter((l, n, a) => l || n < a.length - 1);
  let o = 0, n = 0, top = 0;
  const rows = body.map((l) => {
    const h = l.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)/);
    if (h) { o = +h[1]; n = +h[2]; return `<span class="h">${esc(l)}</span>`; }
    if (l.startsWith("\\")) return `<span class="h">${esc(l)}</span>`;
    const kind = l.startsWith("+") ? "add" : l.startsWith("-") ? "del" : "";
    const old = kind === "add" || !o ? "" : o++, now = kind === "del" || !n ? "" : n++;
    top = Math.max(top, +old || 0, +now || 0);
    return `<span class="${kind}"><i class="ln" data-o="${old}" data-n="${now}"></i>${esc(l) || " "}</span>`;
  });
  return `<pre class="cpm-diff" style="--ln:${String(top || 1).length}ch">${rows.join("")}</pre>`;
}
/** Its files: as its diff counts them once read (each { path, add, del }), until then as Cue saw them written. */
function crewMateFiles(sid, t) {
  const d = (t.files || []).length || t.worktree ? crewDiffOf(sid, t) : null;
  return d?.files || (t.files || []).map((path) => ({ path }));
}
/** Diff so far: the lit file's part (the first file's until one is picked); its chip opens it whole over the page. */
function crewMateDiff(sid, t, left) {
  if (!(t.files || []).length && !t.worktree) return `<div class="cpm-nodiff"><span class="cp-dim">Cue hasn't seen it change a file yet.</span></div>`;
  const d = crewDiffOf(sid, t);
  if (d.err) return `<div class="cpm-nodiff"><span class="cp-dim">Couldn't read its diff: ${esc(d.err)}</span></div>`;
  if (!d.files) return `<div class="cpm-nodiff"><span class="cp-dim">Reading its diff…</span></div>`;
  if (!d.files.length) return `<div class="cpm-nodiff"><span class="cp-dim">Nothing changed so far.</span></div>`;
  const path = crewMateLitFile(sid, t, left), text = crewDiffText(sid, t, d, path);
  return (text == null ? `<div class="cpm-nodiff"><span class="cp-dim">Reading ${esc(path)}…</span></div>` : crewDiffHtml(text));
}
/** Its file `path` over the page (diff_overlay.js), its other files in the list beside it: each file's part of its
 *  diff read once per reading of the diff (crewDiffText's), then the overlay opens. */
async function crewMateShowFile(sid, key, path) {
  const t = crewMateOf(sid, key), d = t && crewDiffOf(sid, t);
  if (!d?.files?.length) return;
  const agent = t.agent_id || t.call;
  const read = (p) => d.text.get(p) ?? invoke("team_diff", { sessionId: sid, agent, path: p }).then((x) => (d.text.set(p, x), x), () => "");
  const texts = await Promise.all(d.files.map((f) => read(f.path)));
  const files = d.files.map((f, i) => ({ lines: [], isNew: false, ...diffParse(texts[i]).files[0], path: f.path, add: f.add, del: f.del }));
  diffOverlay(t.name, "Diff so far", files, Math.max(0, d.files.findIndex((f) => f.path === path)));
}
/** The file the reading area shows: the one picked, else the first. */
function crewMateLitFile(sid, t, left) {
  const files = crewMateFiles(sid, t);
  return files.some((f) => f.path === left.file) ? left.file : files[0]?.path || "";
}
/** The steps shown under a picked card before "N earlier ›": its latest ones. */
const CREW_STEPS_SHOWN = 8;
/** The teammates (by key) whose earlier steps are unfolded under their card. */
const crewStepsAll = new Set();
/** What unfolds under the picked teammate's card in the list (team_page.js crewListRow): its steps, its latest
 *  CREW_STEPS_SHOWN with "N earlier ›" before them, and what its review found. "" for any other card. */
function crewMateExp(sid, t) {
  const left = crewLeftOf(sid), k = mateKey(sid, t);
  if (left?.kind !== "mate" || left.key !== k) return "";
  const all = crewMateSteps(sid, t), open = crewStepsAll.has(k), cut = open ? 0 : Math.max(0, all.length - CREW_STEPS_SHOWN);
  const more = cut ? `<button class="mt-lk cpm-more" data-cp-steps="${esc(k)}">${cut} earlier ›</button>` : open && all.length > CREW_STEPS_SHOWN ? `<button class="mt-lk cpm-more" data-cp-steps="${esc(k)}">Fewer ‹</button>` : "";
  const steps = all.slice(cut).map((x) => `<div class="cpm-st ${x.now ? "now" : ""} ${x.ask ? "ask" : ""}"><span class="cp-t">${x.at ? esc(hhmm(x.at)) : ""}</span><i class="cpm-dot"></i><span class="cpm-sw">${x.html}</span></div>`).join("");
  const f = t.findings || [];
  const review = f.length ? f.map((l) => `<div class="cpm-fl">${crewKind(findingKind(l))}<span>${crewCode(crewFindingText(l))}</span></div>`).join("")
    : `<div class="cp-dim">${t.recommendation || ["reviewed", "committed"].includes(t.state) ? "Clean: nothing found." : ["queued", "reviewing"].includes(t.state) ? "Its review is under way." : t.state === "working" ? "Not reviewed yet: it's still working." : "Not reviewed."}</div>`;
  const part = (label, html) => `<div class="cpm-part"><div class="cpm-pl">${label}</div>${html}</div>`;
  return `<div class="cp-exp" data-scroll-key="cpe:${esc(k)}">${part("Steps", more + (steps || `<div class="cp-dim">None yet.</div>`))}${part("Review found", review)}</div>`;
}
/** Its files as chips over the reading area: each its name (the path's last part, its folder too when two share a
 *  name) and +/−, the one whose diff shows lit; a click lights it and opens its whole diff over the page. "" with no
 *  files. */
function crewMateChips(sid, t, left) {
  const files = crewMateFiles(sid, t), lit = left.tab === "diff" ? crewMateLitFile(sid, t, left) : "";
  if (!files.length) return "";
  const base = (p) => p.split("/").pop(), twice = (p) => files.filter((x) => base(x.path) === base(p)).length > 1;
  const name = (p) => (twice(p) ? p.split("/").slice(-2).join("/") : base(p));
  return `<div class="cpm-chips">${files.map((x) => `<button class="cpm-chip ${lit === x.path ? "on" : ""}" data-cp-file="${esc(x.path)}" aria-pressed="${lit === x.path}"><span class="cpm-fp">${esc(name(x.path))}</span>${x.add != null ? `<span class="cpm-n"><b class="add">+${x.add}</b>${x.del ? `<b class="del">−${x.del}</b>` : ""}</span>` : ""}</button>`).join("")}</div>`;
}
/** The reading area's tabs and what the lit one shows: Diff so far (the lit file's diff, crewMateDiff; its whole path
 *  at the bar's right), Full review (the reviewer's words), Full task (what it was asked, whole). */
function crewMateRead(sid, t, left) {
  const tabs = [["diff", "Diff so far"], ["review", "Full review"], ["task", "Full task"]];
  const lit = left.tab === "diff" ? crewMateLitFile(sid, t, left) : "";
  const bar = `<div class="cpm-tabs" role="tablist">${tabs.map(([k, w]) => `<button role="tab" class="${left.tab === k ? "on" : ""}" data-cp-tab="${k}" aria-selected="${left.tab === k}">${w}</button>`).join("")}<span class="grow"></span>${lit ? `<span class="cpm-lit">${esc(lit)}</span>` : ""}</div>`;
  let body;
  if (left.tab === "review") body = t.review ? `<div class="msg">${md(t.review)}</div>` : (t.findings || []).length ? `<ol class="mt-fx">${t.findings.map(mateFinding).join("")}</ol>` : `<div class="cp-dim">${["working", "queued", "reviewing"].includes(t.state) ? "No review yet." : "Its review found nothing."}</div>`;
  else if (left.tab === "task") body = `<div class="msg cpm-task">${md(t.asked || t.what || "")}</div>`;
  else body = crewMateDiff(sid, t, left);
  return `<div class="cpm-read">${bar}<div class="cpm-body" data-scroll-key="cpm:${esc(left.key)}:${left.tab}">${body}</div></div>`;
}
/** The teammate on the left (`left`: { key, tab, file }); "" when it's gone from the crew (the chat comes back). */
function crewMateView(sid, left) {
  const t = crewMateOf(sid, left.key);
  if (!t) return "";
  const n = crewNow(sid, t);
  const acts = `<button class="mt-lk" data-cp-talk="${esc(t.name)}">Talk</button>${t.state === "working" ? mateBtn(sid, t, "stop", "Stop", "link") : ""}`;
  const head = crewViewHead(sid, `<b class="cpv-name">${esc(t.name)}</b>${crewMatePill(sid, t)}<span class="cpm-now ${n.ask ? "q" : ""}">${crewCode(n.now)}</span><span class="cpm-acts">${acts}</span>`);
  const asked = `<div class="cpm-asked"><span class="cpm-al">Asked</span><span class="cpm-at">${crewCode(t.asked || t.what || "")}</span><span class="cpm-size">${esc(crewMateSize(t))}</span></div>`;
  return `<div class="cp-view cpm" data-cp-view="mate:${esc(left.key)}">${head}${asked}${crewMateChips(sid, t, left)}${crewMateRead(sid, t, left)}${mateUnderHtml(sid, t)}</div>`;
}
/** Talk: "@name " at the start of the box (in place of another @name there), the cursor after it. */
function crewTalk(sid, name) {
  const key = `s:${sid}`, rest = draft(key).text.replace(/^@[\w.-]+\s*/, "");
  setBox(key, `@${name} ${rest}`)?.focus();
}
/** Whether the lead said something since the left left its chat (its dot on The lead's row). */
function crewLeadSaid(sid) {
  const last = (sessionOf(sid)?.thread || []).filter(askLnSaid).at(-1);
  return !!crewLeftOf(sid) && crewLeftChatAt > 0 && !!last && last.at_ms > crewLeftChatAt;
}

if (typeof document !== "undefined") {
  document.addEventListener("click", (e) => {
    const sid = typeof crewPageSid !== "undefined" ? crewPageSid : null;
    const el = sid && e.target.closest?.(".crew-page [data-cp-mate], .crew-page [data-cp-tab], .crew-page [data-cp-file], .crew-page [data-cp-talk], .crew-page [data-cp-steps]");
    if (!el) return;
    e.stopImmediatePropagation();
    const d = el.dataset, left = crewLeftOf(sid);
    if (d.cpMate) return crewMatePick(sid, d.cpMate);
    if (d.cpTalk) return crewTalk(sid, d.cpTalk);
    if (d.cpSteps) { if (!crewStepsAll.delete(d.cpSteps)) crewStepsAll.add(d.cpSteps); return renderMain(); }
    if (left?.kind !== "mate") return;
    if (d.cpTab) left.tab = d.cpTab;
    if (d.cpFile != null) { left.file = d.cpFile; left.tab = "diff"; renderMain(); return crewMateShowFile(sid, left.key, d.cpFile); }
    renderMain();
  }, true);
}
