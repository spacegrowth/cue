// Run: node tests/live_reload.test.js  (ui/live_reload.js: on "ui-reload" the window loads its files again, once
// you've stopped typing or clicking for 3 s, so a draft is saved first)
const assert = require("assert");
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const src = fs.readFileSync(path.join(__dirname, "../ui/live_reload.js"), "utf8");
let now = 0, timers = [], reloads = 0;
const handlers = {}, listeners = {};
const ctx = {
  Date: { now: () => now },
  setTimeout: (f, ms) => timers.push({ f, at: now + ms }),
  addEventListener: (e, f) => { (handlers[e] ||= []).push(f); },
  location: { reload: () => { reloads++; } },
};
ctx.window = ctx;
ctx.window.__TAURI__ = { event: { listen: (e, f) => { listeners[e] = f; return Promise.resolve(); } } };
vm.runInNewContext(src, ctx);
const run = (ms) => {
  now += ms;
  for (;;) {
    const due = timers.filter((t) => t.at <= now);
    if (!due.length) break;
    timers = timers.filter((t) => t.at > now);
    due.forEach((t) => t.f());
  }
};

assert.ok(listeners["ui-reload"], "it listens for ui-reload");
// Quiet: it reloads at once.
now = 10_000;
listeners["ui-reload"]({ payload: { commit: "abc1234" } });
assert.strictEqual(reloads, 1, "reloads when you aren't typing");

// Typing: it waits until 3 s after the last key.
reloads = 0;
handlers.keydown.forEach((f) => f());
listeners["ui-reload"]({ payload: {} });
assert.strictEqual(reloads, 0, "not while you type");
run(1000);
handlers.keydown.forEach((f) => f());
run(2000);
assert.strictEqual(reloads, 0, "a key resets the wait");
run(1500);
assert.strictEqual(reloads, 1, "3 s after the last key");
console.log("live_reload ok");
