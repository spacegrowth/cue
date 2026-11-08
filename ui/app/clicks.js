// Cue UI app — a click anywhere in the window: the rules in their order (the first that takes it ends the walk),
// then the buttons that say what they do by data-act alone.

/** data-act buttons with nothing more specific above: by name, each with its element. */
const ACTS = {
  next: () => goNext(),
  "set-tab": (el) => { setTab = el.dataset.tab; try { localStorage.setItem("cue.setTab", setTab); } catch {} const b = document.querySelector(".sheet-body"); if (b) b.scrollTop = 0; renderMain(); },
  "test-notify": () => invoke("test_notification").then(toast).catch((e) => toast(`Couldn't send: ${e}`)),
  "open-history": () => { view = "history"; sheet = null; renderMain(); },
  "shell-new": () => openShell(""),
  "open-search": () => openFind(),
  shells: () => toggleShells(),
  "load-older": (el) => loadOlder(el.dataset.sid),
  "load-transcript": (el) => loadOlder(el.dataset.sid, true),
  "open-settings": () => { sheet = sheet === "settings" ? null : "settings"; extSections = null; renderMain(); },
  // Buttons that name a session, not a card (no data-id): the "!" cards, Send again, Move to Cue.
  // Move to Cue / Move to hosted / Move to terminal: from a corner or from ⋯ (which closes).
  "move-cue": (el) => { moreFor = handoffArm = null; return el.dataset.why ? toast(`Move to Cue: ${el.dataset.why}`) : moveToCue(el.dataset.sid); },
  "move-term": (el) => { moreFor = handoffArm = null; renderMain(); return invoke("move_to_terminal", { sessionId: el.dataset.sid }).then(toast).catch((e) => toast(`Couldn't move it: ${e}`)); },
  "move-dismiss": () => { moving = null; renderMain(); },
  "delete-yes": () => deleteAnswer(true),
  "delete-no": () => deleteAnswer(false),
  resend: (el) => invoke("send_to_session", { sessionId: el.dataset.sid, text: el.dataset.resend, images: [], now: false }).then((r) => toast(`Sent ${r}`)).catch((x) => toast(`Couldn't send: ${x}`)),
  "ran-again": (el) => ranAct(el, (who, r) => runLine(who, r.line, r.cwd, r.machine)),
  // The line and what it printed, as the card shows them.
  "ran-copy": (el) => ranAct(el, (who, r) => invoke("copy_text", { text: r.output ? `${r.line}\n${r.output}` : r.line }).then(() => toast("Copied"), () => toast("Couldn't copy"))),
  "ran-send": (el) => ranAct(el, (who, r) => invoke("send_to_session", { sessionId: who, text: ranAsMessage(r), images: [], now: false }).then((x) => toast(`Sent ${x}`)).catch((x) => toast(`Couldn't send: ${x}`))),
  "ran-term": (el) => ranAct(el, (who, r) => invoke("send_to_session", { sessionId: who, text: `! ${r.line}`, images: [], now: false }).then(() => toast("Typed into its terminal")).catch((x) => toast(`Couldn't: ${x}`))),
};
/** A "!" card's button: `run(session, entry)` for the line it shows, if Cue still has it. */
function ranAct(el, run) {
  const who = el.dataset.who, r = (ranLog.get(who) || [])[+el.dataset.n];
  return r && run(who, r);
}

/** data-act buttons on a card (data-id): what they do to its item. */
const ITEM_ACTS = {
  "form-send": (it) => formSend(it),
  "submit-answers": (it) => { const a = answersFor(it); return a && respond(it, { behavior: "allow", answers: a }); },
  back: (it) => invoke("send_to_back", { id: it.id }).catch((e) => toast(`Couldn't move it to the bottom: ${e}`)),
  allow: (it) => allow(it),
  deny: (it) => deny(it),
  send: (it) => submitText(it),
  commit: (it) => invoke("reply", { id: it.id, text: "Commit", images: [] }).then((r) => toast(`“Commit” ${r}`)).catch((e) => toast(`Couldn't send: ${e}`)),
  continue: (it) => invoke("reply", { id: it.id, text: "continue", images: [] }).then((r) => toast(`“continue” ${r}`)).catch((e) => toast(`Couldn't send: ${e}`)),
  go: (it) => goTo(it),
  dismiss: (it) => { cleared.add(it.id); return invoke("dismiss", { id: it.id }); },
  menu: (it) => { menuFor = menuFor === it.id ? null : it.id; renderMain(); },
  redirect: (it) => { redirectFor = it.id; renderMain(); document.querySelector(`[data-text="${CSS.escape(it.id)}"]`)?.focus(); },
};

/** The last rule: a data-act button, by its name (an insight's, one of ACTS, or one of its card's ITEM_ACTS).
 *  Pressing a pending card's button (but Dismiss and Send to back) makes that card the one shown. */
function clickAct(t) {
  const el = t.closest("[data-act]"), act = el?.dataset.act;
  if (act?.startsWith("ins-")) { insAct(act, el.dataset); return true; }
  if (act && Object.hasOwn(ACTS, act)) { ACTS[act](el); return true; }
  const it = el?.dataset.id ? findItem(el.dataset.id) : null;
  if (!act || !it) return false;
  if (it.status === "pending" && act !== "dismiss" && act !== "back") active = { id: it.id, sid: it.session_id };
  if (Object.hasOwn(ITEM_ACTS, act)) ITEM_ACTS[act](it);
  return true;
}

const CLICKS = [
  ...OVERLAY_CLOSERS, ...BTW_CLICKS, ...PROJECT_CLICKS, ...TERM_CLICKS, ...STATUS_CLICKS, ...QUEUE_CLICKS, ...SETUP_CLICKS,
  ...MORE_MENU_CLICKS, ...UPDATE_CLICKS, ...THREAD_CLICKS, ...LISTEN_CLICKS, ...READER_CLICKS, ...BOX_CLICKS, ...DRAWER_CLICKS,
  ...FIND_CLICKS, ...CARD_CLICKS, ...MESSAGE_CLICKS, ...USAGE_CLICKS, ...SETTING_CLICKS, ...CHAT_FOOT_CLICKS, ...LIVE_CLICKS,
  ...START_CLICKS, ...PARKED_CLICKS, ...BOARD_CLICKS, clickAct,
];
