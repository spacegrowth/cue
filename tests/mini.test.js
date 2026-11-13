// Run: node tests/mini.test.js  (the mini window's page loads Cue's own chat: ui/mini.html against ui/index.html)
// The mini window draws its open chat with the main window's code (ui/mini.js). So its page loads the main window's
// stylesheets and scripts, in index.html's order, all but the terminal (vendor/xterm), the window's draw
// (app/render.js) and its boot (app.js); then its own. A script added to index.html and not here would leave the mini
// window's chat a function short (a ReferenceError mid-draw); one here and not there, a name the Rust check
// (lib.rs the_windows_scripts_declare_each_top_level_name_once) never saw.
const assert = require("assert");
const fs = require("fs");
const path = require("path");

const read = (f) => fs.readFileSync(path.join(__dirname, "../ui", f), "utf8");
const scripts = (html) => [...html.matchAll(/<script src="([^"]+)"/g)].map((m) => m[1]);
const sheets = (html) => [...html.matchAll(/<link rel="stylesheet" href="([^"]+)"/g)].map((m) => m[1]);
const index = read("index.html"), mini = read("mini.html");

const MAIN_ONLY = new Set(["app/render.js", "app.js"]);
const want = scripts(index).filter((s) => !s.startsWith("vendor/") && !MAIN_ONLY.has(s));
assert.deepStrictEqual(scripts(mini), [...want, "mini.js"], "mini.html's scripts: index.html's (but its terminal, draw and boot), then mini.js");
assert.deepStrictEqual(sheets(mini), [...sheets(index).filter((s) => !s.startsWith("vendor/")), "styles/mini.css"], "mini.html's styles: index.html's (but its terminal's), then mini.css");

// mini.js keeps its names in one block: nothing declared at its top level (a name there could clash with one of the
// main window's scripts, and WebKit would skip the whole script).
const top = read("mini.js").split("\n").filter((l) => /^(const|let|var|function|class|async function)\b/.test(l));
assert.deepStrictEqual(top, [], "mini.js declares nothing at its top level");
console.log("mini.html loads Cue's chat as index.html does");
