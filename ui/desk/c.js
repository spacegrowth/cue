// Cue UI desk — the new Desk layout (#738, "C, reviewed" of the #726 mocks), tried with "Try the new Desk layout" in
// Desk settings: off by default, remembered across launches. Today's page (B2, desk/b2.js) with three columns on the
// green: at the left the Desk's sessions right on the green, grouped by machine, the lead on top with a thin line under
// it; in the middle the lead's chat (or what's open in its place, as on B2), its panel only Now and Activity; at the
// right Items, one white card for what waits on you and what's working (a row per item with its six steps, the one it's
// at in the Board's waiting stripes), and under it Up next and Done today side by side on the green, each folding to its
// heading. One count everywhere: the header's "N wait on you" is the Desk's own. Drawn by desk/view.js deskView;
// everything here sits under `.dk-c` (styles/desk-c.css). Loaded after desk/b2.js (index.html, mini.html).

const DESK_C_KEY = "cue.deskC";
/** The new Desk layout is on. Kept in this window's storage, like the Desk's other choices of look. */
let deskC = (() => { try { return localStorage.getItem(DESK_C_KEY) === "1"; } catch { return false; } })();
/** Up next and Done today folded to their headings, by key ("next", "done"); kept across restarts. */
const deskCFolds = (() => { try { return new Set(JSON.parse(localStorage.getItem("cue.deskCFolds") || "[]")); } catch { return new Set(); } })();

/** Turn the new layout on or off, at once. It's today's page underneath, so the new Desk page (#628) goes off with it. */
function deskCTurn(on) {
  deskC = on;
  try { localStorage.setItem(DESK_C_KEY, on ? "1" : "0"); } catch { /* not kept: on for this run only */ }
  if (on && typeof deskV2 !== "undefined" && deskV2) return deskV2Turn(false);
  renderMain();
}
/** The switch, by Desk settings' title beside the new Desk page's. */
const deskCToggle = () => `<button class="dk2-sw${deskC ? " on" : ""}" role="switch" aria-checked="${deskC}" data-desk-c-try><span class="dk2-knob" aria-hidden="true"></span>Try the new Desk layout</button>`;
if (typeof document !== "undefined") {
  document.addEventListener("click", (e) => {
    if (!e.target.closest?.("[data-desk-c-try]")) return;
    e.stopPropagation();
    deskCTurn(!deskC);
  }, true);
}
/** The new layout is what the Desk shows now: on, and not the new Desk page. */
const deskCOn = () => deskC && !(typeof deskV2 !== "undefined" && deskV2);

const DESK_C_MAC = `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="5" width="16" height="11" rx="1.5"/><path d="M2 19h20"/></svg>`;
const DESK_C_CREW = `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="9" cy="8" r="3"/><path d="M3 20a6 6 0 0 1 12 0"/><circle cx="17" cy="9" r="2.4"/><path d="M15.5 14.5A5 5 0 0 1 21 19"/></svg>`;
const DESK_C_ARROW = (open) => `<span class="dkn-fa" aria-hidden="true">${open ? "▾" : "▸"}</span>`;

// ---------- the header's count ----------

/** What the header says waits on you while the Desk shows this layout: the Desk's own count (the strip's, B2's
 *  deskB2WaitN), and what its click opens. Null off the Desk, with the layout off, or with nothing waiting. */
function deskCHeadWait() {
  if (!deskCOn() || view !== "desk" || !deskSelection()) return null;
  const d = deskOf(deskSelection());
  if (!d) return null;
  const taskApprovals = deskTaskApprovals(d);
  const n = deskB2WaitN(d, taskApprovals);
  if (!n) return null;
  const g = deskGroups(d.project, taskApprovals), first = [...g.asks, ...g.check][0];
  return { n, attrs: first ? `data-desk-chk-item="${esc(first.id)}"` : `data-desk-tab="lead"` };
}

