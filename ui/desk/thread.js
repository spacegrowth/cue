// Cue UI desk — what you sent about an item that's on its way, an item not started yet, a holder's page, a crew member's page.
// Split from desk.js, which keeps the Desk's state and item basics; loaded after it (index.html, mini.html).
/** What opens session `sid`'s chat here on the Desk, as its row in the sessions on the left does: the lead's chat
 *  for the Desk's lead, else that session's chat in the middle card. Never the board. */
const deskChatAttrs = (sid, d) => (sid === deskLead(d)?.sid ? `data-desk-lead-chat="${esc(sid)}"` : `data-desk-chat-of="${esc(sid)}"`);
/** What you sent about an item that's still on its way, by item: [{ text, why, at, key, images }] (deskSendBox,
 *  deskSendBack); one that couldn't go stays, `failed` (why, "" when not known) (#422, #688). `key`: the box it went
 *  from. */
const deskPending = new Map();
const deskPend = (id, text, why, pics = 0, more = {}) => { const p = { text, why, pics, at: now(), ...more }; deskPending.set(id, [...(deskPending.get(id) || []), p]); return p; };
/** The session item `i`'s holder works in itself (a crew member's lead for a crew member), or "". */
const deskHolderSid = (i) => (i?.holder ? (i.holder.kind === "crew" ? i.holder.lead_sid : i.holder.sid) : "");
/** Item `i` not started yet (#476, #626): a status card saying so, what it waits for by title (the item before it, or
 *  the items using the working folders), what happens next, and Start now. In place of its holder's own line, which
 *  says "Waiting on you for your reply" of an idle holder when nothing waits on you. Shown for one given to a session,
 *  or queued after another; "" once it has started. */
function deskNotStarted(i) {
  if (!["queued", "new"].includes(i.state) || !(i.holder || (i.state === "queued" && i.after))) return "";
  const who = esc(i.holder?.name || "its session"), link = (id, it) => `<a href="#" data-desk-item="${esc(id)}">${it ? esc(it.title) : `#${esc(id)}`}</a>`;
  const first = i.after && deskItemAny(i.project, i.after);
  let t, sub = "", next;
  if (i.state === "queued" && i.tree_wait) {
    // One short line (#671): how many hold the working folders; their titles only once you open the fold.
    const by = (i.copy_users || []).map((x) => `<li>${link(x, deskItem(i.project, x))}</li>`), n = by.length;
    const wait = n > 1 ? `Waiting for one of ${n} items on This Mac to finish.` : n ? "Waiting for an item on This Mac to finish." : "Waiting for a free working folder on This Mac.";
    t = `${i.back_waiting ? "Sent back. " : ""}${wait}`;
    sub = n ? `<details class="dk-ns-by"><summary>${n > 1 ? "Show which" : "Show which one"}</summary><ul>${by.join("")}</ul></details>` : "";
    next = "It starts by itself when one is free.";
  } else if (i.after) {
    const on = first ? `${first.project !== i.project ? `${esc(deskAfterName(i))} · ` : ""}${link(first.id, first)}` : esc(deskAfterName(i));
    t = `It waits for ${on}.`;
    const busy = first?.state === "working" && first.holder ? `${esc(first.holder.name)} is on it now` : "";
    const files = first?.state === "working" ? deskFilesShort(first.files) : "";
    sub = [busy, first?.state === "working" && first.latest ? esc(firstLine(first.latest)) : "", files ? `has ${esc(files)} open` : ""].filter(Boolean).join(" · ");
    next = `It starts by itself when that's done${i.after_why ? `: ${esc(i.after_why)}` : ""}.`;
  } else {
    const busy = sessionOf(deskHolderSid(i))?.state === "working";
    t = busy ? `It's next for ${who}, after what it's on now.` : `It's next for ${who}.`;
    next = busy ? `${who} starts on it when that's done.` : `${who} starts on it when it's free.`;
  }
  const go = `<button class="btn small" data-desk-act="start_now" data-id="${esc(i.id)}">Start now</button>`;
  return `<div class="dk-ns" role="status"><div class="dk-ns-h">${deskStateIcon(i)}<span>Not started yet</span></div><div class="dk-ns-t">${t}</div>${sub ? `<div class="dk-ns-s">${sub}</div>` : ""}<div class="dk-ns-n">${next}</div><div class="dk-ns-a">${go}</div></div>`;
}
/** An item's quiet line under its title (#200): who has it, its commit, who added it and when; its state is the
 *  panel's, said once there. */

