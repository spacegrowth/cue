// Checks that Cue's windows load from a copy of the repo, before that copy is installed or served in place: what Cue
// runs ahead of an install (scripts/self-install.sh) and before it serves a commit's window files to the running app
// (src-tauri/src/live_ui.rs).
//
//   node scripts/check-window.mjs <copy>
//
// <copy> is a clean copy of the repo at the commit to check (its ui/, tests/ui/ and scripts/, with node_modules
// linked in). Two steps, each for ui/index.html and ui/mini.html:
//
//   1. Names. The window's scripts share one global scope. A top-level name declared in two of them stops the second
//      before it runs (WebKit says nothing: the window shows its background and no more), and a call to a name no
//      script declares stops whatever reaches it (a click on Desk did nothing, three times on 17 November 2026). Each
//      script is parsed, every name it uses but doesn't declare must be declared by one of the page's scripts or be
//      the browser's own.
//   2. Load. The page opened in headless Chromium on the UI tests' fake bridge (tests/ui/app.html, mini.html): any
//      error as it loads fails, and so does a page that draws nothing.
//
// Exit 0: both pass. 1: the window is broken, each problem on its own line ("missing: foo, called in ui/a.js:12").
// 2: the check couldn't run (no node_modules, no browser): treated as a failure by whoever runs it.
// CUE_CHECK_NO_LOAD=1 skips step 2.
import { existsSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { spawn } from "node:child_process";
import { createServer } from "node:net";

const copy = resolve(process.argv[2] || ".");
const ui = join(copy, "ui");
const problems = [];

function give_up(why) {
  console.error(`check-window: couldn't check: ${why}`);
  process.exit(2);
}

if (!existsSync(join(ui, "index.html"))) give_up(`${ui}/index.html isn't there`);
const need = createRequire(join(copy, "package.json"));
let babel, playwright;
try {
  // The parser Playwright carries (a devDependency already), so the check adds nothing to install.
  babel = need(join(dirname(need.resolve("playwright/package.json")), "lib/transform/babelBundle.js"));
  playwright = need("playwright");
} catch (e) {
  give_up(`no Playwright in ${copy}/node_modules (${e.message.split("\n")[0]})`);
}
const { babelParse, traverse } = babel;

// The optional add-on gives the window its own script in addon.js's place, as a build with it does.
const addonDir = join(copy, "src-tauri/ext/ui");
const addon = existsSync(join(addonDir, "addon.js"));
const fileOf = (src) => (addon && src === "addon.js" ? join(addonDir, "addon.js") : join(ui, src));
const shown = (src) => (addon && src === "addon.js" ? "src-tauri/ext/ui/addon.js" : `ui/${src}`);

/** The scripts a page loads, in its order. */
function scriptsOf(page) {
  const html = readFileSync(join(ui, page), "utf8");
  return [...html.matchAll(/<script\b[^>]*\bsrc="([^"]+)"/g)].map((m) => m[1]);
}

/** One script's top-level names (name → line), the names it sets on window, and the names it uses free (name →
 *  first line). */
function namesIn(src) {
  const code = readFileSync(fileOf(src), "utf8");
  let ast;
  try {
    ast = babelParse(code, src, false);
  } catch (e) {
    problems.push(`doesn't parse: ${shown(src)}: ${e.message.split("\n")[0]}`);
    return { top: new Map(), onWindow: new Set(), free: new Map() };
  }
  const top = new Map();
  const onWindow = new Set();
  const free = new Map();
  const guarded = new Set();
  traverse(ast, {
    Program(p) {
      for (const [name, b] of Object.entries(p.scope.bindings)) top.set(name, b.identifier.loc?.start.line ?? 0);
    },
    // window.foo = …, globalThis.foo = …, self.foo = …: a global all the same.
    AssignmentExpression(p) {
      const l = p.node.left;
      if (l.type === "MemberExpression" && !l.computed && l.object.type === "Identifier" && ["window", "globalThis", "self"].includes(l.object.name)) onWindow.add(l.property.name);
    },
    ReferencedIdentifier(p) {
      const name = p.node.name;
      if (p.scope.getBinding(name) || free.has(name)) return;
      // typeof foo: asking whether it's there is fine, and so is using it in the script that asks.
      if (p.parentPath.isUnaryExpression({ operator: "typeof" })) return void guarded.add(name);
      free.set(name, p.node.loc?.start.line ?? 0);
    },
  });
  for (const name of guarded) free.delete(name);
  return { top, onWindow, free };
}

/** Of `names`, the ones the browser itself (and the page's vendor scripts) has as globals. */
async function browserHas(browser, vendor, names) {
  const page = await browser.newPage();
  for (const v of vendor) await page.addScriptTag({ content: readFileSync(join(ui, v), "utf8") });
  const has = await page.evaluate((ns) => ns.filter((n) => {
    try {
      return n in globalThis || typeof (0, eval)(n) !== "undefined";
    } catch {
      return false;
    }
  }), names);
  await page.close();
  return new Set(has);
}

/** Step 1, over `pages`. A name declared twice is one page's problem; a used name counts as declared when any page
 *  declares it, since the mini window loads only part of the main window's scripts and never reaches the rest. */
async function checkNames(browser, pages) {
  const declared = new Map(); // name → "ui/x.js:12"
  const used = []; // [name, "ui/x.js:12"]
  const vendor = new Set();
  const parsed = new Map();
  for (const page of pages) {
    const mine = new Map();
    for (const src of scriptsOf(page)) {
      if (src.startsWith("vendor/")) {
        vendor.add(src);
        continue;
      }
      if (!existsSync(fileOf(src))) {
        problems.push(`missing file: ui/${page} loads ${shown(src)}, which isn't there`);
        continue;
      }
      if (!parsed.has(src)) parsed.set(src, namesIn(src));
      const { top, onWindow, free } = parsed.get(src);
      for (const [name, line] of top) {
        const at = `${shown(src)}:${line}`;
        const first = mine.get(name);
        if (first && !first.startsWith(`${shown(src)}:`)) problems.push(`declared twice: ${name}, in ${first} and ${at}`);
        else if (!first) mine.set(name, at);
      }
      for (const name of onWindow) if (!mine.has(name)) mine.set(name, `${shown(src)} (on window)`);
      for (const [name, line] of free) used.push([name, `${shown(src)}:${line}`]);
    }
    for (const [name, at] of mine) if (!declared.has(name)) declared.set(name, at);
  }
  const unknown = [...new Set(used.map(([n]) => n).filter((n) => !declared.has(n)))];
  const builtIn = unknown.length ? await browserHas(browser, [...vendor], unknown) : new Set();
  const seen = new Set();
  for (const [name, at] of used) {
    if (declared.has(name) || builtIn.has(name) || seen.has(name)) continue;
    seen.add(name);
    problems.push(`missing: ${name}, used in ${at} and declared by none of the window's scripts`);
  }
  // The same double found from both pages is said once.
  const once = [...new Set(problems)];
  problems.length = 0;
  problems.push(...once);
}

/** A free port on 127.0.0.1. */
function freePort() {
  return new Promise((ok, fail) => {
    const s = createServer().listen(0, "127.0.0.1", () => {
      const { port } = s.address();
      s.close(() => ok(port));
    }).on("error", fail);
  });
}

/** Step 2: the page `harness` of tests/ui opened, its errors as it loads. */
async function checkLoad(browser, base, harness, root) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message.split("\n")[0]));
  page.on("requestfailed", (r) => errors.push(`couldn't load ${r.url().replace(base, "")}`));
  page.on("response", (r) => { if (r.status() >= 400) errors.push(`couldn't load ${r.url().replace(base, "")} (${r.status()})`); });
  try {
    await page.goto(`${base}tests/ui/${harness}${addon ? "&addon=1" : ""}`, { waitUntil: "load", timeout: 30000 });
    await page.waitForFunction(() => !!window.CueUI, null, { timeout: 30000 });
    await page.evaluate(() => window.CueUI.ready);
    await page.waitForTimeout(500);
    const drawn = await page.evaluate((sel) => (document.querySelector(sel)?.childElementCount ?? 0) > 0, root);
    if (!drawn) errors.push(`${root} is empty once the scripts ran`);
  } catch (e) {
    errors.push(e.message.split("\n")[0]);
  }
  await page.close();
  for (const e of [...new Set(errors)]) problems.push(`doesn't load: ${harness.split("?")[0]}: ${e}`);
}

