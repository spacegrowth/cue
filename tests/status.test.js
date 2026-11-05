// Run: node tests/status.test.js  (the status-list parser, ui/status.js)
const assert = require("assert");
const { parseStatus, stripStatus, statusChipHtml, statusParts, statusOn, statusShown, statusSteps, parallelReady, PARALLELIZE } = require("../ui/status.js");
global.esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
/** Everything a session's list shows (in the panel above the chat, or its ring's card), in order. */
const body = (st, sw) => { const p = statusParts(st, sw); return p.list + p.par + p.toggle + p.starts + p.cost; };

const list = "**Where we are**\n✓ built it\n→ testing it\n○ ship it\nWaiting on you: nothing";
const a = parseStatus(`Done.\n\n${list}`);
assert.strictEqual(a.body, "Done.");
assert.deepStrictEqual(a.items.map((i) => i.state), ["done", "now", "left"]);
assert.strictEqual(a.now.text, "testing it");
assert.strictEqual(a.waiting, "", "nothing means nothing");

const b = parseStatus("Ok.\n**Where we are**\n✓ a\n⏸ Waiting on you: which branch?");
assert.strictEqual(b.waiting, "which branch?", "the older ⏸ form");
assert.strictEqual(b.now, null, "all done: no current item");
assert.strictEqual(parseStatus("x\n**Where we are**\n○ a\nWaiting on you: None.").waiting, "");
assert.strictEqual(parseStatus("x\n**Where we are**\n○ a\n○ b").now.text, "a", "no →: the first left");
assert.strictEqual(parseStatus("x\n**Where we are**\n→ a").waiting, "", "no waiting line");

const c = `Done.\n**Where we are**\n✓ a\nAnd one more thing.`;
assert.deepStrictEqual([parseStatus(c).body, parseStatus(c).items.length], [c, 0], "not at the end: left alone");
assert.strictEqual(parseStatus("Just words.").body, "Just words.");
assert.strictEqual(parseStatus("x\n**Where we are**\nWaiting on you: nothing").items.length, 0, "a heading with no items");
assert.strictEqual(parseStatus("Text\n\n---\n**Where we are**\n✓ a").body, "Text", "a rule before it goes too");
assert.strictEqual(stripStatus(`old\n${list}`), "old");
assert.strictEqual(parseStatus(`**Where we are**\n✓ old\n\nmore\n${list}`).items.length, 3, "the last list");