/** A session's latest words in a line. */
const deskLatestOf = (s) => plain(firstLine([...(s?.thread || [])].reverse().find((e) => e.role === "agent")?.text || ""));
/** What a session's page says of it now (#388): at work, the step it's on (its last reply is from before this turn);
 *  else the first line of its latest reply as its row says it (read from its log, deskSessSaid), and how long ago.
 *  { text, at, now } ({ text: "" } when there's nothing to say). */
function deskLatestNow(s) {
  if (!s) return { text: "" };
  if (deskPstate(s)[0] === "work") return { text: s.compacting_ms ? "Compacting…" : s.doing || "Working on its reply…", at: s.compacting_ms || s.doing_ms || 0, now: true };
  if (typeof deskInsAsk === "function") deskInsAsk(s.session_id);   // its log's numbers, asked for when no card brings them
  const said = deskSessSaid(s.session_id), last = [...(s.thread || [])].reverse().find((e) => e.role === "agent");
  const c = typeof insCache !== "undefined" ? insCache.get(s.session_id) : null;
  // Whichever is newer: its log's numbers are asked for once a minute, its thread moves as each turn ends.
  const fromLog = { text: said, at: c?.last_ms || 0 }, fromThread = { text: deskLatestOf(s), at: last?.at_ms || 0 };
  return said && (!fromThread.text || fromLog.at >= fromThread.at) ? fromLog : fromThread;
}
/** `deskLatestNow` as a line: its words, then how long it's been on that step ("· 3m") or when it replied ("· 2h ago"),
 *  kept current by data-ago. */
