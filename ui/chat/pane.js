// Cue UI chat — the Active pane.
/** "Tell me": told when this turn is done, even with Cue in front. ⌘D (watchTurn) or ⋯ → Tell me when this is
 *  done (tellItem), from the turn's first second; the tada in place of the title's dot and the line under the
 *  chat (tips.js tellTip) say it's on. */
const atWork = (s) => s?.state === "working" || s?.state === "agent";
function setWatch(sid, on) {
  const s = sessionOf(sid);
  if (s) s.watch_ms = on ? Date.now() : 0;   // shown now; Cue's next update says the same
  renderMain();
  return invoke("set_watch", { sessionId: sid, on }).catch((e) => toast(`Couldn't: ${e}`));
}
/** ⌘D on the open session: tell me when this turn is done. Only ever on (Undo on the line under the chat
 *  takes it back); pressed again, it says so again. */
function watchTurn() {
  const s = sessionOf(quietOpen || active?.sid);
  if (!atWork(s)) return toast("Nothing running to tell you about");
  if (!s.watch_ms) setWatch(s.session_id, true);
  toast("Telling you when this is done");
}
/** ⋯ → Tell me when this is done, while a turn runs; Don't tell me once it's on. */
function tellItem(sid, s) {
  if (!atWork(s)) return "";
  const on = !!s.watch_ms;
  return `<button data-act="watch" data-sid="${esc(sid)}" data-on="${on ? 0 : 1}">${on ? "Don't tell me" : "Tell me when this is done"}<span>${on ? "No notification when this turn ends" : "A notification and the tada sound when this turn ends, even with Cue in front · ⌘D"}</span></button>`;
}
/** Where the project's dot is, while you're waiting to be told: the tada, in the project's color. */
const watchMark = (project) => `<span class="watch-mark" style="${projStyle(project)}" role="img" aria-label="Telling you when this turn is done">${tadaIcon(14, true)}</span>`;

/** The Active pane when nothing is open: Connect Claude Code until it is, then "Nothing waiting". */
function emptyPane() {
  if (state.connections && !state.connections.claude?.ok) return `<div class="active-pane empty"><div class="quiet-big">Connect Cue to Claude Code</div><div class="dim">Cue adds its hooks to Claude Code's settings (backed up first), then sessions that need you show up here.</div><button class="btn primary" data-connect="claude">Connect Claude Code</button></div>`;
  return `<div class="active-pane empty"><div class="quiet-big">Nothing waiting.</div><div class="dim">Pick anything on the right to read it or message the session.</div></div>`;
}
/** The head's pill: what a pending card wants (a finished turn that changed files says what changed, "4 files ·
 *  +98 −9"; hover for which), or the session's state. */
function activePill(it, s, pending, ch) {
  const turnWord = ch ? `${ch.files.length} file${ch.files.length === 1 ? "" : "s"} · +${ch.add} −${ch.del}` : it?.interrupted ? "interrupted" : "your turn";
  return pending
    ? `<span class="pill ${it.interrupted ? "intr" : ""}">${it.kind === "waiting" ? turnWord : it.kind === "question" || it.kind === "input" ? "asks you" : inTerminal(it) ? "asks in its terminal" : "needs a decision"} · ${ago(it.created_ms)}${it.kind === "waiting" ? `<button class="pill-x" data-act="dismiss" data-id="${esc(it.id)}" title="Take it off Waiting" aria-label="Take it off Waiting">×</button>` : ""}</span>`
    : `<span class="pill soft">${s ? { working: "working", waiting: "your turn", deciding: "deciding", agent: s.helpers ? "on its agents" : "on its lead", stopped: "stopped", limited: s.limit && lifted(s.limit) ? "usage is back" : "out of usage" }[s.state] || s.state : "answered"}</span>`;
}
/** A pending ask's card under the chat: what it asks and its buttons. In its terminal: what Claude Code said ("A
 *  sandboxed command needs network access"), then the call it's about. You were typing to this session when it
 *  asked: your box stays (cursor and text intact), and sending waits until you've answered, so nothing gets typed
 *  into its prompt. */
