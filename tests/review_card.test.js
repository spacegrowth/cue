// Run: node tests/review_card.test.js  (the Review rows at the top of a lead's chat: ui/reviewcard.js, and the words core.js takes from the same state)
const assert = require("assert");
const fs = require("fs");
const vm = require("vm");

const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const world = { reviews: {} };
const ctx = {
  esc,
  get state() { return { reviews: world.reviews }; },
  crewBtn: (sid, a, label, primary) => `<button class="btn${primary ? " primary" : ""}" data-crew="${a}" data-crew-sid="${sid}">${label}</button>`,
};
vm.createContext(ctx);
const load = (src) => vm.runInContext(src.replace(/^(const|let) /gm, "var "), ctx);
load(fs.readFileSync(__dirname + "/../ui/reviewcard.js", "utf8"));
const core = fs.readFileSync(__dirname + "/../ui/core.js", "utf8");
load(core.slice(core.indexOf("const REVIEW_WORDS"), core.indexOf("/** An executor whose agent exited before it reported")));

const entry = (o = {}) => ({ state: "waiting", missed: false, name: "cue-build-install", lead: "L", packet: 2, goal: "Make the status list follow a session into its handoff.", outcome: "Done", stat: "", findings: [], recommendation: "", ...o });
const rows = (o) => { world.reviews = { E1: entry(o) }; return ctx.reviewRows("L"); };
const buttons = (h) => [...h.matchAll(/data-(?:crew|rv-say)="[^"]+"[^>]*>([^<]+)</g)].map((m) => m[1]);
const said = (h) => h.match(/class="rv-st"[^>]*>([^<]*)/)[1];

// waiting: one line, the name and where it stands; Review · Diff, the rest under ⋯
let h = rows({ stat: "3 files · +94 −2" });
assert.match(h, /<b class="rv-name">cue-build-install<\/b>/);
assert.equal(said(h), "finished packet 2 · 3 files · +94 −2");
assert.deepEqual(buttons(h), ["Review", "Diff"]);
assert.match(h, /data-rv-more="E1"/);
assert.doesNotMatch(h, /Make the status list/, "no packet goal on the row");
assert.equal(said(rows({ missed: true })), "finished packet 2 · its lead hasn't picked this up");
assert.doesNotMatch(h, /title=/, "no hover text");

// reviewing, queued and auto: Diff
assert.equal(said(rows({ state: "reviewing" })), "reviewing…");
assert.equal(said(rows({ state: "queued" })), "review queued");
assert.equal(said(rows({ state: "auto" })), "finished packet 2 · its lead reviews it");
for (const st of ["reviewing", "queued", "auto"]) assert.deepEqual(buttons(rows({ state: st })), ["Diff"]);

// reviewed: what it recommends and how many notes, › opens them; Commit · Send fixes type their words into the lead
const findings = ["1. src/a.rs:12 — note — Without a review file, the findings fall back.", "2. The count only comes from auto-commit."];
h = rows({ state: "reviewed", recommendation: "commit", findings });
assert.match(h, /data-rv-open="E1">reviewed: commit · 2 notes<span class="rv-chev">›<\/span>/);
assert.match(h, /data-rv-say="L" data-rv-text="commit it">Commit/);
assert.match(h, /data-rv-say="L" data-rv-text="send the fixes">Send fixes/);
assert.deepEqual(buttons(h), ["Commit", "Send fixes"]);
assert.doesNotMatch(h, /rv-under/, "closed until you open it");
ctx.reviewOpen.set("E1", "findings");
h = rows({ state: "reviewed", recommendation: "commit", findings });
assert.match(h, /<span class="rv-chev">⌄<\/span>/);
assert.match(h, /<div class="rv-under"><div>1\. Without a review file, the findings fall back\. <span class="rv-k">note<\/span><\/div><div>2\. The count only comes from auto-commit\.<\/div><\/div>/, "numbered, plain, the kind muted, no file:line");
assert.equal(said(rows({ state: "reviewed", recommendation: "fix-list", findings: ["1. one"] })), "reviewed: fix-list · 1 note");
assert.doesNotMatch(rows({ state: "reviewed" }), /data-rv-open/, "nothing to open: no ›");
assert.equal(said(rows({ state: "reviewed" })), "reviewed");
ctx.reviewOpen.clear();

// ⋯: Diff when the row doesn't show it, Set aside, what it was asked (shown under the row as it was written)
ctx.reviewMenu = "E1";
const menu = (o) => rows(o).match(/<div class="more-menu rv-menu">.*<\/div>/)[0];
assert.deepEqual(buttons(menu({})), ["Set aside"]);
assert.match(menu({}), /data-rv-asked="E1">What it was asked/);
assert.deepEqual(buttons(menu({ state: "reviewed" })), ["Diff", "Set aside"]);
assert.doesNotMatch(menu({ goal: "" }), /What it was asked/);
ctx.reviewMenu = null;
assert.doesNotMatch(rows({}), /rv-menu/);
ctx.reviewOpen.set("E1", "asked");
assert.match(rows({}), /<div class="rv-under"><div>Make the status list follow a session into its handoff\.<\/div><\/div>/);
assert.doesNotMatch(rows({}), /\.\./, "never a double period");
ctx.reviewOpen.clear();

// nothing waiting, or only another lead's: no rows; one row per packet, in one group
world.reviews = {};
assert.equal(ctx.reviewRows("L"), "");
world.reviews = { E1: entry(), E2: entry({ name: "other" }), E3: entry({ lead: "M" }) };
h = ctx.reviewRows("L");
assert.equal((h.match(/class="rv-row"/g) || []).length, 2);
assert.equal((h.match(/class="rv-rows"/g) || []).length, 1);

// the same state gives the rows their words
const word = (o) => { world.reviews = { E1: entry(o) }; return [ctx.reviewWord("E1"), ctx.missedWake("E1")]; };
assert.deepEqual(word({ state: "waiting" }), ["", false]);
assert.deepEqual(word({ state: "waiting", missed: true }), ["", true]);
assert.deepEqual(word({ state: "queued" }), ["review queued", false]);
assert.deepEqual(word({ state: "reviewing" }), ["in review", false]);
assert.deepEqual(word({ state: "reviewed" }), ["reviewed", false]);
assert.deepEqual([ctx.reviewWord("nobody"), ctx.missedWake("nobody")], ["", false]);
console.log("review_card.test.js ok");
