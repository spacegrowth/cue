// Run: node tests/card-tests.test.js  (tests/run.sh card: which tests a card runs for the files it changed,
// by tests/card-tests.map; tests/card-pick.js picks, tests/run.sh card --list says what would run)
const assert = require("assert");
const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");

const ROOT = path.resolve(__dirname, "..");
const { read_map, pick } = require("./card-pick.js");
const RUST = "src-tauri/src/testing/tests";

/** What `tests/run.sh card --list <files>` says: every line, and the commands it would run. */
function listed(...files) {
  const out = execFileSync("bash", [path.join(ROOT, "tests/run.sh"), "card", "--list", ...files], { encoding: "utf8" });
  const lines = out.trim().split("\n");
  return { out, lines, would: lines.filter((l) => l.startsWith("would run: ")).map((l) => l.slice(11)) };
}
const filters_of = (r) => (r.would.find((w) => w.includes("cargo test")) || "").split(" -- ")[1]?.split(" ") || [];
const specs_of = (r) => (r.would.find((w) => w.includes("playwright test")) || "").split(" ").filter((a) => a.endsWith(".spec.mjs")).map((a) => path.basename(a));

// The release scripts' tests: any of their files runs both of them; the full run's script and its test run its own.
{
  const runs = (r, test) => r.would.includes("bash -c tests/release/" + test);
  const r = listed("tests/release/debug_only.test.sh");
  assert.ok(runs(r, "gates.test.sh") && runs(r, "debug_only.test.sh") && !runs(r, "full_run.test.sh"), r.out);
  assert.deepStrictEqual(listed("scripts/full-run.sh").would, ["bash -c tests/release/full_run.test.sh"]);
  const own = listed("tests/release/full_run.test.sh");
  assert.ok(runs(own, "full_run.test.sh"), own.out);
}

// A file moved away counts under its old name too (a rename read as one would hide the folder it left).
assert.ok(fs.readFileSync(path.join(ROOT, "tests/run.sh"), "utf8").includes('diff --no-renames --name-only "$base"'), "tests/run.sh card must list a rename's both names");

// Documents only: nothing runs, and it says so.
{
  const r = listed("docs/faster-checks-plan.md", "work/notes.txt", "README.md", "src-tauri/src/desk/README.md");
  assert.deepStrictEqual(r.would, [], r.out);
  assert.ok(r.out.includes("nothing to run: only documents changed"), r.out);
}

// A Desk window file: the desk specs and fits, the node tests and the window check; never the whole window suite.
{
  const r = listed("ui/desk/list.js");
  const specs = specs_of(r);
  const all = fs.readdirSync(path.join(ROOT, "tests/ui/specs")).filter((f) => f.endsWith(".spec.mjs"));
  for (const s of all.filter((f) => f.startsWith("desk-"))) assert.ok(specs.includes(s), `${s} picked`);
  assert.ok(specs.includes("fits.spec.mjs"), "fits always");
  assert.ok(!specs.includes("terminal.spec.mjs") && specs.length < all.length, "not every spec");
  assert.ok(r.would.includes("node tests/box.test.js") && r.would.includes("node tests/ui/commands.test.js"), r.out);
  assert.ok(r.would.includes("node scripts/check-window.mjs ."), r.out);
  assert.ok(!r.would.some((w) => w.includes("cargo")), "no Rust for a window file");
  assert.ok(!r.out.includes("wide card"), r.out);
}

// A landing Rust file: the build, the code check, the landing tests and its own unit tests; no other Desk group.
{
  const r = listed("src-tauri/src/desk/hub/landing/finish.rs");
  const f = filters_of(r);
  for (const t of ["desk_land", "desk_finish", "desk_commit", "desk_spec_landing"]) assert.ok(f.includes(`testing::tests::${t}::`), `${t} picked: ${f}`);
  assert.ok(f.includes("desk::hub::landing::finish::"), "its own unit tests");
  for (const t of ["desk_routing", "desk_queue", "desk_install", "desk_lead", "desk_copies", "typing"]) assert.ok(!f.includes(`testing::tests::${t}::`), `${t} left out`);
  assert.ok(r.would.includes("(in src-tauri) cargo build -q") && r.would.includes("(in src-tauri) cargo run -q -p xtask -- check"), r.out);
  assert.ok(!r.out.includes("wide card") && !r.would.some((w) => w.includes("playwright")), r.out);
}

