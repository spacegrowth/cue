// Run: node tests/agents.test.js  (agents of every kind as one: ui/agents.js)
const assert = require("assert");
const fs = require("fs");
const vm = require("vm");

const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const world = { helpers: {}, sessions: {}, items: [] };
const ctx = {
  esc,
  get state() { return { items: world.items }; },
  document: { addEventListener() {} },
  bgTabOn: new Map(),
  helpersOf: (sid) => world.helpers[sid] || [],
  helperRunning: (h) => !!h.running,
  sessionOf: (sid) => world.sessions[sid] || null,
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
  verb: () => "wants to run",
  isBash: () => true,
  plain: (s) => s,
  summary: (it) => it.cmd,
};
vm.createContext(ctx);
const load = (src) => vm.runInContext(src.replace(/^(const|let) /gm, "var "), ctx);
load(fs.readFileSync(__dirname + "/../ui/agents.js", "utf8"));
load(fs.readFileSync(__dirname + "/../ui/team.js", "utf8"));

// ---- one state vocabulary ----
const S = ctx.agentState;
assert.equal(S({ running: true }), "working");
assert.equal(S({ running: true, stuck: true }), "stuck", "a helper quiet for minutes is stuck");
assert.equal(S({ running: false }), "done", "a finished background agent is done");
assert.equal(S({ running: false, teammate: true }), "idle", "a teammate between tasks is idle");
assert.equal(S({ running: false, stopped: true }), "stopped");
assert.equal(S({ running: true, asks: true }), "needs", "asking you beats everything");
assert.deepEqual(Object.keys(ctx.AGENT_WORDS), ["needs", "working", "stuck", "done", "idle", "stopped"]);
assert.equal(ctx.AGENT_WORDS.needs, "needs you");

// ---- a session with 1 teammate + 1 background agent (and a script, which isn't an agent) ----
world.helpers.L = [
  { id: "t1", teammate: true, name: "reviewer", desc: "review it", running: false, started_ms: 100, last_ms: 900, items: [], latest: "Sent findings" },
  { id: "a1", kind: "Explore", desc: "Inventory agent UIs", running: true, started_ms: 200, tokens: 68000, items: [], latest: "Read ui/background.js" },
  { id: "s1", script: true, desc: "npm test", running: true, started_ms: 300 },
];
world.sessions = { L: { session_id: "L", agents: 2 } };

let list = ctx.agentsOf("L");
assert.deepEqual(list.map((a) => [a.name, a.kind, a.type, a.state]), [
  ["reviewer", "teammate", "crew", "idle"],
  ["Inventory agent UIs", "agent", "Explore", "working"],
], "one list: teammates and background agents; scripts are not agents");
let g = ctx.agentGroups(list);
assert.deepEqual(g.needs.map((a) => a.name), []);
assert.deepEqual(g.working.map((a) => a.name), ["Inventory agent UIs"]);
assert.deepEqual(g.done.map((a) => a.name), ["reviewer"], "done, idle and stopped together");

// One count everywhere: the title's, the tab's and the cards' come from agentCount, and match the list.
assert.equal(ctx.agentCount("L").n, list.length, "the count is the list's length");
// The title line: no agent chips while nothing needs you, just "2 agents ›" (opens the Agents tab).
let h = ctx.agentChips("L");
assert.ok(h.includes("2 agents") && h.includes('data-bg-tab="agents"'), "N agents › opens the Agents tab");
assert.ok(!h.includes("needs you"), "no chip for an agent that doesn't need you");

// The row above the box: working ones only (an idle teammate is in the Agents tab), then scripts.
h = ctx.agentTray("L");
assert.ok(h.includes("Inventory agent UIs"), "working agents");
assert.ok(!h.includes("reviewer"), "idle ones aren't in the row");
assert.ok(h.includes('data-helper="L|a1"'), "a helper row opens its view");
assert.ok(h.indexOf("Inventory agent UIs") < h.indexOf("<script-row s1>"), "running scripts after the agents");
assert.ok(!h.includes("more · Agents"), "four or fewer: no more link");
assert.ok(ctx.agentPill("L").includes("2 agents"), "a card says N agents, every kind: the title's count");

// A background agent asks (Claude Code names it on the hook: the item's agent_id): it needs you, on its own row,
// first, with Deny / Allow; the title and the card name it.
world.items = [{ id: "I2", session_id: "L", kind: "permission", cmd: "rm -rf target", agent_id: "a1", agent_name: "Inventory agent UIs" }];
list = ctx.agentsOf("L");
assert.equal(list.find((a) => a.id === "a1").state, "needs", "a background agent's ask: needs you");
assert.equal(list.find((a) => a.id === "t1").state, "idle", "the others are untouched");
h = ctx.agentTray("L");
assert.ok(h.includes('data-act="deny" data-id="I2"') && h.includes('data-act="allow" data-id="I2"'), "Deny / Allow inline");
assert.ok(ctx.agentChips("L").includes('data-big="I2"'), "its chip on the title line");
assert.deepEqual(ctx.agentCount("L"), { n: 2, needs: ["Inventory agent UIs"] }, "the card names it");
assert.ok(ctx.agentPill("L").includes("Inventory agent UIs") && ctx.agentPill("L").includes("needs you"), "a card names the agent that needs you");
world.items = [{ id: "I3", session_id: "L", kind: "permission", cmd: "ls" }];
assert.ok(ctx.agentsOf("L").every((a) => a.state !== "needs"), "the session's own ask isn't an agent's");
world.items = [];

// The Agents tab: grouped.
h = ctx.agentsPage("L", world.helpers.L);
assert.ok(h.indexOf("WORKING") < h.indexOf("DONE"), "grouped Working then Done");
assert.ok(h.includes("68000 tokens"), "tokens in the tab");

// More than four at work: four rows, then "N more · Agents ›".
world.helpers.L = Array.from({ length: 5 }, (_, i) => ({ id: `x${i}`, desc: `job ${i}`, running: true, started_ms: 10 + i, items: [] }));
h = ctx.agentTray("L");
assert.equal((h.match(/class="stx-line bg-row ag-row/g) || []).length, 4, "at most four rows");
assert.ok(h.includes("1 more · Agents ›"), "then how many more");

// One agent's view: the same head for every kind.
const a = ctx.agentsOf("L").find((x) => x.id === "x0");
h = ctx.agentHead(a, "<back>");
assert.ok(h.startsWith('<div class="bg-view-head ag-view-head"') && h.includes("<back>") && h.includes("job 0") && h.includes(">AGENT<") && h.includes("working") && h.includes("for L"), "‹ Back · mark · name · kind · state · for <session>");
console.log("agents ok");

// ---- a hosted lead's crew is in its strip, never also in its agents ----
world.sessions = { L: { session_id: "L", agents: 2, team: [{ agent_id: "x0", name: "job 0", state: "working" }] } };
assert.deepEqual(ctx.agentsOf("L").map((a) => a.id), ["x1", "x2", "x3", "x4"], "a crew member isn't listed again as an agent");
assert.equal(ctx.agentCount("L").n, 1, "nor counted in the agents chip while it works");
world.sessions.L.team[0].state = "committed";
assert.equal(ctx.agentCount("L").n, 1, "nor once it's finished: the count says what the list shows");
world.sessions = { L: { session_id: "L", agents: 6, team: ["a", "b", "c", "d", "e", "f"].map((n) => ({ agent_id: n, name: n, state: "reviewed" })) } };
assert.equal(ctx.agentCount("L").n, 0, "six finished crew members and nothing else: no agents to count");
