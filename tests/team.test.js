// Run: node tests/team.test.js  (a hosted lead's team in its chat: ui/team.js)
const assert = require("assert");
const fs = require("fs");
const vm = require("vm");

const esc = (s) => String(s ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const NOW = 1793984400000, M = 60000;
// Settings as config.rs effective gives them: One-way here (one_way.rs view), nothing set, so the default.
const ONE_WAY = { set: {}, default: ["main"], kinds: [["main", "Merge or push to main"], ["push", "Push any branch"], ["release", "A release"], ["deploy", "Deploy"], ["data", "Delete data"], ["outside", "Files outside the project"]].map(([key, label]) => ({ key, label })) };
const world = { sessions: {}, items: [], settings: { project: { oneway: ONE_WAY } } };
const ctx = {
  esc,
  get state() { return { items: world.items, settings: world.settings }; },
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
  // Ask me (every review waits under Ready) unless a test says Handle it.
  crewCommit: () => world.commit || "wait",
  ago: (ms) => `${Math.round((NOW - ms) / M)}m`,
  renderMain() {},
  toast() {},
  hhmm: (ms) => `T${Math.round((NOW - ms) / M)}`,
  addOn: {},
};
vm.createContext(ctx);
vm.runInContext(fs.readFileSync(__dirname + "/../ui/team.js", "utf8").replace(/^(const|let) /gm, "var "), ctx);
vm.runInContext(fs.readFileSync(__dirname + "/../ui/team_page.js", "utf8").replace(/^(const|let) /gm, "var "), ctx);

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
assert.match(g, /<div class="crew-body" data-scroll-key="crew:L"><div class="crew-goal">.*<div class="mt-grp red">Needs you · 1<\/div>.*<div class="mt-grp ">Ready<\/div>.*<div class="mt-grp ">Working<\/div>.*<div class="mt-done " data-mt-for="done:L"><button class="mt-lk mt-done-line" data-mt-done="L" aria-expanded="false">2 done ›<\/button><\/div>/s, "Needs you, Ready, Working, then the finished ones (done, and committed in the last hour: not old) folded into N done ›");
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
assert.match(g, /handoff<\/button><i>2 findings · fixes go back, round 1 of 1<\/i><\/div><div class="mt-btns"><button class="btn mt-b primary "[^>]*data-mt-act="fixes"[^>]*>Send fixes<\/button><\/div><ol class="mt-fx"><li><code>hub\/handoff.rs:40<\/code> the list is cut at 8 <em>should fix<\/em><\/li><li><code>ui\/core.js:212<\/code> commit after rows <em>note<\/em><\/li><\/ol>/, "one button that agrees with its line; the findings a plain numbered list");
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
  assert.match(ctx.crewTray("L"), /handoff<\/button><i>2 findings · fixes go back, round 1 of 1<\/i>.*>Send fixes</s, "said neither, with findings: they go back");
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
  assert.match(rowOf("fixer"), /<i>1 finding · fixes go back, round 1 of 1<\/i>.*data-mt-act="fixes"[^>]*>Send fixes</s, "its review asks for fixes: Send fixes, one button");
  assert.doesNotMatch(rowOf("quiet"), /data-mt-act=/, "stopped having written nothing: no button (Cue clears it)");
  assert.match(d, /<div class="mt-grp red">Needs you · 1<\/div><div class="mt-row"[^>]*><div class="mt-n">[^]*>lint<\/button><i>Its lead didn't say it committed it<\/i>/, "the hub's needs: under Needs you, what it needs");
  assert.match(d, /<span class="crew-need">· lint needs you<\/span> <span class="crew-mu">· 2 ready<\/span>/, "needs counts in needs you");
  assert.doesNotMatch(d, /aside/i, "no Set aside anywhere");
  delete world.sessions.U;
}

// ---- what Cue does for a member that waits on its lead: on the member's row, never in the chat ----
{
  world.sessions.W = { session_id: "W", team: [
    { call: "c-w1", agent_id: "w1", name: "queue", state: "queued", started_ms: NOW - 9 * M, ended_ms: NOW - M, files: ["a.js"] },
    { call: "c-w2", agent_id: "w2", name: "board", state: "reviewed", recommendation: "commit", commit_wait_ms: NOW - M, started_ms: NOW - 9 * M, files: [] },
    { call: "c-w3", agent_id: "w3", name: "site", state: "done", commit_asked_ms: NOW - M, started_ms: NOW - 9 * M, ended_ms: NOW - 2 * M, files: [] },
    { call: "c-w4", agent_id: "w4", name: "lost", state: "done", step: "Its review didn't come back", summary: "Board columns", started_ms: NOW - 9 * M, ended_ms: NOW - 2 * M, files: [] },
    { call: "c-w5", agent_id: "w5", name: "notes", state: "reviewed", recommendation: "commit", findings: ["1. a.md:1 — note — verified fine"], started_ms: NOW - 9 * M, files: ["a.md"] },
  ] };
  ctx.mateOpenLeads.add("W");
  ctx.mateDoneOpen.add("W");
  const d = ctx.crewTray("W");
  const rowOf = (name) => { const at = d.indexOf(`>${name}</button>`), next = d.slice(at).search(/<div class="mt-(row|one|grp|done)/); return d.slice(at, next < 0 ? undefined : at + next); };
  assert.match(rowOf("queue"), /<span class="mt-line">8m · review waits for the lead<\/span><button class="mt-lk" data-act="unqueue-review" data-exec="team:W:w1">Skip<\/button>/, "a queued review: its row says it waits, with Skip");
  assert.match(rowOf("board"), /<span class="mt-line">[^<]*Cue asks its lead to commit it when it's free<\/span><\/div>/, "a commit ask waiting for the lead: said on its row, no button");
  assert.match(rowOf("site"), /<span class="mt-line">[^<]*its lead commits it<\/span><\/div>/, "asked: its lead commits it");
  assert.match(d, /<div class="mt-grp ">Ready<\/div>.*<div class="mt-grp ">Waiting on the lead<\/div>.*>queue<.*>board<.*>site</s, "all three wait on the lead, after Ready, not on you");
  assert.doesNotMatch(d, /<div class="mt-grp ">Working<\/div>/, "none of them is at work");
  assert.match(d, /<span class="crew-dot"><\/span><span class="crew-w">Cue Crew · 5 <span class="crew-mu">· 1 ready · 3 waiting on the lead<\/span>/, "counted as waiting on the lead, not working; a still dot");
  assert.match(rowOf("lost"), /<span class="mt-line">7m · Its review didn't come back · Board columns/, "a review that never came: its row says so");
  assert.match(rowOf("notes"), /<i>1 note · commit it<\/i>.*data-mt-act="commit"[^>]*>Commit</s, "a commit with only a note: Commit");
  assert.deepEqual(ctx.mateWaits("W").map((t) => t.name), ["notes"], "only the reviewed one Cue isn't committing waits on you");
  delete world.sessions.W;
}

// ---- fixes kept for a busy lead (hub/team.rs send_fixes): it waits on the lead, not at work, not on you ----
{
  world.sessions.K = { session_id: "K", team: [
    { call: "c-k1", agent_id: "k1", name: "ops", state: "reviewed", recommendation: "send fixes", findings: ["1. a.js:1 — should-fix — x"], fixes_kept_ms: NOW - M, fix_rounds: 1, step: "Fixes go to its lead when its turn ends", started_ms: NOW - 9 * M, ended_ms: NOW - 2 * M, files: ["a.js"] },
    { call: "c-k2", agent_id: "k2", name: "docs", state: "queued", started_ms: NOW - 9 * M, ended_ms: NOW - M, files: ["b.md"] },
  ] };
  ctx.mateOpenLeads.add("K");
  const d = ctx.crewTray("K");
  assert.match(d, /<span class="crew-dot"><\/span><span class="crew-w">Cue Crew · 2 <span class="crew-mu">· 2 waiting on the lead<\/span>/, "kept fixes: waiting on the lead, not working, not ready; a still dot");
  assert.match(d, />ops<\/button><span class="mt-line">7m · its fixes go to its lead when its turn ends<\/span>/, "its row says its fixes wait for the lead");
  assert.equal(ctx.crewWaitLine("K"), "", "neither at work nor waiting on you: no 'Crew working', no 'Waiting on you'");
  assert.deepEqual(ctx.mateWaits("K").map((t) => t.name), []);
  delete world.sessions.K;
}

// ---- Handle it, the goal, and the crew's decisions and steps (crewGoalHtml, crewLogHtml) ----
{
  world.commit = "auto";
  world.sessions.H = { session_id: "H", project: "api", crew_job: "Rate limits on the public API, per key", team: [
    mate("auth", "reviewed", { recommendation: "send fixes", findings: ["1. a.rs:1 — should-fix — x"], files: ["a.rs"] }),
    mate("docs", "reviewed", { recommendation: "commit", files: ["d.md"], review: "1. d.md:1 — note — fine\nRecommendation: commit" }),
    mate("keys", "reviewed", { recommendation: "send fixes", findings: ["1. k.rs:1 — should-fix — y"], fix_rounds: 1, files: ["k.rs"] }),
  ], crew_log: [
    { id: "d1-0", at_ms: NOW - 40 * M, who: "lead", kind: "split", what: "Split the job into 3: auth, docs, keys", why: "", one_way: false, sha: "", back: "" },
    { id: "d2-0", at_ms: NOW - 4 * M, who: "docs", kind: "commit", what: "Reviewed: 1 note, left as is. Committed to `crew/docs`", why: "", one_way: false, sha: "3f2a1c", back: "" },
    { id: "d3-0", at_ms: NOW - M, who: "lead", kind: "merge", what: "Merged auth into main", why: "you said yes", one_way: true, sha: "", back: "" },
    { id: "d4-0", at_ms: NOW - 2 * M, who: "keys", kind: "hold", what: "Held until auth lands", why: "", one_way: false, sha: "", back: "asked" },
    { id: "d5-0", at_ms: NOW - 20 * M, who: "lead", kind: "decided", what: "Kept per-key limits over per-IP", why: "keys are what the docs promise", one_way: false, sha: "", back: "" },
  ] };
  ctx.mateOpenLeads.add("H");
  let d = ctx.crewTray("H");
  assert.match(d, /<div class="crew-goal"><div class="cg-box"><div class="cg-l">The job<\/div><div class="cg-t">Rate limits on the public API, per key<\/div>.*May do alone<\/div><div class="cg-t">Review, send fixes, commit to crew\/ branches<\/div>/s, "the goal on top: the job, and what Cue may do alone");
  assert.doesNotMatch(d, /data-mt-act="fixes" data-mt-sid="H" data-mt-agent="id-auth"|data-mt-act="commit" data-mt-sid="H" data-mt-agent="id-docs"/, "Handle it: no Send fixes or Commit for the ones Cue handles");
  assert.match(d, /Needs you · 1<\/div>.*>keys<\/button><i>1 finding · fixes go back, round 2<\/i>.*>Send fixes</s, "out of fix rounds: it needs you, with its button");
  assert.match(d, />auth<\/button><span class="mt-line">9m · reviewed · Cue sends its fixes back<\/span>/, "Cue handles auth: one line");
  assert.deepEqual(ctx.mateWaits("H").map((t) => t.name), ["keys"], "only keys waits on you");
  assert.match(d, /Asks you first<\/div><div class="cg-t">Merge or push to main, real choices<\/div>/, "asks first: its project's one-way kinds (never set: the default)");
  assert.match(d, /Needs you · 1<\/div>/);
  assert.match(d, /Decisions and steps · 5<\/div><div class="cd-row[^"]*"[^>]*><b class="cd-who">lead<\/b><span class="cd-what">Merged auth into main <span class="cd-why">· you said yes<\/span><\/span><span class="cd-at">1m<\/span><span class="cd-one">one-way ·<\/span><button class="mt-lk" data-act="crew-back" data-sid="H" data-id="d3-0">Make a revert<\/button>/, "newest first; one-way offers a revert");
  assert.match(d, /Committed to <code>crew\/docs<\/code><\/span><span class="cd-at">4m<\/span><button class="mt-lk" data-act="crew-back" data-sid="H" data-id="d2-0">Undo this<\/button>/, "code in backticks; Undo this");
  assert.match(d, /<div class="cd-row  step  can"[^>]*><b class="cd-who">keys<\/b><span class="cd-what">Held until auth lands<\/span><span class="cd-at">2m<\/span><\/div>/, "activity: a quiet step, no Undo (an old log's \"asked\" too)");
  assert.match(d, /<div class="cd-row  step  "[^>]*><b class="cd-who">lead<\/b><span class="cd-what">Split the job into 3: auth, docs, keys<\/span><span class="cd-at">40m<\/span><\/div>/, "a teammate started: a step");
  assert.match(d, /<div class="cd-row  kinded  "[^>]*><b class="cd-who">lead<\/b><span class="cd-what">Kept per-key limits over per-IP <span class="cd-why">· keys are what the docs promise<\/span><\/span><span class="cd-at">20m<\/span><button class="mt-lk" data-act="crew-back" data-sid="H" data-id="d5-0">Undo this<\/button>/, "a judgment call: Undo this");
  assert.equal((d.match(/data-act="crew-back"/g) || []).length, 3, "Undo only on the commit, the merge and the call");
  // On the crew's page: what can be undone is a card saying what it is; activity a quiet line.
  const big = (id) => ctx.crewLogRow("H", world.sessions.H.crew_log.find((x) => x.id === id), true);
  assert.match(big("d5-0"), /^<div class="cd-row big kinded  "[^>]*><span class="cd-kind">Decided<\/span><div class="cd-main"><div class="cd-what">Kept per-key limits over per-IP<\/div><div class="cd-sub"><span class="cd-tag">lead<\/span><span class="cd-why">keys are what the docs promise · 20m<\/span><\/div><\/div><button class="btn small cd-back"[^>]*>Undo this<\/button><\/div>$/);
  assert.match(big("d2-0"), /<span class="cd-kind">Committed<\/span>.*>Undo this<\/button>/);
  assert.equal(big("d1-0"), '<div class="cd-row big step  " data-key="cd:H|d1-0" ><span class="cd-who">lead</span><span class="cd-what">Split the job into 3: auth, docs, keys</span><span class="cd-why">40m</span></div>');
  // A project that counts a push of any branch and a deploy one-way, and one that counts nothing.
  world.settings.project.oneway = { ...ONE_WAY, set: { api: ["push", "deploy"] } };
  assert.match(ctx.crewTray("H"), /Asks you first<\/div><div class="cg-t">Push any branch, deploy, real choices</);
  world.settings.project.oneway = { ...ONE_WAY, set: { api: [] } };
  assert.match(ctx.crewTray("H"), /Asks you first<\/div><div class="cg-t">Real choices</);
  world.settings.project.oneway = ONE_WAY;
  assert.doesNotMatch(d, /data-crew-log="H\|d1-0"/, "the lead's line has no member to open");
  ctx.crewLogOpen.add("H|d2-0");
  d = ctx.crewTray("H");
  assert.match(d, /data-crew-log="H\|d2-0" aria-expanded="true">.*<div class="mt-under cd-under" data-mt-for="log:H\|d2-0"><div class="mt-review msg">1\. d\.md:1[^]*Committed <code>3f2a1c<\/code> · 1 file<\/span><button class="mt-lk "[^>]*data-mt-act="diff"/s, "opened: its review, its commit and Diff");
  ctx.crewBackAsked.add("H|d2-0");
  assert.match(ctx.crewTray("H"), /crew\/docs<\/code><\/span><span class="cd-at">4m<\/span><span class="cd-going">undoing…/, "just clicked: undoing… at once");
  world.commit = "wait";
  d = ctx.crewTray("H");
  assert.match(d, /May do alone<\/div><div class="cg-t">Nothing: it asks you first/, "Ask me: it does nothing alone");
  assert.match(d, /<div class="mt-grp ">Ready<\/div>.*data-mt-agent="id-auth"[^>]*>Send fixes<.*Decisions and steps · 5/s, "Ask me: Ready with its buttons, then the decisions");
  delete world.commit;
  delete world.sessions.H;
}

// ---- the crew's page: what a lane says a member does now, and its meter (team_page.js crewNow) ----
{
  world.sessions.N = { session_id: "N", team: [
    mate("auth", "working", { step: "Running: npm run migrate", step_ms: NOW - 2 * M }),
    mate("api", "working", { step: "Running: npm test -- keys", step_ms: NOW - 3 * M }),
    mate("web", "working", { step: "Editing ui/keys/usage.tsx", step_ms: NOW - M }),
    mate("docs", "working", { fix_rounds: 1, step: "Editing docs/limits.md", findings: ["1. docs/limits.md:12 — should-fix — Retry-After is in seconds, not ms", "2. docs/limits.md:30 — note — link the Keys page"] }),
    mate("cli", "committed", { commit: "3f2a9c1e", recommendation: "commit", ended_ms: NOW - 6 * M }),
    mate("hub", "reviewed", { findings: ["1. a.rs:1 — note — fine"], recommendation: "commit" }),
    mate("quiet", "working", { step: "", step_ms: 0 }),
    mate("gone", "done", { summary: "Wrote it; 3 tests pass" }),
    mate("held", "done", { needs: "Its review didn't come back" }),
  ], crew_log: [
    { id: `d${NOW - 9 * M}-0`, at_ms: NOW - 6 * M, who: "cli", call: "call-cli", kind: "commit", what: "Reviewed: clean. Committed", head: "reviewed: clean", steps: ["committed `crew/cli`"] },
    { id: `d${NOW - 5 * M}-0`, at_ms: NOW - 4 * M, who: "docs", call: "call-docs", kind: "fixes", what: "Reviewed: 1 should-fix, 1 note. Sent the fix back", head: "reviewed: 1 should-fix, 1 note", steps: ["sent back", "left the note as is"] },
  ] };
  world.items.push({ id: "q1", session_id: "N", kind: "permission", agent_id: "id-auth", cmd: "npm run migrate" });
  ctx.verb = () => "wants to run";
  const at = (n) => ctx.crewNow("N", world.sessions.N.team.find((t) => t.name === n));
  const pick = (n) => { const x = at(n); return [x.now, x.sub, x.at, x.look, x.word]; };
  assert.deepEqual(pick("auth"), ["Wants to run `npm run migrate`", "", 0, "need", "needs you"], "asks you: what it wants, red at its stage; no context the data doesn't have");
  assert.equal(at("auth").ask, true);
  world.sessions.N.team[0].step = "Running: npm test";
  assert.equal(at("auth").sub, "Running `npm test`", "asks you while on another step: that step, in the same form as elsewhere");
  world.sessions.N.team[0].step = "Running: npm run migrate";
  assert.deepEqual(pick("api"), ["Running `npm test -- keys`", "on it since T3", 1, "now", "checking"], "a test run is checking");
  assert.deepEqual(pick("web"), ["Editing `ui/keys/usage.tsx`", "on it since T1", 0, "now", "working"], "an edit is working");
  const docs = at("docs");
  assert.deepEqual([docs.now, docs.at, docs.word, docs.fix], ["Fixing what review found", 3, "fixing", ["should-fix", "docs/limits.md:12 — Retry-After is in seconds, not ms"]], "sent back: fixing its first should-fix");
  assert.deepEqual(pick("cli"), ["Committed `crew/cli` 3f2a9c1", "review: clean · ended T6", 4, "done", "committed"], "committed: its branch from the log, its commit, when it ended");
  assert.deepEqual(pick("hub"), ["Reviewed: 1 note", "waits for you", 2, "done", "reviewed"], "reviewed: what it found, who it waits on");
  assert.deepEqual(pick("quiet"), ["Working", "", 0, "now", "working"], "no step yet: just working");
  assert.deepEqual(pick("gone"), ["Done", "Wrote it; 3 tests pass", 0, "done", "done"], "done: its last words");
  assert.deepEqual(pick("held"), ["Its review didn't come back", "", 0, "need", "needs you"], "needs you: what the hub says");
  // The meter: stages before its own done, its own as it is.
  assert.equal(ctx.crewMeter(docs), '<span class="cp-meter"><span class="cp-m" aria-hidden="true"><b class="done"></b><b class="done"></b><b class="done"></b><b class="now"></b><b class=""></b></span><span class="cp-ml">fixing</span></span>');
  const t = world.sessions.N.team.find((m) => m.name === "docs");
  // Its card in the page's list: name, its stage in a word and meter, what it was asked, what it does now; its steps,
  // what its review found and its files; its files and how long at its foot. Its top is one button: it opens its menu.
  // At its foot the drawer's one action for it (mateOneAct): Stop at work, Resume first when stuck, Skip while its
  // review waits.
  t.asked = "Document the limits"; t.files = ["docs/limits.md"];
  const row = ctx.crewListRow("N", t), top = row.slice(0, row.indexOf("</button>"));
  assert.doesNotMatch(row, /title=/, "no hover text");
  assert.match(row, /^<div class="cp-card [^"]*" data-key="cpc:N\|id-docs"><button class="cp-row" data-key="cp:N\|id-docs" data-mt-more="N\|id-docs" aria-expanded="false">/, "its top is the one button: it opens its menu");
  assert.equal((top.match(/<button/g) || []).length, 1, "nothing else to press in it");
  assert.match(row, /<div class="cp-acts"><span class="cp-r">1 file · 9m<\/span><button class="mt-lk " data-mt-act="stop" data-mt-sid="N" data-mt-agent="id-docs" >Stop<\/button><\/div><\/div>$/, "at work: its files and time, and Stop");
  assert.match(ctx.crewListRow("N", { ...t, quiet: "stuck" }), /data-mt-act="resume"[^>]*>Resume<\/button><button[^>]*data-mt-act="stop"/, "stuck: Resume, then Stop");
  assert.match(ctx.crewListRow("N", { ...t, state: "queued" }), /<div class="cp-acts"><span class="cp-r">[^<]*<\/span><button class="mt-lk" data-act="unqueue-review"[^>]*>Skip<\/button><\/div>/, "its review waiting: Skip");
  assert.match(row, /<span class="cp-nm">docs<\/span><span class="cp-meter">.*<span class="cp-ml">fixing<\/span>/, "its name, its meter and its stage");
  assert.match(row, /<span class="cp-ask1">Document the limits<\/span><span class="cp-now ">Fixing what review found<\/span><\/button>/, "what it was asked, then what it's doing now; the finding it fixes is in its Review");
  assert.match(row, /<span class="cp-pl">Steps<\/span><span class="cp-pv"><span class="cp-st now"><span class="cp-t"><\/span><span class="cp-sw">Editing <code>docs\/limits\.md<\/code><\/span><\/span><\/span>/, "no feed of its own: its step now (no time known)");
  assert.match(ctx.crewListRow("N", world.sessions.N.team.find((m) => m.name === "api")), /<span class="cp-st now"><span class="cp-t">T3<\/span><span class="cp-sw">Running <code>npm test -- keys<\/code><\/span><\/span>/, "with the time it started it");
  assert.match(row, /<span class="cp-pl">Review<\/span><span class="cp-pv"><span class="cp-fl"><span class="cp-f fix">should-fix<\/span>docs\/limits\.md:12 — Retry-After[^<]*<\/span>/, "what review found: its kind, its words");
  assert.match(row, /<span class="cp-pl">Files<\/span><span class="cp-pv"><span class="cp-fn"><code>limits\.md<\/code><\/span><\/span>/, "its files by their own names");
  assert.match(ctx.crewListRow("N", { ...t, findings: [], recommendation: "", state: "working", fix_rounds: 0 }), /<span class="cp-pl">Review<\/span><span class="cp-pv"><span class="cp-dim">not yet<\/span>/, "no review yet");
  assert.match(ctx.crewListRow("N", world.sessions.N.team[0]), /class="cp-card  q /, "one that asks you is marked to show red");
  assert.match(ctx.crewListRow("N", world.sessions.N.team[0]), /<span class="cp-ml">needs you<\/span>/, "its word: needs you");
  world.items = world.items.filter((i) => i.id !== "q1");
  delete world.sessions.N;
}

// ---- the crew's page: the list in the crew's order, whatever each one's state; the finished ones folded at its end;
// Needs you shows its first row, "N more ›" the rest ----
{
  world.sessions.P = { session_id: "P", state: "working", since_ms: NOW - M, harness: "claude", team: [
    mate("api", "working", { step: "Editing api.rs" }),
    mate("web", "reviewed", { findings: [], recommendation: "commit" }),
    mate("auth", "working", { step: "Editing auth.rs", needs: "Out of fix rounds" }),
    mate("docs", "committed", { ended_ms: NOW - 2 * M }),
    mate("spike", "aside"),
  ] };
  const keys = () => [...ctx.crewRowsHtml("P").matchAll(/class="cp-row" data-key="cp:P\|id-(\w+)"/g)].map((m) => m[1]);
  ctx.mateDoneOpen.delete("P");
  assert.deepEqual(keys(), ["api", "web", "auth"], "the crew's own order: auth, which needs you, isn't pulled to the top");
  assert.match(ctx.crewRowsHtml("P"), /data-mt-done="P" aria-expanded="false">2 done ›<\/button><\/div>$/, "committed and set aside folded into one line at the list's end");
  ctx.mateDoneOpen.add("P");
  assert.deepEqual(keys(), ["api", "web", "auth", "docs", "spike"], "unfolded: the finished ones under it, in their order");
  assert.match(ctx.crewRowsHtml("P"), /class="cp-card past  "/, "dimmed");
  ctx.mateDoneOpen.delete("P");
  // A change of state moves nothing.
  world.sessions.P.team[0].state = "reviewed";
  assert.deepEqual(keys(), ["api", "web", "auth"], "a row never moves for its state");
  // The list's line: what needs you first, in red; no time since it started.
  assert.match(ctx.crewStatusLine("P", 2), /The crew · 5<\/span><span class="cp-count"><span class="cp-q">2 need you<\/span> · 1 working · 1 committed<\/span>/);
  assert.doesNotMatch(ctx.crewStatusLine("P", 0), /need|since/);
  // Needs you: nothing at all when nothing needs you; one row and "N more ›" when more do; open, all of them.
  // Only the rows shown are drawn: none hidden.
  const rows3 = ["<r1/>", "<r2/>", "<r3/>"];
  assert.equal(ctx.crewNeedsBox("P", []), "", "nothing needs you: no box");
  assert.doesNotMatch(ctx.crewNeedsBox("P", ["<r1/>"]), /more/, "one: no more");
  assert.match(ctx.crewNeedsBox("P", rows3), /^<div class="cp-needs "><div class="cp-nh"><span class="mt-grp red">Needs you · 3<\/span><span class="grow"><\/span><button class="mt-lk cp-nmore" data-cp-needs="P" aria-expanded="false">2 more ›<\/button><\/div><div class="cp-nrows"><r1\/><\/div><\/div>$/, "three: the first only, and 2 more ›");
  ctx.crewNeedsAll.add("P");
  assert.match(ctx.crewNeedsBox("P", rows3), /class="cp-needs all".*Fewer ‹<\/button><\/div><div class="cp-nrows"><r1\/><r2\/><r3\/><\/div>/, "open: all of them");
  ctx.crewNeedsAll.delete("P");
  // An add-on that still gives its rows as one piece of html: that piece is one row, the count its own.
  assert.match(ctx.crewNeedsBox("P", ["<r1/>", "<a1/><a2/>"], 3), /Needs you · 3.*2 more ›.*<div class="cp-nrows"><r1\/><\/div>/, "the count stays true");
  delete world.sessions.P;
}
