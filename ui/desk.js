// Cue UI desk — the Desk view: one project's items (a bug, a feature, a task), who holds each (a lead and its crew, a
// session on its own, or the add-on's), what each waits on, the chat with the project's lead (desk_lead.js: who runs
// the Desk, Give to…, the quick-add line) and today's decisions. With All projects picked, every project's Desk as a
// card (what waits on you, what got done, what was decided).
// The data is Cue's `state.desk` (hub: items, the lead, holders by session, decisions); the commands are desk_send,
// desk_add, desk_give, desk_set_lead, desk_item_act and desk_undo.

/** What the right card shows besides the lead's chat: null (the chat, or Decisions), or { kind: "item", id },
 *  { kind: "holder", sid } (a lead or a session), { kind: "crew", lead, agent } (a lead's crew member), or
 *  { kind: "addon", key } (a holder the add-on added); each with the `project` it was opened in (another project
 *  picked closes it). Its row in the list lights in place. */
let deskOpen = null;
/** The right card's tab while nothing is open: "lead" (its chat) or "decisions". */
let deskTab = "lead";
/** Done today unfolded into rows. */
let deskDoneOpen = false;
/** The item whose Drop was clicked once: it now reads "Drop it", and a second click drops it. */
let deskDropArm = null;
/** Where each box on the Desk sends, by its draft key: { project, item } (the lead, naming the item when there is
 *  one) or { sid, mate } (a session, naming its crew member). Kept beside the key, never read back out of it. */
const deskBoxes = new Map();
/** What you wrote to a project's lead that its chat doesn't show yet: project → [{ text, at, sent }] (`sent`: when
 *  the send returned, or 0). */
const deskSent = new Map();
/** The lead's chat was at its end before the draw (it stays there as new messages come). */
let deskChatEnd = true;

const DESK_KINDS = ["bug", "feature", "task"];
/** Seconds or ms, as the hub stamps them, in ms. */
const deskMs = (t) => (!t ? 0 : t < 1e12 ? t * 1000 : t);
/** Project `p`'s Desk ({ project, lead, items, decisions }), or an empty one. A dropped item is gone from it. */
const deskOf = (p) => {
  const d = (state.desk?.projects || []).find((x) => x.project === p);
  return d ? { ...d, items: d.items.filter((i) => i.state !== "dropped") } : { project: p, lead: null, items: [], decisions: [] };
};
const deskItem = (p, id) => deskOf(p).items.find((i) => i.id === id) || null;
/** Since midnight, here. */
const deskToday = (t) => deskMs(t) >= new Date(now()).setHours(0, 0, 0, 0);
const deskWaits = (i) => i.state === "needs_you" || i.state === "to_check";
const deskDoneToday = (i) => i.state === "done" && deskToday(i.done_at || i.updated);
const deskMoving = (i) => i.state === "working" || i.state === "queued";
/** A title cut short for a button or a line ("Search stays centred"): its first few words. */
const deskShort = (t) => { const w = String(t || "").split(/\s+/); return w.length > 3 ? w.slice(0, 3).join(" ") : t; };
const deskCap = (s) => String(s || "").replace(/^./, (c) => c.toUpperCase());

// ---------- holders: a lead (its crew from its team), a session on its own, or the add-on's ----------

/** The lead's teammates still at it or waiting (team.js mateList), or none for a session that isn't one. */
const deskCrew = (sid) => (typeof mateList === "function" ? mateList(sid) : []);
/** A crew member of lead `lead` by its holder id or name. */
const deskMate = (lead, key) => (sessionOf(lead)?.team || []).find((t) => t.agent_id === key || t.call === key || t.name === key) || null;
/** The group an item is listed under: a crew member's item under its lead. */
const deskGroupKey = (h) => (!h ? "" : h.kind === "crew" ? h.lead_sid || h.sid : h.sid);
/** A holder's name: what the hub named it, else the session's name. */
const deskHolderName = (sid, fallback = "") => fallback || (sessionOf(sid) ? nameOf(sid, sessionOf(sid).project) : sid);
/** Whether session `sid` leads a crew, or is given items as a lead. */
const deskIsLead = (sid, items) => deskCrew(sid).length > 0 || items.some((i) => i.holder?.kind === "lead" || i.holder?.kind === "crew");
/** What a holder is, in a few words: "lead · crew of 2", "lead · on its own", "on its own", "loop". */
function deskHolderSub(sid, items, kind) {
  if (kind === "loop") return "loop";
  if (!deskIsLead(sid, items)) return "on its own";
  const n = deskCrew(sid).length;
  return n ? `lead · crew of ${n}` : "lead · on its own";
}

/** The list's groups: Waiting on you, Not given out yet, one a holder (in the order its first item came), the
 *  add-on's, and Done today. An item is in one group only. */
