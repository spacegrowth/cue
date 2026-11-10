// Cue UI app — clicks on the board and around it: menus that close on a click elsewhere, the Projects chip and the
// project dots, cards and their buttons, usage and the header chips, "N live", Review rows, parked sessions, the
// views and History. Each list is a run of ui/app/clicks.js's order (first match wins).

/** Whether the live drop-down had the press that began this click: it redraws every few seconds, so a click that
 *  began inside it can end on a fresh copy and reach the page as a click "outside". Only a press that began
 *  outside closes it. */
let downInLive = false;

const OVERLAY_CLOSERS = [
  // An add-on's open menu closes on a click anywhere else.
  (t) => { if (extPickOpen && !t.closest(".xp-menu, [data-ext-open]")) { extPickOpen = null; renderMain(); } return false; },
  // A picture shown full size: any click closes it, and does nothing else.
  () => { if (!lightbox) return false; lightbox = null; renderMain(); return true; },
];

/** The Projects chip: its list opened or closed, or the project it shows alone (empty: all of them). */
function pickProject(pj) {
  pjRenaming = null;
  if (pj.dataset.pj === "toggle") { pjOpen = !pjOpen; pjNewOpen = false; }
  else {
    const was = projFilter;
    setProjFilter(pj.dataset.project || ""); pjOpen = false;
    // The terminal isn't per project: picking one there takes you to its board.
    if (view === "terminal") view = "board";
    if (projFilter !== was) openTopWaiting();
  }
  renderMain();
}
/** ✎ on a project in the list: its name in a box, selected. */
function renameProject(renB) {
  pjRenaming = renB.dataset.pjren;
  draft(`pjname:${pjRenaming}`).text = (state.settings?.project?.names || {})[pjRenaming] || "";
  renderMain();
  const el = document.querySelector(".pj-name-in");
  el?.focus(); el?.select();
}
const PROJECT_CLICKS = [
  // A project's dot: its swatches open under it; a swatch sets the color for that folder everywhere.
  rule("[data-pcpick]", (sw) => { setProjColor(sw.dataset.pcpick, +sw.dataset.pcidx); colorFor = null; renderMain(); }),
  // Your own Color: the Mac's color picker.
  rule("[data-pcown]", (el) => pickOwnColor(el)),
  rule("[data-pcolor]", (pc) => { const { pcolor: p, pcwhere: w } = pc.dataset; colorFor = colorFor?.p === p && colorFor.w === w ? null : { p, w }; renderMain(); }),
  (t) => { if (colorFor && !t.closest(".pc-wrap")) { colorFor = null; renderMain(); } return false; },
  // Renaming a project in the list: its box takes clicks (not a pick); ✎ opens it.
  rule(".pj-name-in, .pj-new-in", () => {}),
  // × on a project you made: armed by the first click, removed by the second.
  rule("[data-pjdel]", (delB) => { const p = delB.dataset.pjdel; if (pjDelArm === p) { pjDelArm = null; removeProject(p); } else pjDelArm = p; renderMain(); }),
  (t) => { if (pjDelArm && t.closest(".pj-wrap")) pjDelArm = null; return false; },   // any other click in the list disarms it
  rule("[data-pj-new]", () => { pjNewOpen = true; pjNewNote = ""; draft("pjnew").text = ""; renderMain(); document.querySelector(".pj-new-in")?.focus(); }),
  rule("[data-pjren]", renameProject),
  rule("[data-pjpin]", (pinB) => { togglePin(pinB.dataset.pjpin); renderMain(); }),
  rule("[data-pj]", pickProject),
  (t) => { if (pjOpen && !t.closest(".pj-wrap")) { pjOpen = pjNewOpen = false; pjRenaming = pjDelArm = null; renderMain(); } return false; },
  rule("[data-mach]", (mach) => machAct(mach.dataset.mach, mach.dataset.name)),
];

const DRAWER_CLICKS = [
  rule("[data-fold]", (fd) => foldDrawer(fd.dataset.fold)),
  rule("[data-fold-newest]", () => foldNewest()),
];

