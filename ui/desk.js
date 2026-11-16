// Cue UI desk — the Desk view: the project's sessions (on the Desk and not), one project's items (a bug, a feature, a task), who holds each (a lead and its crew, a
// session on its own, or the add-on's), what each waits on, the chat with the project's lead (desk_lead.js: who runs
// the Desk, Give to…, the quick-add line) and today's decisions. With All projects picked, every project's Desk as a
// card (what waits on you, what got done, what was decided).
// The data is Cue's `state.desk` (hub: items, the lead, holders by session, decisions); the commands are desk_send,
// desk_add, desk_give, desk_set_lead, desk_item_act and desk_undo.

/** What the chat card shows besides the lead's chat: null (the chat, or Decisions), or { kind: "item", id },
 *  { kind: "holder", sid } (a lead or a session), { kind: "crew", lead, agent } (a lead's crew member), or
 *  { kind: "addon", key } (a holder the add-on added), or { kind: "chat", sid } (a session's chat, from the sessions
 *  on the left); each with the `project` it was opened in (another project
 *  picked closes it). Its row in the list lights in place. */
let deskOpen = null;
/** The project whose lead row you clicked on the left (its item lit), till something else is shown. */
let deskLeadLit = null;
/** The chat card's tab while nothing is open: "lead" (its chat) or "decisions". */
let deskTab = "lead";
/** The item whose Drop was clicked once: it now reads "Drop it", and a second click drops it. */
let deskDropArm = null;
/** The item whose Send back is open: its own "Why send it back?" box in place of its buttons (deskBackHtml). */
let deskBackFor = null;
/** Where each box on the Desk sends, by its draft key: { project, item } (the lead, naming the item when there is
 *  one) or { sid, mate } (a session, naming its crew member). Kept beside the key, never read back out of it. */
const deskBoxes = new Map();
/** What you wrote to a project's lead that its chat doesn't show yet: project → [{ text, at, sent }] (`sent`: when
 *  the send returned, or 0). */
const deskSent = new Map();
/** The lead's chat was at its end before the draw (it stays there as new messages come). */
let deskChatEnd = true;

/** Seconds or ms, as the hub stamps them, in ms. */
const deskMs = (t) => (!t ? 0 : t < 1e12 ? t * 1000 : t);
/** Project `p`'s Desk ({ project, lead, items, decisions, members: its sessions' ids }), or an empty one. A dropped item is gone from it. */
const deskOf = (p) => {
  const d = (state.desk?.projects || []).find((x) => x.project === p);
  return d ? { ...d, items: d.items.filter((i) => i.state !== "dropped") } : { project: p, lead: null, items: [], decisions: [] };
};
// An item by its number, or by the tag it had before items were numbered (its alias), so old lines still find it.
const deskIdIs = (i, id) => i.id === id || (!!i.alias && i.alias === id);
const deskItem = (p, id) => deskOf(p).items.find((i) => deskIdIs(i, id)) || null;
/** An item as people name it: its kind and number, "Bug #2", "Task #1"; "#2" while it has no kind yet. */
/** An item's kind as a small line icon in the one theme color (the "Type marks" pick): a checked box, a bug, a
 *  sparkle. The one helper for it: every place an item's kind shows (the item list, the left list, the item page,
 *  the lead's route and new-item cards, Ready to check) takes it from here, styled by .dk-kic. Takes an item or a
 *  kind; "" for none. */
const DESK_KIND_SVG = {
  task: '<rect x="4" y="4" width="16" height="16" rx="4"/><path d="m8.5 12.2 2.4 2.4 4.6-5"/>',
  bug: '<rect x="8" y="7" width="8" height="12" rx="4"/><path d="M12 11v8M8 12H4.5M19.5 12H16M8 16.5l-3 1.5M16 16.5l3 1.5M8.5 9 6 7M15.5 9 18 7M10 7a2 2 0 0 1 4 0"/>',
  feature: '<path d="M12 3.5 13.9 10.1 20.5 12 13.9 13.9 12 20.5 10.1 13.9 3.5 12 10.1 10.1Z"/>',
};
const deskKindIcon = (i) => ((i = typeof i === "string" ? i : i?.kind), DESK_KIND_SVG[i] ? `<svg class="dk-kic" aria-hidden="true" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${DESK_KIND_SVG[i]}</svg>` : "");
const deskNum = (i) => `${["bug", "feature", "task"].includes(i?.kind) ? `${i.kind[0].toUpperCase()}${i.kind.slice(1)} ` : ""}#${i?.id ?? ""}`;
/** Since midnight, here. */
const deskToday = (t) => deskMs(t) >= new Date(now()).setHours(0, 0, 0, 0);
const deskWaits = (i) => i.state === "needs_you" || i.state === "to_check";
/** Items you've opened since they came back to check: id → its `updated` then (kept across restarts).
 *  SHORTCUT: kept in this window only, so another window or device still shows them as landed; move to a `seen`
 *  stamp on the item (hub/desk.rs, a desk_seen command) when the Desk is shown anywhere else. */
