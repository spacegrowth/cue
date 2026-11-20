// Pending decisions belong to sessions, including when their current work has no item tag.
function deskSessionRequests(sid, agent = "") {
  return state.items.filter((it) => it.session_id === sid && actionableRequest(it)
    && (agent ? it.agent_id === agent : !mateOfAsk(it)));
}

/** Match a pending permission to its task, using the prompt saved when Cue received it. */
function deskTaskApprovals(d) {
  const tasks = new Map();
  for (const request of state.items) {
    if (!actionableRequest(request) || request.kind !== "permission") continue;
    const live = d.items.find((i) => i.ask?.item_id === request.id);
    const prompt = request.thread?.findLast((e) => e.role === "you" && !e.decision);
    const tag = /^(?:Desk item|About) #([a-z0-9_-]+)\b/i.exec(prompt?.text || "")?.[1];
    const held = (i) => !["done", "dropped"].includes(i.state) && (i.holder?.lead_sid || i.holder?.sid) === request.session_id;
    // A crew member's ask carries its brief, not "Desk item #id" (#299): its own item, else the one its session holds.
    const crewOf = request.agent_id && d.items.filter((i) => held(i) && i.holder?.kind === "crew" && i.holder.sid === request.agent_id);
    const only = d.items.filter(held);
    const task = live || (tag && d.items.find((i) => deskIdIs(i, tag) && held(i)))
      || (crewOf?.length === 1 && crewOf[0]) || (!tag && only.length === 1 && only[0]) || null;
    if (task && !["done", "dropped"].includes(task.state) && !tasks.has(task.id)) tasks.set(task.id, request);
  }
  return tasks;
}

/** The same live request and response handler used by the session's full chat. */
function deskSessionRequestFoot(d, open) {
  const sid = open?.kind === "crew" ? open.lead : open?.sid;
  if (!sid) return deskBox(d);
  const request = deskSessionRequests(sid, open.kind === "crew" ? open.agent : "")[0];
  if (!request) return deskBox(d);
  const key = deskBoxKey({ sid, ...(open.kind === "crew" && { mate: deskMate(sid, open.agent)?.name || "" }) });
  return `<div class="dk-ask">${askFoot(request, sid, request.harness || "claude", key, "No")}</div>`;
}