function deskGroups(project) {
  const d = deskOf(project);
  const items = [...d.items].sort((a, b) => deskMs(a.created) - deskMs(b.created));
  // What asks you something before what's ready to check.
  const waiting = items.filter(deskWaits).sort((a, b) => (a.state === "needs_you" ? 0 : 1) - (b.state === "needs_you" ? 0 : 1));
  const done = items.filter(deskDoneToday).sort((a, b) => deskMs(b.done_at) - deskMs(a.done_at));
  const rest = items.filter((i) => !deskWaits(i) && i.state !== "done");
  const holders = new Map();
  for (const i of rest.filter((x) => x.holder)) {
    const k = deskGroupKey(i.holder);
    if (!holders.has(k)) holders.set(k, { sid: k, kind: i.holder.kind === "crew" ? "lead" : i.holder.kind, name: i.holder.kind === "crew" ? deskHolderName(k) : deskHolderName(k, i.holder.name), items: [] });
    holders.get(k).items.push(i);
  }
  // A lead whose items all wait on you is still a holder: its head stays, so its crew can be opened.
  for (const i of waiting.filter((x) => x.holder)) {
    const k = deskGroupKey(i.holder);
    if (!holders.has(k) && sessionOf(k)) holders.set(k, { sid: k, kind: i.holder.kind === "crew" ? "lead" : i.holder.kind, name: deskHolderName(k, i.holder.kind === "crew" ? "" : i.holder.name), items: [] });
  }
  for (const h of holders.values()) h.sub = deskHolderSub(h.sid, d.items.filter((i) => deskGroupKey(i.holder) === h.sid), h.kind);
  let extra = [];
  try { extra = addOn.deskHolders?.(project) || []; } catch (e) { console.error(e); }
  return { d, waiting, holders: [...holders.values()], fresh: rest.filter((i) => !i.holder), extra, done, all: items };
}

// ---------- the list ----------

/** An item's second line: what it waits on, or what its holder is on now. */
function deskItemLine(i) {
  if (i.state === "needs_you") return esc(i.ask?.text || `${i.holder?.name || "Its holder"} asks you something`);
  if (i.state === "to_check") return "Ready to check";
  if ((i.state === "new" || i.state === "queued") && i.route_failed) return `Couldn't give it out: ${esc(i.route_failed)}`;
  if (i.state === "queued") {
    const first = i.after && deskItem(i.project, i.after);
    return first ? `next · after ${esc(first.title)}${i.after_why ? ` (${esc(i.after_why)})` : ""}` : "next";
  }
  if (i.state === "new") return "Not given out yet";
  if (i.state === "done") return "Done";
  const who = i.holder?.kind === "crew" ? i.holder.name : "";
  const what = [who, i.latest].filter(Boolean).join(", ");
  return `<b>Now</b>${what ? ` · ${esc(what)}` : ""}`;
}
/** One item: its kind, title and age; the line under it. Lit when it's open, or its crew member is. */
function deskItemRow(i, lit) {
  const on = lit || (deskOpen?.kind === "item" && deskOpen.id === i.id);
  // No one holds it: "Give to…" under its line (a plain new one's line is just that: its group says the rest).
  const give = !i.holder && i.state !== "done" ? `<div class="dk-h dk-hg">${i.state === "new" && !i.route_failed ? "" : `${deskItemLine(i)} · `}${deskGive(i, "row")}</div>` : "";
  return `<div class="dk-it ${deskWaits(i) ? "ask" : ""} ${i.state === "queued" ? "next" : ""} ${on ? "on" : ""}" role="button" tabindex="0" data-desk-item="${esc(i.id)}">
    <div class="dk-t"><span class="dk-k">${esc(i.kind)}</span>${esc(i.title)}</div><span class="dk-age">${ago(deskMs(i.created))}</span>
    ${give || `<div class="dk-h">${deskItemLine(i)}</div>`}</div>`;
}
/** A holder's head: its name, what it is, › (it opens). */
const deskHolderHead = (h, on) => `<div class="dk-holder ${on ? "on" : ""}" role="button" tabindex="0" ${h.addon ? `data-desk-addon="${esc(h.key)}"` : `data-desk-holder="${esc(h.sid)}"`}><span class="dk-nm">${esc(h.name)}</span><span class="dk-s">${esc(h.sub || "")}</span><span class="grow"></span><span class="dk-go">›</span></div>`;
/** The item a crew member is on (lit with it while its pane is open). */
const deskCrewItem = (project, lead, agent) => { const t = deskMate(lead, agent); return deskOf(project).items.find((i) => i.holder?.kind === "crew" && (i.holder.sid === agent || (t && i.holder.name === t.name)) && i.state !== "done") || null; };

function deskList(project) {
  const g = deskGroups(project);
  const o = deskOpen;
  const crewLit = o?.kind === "crew" ? deskCrewItem(project, o.lead, o.agent)?.id : null;
  const row = (i) => deskItemRow(i, i.id === crewLit);
  const headOn = (sid) => (o?.kind === "holder" && o.sid === sid) || (o?.kind === "crew" && o.lead === sid);
  const parts = [];
  if (g.waiting.length) parts.push(`<div class="dk-gh you">Waiting on you <span class="n">${g.waiting.length}</span></div>${g.waiting.map(row).join("")}`);
  if (g.fresh.length) parts.push(`<div class="dk-gh">Not given out yet <span class="n">${g.fresh.length}</span></div>${g.fresh.map(row).join("")}`);
  for (const h of g.holders) parts.push(deskHolderHead(h, headOn(h.sid)) + h.items.map(row).join(""));
  for (const h of g.extra) parts.push(deskHolderHead({ ...h, addon: true, key: h.key || h.name }, o?.kind === "addon" && o.key === (h.key || h.name)) + (h.items || []).map(row).join(""));
  if (g.done.length) {
    parts.push(`<button class="dk-gh fold" data-desk-done aria-expanded="${deskDoneOpen}">Done today <span class="n">${g.done.length}</span></button>`);
    parts.push(deskDoneOpen ? g.done.map(row).join("") : `<button class="dk-folded" data-desk-done>${g.done.map((i) => esc(i.title)).join(" · ")} ▸</button>`);
  }
  const body = parts.join("") || `<div class="dk-empty">No items yet.</div>`;
  return `<div class="dk-card dk-items"><div class="dk-ch"><b>Items</b>${deskRunBy(g.d)}</div>
    ${deskQuickAdd()}<div class="dk-scroll" data-scroll-key="desk-items:${esc(project)}">${body}</div></div>`;
}