const deskSeen = (() => { try { return JSON.parse(localStorage.getItem("cue.deskSeen") || "{}"); } catch { return {}; } })();
/** Just landed: back to check and not opened since (it stands out till you open it). */
const deskLanded = (i) => i.state === "to_check" && deskSeen[i.id] !== i.updated;
/** Item `id` opened: it no longer stands out (only the items still to check are kept). */
function deskSeeItem(id) {
  const i = (state.desk?.projects || []).flatMap((d) => d.items).find((x) => x.id === id);
  if (!i || i.state !== "to_check" || deskSeen[id] === i.updated) return;
  deskSeen[id] = i.updated;
  const open = new Set((state.desk?.projects || []).flatMap((d) => d.items).filter((x) => x.state === "to_check").map((x) => x.id));
  for (const k of Object.keys(deskSeen)) if (!open.has(k)) delete deskSeen[k];
  try { localStorage.setItem("cue.deskSeen", JSON.stringify(deskSeen)); } catch {}
}
/** A thing an item's holder made, as a link's words: a page, a file by its name. */
function deskMadeLabel(m) {
  if (/^https?:\/\//.test(m)) {
    if (/claude\.ai\/(code\/)?artifact\//.test(m)) return "Open the page";
    try { return new URL(m).host.replace(/^www\./, ""); } catch { return "Open the link"; }
  }
  return m.split("/").pop();
}
/** Links to what an item's holder made (its pages and files, from the turn that said DONE), and its changes when it
 *  has files: in its pane. */
function deskMadeLinks(i) {
  const links = (i.made || []).map((m) => `<button class="dk-made" data-desk-made="${esc(m)}">${esc(deskMadeLabel(m))} ↗</button>`);
  return links.length ? `<div class="dk-mades">${links.join("")}</div>` : "";
}
/** A to-check or done item's Files changed, as Looks right commits them (or committed them): item id → { key, files }
 *  (files null while it's read). `key` is what it was read for, so a new check or commit reads it again; `page` the
 *  page it was read for. */
const deskDiffs = new Map();
const deskDiffKey = (i) => [i.state, i.commit || "", i.updated || "", (i.files || []).length, (i.commit_wait?.files || []).length].join("|");
function deskFilesChanged(i) {
  if (i.state !== "to_check" && i.state !== "done") return "";
  // Read again each time its page is opened too: its files can change on disk with nothing else about it changing.
  const key = deskDiffKey(i), got = deskDiffs.get(i.id);
  if (got?.key !== key || got.page !== deskOpen) {
    deskDiffs.set(i.id, { key, page: deskOpen, files: got?.files || null });
    const keep = (files) => { if (deskDiffs.get(i.id)?.key !== key) return; deskDiffs.set(i.id, { ...deskDiffs.get(i.id), files }); if (deskOpen?.id === i.id) renderMain(); };
    invoke("desk_item_diff", { id: i.id }).then((r) => keep(diffParse(r?.diff || "").files), () => keep([]));
  }
  const files = deskDiffs.get(i.id).files || [];
  return files.map((f, n) => `<button class="dk-fc" data-desk-diff-file="${esc(i.id)}" data-n="${n}"><span class="dk-fc-p">${esc(f.path)}</span>${f.isNew ? `<span class="dk-fc-new">new</span>` : ""}<span class="dv-n dv-add">+${f.add}</span><span class="dv-n dv-del">−${f.del}</span></button>`).join("");
}
/** Item `id`'s file `n` in Cue's Diff view: its holder's session (a crew member's: its lead's), or the Desk lead's
 *  once that has ended, on the item's changes, that file open. */
function deskShowFile(id, n) {
  const i = (state.desk?.projects || []).flatMap((d) => d.items).find((x) => x.id === id), h = i?.holder;
  const sid = [h?.kind === "crew" ? h.lead_sid : h?.sid, deskLead(deskOf(i?.project))?.sid].find((x) => x && sessionOf(x));
  if (!sid) return toast("No session to show it in");
  deskSeeItem(id);
  openSession(sid);
  diffShowItem(sid, id, deskNum(i), n);
}
const deskDoneToday = (i) => i.state === "done" && deskToday(i.done_at || i.updated);
const deskMoving = (i) => i.state === "working" || i.state === "queued";
/** A title cut short for a button or a line ("Search stays centred"): its first few words. */
const deskShort = (t) => { const w = String(t || "").split(/\s+/); return w.length > 3 ? w.slice(0, 3).join(" ") : t; };
const deskCap = (s) => String(s || "").replace(/^./, (c) => c.toUpperCase());

// ---------- holders: a lead (its crew from its team), a session on its own, or the add-on's ----------

/** The lead's teammates still at it or waiting (team.js mateList), or none for a session that isn't one. */
const deskCrew = (sid) => (typeof mateList === "function" ? mateList(sid) : []);
/** A crew member at work (or its lead reviewing it). */
const deskMateBusy = (t) => t.state === "working" || t.state === "reviewing";
/** A crew member that gets its own row on the Desk: at work, asking you, or with uncommitted files (`uncommitted`,
 *  from git: hub/team_tidy.rs). */
const deskMateLive = (sid, t) => t.state !== "committed" && t.state !== "aside" && (deskMateBusy(t) || mateNeeds(sid, t) || !!t.uncommitted);
/** The leads whose "N done" fold you opened on the Desk: kept across draws and restarts (this window's storage). */
const deskFoldOpen = (() => { try { return new Set(JSON.parse(localStorage.getItem("cue.deskFold") || "[]")); } catch { return new Set(); } })();
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

/** An item's state as its row's tag: [words, look]. Landed (back to check, not opened since) stands out. */
function deskTag(i) {
  if (i.state === "needs_you") return ["asks", "ask"];
  if (i.state === "to_check") return deskLanded(i) ? ["landed", "landed"] : ["check", "check"];
  if ((i.state === "new" || i.state === "queued") && i.route_failed) return ["stuck", "ask"];
  return { queued: ["next", ""], new: ["new", ""], done: ["done", "done"], working: ["now", "now"] }[i.state] || [i.state, ""];
}
/** Who holds an item, in a few words: "web-lead → search-centre" for its crew member. */
const deskHolderOf = (i) => (!i.holder ? "" : i.holder.kind === "crew" ? `${deskHolderName(i.holder.lead_sid)} → ${i.holder.name}` : i.crew ? `${i.holder.name} → ${i.crew}` : i.holder.name);
/** One item in two rows: its state, #id and title; under them who holds it (Give to… when no one does), its age (a
 *  done one's time it finished), and a quiet ✓ when it's ready to check (Looks right; it fills on hover); an ask's question below that. Lit
 *  when it's open, or its crew member is. */
function deskItemRow(i, lit, held = false) {
  const on = lit || (deskOpen?.kind === "item" && deskOpen.id === i.id);
  const [tag, look] = deskTag(i);
  // Lit by its session (clicked on the left) and at work: "working · host-lead · 4m", how long its session has been at it.
  const s = held && i.state === "working" ? sessionOf(i.holder?.sid) : null;
  const at = s ? (s.state === "working" && s.since_ms) || deskMs(i.updated) : 0;
  const who = s ? `working · ${esc(deskHolderOf(i))} · <span data-ago="${at}">${ago(at)}</span>` : !i.holder && i.state !== "done" ? deskGive(i, "row") : esc(deskHolderOf(i));
  const when = s ? "" : i.state === "done" ? `${i.commit ? `<span class="dk-cm">committed ${esc(i.commit)}</span> · ` : ""}${clockAt(deskMs(i.done_at || i.updated))}` : ago(deskMs(i.created));
  const ok = i.state === "to_check" && deskLooksSoon.has(i.id) ? deskLooksSoonHtml(i.id) : i.state === "to_check" ? `<button class="dk-ok" data-desk-act="looks_right" data-id="${esc(i.id)}" aria-label="Looks right"></button>` : "";
  const ask = i.state === "needs_you" ? `<div class="dk-h">${esc(i.ask?.text || `${i.holder?.name || "Its holder"} asks you something`)}</div>` : "";
  return `<div class="dk-it ${i.state === "needs_you" ? "ask" : ""} ${look === "landed" ? "landed" : ""} ${on ? "on" : ""}" role="button" tabindex="0" data-desk-item="${esc(i.id)}">
    <span class="dk-tag ${look}">${tag}</span><div class="dk-i1">${deskKindIcon(i)}<span class="dk-id">${esc(deskNum(i))}</span><span class="dk-t">${esc(i.title)}</span></div>
    <div class="dk-i2"><span class="dk-who${s ? " held" : ""}">${who}</span><span class="dk-age">${when}</span>${ok}</div>${ask}</div>`;
}
/** A holder's head: its name, what it is, › (it opens). */
const deskHolderHead = (h, on) => `<div class="dk-holder ${on ? "on" : ""}" role="button" tabindex="0" ${h.addon ? `data-desk-addon="${esc(h.key)}"` : `data-desk-holder="${esc(h.sid)}"`}><span class="dk-nm">${esc(h.name)}</span><span class="dk-s">${esc(h.sub || "")}</span><span class="grow"></span><span class="dk-go">›</span></div>`;
/** The item a crew member is on (lit with it while its pane is open). */
const deskCrewItem = (project, lead, agent) => { const t = deskMate(lead, agent); return deskOf(project).items.find((i) => i.state !== "done" && ((i.holder?.kind === "crew" && (i.holder.sid === agent || (t && i.holder.name === t.name))) || (t && i.crew && i.holder?.sid === lead && i.crew.toLowerCase() === (t.name || "").toLowerCase()))) || null; };

/** Finished items the list loaded from Cue (desk_done) as you scrolled, a project each: { items, end (none older),
 *  loading }; and how many finished items each project's list shows (10, then 10 more each time its end comes into
 *  view). Today's come with the Desk's state; older ones only from here. */
const deskDonePages = new Map(), deskDoneShown = new Map();
const DESK_DONE_STEP = 10;
const deskDoneKey = (i) => [deskMs(i.updated), i.id];
/** A project's finished items, newest first: today's as the Desk's state has them, then those loaded; one that's open
 *  again (in the state, not done) left out. */
function deskDoneList(d) {
  const byId = new Map();
  for (const i of deskDonePages.get(d.project)?.items || []) byId.set(i.id, i);
  for (const i of d.items) { if (i.state === "done") byId.set(i.id, i); else byId.delete(i.id); }
  return [...byId.values()].sort((a, b) => deskMs(b.updated) - deskMs(a.updated) || (a.id < b.id ? 1 : a.id > b.id ? -1 : 0));
}
/** The next finished items older than the list's last, a step ahead of what shows so the next ones are there as you
 *  reach them. */
async function deskDoneFetch(project) {
  const pg = deskDonePages.get(project) || { items: [], end: false, loading: false };
  deskDonePages.set(project, pg);
  if (pg.end || pg.loading) return;
  pg.loading = true;
  const last = pg.items.at(-1);
  try {
    const got = await invoke("desk_done", { project, beforeMs: last ? deskMs(last.updated) : null, beforeId: last?.id ?? null, take: DESK_DONE_STEP * 2 });
    pg.items.push(...got);
    pg.end = got.length < DESK_DONE_STEP * 2;
  } catch (e) { console.error(e); pg.end = true; }
  pg.loading = false;
  if (projFilter === project && view === "desk") renderMain();
}
/** The list's end came into view: 10 more finished items, and the next ones fetched ahead. */
function deskDoneMore(project) {
  const pg = deskDonePages.get(project);
  const all = deskDoneList(deskOf(project)).length, n = deskDoneShown.get(project) || DESK_DONE_STEP;
  if (all > n) { deskDoneShown.set(project, n + DESK_DONE_STEP); renderMain(); }
  if (!pg?.end && all < n + DESK_DONE_STEP * 2) deskDoneFetch(project);
}
let deskDoneSeen = null;
/** Watches the list's end (after each draw): in view, 10 more. */
function deskDoneWatch() {
  deskDoneSeen?.disconnect();
  const end = document.querySelector(".dk-items [data-desk-done-more]"), box = end?.closest(".dk-scroll");
  if (!end || typeof IntersectionObserver === "undefined") return;
  const project = end.dataset.deskDoneMore;
  deskDoneSeen = new IntersectionObserver((es) => { if (es.some((e) => e.isIntersecting)) { deskDoneSeen.disconnect(); deskDoneMore(project); } }, { root: box, rootMargin: "0px 0px 240px 0px" });
  deskDoneSeen.observe(end);
}

function deskList(project) {
  const g = deskGroups(project);
  const o = deskOpen;
  const crewLit = o?.kind === "crew" ? deskCrewItem(project, o.lead, o.agent)?.id : null;
  // The session clicked on the left lights the item it holds, where it is (nothing moves).
  const sessSid = o?.kind === "chat" || o?.kind === "holder" ? o.sid : !o && deskLeadLit === project ? deskLead(g.d)?.sid : null;
  const held = sessSid ? deskSessItem(g.d, sessSid)?.id : null;
  const row = (i) => deskItemRow(i, i.id === crewLit || i.id === held, i.id === held);
  const headOn = (sid) => (o?.kind === "holder" && o.sid === sid) || (o?.kind === "crew" && o.lead === sid);
  const parts = [];
  if (g.waiting.length) parts.push(`<div class="dk-gh you">Waiting on you <span class="n">${g.waiting.length}</span></div>${g.waiting.map(row).join("")}`);
  if (g.fresh.length) parts.push(`<div class="dk-gh">Not given out yet <span class="n">${g.fresh.length}</span></div>${g.fresh.map(row).join("")}`);
  for (const h of g.holders) parts.push(deskHolderHead(h, headOn(h.sid)) + h.items.map(row).join(""));
  for (const h of g.extra) parts.push(deskHolderHead({ ...h, addon: true, key: h.key || h.name }, o?.kind === "addon" && o.key === (h.key || h.name)) + (h.items || []).map(row).join(""));
  // Finished: the 10 newest, then 10 more each time the list's end comes into view (deskDoneWatch). Its end holds
  // its place while the next ones load, so nothing jumps.
  const pg = deskDonePages.get(project);
  if (!pg) queueMicrotask(() => deskDoneFetch(project));
  const done = deskDoneList(g.d), shown = done.slice(0, deskDoneShown.get(project) || DESK_DONE_STEP);
  const more = done.length > shown.length || !pg?.end;
  if (shown.length) parts.push(`<div class="dk-gh">Done</div>${shown.map(row).join("")}`);
  if (more && (shown.length || pg?.loading || !pg)) parts.push(`<div class="dk-more" data-desk-done-more="${esc(project)}">${shown.length ? "Loading older…" : ""}</div>`);
  const body = parts.join("") || `<div class="dk-empty">No items yet.</div>`;
  return `<div class="dk-card dk-items"><div class="dk-ch"><b>Items</b></div>
    ${deskQuickAdd()}<div class="dk-scroll" data-scroll-key="desk-items:${esc(project)}">${body}</div></div>`;
}

// ---------- the sessions: the project's sessions, on the Desk and not ----------

/** A session's state, for its row: [dot, words]. Waiting on you when it asks you something. */
function deskSessState(s) {
  const asks = s.state === "deciding" || state.items.some((i) => i.session_id === s.session_id && i.status === "pending" && i.kind !== "waiting");
  return asks ? ["ask", "waiting on you"] : s.state === "working" ? ["", "working"] : ["idle", "idle"];
}
/** The item a session holds (an ask before what's at work), or null. */
const deskSessItem = (d, sid) => d.items.filter((i) => i.state !== "done" && i.holder && i.holder.kind !== "crew" && i.holder.sid === sid)
  .sort((a, b) => (b.state === "needs_you") - (a.state === "needs_you") || (b.state === "working") - (a.state === "working"))[0] || null;
/** A session's context and cache, as the lead's summary says them (hub/desk_lead.rs context_note): "142k · cache 12m",
 *  or "142k · cold"; amber with under 5 minutes left. From its card's numbers (insights.js insCache); "" without. */
function deskSessCache(sid) {
  const c = typeof insCache !== "undefined" ? insCache.get(sid) : null;
  if (!c?.tokens) return "";
  const left = (c.until_ms || 0) - now();
  const cache = left > 0 ? ` · cache ${Math.ceil(left / 60000)}m` : c.until_ms ? " · cold" : "";
  return `<div class="dk-s3 dk-size${left > 0 && left < HOT_CACHE_MS ? " hot" : ""}">${esc(insTok(c.tokens))}${cache}</div>`;
}
/** One row: name, LEAD for a lead (crew show by their indent), its state; then the item it's on, or nothing on the Desk; then its
 *  size and cache. */
/** A working session's (or teammate's) current step and how long it's been at it, kept live (window.js ticks
 *  data-ago): "Editing ui/desk.js · 4m". "" when it isn't working or says no step. */
const deskStepLine = (working, step, at) => (working && step ? `<div class="dk-s3 dk-now">${esc(step)}${at ? ` · <span data-ago="${at}">${ago(at)}</span>` : ""}</div>` : "");
/** A session's name with ✎ beside it, shown on hover, that renames the session (as the Active pane's header does:
 *  data-desk-ren → renameSession, Enter keeps it, Esc or leaving the box doesn't); the box itself while you do. */
const DESK_PEN = `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 20h4L19 9l-4-4L4 16z"/><path d="M14 6l4 4"/></svg>`;
function deskSessName(sid, name, cls) {
  if (renaming === sid) return `<input class="rename-in dk-ren-in" data-text="rename:${esc(sid)}" maxlength="60" placeholder="Name this session" aria-label="Rename this session" spellcheck="false" value="${esc(draft(`rename:${sid}`).text)}"/>`;
  return `<${cls === "b" ? "b" : `span class="${cls}"`}>${esc(name)}</${cls === "b" ? "b" : "span"}>${sessionOf(sid) ? `<button class="dk-ren dk-sren" data-desk-ren="${esc(sid)}" aria-label="Rename this session">${DESK_PEN}</button>` : ""}`;
}
function deskSessRow({ name, sid = "", mark = "", dot, word, item, line, on, attrs, crew, step = "", size = "", loose = false, past = false, add = "" }) {
  const what = `<span class="dk-w">${item ? `${deskKindIcon(item)}<span class="dk-id">${esc(deskNum(item))}</span> ${esc(item.title)}` : line ? esc(line) : `<span class="dk-none">nothing on the Desk</span>`}</span>`;
  return `<div class="dk-srow ${crew ? "crew" : ""} ${past ? "past" : ""} ${on ? "on" : ""}" role="button" tabindex="0" ${attrs}>${mark ? `<span class="dk-rmark">${mark}</span>` : ""}
    <div class="dk-s1">${sid ? deskSessName(sid, name, "dk-nm") : `<span class="dk-nm">${esc(name)}</span>`}<span class="dk-st"><i class="${dot}"></i><b>${word}</b></span></div>
    <div class="dk-s2">${what}${add}${loose ? `<span class="dk-pill loose">not committed</span>` : ""}</div>${step}${size}</div>`;
}
/** The left card: the Desk's lead on its own at the top (no crew), then the project's other sessions, "Working"
 *  first, then "On the Desk" (the Desk's own list: the ones its lead routes to, each with its crew under it) and
 *  "Not on the Desk" (each with Add to Desk); the one whose chat shows is lit; + New session at its foot. */
function deskSessions(project) {
  const d = deskOf(project), o = deskOpen, leadSid = deskLead(d)?.sid;
  const sessions = (state.sessions || []).filter((s) => s.project === project && s.session_id);
  const onDesk = (sid) => sid === leadSid || (d.members || []).includes(sid);
  const lit = (sid) => (sid === leadSid && !o && deskPick !== project) || ((o?.kind === "chat" || o?.kind === "holder") && o.sid === sid);
  const row = (s) => {
    const sid = s.session_id, [dot, word] = deskSessState(s);
    const lead = sid === leadSid || deskIsLead(sid, d.items.filter((i) => deskGroupKey(i.holder) === sid));
    const attrs = sid === leadSid ? `data-desk-lead-chat="${esc(sid)}"` : `data-desk-chat-of="${esc(sid)}"`;
    const mate = (t, past) => {
      const key = t.agent_id || t.call, it = past ? null : deskCrewItem(project, sid, key);
      const [cd, cw] = past ? ["idle", t.state === "aside" ? "set aside" : t.state] : mateNeeds(sid, t) ? ["ask", "waiting on you"] : deskMateBusy(t) ? ["", t.state] : ["idle", t.state];
      return deskSessRow({ name: t.name, mark: "Crew", dot: cd, word: cw, item: it, line: it ? "" : t.what, crew: true, past, loose: !past && !deskMateBusy(t) && !!t.uncommitted,
        step: past ? "" : deskStepLine(cd === "", t.step, t.step_ms || t.started_ms),
        on: o?.kind === "crew" && o.lead === sid && o.agent === key, attrs: `data-desk-crew="${esc(key)}" data-lead="${esc(sid)}"` });
    };
    // Rows only for crew still in play: at work, asking you, or with files git says aren't committed. The rest
    // (committed, set aside, or finished with nothing left to commit) fold into one "N done" line, opened in place and
    // kept the way you left it (deskFoldOpen).
    const team = sessionOf(sid)?.team || [];
    const rank = (t) => (deskMateBusy(t) ? 0 : mateNeeds(sid, t) ? 1 : 2);
    const live = team.filter((t) => deskMateLive(sid, t)).sort((a, b) => rank(a) - rank(b)), done = team.filter((t) => !deskMateLive(sid, t)), open = deskFoldOpen.has(sid);
    const fold = done.length ? `<button class="dk-sfold" data-desk-done-fold="${esc(sid)}" aria-expanded="${open}">${done.length} done · ${open ? "hide" : "show"}</button>${open ? done.map((t) => mate(t, true)).join("") : ""}` : "";
    // The Desk's own lead shows no crew (it routes; any teammates it had, even old ones, aren't the Desk's).
    const crew = sid === leadSid ? "" : live.map((t) => mate(t, false)).join("") + fold;
    const add = onDesk(sid) ? "" : `<button class="btn small dk-sadd" data-desk-member="${esc(sid)}" data-project="${esc(project)}">Add to Desk</button>`;
    return deskSessRow({ name: deskHolderName(sid), sid, mark: sid === leadSid ? "Desk lead" : lead ? "Lead" : "Session", dot, word, item: deskSessItem(d, sid), on: lit(sid), attrs, step: deskStepLine(dot === "", s.compacting_ms ? "Compacting…" : s.doing, s.compacting_ms || s.doing_ms), size: deskSessCache(sid), add }) + crew;
  };
  // The Desk's lead in a spot of its own at the top, never in the list. Then working ones first (a session moves only
  // when its state does, never on a click), then On the Desk, then Not on the Desk, each as Cue lists them. Crew stay
  // under their lead, wherever it is.
  const leadS = sessions.find((s) => s.session_id === leadSid), rest = sessions.filter((s) => s !== leadS);
  // At work: the session itself, its background agents (state "agent" with helpers running), or any of its crew. A
  // session idle while its crew works is listed with the working ones, so their rows never sit under idle sessions.
  const busy = (s) => deskSessState(s)[1] === "working" || (s.state === "agent" && !!s.helpers)
    || (sessionOf(s.session_id)?.team || []).some(deskMateBusy);
  const atWork = rest.filter(busy);
  const on = rest.filter((s) => !busy(s) && onDesk(s.session_id));
  const off = rest.filter((s) => !busy(s) && !onDesk(s.session_id));
  const working = atWork.length;
  const group = (title, list) => (list.length ? `<div class="dk-gh">${title} <span class="n">${list.length}</span></div>${list.map(row).join("")}` : "");
  const top = leadS ? `<div class="dk-leadspot"><div class="dk-gh">Runs this Desk<span class="grow"></span><button class="dk-more-btn" data-desk-change aria-label="Hand this Desk to another session"><svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><circle cx="5" cy="12" r="1.8"/><circle cx="12" cy="12" r="1.8"/><circle cx="19" cy="12" r="1.8"/></svg></button></div>${row(leadS)}</div>` : "";
  const body = group("Working", atWork) + group("On the Desk", on) + group("Not on the Desk", off) || `<div class="dk-empty">No ${leadS ? "other " : ""}sessions in this project yet.</div>`;
  return `<div class="dk-card dk-sessions">${deskSessHead(project, rest.length, working)}
    ${top}<div class="dk-scroll" data-scroll-key="desk-sessions:${esc(project)}">${body}</div>
    <div class="dk-sfoot"><button class="btn small" data-desk-new-session="${esc(project)}">+ New session</button></div></div>`;
}
/** The Sessions card's head: ‹ back to every Desk, this Desk's name (✎ on hover renames it, as the projects list
 *  does), then how many sessions and how many work. */
function deskSessHead(project, n, working) {
  const name = pjRenaming === project
    ? `<input class="pj-name-in" data-text="pjname:${esc(project)}" maxlength="40" placeholder="${esc(project)}" aria-label="Rename this Desk" spellcheck="false" value="${esc(draft(`pjname:${project}`).text)}"/>`
    : `<b>${esc(projLabel(project))}</b><button class="dk-ren" data-pjren="${esc(project)}" aria-label="Rename this Desk"><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 20h4L19 9l-4-4L4 16z"/><path d="M14 6l4 4"/></svg></button>`;
  return `<div class="dk-ch dk-sh"><button class="dk-home-btn" data-desk-home aria-label="All Desks">‹</button><div class="dk-shn"><div class="dk-shr">${name}</div><span class="dk-sub">${n} session${n === 1 ? "" : "s"}${working ? ` · ${working} working` : ""}</span></div></div>`;
}
/** + New, its folder that of a session in the project (else what + New would pick). */
/** Session `sid` on `project`'s Desk (`on`) or off it: the sessions its lead routes to without asking you. */
async function deskMember(project, sid, on) {
  try { await invoke("desk_member", { project, sid, on }); }
  catch (e) { toast(String(e)); }
}
/** ⋯ → Add to Desk / Remove from Desk, for a session of a project (not the Desk's lead, which is always on it). */
function deskMemberItem(sid, s) {
  const project = s?.project;
  if (!project || !sid) return "";
  const d = deskOf(project);
  if (deskLead(d)?.sid === sid) return "";
  const on = (d.members || []).includes(sid);
  return on ? `<button data-desk-member-set="${esc(sid)}" data-project="${esc(project)}" data-on="0">Remove from Desk<span>Its Desk's lead asks you before giving it work again</span></button>`
    : `<button data-desk-member-set="${esc(sid)}" data-project="${esc(project)}" data-on="1">Add to Desk<span>Its Desk's lead can give it work without asking you</span></button>`;
}
function deskNewSession(project) {
  const s = (state.sessions || []).find((x) => x.project === project && x.cwd && !x.machine);
  if (typeof lvAct === "function") lvAct("new", { spot: true, cwd: s?.cwd || "" });
}

// ---------- the chat card: the lead's chat, Decisions, or what's open ----------

/** Decisions or steps as one quiet table: when | what, with who and why muted under it | Undo (shown on hover). The
 *  same line said again in a row is one row marked "×N" (its latest time and Undo). `about`: more for the muted line
 *  (the item a lead's decision was on). */
function deskLog(list, about = null) {
  const rows = [];
  for (const c of list) {
    const last = rows[rows.length - 1];
    if (last && last.c.text === c.text && (last.c.why || "") === (c.why || "") && last.c.call === c.call) { last.n++; if (deskMs(c.at) >= deskMs(last.c.at)) last.c = c; }
    else rows.push({ c, n: 1 });
  }
  return `<div class="dk-log">${rows.map(({ c, n }) => {
    const on = deskLogOpen.has(c.id), what = about?.(c) || "";
    return `<div class="dk-lr ${c.call ? "dk-dec" : "dk-step"}${on ? " open" : ""}" data-desk-log-row="${esc(c.id)}"><span class="dk-lby">${esc(c.by || "")}</span>`
      + `<span class="dk-d">${esc(c.text)}${n > 1 ? ` <span class="dk-x">×${n}</span>` : ""}${what ? ` <span class="dk-la">· ${what}</span>` : ""}</span>`
      + `<span class="dk-w">${c.why ? esc(c.why) : ""}</span><span class="dk-age">${ago(deskMs(c.at))} ago</span><span class="dk-lu">${deskUndoLink(c)}</span></div>`;
  }).join("")}</div>`;
}
/** Rows of a Decisions or Steps table shown in full (a click on a row; long ones are cut at two lines). */
const deskLogOpen = new Set();
const DESK_UNDO_ICON = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/></svg>`;
/** Undo at a row's end: a small quiet icon shown while the pointer is on the row; "asked" once it was. */
const deskUndoLink = (c) => (c.undo === "can" ? `<button class="dk-undo-l" data-desk-undo="${esc(c.id)}" aria-label="Undo">${DESK_UNDO_ICON}</button>`
  : c.undo === "asked" ? `<span class="dk-u">Undo asked</span>` : "");
/** What you wrote to the lead, without the summary of the Desk (and, the first time, its instructions) Cue puts before
 *  it: a block of exactly that shape (`<desk project="…" …>`, its lines, `</desk>`, a blank line), so words of yours
 *  that start with <desk> stay, even typed in that shape without the project. */
const deskSaid = (t) => String(t ?? "").replace(/^<desk project="[^"]*"[^>\n]*>\n[\s\S]*?\n<\/desk>\n\n/, "");
/** Your words about an item ("About #69 (its title): …", as deskSendBox sends them): a head over them with its kind's
 *  icon, its kind and number and its title (a click opens it), and your words under it. null for any other words. */
function deskAboutHead(e) {
  const m = /^About #([^\s(]+) \(/.exec(e?.text || "");
  if (!m) return null;
  const it = (state.desk?.projects || []).flatMap((d) => d.items).find((i) => deskIdIs(i, m[1]));
  // The title as sent; when the item is gone or renamed since, up to the first "): ".
  const rest = e.text.slice(m[0].length), title = it && rest.startsWith(`${it.title}): `) ? it.title : rest.split("): ")[0];
  if (rest.length === title.length) return null;
  const html = `<button class="dk-about" data-desk-chk-item="${esc(it?.id ?? m[1])}">${deskKindIcon(it)}<b>${esc(it ? deskNum(it) : `#${m[1]}`)}</b> · ${esc(title)}</button>`;
  return { html, text: rest.slice(title.length + 3) };
}
/** Your words as the lead's chat shows them (deskSaid). */
const deskSaidE = (e) => (e.role === "you" ? { ...e, text: deskSaid(e.text) } : e);
/** A chat the Desk shows: its earlier messages from Cue's log too (its thread keeps only the last few, all a restart
 *  starts with), loaded as it first shows, and Earlier messages above them, as in a session's chat (chat/thread.js). */
function deskChatRows(s, page) {
  const sid = s.session_id;
  if (!older.has(sid)) queueMicrotask(() => loadOlder(sid));
  const any = (s.thread || []).length || older.get(sid)?.msgs.length;
  return any ? chatHtml(null, s, s.harness || "claude", true, { lines: deskLineHtml, you: deskCueNote, youHead: deskAboutHead, noActs: true, ...page }) : "";
}
/** For ↑ ↓ in a Desk's box (chat/compose.js recallSent): the session it goes to ({ s, said }: your words as its chat
 *  shows them), the lead's for the lead's box; null for a box of no session's. */
function deskRecall(key) {
  const to = deskBoxes.get(key);
  if (!to) return null;
  if (to.sid) { const s = sessionOf(to.sid); return s ? { s } : null; }
  const d = deskOf(to.project), s = deskLead(d) && !deskLeadEnded(d) ? deskLeadSession(d) : null;
  return s ? { s, said: deskSaidE } : null;
}
/** The session whose chat the Desk shows, when it's `sid`: its lead's (a parked one's kept messages too), or the one
 *  opened from the sessions on the left. Null otherwise. */
function deskShownChat(sid) {
  if (!projFilter) return null;
  if (deskOpen?.kind === "chat") return deskOpen.project === projFilter && deskOpen.sid === sid ? sessionOf(sid) : null;
  if (deskOpen || deskTab !== "lead" || deskPick === projFilter) return null;
  const d = deskOf(projFilter);
  return deskLead(d)?.sid === sid && !deskLeadEnded(d) ? deskLeadSession(d) : null;
}
/** The lead's chat: its session's messages, drawn as any session's chat is (chat/render.js), your words without the
 *  Desk's summary; then what you sent that it doesn't show yet (given up a minute after the send returned). */
function deskChat(d) {
  const s = deskLeadSession(d);
  const thread = s?.thread || [];
  const pending = (deskSent.get(d.project) || []).filter((p) => !(p.sent && now() - p.sent > 60000)
    && !thread.some((e) => e.role === "you" && e.at_ms >= p.at - 60000 && e.text.includes(p.text)));
  deskSent.set(d.project, pending);
  // The lead's own Desk lines drawn as lines, not raw text; no Actions row under its reply (that's a session chat's).
  const rows = s ? deskChatRows(s, { said: deskSaidE, extra: deskCheckRows(d) }) : "";
  const mine = pending.map((p) => { const hd = deskAboutHead(p); return `<div class="cv-you sending">${p.text ? `<div class="cv-you-text">${hd?.html || ""}${esc(hd ? hd.text : p.text).replace(/\n/g, "<br>")}</div>` : ""}${p.images?.length ? `<div class="thumbs">${p.images.map((im) => `<span class="thumb"><img src="${esc(im.data)}" alt=""/></span>`).join("")}</div>` : ""}<div class="cv-meta">Sending</div></div>`; }).join("");
  // Where the lead is now (working on what, its latest step, stuck, compacting), as under any session's chat.
  const where = s && sessionOf(s.session_id) ? whereNow(null, s) : "";
  const empty = !rows && !mine && !where ? `<div class="dk-empty">Tell ${esc(deskLeadName(d))} about a bug, a feature or a task. It adds them here, does each or hands it out, and says why.</div>` : "";
  return `<div class="dk-chat" data-desk-chat="${esc(d.project)}">${empty}${rows}${mine}${where}</div>`;
}
/** A session's chat, opened from the sessions on the left: its name, Open its session, its messages and where it is
 *  now, as under any session's chat. */
/** The mark at the top right of every chat on the Desk (Desk picks, mark A): one block in the theme's color with what
 *  the chat is in it. `kind`: "desk" (the Desk's lead), "session", "crew", "lead", "task", "bug" or "feature"; "" for
 *  none. `float`: over a pane with no header bar (an item's, a holder's, a crew member's), at the card's top right. */
const DESK_MARKS = { desk: "Desk lead", session: "Session", crew: "Crew", lead: "Lead", task: "Task", bug: "Bug", feature: "Feature" };
const deskMark = (kind, float = false) => (DESK_MARKS[kind] ? `<span class="dk-mark${float ? " float" : ""}" data-desk-mark="${kind}">${DESK_MARKS[kind]}</span>` : "");
/** What session `s`'s chat is: a lead (it has a crew), else the kind of the item it holds, else a session. */
function deskSessKind(s) {
  if (deskCrew(s.session_id).length) return "lead";
  return deskSessItem(deskOf(s.project), s.session_id)?.kind || "session";
}
/** The mark over what's open in the chat card: an item's kind, a holder's, a crew member's. */
function deskPaneMark(o, d) {
  if (o?.kind === "item") return deskMark(deskItem(d.project, o.id)?.kind, true);
  if (o?.kind === "crew") return deskMark("crew", true);
  if (o?.kind === "holder") { const s = sessionOf(o.sid); return deskMark(s ? deskSessKind(s) : "lead", true); }
  return "";
}
function deskSessChat(sid) {
  const s = sessionOf(sid);
  if (!s) return null;
  const rows = deskChatRows(s);
  const where = whereNow(null, s);
  const empty = !rows && !where ? `<div class="dk-empty">Nothing said yet.</div>` : "";
  return `<div class="dk-ch dk-sch">${deskSessName(sid, deskHolderName(sid), "b")}<span class="dk-sub">${esc(deskSessState(s)[1])}</span><span class="grow"></span><button class="btn small" data-desk-session="${esc(sid)}">Open its session</button>${deskMark(deskSessKind(s))}</div>
    <div class="dk-chat" data-desk-chat="${esc(sid)}">${empty}${rows}${where}</div>`;
}
/** The session whose steps the Desk follows live (chat/thread.js followSteps): its lead's, while its chat shows, or
 *  the session's whose chat is open. */
function deskFollowSid() {
  if (deskOpen?.kind === "chat" && deskOpen.project === projFilter) return sessionOf(deskOpen.sid) ? deskOpen.sid : null;
  if (!projFilter || deskOpen || deskTab !== "lead" || deskPick === projFilter) return null;
  const d = deskOf(projFilter), l = deskLead(d);
  return l && !deskLeadEnded(d) && sessionOf(l.sid) ? l.sid : null;
}
/** The Desk's judgment calls (DECIDED lines), the only decisions worth your look: routes, gives, sessions started,
 *  contexts kept and Cue's hand-off asks are steps, kept on their item (deskItemPane). */
const deskCalls = (d) => d.decisions.filter((c) => c.call);
function deskDecisions(d) {
  const calls = deskCalls(d);
  if (!calls.length) return `<div class="dk-empty">No judgment calls yet. Routes and hand-offs are on each item.</div>`;
  return `<div class="dk-pane" data-scroll-key="decisions:${esc(d.project)}"><div class="dk-sec">${deskLog(calls)}</div></div>`;
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
  const who = first.holder?.kind === "crew" ? `${first.holder.name} (${deskHolderName(first.holder.lead_sid)}'s crew)` : first.crew && first.holder ? `${first.holder.name} → ${first.crew}` : first.holder?.name || "";
  const files = deskFilesShort(first.files);
  const sub = [who, first.latest, files ? `has ${files} open` : ""].filter(Boolean).join(" · ");
  return `<div class="dk-chain"><div class="dk-ln"><i></i><div><div class="dk-lt">${esc(first.title)}</div><div class="dk-ls">${esc(sub)}</div></div></div>
    <div class="dk-ln"><i class="wait"></i><div><div class="dk-lt">This one</div><div class="dk-ls">starts when ${esc(deskShort(first.title))} is done${i.after_why ? `: ${esc(i.after_why)}` : ""}</div></div></div></div>`;
}
/** An item's files, short: a few names, the rest as "+N more" ("" for none). */
function deskFilesShort(files, sep = ", ") {
  const names = (files || []).map((f) => f.split("/").pop());
  const more = names.length > 3 ? ` +${names.length - 3} more` : "";
  return names.slice(0, 3).join(sep) + more;
}
/** Holder `h` not at work (as hub/desk.rs at_work): its session at its prompt, or for a crew member, that teammate. */
function deskHolderIdle(h) {
  if (!h) return false;
  // A teammate gone with the crash counts as not at work: its lead is sent it, for that teammate.
  if (h.kind === "crew") {
    const lead = sessionOf(h.lead_sid);
    return !!lead && !(lead.team || []).some((m) => (m.agent_id === h.sid || m.call === h.sid) && m.state === "working");
  }
  const s = sessionOf(h.sid);
  return !!s && !["working", "deciding", "agent"].includes(s.state);
}
/** An item's buttons, by where it is. */
function deskItemActs(i) {
  const b = (act, label, look = "") => `<button class="btn small ${look}" data-desk-act="${act}" data-id="${esc(i.id)}">${esc(label)}</button>`;
  const first = i.after && deskItem(i.project, i.after);
  if (i.state === "to_check" && deskBackFor === i.id) return deskBackHtml(i.id);
  const acts = i.state === "to_check" && deskLooksSoon.has(i.id) ? deskLooksSoonHtml(i.id) : i.state === "to_check" ? b("looks_right", "Looks right", "primary") + b("send_back", "Send back")
    : i.state === "queued" ? b("start_now", "Start it now in a new session") + (first ? b("put_before", `Put it before ${deskShort(first.title)}`) : "")
    : i.state === "new" ? b("start_now", "Start it now in a new session")
    // Its holder's turn cut off (a crash, a quit) leaves it "working" with nobody on it: sent to the holder again.
    : i.state === "working" && deskHolderIdle(i.holder) ? b("start_now", "Restart")
    : deskHasCommit(i) ? `<div class="dk-chk-after">${deskCommitLine(i)}</div>` : "";
  const give = !i.holder && i.state !== "done" ? deskGive(i, "pane") : "";
  return acts || give ? `<div class="dk-acts">${give}${acts}</div>` : "";
}
/** Where it stands in a line: "Bug · next for web-lead · added by you 20m ago". */
function deskItemState(i) {
  const who = i.holder ? `<b>${esc(i.holder.name)}</b>${i.crew ? ` → <b>${esc(i.crew)}</b>` : ""}` : "";
  const by = (pre) => (who ? ` ${pre} ${who}` : "");
  const where = { new: "not given out yet", queued: `next${by("for")}`, working: who ? `with ${who}` : "at work", needs_you: `waits on you${who ? ` · with ${who}` : ""}`, to_check: `ready to check${by("· from")}`, done: `done${by("by")}${i.commit ? ` · committed <span class="dk-cm">${esc(i.commit)}</span>` : ""}` }[i.state] || "";
  // No kind yet (one you added, till the lead says): it starts with where it is.
  return `${i.kind ? `${esc(deskCap(i.kind))} · ${where}` : deskCap(where)} · added by ${esc(deskFrom(i, deskOf(i.project)))} ${ago(deskMs(i.created))} ago`;
}
function deskItemPane(i, d) {
  const decs = d.decisions.filter((c) => c.item_id === i.id && c.call), steps = d.decisions.filter((c) => c.item_id === i.id && !c.call);
  // To check or done: each file it changed with its counts (a click opens its diff); otherwise the files it's on.
  const changed = deskFilesChanged(i), fileNames = deskFilesShort(i.files, " · ");
  const files = changed ? `<div class="dk-fcs">${changed}</div>` : fileNames ? `<div class="dk-files">${esc(fileNames)}</div>` : "";
  const armed = deskDropArm === i.id;
  return `<div class="dk-pane" data-scroll-key="item:${esc(i.id)}">${deskBack(deskLeadName(d) || "Back")}<h1>${esc(i.title)}</h1><div class="dk-state">${deskKindIcon(i)}${deskItemState(i)}</div>
    ${deskSec("Couldn't give it out", i.route_failed ? `<p>${esc(i.route_failed)}</p>` : "")}
    ${deskSec(i.state === "needs_you" ? "It asks you" : "What's holding it up", deskChain(i))}${deskSec("What it made", i.state === "to_check" || i.state === "done" ? deskMadeLinks(i) : "")}${deskItemActs(i)}
    ${deskSec("Done when", i.done_when ? `<p>${esc(i.done_when)}</p>` : "")}
    ${deskSec("Pictures", thumbs(i.images))}
    ${deskSec("Latest", i.latest && i.state !== "queued" ? `<div class="dk-md">${md(i.latest)}</div>` : "")}
    ${deskSec(changed ? "Files changed" : "Files", files)}
    ${deskSec(`Decisions · ${decs.length}`, decs.length ? deskLog(decs) : "")}
    ${deskSec("Steps", steps.length ? deskLog(steps) : "")}
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
  const decs = d.decisions.filter((c) => c.call && (c.sid === sid || c.by === name));
  const decHtml = decs.length ? deskLog(decs, (c) => (c.item_id && deskItem(d.project, c.item_id) ? esc(deskItem(d.project, c.item_id).title) : "")) : "";
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
    ${deskSec("Latest", latest.map((l) => `<div class="dk-md dk-quiet">${md(l)}</div>`).join(""))}
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
  else if (o?.kind === "holder" || o?.kind === "crew" || o?.kind === "chat") {
    const sid = o.kind === "crew" ? o.lead : o.sid;
    if (!sessionOf(sid)) return `<div class="dk-ended">Its session has ended.</div>`;
    if (o.kind !== "crew") { to = { sid }; ph = `Talk to ${deskHolderName(sid, deskGroups(d.project).holders.find((h) => h.sid === sid)?.name)}…`; }
    else { const t = deskMate(o.lead, o.agent); to = { sid, mate: t?.name || "" }; ph = `Tell ${deskHolderName(sid)} about ${t?.name || "it"}…`; }
  }
  const key = deskBoxKey(to);
  // Pictures pasted, dropped or added, and the mic, as in a session's chat box (app/inputs.js, chat/compose.js).
  const mic = dictating?.key === key;
  return `<div class="dk-box" data-drop="${esc(key)}">${draftChips(key)}<div class="dk-box-row"><textarea rows="1" data-text="${esc(key)}" placeholder="${esc(ph)}" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off">${esc(draft(key).text)}</textarea><button class="icon-btn mic ${mic ? "on" : ""}" data-mic="${esc(key)}" aria-label="${mic ? "Stop dictating" : "Dictate"}">${MIC_ICON}</button><button class="icon-btn" data-attach="${esc(key)}" aria-label="Add an image">${IMAGE_ICON}</button><button class="btn primary small" data-desk-send="${esc(key)}">Send</button></div></div>`;
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
  else if (o?.kind === "chat") body = deskSessChat(o.sid);
  else if (o?.kind === "crew") body = deskCrewPane(o.lead, o.agent, d);
  else if (o?.kind === "addon") {
    const h = deskGroups(project).extra.find((x) => (x.key || x.name) === o.key);
    let html = "";
    try { html = (h && addOn.deskPane?.(h)) || ""; } catch (e) { console.error(e); }
    body = h ? `<div class="dk-pane" data-scroll-key="addon:${esc(o.key)}">${deskBack(deskLeadName(d) || "Back")}${html || `<h1>${esc(h.name)}</h1><div class="dk-state">${esc(h.sub || "")}</div>`}</div>` : null;
  }
  if (o && !body) deskOpen = null;
  if (deskOpen) return `<div class="dk-card dk-right">${deskPaneMark(o, d)}${body}${deskBox(d)}</div>`;
  return deskRightChat(d);
}
/** The chat card with nothing open: who runs the Desk (no lead yet, or Change), else the lead's chat (or the line
 *  saying it ended), or Decisions. */