/** ✎ on a session: its name in a box, selected. */
function renameSession(rn) {
  renaming = rn.dataset.rename;
  draft(`rename:${renaming}`).text = sessionOf(renaming)?.name || "";
  renderMain();
  const el = document.querySelector(".rename-in");
  el?.focus(); el?.select();
}
const CARD_CLICKS = [
  rule("[data-rename]", renameSession),
  rule("[data-mic]", (mic) => toggleDictation(mic.dataset.mic)),
  rule("[data-park]", (pb) => { const [sid, on] = pb.dataset.park.split(/:(?=[01]$)/); setParked(sid, on === "1"); }),
  rule("[data-forget]", (fg) => invoke("forget_session", { sessionId: fg.dataset.forget }).then(() => toast("Removed from Cue")).catch((e) => toast(`Couldn't: ${e}`))),
  rule("[data-hide-idle]", (hide) => hideIdle(hide.dataset.hideIdle)),
];

/** The usage meter: Send again to the sessions it paused, step through them oldest first, or its list. */
function usageClick(us) {
  const k = us.dataset.usage;
  if (k === "resend") return resend(limitedSessions().filter((s) => lifted(s.limit)).map((s) => s.session_id));
  if (k === "resend-one") return resend([us.dataset.sid]);
  if (k === "out" || k === "model") {
    const list = limitedSessions().filter((s) => (k === "out") === (s.limit.scope === "all")).sort((a, b) => a.since_ms - b.since_ms);
    const at = list.findIndex((s) => s.session_id === active?.sid);
    const nx = list[(at + 1) % list.length];
    return nx && setActive(null, nx.session_id);
  }
  usageOpen = !usageOpen;
  renderMain();
}
const USAGE_CLICKS = [
  rule("[data-usage]", usageClick),
  (t) => { if (usageOpen && !t.closest(".usage-pop")) { usageOpen = false; renderMain(); } return false; },
  rule("[data-chip-step]", (hs) => { const [kind, dir] = hs.dataset.chipStep.split(":"); openFromChip(kind, +dir); }),
  rule("[data-chip]", (hc) => openFromChip(hc.dataset.chip)),
];

const LIVE_CLICKS = [
  (t) => { if (nfFolderOpen && !t.closest(".nf-fwrap")) { closeFolders(); renderMain(); } return false; },
  rule("[data-lv]", (lv) => lvAct(lv.dataset.lv, lv.dataset)),
  // A button in the row (Allow / Deny in the starred list) does its own thing, not "open".
  rule("[data-sv]", (sv) => { if (liveOpen && sv.dataset.sv !== "close") liveOpen = false; svAct(sv.dataset.sv, sv.dataset.sid); }, (sv, t) => !t.closest("button[data-act], button[data-wake]")),
  (t) => { if (liveOpen && !downInLive && !t.closest(".lv-wrap")) { liveOpen = newOpen = false; renderMain(); } return false; },
];

/** A rename the agent didn't take: try it again, or let it be. */
function renameAgain(rn2) {
  const sid = rn2.dataset.sid;
  if (rn2.dataset.act === "rename-again") return submitRename(sid, true);
  const x = sessionOf(sid);
  if (x) delete x.rename_error;
  renderMain();
  return invoke("rename_dismiss", { sessionId: sid }).catch(() => {});
}
const START_CLICKS = [
  rule("[data-act=rename-again], [data-act=rename-dismiss]", renameAgain),
  rule("[data-act=continue-start]", (cs) => invoke("continue_start", { sessionId: cs.dataset.sid }).then(() => toast("Pressed Enter: it's starting")).catch((e) => toast(`Couldn't: ${e}. Answer it in its terminal.`))),
  rule("[data-act=trust]", (tr) => invoke("trust_folder", { sessionId: tr.dataset.sid }).then(() => toast("Trusted: it's starting")).catch((e) => toast(`Couldn't answer it: ${e}. Answer it in its terminal.`))),
  rule("[data-act=go-session]", (goS) => invoke("focus_session_id", { sessionId: goS.dataset.sid }).then((r) => toast(`Jumped to ${r}`)).catch((e) => toast(`Couldn't jump: ${e}`))),
];

