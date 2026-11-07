// Run: node tests/status_edges.test.js  (the status list read from replies too big or not quite right, ui/status.js)
const assert = require("assert");
const { parseStatus, statusChipHtml } = require("../ui/status.js");
global.esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

const LIST = "**Where we are**\n✓ built it\n→ testing it\n○ ship it\nWaiting on you: nothing";

// Five megabytes of reply before its list: the list is read, and the reply is kept whole as its body.
const words = "w".repeat(5_000_000);
const big = parseStatus(`${words}\n\n${LIST}`);
assert.deepStrictEqual(big.items.map((i) => i.state), ["done", "now", "left"]);
assert.strictEqual(big.body.length, 5_000_000);

// A list of a hundred thousand items (five megabytes or so): every one read, the ring counts them all.
const many = parseStatus("Done.\n**Where we are**\n" + Array.from({ length: 100_000 }, (_, i) => `${i % 3 ? "○" : "✓"} step ${i}`).join("\n"));
assert.strictEqual(many.items.length, 100_000);
assert.match(statusChipHtml(many, "k"), /<\/svg>33334\/100000<\/button>/, "done/total on the ring");

// Two hundred thousand headings in a row, then one item: the last heading is the list's.
assert.strictEqual(parseStatus("**Where we are**\n".repeat(200_000) + "✓ a").items.length, 1);

// Five megabytes with no list: nothing read, the reply as it is.
assert.strictEqual(parseStatus(words).items.length, 0);

// Written with Windows line ends: the same list.
const crlf = parseStatus(`Done.\r\n\r\n${LIST.replace(/\n/g, "\r\n")}\r\n`);
assert.deepStrictEqual([crlf.body, crlf.items.length, crlf.waiting], ["Done.", 3, ""]);

// Not a list: the heading's bold left open, the heading mid-sentence, a list quoted in a code block with more after it,
// a sub-bullet under an item, or a mark with no words at all.
for (const [text, why] of [
  ["Done.\n**Where we are\n✓ a", "its bold left open"],
  ["Done. The **Where we are** list:\n✓ a", "mid-sentence"],
  ["Like this:\n```\n**Where we are**\n✓ a\n```\nThat's the shape.", "quoted in a code block"],
  ["Done.\n**Where we are**\n✓ a\n  - a sub-point", "a sub-bullet is writing after the list"],
  ["Done.\n**Where we are**\n✓\n→ \nWaiting on you: nothing", "marks with no words"],
]) assert.strictEqual(parseStatus(text).items.length, 0, why);

// Markup in an item or in what it waits on is shown as text, never as markup.
const sneaky = parseStatus('x\n**Where we are**\n✓ <img src=x onerror="alert(1)">\n→ <b>bold</b>\nWaiting on you: <script>alert(2)</script>');
const html = statusChipHtml(sneaky, "k", true);
assert.ok(!/<img|<b>|<script/.test(html), "no markup from the reply gets through");
assert.ok(html.includes("&lt;img src=x onerror=&quot;alert(1)&quot;&gt;") && html.includes("Waiting on you: &lt;script&gt;alert(2)&lt;/script&gt;"));

// A heading with a colon and spaces after it, an older ⏸ waiting line with a decision: read.
const colon = parseStatus("x\n**Where we are**:  \n○ a\n⏸ Waiting on you: pick a branch");
assert.deepStrictEqual([colon.items.length, colon.waiting], [1, "pick a branch"]);

// Nothing at all, or not text: no list, and no throw.
for (const v of [undefined, null, "", 42]) assert.strictEqual(parseStatus(v).items.length, 0);

console.log("status edges: ok");
