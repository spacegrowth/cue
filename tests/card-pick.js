// Picks the tests a card runs from the files it changed, by tests/card-tests.map (tests/run.sh card runs them).
//
//   node tests/card-pick.js [--plan <file>] [--check] [file…]
//
// Prints which sets it picked and why. --plan writes what to run, one step a line, for tests/run.sh:
//   rust-wide | window-wide | rust | rust-filter <f> | spec <path> | node-tests | check-window | run <command>
// --check prints what is wrong with the map itself (a name no test file has) and exits 1 if anything is.
const fs = require("fs");
const path = require("path");

const ROOT = path.resolve(__dirname, "..");
const MAP = path.join(__dirname, "card-tests.map");
const RUST_TESTS = "src-tauri/src/testing/tests";
const SPECS = "tests/ui/specs";
const ALWAYS_SPEC = "fits.spec.mjs";

const names_in = (dir, end) => fs.readdirSync(path.join(ROOT, dir)).filter((f) => f.endsWith(end)).sort();
const glob = (p) => new RegExp("^" + p.split("**").map((part) => part.split("*").map((s) => s.replace(/[.+?^${}()|[\]\\]/g, "\\$&")).join("[^/]*")).join(".*") + "$");

/** The map: its rules in order, its sets, and what is wrong with it. */
function read_map() {
  const rules = [], sets = {}, wrong = [];
  const rust = names_in(RUST_TESTS, ".rs").map((f) => f.slice(0, -3)).filter((n) => n !== "mod");
  const specs = names_in(SPECS, ".spec.mjs");
  // A name of the map as the test files it means.
  const expand = (what, name, line) => {
    if (name.startsWith("@")) {
      const set = sets[name.slice(1)];
      if (!set) wrong.push(`line ${line}: no set named ${name.slice(1)}`);
      return (set || []).flatMap((n) => expand(what, n, line));
    }
    if (what === "rust" && name.includes("::")) return [name];
    const full = what === "window" && !name.endsWith(".spec.mjs") ? `${name}.spec.mjs` : name;
    const found = (what === "rust" ? rust : specs).filter((f) => glob(full).test(f));
    if (!found.length) wrong.push(`line ${line}: no ${what === "rust" ? `${RUST_TESTS}/${full}.rs` : `${SPECS}/${full}`}`);
    return found;
  };
  fs.readFileSync(MAP, "utf8").split("\n").forEach((raw, i) => {
    const text = raw.replace(/(^|\s)#.*$/, "").trim();
    if (!text) return;
    const set = text.match(/^set\s+(\S+)\s*=\s*(.*)$/);
    if (set) return void (sets[set[1]] = set[2].split(/\s+/).filter(Boolean));
    const [pattern, what, ...rest] = text.split(/\s+/);
    if (!["skip", "rust", "window", "node", "self", "run", "wide", "wide-rust", "wide-window"].includes(what)) return void wrong.push(`line ${i + 1}: "${what}" isn't one of the map's words`);
    const rule = { pattern, what, line: i + 1, match: glob(pattern), by_name: !pattern.includes("/") };
    if (what === "run") rule.command = text.slice(text.indexOf(" run ") + 5).trim();
    if (what === "rust" || what === "window") {
      const before = wrong.length;
      rule.label = rest.join(" ");
      rule.tests = [...new Set(rest.flatMap((n) => expand(what, n, i + 1)))];
      rule.broken = wrong.length > before;
    }
    rules.push(rule);
  });
  return { rules, sets, wrong, rust, specs };
}

/** Which side a file no rule matches belongs to. */
const side_of = (file) => (file.startsWith("src-tauri/") ? ["rust"] : file.startsWith("ui/") || file.startsWith("tests/ui/") ? ["window"] : ["rust", "window"]);

/** The filter for the unit tests inside a Rust source file (and a `<name>_tests.rs` beside it). */
function own_filters(file) {
  const m = file.match(/^src-tauri\/src\/(.+)\.rs$/);
  if (!m || file.startsWith(RUST_TESTS + "/")) return [];
  const parts = m[1].split("/");
  if (parts[parts.length - 1] === "mod") return [parts.slice(0, -1).concat("tests").join("::") + "::"];
  const out = [parts.join("::") + "::"];
  if (fs.existsSync(path.join(ROOT, `src-tauri/src/${m[1]}_tests.rs`))) out.push(parts.join("::") + "_tests::");
  return out;
}

/** What to run for `files`: the plan's steps and the lines saying why. */
function pick(files, map) {
  const wide = { rust: [], window: [] }, filters = new Map(), specs = new Map(), runs = new Map(), said = new Map();
  const say = (line, file) => said.set(line, [...(said.get(line) || []), file]);
  const told = (side) => [...said].filter(([line]) => line.startsWith(side)).map(([line, why]) => `${line} for ${why.slice(0, 2).join(", ")}${why.length > 2 ? ` (and ${why.length - 2} more)` : ""}`);
  let rust = false, window = false, node = false;
  const add = (to, key, file) => to.set(key, [...(to.get(key) || []), file]);
  for (const file of files) {
    const rules = map.rules.filter((r) => r.match.test(r.by_name ? path.basename(file) : file));
    let covered = false;
    for (const r of rules) {
      if (r.what === "skip") covered = true;
      else if (r.what === "wide") (wide.rust.push(`${file} is shared ground`), wide.window.push(`${file} is shared ground`), (covered = true));
      else if (r.what === "wide-rust" || r.what === "wide-window") (wide[r.what.slice(5)].push(`${file} is shared ground`), (covered = true));
      else if (r.what === "node") node = covered = true;
      else if (r.what === "run") (add(runs, r.command, file), (covered = true));
      else if (r.what === "self") {
        const spec = file.startsWith(SPECS + "/") && file.endsWith(".spec.mjs");
        const there = fs.existsSync(path.join(ROOT, file));
        if (spec && there) (add(specs, path.basename(file), file), (window = covered = true), say(`window: ${path.basename(file)} itself, changed`, file));
        else if (!spec && there && fs.readFileSync(path.join(ROOT, file), "utf8").includes("#[test]")) {
          const name = path.basename(file, ".rs");
          add(filters, `testing::tests::${name}::`, file);
          rust = covered = true;
          say(`rust: ${name} itself, changed`, file);
        }
      } else if (r.broken) wide[r.what].push(`${file}: line ${r.line} of tests/card-tests.map names a test that isn't there`), (covered = true);
      else if (r.what === "rust") {
        for (const t of r.tests) add(filters, t.includes("::") ? t : `testing::tests::${t}::`, file);
        rust = covered = true;
        if (r.tests.length) say(`rust: ${r.label} (${r.tests.length} test file${r.tests.length === 1 ? "" : "s"})`, file);
      } else if (r.what === "window") {
        for (const s of r.tests) add(specs, s, file);
        window = covered = true;
        say(`window: ${r.label} (${r.tests.length} spec${r.tests.length === 1 ? "" : "s"})`, file);
      }
    }
    if (!covered) for (const side of side_of(file)) wide[side].push(`${file} matches no line of tests/card-tests.map`);
    if (rules.some((r) => r.what === "rust" && !r.broken)) for (const f of own_filters(file)) add(filters, f, file);
  }
  const plan = [], lines = [`card: ${files.length} changed file${files.length === 1 ? "" : "s"}`];
  for (const side of ["rust", "window"]) {
    if (!wide[side].length) continue;
    lines.push(`wide card (${side}): the full ${side} suite, because ${wide[side][0]}${wide[side].length > 1 ? ` (and ${wide[side].length - 1} more)` : ""}`);
    plan.push(`${side}-wide`);
  }
  if (rust && !wide.rust.length) {
    lines.push(...told("rust:"));
    const own = [...filters.keys()].filter((f) => !f.startsWith("testing::tests::"));
    if (own.length) lines.push(`rust: the changed files' own unit tests (${own.join(" ")})`);
    plan.push("rust", ...[...filters.keys()].sort().map((f) => `rust-filter ${f}`));
  }
  if (window && !wide.window.length) {
    lines.push(...told("window:"), `window: always ${ALWAYS_SPEC}, the node tests and scripts/check-window.mjs`);
    specs.set(ALWAYS_SPEC, []);
    plan.push("node-tests", "check-window", ...[...specs.keys()].sort().map((s) => `spec ${SPECS}/${s}`));
  } else if (node && !wide.window.length) {
    lines.push("window: the node tests only");
    plan.push("node-tests");
  }
  for (const [command, why] of runs) {
    lines.push(`run: ${command}, for ${why[0]}`);
    plan.push(`run ${command}`);
  }
  if (!plan.length) lines.push(files.length ? "nothing to run: only documents changed" : "nothing to run: no file changed against main");
  return { plan, lines: [...new Set(lines)] };
}

if (require.main === module) {
  const args = process.argv.slice(2);
  const map = read_map();
  if (args[0] === "--check") {
    map.wrong.forEach((w) => console.log(`tests/card-tests.map ${w}`));
    process.exit(map.wrong.length ? 1 : 0);
  }
  const at = args.indexOf("--plan");
  const out = at >= 0 ? args.splice(at, 2)[1] : null;
  const { plan, lines } = pick([...new Set(args)], map);
  console.log(lines.join("\n"));
  if (out) fs.writeFileSync(out, plan.join("\n") + (plan.length ? "\n" : ""));
}

module.exports = { read_map, pick, own_filters };
