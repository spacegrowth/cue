// Cue UI desk — the sessions on the left: the lead and the Desk's sessions with their crew, the head strip, the Desk switcher, Add to / Remove from Desk.
// Split from desk.js, which keeps the Desk's state and item basics; loaded after it (index.html, mini.html).
// ---------- the sessions: the project's sessions, on the Desk and not ----------

/** A session's state, for its row: [dot, words]. "needs you" when it asks you something (a permission too: the request
 *  itself is on its item and in its chat, never as a card in this list, #431). */
function deskSessState(s) {
  const asks = state.items.some((i) => i.session_id === s.session_id && actionableRequest(i));
  return asks ? ["ask", "needs you"] : s.state === "working" ? ["", "working"] : ["idle", "idle"];
}
/** The item a session holds (an ask before what's at work), or null. */
const deskSessItem = (d, sid) => deskSessItems(d, sid)[0] || null;
/** Every open item session `sid` holds, the one it's on now first (working, then waiting on you, then the newest). */
const deskSessItems = (d, sid) => d.items.filter((i) => i.state !== "done" && i.holder && i.holder.kind !== "crew" && i.holder.sid === sid)
  .sort((a, b) => (b.state === "working") - (a.state === "working") || (b.state === "needs_you") - (a.state === "needs_you") || (b.updated ?? 0) - (a.updated ?? 0));
/** A session row's size and cache, as its card says them: "142k · cache 3m" (minutes its cache has left)
 *  or "142k · cold"; amber with under 5 minutes left. With `when` (an idle row), when its latest reply came first:
 *  "2h ago · 142k · cold". From its card's numbers (insights.js insCache), asked for here when no card brings them
 *  (deskInsAsk); "" without. */
function deskSessCache(sid, when = false) {
  deskInsAsk(sid);
  const c = typeof insCache !== "undefined" ? insCache.get(sid) : null;
  if (!c?.tokens && !c?.until_ms) return "";
  const left = (c.until_ms || 0) - now();
  const cache = left > 0 ? `cache ${Math.ceil(left / 60000)}m` : c.until_ms ? "cold" : "";
  const details = [c.tokens && insTok(c.tokens), cache].filter(Boolean).join(" · ");
  const at = when && c.last_ms ? `<span data-ago="${c.last_ms}">${ago(c.last_ms)}</span> ago · ` : "";
  // How full its window is, as a short bar at the end of the line (no % written; amber from 80% full, #423).
  const pct = c.tokens ? Math.min(100, Math.round((c.tokens / (c.window || HANDOFF.window)) * 100)) : 0;
  const meter = pct ? ` · <span class="dk-meter${pct >= 80 ? " full" : ""}" role="img" aria-label="${pct}% of its window"><i style="width:${Math.max(3, pct)}%"></i></span>` : "";
  return `<div class="dk-s3 dk-size${left > 0 && left < HOT_CACHE_MS ? " hot" : ""}">${at}${esc(details)}${meter}</div>`;
}
/** The first line of a session's latest reply, from the same numbers ("" until they're in). */
const deskSessSaid = (sid) => (typeof insCache !== "undefined" && insCache.get(sid)?.said) || "";
/** Sessions on a Desk whose numbers no card brings (an idle one with its card gone, #185): asked for together,
 *  each again a minute after it was last asked, so its last reply, size and cache show and stay current. */
const deskInsAt = new Map();
let deskInsWant = null;
function deskInsAsk(sid) {
  if (!sid || typeof insCache === "undefined" || (state.items || []).some((i) => i.session_id === sid)) return;
  if (now() - (deskInsAt.get(sid) || 0) < 60000) return;
  deskInsAt.set(sid, now());
  if (deskInsWant) return deskInsWant.add(sid);
  deskInsWant = new Set([sid]);
  setTimeout(async () => {
    const sids = [...deskInsWant];
    deskInsWant = null;
    try {
      const r = (await invoke("insight_state", { sessionIds: sids })) || {};
      let changed = false;
      for (const id of sids) {
        const c = r[id], had = insCache.get(id);
        if (c && (!had || (c.last_ms || 0) >= (had.last_ms || 0)) && JSON.stringify(c) !== JSON.stringify(had)) { insCache.set(id, c); changed = true; }
      }
      if (changed) renderMain();
    } catch {}
  }, 0);
}
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
/** One row: what it is as its icon before the name (`mark`: desk, lead, session or crew, the icons the chat headers use,
 *  #202), its name, its state; then the item it's on (no line when it's on none, #179); then its size and cache. `cls`
 *  is dk-srow; the lead's card in the head strip is dk-lcard, with `tag` "Lead" as a pill after its name (pick M1, #446).
 *  Rows run edge to edge, so their hover and their lime reach both sides of the column. */