/** The header's chip in that case: "2 wait on you", the same number as the strip, the lead's panel and the list. */
function deskCHeadChip() {
  const w = deskCHeadWait();
  return w ? `<button class="hchip dkn-hwait" data-desk-c-head ${w.attrs}><span class="hdot ask"></span>${w.n}<span class="hlbl"> wait${w.n === 1 ? "s" : ""} on you</span></button>` : "";
}
if (typeof document !== "undefined") {
  // The header's count opens what waits on you first, as Check does.
  document.addEventListener("click", (e) => {
    const el = e.target.closest?.("[data-desk-c-head]");
    if (!el) return;
    e.stopPropagation();
    if (el.dataset.deskChkItem) DESK_CLICKS.deskChkItem(el); else DESK_CLICKS.deskLeadChat(el);
  }, true);
}

// ---------- the sessions column, on the green ----------

/** How a session stands in the column: [dot, words]. dot: "ask" (asks you), "check" (its item waits for you to check
 *  it), "work", or "idle". */
function deskCSessWord(d, s) {
  const sid = s.session_id, it = deskSessItem(d, sid), [dot] = deskSessWithHelpers(s);
  const pre = it ? `#${it.id} · ` : "";
  if (dot === "ask" || it?.state === "needs_you") return ["ask", `${pre}<span class="dkn-ask">asks you</span>`];
  if (it?.state === "to_check") return ["check", `${esc(pre)}ready to check`];
  if (dot === "" && it) {
    const st = deskProgStamps(it), at = deskCAt(it), t = at >= 0 ? st[at] : 0;
    return ["work", `${esc(pre)}${esc(DESK_PROG_STEPS[Math.max(0, at)])}${t ? ` · <span data-ago="${t}">${ago(t)}</span>` : ""}`];
  }
  if (dot === "") return ["work", esc(s.running ? `Running: ${s.running}` : s.doing || "working")];
  if (it) return ["idle", `${esc(pre)}${deskB2Status(it)[0]}`];
  return ["idle", `free · idle${s.since_ms ? ` <span data-ago="${s.since_ms}">${ago(s.since_ms)}</span>` : ""}`];
}
/** A crew member of `sid` as a line under it, while it's in play. */
function deskCMate(project, sid, t, o) {
  const key = t.agent_id || t.call;
  const ask = mateNeeds(sid, t), busy = deskMateBusy(t);
  const on = o?.kind === "crew" && o.lead === sid && o.agent === key;
  const word = ask ? `<span class="dkn-ask">asks you</span>` : esc(busy ? "working" : t.state || "");
  const line = t.step || t.what || "";
  return `<button class="dkn-cr${on ? " on" : ""}" data-desk-crew="${esc(key)}" data-lead="${esc(sid)}" aria-label="${esc(t.name)}"><span class="dkn-n"><i class="dkn-d ${ask ? "ask" : busy ? "work" : "idle"}" aria-hidden="true"></i>${DESK_C_CREW}<span class="dkn-nm">${esc(t.name)}</span><span class="dkn-st">${word}</span></span>${line ? `<span class="dkn-s">${esc(line)}</span>` : ""}</button>`;
}
/** The Desk's sessions at the left, on the green: its name and one count line; the lead pinned on top, a thin line under
 *  it; then a heading per machine ("THIS MAC · 3 of 4 busy") over its sessions, each with its crew in play. A click
 *  opens that session's chat in the middle; the lead's brings its chat back. All N sessions › at the foot opens the
 *  full list (add, remove, close the old ones). In a narrow window the column is a strip of dots (styles/desk-c.css). */