let browser;
try {
  browser = await playwright.chromium.launch({ headless: true });
} catch (e) {
  give_up(`no headless Chromium (${e.message.split("\n")[0]})`);
}
let server;
try {
  const pages = ["index.html", "mini.html"].filter((p) => existsSync(join(ui, p)));
  await checkNames(browser, pages);
  const harness = existsSync(join(copy, "tests/ui/serve.mjs")) && existsSync(join(copy, "tests/ui/app.html"));
  if (!process.env.CUE_CHECK_NO_LOAD && harness && !problems.length) {
    const port = await freePort();
    server = spawn(process.execPath, [join(copy, "tests/ui/serve.mjs"), String(port)], { stdio: ["ignore", "pipe", "inherit"] });
    await new Promise((ok, fail) => {
      server.stdout.once("data", ok);
      server.once("exit", () => fail(new Error("the test server quit")));
    });
    const base = `http://127.0.0.1:${port}/`;
    await checkLoad(browser, base, "app.html?latency=0", "#app");
    if (existsSync(join(copy, "tests/ui/mini.html")) && pages.includes("mini.html")) await checkLoad(browser, base, "mini.html?latency=0", "#mini");
  }
} finally {
  server?.kill();
  await browser.close();
}

if (problems.length) {
  console.log(`The window is broken (${problems.length}):`);
  for (const p of problems) console.log(`  ${p}`);
  process.exit(1);
}
console.log("The window checks out: every name declared once, and it loads.");
