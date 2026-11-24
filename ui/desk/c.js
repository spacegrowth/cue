// Cue UI desk — the Desk's layout (#738, "C, reviewed" of the #726 mocks; the only one since #748): three columns on the
// green: at the left the Desk's sessions right on the green, grouped by machine, the lead on top with a thin line under
// it; in the middle the lead's chat (or what's open in its place), its panel only Now and Activity; at the
// right Items, one white card for what waits on you and what's working (a row per item with its progress as a strip,
// or a line along its foot (#751), the step it's at in the Board's waiting stripes), and under it Up next and Done today side by side on the green, each folding to its
// heading. One count everywhere: Waiting on you's heading is the Desk's own. Drawn by desk/view.js deskView;
// everything here sits under `.dk-c` (styles/desk-c.css). Loaded after desk/b2.js (index.html, mini.html).

/** Up next and Done today folded to their headings, by key ("next", "done"); kept across restarts. */
const deskCFolds = (() => { try { return new Set(JSON.parse(localStorage.getItem("cue.deskCFolds") || "[]")); } catch { return new Set(); } })();

const DESK_C_MAC = `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="5" width="16" height="11" rx="1.5"/><path d="M2 19h20"/></svg>`;
const DESK_C_CREW = `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="9" cy="8" r="3"/><path d="M3 20a6 6 0 0 1 12 0"/><circle cx="17" cy="9" r="2.4"/><path d="M15.5 14.5A5 5 0 0 1 21 19"/></svg>`;
/** Which of Up next and Parked is open ("next", "parked" or "" for neither): one at a time, Up next to begin with. */
const deskCLowOpen = () => { try { return localStorage.getItem("cue.deskCLow") ?? "next"; } catch { return "next"; } };
/** Up next or Parked folded or opened as the Board's drawers are (board.js foldDrawer): one open at a time, so a
 *  click moves the room from one to the other. Only these two move, in place, with their rows showing (the folded one
 *  keeps its rows in the page, hidden): measured as they'll end up, then each eased from its height to that, and the
 *  green under the card with them when it grows or shrinks (both folded it's their headings alone). Nothing is drawn
 *  afresh: redraws wait while they slide (`folding`, app/render.js), and on landing they're left as they are, the
 *  lists' cuts measured again; a redraw held meanwhile happens then (#838: the slide ended a frame early, at its
 *  natural heights, then the whole list was drawn again, and a redraw mid-slide jumped it to the end). */
let deskCLowSliding = 0;
function deskCLowFold(name) {
  const to = deskCLowOpen() === name ? "" : name;
  try { localStorage.setItem("cue.deskCLow", to); } catch { /* this window only */ }
  const secs = [...document.querySelectorAll("#app .dkn-lcol > .dkn-dr")], low = document.querySelector("#app .dkn-low");
  const set = (sec, open) => {
    sec.classList.toggle("fold", !open);
    const head = sec.querySelector(".dkn-dh");
    head.setAttribute("aria-expanded", String(open));
    head.firstElementChild.outerHTML = DESK_C_ARROW(open);
  };
  const land = () => { deskCFitKey = ""; deskCFitLists(); };
  if (!secs.length || !low) return renderMain();
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) { secs.forEach((sec) => set(sec, sec.dataset.dknDrawer === to)); return land(); }
  // A slide still going: on from where it has got to.
  const h = (el) => el.getBoundingClientRect().height;
  const from = secs.map(h), lowFrom = h(low);
  for (const el of [low, ...secs]) { el.getAnimations().forEach((a) => a.cancel()); el.style.flex = el.style.overflow = el.style.minHeight = el.style.height = ""; }
  secs.forEach((sec) => set(sec, sec.dataset.dknDrawer === to));
  const end = secs.map(h), lowEnd = h(low);
  // The one folding keeps its rows in sight as it closes.
  secs.forEach((sec) => { if (sec.dataset.dknDrawer !== to) sec.classList.remove("fold"); sec.style.flex = "none"; sec.style.overflow = "hidden"; });
  const lowMoves = Math.abs(lowEnd - lowFrom) > 0.5;
  if (lowMoves) { low.style.flex = "none"; low.style.minHeight = "0"; }
  const run = ++deskCLowSliding;
  folding = true;
  const done = () => {
    if (run !== deskCLowSliding) return;
    deskCLowSliding = 0;
    folding = false;
    // Landed, in the same frame: the folded one shut, every height its own again, the cuts measured again.
    secs.forEach((sec) => { if (sec.dataset.dknDrawer !== to) sec.classList.add("fold"); });
    for (const el of [low, ...secs]) { el.getAnimations().forEach((a) => a.cancel()); el.style.flex = el.style.overflow = el.style.minHeight = ""; }
    land();
    if (heldDraw) { heldDraw = false; renderMain(); }
  };
  // Each holds its last frame until done() puts it as it is: no frame at its natural height between.
  const opts = { duration: DESK_C_OPEN_MS, easing: "cubic-bezier(0.2, 0, 0, 1)", fill: "forwards" };
  const anims = secs.map((sec, k) => sec.animate([{ height: `${from[k]}px` }, { height: `${end[k]}px` }], opts));
  if (lowMoves) anims.push(low.animate([{ height: `${lowFrom}px` }, { height: `${lowEnd}px` }], opts));
  Promise.all(anims.map((a) => a.finished)).then(done, () => {});
  // A redraw that went ahead anyway (the page drawn afresh) drops the animations and their finish.
  setTimeout(done, DESK_C_OPEN_MS + 200);
}
/** How long a drawer's slide takes, as desk-c.css's dkn-open. */
const DESK_C_OPEN_MS = 260;
/** Drawers you just opened: key -> when, for their rows' slide in (DESK_C_OPEN_MS, as desk-c.css's dkn-open). */
const deskCOpened = new Map();
const DESK_C_ARROW = (open) => `<span class="dkn-fa" aria-hidden="true">${open ? "▾" : "▸"}</span>`;

// ---------- the sessions column, on the green ----------

