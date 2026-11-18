// Pending decisions belong to sessions, including when their current work has no item tag.
function deskSessionRequests(sid, agent = "") {
  return state.items.filter((it) => it.session_id === sid && it.status === "pending" && it.kind !== "waiting"
    && !answering.has(it.id) && (agent ? it.agent_id === agent : !mateOfAsk(it)));
}

/** Match a pending permission to its task, using the prompt saved when Cue received it. */
function deskTaskApprovals(d) {
  const tasks = new Map();
  for (const request of state.items) {
    if (request.status !== "pending" || request.kind !== "permission") continue;
    const live = d.items.find((i) => i.ask?.item_id === request.id);
    const prompt = request.thread?.findLast((e) => e.role === "you" && !e.decision);
    const tag = /^(?:Desk item|About) #([a-z0-9_-]+)\b/i.exec(prompt?.text || "")?.[1];
    const task = live || (tag && d.items.find((i) => deskIdIs(i, tag)
      && (i.holder?.lead_sid || i.holder?.sid) === request.session_id));
    if (task && !["done", "dropped"].includes(task.state) && !tasks.has(task.id)) tasks.set(task.id, request);
  }
  return tasks;
}

function deskApprovals(d, taskApprovals) {
  const placed = new Set([...taskApprovals.values()].map((request) => request.id));
  const requests = state.items.filter((it) => !placed.has(it.id) && deskHasSession(d, it.session_id) && it.status === "pending" && it.kind !== "waiting" && !answering.has(it.id));
  if (!requests.length) return "";
  return `<section class="dk-approvals" aria-label="Pending requests"><div class="dk-requests-head">Needs your decision <span class="n">${requests.length}</span></div>${requests.map((it) => {
    const who = deskHolderName(it.session_id), command = it.tool_input?.command;
    const preview = it.kind === "input" ? formBody(it) : command ? `<pre>${esc(command)}</pre>` : `<p>${esc(summary(it))}</p>`;
    const actions = ["permission", "input"].includes(it.kind) && !inTerminal(it) ? decisionButtons(it, "No") : "";
    const target = it.agent_id && deskMate(it.session_id, it.agent_id)
      ? `data-desk-crew="${esc(it.agent_id)}" data-lead="${esc(it.session_id)}"` : `data-desk-chat-of="${esc(it.session_id)}"`;
    return `<div class="dk-request" data-desk-request="${esc(it.id)}"><button class="dk-request-who" ${target}>${esc(who)}${it.agent_name ? ` · ${esc(it.agent_name)}` : ""} ›</button>${preview}${actions}</div>`;
  }).join("")}</section>`;
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
