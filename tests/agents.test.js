// Run: node tests/agents.test.js  (agents of every kind as one: ui/agents.js, and a relay agent's "for <lead> ›" in ui/core.js)
const assert = require("assert");
const fs = require("fs");
const vm = require("vm");

const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const world = { helpers: {}, crew: {}, sessions: {}, items: [] };
const ctx = {
  esc,
  get state() { return { items: world.items }; },
  missedWake: () => false,
  document: { addEventListener() {} },
  bgTabOn: new Map(),
  helpersOf: (sid) => world.helpers[sid] || [],
  helperRunning: (h) => !!h.running,
  crewOf: (sid) => world.crew[sid],
  execOf: (sid) => (world.crew[sid]?.role === "executor" ? world.crew[sid] : null),
  teamOf: (sid) => { const m = world.crew[sid]; return m?.role === "executor" ? { lead: m.lead, leadName: m.lead_name, ex: [] } : null; },
  sessionOf: (sid) => world.sessions[sid] || null,
  asksOf: (sid) => world.items.find((i) => i.session_id === sid && i.kind !== "waiting") || null,
  preStart: () => "",
  liveStep: (sid) => (world.sessions[sid]?.doing ? { what: world.sessions[sid].doing, at: 1 } : null),
  nameOf: (sid) => sid,
  bgName: (h) => (h.teammate && h.name) || h.desc,
  bgLatest: (h) => h.latest || "",
  bgIcon: (h) => (h.teammate ? `<i class="stx-ic k-team"></i>` : `<i class="stx-ic k-bgagent">A</i>`),
  bgRow: (sid, h) => `<script-row ${h.id}>`,
  helperSpan: () => "1m 0s",
  helperTokens: (n) => (n ? `${n} tokens` : ""),
  helperQuiet: () => `<span class="bg-quiet">quiet · 4m</span>`,
  bgClock: () => "2:41pm",
  ago: () => "4m",
  pkt: (n) => (n ? `packet ${n}` : ""),
  exGo: (sid) => `data-session="${sid}"`,
  verb: () => "wants to run",
  isBash: () => true,
  plain: (s) => s,
  summary: (it) => it.cmd,
  diffBtn: (sid) => `<button data-crew="diff" data-crew-sid="${sid}">Diff</button>`,
  crewBtn: (sid, a, label) => `<button data-crew="${a}" data-crew-sid="${sid}">${label}</button>`,
  reviewWord: () => "",
};
vm.createContext(ctx);
const load = (src) => vm.runInContext(src.replace(/^(const|let) /gm, "var "), ctx);
load(fs.readFileSync(__dirname + "/../ui/agents.js", "utf8"));
const core = fs.readFileSync(__dirname + "/../ui/core.js", "utf8");
const cut = (from, to) => { const a = core.indexOf(from), b = core.indexOf(to, a); assert.ok(a > 0 && b > a, `core.js still has ${from}`); return core.slice(a, b); };
load(cut("/** On a relay agent's title line", "/** relay verify's verdict, in words"));

// ---- one state vocabulary ----
const S = ctx.agentState;
assert.equal(S({ running: true }), "working");
assert.equal(S({ running: true, stuck: true }), "stuck", "a helper quiet for minutes is stuck");
assert.equal(S({ running: false }), "done", "a finished background agent is done");
assert.equal(S({ running: false, teammate: true }), "idle", "a teammate between tasks is idle");
assert.equal(S({ running: false, stopped: true }), "stopped");
assert.equal(S({ relay: true, status: "busy" }), "working");
assert.equal(S({ relay: true, status: "busy", quiet: true }), "stuck", "a relay agent whose session went quiet is stuck");
assert.equal(S({ relay: true, status: "stalled" }), "stuck", "relay's stalled reads as stuck");
assert.equal(S({ relay: true, status: "reported" }), "done");
assert.equal(S({ relay: true, status: "idle" }), "done", "relay's idle is a reported packet: done");
for (const st of ["closed", "superseded", "dead"]) assert.equal(S({ relay: true, status: st }), "stopped", `relay's ${st} is stopped`);
assert.equal(S({ relay: true, status: "busy", asks: true }), "needs", "asking you beats everything");
assert.equal(S({ running: true, asks: true }), "needs");
assert.deepEqual(Object.keys(ctx.AGENT_WORDS), ["needs", "working", "stuck", "done", "idle", "stopped"]);
assert.equal(ctx.AGENT_WORDS.needs, "needs you");