// ---------- the right card: the lead's chat, Decisions, or what's open ----------

/** A decision: who decided and when, what, why, and Undo this (or what became of it). `head`: the line above it. */
function deskDecision(c, head = null) {
  const undo = c.undo === "can" ? `<button class="btn small" data-desk-undo="${esc(c.id)}">Undo this</button>`
    : c.undo === "asked" ? `<span class="dk-u">Undo asked</span>` : c.undo === "one_way" ? `<span class="dk-u">Can't be undone</span>` : "";
  return `<div class="dk-dec"><div class="dk-w">${head ?? `<b>${esc(c.by)}</b> decided, ${ago(deskMs(c.at))} ago`}</div><div class="dk-d">${esc(c.text)}</div>${c.why ? `<div class="dk-y">${esc(c.why)}</div>` : ""}${undo ? `<div class="dk-undo">${undo}</div>` : ""}</div>`;
}
/** What you wrote to the lead, without the summary of the Desk (and, the first time, its instructions) Cue puts before
 *  it: a block of exactly that shape (`<desk project="…" …>`, its lines, `</desk>`, a blank line), so words of yours
 *  that start with <desk> stay, even typed in that shape without the project. */
const deskSaid = (t) => String(t ?? "").replace(/^<desk project="[^"]*"[^>\n]*>\n[\s\S]*?\n<\/desk>\n\n/, "");
/** The lead's chat: its session's messages, drawn as any session's chat is (chat/render.js), your words without the
 *  Desk's summary; then what you sent that it doesn't show yet (given up a minute after the send returned). */
function deskChat(d) {
  const s0 = deskLeadSession(d);
  const s = s0 && { ...s0, thread: (s0.thread || []).map((e) => (e.role === "you" ? { ...e, text: deskSaid(e.text) } : e)) };
  const thread = s?.thread || [];
  const pending = (deskSent.get(d.project) || []).filter((p) => !(p.sent && now() - p.sent > 60000)
    && !thread.some((e) => e.role === "you" && e.at_ms >= p.at - 60000 && e.text.includes(p.text)));
  deskSent.set(d.project, pending);
  const rows = s && thread.length ? chatHtml(null, s, s.harness || "claude") : "";
  const mine = pending.map((p) => `<div class="cv-you sending"><div class="cv-you-text">${esc(p.text).replace(/\n/g, "<br>")}</div><div class="cv-meta">Sending</div></div>`).join("");
  const empty = !rows && !mine ? `<div class="dk-empty">Tell ${esc(deskLeadName(d))} about a bug, a feature or a task. It adds them here, does each or hands it out, and says why.</div>` : "";
  return `<div class="dk-chat" data-desk-chat="${esc(d.project)}">${empty}${rows}${mine}</div>`;
}
function deskDecisions(d) {
  if (!d.decisions.length) return `<div class="dk-empty">Nothing decided today.</div>`;
  return `<div class="dk-pane" data-scroll-key="decisions:${esc(d.project)}">${d.decisions.map((c) => deskDecision(c)).join("")}</div>`;
}
const deskSec = (title, html) => (html ? `<div class="dk-sec"><h3>${title}</h3>${html}</div>` : "");
const deskBack = (label, to = "") => `<button class="dk-back" data-desk-back="${esc(to)}">‹ ${esc(label)}</button>`;

