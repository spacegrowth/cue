// Fakes for src-tauri/src/extensions.rs: the installed extensions, switching them on, what they did, and the git
// and program calls their pages make. ?xt=1: three of them: one on, one updated and asking for more, one off.
// And the read-only git Cue's own Diff tab reads (diff_git): nothing changed, unless ?diff=1 (two files edited).
CueBridge.area("extensions", (w) => {
  const root = "/Users/me/Library/Application Support/dev.spacegrowth.cue/extensions";
  const ext = (name, title, version, capabilities, o = {}) => ({ name, title, version, capabilities, programs: [], on: true, enabled: true, pending_capabilities: [], pending_programs: [], folder: `${root}/${name}`, ...o });
  const list = w.q.has("xt") ? [
    ext("boxes", "Containers", "0.3.0", ["sessions.read", "programs.run"], { panel: "Containers", add: "A new container", programs: ["docker", "mutagen"] }),
    ext("notes", "Session notes", "0.3.0", ["sessions.read", "cmd.run"], { tab: "Notes", programs: ["git"], enabled: false, pending_capabilities: ["cmd.run"], pending_programs: ["git"] }),
    ext("digest", "Daily digest", "0.1.0", ["sessions.read"], { on: false, enabled: false }),
  ] : [];
  /** "<unix seconds> <line>", as extensions.rs keeps its activity log; `ago` in seconds. */
  const line = (ago, text) => `${Math.floor(w.now() / 1000) - ago} ${text}`;
  const activity = {
    boxes: [line(600, "boxes switched on in Settings"),
      line(180, "boxes ran docker ps --format json -> 0"),
      line(40, "boxes ran mutagen sync list -> 0")],
  };
  /** What git says about a session's folder for the Diff tab (extensions.rs read_git). */
  const edited = (folder) => [
    { path: `${folder}/src/board.ts`, new: false, by: "", diff: "diff --git a/src/board.ts b/src/board.ts\nindex 3f9a1c2..8be01d4 100644\n--- a/src/board.ts\n+++ b/src/board.ts\n@@ -40,6 +40,9 @@ export function footer(rows) {\n   const total = rows.reduce((n, r) => n + r.spend, 0);\n-  footer.push(total);\n+  // Each executor's spend, then the total.\n+  for (const r of rows) footer.push(fmt(r.spend));\n+  footer.push(fmt(total));\n   return footer;\n }\n" },
    { path: `${folder}/src/fmt.ts`, new: true, by: "", diff: "diff --git a/src/fmt.ts b/src/fmt.ts\nnew file mode 100644\n--- /dev/null\n+++ b/src/fmt.ts\n@@ -0,0 +1,3 @@\n+export function fmt(usd) {\n+  return `$${usd.toFixed(2)}`;\n+}\n" },
  ];

  return {
    ext_list: () => ({ root, programs_allowed: false, list: structuredClone(list) }),
    ext_enable: ({ name, on }) => { const x = list.find((e) => e.name === name); if (!x) throw "No such extension"; x.on = on; x.enabled = on; return null; },
    ext_activity: ({ name, n }) => (activity[name] || []).slice(-n),
    ext_reveal: () => null,
    ext_log: () => null,
    // What git printed, as extensions.rs returns it.
    ext_git: ({ op }) => ({ code: 0, stdout: op === "diff" ? "diff --git a/src/board.ts b/src/board.ts\n+  footer.push(fmt(total));\n" : "", ms: 12 }),
    ext_program: ({ program }) => ({ code: 0, stdout: `${program}: ok\n`, stderr: "" }),
    // A repo's top is its folder's first two levels (/work/site/docs → /work/site).
    diff_git: ({ op, folder }) => op === "edited" ? { files: w.q.has("diff") ? edited(folder) : [], ms: 30 }
      : op === "toplevel" ? { code: 0, stdout: `${(folder.match(/^\/[^/]+\/[^/]+/) || [folder])[0]}\n`, ms: 4 }
      : { code: 0, stdout: "", ms: 8 },
  };
});