// ---- a lead with 1 teammate + 1 background agent + 2 relay agents ----
world.helpers.L = [
  { id: "t1", teammate: true, name: "reviewer", desc: "review it", running: false, started_ms: 100, last_ms: 900, items: [], latest: "Sent findings" },
  { id: "a1", kind: "Explore", desc: "Inventory agent UIs", running: true, started_ms: 200, tokens: 68000, items: [], latest: "Read ui/background.js" },
  { id: "s1", script: true, desc: "npm test", running: true, started_ms: 300 },
];
world.crew.L = { role: "lead", name: "cue-relay", executors: [
  { session_id: "E1", name: "cue-build", status: "busy" },
  { session_id: "E2", name: "cue-docs", status: "reported", packet: 3, outcome: "Docs updated" },
] };
world.crew.E1 = { role: "executor", lead: "L", lead_name: "cue-relay", status: "busy" };
world.sessions = { L: { session_id: "L", agents: 2 }, E1: { session_id: "E1", since_ms: 50, doing: "xcodebuild" }, E2: { session_id: "E2", since_ms: 60 } };

let list = ctx.agentsOf("L");
assert.deepEqual(list.map((a) => [a.name, a.kind, a.type, a.state]), [
  ["reviewer", "teammate", "teammate", "idle"],
  ["Inventory agent UIs", "agent", "Explore", "working"],
  ["cue-build", "relay", "relay", "working"],
  ["cue-docs", "relay", "relay", "done"],
], "one list: teammates, background agents and relay agents; scripts are not agents");
let g = ctx.agentGroups(list);
assert.deepEqual(g.needs.map((a) => a.name), []);
assert.deepEqual(g.working.map((a) => a.name), ["cue-build", "Inventory agent UIs"], "working: oldest first");
assert.deepEqual(g.done.map((a) => a.name), ["reviewer", "cue-docs"], "done, idle and stopped together, newest first");

// One count everywhere: the title's, the tab's and the cards' come from agentCount (the session's agents as Cue
// counts its chat's list, plus its relay agents), and match the list.
assert.equal(ctx.agentCount("L").n, list.length, "the count is the list's length");
// The title line: no agent chips while nothing needs you, just "4 agents ›" (opens the Agents tab).
let h = ctx.agentChips("L");
assert.ok(h.includes("4 agents") && h.includes('data-bg-tab="agents"'), "N agents › opens the Agents tab");
assert.ok(!h.includes("needs you"), "no chip for an agent that doesn't need you");

// The row above the box: working ones only (an idle teammate and a done relay agent are in the Agents tab), then scripts.
h = ctx.agentTray("L");
assert.ok(h.includes("cue-build") && h.includes("Inventory agent UIs"), "working agents of every kind");
assert.ok(!h.includes("reviewer") && !h.includes("cue-docs"), "idle and done ones aren't in the row");
assert.ok(h.includes("k-relay") && h.includes(">R<"), "a relay agent's R mark");
assert.ok(h.includes('data-session="E1"') && h.includes('data-helper="L|a1"'), "a relay row opens its chat; a helper row its view");
assert.ok(h.indexOf("cue-build") < h.indexOf("<script-row s1>"), "running scripts after the agents");
assert.ok(!h.includes("more · Agents"), "four or fewer: no more link");

// A relay agent asks: its chip on the title line, and first in the row with Deny / Allow.
world.items = [{ id: "I1", session_id: "E1", kind: "permission", cmd: "rm -rf target" }];
list = ctx.agentsOf("L");
assert.equal(list.find((a) => a.id === "E1").state, "needs");
h = ctx.agentChips("L");
assert.ok(/cue-build<\/span> <span class="tm-sub">needs you/.test(h) && h.includes('data-big="I1"'), "<name> needs you, opening the ask");
h = ctx.agentTray("L");
assert.ok(h.indexOf("cue-build") < h.indexOf("Inventory agent UIs"), "the one that needs you comes first");
assert.ok(h.includes('data-act="deny" data-id="I1"') && h.includes('data-act="allow" data-id="I1"'), "Deny / Allow inline");
assert.deepEqual(ctx.agentCount("L"), { n: 4, needs: ["cue-build"] }, "the board's count names who needs you");
assert.ok(ctx.agentPill("L").includes("cue-build") && ctx.agentPill("L").includes("needs you"), "a card names the agent that needs you");
assert.ok(!ctx.agentPill("L", { count: false, tree: true }).includes("cue-build"), "a card holding the lead's tree leaves its relay agents to the tree");
world.items = [];
assert.ok(ctx.agentPill("L").includes("4 agents"), "a card says N agents, every kind: the title's count");