function askFoot(it, sid, harness) {
  // A teammate's ask: titled "<lead> › <teammate>" (team.js).
  const who = mateOfAsk(it) ? `${esc(nameOf(sid, it.project))} ${mateTitle(it)}` : esc(agentName(harness));
  const head = (inTerminal(it) ? esc(it.message || `${agentName(harness)} ${verb(it)}`) : `${who} ${esc(verb(it))}`) + machTag(it.machine);
  let foot = `<div class="act-card"><div class="act-head">${head}</div>${inTerminal(it) && !it.tool_name ? "" : requestBody(it)}${it.kind !== "question" ? whyLine(it) : ""}${decisionButtons(it)}</div>`;
  const key = `s:${sid}`;
  if (document.activeElement?.dataset?.text === key || draft(key).text.trim() || draft(key).images.length) {
    foot += `<div class="foot held">${box_(key, "Answer above first; your message waits here…", "Answer first", `data-held="1" disabled`)}</div>`;
  }
  return foot;
}
/** Under the chat: an ask's card, a reply box for a finished turn, or the session's box. Working: what you send
 *  waits for its current step (Queue, ↵), or stops it and goes now (⌘↵; the queued message says so, with Send
 *  now). Pi takes it straight into its queue (read at its next step); for the others Cue keeps it until the turn
 *  ends. */
function activeFoot(it, s, pending, harness, project, sid) {
  if (pending && it.kind !== "waiting") return askFoot(it, sid, harness);
  // A hosted session has no terminal: the reply goes straight to it.
  if (pending) return `<div class="foot">${field(it, s?.hosted ? `Reply to ${project}…` : `Reply to ${project} (typed into its terminal)…`, "Send")}</div>`;
  if (!s) return "";
  const busy = s.state === "working";
  const after = harness === "pi" ? "it reads this after its current step" : "Cue keeps it and sends it when this turn ends";
  return `<div class="foot">${box_(`s:${sid}`, busy ? `Message ${nameOf(sid, project)}… ${after}` : `Message ${project}…`, busy ? "Queue" : "Send", `data-act="send-to" data-sid="${esc(sid)}"`)}</div>`;
}
/** What fills the chat area instead of the chat: a background agent opened in full (helperPane), or a reply open
 *  in the reader (readerPane, read-only, the box and insight bar out of the way). */
function fullView(sid, it, s, harness) {
  const hv = sid ? helperViewed(sid) : null, hvp = hv && helperPane(sid, hv);
  const rv = sid && !hvp ? readerViewed(sid, it, s) : null, rvp = rv && readerPane(sid, it, s, harness, rv);
  const full = hvp ? { ...hvp, forKey: `h|${sid}|${hv.id}` } : rvp ? { ...rvp, forKey: `r|${sid}|${rv.all[rv.at].key}` } : null;
  return { full, reading: !!rvp };
}
/** The pane's head: the project, the agent, the name, its chips, the pill, and its buttons. The badge
 *  already says which agent: no "Claude Code" beside it. */