function deskRightChat(d) {
  if (!deskLead(d) || deskPick === d.project) return deskPicker(d);
  const tab = (t, label, n) => `<button class="dk-tab ${deskTab === t ? "on" : ""}" data-desk-tab="${t}" aria-pressed="${deskTab === t}">${label}${n ? `<span class="n">${n}</span>` : ""}</button>`;
  const main = deskTab === "decisions" ? deskDecisions(d) : deskLeadEnded(d) ? deskEndedLine(d) : deskChat(d);
  const ask = deskLeadEnded(d) ? null : deskLeadAsk(d);
  return `<div class="dk-card dk-right"><div class="dk-ch dk-tabs">${tab("lead", esc(deskLeadName(d)), 0)}${tab("decisions", "Decisions", deskCalls(d).length)}<span class="grow"></span>${deskMark("desk")}</div>
    ${main}${deskLeadEnded(d) ? "" : ask ? `<div class="dk-ask">${askFoot(ask, ask.session_id, ask.harness || "claude", deskBoxKey({ project: d.project }))}</div>` : deskBox(d)}</div>`;
}
/** The lead's own open ask (an ASK line's question, a permission, a prompt), not a crew member's (those are in its
 *  crew's Needs you): while it's open the lead takes no message, so its card (choices, Decline, Type answer…) stands
 *  in the box's place, as under a session's chat. Null when none. */
