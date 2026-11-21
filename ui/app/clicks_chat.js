// Cue UI app — clicks in a session's chat and its box: side questions, the terminal, the status list, the ⋯ menu,
// steps, Listen, the reader, the "/" menu, messages' links and pictures. Each list is a run of ui/app/clicks.js's
// order (first match wins).

const BTW_CLICKS = [
  // Fork under a side answer: this conversation as a new session, picking up from that question and answer.
  rule("[data-act=btw-fork]", (bf) => forkBtw(bf.dataset.sid, +bf.dataset.i)),
  rule("[data-act=btw-offer]", (bo) => btwFromKept(bo.dataset.sid)),
  rule("[data-act=btw-keep]", (bk) => { btwKept.add(`${bk.dataset.sid}:${bk.dataset.at}`); renderMain(); }),
  rule("[data-act=btw-requeue]", (br) => btwRequeue(br.dataset.sid, +br.dataset.i)),
  // Clear: forget this session's side questions (they were only ever in this window).
  rule("[data-act=btw-clear]", (bc) => { btwLog.delete(bc.dataset.sid); renderMain(); document.querySelector(`[data-text="${CSS.escape(`btw:${bc.dataset.sid}`)}"]`)?.focus(); }),
];

const TERM_CLICKS = [
  rule("[data-act=term], [data-act=term-open], [data-act=term-card]", (tm) => {
    if (tm.dataset.act === "term") return termSid === tm.dataset.sid ? closeTerm() : openTerm(tm.dataset.sid);
    // From a Waiting card: open its session, then its terminal.
    if (tm.dataset.act === "term-card") { const it = findItem(tm.dataset.id); setActive(tm.dataset.id); return it && openTerm(it.session_id); }
    return termSid === tm.dataset.sid ? termObj?.focus() : openTerm(tm.dataset.sid);
  }),
  rule("[data-act=btw], [data-act=btw-close]", (bw) => { if (bw.dataset.act === "btw" && btwFor !== bw.dataset.sid) { tipUsed("btw"); return openBtw(bw.dataset.sid); } btwFor = null; renderMain(); }),
];

const WATCH_CLICKS = [
  // Tell me when it's done (the line under the chat, or ⋯: done with the menu too).
  rule("[data-act=watch]", (wt) => { moreFor = null; setWatch(wt.dataset.sid, wt.dataset.on === "1"); }),
];

const QUEUE_CLICKS = [
  // A tip's ×: that tip never comes back (tips.js).
  rule("[data-act=tip-x]", (tx) => { tipUsed(tx.dataset.tipId); renderMain(); }),
  // A queued message, sent now: stopping the turn (Esc) makes the agent read it straight away.
  rule("[data-act=send-now]", (sn) => sendQueuedNow(sn.dataset.sid)),
  rule("[data-act=unqueue-review]", (uq) => invoke("unqueue_review", { exec: uq.dataset.exec }).then(() => toast("Taken off the queue")).catch((e) => toast(`Couldn't: ${e}`))),
  // Undo a crew decision (team.js crewLogHtml): Cue tells the lead; the line says "undoing…" at once.
  rule("[data-act=crew-back]", (cb) => {
    const k = `${cb.dataset.sid}|${cb.dataset.id}`;
    crewBackAsked.add(k);
    renderMain();
    invoke("crew_go_back", { sessionId: cb.dataset.sid, id: cb.dataset.id }).then((m) => toast(m), (e) => { crewBackAsked.delete(k); renderMain(); toast(`Couldn't: ${e}`); });
  }),
  // On the Desk (the lead's chat, a session's page) back into the Desk's box it went from (#498), not the Board's.
  rule("[data-act=held-edit]", (he) => pullHeld(he.dataset.sid, boxViewOwner(view)?.keptBox?.(he.dataset.sid) || undefined)),
  rule("[data-act=held-now]", (hn) => sendHeldNow(hn.dataset.sid)),
  rule("[data-act=held-x]", (hx) => cancelHeld(hx.dataset.sid)),
  // A message for later: back to the box, dropped, or sent now.
  rule("[data-act=send-at-edit]", (se) => sendAtEdit(se.dataset.sid)),
  rule("[data-act=send-at-x]", (sx) => invoke("send_at_drop", { sessionId: sx.dataset.sid }).then(() => toast("It won't be sent"))),
  rule("[data-act=send-at-now]", (sn) => invoke("send_at_now", { sessionId: sn.dataset.sid }).then((m) => toast(m), (e) => toast(`Couldn't send it: ${e}`))),
];