// Routing and landing are different sets; two files' sets add up.
{
  const route = filters_of(listed("src-tauri/src/desk/hub/turn/route.rs"));
  assert.ok(route.includes("testing::tests::desk_routing::") && !route.includes("testing::tests::desk_land::"), `${route}`);
  const both = filters_of(listed("src-tauri/src/desk/hub/turn/route.rs", "src-tauri/src/desk/hub/landing/install.rs"));
  assert.ok(both.includes("testing::tests::desk_routing::") && both.includes("testing::tests::desk_install::"), `${both}`);
}

// A changed test runs itself.
{
  assert.deepStrictEqual(filters_of(listed("src-tauri/src/testing/tests/desk_pr.rs")), ["testing::tests::desk_pr::"]);
  assert.deepStrictEqual(specs_of(listed("tests/ui/specs/mini.spec.mjs")).sort(), ["fits.spec.mjs", "mini.spec.mjs"]);
}

// Shared ground: a wide card, the full suite for that side only, with the switch the runner's guard asks for.
{
  const r = listed("src-tauri/src/hub/mod.rs");
  assert.ok(r.out.includes("wide card (rust)"), r.out);
  assert.deepStrictEqual(r.would, ["CUE_FULL_RUN=1 tests/run.sh unit"]);
  const w = listed("tests/ui/bridge.js");
  assert.ok(w.out.includes("wide card (window)"), w.out);
  assert.deepStrictEqual(w.would, ["tests/run.sh ui"]);
  const both = listed("tests/run.sh");
  assert.deepStrictEqual(both.would, ["tests/run.sh ui", "CUE_FULL_RUN=1 tests/run.sh unit"]);
  // Wide wins over a narrow set picked for another file.
  assert.deepStrictEqual(listed("src-tauri/src/db/mod.rs", "src-tauri/src/desk/hub/landing/finish.rs").would, ["CUE_FULL_RUN=1 tests/run.sh unit"]);
}

// A file no line matches is never left untested: wide for its side (both, when the side can't be told).
{
  const r = listed("src-tauri/src/some_new_thing.rs");
  assert.ok(r.out.includes("wide card (rust)") && r.out.includes("matches no line"), r.out);
  assert.deepStrictEqual(r.would, ["CUE_FULL_RUN=1 tests/run.sh unit"]);
  assert.deepStrictEqual(listed("ui/some_new_thing.js").would, ["tests/run.sh ui"]);
  assert.deepStrictEqual(listed("some_new_thing.toml").would, ["tests/run.sh ui", "CUE_FULL_RUN=1 tests/run.sh unit"]);
  // A document beside it changes nothing.
  assert.deepStrictEqual(listed("docs/a.md", "ui/some_new_thing.js").would, ["tests/run.sh ui"]);
}

// The map itself: every name in it is a real test file, and every test file is one a wide run reaches (a Rust test
// file is declared in testing/tests/mod.rs, a spec sits where Playwright looks), so a card that goes wide misses none.
{
  const map = read_map();
  assert.deepStrictEqual(map.wrong, [], "tests/card-tests.map names only tests that are there");
  const mods = fs.readFileSync(path.join(ROOT, "src-tauri/src/testing/tests/mod.rs"), "utf8");
  for (const name of map.rust) assert.ok(new RegExp(`^\\s*(pub(\\([a-z]+\\))? )?mod ${name};`, "m").test(mods), `${name}.rs is declared in testing/tests/mod.rs`);
  const config = fs.readFileSync(path.join(ROOT, "tests/ui/playwright.config.mjs"), "utf8");
  assert.ok(config.includes('testMatch: "*.spec.mjs"') && config.includes('testDir: "specs"'), "a wide window run takes every spec of tests/ui/specs");
  assert.ok(map.specs.length > 100 && map.rust.length > 100, "the test files were found");
  // Each one alone, changed, is picked or makes its side wide: a file with tests in it runs itself, a file of
  // helpers runs the tests its own line names.
  for (const name of map.rust) {
    const file = `${RUST}/${name}.rs`;
    const { plan, lines } = pick([file], map);
    const wide = plan.includes("rust-wide"), filters = plan.filter((p) => p.startsWith("rust-filter ")).map((p) => p.slice(12));
    const has_tests = fs.readFileSync(path.join(ROOT, file), "utf8").includes("#[test]");
    assert.ok(wide || (has_tests ? filters.includes(`testing::tests::${name}::`) : filters.length > 0), `${name}.rs: ${lines.join("; ")}`);
  }
  for (const spec of map.specs) {
    const { plan } = pick([`tests/ui/specs/${spec}`], map);
    assert.ok(plan.includes("window-wide") || plan.includes(`spec tests/ui/specs/${spec}`), `${spec} runs when it changes`);
  }
}

console.log("card-tests: ok");