function deskCSessions(project) {
  const d = deskOf(project), o = deskOpen, leadSid = deskLead(d)?.sid;
  if (!d) return "";
  const all = (state.sessions || []).filter((s) => s.project === deskWorkspace(d) && s.session_id);
  const leadS = all.find((s) => s.session_id === leadSid);
  const mine = all.filter((s) => s !== leadS && deskHasSession(d, s.session_id));
  // At work, or holding an open item, or still warm: shown. Cold ones holding nothing are only in All sessions.
  const busy = (s) => deskSessWithHelpers(s)[0] !== "idle";
  const shown = mine.filter((s) => busy(s) || !deskSessOlder(d, s));
  const working = mine.filter(busy).length;
  const machines = [...new Set(all.map((s) => s.machine || ""))];
  const lit = (sid) => ((o?.kind === "chat" || o?.kind === "holder") && o.sid === sid) || (o?.kind === "item" && deskHolderSid(deskItem(project, o.id)) === sid);
  // The lead: its name, Lead, how it stands; lit while its chat is what's in the middle.
  let lead = "";
  if (leadSid) {
    const s = sessionOf(leadSid), [dot, word] = s ? deskSessWithHelpers(s, false) : ["idle", "ended"];
    const on = !o && deskPick !== project;
    lead = `<button class="dkn-lead${on ? " on" : ""}" data-desk-lead-chat="${esc(leadSid)}" aria-label="${esc(deskLeadName(d))}, the Desk lead"><span class="dkn-n">${deskKindIcon("desk")}<span class="dkn-nm">${esc(deskLeadName(d))}</span><span class="dkn-lp">Lead</span><span class="dkn-st">${dot === "" ? '<i class="dkn-d work" aria-hidden="true"></i>' : ""}${esc(dot === "ask" ? "asks you" : word.split(" · ")[0])}${on ? " ›" : ""}</span></span></button>`;
  }
  // A machine's busy count: the Desk's items at work there, of as many as it runs at once (Desk settings, #711).
  const items = d.items || [];
  const atOnce = (m) => (m ? (d.machines?.machines || []).find((x) => x.name === m || machShort(x.name) === machShort(m))?.at_once : d.lands?.set?.at_once) || 0;
  const busyOn = (m) => items.filter((i) => (i.state === "working" || deskFinishing(i)) && (sessionOf(deskHolderSid(i))?.machine || "") === m).length;
  const row = (s) => {
    const sid = s.session_id, [dot, line] = deskCSessWord(d, s), on = lit(sid);
    const team = (sessionOf(sid)?.team || []).filter((t) => deskMateLive(sid, t));
    const crew = team.map((t) => deskCMate(project, sid, t, o)).join("");
    return `<button class="dkn-ss${on ? " on" : ""}" data-desk-chat-of="${esc(sid)}" aria-label="${esc(deskHolderName(sid))}"><span class="dkn-n"><i class="dkn-d ${dot}" aria-hidden="true"></i><span class="dkn-nm">${esc(deskHolderName(sid))}</span>${on ? '<span class="dkn-chev" aria-hidden="true">›</span>' : ""}</span><span class="dkn-s">${line}</span></button>${crew}`;
  };
  const groups = machines.filter((m) => shown.some((s) => (s.machine || "") === m) || !m).map((m) => {
    const list = shown.filter((s) => (s.machine || "") === m);
    const of = atOnce(m), n = busyOn(m);
    const head = `<div class="dkn-mg">${DESK_C_MAC}<b>${esc(m ? machShort(m).toUpperCase() : "THIS MAC")}</b><span>· ${of ? `${n} of ${of} busy` : `${n} busy`}</span></div>`;
    return head + (list.map(row).join("") || `<div class="dkn-none">No sessions here yet.</div>`);
  }).join("");
  const count = [`${mine.length} session${mine.length === 1 ? "" : "s"}`, `${working} working`, machines.length > 1 ? `${machines.length} machines` : ""].filter(Boolean).join(" · ");
  return `<nav class="dkn-side" aria-label="Sessions"><div class="dkn-head"><div class="dkn-dn">${deskKindIcon("tray")}<b>${esc(deskName(d))}</b></div><div class="dkn-cnt">${esc(count)}</div></div>${lead}<div class="dkn-rule" aria-hidden="true"></div><div class="dkn-list" data-scroll-key="desk-n-sessions:${esc(project)}">${groups}</div><div class="dkn-foot"><button class="dkn-alls" data-desk2b-sessions aria-label="All ${mine.length + (leadS ? 1 : 0)} sessions">All<span class="dkn-w"> ${mine.length + (leadS ? 1 : 0)} sessions</span> ›</button></div></nav>`;
}

// ---------- Items: one white card, then Up next and Done today on the green ----------

/** An item's six steps as the table's cells: a dot on a line, done filled, the one it's at in the Board's waiting
 *  stripes (moving while a session works on it right now), how long each took under it. */
