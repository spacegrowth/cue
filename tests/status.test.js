// Run: node tests/status.test.js  (the status-list parser, ui/status.js)
const assert = require("assert");
const { parseStatus, stripStatus, statusChipHtml, statusOn, statusShown } = require("../ui/status.js");
global.esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

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
const row = (n, mark, text) => `<span class="st-n">${n}</span><span>${text}</span><span class="st-mark">${mark}</span>`;
assert.ok(chip.includes(row(1, "✓", "built it")) && chip.includes(row(2, "→", "testing it")) && chip.includes(row(3, "○", "ship it")), "numbered, in order, with their marks");
assert.ok(chip.includes("Waiting on you: nothing") && !chip.includes("⏸"), "plain text, no icon");
assert.ok(statusChipHtml(b, "k").includes("Waiting on you: which branch?"));
assert.ok(!chip.includes("cv-schip open"), "closed unless it is the open one");
assert.ok(statusChipHtml(a, "k", true).includes("cv-schip open"));
assert.strictEqual(statusChipHtml(parseStatus("plain"), "k"), "");
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
const onList = statusChipHtml(a, "a:1", false, { sid: "a", on: true });
assert.ok(onList.includes("</svg>1/3</button>") && onList.indexOf("st-list") < onList.indexOf("st-toggle") && onList.includes('data-on="0"'));
assert.ok(!off.includes("Status tracking starts") && !onList.includes("Status tracking starts"), "only while it's on and waiting for its first list");
console.log("status.test.js: ok");