// A background agent asks (Claude Code names it on the hook: the item's agent_id): it needs you, on its own row,
// first, with Deny / Allow; the title and the card name it.
world.items = [{ id: "I2", session_id: "L", kind: "permission", cmd: "rm -rf target", agent_id: "a1", agent_name: "Inventory agent UIs" }];
list = ctx.agentsOf("L");
assert.equal(list.find((a) => a.id === "a1").state, "needs", "a background agent's ask: needs you");
assert.equal(list.find((a) => a.id === "t1").state, "idle", "the others are untouched");
h = ctx.agentTray("L");
assert.ok(h.indexOf("Inventory agent UIs") < h.indexOf("cue-build") && h.includes('data-act="allow" data-id="I2"'), "first, with Deny / Allow");
assert.ok(ctx.agentChips("L").includes('data-big="I2"'), "its chip on the title line");
assert.deepEqual(ctx.agentCount("L").needs, ["Inventory agent UIs"], "the card names it");
assert.ok(ctx.agentPill("L", { tree: true }).includes("Inventory agent UIs"), "the tree doesn't name the lead's own agents: its card's chip does");
world.items = [{ id: "I3", session_id: "L", kind: "permission", cmd: "ls" }];
assert.ok(ctx.agentsOf("L").every((a) => a.state !== "needs"), "the session's own ask isn't an agent's");
world.items = [];
assert.equal(ctx.agentPill("L", { count: false }), "", "a lead's card leaves the count to its crew line");

// The Agents tab: grouped, and a done relay agent's row carries Diff and Review.
h = ctx.agentsPage("L", world.helpers.L);
assert.ok(h.indexOf("WORKING") < h.indexOf("DONE"), "grouped Working then Done");
const docs = h.slice(h.indexOf("cue-docs") - 300, h.indexOf("cue-docs") + 600);
assert.ok(docs.includes('data-crew="diff" data-crew-sid="E2"') && docs.includes('data-crew="review" data-crew-sid="E2"'), "Diff and Review on a done relay row");
assert.ok(h.includes("68000 tokens"), "tokens in the tab");

// More than four at work: four rows, then "N more · Agents ›".
world.helpers.L = Array.from({ length: 5 }, (_, i) => ({ id: `x${i}`, desc: `job ${i}`, running: true, started_ms: 10 + i, items: [] }));
h = ctx.agentTray("L");
assert.equal((h.match(/class="stx-line bg-row ag-row/g) || []).length, 4, "at most four rows");
assert.ok(h.includes("2 more · Agents ›"), "then how many more (5 helpers + 1 relay agent at work, 4 shown; the done one not counted)");

// One agent's view: the same head for every kind.
const a = ctx.agentsOf("L").find((x) => x.id === "x0");
h = ctx.agentHead(a, "<back>");
assert.ok(h.startsWith('<div class="bg-view-head ag-view-head"') && h.includes("<back>") && h.includes("job 0") && h.includes(">AGENT<") && h.includes("working") && h.includes("for L"), "‹ Back · mark · name · kind · state · for <session>");

// A relay agent's own title line: "for <lead> ›", opening its lead's chat.
h = ctx.teamChips("E1");
assert.ok(h.includes('data-session="L"') && h.includes("for cue-relay"), "for <lead> ›");
delete world.sessions.L;
assert.ok(ctx.teamChips("E1").includes("lead · not running"), "its lead gone: said so, no link");
assert.equal(ctx.teamChips("L"), "", "a lead's chips are its agents' (agentChips), not this");
assert.ok(ctx.relayBack("E1") === "", "no ‹ Back when Cue doesn't have its lead");
world.sessions.L = { session_id: "L" };
assert.ok(ctx.relayBack("E1").includes('data-agent-back="L"'), "‹ Back to its lead's Agents tab");
console.log("agents ok");
