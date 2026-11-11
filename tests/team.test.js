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
  mate("skill", "done", { ended_ms: NOW - 2 * M, summary: "team skill written, 12 tests pass", stat: "2 files +140 −0" }),
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
assert.match(ctx.crewStrip("L"), /data-mt-open="L" aria-expanded="false"><span class="dot-live"><\/span>Teammates · 2 working · <span class="crew-need">1 needs you<\/span> · 1 ready<span class="tm-sub">▴/, "a live dot and how many work now, who needs you, who's ready");
assert.equal(ctx.crewDock("L"), "", "closed: no rows above the box");
const idle = world.sessions.L.team.map((t) => t.state);
world.sessions.L.team.forEach((t) => { if (t.state === "working") t.state = "done"; });
assert.match(ctx.crewStrip("L"), />Teammates 4 · /, "none at work: how many, no dot");
assert.doesNotMatch(ctx.crewStrip("L"), /dot-live/);
world.sessions.L.team.forEach((t, i) => { t.state = idle[i]; });
assert.equal(ctx.crewStrip("nobody"), "", "no crew, no strip");

// ---- the rows, open above the box ----
ctx.mateOpenLeads.add("L");
assert.match(ctx.crewStrip("L"), /crew-strip on.*▾/s);
const g = ctx.crewDock("L");
assert.match(g, /class="crew-dock open".*mt-rows/s, "the rows above the box");
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
assert.match(row("handoff"), /class="btn primary held"[^>]*data-mt-act="commit"[^>]*aria-disabled="true"/, "Commit held while another teammate shares a file");
assert.match(row("handoff"), /data-mt-act="fixes"/);
assert.match(g, /Commit on <b>handoff<\/b> is held: <b>rows<\/b>/);
assert.doesNotMatch(g, /gone|away/, "committed and set aside: out of the team");
assert.doesNotMatch(g, /left:|border-left/, "no stripes");

// ---- findings fold open; the ⋯ menu ----
ctx.mateUnder.set("L|id-handoff", "findings");
const open = ctx.crewDock("L");
assert.match(open, /<span class="mono">hub\/handoff.rs:40<\/span> · the list is cut at 8 <span class="rv-k">should-fix<\/span>/);
ctx.mateMenu = "L|id-rows";
assert.match(ctx.crewDock("L"), /mt-menu"><button data-mt-act="diff"[^>]*>Diff<\/button><button data-mt-asked="L\|id-rows">What it was asked<\/button><button class="mt-aside" data-mt-act="aside"/, "working: Diff in ⋯");

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
assert.match(line, /data-mt-act="commit"[^>]*>Commit</, "its current review: the button it calls for");
assert.match(line, /data-crew-open="L\|id-docs">Open ›/);
assert.doesNotMatch(ctx.reviewLine("L", "docs", "an older review\nRecommendation: send fixes"), /data-mt-act/, "an older review of it: no button");