function deskCSteps(i) {
  const look = deskProgLook(i), at = deskCAt(i);
  return deskProgSteps(i).map(({ took }, k) => [k < at ? "done" : k === at ? "cur" : "todo", took]).map(([state, took], k) => `<span class="dkn-stp ${state}${state === "cur" && look === "work" ? " live" : ""}${k === 0 ? " first" : ""}"><i></i><span class="dkn-tm">${took || "·"}</span></span>`).join("");
}
/** The step item `i` is at on Cue's six (Picked up, Building, Tests, Commit, Landing, To check; -1 not started): the
 *  furthest it stamped, and no less than its state says (at work: building; landing; to check), as today's bar reads
 *  it (b2.js deskB2At). */
function deskCAt(i) {
  const at = deskProgAt(i);
  if (i.state === "to_check") return 5;
  if (deskFinishing(i)) return Math.max(at, 4);
  return i.state === "working" ? Math.max(at, 1) : at;
}
/** What's under its title: the machine it runs on, who has it, its kind. */
function deskCSub(i) {
  const kind = deskNum(i).slice(0, -`#${i.id}`.length).trim();
  const mach = deskMachTag(i) || (i.holder ? `<span class="mach"><span>This Mac</span></span>` : "");
  return [mach, i.holder ? esc(deskHolderOf(i)) : "", esc(kind)].filter(Boolean).join(" · ");
}
/** The Now cell: what it asks with Choose…, Ready to check with Check, else where it stands. */
function deskCNow(i) {
  if (i.state === "needs_you") return `<span class="dkn-nask">Asks you · </span><button class="btn small" data-desk-chk-item="${esc(i.id)}">Choose…</button>`;
  if (i.state === "to_check" && !deskInstallWait(i)) return `<span class="dkn-nw">Ready to check · </span><button class="btn small primary" data-desk-chk-item="${esc(i.id)}">Check</button>`;
  const [st, mono] = deskB2Status(i);
  return `<span class="dkn-nw${mono ? " mono" : ""}">${st}</span>`;
}
/** One item in the card: #id; its title with where it runs, who has it and its kind; its six steps; Now. Narrow, the
 *  steps are today's six short bars ("3/6 · Tests"). Lit while its page is open. */
function deskCRow(i) {
  // Lit while its page is open, or its holder's chat (that session's chat is the item's chat).
  const o = deskOpen, on = (o?.kind === "item" && deskIdIs(i, o.id)) || ((o?.kind === "chat" || o?.kind === "holder") && !!o.sid && deskHolderSid(i) === o.sid);
  return `<div class="dk-it dkn-row${on ? " on" : ""}${i.state === "needs_you" ? " ask" : ""}" role="button" tabindex="0" data-desk-item="${esc(i.id)}"><span class="dkn-id">#${esc(i.id)}</span><div class="dkn-t"><span class="dkn-tt">${esc(i.title)}</span><span class="dkn-sub">${deskCSub(i)}</span></div><div class="dkn-steps">${deskCSteps(i)}</div><div class="dkn-bar">${deskB2Track(i)}<span>${deskB2StepWord(i)}</span></div><div class="dkn-now">${deskCNow(i)}</div></div>`;
}
/** A row in Up next or Done today, on the green: #id and title (a grip on the ones you can drag), and a line under it. */
function deskCLow(i, line) {
  const on = deskOpen?.kind === "item" && deskIdIs(i, deskOpen.id);
  const mach = deskMachTag(i);
  return `<div class="dk-it dkn-nx${on ? " on" : ""}" role="button" tabindex="0" data-desk-item="${esc(i.id)}"><div class="dkn-a1"><span class="dkn-id">#${esc(i.id)}</span><span class="dkn-tx">${esc(i.title)}</span></div><div class="dkn-a2">${mach}${line ? `<span>${line}</span>` : ""}</div></div>`;
}
/** A done item's line: how long it took and when it landed. */
function deskCDoneLine(i) {
  const st = deskProgStamps(i), start = st.find(Boolean), end = deskMs(i.done_at || i.updated);
  return [start && end > start ? `took ${deskProgDur(end - start)}` : "", end ? `landed ${clockAt(end)}` : ""].filter(Boolean).join(" · ");
}
/** A list on the green with the Board's drawer head: the arrow at the left, its name and count; the whole line folds it
 *  to that line. */
