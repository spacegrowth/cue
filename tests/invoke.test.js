// Run: node tests/invoke.test.js  (the window's invoke, ui/invoke.js: the calls that must land in order do)
const assert = require("assert");
const { orderedInvoke, INVOKE_LANES } = require("../ui/invoke.js");

/** A stand-in for Tauri's invoke: each call is answered when the test says (`answer`), in any order, as
 *  commands running off the window's thread can come back. `started` lists the calls in the order made. */
function bridge() {
  const started = [];
  const open = [];
  const call = (cmd, args) => new Promise((resolve, reject) => { started.push({ cmd, args }); open.push({ cmd, args, resolve, reject }); });
  const answer = (i, err) => { const c = open.splice(i, 1)[0]; err ? c.reject(err) : c.resolve(`${c.cmd}:${JSON.stringify(c.args)}`); };
  return { started, open, call, answer };
}
const tick = () => new Promise((r) => setImmediate(r));

(async () => {
  // Keystrokes into one terminal: the next goes only once the last is back, so they land as typed even when
  // the backend would have taken them in another order.
  {
    const b = bridge();
    const invoke = orderedInvoke(b.call);
    const typed = ["l", "s", " ", "-", "l", "\r"];
    const done = typed.map((d) => invoke("term_write", { sessionId: "s1", data: d }));
    await tick();
    assert.strictEqual(b.started.length, 1, "one keystroke in flight at a time");
    for (let i = 0; i < typed.length; i++) {
      await tick();
      b.answer(0, i === 2 ? "its terminal isn't taking input" : null);   // one fails: the rest still go, in order
      await tick();
    }
    assert.deepStrictEqual(b.started.map((c) => c.args.data), typed, "in the order typed");
    const outcomes = await Promise.allSettled(done);
    assert.deepStrictEqual(outcomes.map((o) => o.status), ["fulfilled", "fulfilled", "rejected", "fulfilled", "fulfilled", "fulfilled"], "each caller hears its own answer");
  }
  // Resume goes after the keystrokes sent before it, never ahead of one still waiting.
  {
    const b = bridge();
    const invoke = orderedInvoke(b.call);
    invoke("term_write", { sessionId: "s1", data: "a" });
    invoke("term_write", { sessionId: "s1", data: "b" });
    invoke("term_resume", { sessionId: "s1" });
    await tick();
    b.answer(0, "isn't responding"); await tick(); await tick();
    b.answer(0, "isn't responding"); await tick(); await tick();
    assert.deepStrictEqual(b.started.map((c) => c.cmd), ["term_write", "term_write", "term_resume"]);
  }
  // Other lanes and other commands aren't held behind a stuck one: another terminal's keys, a resize, the state.
  {
    const b = bridge();
    const invoke = orderedInvoke(b.call);
    invoke("term_write", { sessionId: "stuck", data: "a" });
    invoke("term_write", { sessionId: "stuck", data: "b" });
    invoke("term_write", { sessionId: "other", data: "c" });
    invoke("term_resize", { sessionId: "stuck", cols: 80, rows: 24 });
    invoke("get_state");
    await tick();
    assert.deepStrictEqual(b.started.map((c) => `${c.cmd}:${c.args?.data ?? ""}`), ["term_write:a", "term_write:c", "term_resize:", "get_state:"]);
  }
  // A terminal's opens go one at a time; its close never waits behind an open (one that hangs, its machine
  // asleep, would hold it for good): it goes at once, naming the opening it means (term::close_if).
  {
    const b = bridge();
    const invoke = orderedInvoke(b.call);
    invoke("term_open", { sessionId: "s1", cols: 80, rows: 24 });
    invoke("term_open", { sessionId: "s1", cols: 90, rows: 24 });
    invoke("term_close", { sessionId: "s1", gen: 7 });
    invoke("shell_open", { id: "shell-1", cwd: "~", cols: 80, rows: 24 });
    await tick();
    assert.deepStrictEqual(b.started.map((c) => c.cmd), ["term_open", "term_close", "shell_open"]);
    b.answer(0);
    await tick(); await tick();
    assert.deepStrictEqual(b.started.map((c) => c.cmd), ["term_open", "term_close", "shell_open", "term_open"]);
  }
  // A setting switched twice: saved in that order (the last one wins); a session's status list and a direct save
  // of the same setting share its lane.
  {
    const b = bridge();
    const invoke = orderedInvoke(b.call);
    invoke("set_setting", { key: "tips.on", value: true });
    invoke("set_setting", { key: "tips.on", value: false });
    invoke("status_session", { sessionId: "s1", on: true });
    invoke("set_setting", { key: "status.sessions", value: [] });
    await tick();
    assert.deepStrictEqual(b.started.map((c) => c.cmd), ["set_setting", "status_session"]);
    b.answer(0); b.answer(0);
    await tick(); await tick();
    assert.deepStrictEqual(b.started.map((c) => c.args.value ?? c.args.on), [true, true, false, []]);
  }
  // A hand-off continued and then kept, an extension's log lines: in order, per session and per extension.
  {
    const b = bridge();
    const invoke = orderedInvoke(b.call);
    invoke("handoff_continue", { sessionId: "s1" });
    invoke("handoff_keep", { sessionId: "s1" });
    invoke("handoff_keep", { sessionId: "s2" });
    invoke("ext_log", { ext: "x", text: "one" });
    invoke("ext_log", { ext: "x", text: "two" });
    await tick();
    assert.deepStrictEqual(b.started.map((c) => `${c.cmd}:${c.args.sessionId || c.args.text}`), ["handoff_continue:s1", "handoff_keep:s2", "ext_log:one"]);
  }
  // Every lane names a command the backend has (src-tauri/src/startup/mod.rs generate_handler!).
  const handlers = require("fs").readFileSync(__dirname + "/../src-tauri/src/startup/mod.rs", "utf8").match(/generate_handler!\[([^\]]*)\]/)[1].split(",").map((s) => s.trim().split("::").pop());
  for (const cmd of Object.keys(INVOKE_LANES)) assert.ok(handlers.includes(cmd), `${cmd} is a command`);
  console.log("invoke.test.js: ok");
})().catch((e) => { console.error(e); process.exit(1); });