/** How a session stands in the column: [dot, words]. dot: "ask" (asks you), "work", or "idle". */
function deskCSessWord(d, s) {
  const sid = s.session_id, it = deskSessItem(d, sid), [dot] = deskSessWithHelpers(s);
  const pre = it ? `#${it.id} · ` : "";
  if (dot === "ask" || it?.state === "needs_you") return ["ask", `${pre}<span class="dkn-ask">asks you</span>`];
  // Ready to check is the Items card's (on white, with its ✓); here it's a session that finished and sits still.
  if (it?.state === "to_check" && dot !== "") return ["idle", `${esc(pre)}idle${s.since_ms ? ` <span data-ago="${s.since_ms}">${ago(s.since_ms)}</span>` : ""}`];
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
/** The Desk's sessions at the left, on the green: its name and how many work on how many machines; the lead pinned on top, a thin line under
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
    lead = `<button class="dkn-lead${on ? " on" : ""}" data-desk-c-lead aria-label="${esc(deskLeadName(d))}, the Desk lead"><span class="dkn-n">${deskKindIcon("desk")}<span class="dkn-nm">${esc(deskLeadName(d))}</span><span class="dkn-lp">Lead</span><span class="dkn-st">${dot === "" ? '<i class="dkn-d work" aria-hidden="true"></i>' : ""}${esc(dot === "ask" ? "asks you" : word.split(" · ")[0])}${on ? " ›" : ""}</span></span></button>`;
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
    // The ones at work first, in their order; then the rest, the one that stopped last first.
    const list = shown.filter((s) => (s.machine || "") === m).sort((a, b) => busy(b) - busy(a) || (busy(a) ? 0 : (b.since_ms || 0) - (a.since_ms || 0)));
    const of = atOnce(m), n = busyOn(m);
    const head = `<div class="dkn-mg">${DESK_C_MAC}<b>${esc(m ? machShort(m).toUpperCase() : "THIS MAC")}</b><span>· ${of ? `${n} of ${of} busy` : `${n} busy`}</span></div>`;
    return head + (list.map(row).join("") || `<div class="dkn-none">No sessions here yet.</div>`);
  }).join("");
  // How many are at work, and on how many machines; how many sessions in all is on All N sessions at its foot.
  const count = [`${working} working`, machines.length > 1 ? `${machines.length} machines` : ""].filter(Boolean).join(" · ");
  return `<nav class="dkn-side" aria-label="Sessions"><div class="dkn-head"><div class="dkn-dn">${deskKindIcon("tray")}<b>${esc(deskName(d))}</b></div><div class="dkn-cnt">${esc(count)}</div></div>${lead}<div class="dkn-rule" aria-hidden="true"></div><div class="dkn-list" data-scroll-key="desk-n-sessions:${esc(project)}">${groups}</div><div class="dkn-foot"><button class="dkn-alls" data-desk2b-sessions aria-label="All ${mine.length + (leadS ? 1 : 0)} sessions">All<span class="dkn-w"> ${mine.length + (leadS ? 1 : 0)} sessions</span> ›</button></div></nav>`;
}

// ---------- Items: one white card, then Up next and Done today on the green ----------

/** An item's progress in Items is a strip in its own column (#751, K of #745's canvas): each step's piece as long as
 *  it took, its time on hover. (The Line view and its switch were dropped: Strip won.) */
/** How long each of item `i`'s six steps took, in ms (0: not reached, or no time kept); the one it's at runs till now.
 *  Picked up took till its first round's next step, as deskProgSteps reads it. */
function deskCStepMs(i) {
  const st = deskProgStamps(i), at = deskProgAt(i);
  return st.map((t, k) => {
    if (!t) return 0;
    const next = (k === 0 && i.backs?.length ? i.backs[0].steps || [] : st).slice(k + 1).find(Boolean);
    return Math.max(0, next ? next - t : k === at ? now() - t : 0);
  });
}
/** Item `i`'s six pieces: state ("done", "cur", "todo"), weight (the square root of its seconds: long steps longer, short
 *  ones still seen) and its time (HTML; the one it's at counts up, tickAges). */
function deskCPieces(i) {
  const at = deskCAt(i), ms = deskCStepMs(i), took = deskProgSteps(i);
  return ms.map((m, k) => ({ state: k < at ? "done" : k === at ? "cur" : "todo", w: Math.sqrt(m / 1000), took: took[k].took }));
}
/** What the strip reads out: "Step 3 of 6, Tests", or "Not started". */
const deskCLineSay = (i) => { const at = deskCAt(i); return at < 0 ? "Not started" : `Step ${at + 1} of 6, ${DESK_PROG_STEPS[at]}`; };
/** K: a strip of six pieces, each as long as its step took (done and current never under 18px, steps to come 8px
 *  stubs), the one it's at in the Board's waiting stripes (moving while it's worked on), each time under its piece. */
function deskCStrip(i) {
  const live = deskProgLook(i) === "work";
  const ps = deskCPieces(i).map(({ state, w, took }, k) => {
    const grow = state === "todo" ? "" : ` style="flex-grow:${(state === "cur" ? Math.max(w, 1) : w).toFixed(2)}"`;
    return `<span class="dkn-kp ${state}${state === "cur" && live ? " live" : ""}"${grow}><i></i><span class="dkn-kt">${state === "todo" ? "" : took || "·"}</span>${deskCStepPop(k, state, took)}</span>`;
  }).join("");
  return `<div class="dkn-ks" role="img" aria-label="${esc(deskCLineSay(i))}">${ps}</div>`;
}
/** A strip piece's card on hover: its step's name and how long it took ("so far" for the one it's at, "not yet" for
 *  one to come). Kept in the piece, shown over the page by the mouseover below (a list scrolls, and would cut it). */
const deskCStepPop = (k, state, took) => `<span class="dkn-kpop" hidden><b>${esc(DESK_PROG_STEPS[k])}</b> ${state === "todo" ? "not yet" : `${took || "under a second"}${state === "cur" ? " so far" : ""}`}</span>`;
if (typeof document !== "undefined") {
  // One small card over the page while the pointer is on a strip's piece: that step and its time (deskCStepPop).
  let pop = null, from = null;
  document.addEventListener("mouseover", (e) => {
    const ks = e.target.closest?.(".dk-c .dkn-kp");
    if (ks === from) return;
    from = ks;
    pop?.remove(); pop = null;
    const src = ks?.querySelector(".dkn-kpop");
    if (!src) return;
    pop = document.createElement("div");
    pop.className = "dkn-steps-pop";
    pop.innerHTML = src.innerHTML;
    document.body.appendChild(pop);
    const r = ks.getBoundingClientRect(), h = pop.offsetHeight, w = pop.offsetWidth;
    const top = r.bottom + 8 + h > innerHeight - 8 ? r.top - 8 - h : r.bottom + 8;
    pop.style.top = `${Math.max(8, top)}px`;
    pop.style.left = `${Math.max(8, Math.min(r.left + r.width / 2 - w / 2, innerWidth - 8 - w))}px`;
  });
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
  return [deskCNum(i), i.holder ? esc(deskHolderOf(i)) : "", deskCMach(i)].filter(Boolean).join(" · ");
}
/** Its kind as the Desk's small icon (bug, feature, task) before its number, in place of the word; the word is still read out. */
const deskCKind = (i) => (deskKindIcon(i) ? `<span class="dkn-k" role="img" aria-label="${esc(i.kind)}">${deskKindIcon(i)}</span>` : "");
/** Its kind's icon and number, at the head of its second line ("🐞 #586"): rows have no number column, so titles get the room. */
const deskCNum = (i) => `<span class="dkn-id">${deskCKind(i)}#${esc(i.id)}</span>`;
/** Where it runs, only when that isn't this Mac (most items run here, and the sessions column says This Mac once). */
const deskCMach = (i) => (i?.machine === "This Mac" ? "" : deskMachTag(i));
/** The Now cell: Asks you, Ready to check with the ✓ (Looks right, as today's rows), else where it stands. */
function deskCNow(i) {
  // What it asks opens with the row, as Check did: no Choose… button fighting the words for room.
  if (i.state === "needs_you") return `<span class="dkn-nask">Asks you</span>`;
  if (i.state === "to_check" && !deskInstallWait(i) && !deskUncommitted(i)) return deskLooksSoon.has(i.id) ? `<span class="dk-soon">Done · <button class="dk-undo-c" data-desk-act="looks_undo" data-id="${esc(i.id)}">Undo</button></span>` : `<span class="dkn-nw">Ready to check</span><button class="dk-ok" data-desk-act="looks_right" data-id="${esc(i.id)}" aria-label="Looks right"></button>`;
  const [st, mono] = deskB2Status(i);
  return `<span class="dkn-nw${mono ? " mono" : ""}">${st}</span>`;
}
/** One item in the card: its title with its kind, number, who has it and where it runs; then its strip and Now.
 *  Narrow, the strip goes under the title. Lit while its page is open. */
function deskCRow(i) {
  // Lit while its page is open, or its holder's chat (that session's chat is the item's chat).
  const o = deskOpen, on = (o?.kind === "item" && deskIdIs(i, o.id)) || ((o?.kind === "chat" || o?.kind === "holder") && !!o.sid && deskHolderSid(i) === o.sid);
  return `<div class="dk-it dkn-row${on ? " on" : ""}${i.state === "needs_you" ? " ask" : ""}${(i.state === "to_check" && deskLooksSoon.has(i.id)) || deskCStays(i) ? " soon" : ""}${deskLeaveStyle(i.id)}" role="button" tabindex="0" data-desk-item="${esc(i.id)}"><div class="dkn-t"><span class="dkn-tt">${esc(i.title)}</span><span class="dkn-sub">${deskCSub(i)}</span></div><div class="dkn-pl">${deskCStrip(i)}</div><div class="dkn-now">${deskCNow(i)}</div>${deskCRowMore(i)}</div>`;
}
/** A row's ⋯ (#839): its page's own menu (page.js deskMoreItems), without opening the item. Shown as you point at the
 *  row and on the open one (desk-c.css .dkn-rwrap); "" when nothing fits. */
const deskCRowMore = (i) => deskMore(i, deskMoreItems(i), "rmore");
/** A row in Up next or Done today, on the green: #id and title (a grip on the ones you can drag), and a line under it. */
/** Why an Up next item waits, in full plain words (html, escaped): what it waits for and what starts it. "" when it
 *  isn't waiting on anything Cue knows (then its status line says where it stands). */
function deskCWhy(i) {
  if (i.state === "held") return "Parked: press Start to begin it";
  if (i.state !== "queued" && i.state !== "new") return "";
  // Every slot where it would work taken (#798, that machine's at once in Desk settings): it starts as one finishes.
  if (i.slot_wait) return deskSlotWhy(i, true);
  if (i.holder || i.start_failed || deskCopyWaits(i) || (i.route_failed && !i.next_for)) return "";
  const nf = i.next_for ? esc(deskHolderOf({ holder: i.next_for })) : "";
  const then = nf ? `, then goes to ${nf}` : "";
  if (i.after) {
    const on = deskAfterName(i);
    // Which point lets it go, by its Desk's setting (#808, desk/lands.rs "waits"): landed, or your Looks right.
    const point = deskOf(i.project)?.lands?.set?.waits === "checked" ? "to be checked first" : "to land first";
    return on.startsWith("#") ? `Waits for ${esc(on)} ${point}${then}` : `Waits for ${esc(on)}${then}`;
  }
  // Nobody has it yet, so no machine of its own (#776): the ones it may start on, each full.
  if (i.machine_wait) return `Waits for room on ${esc(i.machine_wait)}: ${/ or /.test(i.machine_wait) ? "each" : "it"} already runs all the items it can`;
  if (nf) return `Next for ${nf} once it finishes what it's on${i.fresh_at ? `; a fresh session takes it at ${deskQHm(i.fresh_at)} if not` : ""}`;
  if (i.state === "new") return "Not given out yet: the Desk lead hasn't picked it up";
  return "Waits for a free session";
}
/** Waiting for a free slot (#798), in words: "Waiting for a free slot" and, `full`, the session it goes to and why. */
function deskSlotWhy(i, full) {
  if (!full) return "waiting for a free slot";
  const h = i.holder || i.next_for, to = h ? esc(deskHolderOf({ holder: h })) : "";
  return `Waiting for a free slot${to ? `, then goes to ${to}` : ""}: it starts by itself as one finishes`;
}
function deskCLow(i, line) {
  const on = deskOpen?.kind === "item" && deskIdIs(i, deskOpen.id);
  // Nobody has it yet: no machine, it's picked as the item starts (#776).
  const mach = i.holder ? deskCMach(i) : "";
  return `<div class="dk-it dkn-nx${on ? " on" : ""}" role="button" tabindex="0" data-desk-item="${esc(i.id)}"><div class="dkn-a1"><span class="dkn-tx">${esc(i.title)}</span></div><div class="dkn-a2">${deskCNum(i)}${mach}${line ? `<span>${line}</span>` : ""}</div>${deskCRowMore(i)}</div>`;
}
/** A done item's line: how long it took and when it landed. */
function deskCDoneLine(i) {
  const st = deskProgStamps(i), start = st.find(Boolean), end = deskMs(i.done_at || i.updated);
  return [start && end > start ? `took ${deskProgDur(end - start)}` : "", end ? `landed ${clockAt(end)}` : ""].filter(Boolean).join(" · ");
}
/** A list on the green with the Board's drawer head: the arrow at the left, its name and count; the whole line folds it
 *  to that line. */
/** `keep`: its rows stay in the page while it's folded, hidden, so it can slide open with them (deskCLowFold). */
function deskCDrawer(key, label, n, hint, rows, none, open = !deskCFolds.has(key), keep = false) {
  // Just opened: its rows slide in, already as far in as the time since (a redraw meanwhile doesn't start it again).
  const since = Date.now() - (deskCOpened.get(key) || 0), anim = open && since < DESK_C_OPEN_MS ? ` dkn-opening" style="animation-delay:-${since}ms` : "";
  return `<section class="dkn-dr${open ? "" : " fold"}" data-dkn-drawer="${key}"><button class="dkn-dh" data-desk-c-fold="${key}" aria-expanded="${open}">${DESK_C_ARROW(open)}${label} <span class="dkn-dn">${n}</span>${hint && (open || keep) ? `<span class="dkn-hint">· ${hint}</span>` : ""}</button>${open || keep ? `<div class="dkn-dl${anim}" data-dkn-list="${key}"${deskCFitStyle(key)}>${rows || `<div class="dkn-dnone">${none}</div>`}</div><button class="dkn-more" data-desk-c-all="${key}"${deskCFitBy.get(key) ? "" : " hidden"}>${deskCMoreBy.get(key) || ""}</button>` : ""}</section>`;
}
/** Items' card, Up next and Done today each scroll, and at rest end on a whole row: a list's height is cut to the
 *  last row that fits (what's left goes below it). Under each, "↓ N more" says how many rows are below its edge as
 *  you scroll (a click scrolls a page); at the end it shows nothing but keeps its room, so the list doesn't jump.
 *  The cuts are measured once (one layout) and kept while the window's size and the lists' rows stay the same. */
let deskCFitKey = "", deskCFits = [];
/** Each list's cut as last measured, by list ("items", "next", "done"), drawn with it so a redraw lays it out at that
 *  height at once (a list scrolled to its end is put back there, and stays at its end). */
const deskCFitBy = new Map();
const deskCFitStyle = (name) => (deskCFitBy.get(name) ? ` style="max-height:${deskCFitBy.get(name)}px"` : "");
/** How many of Working's rows show before it scrolls within (7 to 10 at once happens). */
const DESK_C_WORK_ROWS = 4;
/** Working's cut as last measured, drawn with it: a redraw lays it out at that height at once, as the card's own cut
 *  (deskCFitBy) is, else for a frame it showed every row with the card cut to the shorter Working. */
let deskCWorkCap = 0;
function deskCFitLists() {
  deskCStackHeadings();
  // Working first: cut to its first rows, so the card's own cut below counts it at that height.
  deskCCapWorking();
  const lists = [...document.querySelectorAll("#app .dkn-items [data-dkn-list]:not([data-dkn-cap])")];
  // A folded list (Up next or Parked, kept in the page hidden) counts as folded: opened, it and the card are measured again.
  const key = `${innerWidth}x${innerHeight}|${lists.map((el) => `${el.dataset.dknList}:${el.getClientRects().length ? el.querySelectorAll(".dk-it").length : "-"}`).join(",")}`;
  // Where each list was scrolled, and whether to its end (#791): a row folded shut and gone there must not pull the
  // rows down as the list is cut again.
  const tops = lists.map((el) => el.scrollTop), atEnd = lists.map((el) => el.scrollTop > 0 && el.scrollTop + el.clientHeight >= el.scrollHeight - 1);
  if (key !== deskCFitKey) {
    // "↓ N more" measured with its words in, as it will be drawn (empty, it's a line shorter, and the cut a row too low).
    for (const el of lists) { el.style.maxHeight = ""; el.nextElementSibling.hidden = false; el.nextElementSibling.textContent ||= "↓ 0 more"; }
    deskCFits = lists.map((el) => {
      if (el.scrollHeight <= el.clientHeight + 1) return 0;
      const top = el.getBoundingClientRect().top - el.scrollTop;
      let fit = 0;
      // Row by row, into each group's band (option A wraps a group in one): a cut never falls mid-row. Working's capped
      // list counts by its rows in sight, then as a whole: taller than the room, counted as one, it cut the card to
      // Working's heading alone, the rows under it out of reach behind the stacked headings.
      const rows = [...el.querySelectorAll(":scope > :not(.dkn-band), :scope > .dkn-band > .dkn-gh, :scope > .dkn-band > .dkn-bb > *")].flatMap((r) => {
        if (!r.matches(".dkn-wl")) return [r];
        const edge = r.getBoundingClientRect().bottom;
        return [...[...r.children].filter((c) => c.getBoundingClientRect().bottom <= edge + 1), r];
      });
      for (const r of rows) {
        const bottom = r.getBoundingClientRect().bottom - top;
        if (bottom > el.clientHeight) break;
        fit = bottom;
      }
      return Math.floor(fit);
    });
    // Scrolled to its end and a row fewer: its cut as it was. A taller one would show more above, the rows coming down.
    deskCFits = deskCFits.map((fit, k) => {
      const was = deskCFitBy.get(lists[k].dataset.dknList);
      return atEnd[k] && was && (fit > was || !fit) && lists[k].scrollHeight > was ? was : fit;
    });
    deskCFitKey = key;
    deskCFitBy.clear();
    lists.forEach((el, k) => deskCFitBy.set(el.dataset.dknList, deskCFits[k]));
  }
  deskCCutLists(lists, tops);
  deskCMore();
}
/** The bands' headings stack as they stick: each under the ones above it (desk-c.css .dkn-band > .dkn-gh). */
function deskCStackHeadings() {
  for (const list of document.querySelectorAll("#app .dkn-items [data-dkn-list=items]")) {
    let top = 0;
    for (const gh of list.querySelectorAll(":scope > .dkn-band > .dkn-gh")) { gh.style.top = `${top}px`; top += gh.offsetHeight; }
  }
}
/** Working cut to its first DESK_C_WORK_ROWS rows, kept in deskCWorkCap. */
function deskCCapWorking() {
  for (const el of document.querySelectorAll("#app .dkn-items [data-dkn-cap]")) {
    const r = el.children[DESK_C_WORK_ROWS - 1];
    if (!r) continue;
    deskCWorkCap = Math.floor(r.getBoundingClientRect().bottom - el.getBoundingClientRect().top + el.scrollTop);
    el.style.maxHeight = `${deskCWorkCap}px`;
  }
}
/** Each list cut to its measured height (deskCFits), "↓ N more" shown under the ones cut, and each put back where it
 *  was scrolled (`tops`), or on along the slide a "↓ N more" click started. */
function deskCCutLists(lists, tops) {
  lists.forEach((el, k) => {
    el.style.maxHeight = deskCFits[k] ? `${deskCFits[k]}px` : "";
    el.nextElementSibling.hidden = !deskCFits[k];
    // On its way after "↓ N more": a redraw meanwhile made it afresh where it had got to (its start, in the slide's
    // first frame), so it goes on from there to where the click sent it.
    const go = deskCGoing.get(el.dataset.dknList);
    if (go && Date.now() < go.until && Math.abs(el.scrollTop - go.top) > 1) el.scrollTo({ top: go.top, behavior: "smooth" });
    else deskCGoing.delete(el.dataset.dknList);
    if (!go && el.scrollTop !== tops[k]) el.scrollTop = tops[k];
  });
}
/** Where a list's "↓ N more" is sending it, by list, for as long as its slide takes (deskCAll, deskCFitLists). */
const deskCGoing = new Map(), DESK_C_GOING_MS = 1000;
/** "↓ N more" under each list that scrolls: the rows still below its edge. Kept by list, and drawn with it next time,
 *  so a redraw doesn't show it empty for a frame. */
const deskCMoreBy = new Map();
function deskCMore() {
  for (const el of document.querySelectorAll("#app .dkn-items [data-dkn-list]")) {
    const more = el.nextElementSibling;
    if (more.hidden) continue;
    const edge = el.getBoundingClientRect().bottom;
    const n = [...el.querySelectorAll(":scope > .dk-it, :scope > .dkn-band > .dkn-bb > .dk-it, :scope > .dkn-band > .dkn-bb > .dk-prows")].filter((r) => r.getBoundingClientRect().bottom > edge + 1).length;
    more.textContent = `↓ ${n} more`;
    deskCMoreBy.set(el.dataset.dknList, more.textContent);
    more.classList.toggle("end", !n);
  }
}
window.addEventListener("resize", () => { if (view === "desk") deskCFitLists(); });
document.addEventListener("scroll", (e) => { if (e.target?.dataset?.dknList) deskCMore(); }, true);
/** The bar over an opened item: its strip, then what you can do with it. Ready to check: Looks right and Send back,
 *  and what to try under them; writing why to send it back: that box in their place. Else its own buttons (Give to…,
 *  what it waits on and what clears it: Clear and land, Run the landing again…; its ⋯ menu: Start, Resume, Install
 *  now…; a plain working item has none, Stop is in the chat's own top bar), and a failed build's output. With no
 *  session (`bare`) also Drop this item, or for a dropped item's kept copy Pick up again and Remove. */
function deskCItemBar(i, bare = false) {
  const id = esc(i.id), check = i.state === "to_check" && !deskInstallWait(i), backing = deskBackFor === i.id, armed = deskDropArm === i.id;
  const drop = i.state !== "done" && i.state !== "dropped" ? `<button class="btn small dk-drop${armed ? " armed" : ""}" data-desk-act="drop" data-id="${id}">${armed ? "Drop it" : "Drop this item"}</button>` : "";
  // Done (just now, from Looks right, or before): how it went on the line under the strip, where what to try was, and
  // only Reopen beside the strip, so the bar keeps its height as it turns done and the chat under it stays put (#791).
  const done = i.state === "done" && !i.picked && !backing;
  const acts = backing ? "" : done ? (i.holder ? `<button class="btn small" data-desk-act="send_back" data-id="${id}">Reopen</button>` : "") : i.kept && bare ? `<button class="btn small primary" data-desk-act="pickup_kept" data-id="${id}">Pick up again</button><button class="btn small dk-drop${armed ? " armed" : ""}" data-desk-act="remove_kept" data-id="${id}">${armed ? "Remove it and its work" : "Remove"}</button>`
    : !check ? deskItemActs(i, bare) + (bare && i.state !== "held" ? drop : "") : deskLooksSoon.has(i.id) ? deskLooksSoonHtml(i.id)
    : `<button class="btn small primary" data-desk-act="looks_right" data-id="${id}">${deskUncommitted(i) ? "Commit to main" : "Looks right"}</button><button class="btn small" data-desk-act="send_back" data-id="${id}">Send back</button>`;
  // What to try: its session's check line (Try / Nothing to try), else its You'll see, else Send back to ask.
  const cl = check && !backing ? deskToCheckLine(i) : null;
  const see = done ? `<div class="dkn-ibsee">${deskCommitLine(i)}</div>` : cl ? `<div class="dkn-ibsee">${deskCheckHtml(cl)}</div>` : "";
  const back = backing ? `<div class="dkn-ibback">${deskBackHtml(i.id)}</div>` : "";
  // Queued for its session, which is still on something else: what it waits for and Start now, over that chat.
  const log = deskFinLog(i), ns = bare ? "" : deskNotStarted(i);
  // Options to pick from (#281, #445): each with what it is and Pick; and its mocks, a click away.
  const picks = backing ? "" : deskPickBtns(i, "dkn-ibopts"), mocks = deskCanvas(i) ? `<div class="dkn-ibmocks">${deskDesignsA(deskCanvas(i))}</div>` : "";
  const kept = i.kept ? `<div class="dkn-ibkept">Its work is kept in ${esc(i.kept.path)} on ${esc(i.kept.branch)}. Pick up again puts it back on the Desk with this copy, for ${esc(deskLeadName(deskOf(i.project)) || "the lead")} to give out.</div>` : "";
  return `<div class="dkn-ib${check ? " check" : ""}"><div class="dkn-ib2">${deskCStrip(i)}<div class="dkn-ibacts">${acts}</div></div>${see}${kept}${mocks}${picks}${back}${ns}${log ? `<div class="dkn-iblog">${log}</div>` : ""}</div>`;
}
/** An item opened: its session's chat (the same as clicking the session on the left), not a page of its own (#560's
 *  plan A, finished in #744: one item per session until it's done, so that chat is its story), its bar over the chat.
 *  null when no session has it (deskCItemBare instead). */
function deskCItemChat(i, d, foot) {
  const sid = deskHolderSid(i);
  // A dropped item's kept copy is off the Desk: its own view, with Pick up again and Remove.
  if (!sid || !sessionOf(sid)) return null;
  // A dropped item's kept copy (#608): the chat of the session that held it, which you can still write to; its bar
  // says where the work is kept, with Pick up again and Remove.
  return deskCSessChat(sid, null, d, foot, deskCItemBar(i, !!i.kept), i);
}
/** An item no session has (Up next, parked, a dropped one's kept copy, or one whose session has ended): back, its
 *  kind, number and title, where it stands, its bar, then what it was added with (its words, pictures, what you'll
 *  see) and, given out but not started, what it waits for; the box at the foot. */
function deskCItemBare(i, d, foot) {
  const [st] = deskB2Status(i), pics = thumbs(i.images), pts = deskDoneWhenPoints(i.see);
  const words = `<div class="dk-md">${deskMd(i.words || i.title)}</div>${pics ? `<div class="dk-tpics">${pics}</div>` : ""}`;
  const see = pts.length ? deskSec("You'll see", `<ul class="dkn-isee">${pts.map((p) => `<li>${linkify(esc(p))}</li>`).join("")}</ul>`) : "";
  // What you sent about it from this box, on its way to the lead (or that couldn't go): as a chat's outbox rows.
  const ls = deskLead(d) && !deskLeadEnded(d) ? deskLeadSession(d) : null;
  const sent = ls ? deskCItemPending(i, ls).map((o) => outboxRow(o, ls, o.key)).join("") : "";
  const head = `<div class="dkn-ihead"><h1 class="dkn-ih1">${deskCKind(i)}<span class="dkn-iid">#${esc(i.id)}</span> <span class="dkn-itt">${esc(i.title)}</span></h1><div class="dkn-smeta">${st}</div></div>`;
  return `<div class="dkn-sp" data-desk-page="item"><div class="dkn-sback">${deskBack("")}</div>${head}${deskCItemBar(i, true)}<div class="dkn-spm"><div class="dk-pane dkn-ibare" data-scroll-key="item:${esc(i.id)}">${i.state === "needs_you" ? deskChain(i) : ""}${deskNotStarted(i)}${deskSec("Couldn't start", !i.holder && i.start_failed ? `<p>Cue couldn't start a new session for it: ${esc(i.start_failed)}</p>` : "")}${deskSec("Couldn't give it out", i.route_failed && !i.holder ? `<p>${esc(i.route_failed)}</p>` : "")}${deskSec("Added with", words)}${see}${deskCItemActs(i, d)}${sent ? `<div class="dkn-isent">${sent}</div>` : ""}</div>${foot}</div></div>`;
}
/** An item no session has: its Activity (#789), what the Desk did with it so far, newest first; "" for none. */
function deskCItemActs(i, d) {
  const rows = deskItemActivity(d, i);
  return rows.length ? deskSec("Activity", `<div class="dk-moves dkn-iact">${rows.map((r) => deskActRowHtml(r, d)).join("")}</div>`) : "";
}
/** Each session's open tab on the new layout ("chat" | "files" | "activity"), by session: Chat unless you picked another. */
const deskCSessTabs = new Map();
/** A session's chat on the new layout (a session clicked on the left, or the item it's on): back, then the Board's own
 *  top bar for it (its name, state, chips, Commit, ⋯, btw, Stop: chat/pane.js activeHead), the item it's on, the item's
 *  bar when there is one, then Chat and Files (what it changed this turn, each opening its changes) as tabs, and the box.
 *  No panel: Stop and the rest are in the bar, the files in their tab. */
/** A crew member's page in the new layout (desk/thread.js deskCrewPane): back, its name with its state and Stop it, ⋯;
 *  the line saying whose crew and what it's on; then Chat (its steps), Files (what it changed) and Activity (its lead's
 *  Desk lines) as tabs; and the box to its lead. No panel. */
function deskCCrewPage({ lead, key, t, it, leadName, back, word, dot, acts, main, files, d, foot }) {
  const k = `crew:${lead}:${key}`, tab = ["files", "activity"].includes(deskCSessTabs.get(k)) ? deskCSessTabs.get(k) : "chat";
  const rows = dkActivityOf(lead);
  const b = (id, label) => `<button class="dkn-stab${tab === id ? " on" : ""}" role="tab" aria-selected="${tab === id}" data-desk-c-stab="${id}" data-sid="${esc(k)}">${label}</button>`;
  const tabs = `<div class="dkn-stabs" role="tablist">${b("chat", "Chat")}${b("files", "Files")}${b("activity", "Activity")}</div>`;
  const body = tab === "files" ? `<div class="dk-pane dkn-sfiles" data-scroll-key="cfiles:${esc(k)}">${files || `<div class="dk-quiet">Nothing changed yet.</div>`}</div>`
    : tab === "activity" ? `<div class="dk-pane dkn-sfiles" data-scroll-key="cact:${esc(k)}">${rows.length ? `<div class="dk-prows">${rows.map((r) => deskActPanelRow({ at: r.at, line: r }, d)).join("")}</div>` : `<div class="dk-quiet">Nothing yet.</div>`}</div>`
    : main;
  const head = `<div class="dkn-chead"><i class="dk-pdot ${dot}" aria-hidden="true"></i>${DESK_C_CREW}<b class="dkn-cname">${esc(t.name)}</b><span class="dkn-cword">${esc(word)}</span><span class="grow"></span>${acts}</div>`;
  const meta = `<div class="dkn-smeta">in <b>${esc(leadName)}</b>'s crew${it ? ` · on ${deskItemLink(it)} · ${esc(it.title)}` : t.what ? ` · on ${esc(t.what)}` : ""}</div>`;
  return `<div class="dkn-sp dkn-crew" data-desk-page="crew"><div class="dkn-sback">${back}</div>${head}${meta}${tabs}<div class="dkn-spm">${body}${foot}</div></div>`;
}
/** Under this turn's files in a session's Files tab: each item it worked on, newest first, with the files that item
 *  changed (its commit's, or so far). */
const deskCSessTasks = (d, sid) => (d.items || []).filter((i) => i.holder && i.holder.kind !== "crew" && i.holder.sid === sid).sort((a, b) => (b.updated ?? 0) - (a.updated ?? 0));
/** The files item `i` says it's on, by path (its commit's while that waits). */
const deskCFilesOn = (i) => (i.commit_wait?.files || i.files || []).map((f) => (Array.isArray(f) ? f[1] : f));
/** Item `i`'s changed files as rows, each opening its diff over the Desk (desk.js deskShowFile); till they're read,
 *  the files it says it's on. "" when it has none. */
function deskCItemFiles(i) {
  const on = deskCFilesOn(i);
  if (!on.length && i.state !== "working") return "";
  const files = deskDiffRead(i);
  if (!files?.length) return on.map((p) => `<span class="dk-fc"><span class="dk-fc-p">${esc(p)}</span></span>`).join("");
  return files.map((f, n) => `<button class="dk-fc" data-desk-diff-file="${esc(i.id)}" data-n="${n}"><span class="dk-fc-p">${esc(f.path)}</span>${f.isNew ? `<span class="dk-fc-new">new</span>` : ""}<span class="dv-n dv-add">+${f.add}</span><span class="dv-n dv-del">−${f.del}</span></button>`).join("");
}
function deskCSessTaskFiles(d, sid) {
  return deskCSessTasks(d, sid).map((i) => { const f = deskCItemFiles(i); return f ? deskPsec(`${deskNum(i)} · ${esc(i.title)}`, `<div class="dk-fcs">${f}</div>`, "dk-pfiles") : ""; }).join("");
}
/** What you sent from an opened item's box that its session's chat doesn't show yet (deskPend), as the chat's own
 *  outbox rows under the item's head: Sending till it's there, a send-back said so, red with Send again if it couldn't
 *  go. */
function deskCItemPending(i, s, d = deskOf(i.project)) {
  const thread = s.thread || [], said = (p) => p.text.trim();
  // Gone by the hub's own record (#525): a step of yours with the same words that it says went.
  const steps = [...(d?.older || []), ...(d?.decisions || [])].filter((c) => c.item_id === i.id && c.by === "You" && !c.call);
  const after = (c, p) => Math.max(deskMs(c.at), c.delivery?.at_ms || 0) >= p.at - 1000;
  // A note to the lead is kept with no delivery: in, it has gone. One to a holder has gone once the hub says so.
  const went = (p) => p.failed == null && said(p) && steps.some((c) => String(c.text || "").trim() === said(p) && after(c, p) && (!c.delivery || !["", "sending"].includes(c.delivery.went || "")));
  const sent = (p) => went(p) || p.failed == null && ([s.held, s.queued].some((w) => said(p) && String(w?.text || "").includes(said(p)))
    || outbox.some((o) => o.sid === s.session_id && o.text === p.text)
    || thread.some((e) => e.role === "you" && e.at_ms >= p.at - 60000 && (said(p) ? String(e.text || "").includes(said(p)) : true)));
  const head = deskAboutBtn(i.id, i, i.title);
  // What the hub says didn't reach it (#437: Cue stopped, the session was gone): red with why and Send again, kept
  // across restarts as the hub keeps it; a copy of it waiting here is shown by it.
  const lost = steps.filter((c) => (c.delivery?.went || "").startsWith("failed"));
  const lostOf = (p) => lost.some((c) => String(c.text || "").trim() === said(p) && after(c, p));
  const failedRows = lost.map((c) => ({ text: c.text, at: deskMs(c.at), key: `lost:${c.id}`, head, m: { st: "failed", why: esc(c.delivery.went.replace(/^failed:?\s*/, "") || "it didn't go"), to: i.holder?.name, item: i.id,
    acts: sendAgainBtn(`data-desk-act="msg_retry" data-id="${esc(i.id)}" data-step="${esc(c.id)}"`) } }));
  return failedRows.concat((deskPending.get(i.id) || []).filter((p) => !sent(p) && !(p.failed == null && lostOf(p))).map((p) => {
    const failed = p.failed != null;
    const acts = failed ? sendAgainBtn(`data-desk-act="msg_retry" data-id="${esc(i.id)}" data-at="${esc(String(p.at))}"`) : "";
    const picsHtml = !p.images?.length && p.pics ? `<div class="dk-tpn">${p.pics === 1 ? "1 picture" : `${p.pics} pictures`}</div>` : "";
    const m = { ...(failed ? { st: "failed", why: esc(p.failed), retry: p.at } : i.state === "queued" && i.tree_wait ? { st: "sending", note: "Waiting: sent back, it starts when a working folder is free" } : { st: "sending" }), to: p.why === "lead" ? deskLeadName(d) : i.holder?.name, item: i.id, acts, extra: p.why === "back" ? ' · <span class="ml-back">sent back</span>' : "" };
    return { text: p.text, images: p.images, picsHtml, at: p.at, key: `p:${p.at}`, head, m };
  }));
}
function deskCSessChat(sid, from, d, foot, bar = "", item = null) {
  const s = sessionOf(sid);
  if (!s) return null;
  const card = (state.items || []).find((x) => x.session_id === sid) || null;
  // The Board's own top bar, without its hover words (none on the Desk).
  const head = activeHead(paneArgs({ it: card, s })).replace(/ title="[^"]*"/g, "");
  const on = item || deskSessItems(d, sid)[0];
  const meta = on ? `<div class="dkn-smeta">on ${deskItemLink(on)} · ${esc(on.title)}</div>` : "";
  // This turn's files and each of its items' (its Files tab lists them all).
  const n = (deskTurnDiff(sid)?.files || s.changes?.files || []).length + deskCSessTasks(d, sid).reduce((k, i) => k + (deskDiffs.get(deskDiffId(i))?.files || deskCFilesOn(i)).length, 0);
  const tab = ["files", "activity"].includes(deskCSessTabs.get(sid)) ? deskCSessTabs.get(sid) : "chat";
  const t = (k, label) => `<button class="dkn-stab${tab === k ? " on" : ""}" role="tab" aria-selected="${tab === k}" data-desk-c-stab="${k}" data-sid="${esc(sid)}">${label}</button>`;
  // Activity (#789): the Desk's moves on the item open here (or, a session opened on its own, those in its chat and
  // Cue's messages to it), which its chat no longer shows.
  const acts = tab === "activity" ? (item ? deskItemActivity(d, item) : deskActivity(d).filter((r) => r.line?.sid === sid || r.x?.sid === sid)) : null;
  const tabs = `<div class="dkn-stabs" role="tablist">${t("chat", "Chat")}${t("files", `Files <span class="n">${n}</span>`)}${t("activity", "Activity")}</div>`;
  let main;
  if (tab === "activity") main = `<div class="dk-pane dk-moves dkn-sact" data-scroll-key="sact:${esc(sid)}:${esc(item?.id || "")}">${acts.length ? acts.map((r) => deskActRowHtml(r, d)).join("") : `<div class="dk-quiet">Nothing yet.</div>`}</div>`;
  else if (tab === "files") main = `<div class="dk-pane dkn-sfiles" data-scroll-key="files:${esc(sid)}">${deskSessFiles(sid) || deskPsec("Changed this turn", `<div class="dk-quiet">Nothing yet.</div>`, "dk-pfiles")}${deskCSessTaskFiles(d, sid)}</div>`;
  else {
    const lead = (state.desk?.projects || []).some((x) => deskLead(x)?.sid === sid);
    const pending = item ? deskCItemPending(item, s) : [];
    // Not started on the item yet (its card is in the bar, #626): its holder's own line only when it really asks you.
    const notYet = item && deskNotStarted(item), asks = ["waiting", "deciding"].includes(s.state) && waitingFor(null, s) !== "your reply";
    const rows = deskChatRows(s, lead || pending.length ? { ...(lead ? { you: deskLeadNote } : {}), ...(pending.length ? { pending } : {}) } : undefined), where = !notYet || asks ? statusLine(null, s) : "";
    const empty = !rows && !where ? `<div class="dk-empty" data-key="empty">Nothing said yet.</div>` : "";
    // By the way's panel over the chat, as on the Board (its button is in the head above, #832); the status line under
    // it as on the Board too: a hand-off on its way and its background agents, not just where it is.
    main = `${btwPanel(sid)}<div class="dk-chat" data-desk-chat="${esc(sid)}">${empty}${rows}${where ? keyed(where, "where") : ""}</div>`;
  }
  // An item's page (its holder's chat under its bar) goes back to the list as an item without a holder does; a session
  // opened from the left has no back (deskChatBack).
  return `<div class="dkn-sp" data-desk-page="session"><div class="dkn-sback">${item ? deskBack("") : deskChatBack(from, d)}</div>${head}${meta}${bar}${tabs}<div class="dkn-spm">${main}${foot}</div></div>`;
}
/** Items you just sent back, by id: what was open on the Desk then (commit.js deskLooksPlace). Their rows stay where
 *  they were in Waiting on you till you open something else (#791), rather than jump up to Working under you. */
const deskCStay = new Map();
/** Whether item `i` is in Waiting on you only because you just sent it back. */
const deskCStays = (i) => deskCStay.has(i.id) && i.state !== "to_check" && i.state !== "needs_you";
/** Waiting on you's order as it was last drawn. */
let deskCWaitOrder = [];
/** Rows you just said Looks right to stay where they were while "Done · Undo" shows and they fold shut, and rows you
 *  sent back till you open something else (#791): that moment moves them from asking to ready to check (list.js
 *  deskGroups), or out to Working, which would otherwise send them to the list's end or out of it under you. */
function deskCKeepPlace(list) {
  const stay = (i) => deskLooksSoon.has(i.id) || deskCStay.has(i.id);
  const out = list.filter((i) => !stay(i));
  list.filter(stay).map((i) => [deskCWaitOrder.indexOf(i.id), i]).sort((a, b) => a[0] - b[0])
    .forEach(([at, i]) => out.splice(at < 0 ? out.length : Math.min(at, out.length), 0, i));
  deskCWaitOrder = out.map((i) => i.id);
  return out;
}
/** The heading of Desk `d`'s group of items on main waiting for its update after landing (#824), from its own step
 *  names: Cue's "Waiting to install"; a single step X, "Waiting for X"; "" when it has no such steps (no group). */
function deskCInstallHead(d) {
  const n = d?.lands?.set?.after_names || {}, name = n.installing || Object.values(n).at(-1);
  if (!name) return "";
  return /^install$/i.test(name) ? "WAITING TO INSTALL" : `WAITING FOR ${esc(name.toUpperCase())}`;
}
/** Items: one line at its head (Items, Add an item…, Away, Pause; no tabs: every group is on screen); the white card
 *  with the step names over Waiting on you and Working; then Up next and Done today side by side on the green. */
function deskCList(project, taskApprovals) {
  const g = deskGroups(project, taskApprovals), d = g.d;
  const aw = deskAwayWaiting(d, taskApprovals);
  // Looks right sent and done: its shut row goes (commit.js deskLooksLetGo). Sent back: kept till you open something else.
  deskLooksLetGo(d?.items || []);
  for (const [id, at] of deskCStay) if (at !== deskLooksPlace()) deskCStay.delete(id);
  const inWait = new Set([...g.asks, ...g.check].map((i) => i.id));
  const held = (d?.items || []).filter((i) => !inWait.has(i.id) && deskCStays(i)), heldIds = new Set(held.map((i) => i.id));
  const waits = deskCKeepPlace([...g.asks, ...g.check, ...held]);
  // On main and waiting for the Desk's update after landing (#824): a group of its own under Waiting on you, named by
  // that Desk's own steps; a Desk with none keeps them with Working.
  const instHead = deskCInstallHead(d), inst = instHead ? g.install.filter((i) => !heldIds.has(i.id)) : [];
  const working = [...g.landing, ...(instHead ? [] : g.install), ...g.working, ...g.resume, ...g.paused].filter((i) => !heldIds.has(i.id));
  const queued = new Set(g.queued), next = [...g.queued, ...g.held];
  const doneToday = deskB2DoneToday(d).filter((i) => !heldIds.has(i.id));
  // Each group folds to its heading with a click on it, as Up next and Done today do (remembered with them).
  // Option A ("Items: working, waiting, asking", picked 23 Nov): each kind in its own band, the heading with a dot,
  // its count and a plain line saying what it means for you; asking you on the soft red, Working on white, the rest on
  // the card's grey.
  const GRP_SAY = { blocked: "nothing moves till you answer", working: "at work · nothing for you", you: "done · ready for you to check", kept: "copies of dropped work", install: "on main · nothing for you", away: "what the lead decided while you were away" };
  const grp = (key, label, list, extra = "", n = list.length, first = "") => {
    if (!list.length && !extra && !first) return "";
    const open = !deskCFolds.has(key);
    const rows = first + list.map(deskCRow).join("") + extra;
    // Working holds at most DESK_C_WORK_ROWS rows and scrolls within, with its own "↓ N more", so Waiting on you stays in sight.
    const body = key === "working" && list.length > DESK_C_WORK_ROWS ? `<div class="dkn-wl" data-dkn-list="working" data-dkn-cap${deskCWorkCap ? ` style="max-height:${deskCWorkCap}px"` : ""}>${rows}</div><button class="dkn-more dkn-wmore" data-desk-c-all="working"></button>` : rows;
    const say = GRP_SAY[key] ? `<em class="dkn-gsay">${GRP_SAY[key]}</em>` : "";
    return `<section class="dkn-band dkn-b-${key}" data-dkn-band="${key}"><button class="dkn-gh" data-desk2b-group="${key}" data-desk-c-fold="${key}" aria-expanded="${open}"><i class="dkn-gdot" aria-hidden="true"></i>${label} <span>${n}</span>${say}</button>${open ? `<div class="dkn-bb">${body}</div>` : ""}</section>`;
  };
  // The lead's own ask waits on you too, as the header and the strip count it (deskB2WaitN): one count.
  const ask = d && !deskLeadEnded(d) ? deskLeadAsk(d) : null;
  const leadAsk = ask ? `<div class="dk-it dkn-row ask" role="button" tabindex="0" data-desk-c-lead><div class="dkn-t"><span class="dkn-tt">${esc(deskLeadName(d))}</span><span class="dkn-sub">The Desk lead</span></div><div class="dkn-pl"></div><div class="dkn-now"><span class="dkn-nask">${esc(deskOneLine(summary(ask) || "Asks you something"))}</span></div></div>` : "";
  // What's blocked on you goes first, over Working: nothing moves till you answer. Steps held for your OK, oldest first
  // (#749), with the lead's own one-way ask among them; then the lead asking you, then asks while you were away.
  const hf = deskHeldFirst(aw, waits, deskCRow, ask?.one_way && leadAsk ? [[ask.created_ms || 0, leadAsk]] : []);
  const blockedN = (aw.n || 0) + (ask ? 1 : 0) + (waits.length - hf.rest.length);
  const blocked = grp("blocked", "BLOCKED ON YOU", [], (ask?.one_way ? "" : leadAsk) + (aw.more || ""), blockedN, hf.html);
  const you = grp("you", "WAITING ON YOU", hf.rest, "", hf.rest.filter((i) => !heldIds.has(i.id)).length);
  const work = grp("working", "WORKING", working);
  const install = instHead ? grp("install", instHead, inst) : "";
  // Kept copies and Calls while away are bands too: their headings stick and stack with the rest.
  const kept = (d?.kept || []).length ? grp("kept", "KEPT COPIES", [], d.kept.map(deskKeptRow).join(""), d.kept.length) : "";
  const calls = d?.away?.on ? deskAwayCalls(d) : [];
  const away = d?.away?.on && calls.length ? grp("away", "CALLS WHILE AWAY", [], deskAwayCallsHtml(d), calls.length) : "";
  let extra = [];
  try { extra = addOn.deskHolders?.(project) || []; } catch (e) { console.error(e); }
  const tail = extra.map((h) => deskHolderHead({ ...h, addon: true, key: h.key || h.name }, deskOpen?.kind === "addon" && deskOpen.key === (h.key || h.name)) + (h.items || []).map(deskCRow).join("")).join("");
  // Working next: it's what's live, and it stays short (the Desk runs only so many at once); Waiting on you under it.
  const body = blocked + work + you + install + kept + away + tail;
  const head = `<div class="dkn-th" aria-hidden="true"><span>Item</span><span>Progress</span><span>Now</span></div>`;
  // Where it stands, in words only (deskB2Status is html, escaped already).
  const said = (i) => deskB2Status(i)[0].replace(/<(?!\/?span\b)[^>]*>/g, "");
  const why = (i) => deskCWhy(i) || said(i);
  const qRow = deskQRow((i) => deskCLow(i, why(i))), lowRow = (i) => (queued.has(i) ? qRow(i) : deskCLow(i, why(i)));
  // Up next and Parked share the left column, one open at a time, as the Board's Starred and Recently answered do:
  // opening one folds the other (deskCLowFold slides it). Parked stays folded till you open it.
  const nq = g.queued, up = deskCDrawer("next", "UP NEXT", nq.length, nq.length > 1 ? "drag to reorder" : "", nq.map(lowRow).join(""), "Nothing up next.", deskCLowOpen() === "next", true);
  const parked = g.held.length ? deskCDrawer("parked", "PARKED", g.held.length, "press Start to begin one", g.held.map(lowRow).join(""), "", deskCLowOpen() === "parked", true) : "";
  const done = deskCDrawer("done", "DONE TODAY", doneToday.length, "", doneToday.map((i) => deskCLow(i, esc(deskCDoneLine(i)))).join(""), "Nothing done today.");
  requestAnimationFrame(deskCFitLists);
  return `<div class="dk-items dkn-items"><div class="dk-card dkn-card dkn-strip"><div class="dk2b-ih dkn-ih"><b>Items</b>${deskQuickAdd()}${deskAwaySwitch(d)}${deskPausedTag(d)}${deskPauseButton(d)}</div>${body ? head : ""}<div class="dk-scroll dkn-body" data-scroll-key="desk-items:${esc(project)}" data-dkn-list="items"${deskCFitStyle("items")}>${body || `<div class="dk-empty">Nothing waits on you and nothing is working.</div>`}</div><button class="dkn-more" data-desk-c-all="items"${deskCFitBy.get("items") ? "" : " hidden"}>${deskCMoreBy.get("items") || ""}</button></div><div class="dkn-low"><div class="dkn-lcol">${up}${parked}</div>${done}</div></div>`;
}

// ---------- clicks ----------

deskClicks({
  // The lead (its block in the sessions column, its ask in Items, the header's count): its chat, whatever was open,
  // All sessions too.
  deskCLead: () => { deskPick = null; deskTab = "lead"; readerView = null; deskShow(null); },
  deskCStab: (el) => { deskCSessTabs.set(el.dataset.sid, el.dataset.deskCStab); renderMain(); },
  deskCAll: (el) => {
    const list = el.previousElementSibling;
    if (!list) return;
    const top = Math.min(list.scrollTop + list.clientHeight, list.scrollHeight - list.clientHeight);
    deskCGoing.set(list.dataset.dknList, { top, until: Date.now() + DESK_C_GOING_MS });
    list.scrollTo({ top, behavior: "smooth" });
  },
  deskCFold: (el) => {
    const k = el.dataset.deskCFold;
    if (k === "next" || k === "parked") return deskCLowFold(k);
    if (!deskCFolds.delete(k)) deskCFolds.add(k);
    else deskCOpened.set(k, Date.now());
    try { localStorage.setItem("cue.deskCFolds", JSON.stringify([...deskCFolds])); } catch { /* kept for this window only */ }
    renderMain();
  },
});