function deskCDrawer(key, label, n, hint, rows, none) {
  const open = !deskCFolds.has(key);
  return `<section class="dkn-dr${open ? "" : " fold"}" data-dkn-drawer="${key}"><button class="dkn-dh" data-desk-c-fold="${key}" aria-expanded="${open}">${DESK_C_ARROW(open)}${label} <span class="dkn-dn">${n}</span>${hint && open ? `<span class="dkn-hint">· ${hint}</span>` : ""}</button>${open ? `<div class="dkn-dl">${rows || `<div class="dkn-dnone">${none}</div>`}</div>` : ""}</section>`;
}
/** Items: one line at its head (Items, Add an item…, Away, Pause; no tabs: every group is on screen); the white card
 *  with the step names over Waiting on you and Working; then Up next and Done today side by side on the green. */
function deskCList(project, taskApprovals) {
  const g = deskGroups(project, taskApprovals), d = g.d;
  const aw = deskAwayWaiting(d, taskApprovals);
  const waits = [...g.asks, ...g.check];
  const working = [...g.landing, ...g.install, ...g.working, ...g.resume, ...g.paused];
  const queued = new Set(g.queued), next = [...g.queued, ...g.held];
  const doneToday = deskB2DoneToday(d);
  const grp = (key, label, list, extra = "", n = list.length) => (list.length || extra ? `<div class="dkn-gh" data-desk2b-group="${key}">${label} <span>${n}</span></div>${list.map(deskCRow).join("")}${extra}` : "");
  const you = grp("you", "WAITING ON YOU", waits, aw.html || "", waits.length + (aw.n || 0));
  const work = grp("working", "WORKING", working);
  const kept = (d?.kept || []).length ? `<div class="dkn-gh" data-desk2b-group="kept">KEPT COPIES <span>${d.kept.length}</span></div>${d.kept.map(deskKeptRow).join("")}` : "";
  const calls = d?.away?.on ? deskAwayCalls(d) : [];
  const away = d?.away?.on ? deskB2Fold("away", "CALLS WHILE AWAY", calls.length, deskAwayCallsHtml(d)) : "";
  let extra = [];
  try { extra = addOn.deskHolders?.(project) || []; } catch (e) { console.error(e); }
  const tail = extra.map((h) => deskHolderHead({ ...h, addon: true, key: h.key || h.name }, deskOpen?.kind === "addon" && deskOpen.key === (h.key || h.name)) + (h.items || []).map(deskCRow).join("")).join("");
  const body = you + work + kept + away + tail;
  const steps = DESK_PROG_STEPS.map((s) => `<span>${esc(s)}</span>`).join("");
  const head = `<div class="dkn-th" aria-hidden="true"><span></span><span>Item</span><span class="dkn-ths">${steps}</span><span>Now</span></div>`;
  // Where it stands, in words only (deskB2Status is html, escaped already).
  const said = (i) => deskB2Status(i)[0].replace(/<(?!\/?span\b)[^>]*>/g, "");
  const qRow = deskQRow((i) => deskCLow(i, said(i))), lowRow = (i) => (queued.has(i) ? qRow(i) : deskCLow(i, said(i)));
  const up = deskCDrawer("next", "UP NEXT", next.length, next.length > 1 ? "drag to reorder" : "", next.map(lowRow).join(""), "Nothing up next.");
  const done = deskCDrawer("done", "DONE TODAY", doneToday.length, "", doneToday.map((i) => deskCLow(i, esc(deskCDoneLine(i)))).join(""), "Nothing done today.");
  return `<div class="dk-items dkn-items"><div class="dk-card dkn-card"><div class="dk2b-ih dkn-ih"><b>Items</b>${deskQuickAdd()}${deskAwaySwitch(d)}${deskPausedTag(d)}${deskPauseButton(d)}</div>${body ? head : ""}<div class="dk-scroll dkn-body" data-scroll-key="desk-items:${esc(project)}">${body || `<div class="dk-empty">Nothing waits on you and nothing is working.</div>`}</div></div><div class="dkn-low">${up}${done}</div></div>`;
}

// ---------- clicks ----------

deskClicks({
  deskCFold: (el) => {
    const k = el.dataset.deskCFold;
    if (!deskCFolds.delete(k)) deskCFolds.add(k);
    try { localStorage.setItem("cue.deskCFolds", JSON.stringify([...deskCFolds])); } catch { /* kept for this window only */ }
    renderMain();
  },
});
