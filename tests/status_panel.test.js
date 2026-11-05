// Run: node tests/status_panel.test.js  (the status list above the chat, ui/status_panel.js)
const assert = require("assert");
global.esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
Object.assign(global, require("../ui/status.js"));
const store = new Map();
global.localStorage = { getItem: (k) => store.get(k) ?? null, setItem: (k, v) => store.set(k, String(v)) };
const { statusPanelHtml, statusPanelView, statusPanelFlip } = require("../ui/status_panel.js");

const L = (body, waiting = "nothing") => parseStatus(`x\n**Where we are**\n${body}\nWaiting on you: ${waiting}`);
const sw = { sid: "a", on: true };
const items = (html) => [...html.matchAll(/<span class="st-n">(\d+)<\/span>/g)].map((m) => +m[1]);

// Closed: one line in the theme's color (how far along, what it's on and beside it, what it waits on), no list.
const long = L("✓ a\n✓ b\n✓ c\n✓ d\n✓ e\n→ ∥ f\n→ ∥ g\n○ ∥ h\n○ i\n○ j\n○ k", "try the row");
const closed = statusPanelHtml(long, sw, {});
assert.ok(closed.includes('<div class="st-panel">') && closed.includes('class="st-bar st-inv" data-act="status-open" data-sid="a" aria-expanded="false"'), "the line opens the list");
assert.ok(closed.includes("<b>5 of 11</b>") && closed.includes("now:</span> f") && closed.includes("+ 2 side by side") && closed.includes("▾"));
assert.ok(!closed.includes("waiting on you"), "what it waits on is in the open list, not the line");
const notStarted = statusPanelHtml(L("✓ a\n○ b\n○ c"), sw, {});
assert.ok(notStarted.includes("next:</span> b") && !notStarted.includes("now:"), "nothing marked →: the first left is next, not now");
assert.ok(!closed.includes("st-drawer") && !closed.includes("st-list"), "closed: no list");
const short = L("✓ plan\n→ move the reader\n○ ∥ fix scroll\n○ ∥ cache usage\n○ review");
assert.ok(statusPanelHtml(L("✓ a\n✓ b"), sw, {}).includes("all done"));

// Open: the whole list hangs from the line (no count of its own: the line has it), then the switch and what it costs.
const open = statusPanelHtml(long, sw, { open: true });
assert.ok(open.includes('<div class="st-panel">') && open.includes('aria-expanded="true"') && open.includes("▴") && open.includes("st-drawer st-inv"));
const drawer = open.slice(open.indexOf("st-drawer"));
assert.deepStrictEqual(items(drawer), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11], "every item, long list or short");
assert.ok(!drawer.includes("of 11 done"), "the count is on the line only");
assert.ok(drawer.includes("Waiting on you: try the row") && drawer.indexOf("st-list") < drawer.indexOf("st-toggle") && drawer.includes("half a cent"), "then the switch and what it costs");
assert.ok(statusPanelHtml(L("✓ a\n○ ∥ b\n○ ∥ c"), sw, { open: true }).includes('data-act="status-parallel"'), "Parallelize when a group is ready");
const spent = statusPanelHtml(long, { ...sw, spent: "about $0.04 so far" }, { open: true });
assert.ok(spent.includes('Status list for this session <span class="st-dim">(about $0.04 so far)</span>'), "this session's cost, beside the switch");
assert.ok(!open.includes("so far)") && !statusPanelHtml(long, { ...sw, spent: "x" }, {}).includes("(x)"), "none yet: nothing beside it; closed: not shown");
assert.ok(statusPanelHtml(short, sw, { open: true, slide: true }).includes("st-drawer st-inv slide"), "just opened: it slides down");
assert.ok(!statusPanelHtml(short, sw, { open: true }).includes(" slide\""), "otherwise not (a redraw doesn't slide it again)");

// Relay mode, on the line (open or not) while relay's skills are there: its switch; on, Run on relay (2+ steps not
// started) beside it.
const head = (h) => h.slice(h.indexOf("st-head"), h.indexOf("st-drawer") < 0 ? undefined : h.indexOf("st-drawer"));
const relayOff = statusPanelHtml(short, { ...sw, relay: { on: false } }, {});
assert.ok(head(relayOff).includes('data-act="status-relay"') && head(relayOff).includes('data-on="1"') && head(relayOff).includes('aria-checked="false"') && !relayOff.includes("status-relay-run"), "off: the switch on the line, no Run");
const relayOn = statusPanelHtml(short, { ...sw, relay: { on: true } }, { open: true });
assert.ok(head(relayOn).includes(">Run on relay<") && head(relayOn).includes('aria-checked="true"') && head(relayOn).includes('data-on="0"'), "on: Run on relay and the switch, on, on the line");
assert.ok(!statusPanelHtml(L("✓ a\n→ b\n○ c"), { ...sw, relay: { on: true } }, { open: true }).includes("status-relay-run"), "one step left: nothing to hand out");
assert.ok(statusPanelHtml(L("✓ a\n→ b\n○ c"), { ...sw, relay: { on: true } }, {}).includes('aria-checked="true"'), "one step left: still the switch");
assert.ok(!statusPanelHtml(short, sw, { open: true }).includes("status-relay"), "no relay for the session: no switch");

// Nothing: put away, off for the session, or no list.
assert.strictEqual(statusPanelHtml(short, sw, { away: true }), "", "put away with the ring");
assert.strictEqual(statusPanelHtml(short, { sid: "a", on: false }, {}), "", "off for the session");
assert.strictEqual(statusPanelHtml(parseStatus("plain"), sw, {}), "", "no list yet");

// The choices: put away kept (per session, across a reload); open this run only; back to the default isn't kept.
statusPanelFlip("a", "away");
statusPanelFlip("b", "open");
const vb = statusPanelView("b");
assert.ok(vb.open && vb.slide && !vb.away, "b: open, just now");
assert.deepStrictEqual(JSON.parse(store.get("cue.statusPanel")), { a: { away: true } }, "only put away is kept");
statusPanelFlip("a", "away");
statusPanelFlip("b", "open");
assert.deepStrictEqual(statusPanelView("a"), { away: false, open: false, slide: false });
assert.ok(!statusPanelView("b").open);
assert.deepStrictEqual(JSON.parse(store.get("cue.statusPanel")), {}, "brought back: not kept");
console.log("status_panel.test.js: ok");