function deskLeadAsk(d) {
  const sid = deskLead(d)?.sid;
  return (sid && state.items.find((i) => i.session_id === sid && i.kind !== "waiting" && !mateOfAsk(i))) || null;
}

// ---------- All projects: every Desk ----------

/** A Desk on the home list: one row, its name and lead, then what it holds today. Opens it. */
function deskCardOf(d) {
  const waits = d.items.filter(deskWaits).length, done = d.items.filter(deskDoneToday).length, moving = d.items.filter(deskMoving).length;
  const lead = deskLeadName(d), open = d.items.filter((i) => i.state !== "done").length;
  const b = waits || moving || done
    ? [waits ? `<b>${waits} waiting on you</b>` : "", moving ? `${moving} moving` : "", done ? `${done} done today` : ""].filter(Boolean).join(" · ")
    : `Quiet${open ? ` · ${open} item${open === 1 ? "" : "s"} open` : ""}`;
  return `<div class="dk-card dk-desk" role="button" tabindex="0" data-desk-project="${esc(d.project)}"><div class="dk-dn"><b>${esc(projLabel(d.project))}</b><span>${lead ? `run by ${esc(lead)}` : "no lead yet"}</span></div><span class="dk-b">${b}</span><span class="grow"></span><span class="dk-open">Open ›</span></div>`;
}
let deskStarting = false;   // + New Desk's Start, until Cue answers
/** + New Desk: a folder (the ones you've worked in), then Start: Cue starts a lead there and opens its Desk. */
function deskNewRow() {
  const have = new Set((state.desk?.projects || []).map((x) => x.project));
  const fs = recentFolders().filter((f) => !have.has(state.projects?.[f]?.own ?? ""));
  const opts = fs.map((f) => `<option value="${esc(f)}">${esc(homeless(f))}</option>`).join("");
  return `<div class="dk-card dk-newdesk"><b>+ New Desk</b>${fs.length ? `<select class="dk-fld" aria-label="Folder for the new Desk">${opts}</select><span class="dk-hint">Cue starts its lead.</span><span class="grow"></span><button class="btn small primary" data-desk-start ${deskStarting ? "disabled" : ""}>${deskStarting ? "Starting…" : "Start"}</button>` : `<span class="dk-hint">Start a session in a folder first (+ New).</span>`}</div>`;
}
/** Start a Desk in `folder`, then open it. */
async function deskStartIn(folder) {
  if (!folder || deskStarting) return;
  deskStarting = true; renderMain();
  try {
    const r = await invoke("desk_start", { folder });
    if (r?.project) { view = "desk"; setProjFilter(r.project); deskOpen = null; deskTab = "lead"; deskChatEnd = true; }
  } catch (e) { toast(`Couldn't start the Desk: ${e}`); }
  deskStarting = false; renderMain();
}
function deskAll() {
  const ds = (state.desk?.projects || []).map((x) => deskOf(x.project)).sort((a, b) => b.items.filter(deskWaits).length - a.items.filter(deskWaits).length || a.project.localeCompare(b.project));
  return `<div class="board dk-board all"><div class="dk-desks" data-scroll-key="desks"><div class="dk-hh">Desks</div>${ds.map(deskCardOf).join("")}${deskNewRow()}</div></div>`;
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
  return `<div class="board dk-board">${deskSessions(projFilter)}${deskRight(projFilter)}${deskList(projFilter)}</div>`;
}
/** After the draw: the lead's chat at its end, as a chat opens (unless you'd scrolled up in it). */
function deskSettle() {
  deskDoneWatch();
  const c = document.querySelector(".dk-chat");
  // Earlier messages just went in above (chat/thread.js loadOlder): what you were reading stays put.
  if (c && holdChat != null) { c.scrollTop = c.scrollHeight - holdChat; holdChat = null; }
  else if (c && deskChatEnd) c.scrollTop = c.scrollHeight;
}

