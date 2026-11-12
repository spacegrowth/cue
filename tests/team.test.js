// Run: node tests/team.test.js  (a hosted lead's team in its chat: ui/team.js)
const assert = require("assert");
const fs = require("fs");
const vm = require("vm");

const esc = (s) => String(s ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const NOW = 1793984400000, M = 60000;
const world = { sessions: {}, items: [] };
const ctx = {
  esc,
  get state() { return { items: world.items }; },
  document: { addEventListener() {} },
  sessionOf: (sid) => world.sessions[sid] || null,
  now: () => NOW,
  verb: () => "Allow",
  plain: (s) => s,
  summary: (it) => it.cmd,
  PERSON_ICON: "<svg></svg>",
  renderMain() {},
  toast() {},
};
vm.createContext(ctx);
vm.runInContext(fs.readFileSync(__dirname + "/../ui/team.js", "utf8").replace(/^(const|let) /gm, "var "), ctx);

const mate = (name, state, o = {}) => ({ call: `call-${name}`, agent_id: `id-${name}`, name, state, started_ms: NOW - 9 * M, files: [], ...o });
world.sessions.L = { session_id: "L", team: [
  mate("hub", "working", { step: "Running: cargo test" }),
  mate("rows", "working", { step: "Editing teamcard.js", files: ["ui/teamcard.js", "ui/core.js", "ui/x.js"], via_shell: ["ui/x.js"] }),
  mate("skill", "done", { ended_ms: NOW - 2 * M, summary: "team skill written, 12 tests pass", stat: "2 files +140 −0", files: ["skills/team/SKILL.md", "skills/team/review.md"] }),
  mate("handoff", "reviewed", { findings: ["1. hub/handoff.rs:40 — should-fix — the list is cut at 8", "2. ui/core.js:212 — note — commit after rows"], recommendation: "commit", stat: "3 files +88 −12", also: ["rows"] }),
  mate("gone", "committed"),
  mate("away", "aside"),
] };
world.items = [{ id: "a1", session_id: "L", kind: "permission", agent_id: "id-hub", agent_name: "hub", cmd: "cargo test --features ext" }];

// ---- the title line: who needs you; the strip above the box ----
let chips = ctx.mateChips("L");
assert.match(chips, /data-big="a1"><span class="tm-n">hub<\/span> <span class="tm-sub">· needs you/, "a chip for the crew member that asks, opening its ask");
assert.doesNotMatch(chips, /data-mt-open/, "the crew opens from its strip, not the title");
assert.equal(ctx.mateChips("nobody"), "");
assert.match(ctx.crewStrip("L"), /data-mt-open="L" aria-expanded="true"><span class="dot-live"><\/span>Cue Crew · 4 · 2 working · <span class="crew-need">1 needs you<\/span> · 1 ready<span class="tm-sub">▾/, "a live dot and how many work while any does, how many are in it (committed and set aside left), who needs you, who's ready; open, as some wait on you");
// A teammate waiting on you (handoff: reviewed; hub: asks) opens them over the box, once; closed, they stay closed
// until another starts waiting.
ctx.mateOpenLeads.clear(); ctx.mateSeenWaiting.clear();
assert.equal(ctx.mateOpenLeads.has("L"), false);
ctx.crewStrip("L");
assert.equal(ctx.mateOpenLeads.has("L"), true, "opened for the ones waiting on you");
ctx.mateOpenLeads.delete("L");
ctx.crewStrip("L");
assert.equal(ctx.mateOpenLeads.has("L"), false, "closed by you: stays closed");
assert.match(ctx.crewWaitLine("L"), /Waiting on you: <b>hub<\/b> asks you something and 1 more/, "the line under the chat names who waits on you");
const ready = world.sessions.L.team.find((t) => t.name === "skill");
ready.state = "reviewed";
ctx.crewStrip("L");
assert.equal(ctx.mateOpenLeads.has("L"), true, "another ready: open again");
ready.state = "done";
ctx.mateOpenLeads.delete("L");
const saved = world.items; world.items = [];
const states = world.sessions.L.team.map((t) => t.state);
world.sessions.L.team.forEach((t) => { if (t.state === "reviewed") t.state = "done"; });
assert.match(ctx.crewWaitLine("L"), /dot-live.*Crew working: hub, rows/, "none waits on you: who's at work, by name");
world.sessions.L.team.forEach((t) => { if (t.state === "working") t.state = "done"; });
assert.equal(ctx.crewWaitLine("L"), "", "neither: no line of its own");
world.sessions.L.team.forEach((t, i) => { t.state = states[i]; });
world.items = saved;
assert.equal(ctx.crewDock("L"), "", "closed: no rows above the box");
const idle = world.sessions.L.team.map((t) => t.state);
world.sessions.L.team.forEach((t) => { if (t.state === "working") t.state = "done"; });
assert.match(ctx.crewStrip("L"), />Cue Crew · 4 · /, "none at work: how many, no dot");
assert.doesNotMatch(ctx.crewStrip("L"), /dot-live/);
world.sessions.L.team.forEach((t, i) => { t.state = idle[i]; });
assert.equal(ctx.crewStrip("nobody"), "", "no crew, no strip");

// ---- the rows, open above the box ----
ctx.mateOpenLeads.add("L");
assert.match(ctx.crewStrip("L"), /crew-strip on.*▾/s);
const g = ctx.crewDock("L");
assert.match(g, /class="crew-dock open" data-key="crew-dock"><div class="crew-drawer" data-crew-drawer="L"><div class="crew-bar" data-mt-open="L" role="button" aria-expanded="true"><span class="crew-bar-w"><span class="dot-live"><\/span>Cue Crew · 4 · 2 working · <span class="crew-need">1 needs you<\/span> · 1 ready<\/span><button class="crew-close" data-mt-open="L">Close <span class="tm-sub">▾<\/span><\/button><\/div>.*mt-rows/s, "the drawer's title bar: the tab's words, Close ▾; the rows under it");
assert.doesNotMatch(g, /title="/, "no hover text");
assert.doesNotMatch(g, /Set aside|data-mt-act="aside"/, "no Set aside: Cue clears members by itself");
const row = (name) => g.split('<div class="mt-row').find((r) => r.includes(`>${name}</button>`)) || "";
assert.match(row("hub"), /needs you/);
assert.match(row("hub"), /Allow: cargo test --features ext/, "what it asks, on its row");
assert.doesNotMatch(row("hub"), /data-mt-act="stop"/, "an asking teammate's buttons are its card's");
assert.match(row("rows"), /working.*>9m · Editing teamcard.js · 3 files so far \(1 via shell\)/s, "a file read off its shell commands says so");
assert.match(row("rows"), /data-mt-act="stop"/);
assert.match(row("skill"), /done.*>7m · team skill written, 12 tests pass · 2 files \+140 −0/s, "how long it took, its summary, its size");
assert.match(row("skill"), /data-mt-act="diff"/);
assert.doesNotMatch(row("skill"), /data-mt-act="review"/, "finished: reviewed by itself, no Review button");
assert.match(row("handoff"), /reviewed · 2 findings/);
assert.match(row("handoff"), /also changed by rows/);
// handoff's review says commit but lists 2 findings: they go back first (nothing dropped), so Send fixes is its button
// and Commit anyway is in ⋯, held as Commit would be while rows shares a file.
assert.match(row("handoff"), /class="btn primary "[^>]*data-mt-act="fixes"[^>]*>Send fixes</, "commit with findings: the findings first");
assert.doesNotMatch(row("handoff"), /data-mt-act="commit"/);
{
  const h = world.sessions.L.team.find((t) => t.name === "handoff");
  assert.match(ctx.mateMenuHtml("L", h), /data-mt-act="commit"[^>]*aria-disabled="true"[^>]*>Commit anyway</, "Commit anyway in ⋯, held");
  const findings = h.findings;
  h.findings = [];
  let r2 = ctx.crewDock("L").split('<div class="mt-row').find((r) => r.includes(">handoff</button>")) || "";
  assert.match(r2, /class="btn primary held"[^>]*data-mt-act="commit"[^>]*aria-disabled="true"/, "a clean commit: only Commit, held while another crew member shares a file");
  assert.doesNotMatch(r2, /data-mt-act="fixes"/);
  assert.match(ctx.mateMenuHtml("L", h), /data-mt-act="fixes"[^>]*>Send fixes</, "Send fixes in ⋯");
  assert.doesNotMatch(ctx.mateMenuHtml("L", h), /Commit anyway/);
  h.findings = findings;
  h.recommendation = "send fixes";
  r2 = ctx.crewDock("L").split('<div class="mt-row').find((r) => r.includes(">handoff</button>")) || "";
  assert.match(r2, /class="btn primary "[^>]*data-mt-act="fixes"[^>]*>Send fixes</, "said send fixes: only Send fixes");
  h.recommendation = "";
  r2 = ctx.crewDock("L").split('<div class="mt-row').find((r) => r.includes(">handoff</button>")) || "";
  assert.match(r2, /data-mt-act="commit".*data-mt-act="fixes"/s, "said neither: both");
  h.recommendation = "commit";
}
assert.match(g, /Commit on <b>handoff<\/b> is held: <b>rows<\/b>/);
assert.doesNotMatch(g, /gone|away/, "committed and set aside: out of the team");
assert.doesNotMatch(g, /left:|border-left/, "no stripes");

// ---- findings fold open; the ⋯ menu ----
ctx.mateUnder.set("L|id-handoff", "findings");
const open = ctx.crewDock("L");
assert.match(open, /<span class="mono">hub\/handoff.rs:40<\/span> · the list is cut at 8 <span class="rv-k">should-fix<\/span>/);
ctx.mateMenu = "L|id-rows";
assert.match(ctx.crewDock("L"), /mt-menu"><button data-mt-act="diff"[^>]*>Diff<\/button><button data-mt-asked="L\|id-rows">What it was asked<\/button><\/div>/, "working: Diff in ⋯, no Set aside");
// No files Cue knows (hub's asked its question before writing any): no Diff, on its row or in ⋯.
const hub = world.sessions.L.team[0], hubAsk = world.items;
world.items = [];
ctx.mateMenu = "L|id-hub";
assert.doesNotMatch(ctx.crewDock("L").split('<div class="mt-row').find((r) => r.includes(">hub</button>")), /data-mt-act="diff"/, "no files: no Diff");
hub.worktree = "/work/cue-hub";
assert.match(ctx.crewDock("L"), /mt-menu"><button data-mt-act="diff"/, "a worktree of its own: Diff reads it whole");
delete hub.worktree;
world.items = hubAsk;
ctx.mateMenu = null;
// Diff opens the session's Diff tab on that member's files (diff.js diffShowMember), nothing under its row.
const shown = [];
ctx.diffShowMember = (...a) => shown.push(a);
ctx.mateAct("L", "id-skill", "diff");
assert.deepEqual(shown, [["L", "id-skill", "skill"]]);
assert.doesNotMatch(ctx.crewDock("L"), /mt-diff|Reading its diff/);
for (const t of world.sessions.L.team) assert.doesNotMatch(ctx.mateMenuHtml("L", t), /aside/i, `no Set aside in ${t.name}'s ⋯`);
ctx.mateMenu = null;

// ---- asks titled with the teammate ----
assert.equal(ctx.mateOfAsk(world.items[0]), "hub");
assert.equal(ctx.mateTitle(world.items[0]), `<span class="mt-of">› <b>hub</b></span>`);
assert.equal(ctx.mateTitle({ session_id: "L", agent_id: "" }), "", "the lead's own ask: just its name");
assert.equal(ctx.mateTitle({ session_id: "L", agent_id: "someone-else" }), "", "an agent not in its team");

console.log("team ok");

// ---- Cue's own words to a lead, and a review's one line: every case both windows are held to ----
const cases = JSON.parse(require("fs").readFileSync(require("path").join(__dirname, "fixtures/crew-review.json"), "utf8"));
for (const c of cases.said) {
  const got = ctx.cueSaid(c.text);
  if (c.words === null) assert.equal(got, null, `yours, not Cue's: ${c.text}`);
  else assert.deepEqual([got?.words, got?.name, got?.review], [c.words, c.name, c.review], c.text);
}
assert.match(ctx.cueSaid("Review your teammate docs's work with a fork").html, /Cue asked for <b>docs<\/b>'s review/, "the name in bold");
assert.equal(ctx.cueSaid("Review your teammate <img>'s work with a fork").html.includes("<img>"), false, "a name is escaped");
// Drawn as Cue's bubble on the right: named Cue in its meta line, its words without "Cue", the text folded behind addedNote.
ctx.addedNote = (key, label, line) => `<note ${key}|${label}|${line}>`;
const bubble = ctx.cueSaidLine(ctx.cueSaid("Review your teammate docs's work with a fork"), "L:1", "Review your teammate docs's work with a fork", "2m ago");
assert.match(bubble, /^<div class="cv-you cue-you"><div class="cv-you-text">Asked for <b>docs<\/b>'s review<note L:1\|What Cue sent\|Review your teammate docs's work with a fork><\/div><div class="cv-meta">Cue · 2m ago<\/div><\/div>$/);
assert.doesNotMatch(ctx.cueSaidLine({ html: "Cue will ask for <b>x</b>'s review" }, "", "", "waits"), /<note/, "nothing sent yet: nothing to unfold");
for (const c of cases.reviews) assert.deepEqual(ctx.reviewSays(c.text), { verdict: c.verdict, counts: c.counts }, c.text);
for (const c of cases.lines) {
  const p = ctx.reviewLineParts(c.team, "docs", c.text);
  assert.deepEqual([p.words, p.button || null, p.open], [c.words, c.button, c.open], c.about);
}
for (const c of cases.replies) {
  const got = ctx.reviewReplies(c.chat, (name) => c.member?.name === name ? c.member.review : "");
  assert.deepEqual(Object.fromEntries([...got].map(([k, v]) => [String(k), v])), c.reviews, c.about);
}
world.sessions.L.team.push({ call: "c9", agent_id: "id-docs", name: "docs", state: "reviewed", findings: ["1. d.md:1 — note — fine"], recommendation: "commit", review: "1. d.md:1 — note — fine\nRecommendation: commit" });
const line = ctx.reviewLine("L", "docs", "1. d.md:1 — note — fine\nRecommendation: commit");
assert.match(line, /<b>docs<\/b> reviewed: <b class="rv-ok">commit<\/b> · 1 note/);
assert.match(line, /data-mt-act="fixes"[^>]*>Send fixes</, "its current review says commit with a note: the note goes back first");
assert.match(line, /data-crew-open="L\|id-docs">Open ›/);
assert.doesNotMatch(ctx.reviewLine("L", "docs", "an older review\nRecommendation: send fixes"), /data-mt-act/, "an older review of it: no button");

// ---- a finished member whose files Cue never saw (shell commands outside the lead's folder); one the hub says needs you ----
{
  world.sessions.U = { session_id: "U", team: [
    { call: "c-u1", agent_id: "u1", name: "board", state: "reviewed", recommendation: "commit", started_ms: NOW - 9 * M, files: [] },
    { call: "c-u2", agent_id: "u2", name: "mock", state: "done", started_ms: NOW - 9 * M, ended_ms: NOW - M, files: [] },
    { call: "c-u3", agent_id: "u3", name: "fixer", state: "reviewed", recommendation: "send fixes", findings: ["1. a — should-fix — b"], started_ms: NOW - 9 * M, files: [] },
    { call: "c-u4", agent_id: "u4", name: "quiet", state: "stopped", started_ms: NOW - 9 * M, files: [] },
    { call: "c-u5", agent_id: "u5", name: "lint", state: "done", started_ms: NOW - 9 * M, files: ["a.js"], stat: "1 file +2 −0", needs: "Its lead didn't say it committed it" },
  ] };
  // Files Cue never saw alone don't wait on you (Cue handles them): no drawer by itself, no line.
  world.sessions.V = { session_id: "V", team: [{ call: "c-v", agent_id: "v1", name: "solo", state: "done", started_ms: NOW - 9 * M, files: [] }] };
  ctx.crewStrip("V");
  assert.equal(ctx.mateOpenLeads.has("V"), false, "files unknown: not waiting on you, stays closed");
  assert.equal(ctx.crewWaitLine("V"), "");
  delete world.sessions.V;
  ctx.crewStrip("U");
  assert.equal(ctx.mateOpenLeads.has("U"), true, "they wait on you: the drawer opens by itself");
  const d = ctx.crewDock("U");
  const rowOf = (name) => d.split('<div class="mt-row').find((r) => r.includes(`>${name}</button>`)) || "";
  assert.match(rowOf("board"), /class="btn primary "[^>]*data-mt-act="commit"[^>]*>Commit</, "reviewed, files unknown: Commit (its lead commits them)");
  assert.match(rowOf("mock"), /class="btn primary "[^>]*data-mt-act="commit"[^>]*>Commit</, "done, files unknown: Commit, not Diff");
  assert.doesNotMatch(rowOf("mock"), /data-mt-act="diff"/);
  assert.match(rowOf("mock"), />8m · Cue doesn&#39;t know which files it changed: Commit asks its lead to commit them<|>8m · Cue doesn't know which files it changed: Commit asks its lead to commit them</, "says why");
  assert.match(rowOf("fixer"), /class="btn primary "[^>]*data-mt-act="fixes"[^>]*>Send fixes</, "its review asks for fixes: Send fixes, one button");
  assert.doesNotMatch(rowOf("quiet"), /data-mt-act=/, "stopped having written nothing: no button (Cue clears it)");
  assert.match(rowOf("lint"), /tm-chip need small">needs you<.*· Its lead didn(&#39;|')t say it committed it/s, "the hub's needs: needs you, and what it needs");
  assert.match(ctx.crewStrip("U"), /<span class="crew-need">1 needs you<\/span> · 2 ready/, "needs counts in needs you");
  assert.doesNotMatch(d, /aside/i, "no Set aside anywhere");
  delete world.sessions.U;
}
