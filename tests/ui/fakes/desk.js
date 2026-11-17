// Fakes for the Desk's commands (desk_set_lead, desk_start, desk_send, desk_add, desk_give, desk_member, desk_item_act,
// desk_looks_right, desk_commit_undo, desk_item_diff, desk_done, desk_undo): `state.desk`,
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
  let n = 0;
  /** A project's folder: where a session of it runs (the fake knows none for a project with no session). */
  const folderOf = (p) => w.state.sessions.find((s) => s.project === p && s.cwd)?.cwd || "";
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
    desk_start: ({ folder }) => {
      const cwd = String(folder || "").replace(/\/+$/, "");
      if (!cwd) throw "no such folder";
      const project = w.state.sessions.find((s) => s.cwd === cwd)?.project || cwd.split("/").pop();
      const d = deskOf(project);
      if (d.lead) return { project, sid: d.lead.sid };
      const name = `${project}-lead`, id = `lead-${project}-${++n}`;
      w.state.sessions.push({ session_id: id, name, project, cwd, harness: "claude", hosted: true, state: "idle", since_ms: w.now(), thread: [], team: [], segments: [], queued: null, prompt: "" });
      d.lead = { sid: id, name, state: "live" };
      w.changed();
      return { project, sid: id };
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
    desk_add: ({ project, kind, title, doneWhen, holder }) => {
      const t = secs(), s = holder && w.session(holder);
      const it = { id: `x${++n}`, project, kind, title, done_when: doneWhen || "", state: s ? "working" : "new", holder: s ? { sid: holder, name: s.name || holder, kind: "session" } : null, after: null, after_why: "", latest: "", ask: null, files: [], created: t, updated: t, done_at: null, from: "you" };
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
      if (s && s.project !== project) throw `that session isn't of ${project}`;
      if (on) join(project, sid);
      else { const d = deskOf(project); d.members = (d.members || []).filter((m) => m !== sid); }
      w.changed();
      return null;
    },
    desk_item_act: ({ id, act, text, images }) => {
      const it = itemOf(id);
      if (!it) throw "No such item";
      const t = secs();
      if (act === "wrote_lead" || act === "wrote_holder") {
        const d = desk().projects.find((x) => x.project === it.project);
        d.decisions.unshift({ id: `w${++n}`, project: it.project, item_id: it.id, by: "You", sid: "", text, why: act === "wrote_lead" ? `to ${d.lead?.name || ""}` : `to ${it.holder?.name || ""}`, at: secs(), undo: "none", call: false });
        w.changed();
        return it;
      }
      if (act === "looks_right") Object.assign(it, { state: "done", done_at: t });
      else if (act === "send_back") { if (!text?.trim()) throw "Say what isn't right first: type it in the box, then Send back"; Object.assign(it, { state: "working", latest: `sent back: ${text}` }); }
      else if (act === "start_now") Object.assign(it, { state: "working", after: null, after_why: "", latest: "starting in a new session" });
      else if (act === "put_before") { const first = itemOf(it.after); if (first) Object.assign(first, { state: "queued", after: it.id, after_why: it.after_why }); Object.assign(it, { state: "working", after: null, after_why: "" }); }
      else if (act === "drop") it.state = "dropped";
      else throw `Unknown act ${act}`;
      it.updated = t;
      // Its cards in the lead's chat say where it is now, as the hub's view does.
      for (const c of desk().projects.flatMap((d) => d.checks || [])) if (c.id === it.id) c.state = it.state;
      // Sent back: why, on its latest card, as the hub keeps it (pictures counted).
      const last = act === "send_back" && desk().projects.flatMap((d) => d.checks || []).filter((c) => c.id === it.id && c.last).at(-1);
      if (last) Object.assign(last, { back: text.trim(), back_pics: (images || []).length });
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
      const on = it.state === "to_check" || (it.state === "done" && (it.commit || it.commit_wait));
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
    desk_undo: ({ decisionId }) => {
      const dec = desk().projects.flatMap((d) => d.decisions).find((x) => x.id === decisionId);
      if (!dec) throw "No such decision";
      if (dec.undo !== "can") throw "It can't be undone now";
      dec.undo = "asked";
      w.changed();
      return "Asked its lead to undo it";
    },
  };
});