/** What holds an item up: the item it waits for, then this one; or its holder's question. */
function deskChain(i) {
  if (i.state === "needs_you" && i.ask) {
    const it = i.ask.item_id && findItem(i.ask.item_id);
    const qs = it?.kind === "question" ? questions(it) : [];
    const choices = qs.length === 1 ? (qs[0].options || []).map((op, oi) => `<button class="btn small" data-pick="0:${oi}" data-id="${esc(it.id)}">${esc(op.label)}</button>`).join("")
      : `<button class="btn small" data-desk-inbox="${esc(i.ask.sid)}">Answer it</button>`;
    return `<div class="dk-chain"><div class="dk-ln"><i class="ask"></i><div><div class="dk-lt">${esc(i.ask.text)}</div><div class="dk-acts">${choices}</div></div></div></div>`;
  }
  if (i.state !== "queued" || !i.after) return "";
  const first = deskItem(i.project, i.after);
  if (!first) return "";
  const who = first.holder?.kind === "crew" ? `${first.holder.name} (${deskHolderName(first.holder.lead_sid)}'s crew)` : first.holder?.name || "";
  const files = (first.files || []).map((f) => f.split("/").pop());
  const sub = [who, first.latest, files.length ? `has ${files.join(", ")} open` : ""].filter(Boolean).join(" · ");
  return `<div class="dk-chain"><div class="dk-ln"><i></i><div><div class="dk-lt">${esc(first.title)}</div><div class="dk-ls">${esc(sub)}</div></div></div>
    <div class="dk-ln"><i class="wait"></i><div><div class="dk-lt">This one</div><div class="dk-ls">starts when ${esc(deskShort(first.title))} is done${i.after_why ? `: ${esc(i.after_why)}` : ""}</div></div></div></div>`;
}
/** An item's buttons, by where it is. */
function deskItemActs(i) {
  const b = (act, label, look = "") => `<button class="btn small ${look}" data-desk-act="${act}" data-id="${esc(i.id)}">${esc(label)}</button>`;
  const first = i.after && deskItem(i.project, i.after);
  const acts = i.state === "to_check" ? b("looks_right", "Looks right", "primary") + b("send_back", "Send back")
    : i.state === "queued" ? b("start_now", "Start it now in a new session") + (first ? b("put_before", `Put it before ${deskShort(first.title)}`) : "")
    : i.state === "new" ? b("start_now", "Start it now in a new session") : "";
  const give = !i.holder && i.state !== "done" ? deskGive(i, "pane") : "";
  return acts || give ? `<div class="dk-acts">${give}${acts}</div>` : "";
}
/** Where it stands in a line: "Bug · next for web-lead · added by you 20m ago". */
function deskItemState(i) {
  const who = i.holder ? `<b>${esc(i.holder.name)}</b>` : "";
  const by = (pre) => (who ? ` ${pre} ${who}` : "");
  const where = { new: "not given out yet", queued: `next${by("for")}`, working: who ? `with ${who}` : "at work", needs_you: `waits on you${who ? ` · with ${who}` : ""}`, to_check: `ready to check${by("· from")}`, done: `done${by("by")}` }[i.state] || "";
  return `${esc(deskCap(i.kind))} · ${where} · added by ${esc(deskFrom(i, deskOf(i.project)))} ${ago(deskMs(i.created))} ago`;
}
function deskItemPane(i, d) {
  const decs = d.decisions.filter((c) => c.item_id === i.id);
  const files = (i.files || []).length ? `<div class="dk-files">${i.files.map(esc).join(" · ")}</div>` : "";
  const armed = deskDropArm === i.id;
  return `<div class="dk-pane" data-scroll-key="item:${esc(i.id)}">${deskBack(deskLeadName(d) || "Back")}<h1>${esc(i.title)}</h1><div class="dk-state">${deskItemState(i)}</div>
    ${deskSec("Couldn't give it out", i.route_failed ? `<p>${esc(i.route_failed)}</p>` : "")}
    ${deskSec(i.state === "needs_you" ? "It asks you" : "What's holding it up", deskChain(i))}${deskItemActs(i)}
    ${deskSec("Done when", i.done_when ? `<p>${esc(i.done_when)}</p>` : "")}
    ${deskSec("Latest", i.latest && i.state !== "queued" ? `<p>${esc(i.latest)}</p>` : "")}
    ${deskSec("Files", files)}
    ${deskSec(`Decisions · ${decs.length}`, decs.map((c) => deskDecision(c)).join(""))}
    ${i.state !== "done" ? `<button class="dk-drop${armed ? " armed" : ""}" data-desk-act="drop" data-id="${esc(i.id)}">${armed ? "Drop it" : "Drop this item"}</button>` : ""}</div>`;
}
/** A session's latest words in a line. */
const deskLatestOf = (s) => plain(firstLine([...(s?.thread || [])].reverse().find((e) => e.role === "agent")?.text || ""));
/** A crew member's dot: red when it asks you, filled at work, hollow otherwise. */
const deskMateDot = (lead, t) => (mateNeeds(lead, t) ? "ask" : t.state === "working" ? "" : "idle");
/** A lead (its crew, what's next, what it decided, its latest) or a session on its own. */
function deskHolderPane(sid, d) {
  const s = sessionOf(sid);
  const mine = d.items.filter((i) => deskGroupKey(i.holder) === sid && i.state !== "done");
  const name = deskHolderName(sid, mine.find((i) => i.holder?.kind !== "crew")?.holder?.name);
  const crew = deskCrew(sid), lead = deskIsLead(sid, mine);
  const waits = mine.filter(deskWaits).length;
  const head = [lead ? "Lead" : "Session", `${mine.length} item${mine.length === 1 ? "" : "s"}${waits ? `, ${waits} waiting on you` : ""}`, lead ? (crew.length ? `crew of ${crew.length}` : "on its own") : "", s?.since_ms ? `since ${clockAt(s.since_ms)}` : ""].filter(Boolean).join(" · ");
  const crewRows = crew.map((t) => {
    const it = deskCrewItem(d.project, sid, t.agent_id || t.call);
    const files = (t.files || []).map((f) => f.split("/").pop()).join(", ");
    const line = [it ? it.title : t.what, mateNeeds(sid, t) ? "stopped to ask you" : t.step, files].filter(Boolean).join(" · ");
    return `<div class="dk-mb" role="button" tabindex="0" data-desk-crew="${esc(t.agent_id || t.call)}" data-lead="${esc(sid)}"><i class="${deskMateDot(sid, t)}"></i><div class="dk-lt">${esc(t.name)}</div><span class="dk-age">${esc(mateSpan(t))}</span><div class="dk-ls">${esc(line)}</div></div>`;
  }).join("");
  const next = mine.find((i) => i.state === "queued");
  const nextLine = next ? `<p class="dk-note">Next for this ${lead ? "lead" : "session"}: ${esc(next.title)}${next.after ? `, after ${esc(deskItem(d.project, next.after)?.title || "")}` : ""}.</p>` : "";
  const decs = d.decisions.filter((c) => c.sid === sid || c.by === name);
  const decHtml = decs.map((c) => deskDecision(c, `${ago(deskMs(c.at))} ago${c.item_id && deskItem(d.project, c.item_id) ? ` · ${esc(deskItem(d.project, c.item_id).title)}` : ""}`)).join("");
  const latest = deskLatestOf(s);
  return `<div class="dk-pane" data-scroll-key="holder:${esc(sid)}">${deskBack(deskLeadName(d) || "Back")}<h1>${esc(name)}</h1><div class="dk-state">${esc(head)}</div>
    ${lead ? deskSec("Crew", crewRows ? `<div class="dk-crew">${crewRows}</div>` : `<p class="dk-note">No crew at work.</p>`) : ""}${nextLine}
    ${deskSec(`Decided by ${esc(name)} · ${decs.length}`, decHtml)}
    ${deskSec(`Latest from ${esc(name)}`, latest ? `<p class="dk-quiet">${esc(latest)}</p>` : "")}
    ${s ? `<div class="dk-acts"><button class="btn small" data-desk-session="${esc(sid)}">Open its session</button></div>` : ""}</div>`;
}
/** A crew member: where it is, its files, its latest, Open its session and Stop it. No box of its own: the box goes
 *  to its lead. */