const deskLatestHtml = (l) => (l.text ? `${esc(l.text)}${l.at ? ` · <span data-ago="${l.at}">${ago(l.at)}</span>${l.now ? "" : " ago"}` : ""}` : "");
/** A crew member's dot: red when it asks you, filled at work, hollow otherwise. */
const deskMateDot = (lead, t) => (mateNeeds(lead, t) ? "ask" : t.state === "working" ? "" : "idle");
/** A lead (its crew, what's next, what it decided, its latest) or a session on its own. */
function deskHolderPane(sid, d, foot = "") {
  const s = sessionOf(sid);
  const mine = deskItemsWithRequests(d).filter((i) => deskGroupKey(i.holder) === sid && i.state !== "done");
  const name = deskHolderName(sid, mine.find((i) => i.holder?.kind !== "crew")?.holder?.name);
  const crew = deskCrew(sid), lead = deskIsLead(sid, mine);
  const waits = mine.filter(deskWaits).length;
  const head = [`${mine.length} item${mine.length === 1 ? "" : "s"}${waits ? `, ${waits} waiting on you` : ""}`, lead ? (crew.length ? `crew of ${crew.length}` : "on its own") : "", s?.since_ms ? `since ${clockAt(s.since_ms)}` : ""].filter(Boolean).join(" · ");
  const crewRows = crew.map((t) => {
    const it = deskCrewItem(d.project, sid, t.agent_id || t.call);
    const files = mateShownFiles(t).map((f) => f.split("/").pop()).join(", ");
    const line = [it ? it.title : t.what, mateNeeds(sid, t) ? "stopped to ask you" : t.step, files].filter(Boolean).join(" · ");
    return `<div class="dk-mb" role="button" tabindex="0" data-desk-crew="${esc(t.agent_id || t.call)}" data-lead="${esc(sid)}"><i class="${deskMateDot(sid, t)}"></i><div class="dk-lt">${esc(t.name)}</div><span class="dk-age">${esc(mateSpan(t))}</span><div class="dk-ls">${esc(line)}</div></div>`;
  }).join("");
  const next = mine.find((i) => i.state === "queued");
  const nextLine = next ? `<p class="dk-note">Next for this ${lead ? "lead" : "session"}: ${esc(next.title)}${next.after ? `, after ${esc(deskItem(d.project, next.after)?.title || "")}` : ""}.</p>` : "";
  const decs = d.decisions.filter((c) => c.call && (c.sid === sid || c.by === name));
  const decHtml = decs.length ? deskLog(decs, (c) => (c.item_id && deskItem(d.project, c.item_id) ? esc(deskItem(d.project, c.item_id).title) : "")) : "";
  const latest = deskLatestHtml(deskLatestNow(s));
  // One page (#200): its crew, what's next, its calls and latest at the left; in the panel, what of it waits on you
  // (Open #190), else its state, and its crew and items counted.
  const kind = deskSessKind(sid, d), wait = mine.find(deskWaits);
  const [dot, word] = deskPstate(s);
  const now = wait ? deskPnow({ dot: "ask", word: "Waiting on you", say: `${deskKindIcon(wait)}<b>${esc(deskNum(wait))}</b> ${deskVerbHtml(wait) || esc(wait.title)}`, acts: `<button class="btn small primary" data-desk-chk-item="${esc(wait.id)}">Open #${esc(wait.id)}</button>`, fold: esc(head) })
    : deskPnow({ dot, word, say: latest, fold: esc(head) });
  const by = (f) => crew.filter(f).length;
  const counts = [[by((t) => mateNeeds(sid, t)), "ask you"], [by((t) => !mateNeeds(sid, t) && t.state === "working"), "working"], [by((t) => !mateNeeds(sid, t) && t.state !== "working" && t.state !== "done"), "stopped"], [by((t) => t.state === "done"), "done"]].filter(([n]) => n).map(([n, w]) => `${n} ${w}`).join(" · ");
  const who = deskPsec("Who", deskPwho("Crew", lead ? counts || "none at work" : "") + deskPwho("Items", `${mine.length} open${waits ? ` · ${waits} ${waits === 1 ? "waits" : "wait"} on you` : ""}`));
  const main = `<div class="dk-pane" data-scroll-key="holder:${esc(sid)}">
    ${lead ? deskSec("Crew", crewRows ? `<div class="dk-crew">${crewRows}</div>` : `<p class="dk-note">No crew at work.</p>`) : ""}${nextLine}
    ${deskSec(`Decided by ${esc(name)} · ${decs.length}`, decHtml)}
    ${deskSec(`Latest from ${esc(name)}`, latest ? `<p class="dk-quiet">${latest}</p>` : "")}</div>`;
  return deskPage({ page: kind === "lead" ? "lead" : kind === "desk" ? "dispatcher" : "session", kind, back: deskBack(deskLeadName(d) || "Back"),
    acts: s ? `<button class="btn small" ${deskChatAttrs(sid, d)}>Open its chat</button>` : "", mark: `${DESK_MARKS[kind]} · `, title: esc(name), meta: esc(head) + machTag(s?.machine), now, panel: who + deskSessFiles(sid), main, foot });
}
/** Under a crew member's chat in the new layout: where it is now, as a session's chat says it (chat/render.js
 *  sessionStatus): at work, its step with a live dot; asking you, said so; else its state, its last word, and when. */
function deskCCrewStatus(t, working, asks, word) {
  const at = t.step_ms || t.started_ms, when = at ? `<span class="dim"> · <span data-ago="${at}">${ago(at)}</span>${working ? "" : " ago"}</span>` : "";
  if (asks) return `<div class="mg-status cv-status"><i class="sw z live"></i><span class="cv-what">Waiting on you</span>${when}</div>`;
  if (working) return `<div class="mg-status cv-status"><span class="dot-live"></span><span class="cv-what">${esc(t.step || "Working")}</span>${when}</div>`;
  return `<div class="mg-status cv-status"><span class="cv-what">${esc(word)}${t.step ? ` · ${esc(t.step)}` : ""}</span>${when}</div>`;
}
/** The crew member whose ⋯ menu is open on its page ("lead|agent"), so a redraw while it works keeps it open. */
let deskCrewMenu = "";
/** A crew member: its whole chat (#689), the same as the board's view of it (chat/helpers.js agentChatHtml), and
 *  under it what it changed so far, with its Diff. Stop it and ⋯ (Open its session) in the panel. No box of its own:
 *  the box goes to its lead. */