/** The ⋯ menu's item `cmd` for session `sid`. */
function moreCommand(cmd, sid) {
  moreFor = null;
  renderMain();
  invoke("session_command", { sessionId: sid, action: cmd }).then((m) => toast(m), (e) => toast(`Couldn't: ${e}`));
}
/** ⋯ → Restart. Its "/" commands are read again after (a skill added since shows up): the Mac forgets its list too. */
function moreRestart(sid) {
  moreFor = null;
  renderMain();
  return invoke("restart_session", { sessionId: sid }).then((m) => { cmdLists.delete(sid); cmdMissed.delete(sid); if (m) toast(m); }, (e) => toast(`Couldn't restart: ${e}`));
}
const MORE_MENU_CLICKS = [
  // The ⋯ menu: toggle it, run an item, or close it on a click anywhere else.
  rule("[data-more]", (mb) => { moreFor = moreFor === mb.dataset.more ? null : mb.dataset.more; newProjFor = null; movingProj = mcpPicking = false; renderMain(); }),
  rule("[data-move-ask]", () => { movingProj = true; renderMain(); }),
  // ⋯ → Fork: this conversation as a new session too, starting with nothing said.
  rule(".more-menu [data-fork]", (fk) => { moreFor = null; forkBtw(fk.dataset.fork, null); }),
  rule("[data-new-proj]", (np) => { newProjFor = np.dataset.newProj; newProjNote = ""; draft(`newproj:${newProjFor}`).text = ""; renderMain(); document.querySelector(".pm-new-in")?.focus(); }),
  rule("[data-move-proj]", (mvp) => { moreFor = null; movingProj = false; moveToProject(mvp.dataset.sid, mvp.dataset.moveProj); renderMain(); }),
  rule("[data-restart]", (rsb) => moreRestart(rsb.dataset.restart)),
  rule(".more-menu [data-desk-member-set]", (dm) => { moreFor = null; renderMain(); deskMember(dm.dataset.project, dm.dataset.deskMemberSet, dm.dataset.on === "1"); }),
  rule("[data-cmd]", (mc) => moreCommand(mc.dataset.cmd, mc.dataset.sid)),
  (t) => { if (moreFor && !t.closest(".more-wrap")) { moreFor = newProjFor = null; movingProj = mcpPicking = false; renderMain(); } return false; },
  // Decide later: done with the menu too.
  (t) => { if (t.closest(".more-menu [data-park]")) moreFor = null; return false; },
];

/** `key` in `set` taken out if it's there, else put in. */
const flipIn = (set, key) => (set.has(key) ? set.delete(key) : set.add(key));

const THREAD_CLICKS = [
  rule("[data-turn-fold]", (tf) => { const k = tf.dataset.turnFold; turnFold.set(k, tf.dataset.open !== "1"); renderMain(); wantDetails(k.split("|")[0]); }),
  rule("[data-copy-code]", (cc) => copyCode(cc)),
  rule("[data-run-code]", (rc) => runCode(rc)),
  // A card's "N agents": open its session (its item, on a waiting card) on its Agents tab.
  rule("[data-bg-open]", (bo) => { const sid = bo.dataset.bgOpen, it = bo.closest("[data-big]")?.dataset.big; bgTabOn.set(sid, "agents"); return it ? setActive(it) : openSession(sid); }),
  // A stuck agent's Resume / Stop, before its row's own click (agents.js).
  rule("[data-ag-wake]", (b) => agentWake(b.dataset)),
  rule("[data-helper]", (hb) => { const [sid, id] = hb.dataset.helper.split("|"); helperView = { sid, id }; renderMain(); }),
  // Back to the chat (Go to chat, ‹ Back to chat, the lead's chip): with the cursor in its box, ready to write.
  rule("[data-helper-back]", () => { closeHelperView(); focusComposer(); }),
];

