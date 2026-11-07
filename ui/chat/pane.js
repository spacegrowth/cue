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

function activePane() {
  if (quietOpen) return quietPane();
  const pk = parkedActive();
  if (pk) return parkedPane(pk);
  const cur = current();
  if (!cur && state.connections && !state.connections.claude?.ok) return `<div class="active-pane empty"><div class="quiet-big">Connect Cue to Claude Code</div><div class="dim">Cue adds its hooks to Claude Code's settings (backed up first), then sessions that need you show up here.</div><button class="btn primary" data-connect="claude">Connect Claude Code</button></div>`;
  if (!cur) return `<div class="active-pane empty"><div class="quiet-big">Nothing waiting.</div><div class="dim">Pick anything on the right to read it or message the session.</div></div>`;
  const { it, s } = cur;
  const pending = isPending(it);
  const harness = it?.harness || s?.harness;
  const project = it?.project || s?.project;
  const sid = s?.session_id || it?.session_id;
  noteLooked(sid);
  // A finished turn that changed files: the pill says what changed ("4 files · +98 −9"; hover for which), and Commit sits beside it.
  const ch = pending && it.kind === "waiting" && !it.interrupted ? s?.changes : null;
  const turnWord = ch ? `${ch.files.length} file${ch.files.length === 1 ? "" : "s"} · +${ch.add} −${ch.del}` : it?.interrupted ? "interrupted" : "your turn";
  const pill = pending
    ? `<span class="pill ${it.interrupted ? "intr" : ""}">${it.kind === "waiting" ? turnWord : it.kind === "question" ? "asks you" : inTerminal(it) ? "asks in its terminal" : "needs a decision"} · ${ago(it.created_ms)}${it.kind === "waiting" ? `<button class="pill-x" data-act="dismiss" data-id="${esc(it.id)}" title="Take it off Waiting" aria-label="Take it off Waiting">×</button>` : ""}</span>`
    : `<span class="pill soft">${s ? { working: "working", waiting: "your turn", deciding: "deciding", agent: s.helpers ? "on its agents" : "on its lead", stopped: "stopped", limited: s.limit && lifted(s.limit) ? "usage is back" : "out of usage" }[s.state] || s.state : "answered"}</span>`;
  let foot;
  if (pending && it.kind !== "waiting") {
    // In its terminal: what Claude Code said ("A sandboxed command needs network access"), then the call it's about.
    const head = (inTerminal(it) ? esc(it.message || `${agentName(harness)} ${verb(it)}`) : `${esc(agentName(harness))} ${esc(verb(it))}`) + machTag(it.machine);
    foot = `<div class="act-card"><div class="act-head">${head}</div>${inTerminal(it) && !it.tool_name ? "" : requestBody(it)}${it.kind !== "question" ? whyLine(it) : ""}${decisionButtons(it)}</div>`;
    // You were typing to this session when it asked: your box stays (cursor and text intact),
    // and sending waits until you've answered, so nothing gets typed into its prompt.
    const key = `s:${sid}`;
    if (document.activeElement?.dataset?.text === key || draft(key).text.trim() || draft(key).images.length) {
      foot += `<div class="foot held">${box_(key, "Answer above first; your message waits here…", "Answer first", `data-held="1" disabled`)}</div>`;
    }
  } else if (pending) {
    foot = `<div class="foot">${field(it, `Reply to ${project} (typed into its terminal)…`, "Send")}</div>`;
  } else if (s) {
    const busy = s.state === "working";
    // Working: what you send waits for its current step (Queue, ↵), or stops it and goes now (⌘↵; the
    // queued message says so, with Send now).
    // Pi takes it straight into its queue (read at its next step); for the others Cue keeps it until the turn ends.
    const after = harness === "pi" ? "it reads this after its current step" : "Cue keeps it and sends it when this turn ends";
    foot = `<div class="foot">${box_(`s:${sid}`, busy ? `Message ${nameOf(sid, project)}… ${after}` : `Message ${project}…`, busy ? "Queue" : "Send", `data-act="send-to" data-sid="${esc(sid)}"`)}</div>`;
  } else foot = "";
  const cm = crewOf(sid);
  // A background agent opened in full (helperPane): it takes the chat area and the box, under the same head.
  const hv = sid ? helperViewed(sid) : null, hvp = hv && helperPane(sid, hv);
  // A reply open in the reader (readerPane): the same place, read-only, the box and insight bar out of the way.
  const rv = sid && !hvp ? readerViewed(sid, it, s) : null, rvp = rv && readerPane(sid, it, s, harness, rv);
  const full = hvp ? { ...hvp, forKey: `h|${sid}|${hv.id}` } : rvp ? { ...rvp, forKey: `r|${sid}|${rv.all[rv.at].key}` } : null;
  // The badge already says which agent: no "Claude Code" beside it. An executor's relay name and a crew's model stay.
  const who = cm?.role === "executor" ? `${esc(cm.name)}${cm.model ? ` · ${esc(shortModel(cm.model))}` : ""}` : cm?.role === "lead" && cm.model ? esc(shortModel(cm.model)) : "";
  const onMachine = machTag(s?.machine || it?.machine);
  // Restarting (after an MCP change): covered until it's back, with nothing to press meanwhile.
  const rs = sid && state.restarting?.[sid];
  if (rs) return `<div class="active-pane restarting">
    <div class="ap-head">${projBtn(project, "ap")}${badge(harness)}${nameHead(sid, project)}${roleTag(sid, true)}${onMachine}<span class="pill soft">restarting</span><span class="grow"></span></div>
    ${subHead(s?.cwd || it?.cwd, "", sid)}
    ${chatBox(`m|${sid}`, chatHtml(it, s, harness, true))}${foot}${restartCover(rs, nameOf(sid, project))}</div>`;
  // A reply in the reader: the pane is the reader's (its bar names the session), the box out of the way.
  if (rvp) return `<div class="active-pane reading ${s?.state === "working" || s?.compacting_ms ? "busy" : ""}">${sweep(sid, s)}
    ${full.bar + chatBox(full.forKey, full.body)}${extSlot(sid)}${full.foot}${continueCover(sid)}</div>`;
  // At work (a turn, or compacting): a light sweeps along the top edge, as in its terminal tab. Not while it waits.
  return `<div class="active-pane ${s?.state === "working" || s?.compacting_ms ? "busy" : ""}">${sweep(sid, s)}
    <div class="ap-head">${relayBack(sid)}${s?.watch_ms ? watchMark(project) : projBtn(project, "ap")}${badge(harness)}${relayMark(sid)}${nameHead(sid, project)}${roleTag(sid, true)}${who ? `<span class="dim">${who}</span>` : ""}${onMachine}${pill}${sid ? (execOf(sid) ? teamChips(sid) : agentChips(sid)) + titleChips(sid) : ""}${ch ? `<button class="btn small commit-btn" data-act="commit" data-id="${esc(it.id)}">Commit</button>` : ""}<span class="grow"></span>
      ${sid ? moreMenu(sid, s, svClosable({ sid, st: pending && it.kind !== "waiting" ? "asks" : s?.state || "idle" })) : ""}
      ${s && harness === "claude" ? `<button class="btn btw-btn ${btwFor === sid ? "on" : ""}" data-act="btw" data-sid="${esc(sid)}">btw</button>` : ""}
      ${s?.state === "working" ? `<button class="btn deny" data-act="interrupt" data-sid="${esc(sid)}" title="Stop it mid-turn (Esc twice)">Stop</button>` : ""}
      ${sid && svClosable({ sid, st: pending && it.kind !== "waiting" ? "asks" : s?.state || "idle" }) ? closeBtn(sid) : ""}${focus && s?.state !== "working" ? `<span class="esc-hint">esc</span>` : ""}</div>
    ${btwPanel(sid)}
    ${subHead(s?.cwd || it?.cwd, "", sid)}
    ${sid ? reviewRows(sid) + leadCard(sid) + packetCard(sid) : ""}
    ${sid ? `<div class="ap-bar">${barLabels(sid)}${bar(sid)}${barKey(sid)}</div>` : ""}
    ${sessTabs(sid)}${sid ? statusPanel(it, s) : ""}
    ${full ? full.bar + chatBox(full.forKey, full.body) : chatBox(`m|${sid || it?.id}`, chatHtml(it, s, harness, true) + keyed(renameNotice(s), "rename") + (sid && s ? catchUpBar(sid, it, s) : "") + keyed(statusLine(it, s), "status"))}${extSlot(sid)}${aboutSlot(sid)}${actSlot(sid)}${bgSlot(sid)}
    ${"" /* The Next bar, hidden for now (it read as part of the terminal tabs and the box; N still works): pending ? "" : nextBar() */}
    ${termDock(sid)}${full ? full.foot : listenLine(sid) + foot}${sid ? continueCover(sid) : ""}</div>`;
}
