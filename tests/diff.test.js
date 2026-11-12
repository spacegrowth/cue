// Run: node tests/diff.test.js  (the Diff tab's reading of a diff, and a crew member's files by repo: ui/diff.js)
const assert = require("assert");
const fs = require("fs");
const vm = require("vm");

const ctx = { esc: (s) => String(s ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;") };
vm.createContext(ctx);
vm.runInContext(fs.readFileSync(__dirname + "/../ui/diff.js", "utf8").replace(/^(const|let) /gm, "var "), ctx);

// ---- a unified diff as files ----
const text = [
  "diff --git a/ui/a.js b/ui/a.js", "index 1..2 100644", "--- a/ui/a.js", "+++ b/ui/a.js", "@@ -1,2 +1,3 @@", " keep", "-old", "+new", "+more",
  "diff --git a/site/docs/x.md b/site/docs/x.md", "new file mode 100644", "--- /dev/null", "+++ b/site/docs/x.md", "@@ -0,0 +1 @@", "+hello",
  "/tmp/notes.txt: not in a git repository",
  "… (cut here: the rest is in git)",
].join("\n");
const { files, notes } = ctx.diffParse(text);
assert.deepEqual(files.map((f) => [f.path, f.add, f.del, f.isNew]), [["ui/a.js", 2, 1, false], ["site/docs/x.md", 1, 0, true]]);
assert.deepEqual(files[0].lines.map(([k]) => k), ["h", "", "d", "a", "a"]);
assert.deepEqual(notes, ["/tmp/notes.txt: not in a git repository", "… (cut here: the rest is in git)"], "lines outside a diff are notes, not a file's lines");
assert.equal(ctx.diffParse("").files.length, 0);

// ---- a crew member's files by repo ----
// Its own folder's repo (/work/cue) names files from its top; another repo's (/work/site) under its label.
const given = ["ui/a.js", "/work/site/docs/x.md"];
const spans = ctx.diffRepos(files, given, "/work/cue", { "/work/site/docs/x.md": "/work/site" });
assert.equal(spans, true, "two repos");
assert.deepEqual(files.map((f) => [f.repo, f.path]), [["cue", "ui/a.js"], ["site", "docs/x.md"]]);
// A file given by its whole path in its own repo stays there, its path as git gives it.
const own = ctx.diffParse("diff --git a/src/b.rs b/src/b.rs\n+x").files;
assert.equal(ctx.diffRepos(own, ["/work/cue/src/b.rs"], "/work/cue", { "/work/cue/src/b.rs": "/work/cue" }), false, "one repo: no labels shown");
assert.deepEqual(own.map((f) => [f.repo, f.path]), [["cue", "src/b.rs"]]);
// Two checkouts with the same name: the hub labels the other "other/app"; its files still find their repo.
const twin = ctx.diffParse("diff --git a/x.js b/x.js\n+a\ndiff --git a/other/app/y.js b/other/app/y.js\n+b").files;
assert.equal(ctx.diffRepos(twin, ["x.js", "/other/app/y.js"], "/work/app", { "/other/app/y.js": "/other/app" }), true);
assert.deepEqual(twin.map((f) => [f.repo, f.path]), [["app", "x.js"], ["other/app", "y.js"]]);
// A Mac's /private/tmp is /tmp to the stream.
const tmp = ctx.diffParse("diff --git a/a.txt b/a.txt\ndiff --git a/r2/b.txt b/r2/b.txt").files;
assert.equal(ctx.diffRepos(tmp, ["a.txt", "/tmp/r2/b.txt"], "/private/tmp/r1", { "/tmp/r2/b.txt": "/private/tmp/r2" }), true);
assert.deepEqual(tmp.map((f) => [f.repo, f.path]), [["r1", "a.txt"], ["r2", "b.txt"]]);

// ---- its file list: the repo on each file only when they span repos; never hover text ----
const html = ctx.dvFilesHtml(files, [], [], true);
assert.match(html, /<span class="dv-tag dv-repo">cue<\/span><span class="dv-path">ui\/a.js<\/span>/);
assert.match(html, /<span class="dv-tag dv-repo">site<\/span><span class="dv-path">docs\/x.md<\/span><span class="dv-tag">new<\/span>/);
assert.doesNotMatch(ctx.dvFilesHtml(files, [], [], false), /dv-repo/);
assert.doesNotMatch(html, /title=/);
assert.equal((html.match(/<details class="dv-file" data-i="\d+" open/g) || []).length, 2, "the first few open by themselves");

console.log("diff ok");