// ---------- acting ----------

/** Open `what` in the chat card (null: back to the lead's chat). */
function deskShow(what) {
  deskLeadLit = null;
  if (what?.kind === "item") deskSeeItem(what.id);
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
  const images = draft(key).images.map(({ name, mime, data }) => ({ name, mime, data }));
  if (!to || (!text && !images.length)) return;
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
  endDictation(key);
  delete drafts[key];
  saveDrafts();
  const list = deskSent.get(project) || [], mine = { text: said, images, at: now(), sent: 0 };
  list.push(mine);
  deskSent.set(project, list);
  // To the lead on its own: the chat shows it at once, Sending. From an item: the item stays till it went.
  if (!it) { deskTab = "lead"; deskChatEnd = true; }
  renderMain();
  try {
    await invoke("desk_send", { project, text: said, images });
    mine.sent = now();
    // Sent from an item: the chat shows it, on the lead's (unless you've opened something else meanwhile).
    if (it && deskOpen?.kind === "item" && deskOpen.id === id) { deskOpen = null; deskTab = "lead"; deskChatEnd = true; renderMain(); }
  }
  catch (e) { deskSent.set(project, (deskSent.get(project) || []).filter((p) => p !== mine)); Object.assign(draft(key), { text, images }); toast(`Not sent: ${e}`); renderMain(); }
}
/** Item `id` of the project picked, open in the chat card (the Inbox's Open in Desk). */
function deskOpenItem(id) {
  deskSeeItem(id);
  deskOpen = { kind: "item", id, project: projFilter, fresh: now() };
  deskDropArm = deskGiveFor = null;
}
async function deskAct(id, act, el = null) {
  // Looks right commits the item's files at once; Undo takes that commit back (desk_commit.js).
  if (act === "looks_right") return deskLooksRight(id);
  if (act === "looks_undo") return deskLooksUndo(id);
  if (act === "undo_commit") return deskCommitUndo(id);
  // Drop takes two clicks: the first arms it.
  if (act === "drop" && deskDropArm !== id) { deskDropArm = id; return renderMain(); }
  deskDropArm = null;
  // Send back opens the item's own box for why (deskBackHtml); its Send sends that, to the item's holder only.
  if (act === "send_back") { deskBackFor = deskBackFor === id ? null : id; renderMain(); return deskBackFocus(id); }
  if (act === "back_cancel") { endDictation(`back:${id}`); deskBackFor = null; return renderMain(); }
  let text, key, images = [];
  if (act === "back_go") {
    act = "send_back";
    key = `back:${id}`;
    text = draft(key).text.trim();
    images = draft(key).images.map(({ name, mime, data }) => ({ name, mime, data }));
    if (!text && !images.length) { toast("Say why it goes back first", 4000); return deskBackFocus(id); }
    endDictation(key);
  }
  try {
    const it = await invoke("desk_item_act", key ? { id, act, text, images } : { id, act });
    // What it was sent back with is gone from the box once it went (kept when it didn't).
    if (key) { delete drafts[key]; saveDrafts(); deskBackFor = null; toast(`Sent back${it?.holder?.name ? ` to ${it.holder.name}` : ""}`); }
    if (act === "drop") deskOpen = null;
  }
  catch (e) { toast(String(e), 5000); }
  renderMain();
}
/** Item `id`'s Send back, open: "Why send it back?" and Cancel over a box of its own (never the lead's), as the Desk's
 *  boxes are: words, the mic, a picture, Send. Enter sends, ⇧Enter a new line, Esc closes it. */
