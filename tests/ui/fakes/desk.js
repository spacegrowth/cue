// Fakes for the Desk's commands (desk_set_lead, desk_send, desk_add, desk_give, desk_item_act, desk_undo): `state.desk`,
// one entry a project with its items, its lead and today's decisions. Times are seconds; a dropped item stays, as
// "dropped". A lead is a session like any other: desk_set_lead names one (or starts "<project>-lead", hosted), and
// desk_send hands it your message as a session takes one.
CueBridge.area("desk", (w) => {
  const desk = () => (w.state.desk ||= { projects: [] });
  /** Project `p`'s Desk, made empty when it has none yet. */
  const deskOf = (p) => desk().projects.find((d) => d.project === p) || (desk().projects.push({ project: p, lead: null, items: [], decisions: [] }), desk().projects.at(-1));
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
      w.changed();
      return it;
    },
    desk_give: ({ id, sid }) => {
      const it = itemOf(id), s = w.session(sid);
      if (!it) throw "No such item";
      if (!s) throw "No such session";
      const t = secs();
      Object.assign(it, { state: "working", holder: { sid, name: s.name || sid, kind: "session" }, updated: t });
      deskOf(it.project).decisions.unshift({ id: `g${++n}`, project: it.project, item_id: id, by: "You", sid: "", text: `Gave it to ${s.name || sid}`, why: "", undo: "none", at: t });
      w.changed();
      return null;
    },
    desk_item_act: ({ id, act, text }) => {
      const it = itemOf(id);
      if (!it) throw "No such item";
      const t = secs();
      if (act === "looks_right") Object.assign(it, { state: "done", done_at: t });
      else if (act === "send_back") Object.assign(it, { state: "working", latest: text ? `sent back: ${text}` : "sent back" });
      else if (act === "start_now") Object.assign(it, { state: "working", after: null, after_why: "", latest: "starting in a new session" });
      else if (act === "put_before") { const first = itemOf(it.after); if (first) Object.assign(first, { state: "queued", after: it.id, after_why: it.after_why }); Object.assign(it, { state: "working", after: null, after_why: "" }); }
      else if (act === "drop") it.state = "dropped";
      else throw `Unknown act ${act}`;
      it.updated = t;
      w.changed();
      return null;
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