function deskSessRow({ name, sid = "", mark = "", dot, word, item, line, on, attrs, crew, step = "", size = "", loose = false, past = false, cls = "dk-srow", pre = "", harness = "", tag = "" }) {
  const what = item ? `<span class="dk-w">${deskKindIcon(item)}<span class="dk-id">${esc(deskNum(item))}</span> ${esc(item.title)}</span>` : line ? `<span class="dk-w">${esc(line)}</span>` : "";
  const arrow = on ? '<span class="dk-larrow" aria-hidden="true">›</span>' : "";   // only on the open row or card (#219)
  const loosePill = loose ? `<span class="dk-pill loose">not committed</span>` : "";
  const ic = DESK_MARKS[mark] ? `<span class="dk-ric" role="img" data-desk-type="${mark}" aria-label="${DESK_MARKS[mark]}">${deskKindIcon(mark)}</span>` : "";
  return `<div class="${cls} ${crew ? "crew" : ""} ${past ? "past" : ""} ${on ? "on" : ""}" role="button" tabindex="0" ${attrs}>
    <div class="dk-s1">${pre ? `<span class="dk-lk">${pre}</span>` : ""}${ic}${sid ? deskSessName(sid, name, "dk-nm") : `<span class="dk-nm">${esc(name)}</span>`}${harness ? agentBadge(harness) : ""}${tag ? `<span class="dk-ltag">${esc(tag)}</span>` : ""}<span class="dk-st"><i class="${dot}"></i><b>${word}</b></span>${arrow}</div>
    ${what || loosePill ? `<div class="dk-s2">${what}${loosePill}</div>` : ""}${step}${size}</div>`;
}
/** The lead and the sessions already on this Desk, with their crew. */
function deskSessions(project) {
  const d = deskOf(project), o = deskOpen, leadSid = deskLead(d)?.sid;
  const sessions = (state.sessions || []).filter((s) => s.project === deskWorkspace(d) && s.session_id);
  const lit = (sid) => (sid === leadSid && !o && deskPick !== project) || ((o?.kind === "chat" || o?.kind === "holder") && o.sid === sid);
  const row = (s, card = false) => {
    const sid = s.session_id, [dot, word] = deskSessState(s);
    const lead = sid === leadSid || deskIsLead(sid, d.items.filter((i) => deskGroupKey(i.holder) === sid));
    const attrs = sid === leadSid ? `data-desk-lead-chat="${esc(sid)}"` : `data-desk-chat-of="${esc(sid)}"`;
    const mate = (t, past) => {
      const key = t.agent_id || t.call, it = past ? null : deskCrewItem(project, sid, key);
      const [cd, cw] = past ? ["idle", t.state === "aside" ? "set aside" : t.state] : mateNeeds(sid, t) ? ["ask", "needs you"] : deskMateBusy(t) ? ["", t.state] : ["idle", t.state];
      return deskSessRow({ name: t.name, mark: "crew", dot: cd, word: cw, item: it, line: it ? "" : t.what, crew: true, past, loose: !past && !deskMateBusy(t) && !!t.uncommitted,
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
    // The card always carries a second line, as every row under it does (#269): the item it's on, else the first line
    // of its latest reply, else how many items it has open; a one-line card read as the strip's header.
    if (card) {
      const cItem = deskSessItem(d, sid), open = d.items.filter((i) => i.state !== "done").length;
      return deskSessRow({ name: deskHolderName(sid), sid, cls: "dk-lcard", mark: "desk", harness: s.harness, tag: "Lead", dot, word, item: cItem, line: cItem ? "" : deskSessSaid(sid) || (open ? `${open} open item${open === 1 ? "" : "s"} on this Desk` : "No open items on this Desk"), on: lit(sid), attrs, step: deskStepLine(dot === "", s.compacting_ms ? "Compacting…" : s.doing, s.compacting_ms || s.doing_ms), size: deskSessCache(sid, !cItem && dot !== "") });
    }
    // On no item: the first line of its latest reply, and when it came beside its size (#185).
    const item = deskSessItem(d, sid), idle = dot !== "" && !item;
    return deskSessRow({ name: deskHolderName(sid), sid, mark: sid === leadSid ? "desk" : lead ? "lead" : "session", dot, word, item, line: idle ? deskSessSaid(sid) : "", on: lit(sid), attrs, step: deskStepLine(dot === "", s.compacting_ms ? "Compacting…" : s.doing, s.compacting_ms || s.doing_ms), size: deskSessCache(sid, idle) }) + crew;
  };
  const leadS = sessions.find((s) => s.session_id === leadSid);
  const mine = sessions.filter((s) => s !== leadS && deskHasSession(d, s.session_id));
  // At work: the session itself, its background agents (state "agent" with helpers running), or any of its crew. A
  // session idle while its crew works is listed with the working ones, so their rows never sit under idle sessions.
  const busy = (s) => deskSessState(s)[1] === "working" || (s.state === "agent" && !!s.helpers)
    || (sessionOf(s.session_id)?.team || []).some(deskMateBusy);
  // Each group most recent first: the one whose state changed last on top (#184). A row moves only when its state
  // does, never on a click.
  const recent = (list) => list.sort((a, b) => (b.since_ms || 0) - (a.since_ms || 0));
  const atWork = recent(mine.filter(busy));
  // The rest: with work on the Desk (an item it holds, or one done in the last day) first; the others, idle with no
  // recent item, fold into one "N cold" line, shut by default (#363, #428).
  const on = recent(mine.filter((s) => !busy(s))).map((s) => ({ s, work: deskHasWork(d, s.session_id) }));
  const group = (title, list) => (list.length ? `<div class="dk-gh">${title} <span class="n">${list.length}</span></div>${list.map((s) => row(s)).join("")}` : "");
  // The strip's meta line counts this Desk's sessions, and the working ones as the Working group lists them: working
  // itself, or its helpers ("agent") or crew at work; working means working (#402).
  const runs = atWork.length;
  // One Desk list (#428): Working, then Idle; Idle's count takes in the cold ones folded at its end, so the two add up
  // to the strip's session count.
  const onGroup = on.length ? `<div class="dk-gh">Idle <span class="n">${on.length}</span></div>${deskWithOlder(`side:${project}`, on, (r) => row(r.s), undefined, "cold")}` : "";
  const body = group("Working", atWork) + onGroup || '<div class="dk-empty">No other sessions on this Desk.</div>';
  const picking = deskAddingSession === project;
  // Add session offers this Desk's own sessions (ticked), then a light line and only free, recent ones: none another
  // Desk has, none stopped or idle for over a day (#189).
  const candidates = picking ? deskSessionsOf(project, d).filter((r) => deskHasSession(d, r.sid) || (!deskElsewhere(r.sid, d) && !deskSessOld(sessionOf(r.sid)))) : [];
  if (picking && leadSid && !candidates.some((r) => r.sid === leadSid)) {
    candidates.unshift({ sid: leadSid, name: deskLeadName(d) });
  }
  candidates.sort((a, b) => Number(deskHasSession(d, b.sid)) - Number(deskHasSession(d, a.sid)));
  const history = new Set(d.work_history || []);
  const picker = picking ? `<div class="dk-add-picker" id="desk-add-picker" role="group" aria-label="Desk membership">${candidates.length
    ? candidates.map((r, n) => {
      const member = deskHasSession(d, r.sid), locked = member && history.has(r.sid);
      const line = n > 0 && !member && deskHasSession(d, candidates[n - 1].sid) ? '<div class="dk-add-div" role="separator"></div>' : "";
      return `${line}<label class="dk-add-option"><input type="checkbox" data-desk-member="${esc(r.sid)}" data-project="${esc(project)}"${member ? " checked" : ""}${locked ? " disabled" : ""}><span aria-hidden="true">${deskKindIcon(r.sid === leadSid ? "desk" : r.lead ? "lead" : "session")}</span><span>${esc(r.name)}${r.sid === leadSid ? " · Dispatcher" : ""}${locked ? '<small class="dim">Has work history</small>' : ""}</span></label>`;
    }).join("")
    : '<div class="dim">No project sessions available.</div>'}</div>` : "";
  return `<div class="dk-card dk-sessions">${deskSessHead(project, mine.length, runs, leadS ? row(leadS, true) : "", deskLeadBar(leadS))}${leadS ? "" : deskLeadBar(leadS)}
    <div class="dk-scroll" data-scroll-key="desk-sessions:${esc(project)}">${body}</div>
    <div class="dk-sfoot"><div class="dk-add-anchor"><button class="btn small dk-add-button" data-desk-add-session="${esc(project)}" aria-label="Add session" aria-expanded="${picking}"${picking ? ' aria-controls="desk-add-picker"' : ""}><span aria-hidden="true">+</span></button>${picker}</div></div></div>`;
}
/** Whether session `sid` belongs to a Desk other than `d`: it runs it, is on its list, or holds (or its crew holds) an
 *  item there not done (#189). */
const deskElsewhere = (sid, d) => (state.desk?.projects || []).some((o) => o.project !== d.project && (deskHasSession(o, sid)
  || (o.items || []).some((i) => i.state !== "done" && i.state !== "dropped" && (i.holder?.sid === sid || i.holder?.lead_sid === sid))));
/** An old, finished session: gone, stopped, or idle (not asking you) for over a day (#189). */
const DESK_OLD_MS = 24 * 3600 * 1000;
const deskSessOld = (s) => !s || s.state === "stopped" || (deskSessState(s)[0] === "idle" && !!s.since_ms && Date.now() - s.since_ms > DESK_OLD_MS);
/** The line over the lead's card (#263, #359): the same working light a chat shows along its top (.sweep, chat.css), moving
 *  while the lead works (calmer after a quiet spell, still and amber when stuck, as sweep() in chat/thread.js), a plain
 *  lime line while it's idle. No data-sweep: that one is the open chat's, carried over by patch.js. The desk redraws
 *  often, so each new light starts where the clock says its pass is, never over from the left. */
function deskLeadBar(s) {
  if (!(s?.state === "working" || s?.compacting_ms)) return '<div class="dk-lbar"></div>';
  if (s.stuck_ms && !s.compacting_ms) return '<div class="dk-lbar on"><i class="sweep stalled"></i></div>';
  const calm = now() - Math.max(s.doing_ms || 0, s.compacting_ms || 0, s.since_ms || 0) > QUIET_MS, ms = calm ? CALM_MS : SWEEP_MS;
  return `<div class="dk-lbar on"><i class="sweep${calm ? " calm" : ""}" style="animation-duration:${ms}ms;animation-delay:-${Date.now() % ms}ms"></i></div>`;
}
/** The head strip (pick B2), set in from the card's sides so it never meets the window's own colour: the Desk's name
 *  opens the switcher (every Desk, All Desks, + New Desk), ⋯ to hand the Desk to another session, this Desk's name (✎ on hover renames it, as the projects list
 *  does), how many sessions and how many work; then the lead's line (`bar`, #263, #359) and its card (`lead`), which says
 *  "Lead" in a pill by its name and has a divider under it (pick M1, #446: no "Run by" head). */
let deskDeleteAsk = null, deskDeleting = false, deskCloseSelected = new Set();
function deskSessHead(project, n, working, lead = "", bar = "") {
  if (deskRecent[0] !== project) deskRecentPut(project);
  const d = deskOf(project);
  const title = deskName(d);
  const name = pjRenaming === project
    ? `<input class="pj-name-in" data-text="pjname:${esc(project)}" maxlength="40" placeholder="${esc(project)}" aria-label="Rename this Desk" spellcheck="false" value="${esc(draft(`pjname:${project}`).text)}"/>`
    : `<b>${esc(title)}</b><button class="dk-ren" data-pjren="${esc(project)}" aria-label="Rename this Desk"><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 20h4L19 9l-4-4L4 16z"/><path d="M14 6l4 4"/></svg></button>`;
  const more = lead ? `<button class="dk-more-btn" data-desk-change aria-label="Hand this Desk to another session"><svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><circle cx="5" cy="12" r="1.8"/><circle cx="12" cy="12" r="1.8"/><circle cx="19" cy="12" r="1.8"/></svg></button>` : "";
  const sw = pjRenaming === project ? `<span class="dk-ric" aria-hidden="true">${deskKindIcon("tray")}</span>${name}` : `<span class="dk-sw-anchor"><button class="dk-sw-btn" data-desk-switch aria-haspopup="menu" aria-expanded="${deskSwitchOpen}"><span class="dk-ric" aria-hidden="true">${deskKindIcon("tray")}</span><b>${esc(title)}</b><svg class="dk-sw-caret" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m6 9 6 6 6-6"/></svg></button>${deskSwitchOpen ? deskSwitchMenu(project) : ""}</span>${name.slice(name.indexOf("<button"))}`;
  return `<div class="dk-strip"><div class="dk-sh"><div class="dk-shr">${sw}<span class="grow"></span>${more}</div><div class="dk-subr"><span class="dk-sub">${n} session${n === 1 ? "" : "s"}${working ? ` · ${working} working` : ""}</span>${lead && typeof deskSettingsLine === "function" ? deskSettingsLine(deskOf(project)) : ""}</div>${pjRenaming === project ? "" : deskRecentChips(project)}</div>${lead ? `${bar}${lead}` : ""}</div>`;
}
/** The switcher (pick A, #240): the Desk's name opens a menu of every Desk, those waiting on you first, found by typing,
 *  ↑ ↓ ↵ to pick; All Desks and + New Desk under them. A row is the Desk's name and what needs you, nothing else: a
 *  name too long for its room fades at its end and, while its row is lit, slides to show the rest (desk.css). Open while `deskSwitchOpen`; `deskSwitchAt` is the lit row. */
let deskSwitchOpen = false, deskSwitchAt = 0;
function deskSwitchList() {
  const q = (draft("desk-find").text || "").trim().toLowerCase();
  return deskSorted().filter((d) => !q || deskName(d).toLowerCase().includes(q));
}
function deskSwitchMenu(project) {
  const ds = deskSwitchList(), at = Math.min(deskSwitchAt, ds.length - 1);
  const rows = ds.map((d, k) => {
    const n = deskWaitingItems(d).length;
    return `<div class="dk-sw-row${k === at ? " at" : ""}${d.project === project ? " on" : ""}" role="menuitem" tabindex="-1" data-desk-go="${esc(d.project)}"><span class="dk-sw-tick" aria-hidden="true">${d.project === project ? "✓" : ""}</span><span class="dk-sw-nm"><span>${esc(deskName(d))}</span></span>${n ? `<span class="dk-sw-need">${n} need${n === 1 ? "s" : ""} you</span>` : ""}</div>`;
  }).join("");
  return `<div class="dk-sw-menu" role="menu"><input class="dk-sw-find" data-text="desk-find" placeholder="Find a Desk…" aria-label="Find a Desk" spellcheck="false" autocomplete="off" value="${esc(draft("desk-find").text || "")}"/><div class="dk-sw-list">${rows || `<div class="dk-sw-none">No Desk matches.</div>`}</div><div class="dk-sw-foot"><button class="dk-sw-act" data-desk-home>All Desks</button><button class="dk-sw-act" data-desk-new-desk>+ New Desk</button></div><div class="dk-sw-keys">↑ ↓ move · ↵ open · Esc close</div></div>`;
}
/** Close the switcher (and forget what was typed in it). */
function deskSwitchClose() { deskSwitchOpen = false; delete drafts["desk-find"]; }
/** Session `sid` on `project`'s Desk (`on`) or off it: the sessions its lead routes to without asking you. */
async function deskMember(project, sid, on) {
  try {
    await invoke("desk_member", { project, sid, on });
  } catch (e) {
    renderMain();
    toast(String(e));
  }
}
/** ⋯ → Add to Desk / Remove from Desk, for a session of a project (not the Desk's lead, which is always on it). */
function deskMemberItem(sid, s) {
  const workspace = s?.project;
  if (!workspace || !sid) return "";
  const matches = (state.desk?.projects || []).filter((d) => deskWorkspace(d) === workspace);
  const targets = matches.length ? matches : [deskOf(workspace)];
  return targets.filter((d) => deskLead(d)?.sid !== sid).map((d) => {
    const on = (d.members || []).includes(sid);
    const label = targets.length > 1 ? deskName(d) : "Desk";
    return `<button data-desk-member-set="${esc(sid)}" data-project="${esc(d.project)}" data-on="${on ? "0" : "1"}">${on ? "Remove from" : "Add to"} ${esc(label)}<span>${on ? "Its lead asks before giving it work again" : "Its lead can give it work without asking you"}</span></button>`;
  }).join("");
}