function deskCrewPane(lead, agent, d) {
  const t = deskMate(lead, agent);
  const leadName = deskHolderName(lead), skey = `crew:${esc(lead)}:${esc(agent)}`;
  if (!t) return `<div class="dk-pane" data-scroll-key="${skey}">${deskBack(leadName, lead)}<div class="dk-empty">It isn't in ${esc(leadName)}'s crew any more.</div></div>`;
  const it = deskCrewItem(d.project, lead, agent);
  const files = (t.files || []).length ? `<div class="dk-files">${t.files.map(esc).join(" · ")}</div>` : "";
  const latest = [t.summary, it?.latest].filter(Boolean);
  const stop = t.state === "working" ? `<button class="btn small" data-mt-act="stop" data-mt-sid="${esc(lead)}" data-mt-agent="${esc(t.agent_id || t.call)}">Stop it</button>` : "";
  return `<div class="dk-pane" data-scroll-key="${skey}">${deskBack(leadName, lead)}<h1>${esc(t.name)}</h1>
    <div class="dk-state">In <b>${esc(leadName)}</b>'s crew · ${t.state === "working" ? "working on" : esc(t.state)} <b>${esc(it?.title || t.what || "")}</b> · ${esc(mateSpan(t))}</div>
    ${deskSec("Where it is", `<p>${esc(mateNeeds(lead, t) ? "Stopped to ask you something." : t.step || mateLine(t))}</p>`)}
    ${deskSec("Files", files)}
    ${deskSec("Latest", latest.map((l) => `<p class="dk-quiet">${esc(l)}</p>`).join(""))}
    <div class="dk-acts"><button class="btn small" data-desk-crew-open="${esc(t.agent_id || t.call)}" data-lead="${esc(lead)}">Open its session</button>${stop}</div>
    <p class="dk-note">To steer it, tell ${esc(leadName)} below; the lead passes it on. Open its session to talk to it straight.</p></div>`;
}

/** The draft key of the box that sends to `to` ({ project, item } or { sid, mate }), noted with where it sends. */
function deskBoxKey(to) {
  const key = to.sid ? `desk-to:${JSON.stringify([to.sid, to.mate || ""])}` : `desk:${JSON.stringify([to.project, to.item || ""])}`;
  deskBoxes.set(key, to);
  return key;
}
/** The box at the foot: to the Desk's lead (about the open item, named, when one is), or to an open holder (about its
 *  crew member, when one is). None without a live lead; a holder whose session has ended has none either. */