/** Settings → Voice's picker, opened or closed; opened, the voice in use in view, a line or two from the top. */
function flipVoiceList() {
  voiceListOpen = !voiceListOpen;
  renderMain();
  const list = document.querySelector(".vp-list"), on = list?.querySelector(".vp-line.on");
  if (on) list.scrollTop = on.offsetTop - 44;
}
const LISTEN_CLICKS = [
  rule("[data-listen]", (ls) => { const k = ls.dataset.listen; return listening?.key === k ? listenToggle() : listenPlay(k.slice(0, k.lastIndexOf(":")), k, !!ls.dataset.listenReader, ls.closest(".rdv-head") ? readListenFrom() : 0); }),
  rule("[data-listen-toggle]", () => listenToggle()),
  rule("[data-listen-stop]", () => listenStop()),
  rule("[data-listen-skip]", (lk) => listenSkip(+lk.dataset.listenSkip)),
  rule("[data-listen-speed]", (lsp) => listenFaster(+lsp.dataset.listenSpeed)),
  rule("[data-listen-then]", () => listenThenNext()),
  rule("[data-listen-plain]", () => listenPlainFlip()),
  rule("[data-voice-list]", flipVoiceList),
  rule("[data-voice-sample]", (vs) => playSample(vs.dataset.voiceSample)),
  rule("[data-voice-get]", (vg) => getVoice(vg.dataset.voiceGet)),
  rule("[data-voice-pick]", (vp) => { voiceListOpen = false; pickVoice(vp.dataset.voicePick); }),
  rule("[data-kokoro-remove]", () => kokoroTell({ remove: true })),
  rule("[data-listen-open]", (lo) => setActive(null, lo.dataset.listenOpen)),
  // A click in a reply being read out: it reads on from there.
  (t, e) => !t.closest("button, a, input") && !!listenFrom(e),
];

const READER_CLICKS = [
  rule("[data-read]", (rd) => { const k = rd.dataset.read; openReader(k.slice(0, k.lastIndexOf(":")), k); }),
  rule("[data-reader-back]", () => closeReader()),
  rule("[data-reader-open]", (ro) => invoke("open_link", { url: ro.dataset.readerOpen }).catch((err) => toast(`Couldn't open: ${err}`))),
  rule("[data-reader-term]", (rt) => { closeReader(); openTerm(rt.dataset.readerTerm); }),
  rule("[data-reader-reply]", () => replyFromReader()),
  rule("[data-reader-copy]", () => copyReader()),
  rule("[data-reader-step]", (rs) => stepReader(+rs.dataset.readerStep)),
  rule("[data-reader-size]", (rz) => sizeReader(+rz.dataset.readerSize)),
];

/** "Send as a message" under a box whose text starts with "/": a space before it, so it's not a command. */
function sendAsText(st) {
  const key = st.dataset.astext, el = document.querySelector(`textarea[data-text="${CSS.escape(key)}"]`);
  draft(key).text = ` ${draft(key).text.trimStart()}`;
  saveDrafts();
  st.parentElement.hidden = true;
  refreshCmd(key);
  if (el) { el.value = draft(key).text; el.focus(); el.selectionStart = el.selectionEnd = el.value.length; }
}
const BOX_CLICKS = [
  rule("[data-step]", (sl) => { const k = sl.dataset.step; flipIn(stepFlip, k); renderMain(); wantDetails(k.split("|")[0]); }),
  rule("[data-quick]", (qp) => sendPhrase(qp.dataset.quick, qp.dataset.phrase)),
  rule("[data-cmd-refresh]", (cr) => { cmdList(sidOfKey(cr.dataset.cmdRefresh), true); renderMain(); document.querySelector(`[data-text="${CSS.escape(cr.dataset.cmdRefresh)}"]`)?.focus(); }),
  rule("[data-cmd-pick]", (cp) => pickCmd(cp.dataset.cmdPick, cp.dataset.cmdName, false)),
  rule("[data-astext]", sendAsText),
  // 📎 on a box: Finder's picker, for that box; × on a picture takes it off.
  rule("[data-attach]", (at) => pickImages(at.dataset.attach)),
  // The clock on a session's box: its "Send at…" bar, open or shut; "Send tomorrow at 08:01" hands the text to Cue.
  rule("[data-send-at-open]", (so) => { sendAtFor = sendAtFor === so.dataset.sendAtOpen ? null : so.dataset.sendAtOpen; renderMain(); }),
  rule("[data-send-at-x]", () => { sendAtFor = null; renderMain(); }),
  rule("[data-send-at-go]", (sg) => sendAtKeep(sg.dataset.sendAtGo)),
  rule("[data-unattach]", (un) => { const [key, n] = un.dataset.unattach.split(/:(?=\d+$)/); draft(key).images.splice(+n, 1); renderMain(); }),
];