const chip = statusChipHtml(a, "k", false);
assert.match(chip, /<\/svg>1\/3<\/button>/, "done/total on the chip");
const arc = (cls) => +new RegExp(`class="${cls}"[^>]*stroke-dasharray="([\\d.]+) ([\\d.]+)"`).exec(chip).slice(1).reduce((x, c) => x / c);
assert.ok(Math.abs(arc("done") - 1 / 3) < 1e-9, "the ring's solid arc is the done share");
assert.ok(Math.abs(arc("now") - 2 / 3) < 1e-9, "the item it's on follows it");
assert.ok(chip.includes('class="trk"'), "a track for what's left");
const row = (n, mark, text) => `<span class="st-n">${n}</span><span class="st-dot"></span><span class="st-text">${text}</span><span class="st-mark">${mark}</span>`;
assert.ok(chip.includes(row(1, "✓", "built it")) && chip.includes(row(2, "→", "testing it")) && chip.includes(row(3, "○", "ship it")), "numbered, in order, with their marks");
assert.ok(chip.includes("Waiting on you: nothing") && !chip.includes("⏸"), "plain text, no icon");
assert.ok(statusChipHtml(b, "k").includes("Waiting on you: which branch?"));
assert.ok(!chip.includes("cv-schip open"), "closed unless it is the open one");
assert.ok(statusChipHtml(a, "k", true).includes("cv-schip open"));
assert.strictEqual(statusChipHtml(parseStatus("plain"), "k"), "");
assert.ok(chip.includes('class="st-item st-step done first"') && chip.includes('class="st-item st-step left last"'), "one line, from the first dot down to the last");
// Shown unless turned off in Settings; each session off until turned on from its ring.
assert.ok(statusShown(undefined) && statusShown({}) && !statusShown({ on: false }));
assert.ok(!statusOn("a", {}), "a session starts off");
assert.ok(statusOn("a", { sessions: ["a"] }) && !statusOn("b", { sessions: ["a"] }));
assert.ok(!statusOn("a", { on: false, sessions: ["a"] }), "off in Settings: off for every one");
// A session's ring: off (empty ring, "Off", the switch alone), on without a list yet ("On"), on with its list (the switch last).
const off = statusChipHtml(parseStatus("plain"), "a:off", false, { sid: "a", on: false });
assert.ok(off.includes("st-off") && off.includes("</svg></button>") && off.includes('data-act="status-session" data-sid="a" data-on="1"') && off.includes("st-toggle solo") && !off.includes("st-list"), "off: a faint ring, no word, the switch alone");
const offListed = statusChipHtml(a, "a:off", false, { sid: "a", on: false });
assert.ok(offListed.includes("st-off") && !offListed.includes("st-list"), "off: its list isn't shown even if a reply has one");
const onEmpty = statusChipHtml(parseStatus("plain"), "a:on", false, { sid: "a", on: true });
assert.ok(onEmpty.includes("st-empty") && !onEmpty.includes("st-off") && onEmpty.includes('data-on="0"'), "on, no list yet: the ring at full strength");
assert.ok(onEmpty.includes("Status tracking starts with your next message"), "idle: starts with your next message");
assert.ok(statusChipHtml(parseStatus("plain"), "a:on", false, { sid: "a", on: true, working: true }).includes("Status tracking starts when this turn ends"), "working: when this turn ends");
// On with its list: the list is the panel above the chat; the ring by the box puts it away or brings it back.
const onList = statusChipHtml(a, "a:1", false, { sid: "a", on: true, shown: true });
assert.ok(onList.includes("</svg>1/3</button>") && onList.includes('data-act="status-panel" data-sid="a"') && onList.includes('aria-pressed="true"') && onList.includes("put it away"), "with a list: the ring and count, a click puts the panel away");
assert.ok(!onList.includes("cv-spop") && !onList.includes("st-list") && !onList.includes("data-status-chip"), "no card of its own: the list is in the panel");
const onAway = statusChipHtml(a, "a:1", false, { sid: "a", on: true, shown: false });
assert.ok(onAway.includes('aria-pressed="false"') && onAway.includes("show it"), "put away: a click brings it back");
const onBody = body(a, { sid: "a", on: true });
assert.ok(onBody.indexOf("st-list") < onBody.indexOf("st-toggle") && onBody.includes('data-on="0"'), "the list, then the switch");
assert.ok(!off.includes("Status tracking starts") && !onBody.includes("Status tracking starts"), "only while it's on and waiting for its first list");
// Side by side: ∥ after the mark; the ∥ items next to each other are one group, run together; the rest in order.
const tree = parseStatus("x\n**Where we are**\n✓ parse\n○ ∥ api\n→ ∥∥web\n○ ∥ button\n○ tests\n○ ∥ lone\nWaiting on you: nothing");
assert.deepStrictEqual(tree.items.map((i) => [i.text, i.par]), [["parse", false], ["api", true], ["web", true], ["button", true], ["tests", false], ["lone", true]], "∥ (one or more, any spacing) read and taken off the text");
const steps = statusSteps(tree.items);
assert.deepStrictEqual(steps.map((st) => (Array.isArray(st) ? st.map((i) => i.n) : st.n)), [1, [2, 3, 4], 5, 6], "a run of ∥ is a group; a lone ∥ is a step");
assert.strictEqual(parallelReady(steps).length, 3, "the first step not done is a group with two or more left: ready");
assert.strictEqual(parallelReady(statusSteps(parseStatus("x\n**Where we are**\n→ a\n○ ∥ b\n○ ∥ c").items)), null, "an earlier step not done yet: not ready");
assert.strictEqual(parallelReady(statusSteps(parseStatus("x\n**Where we are**\n→ ∥ b\n→ ∥ c\n○ ∥ d").items)), null, "already running side by side");
assert.strictEqual(parallelReady(statusSteps(parseStatus("x\n**Where we are**\n✓ ∥ b\n○ ∥ c\n○ d").items)), null, "only one left in it");
assert.strictEqual(parallelReady(statusSteps(parseStatus("x\n**Where we are**\n○ a\n○ b").items)), null, "no group");
const treeChip = body(tree, { sid: "a", on: true });
// A group is one step: one dot on the line, its items together in one box; no words.
const groupOf = (html) => /<div class="st-group st-step (\w+)[^"]*"><span><\/span><span class="st-dot"><\/span><div class="st-box">(.*?)<\/div><\/div>(?=<div class="st-item st-step|<div class="st-wait)/.exec(html);
const g = groupOf(treeChip);
assert.ok(g && g[1] === "now", "the group's one dot: running while any item is");
assert.deepStrictEqual([...g[2].matchAll(/<span class="st-n">(\d+)<\/span>/g)].map((x) => +x[1]), [2, 3, 4], "its items, numbered, in the box");
assert.ok(!/side by side ·|at the same time/i.test(treeChip.replace(/can run side by side/, "")), "the box says it; no heading");
assert.strictEqual((treeChip.match(/st-dot/g) || []).length, 4, "four steps, four dots: 1, the group, 5, 6");
const endGroup = body(parseStatus("x\n**Where we are**\n✓ a\n✓ ∥ b\n✓ ∥ c"), { sid: "a", on: true });
assert.ok(endGroup.includes('class="st-group st-step done last"'), "a group at the end, all done: its dot filled, the line ends there");
assert.ok(treeChip.includes(`<span class="st-n">3</span><span class="st-text">web</span><span class="st-mark">→</span>`), "a group item it's on: its →, its number kept");
assert.ok(!body(tree, { sid: "a", on: true, working: true }).includes("st-spin"), "no spinners, working or not");
assert.ok(treeChip.includes('data-act="status-parallel" data-sid="a"') && treeChip.includes("3 can run side by side"), "Parallelize, counting the group items not done (one → is not yet side by side)");
assert.ok(treeChip.indexOf("st-par") < treeChip.indexOf("st-toggle"), "above the switch");
assert.ok(!statusChipHtml(tree, "t", false).includes("status-parallel"), "not on a ring that isn't a session's own");
assert.ok(!onBody.includes("status-parallel"), "no group: no button");
assert.ok(PARALLELIZE.startsWith("Parallelize the current step") && PARALLELIZE.includes("∥"));
assert.deepStrictEqual(parseStatus("x\n**Where we are**\n○ || operator handling\n○ ∥\n○").items.map((i) => [i.text, i.par]), [["|| operator handling", false]], "|| is just text; an item with no words is left out");
assert.ok(body(parseStatus("x\n**Where we are**\n→ ∥ solo\n○ b"), { sid: "a", on: true }).includes('<span class="st-mark">→</span>'), "a lone ∥ item is a plain step: its →");
assert.ok(off.includes("about half a cent each") && onBody.includes("Total in Settings → Progress"), "under the switch: what it costs, where the total is");
assert.ok(!statusChipHtml(a, "k", false).includes("half a cent"), "not on a ring that isn't a session's own");
console.log("status.test.js: ok");