function deskBox(d) {
  const o = deskOpen;
  const lead = deskLeadName(d);
  // No lead, or one that has ended: no box to it (a holder's own box stays).
  if ((!deskLead(d) || deskLeadEnded(d)) && (!o || o.kind === "item" || o.kind === "addon")) return "";
  let to = { project: d.project }, ph = `Tell ${lead}: a bug, a feature, or a question…`;
  if (o?.kind === "item") { to = { project: d.project, item: o.id }; ph = `Talk about this item with ${lead}…`; }
  else if (o?.kind === "holder" || o?.kind === "crew") {
    const sid = o.kind === "holder" ? o.sid : o.lead;
    if (!sessionOf(sid)) return `<div class="dk-ended">Its session has ended.</div>`;
    if (o.kind === "holder") { to = { sid }; ph = `Talk to ${deskHolderName(sid, deskGroups(d.project).holders.find((h) => h.sid === sid)?.name)}…`; }
    else { const t = deskMate(o.lead, o.agent); to = { sid, mate: t?.name || "" }; ph = `Tell ${deskHolderName(sid)} about ${t?.name || "it"}…`; }
  }
  const key = deskBoxKey(to);
  return `<div class="dk-box"><textarea rows="1" data-text="${esc(key)}" placeholder="${esc(ph)}" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off">${esc(draft(key).text)}</textarea><button class="btn primary small" data-desk-send="${esc(key)}">Send</button></div>`;
}
function deskRight(project) {
  const d = deskOf(project);
  const o = deskOpen;
  let body;
  if (o?.kind === "item") {
    const i = deskItem(project, o.id);
    if (i) { delete o.fresh; body = deskItemPane(i, d); }
    // Just added: it opens once Cue's state has it (the chat shows till then); not there in a few seconds, it won't.
    else if (o.fresh && now() - o.fresh < 5000) return deskRightChat(d);
  }
  else if (o?.kind === "holder") body = deskHolderPane(o.sid, d);
  else if (o?.kind === "crew") body = deskCrewPane(o.lead, o.agent, d);
  else if (o?.kind === "addon") {
    const h = deskGroups(project).extra.find((x) => (x.key || x.name) === o.key);
    let html = "";
    try { html = (h && addOn.deskPane?.(h)) || ""; } catch (e) { console.error(e); }
    body = h ? `<div class="dk-pane" data-scroll-key="addon:${esc(o.key)}">${deskBack(deskLeadName(d) || "Back")}${html || `<h1>${esc(h.name)}</h1><div class="dk-state">${esc(h.sub || "")}</div>`}</div>` : null;
  }
  if (o && !body) deskOpen = null;
  if (deskOpen) return `<div class="dk-card dk-right">${body}${deskBox(d)}</div>`;
  return deskRightChat(d);
}
/** The right card with nothing open: who runs the Desk (no lead yet, or Change), else the lead's chat (or the line
 *  saying it ended), or Decisions. */
function deskRightChat(d) {
  if (!deskLead(d) || deskPick === d.project) return deskPicker(d);
  const tab = (t, label, n) => `<button class="dk-tab ${deskTab === t ? "on" : ""}" data-desk-tab="${t}" aria-pressed="${deskTab === t}">${label}${n ? `<span class="n">${n}</span>` : ""}</button>`;
  const main = deskTab === "decisions" ? deskDecisions(d) : deskLeadEnded(d) ? deskEndedLine(d) : deskChat(d);
  return `<div class="dk-card dk-right"><div class="dk-ch dk-tabs">${tab("lead", esc(deskLeadName(d)), 0)}${tab("decisions", "Decisions", d.decisions.length)}${deskCrewLine(d)}</div>
    ${main}${deskLeadEnded(d) ? "" : deskBox(d)}</div>`;
}

// ---------- All projects: every Desk ----------

function deskCardOf(d) {
  const waits = d.items.filter(deskWaits), done = d.items.filter(deskDoneToday), moving = d.items.filter(deskMoving).length;
  const decs = d.decisions.filter((c) => deskToday(c.at));
  const name = `<span class="dk-pn">${projDot(d.project)}${esc(projLabel(d.project))}</span>`;
  if (!waits.length && !done.length && !decs.length) {
    const open = d.items.filter((i) => i.state !== "done").length;
    return `<div class="dk-card dk-desk quiet" role="button" tabindex="0" data-desk-project="${esc(d.project)}">${name}<span class="dk-b">Quiet today${open ? ` · ${open} item${open === 1 ? "" : "s"} open` : ""}</span><span class="grow"></span><span class="dk-open">Open ›</span></div>`;
  }
  const b = [waits.length ? `<b>${waits.length} waiting on you</b>` : "", moving ? `${moving} moving` : ""].filter(Boolean).join(" · ") || "nothing waiting";
  const col = (title, cls, rows) => `<div><h3 class="${cls}">${title}</h3>${rows || `<div class="dk-none">None</div>`}</div>`;
  const waitRows = waits.map((i) => `<div class="dk-r ask">${esc(i.title)}<span class="s">${esc(i.state === "to_check" ? "ready to check" : i.ask?.text || "asks you")}</span></div>`).join("");
  const doneRows = done.map((i) => `<div class="dk-r dn">${esc(i.title)}</div>`).join("");
  const decRows = decs.map((c) => `<div class="dk-r"><b>${esc(c.by)}</b>: ${esc(c.text)}${c.undo === "can" ? `<button class="dk-ul" data-desk-undo="${esc(c.id)}">Undo</button>` : c.undo === "asked" ? `<span class="dk-u">undo asked</span>` : ""}</div>`).join("");
  return `<div class="dk-card dk-desk" role="button" tabindex="0" data-desk-project="${esc(d.project)}"><div class="dk-dh">${name}<span class="dk-b">${b}</span><span class="grow"></span><span class="dk-open">Open ›</span></div>
    <div class="dk-cols">${col("Waiting on you", waits.length ? "you" : "", waitRows)}${col(`Done today · ${done.length}`, "", doneRows)}${col(`Decided today · ${decs.length}`, "", decRows)}</div></div>`;
}
function deskAll() {
  const ds = (state.desk?.projects || []).map((x) => deskOf(x.project)).sort((a, b) => b.items.filter(deskWaits).length - a.items.filter(deskWaits).length || a.project.localeCompare(b.project));
  const body = ds.length ? ds.map(deskCardOf).join("") : `<div class="dk-card dk-desk quiet"><span class="dk-b">No Desks yet. Pick a project in the header and choose who runs its Desk.</span></div>`;
  return `<div class="board dk-board all"><div class="dk-desks" data-scroll-key="desks">${body}</div></div>`;
}