/** A path opens on a click, as a link does (settings.rs how_to_open: its app, as text, or shown in Finder).
 *  Not when the press was you selecting part of it. */
function openPath(pl, e) {
  e.preventDefault();
  const sel = getSelection();
  if (!sel.isCollapsed && pl.contains(sel.anchorNode)) return;
  // Found from its session's folder: markdown in Read, any other file in the pop-up viewer (one on this Mac; another's
  // paths aren't linked at all).
  return openPathOrDoc(pl.dataset.path, pl.dataset.sid || current()?.s?.session_id);
}
/** Screenshots a message points at, full size: the ones Cue could read. */
function openLocalImages(llb) {
  const { paths, i } = JSON.parse(llb.dataset.localLb);
  const srcs = paths.map((p) => localImgs.get(p)).filter(Boolean);
  if (srcs.length) { lightbox = { srcs, i: Math.min(i, srcs.length - 1) }; renderMain(); }
}
const MESSAGE_CLICKS = [
  rule("[data-fwd]", (el) => { forward = { ...fwdMsgs.get(el.dataset.fwd), key: el.dataset.fwd, to: null }; sheet = "forward"; renderMain(); }, (el) => fwdMsgs.has(el.dataset.fwd)),
  rule("[data-fwd-to]", (el) => { forward.to = el.dataset.fwdTo; renderMain(); document.querySelector(".fwd-note")?.focus(); }, () => !!forward),
  rule("[data-act='send-forward']", () => sendForward()),
  rule("[data-act='cancel-forward']", () => { sheet = forward = null; renderMain(); }),
  rule("a[data-path]", openPath),
  rule("a[data-href]", (link, e) => { e.preventDefault(); openPathOrDoc(link.dataset.href); }),
  rule("[data-local-lb]", openLocalImages),
  rule("[data-lightbox]", (lb) => { lightbox = JSON.parse(lb.dataset.lightbox); renderMain(); }),
  rule("[data-draft-lightbox]", (dlb) => { const [key, n] = dlb.dataset.draftLightbox.split(/:(?=\d+$)/); lightbox = { srcs: draft(key).images.map((im) => im.data), i: +n }; renderMain(); }),
  (t) => { if (!t.matches(".scrim")) return false; sheet = null; if (t.matches(".spot-scrim")) liveOpen = newOpen = false; renderMain(); return true; },
];

const CHAT_FOOT_CLICKS = [
  rule("[data-hook-fold]", (hk) => { flipIn(hookOpen, hk.dataset.hookFold); renderMain(); }),
  rule("[data-follow]", (fo) => { flipIn(openFollow, fo.dataset.follow); renderMain(); }),
  rule("[data-act=interrupt]", (stop) => interrupt(stop.dataset.sid)),
  rule("[data-act=send-to]", (el) => sendTo(el.dataset.sid)),
];

/** Read (read.js): Recent, the file's head, Contents. */
const READ_CLICKS = [
  rule("[data-read-open]", (el) => { const r = readRecent.find((x) => x.path === el.dataset.readOpen); readOpen(el.dataset.readOpen, r?.sid || null); }),
  rule("[data-read-new]", (el) => readOpen(el.dataset.readNew, null, null, "new")),
  rule("[data-read-forget]", (el) => readForget(el.dataset.readForget)),
  rule("[data-read-back]", () => readBack()),
  rule("[data-read-h]", (el) => readJump(+el.dataset.readH)),
];
