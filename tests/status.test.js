// Run: node tests/status.test.js  (the status-list parser, ui/status.js)
const assert = require("assert");
const { parseStatus, stripStatus, statusHtml } = require("../ui/status.js");
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

assert.match(statusHtml(a, "k", false), /1 of 3 done/);
assert.doesNotMatch(statusHtml(a, "k", false), /st-list/, "folded by default");
assert.match(statusHtml(b, "k", false), /Waiting on you: which branch\?/);
const open = statusHtml(a, "k", true);
assert.ok(open.includes("st-list") && open.includes("Waiting on you: nothing") && !open.includes("⏸"));
assert.strictEqual(statusHtml(parseStatus("plain"), "k", false), "");
console.log("status.test.js: ok");