function deskBackHtml(id) {
  const key = `back:${id}`, mic = dictating?.key === key;
  return `<div class="dk-sb"><div class="dk-sb-h"><span class="dk-sb-l">Why send it back?</span><button class="dk-sb-x" data-desk-act="back_cancel" data-id="${esc(id)}">Cancel</button></div><div class="dk-box dk-sb-box" data-drop="${esc(key)}">${draftChips(key)}<div class="dk-box-row"><textarea rows="1" data-text="${esc(key)}" aria-label="Why send it back?" placeholder="What isn't right yet" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off">${esc(draft(key).text)}</textarea><button class="icon-btn mic ${mic ? "on" : ""}" data-mic="${esc(key)}" aria-label="${mic ? "Stop dictating" : "Dictate"}">${MIC_ICON}</button><button class="icon-btn" data-attach="${esc(key)}" aria-label="Add an image">${IMAGE_ICON}</button><button class="btn primary small" data-desk-act="back_go" data-id="${esc(id)}">Send</button></div></div></div>`;
}
const deskBackFocus = (id) => document.querySelector(`textarea[data-text="${CSS.escape(`back:${id}`)}"]`)?.focus();
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
  ...DESK_LEAD_CLICKS,
  deskSend: (el) => deskSendBox(el.dataset.deskSend),
  deskAct: (el) => { deskGiveFor = null; deskAct(el.dataset.id, el.dataset.deskAct, el); },
  deskUndo: (el) => deskUndo(el.dataset.deskUndo),
  deskLogRow: (el) => { const id = el.dataset.deskLogRow; if (!deskLogOpen.delete(id)) deskLogOpen.add(id); renderMain(); },
  deskSession: (el) => openSession(el.dataset.deskSession),
  deskLeadChat: () => { deskPick = null; deskTab = "lead"; deskShow(null); deskLeadLit = projFilter; renderMain(); },
  deskChkItem: (el) => deskShow({ kind: "item", id: el.dataset.deskChkItem }),
  deskRen: (el) => renameSession({ dataset: { rename: el.dataset.deskRen } }),
  deskChatOf: (el) => { deskChatEnd = true; deskShow({ kind: "chat", sid: el.dataset.deskChatOf }); },
  deskNewSession: (el) => deskNewSession(el.dataset.deskNewSession),
  deskMember: (el) => deskMember(el.dataset.project, el.dataset.deskMember, true),
  deskDoneFold: (el) => {
    const sid = el.dataset.deskDoneFold;
    if (deskFoldOpen.has(sid)) deskFoldOpen.delete(sid); else deskFoldOpen.add(sid);
    try { localStorage.setItem("cue.deskFold", JSON.stringify([...deskFoldOpen])); } catch { /* kept for this window only */ }
    renderMain();
  },
  deskMade: (el) => { deskSeeItem(el.closest("[data-desk-item]")?.dataset.deskItem || deskOpen?.id); invoke("open_link", { url: el.dataset.deskMade }).catch((err) => toast(`Couldn't open: ${err}`)); renderMain(); },
  deskDiffFile: (el) => deskShowFile(el.dataset.deskDiffFile, +el.dataset.n),
  deskCrewOpen: (el) => deskOpenCrew(el.dataset.lead, el.dataset.deskCrewOpen),
  deskInbox: (el) => (typeof inboxOpen === "function" ? inboxOpen(el.dataset.deskInbox) : openSession(el.dataset.deskInbox)),
  deskHome: () => { setProjFilter(""); deskOpen = null; deskPick = null; renderMain(); },
  deskStart: (el) => deskStartIn(el.closest(".dk-newdesk")?.querySelector("select")?.value),
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
  // In a session's name box (✎ on its row): the click stays in the box, it doesn't open the row.
  if (e.target.closest?.(".dk-board .rename-in")) { e.stopPropagation(); return; }
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
  if (e.key === "Enter" && !e.shiftKey && key.startsWith("back:")) { deskAct(key.slice(5), "back_go"); return true; }
  if (e.key === "Escape" && key.startsWith("back:")) { deskAct(key.slice(5), "back_cancel"); return true; }
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
  // Leaving a session's name box (✎ on the Desk) drops the rename (Enter has kept it already). Checked a moment later:
  // a redraw puts the cursor back in the new box, and that isn't leaving it.
  document.addEventListener("focusout", (e) => {
    if (!e.target?.matches?.(".dk-board .rename-in")) return;
    const key = e.target.dataset.text;
    setTimeout(() => {
      if (!renaming || `rename:${renaming}` !== key || document.activeElement?.dataset?.text === key) return;
      renaming = null;
      delete drafts[key];
      renderMain();
    }, 0);
  }, true);
  // The lead's chat scrolled: whether it's at its end (new messages keep it there).
  document.addEventListener("scroll", (e) => { const c = e.target; if (c?.matches?.(".dk-chat")) deskChatEnd = c.scrollHeight - c.scrollTop - c.clientHeight < 24; }, true);
}
