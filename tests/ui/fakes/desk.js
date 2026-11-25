// Fakes for the Desk's commands (desk_set_lead, desk_start, desk_send, desk_add, desk_give, desk_member, desk_item_act,
// desk_looks_right, desk_commit_undo, desk_item_diff, desk_done, desk_moves, desk_moves_clear, desk_undo, desk_held_answer): `state.desk`,
// one entry a project with its items, its lead, its sessions (members) and today's decisions. Times are seconds; a dropped item stays, as
// "dropped". A lead is a session like any other: desk_set_lead names one (or starts "<project>-lead", hosted), and
// desk_send hands it your message as a session takes one.
CueBridge.area("desk", (w) => {
  const desk = () => (w.state.desk ||= { projects: [] });
  /** Project `p`'s Desk, made empty when it has none yet. */
  const deskOf = (p) => desk().projects.find((d) => d.project === p) || (desk().projects.push({ project: p, lead: null, items: [], decisions: [], members: [] }), desk().projects.at(-1));
  /** Session `sid` on project `p`'s Desk, as the hub joins it. */
  const join = (p, sid) => { const d = deskOf(p); d.members ||= []; if (!d.members.includes(sid)) d.members.push(sid); };
  const itemOf = (id) => desk().projects.flatMap((d) => d.items).find((i) => i.id === id);
  const secs = () => Math.floor(w.now() / 1000);
  /** Pictures sent from an item's page as its session's chat keeps them (desk/hub/core/desk.rs desk_send_out): each
   *  saved, and named by where it is (here its data, which the window shows as it would a saved file). */
  const pics = (images) => (images || []).map((im) => im.data);
  let n = 0;
  /** A project's folder: where a session of it runs (the fake knows none for a project with no session). */
  const folderOf = (p) => deskOf(p).folder || w.state.sessions.find((s) => s.project === (deskOf(p).workspace_project || p) && s.cwd)?.cwd || "";
  return {
    desk_set_lead: ({ project, sid }) => {
      const d = deskOf(project);
      if (sid) {
        const s = w.session(sid);
        if (!s) throw "No such session";
        d.lead = { sid, name: s.name || sid, state: "live" };
      } else {
        const cwd = folderOf(project);
        if (!cwd) throw `no folder known for ${project}: start a session in it first`;
        const name = `${project}-lead`, id = `lead-${project}-${++n}`;
        w.state.sessions.push({ session_id: id, name, project, cwd, harness: "claude", hosted: true, state: "idle", since_ms: w.now(), thread: [], team: [], segments: [], queued: null, prompt: "" });
        d.lead = { sid: id, name, state: "live" };
      }
      w.changed();
      return d.lead;
    },
    // + New Desk: the folder's project (its last part, as a folder outside a repository is), and a lead started there.
    desk_delete: ({ id, closeSessions = [] }) => {
      desk().projects = desk().projects.filter((d) => d.project !== id);
      for (const sid of closeSessions) {
        const i = w.state.sessions.findIndex((s) => s.session_id === sid);
        if (i >= 0) (w.state.ended ||= []).unshift(...w.state.sessions.splice(i, 1));
      }
      w.changed(); return { kept: [] };
    },
    // Old working folders (desk_copies.rs): How work lands lists them (a Desk's `old_copies` in the world, as the
    // hub finds them); yes removes them.
    desk_old_copies_list: ({ desk: id }) => deskOf(id).old_copies || { copies: [], kb: 0 },
    desk_old_copies: ({ desk: id, yes }) => {
      const d = deskOf(id), o = d.old_copies;
      if (!yes) return { removed: 0, kb: 0, kept: [] };
      d.old_copies = null; w.changed();
      return { removed: o?.copies?.length || 0, kb: o?.kb || 0, kept: [] };
    },
    desk_rename: ({ id, name }) => {
      const d = deskOf(id); d.name = name.trim(); w.changed(); return null;
    },
    desk_start: ({ folder, name }) => {
      const cwd = String(folder || "").replace(/\/+$/, "");
      if (!cwd) throw "no such folder";
      const workspace = w.state.sessions.find((s) => s.cwd === cwd)?.project || cwd.split("/").pop();
      const project = `desk-${++n}`, sid = `lead-${project}`;
      const d = deskOf(project);
      d.id = project; d.independent = true; d.name = name || workspace; d.workspace_project = workspace; d.folder = cwd;
      const leadName = `${d.name}-lead`;
      w.state.sessions.push({ session_id: sid, name: leadName, project: workspace, cwd, harness: "claude", hosted: true, state: "idle", since_ms: w.now(), thread: [], team: [], segments: [], queued: null, prompt: "" });
      d.lead = { sid, name: leadName, state: "live" }; join(project, sid);
      w.changed();
      return { id: project, project, sid };
    },
    // New Desk's look at a folder (#927, ipc/desk.rs desk_look): one part a call, each answering after a pause. A spec
    // steers it through `w.lookFake`: `wait` (real ms per part, e.g. { github: 800 }, the preview's own pauses
    // otherwise), `fail` (a part's error, e.g. { sessions: "transcripts unreadable" }), `rowsFail` (desk_look_rows'
    // error) and `plain` (folders that aren't repositories). A folder ending in /cue is Cue's own source.
    desk_look: async ({ folder, part }) => {
      const f = w.lookFake || {}, dir = String(folder || "").replace(/\/+$/, "");
      if (f.wait?.[part] != null) await new Promise((r) => setTimeout(r, f.wait[part]));
      else await w.wait({ files: 300, github: 1400, history: 600, sessions: 1000 }[part] || 300);
      if (!dir) throw "No folder at ";
      if (f.fail?.[part]) throw f.fail[part];
      const repo = !(f.plain || []).includes(dir);
      if (part === "files") return {
        part,
        lines: [{ text: "Read package.json · build, test, lint scripts", file: "package.json" }, { text: "No Makefile", file: "Makefile" },
          ...(repo ? [{ text: "Read .github/workflows/ci.yml · runs lint and test", file: ".github/workflows/ci.yml" }] : []), { text: "Read scripts/ · deploy.sh", file: "scripts/" }],
        found: { scripts: { build: "npm run build", test: "npm test", lint: "npm run lint" }, ci: repo ? { file: ".github/workflows/ci.yml", run: ["npm run lint", "npm test"] } : null, deploy: "scripts/deploy.sh" },
      };
      if (part === "github") return repo
        ? { part, lines: [{ text: "GitHub: main needs a pull request with 1 review" }], found: { reached: true, needs_pr: true, reviews: 1, branch: "main" } }
        : { part, lines: [{ text: "Not a git repository: nothing to ask GitHub" }], found: { reached: false } };
      if (part === "history") return repo
        ? { part, lines: [{ text: "Last 30 changes on main: 28 pull requests, 2 direct" }], found: { count: 30, prs: 28, merges: 0, direct: 2 } }
        : { part, lines: [{ text: "Not a git repository: no changes to count" }], found: { count: 0 } };
      return { part, lines: [{ text: "12 past sessions here: 9 ran tests, 4 pushed" }], found: { count: 12, tests: { cmd: "npm test", names: ["fix-cart", "api-tests", "checkout-flow", "lint-pass", "search-box", "cart-badge", "price-fix", "login-copy", "img-sizes"] }, pushed: 4 } };
    },
    // What the look found as rows (ipc/desk.rs desk_look_rows): files win over sessions; a part missing counts as nothing.
    desk_look_rows: async ({ folder, found = {} }) => {
      const f = w.lookFake || {}, dir = String(folder || "").replace(/\/+$/, "");
      await w.wait(100);
      if (f.rowsFail) throw f.rowsFail;
      const place = /\/cue$/.test(dir) ? "cue" : (f.plain || []).includes(dir) ? "folder" : "repo";
      const lands = {
        cue: { where: "copy", how: "squash", check: "cue", after: "install", push: "ask", tests: "changed", at_once: 3, check_steps: [], after_steps: [] },
        repo: { where: "copy", how: "squash", check: "none", after: "nothing", push: "ask", tests: "all", at_once: 3, check_steps: [], after_steps: [] },
        folder: { where: "shared", how: "owner", check: "none", after: "nothing", push: "ask", tests: "all", at_once: 3, check_steps: [], after_steps: [] },
      }[place];
      const fi = found.files, gh = found.github, hi = found.history, se = found.sessions;
      const rows = [];
      const build = fi?.scripts?.build;
      rows.push(build ? { part: "build", label: "Builds with", tag: "found", steps: [`[Build] ${build}`], sources: [{ kind: "file", file: "package.json" }], why: "From package.json, its build script." }
        : { part: "build", label: "Builds with", tag: "usual", steps: [], sources: [], why: "" });
      if (place === "cue") rows.push({ part: "check", label: "Check first", tag: "usual", choice: "cue", steps: [], sources: [], why: "Cue's own check, as on Cue's own source." });
      else if (fi?.ci) rows.push({ part: "check", label: "Check first", tag: "found", choice: "command", steps: ["[Lint] npm run lint", "[Test] npm test"], sources: [{ kind: "file", file: fi.ci.file }], why: `The same steps your GitHub checks run, from ${fi.ci.file}.` });
      else if (se?.tests) rows.push({ part: "check", label: "Check first", tag: "found", choice: "command", steps: [`[Test] ${se.tests.cmd}`], sources: [{ kind: "sessions", names: se.tests.names, count: se.tests.names.length }], why: "" });
      else rows.push({ part: "check", label: "Check first", tag: "unsure", choice: "none", steps: [], sources: [], why: "Nothing says how it's checked. Until you pick, nothing is run." });
      let how = "squash";
      if (place === "repo") {
        if (gh?.needs_pr || (hi?.prs || 0) * 2 > (hi?.count || 0) && hi?.count) {
          how = "pr";
          rows.push({ part: "how", label: "Lands on main as", tag: "found", choice: "pr", steps: [], sources: [...(gh?.reached ? [{ kind: "github" }] : []), ...(hi ? [{ kind: "history" }] : [])], why: gh?.needs_pr ? "main on GitHub needs a pull request with one review, and your last 30 changes came in as pull requests." : "Your last 30 changes came in as pull requests." });
        } else rows.push({ part: "how", label: "Lands on main as", tag: "unsure", choice: "squash", steps: [], sources: [], why: "Nothing says how changes reach main. Until you pick, each lands as one commit." });
      } else if (place === "cue") rows.push({ part: "how", label: "Lands on main as", tag: "usual", choice: "squash", steps: [], sources: [], why: "As Cue's own source always lands." });
      if (place === "cue") rows.push({ part: "after", label: "Then runs", tag: "usual", choice: "install", steps: [], sources: [], why: "Installs or serves Cue, as on Cue's own source." });
      else if (fi?.deploy) rows.push({ part: "after", label: "Then runs", tag: "unsure", choice: "nothing", steps: [], sources: [{ kind: "file", file: fi.deploy }], why: `Found ${fi.deploy}, but nothing says it runs after each change. Until you pick, nothing runs.` });
      else rows.push({ part: "after", label: "Then runs", tag: "usual", choice: "nothing", steps: [], sources: [], why: "" });
      if (place !== "folder") rows.push(how === "pr"
        ? { part: "push", label: "Send to GitHub", tag: "follows", choice: "after", steps: [], sources: [], why: "A pull request needs the item's work on GitHub, so it's sent as each item finishes." }
        : { part: "push", label: "Send to GitHub", tag: "usual", choice: "ask", steps: [], sources: [], why: "You're asked first." });
      for (const r of rows) if (r.part !== "build") lands[r.part] = r.choice;
      const plainSteps = (r) => (r?.steps || []).map((s) => s.replace(/^\[[^\]]*\]\s*/, ""));
      const chk = rows.find((r) => r.part === "check");
      if (chk.choice !== "cue") { lands.check_steps = [...plainSteps(rows[0]), ...(chk.choice === "command" ? plainSteps(chk) : [])]; lands.check = lands.check_steps.length ? "command" : "none"; }
      const unsure = rows.filter((r) => r.tag === "unsure").length, known = rows.filter((r) => r.tag === "found" || r.tag === "follows").length;
      return { place, summary: `${known} found${unsure ? ` · ${unsure} to pick` : ""}`, rows, lands };
    },
    desk_send: ({ project, text }) => {
      const d = deskOf(project);
      if (!d.lead) throw "no lead";
      if (d.lead.state === "ended") throw "lead ended";
      const s = w.session(d.lead.sid);
      if (!s) throw "no lead";
      // Cue puts a summary of the Desk before what you wrote, as the hub does.
      w.working(s, `<desk project="${project}">\n${d.items.length} items.\n</desk>\n\n${text}`, []);
      w.changed();
      return { sid: s.session_id };
    },
    desk_add: ({ project, kind, title, see, doneWhen, holder }) => {
      const t = secs(), s = holder && w.session(holder);
      const it = { id: `x${++n}`, project, kind, title, see: see || "", done_when: doneWhen || "", state: s ? "working" : "new", holder: s ? { sid: holder, name: s.name || holder, kind: "session" } : null, after: null, after_why: "", latest: "", ask: null, files: [], created: t, updated: t, done_at: null, from: "you" };
      deskOf(project).items.push(it);
      if (s) join(project, holder);
      w.changed();
      return it;
    },
    desk_give: ({ id, sid }) => {
      const it = itemOf(id), s = w.session(sid);
      if (!it) throw "No such item";
      if (!s) throw "No such session";
      const t = secs();
      Object.assign(it, { state: "working", holder: { sid, name: s.name || sid, kind: "session" }, updated: t });
      join(it.project, sid);
      deskOf(it.project).decisions.unshift({ id: `g${++n}`, project: it.project, item_id: id, by: "You", sid: "", text: `Gave it to ${s.name || sid}`, why: "", undo: "none", at: t });
      w.changed();
      return null;
    },
    desk_member: ({ project, sid, on }) => {
      const s = w.session(sid);
      if (on && !s) throw "no such session";
      if (s && s.project !== (deskOf(project).workspace_project || project)) throw `that session isn't of ${project}`;
      if (on) join(project, sid);
      else { const d = deskOf(project); d.members = (d.members || []).filter((m) => m !== sid); if (d.lead?.sid === sid) d.lead = null; }
      w.changed();
      return null;
    },
    desk_item_act: ({ id, act, text, images }) => {
      // A dropped item's kept copy (#608): Remove deletes it; Pick up again puts its item back, new, for the lead,
      // with the copy (desk/hub/turn/act.rs desk_kept_pickup).
      const keptIn = desk().projects.find((d) => (d.kept || []).some((k) => k.id === id));
      if (keptIn && (act === "remove_kept" || act === "pickup_kept")) {
        const k = keptIn.kept.find((x) => x.id === id), t = secs();
        keptIn.kept = keptIn.kept.filter((x) => x.id !== id);
        if (act === "pickup_kept") keptIn.items.push({ ...(k.item || { kind: "task", title: k.title, see: "", done_when: "" }), id, project: k.project, state: "new", holder: null, kept_copy: { ...k, item: undefined }, after: null, ask: null, files: [], updated: t, done_at: null });
        w.changed();
        return act === "pickup_kept" ? itemOf(id) : { id, state: "dropped" };
      }
      const it = itemOf(id);
      if (!it) throw "No such item";
      const t = secs();
      // Where an unstarted item's session starts (#597, desk/hub/landing/remote_item.rs desk_start_on): "" this Mac,
      // or a machine the Desk has a folder on.
      if (act === "start_on") {
        if (it.holder || ["done", "dropped"].includes(it.state)) throw "it has started already: it stays where it is";
        const m = (deskOf(it.project).machines?.machines || []).find((x) => x.name === text);
        if (text && !m?.folder) throw `set the folder of this repository's clone on ${m?.label || text} in the Desk's settings first`;
        Object.assign(it, { start_on: text, on_machine: null, machine: m?.label || "", updated: t });
        w.changed();
        return it;
      }
      // A question on an item to check or done (#475): to its holder with Cue's words around it, as asked.rs sends it;
      // the item back at work while it answers.
      if (act === "note" && ["to_check", "done"].includes(it.state) && it.holder && /\?\s*$/.test(text || "")) {
        const sid = it.holder.lead_sid || it.holder.sid, s = w.session(sid);
        const stage = it.state === "done" ? "done" : "with the user to check";
        const message = `${it.holder.kind === "crew" ? `For your teammate ${it.holder.name}: ` : ""}A question about Desk item #${id} (${it.title}), which is ${stage}: ${text.trim()}\nAnswer it. If you change code for it, say DONE: #${id} once that's done; if you don't, the item stays as it was.\nWith your DONE, on a line of its own, write the one line shown to whoever checks it: CHECK: #${id} | Try: <…>, or CHECK: #${id} | Nothing to see: <why>, close it.`;
        deskOf(it.project).decisions.unshift({ id: `w${++n}`, project: it.project, item_id: id, by: "You", sid: "", text: text.trim(), why: `asked ${it.holder.name}`, at: t, undo: "none", call: false, delivery: { session_id: sid, text: message, at_ms: w.now(), went: s ? "sent" : "failed: its session has ended" } });
        Object.assign(it, { state: "working", updated: t });
        if (s) w.working(s, message, pics(images));
        w.changed();
        return it;
      }
      // A note on an item to check sends it back with the note (#437); a quick phrase is a note that never does (#488).
      if (act === "note" && it.state === "to_check" && it.holder) act = "send_back";
      if (act === "phrase") act = "note";
      // Your note to its holder (#437): on its Thread, how it went with it; to the session as about the item.
      if (act === "note") {
        if (!it.holder || ["done", "dropped"].includes(it.state)) throw "nobody holds it: it goes to the lead";
        const sid = it.ask?.sid || it.holder.lead_sid || it.holder.sid, s = w.session(sid);
        const message = `About #${id} (${it.title})${it.holder.kind === "crew" ? `, for ${it.holder.name}` : ""}: ${text}`;
        const went = !s ? "failed: its session has ended" : s.state === "working" ? "kept" : "sent";
        deskOf(it.project).decisions.unshift({ id: `w${++n}`, project: it.project, item_id: id, by: "You", sid: "", text: text.trim(), why: `to ${it.holder.name}`, at: t, undo: "none", call: false, delivery: { session_id: sid, text: message, at_ms: w.now(), went } });
        if (s?.state === "working") s.held = { role: "you", text: s.held ? `${s.held.text}\n\n${message}` : message, at_ms: s.held?.at_ms || w.now(), images: [...(s.held?.images || []), ...pics(images)] };
        else if (s) w.working(s, message, pics(images));
        w.changed();
        return it;
      }
      if (act === "retry") {
        const c = deskOf(it.project).decisions.find((x) => x.id === text && x.item_id === id);
        if (!c?.delivery?.went?.startsWith("failed")) throw "it went already";
        c.delivery.went = "sent";
        w.changed();
        return it;
      }
      if (act === "wrote_lead" || act === "wrote_holder") {
        const d = desk().projects.find((x) => x.project === it.project);
        d.decisions.unshift({ id: `w${++n}`, project: it.project, item_id: it.id, by: "You", sid: "", text, why: act === "wrote_lead" ? `to ${d.lead?.name || ""}` : `to ${it.holder?.name || ""}`, at: secs(), undo: "none", call: false });
        w.changed();
        return it;
      }
      if (act === "looks_right") Object.assign(it, { state: "done", done_at: t });
      // Undo on a done item: back to check, its commit kept, a waiting one not made (no code changes).
      else if (act === "reopen") { if (it.state !== "done") throw "It isn't done: nothing to undo"; Object.assign(it, { state: "to_check", done_at: null, commit_wait: null }); }
      else if (act === "send_back") {
        if (!text?.trim()) throw "Say what isn't right first: type it in the box, then Send back";
        // What followed each option was for the round being redone (#619): it goes.
        Object.assign(it, { state: "working", latest: `sent back: ${text}`, next: [] });
        const sid = it.holder?.lead_sid || it.holder?.sid, s = sid && w.session(sid);
        const prefix = it.holder?.kind === "crew" ? `For your teammate ${it.holder.name}: ` : "";
        const message = `${prefix}Desk item #${id} (${it.title}) isn't done: ${text}\nIts fixes stay in #${id}; don't add new items for them. Say DONE: #${id} when it is.`;
        deskOf(it.project).decisions.unshift({ id: `back${++n}`, project: it.project, item_id: id, by: "You", sid: "", text: text.trim(), why: `sent back to ${it.holder?.name}`, at: secs(), undo: "none", delivery: { session_id: sid, text: message, at_ms: w.now() } });
        if (s?.state === "working") s.held = { role: "you", text: s.held ? `${s.held.text}\n\n${message}` : message, at_ms: s.held?.at_ms || w.now(), images: [...(s.held?.images || []), ...pics(images)] };
        else if (s) w.working(s, message, pics(images));
      }
      // Your drag in Queued (#387): each takes its place.
      else if (act === "reorder") { text.split(" ").forEach((x, k) => { const q = itemOf(x); if (q) q.queue_order = k + 1; }); w.changed(); return it; }
      // At the limit on items at once (#712): refused, as the hub does; Raise and start goes.
      else if (act === "start_now" && it.start_full) throw `${it.start_full.on} already has ${it.start_full.n} of this Desk's items working at once, the most it takes. Raise that number to start it now, or let it wait its turn`;
      else if (act === "start_now_raise" && it.tree_wait && it.holder) { Object.assign(it, { state: "working", tree_wait: false, back_waiting: "" }); delete it.copy_users; delete it.start_full; }
      else if (act === "start_now" && it.tree_wait && it.holder) { Object.assign(it, { state: "working", tree_wait: false, back_waiting: "" }); delete it.copy_users; }
      else if (act === "start_now") Object.assign(it, { state: "working", after: null, after_why: "", latest: "starting in a new session" });
      else if (act === "put_before") { const first = itemOf(it.after); if (first) Object.assign(first, { state: "queued", after: it.id, after_why: it.after_why }); Object.assign(it, { state: "working", after: null, after_why: "" }); }
      else if (act === "drop") {
        it.state = "dropped";
        // A pick's build item dropped: its mocks item can be picked again (#281).
        const m = it.builds && itemOf(it.builds);
        if (m && m.built_as === it.id) { delete m.picked; delete m.built_as; }
      }
      // A pick on any item (#281, #543): done and picked; what its holder said follows that option (IF PICKED:, `next`)
      // filed, linked both ways; named nothing, one with a canvas gets one build item; a pick again makes none.
      else if (act === "pick") {
        if (it.built_as && itemOf(it.built_as)) throw `Option ${it.picked} is picked already: it's being built as #${it.built_as}`;
        if (it.picked) throw `Option ${it.picked} is picked already`;
        const canvas = it.designs || (it.finish?.mocks && (it.made || []).some((m) => /^https?:\/\//.test(m)));
        const option = String(text || "").trim().replace(/^option\s+/i, "").toUpperCase(), d = deskOf(it.project);
        const named = (it.next || []).filter((x) => x.option === "any" || x.option.toUpperCase() === option);
        const filed = named.map((x) => ({ id: `b${++n}`, project: it.project, kind: x.kind, title: x.title, see: x.see || "", done_when: x.done_when || "", state: "new", holder: null, from: "you", builds: it.id, from_pick: option, created: t, updated: t, files: [] }));
        if (!filed.length && canvas) filed.push({ id: `b${++n}`, project: it.project, kind: it.kind || "feature", title: `Build: ${it.title.replace(/^[^:\s]+(?:\s[^:\s]+)?:\s*/, "").replace(/\s\([^)]*\)$/, "")} (option ${option})`, done_when: "", state: "new", holder: null, from: "you", builds: it.id, from_pick: option, created: t, updated: t, files: [] });
        d.items.push(...filed);
        Object.assign(it, { state: "done", done_at: t, picked: option, built_as: filed[0]?.id || "" });
      }
      else throw `Unknown act ${act}`;
      it.updated = t;
      // Its cards in the lead's chat say where it is now, as the hub's view does.
      for (const c of desk().projects.flatMap((d) => d.checks || [])) if (c.id === it.id) Object.assign(c, { state: it.state }, act === "reopen" ? { commit_wait: null } : {});
      // Sent back: why, on its latest card, as the hub keeps it (pictures counted).
      const last = act === "send_back" && desk().projects.flatMap((d) => d.checks || []).filter((c) => c.id === it.id && c.last).at(-1);
      if (last) Object.assign(last, { back: text.trim(), back_pics: (images || []).length, back_by: "You" });
      w.changed();
      return null;
    },
    // Looks right: every file of the item's counts as changed and is committed at once ("abc1234"); when some are named
    // in `state.desk_busy` ([{ path, by }]: another session at work is editing them) it's done and the commit waits
    // (`commit_wait`); an item with no files is just done.
    desk_looks_right: ({ id }) => {
      const it = itemOf(id);
      if (!it) throw "No such item";
      if (it.state !== "to_check") throw "It isn't ready to check";
      const waiting = (w.state.desk_busy || []).filter((b) => it.files.includes(b.path)), files = it.files;
      if (waiting.length) {
        const commit_wait = { files: files.map((f) => ["/repo", f]), busy: waiting };
        Object.assign(it, { state: "done", done_at: secs(), updated: secs(), commit: "", commit_wait });
        for (const c of desk().projects.flatMap((d) => d.checks || [])) if (c.id === it.id) Object.assign(c, { state: "done", commit: "", commit_wait });
        w.changed();
        return { commit: "", files: files.length, waiting };
      }
      const words = w.state.settings?.project?.never?.[it.project] || [];
      const hit = files.length && words.find((x) => it.title.toLowerCase().includes(x.toLowerCase()));
      if (hit) throw `Not committed: "${hit}" is in the message`;
      const commit = files.length ? "abc1234" : "";
      Object.assign(it, { state: "done", done_at: secs(), updated: secs(), commit });
      for (const c of desk().projects.flatMap((d) => d.checks || [])) if (c.id === it.id) Object.assign(c, { state: "done", commit });
      w.changed();
      return { commit, files: files.length, waiting: [] };
    },
    // Files changed: a to-check or done item's files, each a small diff (+2 −1); a done item with no commit, none.
    desk_item_diff: ({ id }) => {
      const it = itemOf(id);
      if (!it) throw "no such item";
      // Still being worked on: what's changed so far (every file it's on, here), as the hub reads it (#149).
      const on = it.state === "to_check" || (it.state === "done" && (it.commit || it.commit_wait)) || (it.state !== "done" && !it.unchanged);
      const diff = on ? (it.files || []).map((f) => `diff --git a/${f} b/${f}\n--- a/${f}\n+++ b/${f}\n@@ -1,2 +1,3 @@\n-old ${f}\n+new ${f}\n+more\n same\n`).join("") : "";
      return { diff, machine: "" };
    },
    desk_commit_undo: ({ id }) => {
      const it = itemOf(id);
      if (!it) throw "No such item";
      if (it.state !== "done") throw "It isn't done: nothing to undo";
      // Back to check; its commit is taken back (the files stay as they are), or kept when `state.desk_commit_after`
      // (another commit came after it). Never any code change.
      const keep = it.commit && w.state.desk_commit_after, commit = keep ? it.commit : "";
      Object.assign(it, { state: "to_check", done_at: null, updated: secs(), commit, commit_wait: null });
      for (const c of desk().projects.flatMap((d) => d.checks || [])) if (c.id === it.id) Object.assign(c, { state: "to_check", commit, commit_wait: null });
      w.changed();
      return it;
    },
    // A project's done items, newest first, `take` after (beforeMs, beforeId): today's as the state has them and the
    // older ones kept only in Cue's database (`older`, a project each; never in the state).
    desk_done: ({ project, beforeMs, beforeId, take }) => {
      const d = deskOf(project);
      const key = (i) => [i.updated * 1000, i.id];
      const after = (i) => beforeMs == null || key(i)[0] < beforeMs || (key(i)[0] === beforeMs && i.id < beforeId);
      return [...d.items.filter((i) => i.state === "done"), ...(d.older || [])].filter(after)
        .sort((a, b) => b.updated - a.updated || (a.id < b.id ? 1 : -1)).slice(0, take);
    },
    // The lead's own moves (#374), newest first, `take` after (beforeMs, beforeId): today's as the state has them and
    // the older ones kept only in Cue's database (`moves`, a project each); none from before the list was cleared.
    desk_moves: ({ project, beforeMs, beforeId, take }) => {
      const d = deskOf(project), from = d.moves_cleared || 0;
      const after = (x) => beforeMs == null || x.at * 1000 < beforeMs || (x.at * 1000 === beforeMs && x.id < beforeId);
      return [...d.decisions.filter((x) => x.act), ...(d.moves || [])].filter((x) => x.at * 1000 > from && after(x))
        .sort((a, b) => b.at - a.at || (a.id < b.id ? 1 : -1)).slice(0, take);
    },
    desk_moves_clear: ({ project }) => {
      const d = deskOf(project);
      d.moves = [];
      d.moves_cleared = Date.now();
      w.changed();
    },
    desk_undo: ({ decisionId }) => {
      const dec = desk().projects.flatMap((d) => d.decisions).find((x) => x.id === decisionId);
      if (!dec) throw "No such decision";
      if (dec.undo !== "can") throw "It can't be undone now";
      dec.undo = "asked";
      w.changed();
      return "Asked its lead to undo it";
    },
    // A step held while you were away (desk/hub/away/away_holds.rs): answered, so it leaves its Desk's list.
    desk_held_answer: ({ id }) => {
      const d = desk().projects.find((x) => (x.held || []).some((h) => h.id === id));
      if (!d) throw "that step was already answered or isn't held any more";
      d.held = d.held.filter((h) => h.id !== id);
      w.changed();
    },
    // Away (#361): the Desk's away kept in the world, as desk_view has it; Go through the items now sends the lead nothing here.
    desk_away_set: ({ desk: id, away }) => {
      const d = desk().projects.find((x) => x.project === id || x.id === id);
      if (!d) throw "no Desk";
      d.away = { ...away, active: !!away.on, ended: "", since: w.now(), spent: 0 };
      w.changed();
      return structuredClone(d.away);
    },
    desk_away_sweep: () => null,
  };
});
