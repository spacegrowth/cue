// Fakes for the Desk's commands (desk_send, desk_add, desk_item_act, desk_undo): `state.desk`, one entry a project with
// its items, its Dispatch session and today's decisions. Times are seconds; a dropped item stays, as "dropped".
// Dispatch is a hosted session like any other; the fake starts one the first time you write to a project's Dispatch,
// and it takes your message as a session takes one.
CueBridge.area("desk", (w) => {
  const desk = () => (w.state.desk ||= { projects: [] });
  /** Project `p`'s Desk, made empty when it has none yet. */
  const deskOf = (p) => desk().projects.find((d) => d.project === p) || (desk().projects.push({ project: p, dispatch: null, items: [], decisions: [] }), desk().projects.at(-1));
  const itemOf = (id) => desk().projects.flatMap((d) => d.items).find((i) => i.id === id);
  const secs = () => Math.floor(w.now() / 1000);
  let n = 0;
  /** Its Dispatch session, started when there's none (a hosted Claude session in the project's folder). */
  function dispatchOf(d) {
    let s = d.dispatch && w.session(d.dispatch);
    if (s) return s;
    const sid = `disp-${d.project}-${++n}`;
    s = { session_id: sid, name: "Dispatch", project: d.project, cwd: `/work/${d.project}`, harness: "claude", hosted: true, state: "idle", since_ms: w.now(), thread: [], team: [], segments: [], queued: null, prompt: "" };
    w.state.sessions.push(s);
    d.dispatch = sid;
    return s;
  }
  return {
    desk_send: ({ project, text, trust }) => {
      const s = dispatchOf(deskOf(project));
      // Cue puts a summary of the Desk before what you wrote, as the hub does.
      w.working(s, `<desk project="${project}">\n${deskOf(project).items.length} items.\n</desk>\n\n${text}`, []);
      w.changed();
      return { sid: s.session_id };
    },
    desk_add: ({ project, kind, title, doneWhen }) => {
      const t = secs();
      const it = { id: `x${++n}`, project, kind, title, done_when: doneWhen || "", state: "new", holder: null, after: null, after_why: "", latest: "", ask: null, files: [], created: t, updated: t, done_at: null, from: "you" };
      deskOf(project).items.push(it);
      w.changed();
      return it;
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