function deskCrewPane(lead, agent, d, foot = "") {
  const t = deskMate(lead, agent);
  const leadName = deskHolderName(lead), skey = `crew:${esc(lead)}:${esc(agent)}`;
  if (!t) return `<div class="dk-pane" data-scroll-key="${skey}">${deskBack(leadName, lead)}<div class="dk-empty">It isn't in ${esc(leadName)}'s crew any more.</div></div>${foot}`;
  const it = deskCrewItem(d.project, lead, agent), key = t.agent_id || t.call;
  const shown = mateShownFiles(t), working = t.state === "working";
  const stop = working ? `<button class="btn small" data-mt-act="stop" data-mt-sid="${esc(lead)}" data-mt-agent="${esc(key)}">Stop it</button>` : "";
  const asks = mateNeeds(lead, t);
  const [dot, word] = asks ? ["ask", "Waiting on you"] : working ? ["work", "Working"] : [t.state === "done" ? "done" : "stop", t.state ? t.state[0].toUpperCase() + t.state.slice(1) : "Stopped"];
  const say = asks ? "Stopped to ask you something." : t.step ? crewCode(crewStepCode(t.step)) : mateLine(t);
  const mk = `${lead}|${key}`, menuOpen = deskCrewMenu === mk;
  const more = `<span class="dk-cmore"><button class="btn small" data-desk-crew-more="${esc(mk)}" aria-expanded="${menuOpen}" aria-label="More">⋯</button>`
    + (menuOpen ? `<span class="dk-cmenu" role="menu"><button role="menuitem" data-desk-crew-open="${esc(key)}" data-lead="${esc(lead)}">Open its session</button></span>` : "") + `</span>`;
  const leadBtn = deskChatLink(lead, leadName, d);
  const who = deskPsec("Who", deskPwho("Lead", leadBtn) + deskPwho("On", it ? deskItemLink(it) : ""));
  // Its chat, as the board shows it from its crew row's Its steps (chat/helpers.js agentChatHtml): what the lead asked
  // it, its messages, its steps folded a run at a time, its answer at the end. From the lead's feed when it has it
  // (or the lead's earlier ones), else what Cue keeps of it (team_mate.js crewMateSteps).
  const h = t.agent_id ? helpersOf(lead).find((x) => x.id === t.agent_id) || bgFull(lead, t.agent_id) : null;
  const chat = agentChatHtml(lead, h, { ask: (t.asked || t.what || "").trim(), by: leadName, answer: t.summary || "", steps: h ? [] : crewMateSteps(lead, t), key });
  // Under it, what it changed so far, with its Diff.
  const diff = mateBtn(lead, t, "diff", "Diff", "link");
  const size = t.stat || (shown.length ? plural(shown.length, "file") : ""), sofar = [size, diff].filter(Boolean).join(" · ");
  const files = shown.length ? `<div class="dk-files">${shown.map(esc).join(" · ")}</div>` : "";
  // data-bg-scroll (app/render.js): it opens at its end and follows new steps there; scrolled up, it keeps your place.
  const main = `<div class="dk-pane dk-crewpg" data-scroll-key="${skey}" data-bg-scroll="${skey}">${chat}${deskCCrewStatus(t, working, asks, word)}</div>`;
  const back = deskOpen?.kind === "crew" && deskOpen.from?.kind === "sessions" ? `<button class="dk-back" data-desk-chat-back>‹ All sessions</button>` : deskBack(leadName, lead);
  // The new layout: no panel. Its name and where it stands over Chat · Files · Activity tabs, Stop it and ⋯ by its name.
  return deskCCrewPage({ lead, key, t, it, leadName, back, word, dot, acts: `${stop}${more}`, main, files: deskCrewFiles(lead, t), d, foot });
}
