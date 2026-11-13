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
  inTerminal: (it) => it.kind === "terminal",
  tool: () => "bash",
  requestBody: (it) => `<body ${it.cmd}>`,
  decisionButtons: (it) => `<buttons ${it.id}>`,
  md: (s) => s,
  crewMode: () => "",
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
  mate("gone", "committed", { ended_ms: NOW - 10 * M }),
  mate("old", "committed", { ended_ms: NOW - 61 * M }),
  mate("away", "aside"),
] };
world.items = [{ id: "a1", session_id: "L", kind: "permission", agent_id: "id-hub", agent_name: "hub", cmd: "cargo test --features ext" }];

// ---- the title line: who needs you; the tray on the box ----
let chips = ctx.mateChips("L");
assert.match(chips, /data-big="a1"><span class="tm-n">hub<\/span> <span class="tm-sub">· needs you/, "a chip for the crew member that asks, opening its ask");
assert.doesNotMatch(chips, /data-mt-open/, "the crew opens from its tray, not the title");
assert.equal(ctx.mateChips("nobody"), "");
const head = (sid) => (ctx.crewTray(sid).match(/<div class="crew-head"[\s\S]*?<\/button><\/div>/) || [""])[0];
assert.match(head("L"), /data-mt-open="L" role="button" aria-expanded="true"><span class="dot-live"><\/span><span class="crew-w">Cue Crew · 4 <span class="crew-need">· hub needs you<\/span> <span class="crew-mu">· 1 ready · 1 working<\/span><\/span>/, "a live dot while any works, how many are in it (committed and set aside left), who needs you (hub, so not counted at work), then ready and working; open, as some wait on you");
assert.match(head("L"), /class="crew-car" data-mt-open="L"[^>]*>▾</, "open: its caret closes it");
// A teammate waiting on you (handoff: reviewed; hub: asks) opens the tray, once; closed, it stays closed until another
// starts waiting.
ctx.mateOpenLeads.clear(); ctx.mateSeenWaiting.clear();
assert.equal(ctx.mateOpenLeads.has("L"), false);
ctx.crewTray("L");
assert.equal(ctx.mateOpenLeads.has("L"), true, "opened for the ones waiting on you");
ctx.mateOpenLeads.delete("L");
assert.match(ctx.crewTray("L"), /^<div class="crew-tray" data-key="crew-tray" data-crew-tray="L"><div class="crew-head"[^]*▴<\/button><\/div><\/div>$/, "closed by you: stays closed, one line");
assert.match(ctx.crewWaitLine("L"), /Waiting on you: <b>hub<\/b> asks you something and 1 more/, "the line under the chat names who waits on you");
const ready = world.sessions.L.team.find((t) => t.name === "skill");
ready.state = "reviewed";
ctx.crewTray("L");
assert.equal(ctx.mateOpenLeads.has("L"), true, "another ready: open again");
ready.state = "done";
ctx.mateOpenLeads.delete("L");
const firstAsk = world.items[0];
world.items = [{ id: "a2", session_id: "L", kind: "permission", agent_id: "id-hub", cmd: "cargo build" }];
ctx.crewTray("L");
assert.equal(ctx.mateOpenLeads.has("L"), true, "the same member asks something new: open again");
world.items = [firstAsk];
ctx.mateOpenLeads.delete("L");
// Its ask opened in the pane (its card, its chip): the tray opens for it, once.
ctx.crewOpenForAsk(world.items[0]);
assert.equal(ctx.mateOpenLeads.has("L"), true, "its ask opened: the tray opens for it");
ctx.mateOpenLeads.delete("L");
ctx.crewOpenForAsk(world.items[0]);
assert.equal(ctx.mateOpenLeads.has("L"), false, "the same ask again: closed by you stays closed");
ctx.crewOpenForAsk({ id: "x", session_id: "L", agent_id: "" });
assert.equal(ctx.mateOpenLeads.has("L"), false, "the lead's own ask: not the tray's");
const saved = world.items; world.items = [];
const states = world.sessions.L.team.map((t) => t.state);
world.sessions.L.team.forEach((t) => { if (t.state === "reviewed") t.state = "done"; });
assert.match(ctx.crewWaitLine("L"), /dot-live.*Crew working: hub, rows/, "none waits on you: who's at work, by name");
world.sessions.L.team.forEach((t) => { if (t.state === "working") t.state = "done"; });
assert.equal(ctx.crewWaitLine("L"), "", "neither: no line of its own");
assert.match(head("L"), /<span class="crew-dot"><\/span><span class="crew-w">Cue Crew · 4<\/span>/, "none at work, none waiting: how many, a still dot");
world.sessions.L.team.forEach((t, i) => { t.state = states[i]; });
world.items = saved;
assert.equal(ctx.crewTray("nobody"), "", "no crew, no tray");