function activeHead(a) {
  const { it, s, sid, harness, project, pending, ch } = a;
  const closable = sid && svClosable({ sid, st: pending && it.kind !== "waiting" ? "asks" : s?.state || "idle" });
  return `<div class="ap-head">${s?.watch_ms ? watchMark(project) : projBtn(project, "ap")}${badge(harness)}${nameHead(sid, project)}${a.onMachine}${a.pill}${sid ? agentChips(sid) + titleChips(sid) : ""}${ch ? `<button class="btn small commit-btn" data-act="commit" data-id="${esc(it.id)}">Commit</button>` : ""}<span class="grow"></span>
      ${sid ? crewChip(sid) + moreMenu(sid, s, closable) : ""}
      ${s && harness === "claude" ? `<button class="btn btw-btn ${btwFor === sid ? "on" : ""}" data-act="btw" data-sid="${esc(sid)}">btw</button>` : ""}
      ${s?.state === "working" ? `<button class="btn deny" data-act="interrupt" data-sid="${esc(sid)}" title="Stop it mid-turn (Esc twice)">Stop</button>` : ""}
      ${closable ? closeBtn(sid) : ""}${focus && s?.state !== "working" ? `<span class="esc-hint">esc</span>` : ""}</div>`;
}
/** The chat area: the full view in its place, or the chat with the rename notice, Catch me up and its status line. */
function activeChat(a, full) {
  const { it, s, sid, harness } = a;
  return full ? full.bar + chatBox(full.forKey, full.body) : chatBox(`m|${sid || it?.id}`, chatHtml(it, s, harness, true) + keyed(renameNotice(s), "rename") + (sid && s ? catchUpBar(sid, it, s) : "") + keyed(statusLine(it, s), "status"));
}
/** The crew's pill on its own, right-aligned above the foot, while an ask's card stands where the box (and its pill) was. */
const crewAlone = (a) => a.pending && a.it?.kind !== "waiting" && crewStrip(a.sid) ? `<div class="crew-alone">${crewStrip(a.sid)}</div>` : "";
/** The pane for an open card or session. At work (a turn, or compacting): a light sweeps along the top edge, as
 *  in its terminal tab. Not while it waits. The Next bar (nextBar) is hidden for now (it read as part of the terminal tabs
 *  and the box; N still works). */
function openPane(a, full) {
  const { s, sid, foot } = a;
  return `<div class="active-pane ${s?.state === "working" || s?.compacting_ms ? "busy" : ""}">${sweep(sid, s)}
    ${activeHead(a)}
    ${btwPanel(sid)}
    ${subHead(s?.cwd || a.it?.cwd, sid)}
    ${sid ? addOn.chatTop(sid) : ""}
    ${sid ? `<div class="ap-bar">${barLabels(sid)}${bar(sid)}${barKey(sid)}</div>` : ""}
    ${sessTabs(sid)}${sid ? statusPanel(a.it, s) : ""}
    ${activeChat(a, full)}${extSlot(sid)}${diffSlot(sid)}${aboutSlot(sid)}${actSlot(sid)}${bgSlot(sid)}
    ${termDock(sid)}${full ? full.foot : listenLine(sid) + (sid ? crewDock(sid) + crewAlone(a) : "") + foot}${sid ? continueCover(sid) : ""}</div>`;
}
function activePane() {
  if (quietOpen) return quietPane();
  const pk = parkedActive();
  if (pk) return parkedPane(pk);
  const cur = current();
  if (!cur) return emptyPane();
  const { it, s } = cur;
  const pending = isPending(it);
  const harness = it?.harness || s?.harness;
  const project = it?.project || s?.project;
  const sid = s?.session_id || it?.session_id;
  noteLooked(sid);
  const ch = pending && it.kind === "waiting" && !it.interrupted ? s?.changes : null;
  const pill = activePill(it, s, pending, ch);
  const foot = activeFoot(it, s, pending, harness, project, sid);
  const { full, reading } = fullView(sid, it, s, harness);
  const onMachine = machTag(s?.machine || it?.machine);
  // Restarting (after an MCP change): covered until it's back, with nothing to press meanwhile.
  const rs = sid && state.restarting?.[sid];
  if (rs) return `<div class="active-pane restarting">
    <div class="ap-head">${projBtn(project, "ap")}${badge(harness)}${nameHead(sid, project)}${onMachine}<span class="pill soft">restarting</span><span class="grow"></span></div>
    ${subHead(s?.cwd || it?.cwd, sid)}
    ${chatBox(`m|${sid}`, chatHtml(it, s, harness, true))}${foot}${restartCover(rs, nameOf(sid, project))}</div>`;
  // A reply in the reader: the pane is the reader's (its bar names the session), the box out of the way.
  if (reading) return `<div class="active-pane reading ${s?.state === "working" || s?.compacting_ms ? "busy" : ""}">${sweep(sid, s)}
    ${full.bar + chatBox(full.forKey, full.body)}${extSlot(sid)}${diffSlot(sid)}${full.foot}${continueCover(sid)}</div>`;
  return openPane({ it, s, sid, harness, project, pending, ch, pill, foot, onMachine }, full);
}