// Parked: Resume (↺ on its row, or the button), Resume & send, Unpark, Park, its earlier messages, fold.
// ↺ on a row of one whose cache is gone and whose context is big opens it first, to say what resuming costs
// (once: with it open, ↺ resumes).
const PARKED_CLICKS = [
  rule("[data-wake]", (wk) => wk.closest(".active-pane") || active?.sid === wk.dataset.wake ? wakeParked(wk.dataset.wake) : insColdResume(wk.dataset.wake).then((cold) => (cold ? setActive(null, wk.dataset.wake) : wakeParked(wk.dataset.wake)))),
  rule("[data-wake-send]", (wsd) => wakeParked(wsd.dataset.wakeSend, draft(`s:${wsd.dataset.wakeSend}`).text.trim())),
  rule("[data-sv-fold]", (svf) => svFold(svf.dataset.svFold)),
  rule("[data-sv-all]", (sva) => showAllSvRest(sva)),
  rule("[data-shelf]", (shw) => pickShelf(shw.dataset.shelf)),
  rule("[data-park-closed]", (pkc) => parkClosed(pkc.dataset.parkClosed)),
  rule("[data-unpark]", (upk) => unparkSession(upk.dataset.unpark)),
  rule("[data-park-session]", (pks) => { moreFor = null; parkSession(pks.dataset.parkSession); }),
  rule("[data-delete-ask]", (dla) => { moreFor = null; deleteAsk = dla.dataset.deleteAsk; document.activeElement?.blur(); renderMain(); }),
  rule("[data-park-all]", (pka) => { parkedAll = parkedAll === pka.dataset.parkAll ? null : pka.dataset.parkAll; renderMain(); }),
];

/** A terminal's own question answered by its number; the card stays the one shown. */
function answerTerminal(tp) {
  const it = findItem(tp.dataset.id);
  if (it) active = { id: it.id, sid: it.session_id };
  return invoke("answer_terminal", { id: tp.dataset.id, n: +tp.dataset.tpick }).then((r) => toast(r)).catch((e) => toast(`Couldn't answer it: ${e}`));
}
const BOARD_CLICKS = [
  // The add-on's first: a button of its on a session's card is its own, not the card's.
  ...addOn.clicks,
  rule("[data-session]", (ss) => openSession(ss.dataset.session)),
  rule("[data-view]", (vb) => { view = vb.dataset.view; renderMain(); }),
  rule("[data-hfilter]", (hf) => { histFilter = hf.dataset.hfilter; histOpen = null; renderMain(); }),
  rule("[data-hgroup]", (hg) => { flipIn(histOpenGroups, hg.dataset.hgroup); renderMain(); }),
  rule("[data-hrow]", (hr) => { histOpen = histOpen === hr.dataset.hrow ? null : hr.dataset.hrow; renderMain(); }),
  rule("[data-detail]", (dt) => setActive(dt.dataset.detail, null, true)),
  rule("[data-msg]", (mo) => { flipIn(openMsgs, mo.dataset.msg); renderMain(); }),
  rule("[data-ask-open]", (va) => { flipIn(openAsks, va.dataset.askOpen); renderMain(); }),
  rule("[data-big]", (big) => setActive(big.dataset.big), (big, t) => !t.closest("button")),
  rule("[data-qstep]", (qs) => { const [id, n] = qs.dataset.qstep.split(/:(?=\d+$)/); qStep[id] = +n; renderMain(); }),
  rule("[data-tpick]", answerTerminal),
  rule("[data-pick]", (pk) => { const [qi, oi] = pk.dataset.pick.split(":").map(Number); const it = findItem(pk.dataset.id); return it && pick(it, qi, oi); }),
  rule("[data-always]", (al) => { const it = findItem(al.dataset.id); return it && respond(it, { behavior: "allow_always", permission: it.suggestions[+al.dataset.always] }); }),
  rule("[data-collapse]", (cbtn) => collapseSection(cbtn.dataset.collapse)),
  rule("[data-open-section]", (obtn) => openSection(obtn.dataset.openSection)),
  rule("[data-col-reset]", (crst) => resetCol(crst.dataset.colReset)),
];