// ---- open: the groups ----
ctx.mateOpenLeads.add("L");
const g = ctx.crewTray("L");
assert.match(g, /<div class="crew-body" data-scroll-key="crew:L"><div class="mt-grp red">Needs you<\/div>.*<div class="mt-grp ">Ready<\/div>.*<div class="mt-grp ">Working<\/div>.*<div class="mt-done " data-mt-for="done:L"><button class="mt-lk mt-done-line" data-mt-done="L" aria-expanded="false">2 done ›<\/button><\/div>/s, "Needs you, Ready, Working, then the finished ones (done, and committed in the last hour: not old) folded into N done ›");
assert.doesNotMatch(g, /title="/, "no hover text");
assert.doesNotMatch(g, /Set aside|data-mt-act="aside"/, "no Set aside: Cue clears members by itself");
assert.doesNotMatch(g, /<svg|stx-ic|⋯|rv-more/, "no person icons, no ⋯");
assert.doesNotMatch(g, /left:|border-left/, "no stripes");
assert.doesNotMatch(g, /is held/, "no held line of its own");
assert.doesNotMatch(g, />skill<|gone|away/, "done and committed: folded away; set aside: out of the team");
ctx.mateDoneOpen.add("L");
assert.match(ctx.crewTray("L"), />gone<\/button>/, "committed 10 minutes ago: among the finished");
assert.doesNotMatch(ctx.crewTray("L"), />old<\/button>/, "committed over an hour ago: gone from the tray");
ctx.mateDoneOpen.delete("L");
// hub asks: its row under Needs you, what it asks under it, ↪, Deny and Allow on it (the ask's own buttons).
assert.match(g, /mt-row need.*data-mt-more="L\|id-hub"[^>]*>hub<\/button><i>Allow<\/i>/s, "its name opens its menu; what it wants");
assert.match(g, /data-act="redirect" data-id="a1"[^>]*>↪<\/button><button class="btn mt-b deny" data-act="deny" data-id="a1">Deny<\/button><button class="btn mt-b primary" data-act="allow" data-id="a1">Allow</, "↪, Deny and Allow on its row");
assert.match(g, /<div class="mt-ask"><body cargo test --features ext><\/div>/, "what it asks, under its row");
ctx.redirectFor = "a1";
assert.match(ctx.crewTray("L"), /<div class="mt-ask"><body cargo test --features ext><buttons a1><\/div>/, "↪ pressed: its box under the ask");
delete ctx.redirectFor;
assert.doesNotMatch(g, /data-mt-act="stop"[^>]*data-mt-agent="id-hub"/, "an asking member's buttons are its ask's");
// handoff: its review says commit but lists 2 findings: they go back first (nothing dropped), and its line says so.
assert.match(g, /handoff<\/button><i>2 findings · fixes go back, round 1 of 2<\/i><\/div><div class="mt-btns"><button class="btn mt-b primary "[^>]*data-mt-act="fixes"[^>]*>Send fixes<\/button><\/div><ol class="mt-fx"><li><code>hub\/handoff.rs:40<\/code> the list is cut at 8 <em>should fix<\/em><\/li><li><code>ui\/core.js:212<\/code> commit after rows <em>note<\/em><\/li><\/ol>/, "one button that agrees with its line; the findings a plain numbered list");
assert.doesNotMatch(g, /recommendation: commit/, "no line that disagrees with its button");
assert.match(g, /rows<\/button><span class="mt-line">9m · Editing teamcard.js · 3 files so far \(1 via shell\)<\/span><button class="mt-lk "[^>]*data-mt-act="stop"[^>]*>Stop<\/button>/, "at work: one line, a plain Stop; a file read off its shell commands says so");
assert.doesNotMatch(g, /data-mt-act="review"/, "finished: reviewed by itself, no Review button");
{
  const h = world.sessions.L.team.find((t) => t.name === "handoff");
  assert.match(ctx.mateMenuHtml("L", h), /data-mt-act="commit"[^>]*aria-disabled="true"[^>]*>Commit anyway</, "Commit anyway in its menu, held");
  const findings = h.findings;
  h.findings = [];
  let r2 = ctx.crewTray("L");
  assert.match(r2, /handoff<\/button><i>clean review · commits after rows does<\/i><\/div><div class="mt-btns"><button class="btn mt-b primary held"[^>]*data-mt-act="commit"[^>]*aria-disabled="true">Commit<\/button><\/div>/, "clean: Commit, held, and its line says why");
  assert.doesNotMatch(r2, /data-mt-act="fixes"/);
  assert.match(ctx.mateMenuHtml("L", h), /data-mt-act="fixes"[^>]*>Send fixes</, "Send fixes in its menu");
  assert.doesNotMatch(ctx.mateMenuHtml("L", h), /Commit anyway/);
  h.also = [];
  h.files = ["a.rs", "b.rs"];
  assert.match(ctx.crewTray("L"), /handoff<\/button><i>clean review · commit it<\/i>.*<div class="mt-l">3 files \+88 −12 · a.rs, b.rs<\/div>/s, "clean: its files under it");
  delete h.files;
  h.also = ["rows"];
  h.findings = findings;
  h.recommendation = "send fixes";
  h.fix_rounds = 2;
  r2 = ctx.crewTray("L");
  assert.match(r2, /handoff<\/button><i>2 findings · fixes go back, round 3<\/i>.*>Send fixes</s, "past Cue's own rounds: just the round");
  h.recommendation = "";
  h.fix_rounds = 0;
  assert.match(ctx.crewTray("L"), /handoff<\/button><i>2 findings · fixes go back, round 1 of 2<\/i>.*>Send fixes</s, "said neither, with findings: they go back");
  h.findings = [];
  assert.match(ctx.crewTray("L"), /handoff<\/button><i>no verdict, nothing found · commits after rows does<\/i>.*>Commit</s, "said neither, nothing found: Commit");
  h.findings = findings;
  h.recommendation = "commit";
}

// ---- the menu a name opens ----
ctx.mateMenu = "L|id-rows";
assert.match(ctx.crewTray("L"), /mt-menu" data-mt-menu="L\|id-rows"><button data-helper="L\|id-rows">Its steps<\/button><button data-mt-act="diff"[^>]*>Diff<\/button><button data-mt-asked="L\|id-rows">What it was asked<\/button><\/div>/, "working: its steps, Diff, what it was asked");
assert.match(ctx.crewTray("L"), /class="mt-name on" data-mt-more="L\|id-rows" aria-expanded="true">rows</);
ctx.mateUnder.set("L|id-rows", "asked");
world.sessions.L.team[1].asked = "Draw the rows";
assert.match(ctx.crewTray("L"), /<div class="mt-under mt-asked" data-mt-for="L\|id-rows">Draw the rows<\/div>/, "what it was asked, under its row");
ctx.mateUnder.delete("L|id-rows");
// No files Cue knows (hub's asked its question before writing any): no Diff, on its row or in its menu.
const hub = world.sessions.L.team[0], hubAsk = world.items;
world.items = [];
ctx.mateMenu = "L|id-hub";
assert.doesNotMatch(ctx.crewTray("L"), /data-mt-act="diff"[^>]*data-mt-agent="id-hub"/, "no files: no Diff");
hub.worktree = "/work/cue-hub";
assert.match(ctx.crewTray("L"), /mt-menu" data-mt-menu="L\|id-hub"><button data-helper="L\|id-hub">Its steps<\/button><button data-mt-act="diff"/, "a worktree of its own: Diff reads it whole");
delete hub.worktree;
world.items = hubAsk;
ctx.mateMenu = null;
// Diff opens the session's Diff tab on that member's files (diff.js diffShowMember), nothing under its row.
const shown = [];
ctx.diffShowMember = (...a) => shown.push(a);
ctx.mateAct("L", "id-skill", "diff");
assert.deepEqual(shown, [["L", "id-skill", "skill"]]);
ctx.mateOpenLeads.add("L");
for (const t of world.sessions.L.team) assert.doesNotMatch(ctx.mateMenuHtml("L", t), /aside/i, `no Set aside in ${t.name}'s menu`);

// ---- the finished ones unfolded: one line each, with its Diff ----
world.sessions.L.team.find((t) => t.name === "gone").commit = "a1b2c3d4e5";
world.sessions.L.team.find((t) => t.name === "gone").files = ["x.js"];
ctx.mateDoneOpen.add("L");
const done = ctx.crewTray("L");
assert.match(done, /<div class="mt-done open" data-mt-for="done:L"><button class="mt-lk mt-done-line" data-mt-done="L" aria-expanded="true">2 done ⌄<\/button>.*skill<\/button><span class="mt-line">7m · team skill written, 12 tests pass · 2 files \+140 −0<\/span><button class="mt-lk "[^>]*data-mt-act="diff"[^>]*>Diff<\/button>.*gone<\/button><span class="mt-line">[^<]*committed a1b2c3d<\/span><button class="mt-lk "[^>]*data-mt-act="diff"/s, "unfolded: each its own line, how long it took, its summary, its size, its Diff");
ctx.mateDoneOpen.delete("L");
delete world.sessions.L.team.find((t) => t.name === "gone").commit;

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
// A review request's bubble folds the lead's review under its own line (the review isn't a message of its own).
world.sessions.L.team.push({ call: "c9", agent_id: "id-docs", name: "docs", state: "reviewed", findings: ["1. d.md:1 — note — fine"], recommendation: "commit", review: "1. d.md:1 — note — fine\nRecommendation: commit" });
const folded = ctx.cueSaidLine(ctx.cueSaid("Review your teammate docs's work with a fork"), "L:1", "Review your teammate docs's work with a fork", "2m ago", { name: "docs", text: "1. d.md:1 — note — fine\nRecommendation: commit", team: world.sessions.L.team });
assert.match(folded, /<note L:1\|What Cue sent\|[^>]*><note L:1:review\|docs reviewed: commit · 1 note\|1. d.md:1 — note — fine\nRecommendation: commit><\/div>/, "the review behind its one line");
assert.doesNotMatch(folded, /data-mt-act|data-crew-open|Open ›/, "no buttons in the chat: they're on its row");

// ---- a finished member whose files Cue never saw (shell commands outside the lead's folder); one the hub says needs you ----
{
  world.sessions.U = { session_id: "U", team: [
    { call: "c-u1", agent_id: "u1", name: "board", state: "reviewed", recommendation: "commit", started_ms: NOW - 9 * M, files: [] },
    { call: "c-u2", agent_id: "u2", name: "mock", state: "done", started_ms: NOW - 9 * M, ended_ms: NOW - M, files: [] },
    { call: "c-u3", agent_id: "u3", name: "fixer", state: "reviewed", recommendation: "send fixes", findings: ["1. a — should-fix — b"], started_ms: NOW - 9 * M, files: [] },
    { call: "c-u4", agent_id: "u4", name: "quiet", state: "stopped", started_ms: NOW - 9 * M, files: [] },
    { call: "c-u5", agent_id: "u5", name: "lint", state: "done", started_ms: NOW - 9 * M, files: ["a.js"], stat: "1 file +2 −0", needs: "Its lead didn't say it committed it" },
  ] };
  // Files Cue never saw alone don't wait on you (Cue handles them): no tray opening by itself, no line.
  world.sessions.V = { session_id: "V", team: [{ call: "c-v", agent_id: "v1", name: "solo", state: "done", started_ms: NOW - 9 * M, files: [] }] };
  ctx.crewTray("V");
  assert.equal(ctx.mateOpenLeads.has("V"), false, "files unknown: not waiting on you, stays closed");
  assert.equal(ctx.crewWaitLine("V"), "");
  delete world.sessions.V;
  ctx.crewTray("U");
  assert.equal(ctx.mateOpenLeads.has("U"), true, "they wait on you: the tray opens by itself");
  ctx.mateDoneOpen.add("U");
  const d = ctx.crewTray("U");
  const rowOf = (name) => { const at = d.indexOf(`>${name}</button>`), next = d.slice(at).search(/<div class="mt-(row|one|grp|done)/); return d.slice(at, next < 0 ? undefined : at + next); };
  assert.match(rowOf("board"), /<i>clean review · commit it<\/i>.*class="btn mt-b primary "[^>]*data-mt-act="commit"[^>]*>Commit<\/button><\/div>/s, "reviewed, files unknown: Commit (its lead commits them)");
  assert.match(d, /board<\/button>.*<div class="mt-l">Cue doesn't know which files it changed: Commit asks its lead to commit them<\/div>/s, "says why");
  assert.match(rowOf("mock"), /8m · Cue doesn't know which files it changed: Commit asks its lead to commit them<\/span><button class="mt-lk "[^>]*data-mt-act="commit"[^>]*>Commit</, "done, files unknown: folded, Commit, not Diff");
  assert.doesNotMatch(rowOf("mock"), /data-mt-act="diff"/);
  assert.match(rowOf("fixer"), /<i>1 finding · fixes go back, round 1 of 2<\/i>.*data-mt-act="fixes"[^>]*>Send fixes</s, "its review asks for fixes: Send fixes, one button");
  assert.doesNotMatch(rowOf("quiet"), /data-mt-act=/, "stopped having written nothing: no button (Cue clears it)");
  assert.match(d, /<div class="mt-grp red">Needs you<\/div><div class="mt-row"[^>]*><div class="mt-n">[^]*>lint<\/button><i>Its lead didn't say it committed it<\/i>/, "the hub's needs: under Needs you, what it needs");
  assert.match(d, /<span class="crew-need">· lint needs you<\/span> <span class="crew-mu">· 2 ready<\/span>/, "needs counts in needs you");
  assert.doesNotMatch(d, /aside/i, "no Set aside anywhere");
  delete world.sessions.U;
}