/** The Desk view: the project picked (its items, and its lead beside them), or with All projects every Desk. */
function deskView() {
  // What was open belongs to the project it was opened in.
  if (deskOpen && deskOpen.project !== projFilter) { deskOpen = null; deskChatEnd = true; }
  if (deskPick && deskPick !== projFilter) deskPick = null;
  const given = deskGiveFor && deskItem(projFilter, deskGiveFor.id);
  if (deskGiveFor && (!given || given.holder)) deskGiveFor = null;
  if (deskDropArm && !deskItem(projFilter, deskDropArm)) deskDropArm = null;
  deskBoxes.clear();   // the boxes drawn now note theirs again
  if (!projFilter) return deskAll();
  queueMicrotask(deskSettle);
  return `<div class="board dk-board">${deskList(projFilter)}${deskRight(projFilter)}</div>`;
}
/** After the draw: the lead's chat at its end, as a chat opens (unless you'd scrolled up in it). */
function deskSettle() {
  const c = document.querySelector(".dk-chat");
  if (c && deskChatEnd) c.scrollTop = c.scrollHeight;
}

// ---------- acting ----------

/** Open `what` on the right (null: back to the lead's chat). */
function deskShow(what) {
  deskOpen = what && { ...what, project: projFilter };
  deskDropArm = null;
  deskGiveFor = null;
  if (!what) deskChatEnd = true;
  renderMain();
}
/** Send what's in box `key`: to the lead (naming the open item), or to the lead (naming its crew member). Only what's
 *  sent is named: the box keeps what you typed if it doesn't go. */
async function deskSendBox(key) {
  const to = deskBoxes.get(key);
  const text = draft(key).text.trim();
  if (!to || !text) return;
  if (to.sid) {
    if (!to.mate) return sendTo(to.sid, false, key);
    if (state.restarting?.[to.sid]) return toast("It's restarting. Send once it's back.");
    if (sessionOf(to.sid)?.state === "deciding") return toast("It's asking you something first. Answer that, then send.");
    return deliver(to.sid, key, (typed, images) => {
      const said = `About ${to.mate}: ${typed}`;
      insSent(to.sid, said, false);
      return invoke("send_to_session", { sessionId: to.sid, text: said, images, now: false });
    });
  }
  const { project, item: id } = to;
  const it = id && deskItem(project, id);
  const said = it ? `About #${it.id} (${it.title}): ${text}` : text;
  delete drafts[key];
  saveDrafts();
  const list = deskSent.get(project) || [], mine = { text: said, at: now(), sent: 0 };
  list.push(mine);
  deskSent.set(project, list);
  // To the lead on its own: the chat shows it at once, Sending. From an item: the item stays till it went.
  if (!it) { deskTab = "lead"; deskChatEnd = true; }
  renderMain();
  try {
    await invoke("desk_send", { project, text: said });
    mine.sent = now();
    // Sent from an item: the chat shows it, on the lead's (unless you've opened something else meanwhile).
    if (it && deskOpen?.kind === "item" && deskOpen.id === id) { deskOpen = null; deskTab = "lead"; deskChatEnd = true; renderMain(); }
  }
  catch (e) { deskSent.set(project, (deskSent.get(project) || []).filter((p) => p !== mine)); draft(key).text = text; toast(`Not sent: ${e}`); renderMain(); }
}
/** Item `id` of the project picked, open on the right (the Inbox's Open in Desk). */
function deskOpenItem(id) {
  deskOpen = { kind: "item", id, project: projFilter, fresh: now() };
  deskDropArm = deskGiveFor = null;
}
async function deskAct(id, act) {
  // Drop takes two clicks: the first arms it.
  if (act === "drop" && deskDropArm !== id) { deskDropArm = id; return renderMain(); }
  deskDropArm = null;
  let text, key;
  if (act === "send_back") {
    key = deskBoxKey({ project: projFilter, item: id });
    text = draft(key).text.trim() || undefined;
  }
  try {
    await invoke("desk_item_act", text ? { id, act, text } : { id, act });
    // What it was sent back with is gone from the box once it went (kept when it didn't).
    if (text) { delete drafts[key]; saveDrafts(); }
    if (act === "drop") deskOpen = null;
  }
  catch (e) { toast(String(e)); }
  renderMain();
}
async function deskUndo(id) {
  try { toast((await invoke("desk_undo", { decisionId: id })) || "Asked to undo it"); }
  catch (e) { toast(String(e)); }
  renderMain();
}
/** A crew member's own session: its lead's crew page, on that member. */
function deskOpenCrew(lead, agent) {
  const t = deskMate(lead, agent);
  if (!t || !sessionOf(lead)) return;
  if (typeof addOn.openCrewPage !== "function") return openSession(lead);
  addOn.openCrewPage(lead, { from: "desk", back: "the Desk" });
  view = "desk";   // its lead becomes the active session, which picks the Board: the page shows on the Desk
  crewLeftSet(lead, { kind: "mate", key: mateKey(lead, t), tab: "", file: "" });
  renderMain();
}

