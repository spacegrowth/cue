// Run: node tests/ui/commands.test.js  (the fake bridge answers every command the app has, and nothing else)
// The commands are read from the Rust source, so a new #[tauri::command] without a fake in tests/ui/fakes fails
// here, as does a fake left behind for a command that's gone, or a command the window calls that the app lacks.
const assert = require("assert");
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const root = path.join(__dirname, "../..");
const read = (p) => fs.readFileSync(path.join(root, p), "utf8");
const rustFiles = (dir) => fs.readdirSync(path.join(root, dir), { withFileTypes: true }).flatMap((e) => e.isDirectory() ? rustFiles(`${dir}/${e.name}`) : e.name.endsWith(".rs") ? [`${dir}/${e.name}`] : []);
const sorted = (xs) => [...new Set(xs)].sort();

// Every function marked #[tauri::command] (its name: the first `fn` after the attribute).
const declared = sorted(rustFiles("src-tauri/src").flatMap((f) => [...read(f).matchAll(/#\[tauri::command[^\]]*\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([a-z_0-9]+)/g)].map((m) => m[1])));
// What lib.rs registers with the window.
const registered = sorted(read("src-tauri/src/lib.rs").match(/generate_handler!\[([^\]]*)\]/)[1].split(",").map((s) => s.trim().split("::").pop()).filter(Boolean));

// The fakes, loaded as the pages load them (load.js's list, in its order) into a window of their own.
const fakesList = JSON.parse(read("tests/ui/load.js").match(/const FAKES = (\[[^\]]*\])/)[1]);
const win = { location: { search: "" }, structuredClone, btoa, TextEncoder, setTimeout, clearTimeout, setInterval, clearInterval, console, URLSearchParams, Promise, Date, Math, JSON };
win.window = win;
vm.createContext(win);
for (const f of ["tests/ui/bridge.js", ...fakesList.map((n) => `tests/ui/fakes/${n}.js`)]) vm.runInContext(read(f), win, { filename: f });
win.CueBridge.start(() => ({ sessions: [], items: [], history: [], settings: {} }));
const faked = sorted(win.CueBridge.commands());

// Every command the window calls by name.
const called = sorted(fs.readdirSync(path.join(root, "ui")).filter((f) => f.endsWith(".js")).flatMap((f) => [...read(`ui/${f}`).matchAll(/\binvoke\("([a-z_0-9]+)"/g)].map((m) => m[1])));

const missing = (a, b) => a.filter((x) => !b.includes(x));
assert.deepStrictEqual(missing(declared, registered), [], "commands declared but not registered in generate_handler!");
assert.deepStrictEqual(missing(registered, declared), [], "registered names with no #[tauri::command] found");
assert.deepStrictEqual(missing(registered, faked), [], "commands with no fake in tests/ui/fakes");
assert.deepStrictEqual(missing(faked, registered), [], "fakes for commands the app doesn't have");
assert.deepStrictEqual(missing(called, registered), [], "commands the window calls that the app doesn't have");
// Every fakes file is loaded (a file left out of load.js's list would fake nothing).
assert.deepStrictEqual(sorted(fs.readdirSync(path.join(root, "tests/ui/fakes")).map((f) => f.replace(/\.js$/, ""))), sorted(fakesList), "tests/ui/fakes files vs load.js FAKES");
assert.ok(registered.length > 100, `found only ${registered.length} commands: the parsing broke`);
console.log(`commands.test.js: ok (${registered.length} commands, every one faked; the window calls ${called.length} by name)`);
