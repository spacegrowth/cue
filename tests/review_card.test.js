// Run: node tests/review_card.test.js  (the Review card at the top of a lead's chat: ui/reviewcard.js, and the words core.js takes from the same state)
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

const entry = (o = {}) => ({ state: "waiting", missed: false, name: "cue-build-install", lead: "L", packet: 2, goal: "the status list follows a session into its handoff", outcome: "Done", stat: "", findings: [], ...o });
const cards = (o) => { world.reviews = { E1: entry(o) }; return ctx.reviewCard("L"); };
const buttons = (h) => [...h.matchAll(/data-(?:crew|rv-say)="[^"]+"[^>]*>([^<]+)</g)].map((m) => m[1]);

// waiting: Review · Diff · Set aside
let h = cards({ stat: "3 files · +94 −2" });
assert.match(h, /WAITING FOR YOU/);
assert.match(h, /<b>cue-build-install<\/b> finished packet 2: the status list follows a session into its handoff\./);
assert.match(h, /3 files · \+94 −2/);
assert.deepEqual(buttons(h), ["Review", "Diff", "Set aside"]);
assert.match(cards({ missed: true }), /Its lead hasn't picked this up/);
assert.doesNotMatch(h, /title=/, "no hover text");

// reviewing and queued: Diff only
h = cards({ state: "reviewing" });
assert.match(h, /REVIEWING/);
assert.match(h, /running the tests and reading the diff against the packet/);
assert.deepEqual(buttons(h), ["Diff"]);
h = cards({ state: "queued" });
assert.match(h, /review queued, it starts when the lead is free/);
assert.deepEqual(buttons(h), ["Diff"]);

// reviewed: the findings, Commit · Send fixes · Diff; Commit and Send fixes type their words into the lead
h = cards({ state: "reviewed", findings: ["1. A regular handoff is not covered yet.", "2. Off stays off: tested."] });
assert.match(h, /REVIEWED · WAITING FOR YOU/);
assert.match(h, /<div>1\. A regular handoff is not covered yet\.<\/div><div>2\. Off stays off: tested\.<\/div>/);
assert.match(h, /data-rv-say="L" data-text="commit it">Commit/);
assert.match(h, /data-rv-say="L" data-text="send the fixes">Send fixes/);
assert.deepEqual(buttons(h), ["Commit", "Send fixes", "Diff"]);
assert.match(cards({ state: "reviewed" }), /: reviewed\./, "no findings anywhere: just says it ran");

// the lead on auto: it says so, Diff only
h = cards({ state: "auto" });
assert.match(h, /The lead is reviewing this on its own/);
assert.deepEqual(buttons(h), ["Diff"]);

// nothing waiting, or only another lead's: no card; one card per packet
world.reviews = {};
assert.equal(ctx.reviewCard("L"), "");
world.reviews = { E1: entry(), E2: entry({ name: "other" }), E3: entry({ lead: "M" }) };
assert.equal((ctx.reviewCard("L").match(/class="rv-card"/g) || []).length, 2);

// the same state gives the rows their words
const word = (o) => { world.reviews = { E1: entry(o) }; return [ctx.reviewWord("E1"), ctx.missedWake("E1")]; };
assert.deepEqual(word({ state: "waiting" }), ["", false]);
assert.deepEqual(word({ state: "waiting", missed: true }), ["", true]);
assert.deepEqual(word({ state: "queued" }), ["review queued", false]);
assert.deepEqual(word({ state: "reviewing" }), ["in review", false]);
assert.deepEqual(word({ state: "reviewed" }), ["reviewed", false]);
assert.deepEqual([ctx.reviewWord("nobody"), ctx.missedWake("nobody")], ["", false]);
console.log("review_card.test.js ok");