/** The Desk's clicks, by attribute: each with its element. */
const DESK_CLICKS = {
  deskItem: (el) => deskShow(deskOpen?.kind === "item" && deskOpen.id === el.dataset.deskItem ? null : { kind: "item", id: el.dataset.deskItem }),
  deskHolder: (el) => deskShow({ kind: "holder", sid: el.dataset.deskHolder }),
  deskAddon: (el) => deskShow({ kind: "addon", key: el.dataset.deskAddon }),
  deskCrew: (el) => deskShow({ kind: "crew", lead: el.dataset.lead, agent: el.dataset.deskCrew }),
  deskBack: (el) => deskShow(el.dataset.deskBack ? { kind: "holder", sid: el.dataset.deskBack } : null),
  deskTab: (el) => { deskTab = el.dataset.deskTab; deskChatEnd = true; renderMain(); },
  deskDone: () => { deskDoneOpen = !deskDoneOpen; renderMain(); },
  ...DESK_LEAD_CLICKS,
  deskSend: (el) => deskSendBox(el.dataset.deskSend),
  deskAct: (el) => { deskGiveFor = null; deskAct(el.dataset.id, el.dataset.deskAct); },
  deskUndo: (el) => deskUndo(el.dataset.deskUndo),
  deskSession: (el) => openSession(el.dataset.deskSession),
  deskCrewOpen: (el) => deskOpenCrew(el.dataset.lead, el.dataset.deskCrewOpen),
  deskInbox: (el) => (typeof inboxOpen === "function" ? inboxOpen(el.dataset.deskInbox) : openSession(el.dataset.deskInbox)),
  deskProject: (el) => { setProjFilter(el.dataset.deskProject); deskOpen = null; deskTab = "lead"; deskChatEnd = true; renderMain(); },
};
/** A click rule's attribute: deskItem → data-desk-item. */
const deskAttr = (k) => `data-${k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`;
const DESK_SEL = Object.keys(DESK_CLICKS).map((k) => `[${deskAttr(k)}]`).join(", ");
/** Element `el` (one with a Desk attribute, or null) clicked or pressed: an armed Drop disarms on anything else (as
 *  Remove does in the projects list); then `el` does its thing. True when it was the Desk's. */
function deskTake(el) {
  if (deskDropArm && !(el?.dataset.deskAct === "drop" && el.dataset.id === deskDropArm)) { deskDropArm = null; if (!el) renderMain(); }
  if (!el || !el.closest(".dk-board")) return false;
  DESK_CLICKS[Object.keys(DESK_CLICKS).find((k) => k in el.dataset)](el);
  return true;
}
/** A click in the Desk: the innermost element with one of its attributes. */
function deskClick(e) {
  if (view !== "desk" || e.button > 0) return;
  if (deskTake(e.target.closest?.(DESK_SEL))) e.stopPropagation();
}
/** Keys on the Desk: ⌘N goes to the quick-add line, Enter sends a box (⇧Enter a new line), the quick-add line's keys
 *  (desk_lead.js), Esc closes a Give to… menu, then the picker (when changing), then the open pane. */
function deskKey(e) {
  if (view !== "desk" || e.isComposing || e.keyCode === 229) return false;
  const key = e.target?.dataset?.text || "";
  // A row (an item, a holder, a crew member, a project's Desk): Enter or Space opens it as a click does, and it keeps
  // the keyboard where it is.
  const row = e.target?.matches?.('.dk-board [role="button"]') && e.target.matches(DESK_SEL) ? e.target : null;
  if (row && (e.key === "Enter" || e.key === " ") && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
    const at = Object.keys(DESK_CLICKS).find((k) => k in row.dataset), sel = `[${deskAttr(at)}="${CSS.escape(row.dataset[at])}"]`;
    deskTake(row);
    document.querySelector(`.dk-board ${sel}`)?.focus();
    return true;
  }
  if (e.metaKey && !e.shiftKey && !e.altKey && !e.ctrlKey && e.key?.toLowerCase() === "n" && projFilter && !sheet && !(crewPageSid && document.querySelector(".crew-page"))) return deskQaFocus();
  if (deskQaKey(e, key)) return true;
  if (e.key === "Enter" && !e.shiftKey && deskBoxes.has(key)) { deskSendBox(key); return true; }
  // Esc: only when nothing over the Desk wants it (the crew page, a sheet, search and the live list, the projects
  // list, a menu, a rename), as the Inbox's keys do.
  const over = sheet || lightbox || liveOpen || deleteAsk || renaming || pjRenaming || pjOpen || pjNewOpen || colorFor || newProjFor || btwFor || menuFor || redirectFor || (typeof moreFor !== "undefined" && moreFor) || (crewPageSid && document.querySelector(".crew-page"));
  if (e.key === "Escape" && !e.metaKey && !e.ctrlKey && !e.altKey && !over) {
    if (deskDropArm) { deskDropArm = null; renderMain(); return true; }
    if (deskGiveFor) { deskGiveFor = null; renderMain(); return true; }
    if (deskPick && deskLead(deskOf(projFilter))) { deskPick = null; renderMain(); return true; }
    if (deskOpen && !(e.target.matches?.("textarea, input") && e.target.value)) { deskShow(deskOpen.kind === "crew" ? { kind: "holder", sid: deskOpen.lead } : null); return true; }
  }
  return false;
}

if (typeof document !== "undefined") {
  document.addEventListener("keydown", (e) => { if (deskKey(e)) { e.preventDefault(); e.stopImmediatePropagation(); } }, true);
  document.addEventListener("click", deskClick, true);
  // The lead's chat scrolled: whether it's at its end (new messages keep it there).
  document.addEventListener("scroll", (e) => { const c = e.target; if (c?.matches?.(".dk-chat")) deskChatEnd = c.scrollHeight - c.scrollTop - c.clientHeight < 24; }, true);
}
