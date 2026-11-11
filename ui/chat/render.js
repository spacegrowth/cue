// Cue UI chat — the chat's HTML: turns, status, the meta line and the ⋯ menu.
/** One turn's steps, between your message and its reply. `live`: the turn it's working on now;
 *  `latest`: the newest turn (open by default, so you see what led to its reply). `part`: when you
 *  wrote mid-turn, the turn is drawn in parts around your messages ({ from, to } ms); only the first
 *  part has the header (its count is the whole turn's). `replied`: the chat has its reply (false when you
 *  cut it off: its last words then stay here, or they'd be nowhere). */
function stepsBlock(sid, turn, live, latest, part = { from: -Infinity, to: Infinity }, said = new Set(), replied = true) {
  const mode = stepsMode();
  // What a Stop hook made it do goes in its own fold at the end (hookFold); the rest is the turn.
  const hooked = turn.items.filter((x) => hookedPart(turn, x) && (x.t !== "say" || !said.has(String(x.text || "").trim())));
  let kept = turn.items.filter((x) => !hookedPart(turn, x));
  // A finished turn's last words are its reply, already in the chat; so are words the chat shows as a
  // reply mid-turn: not twice.
  if (!live && replied) { let end = kept.length; while (end && kept[end - 1].t === "say") end--; kept = kept.slice(0, end); }
  kept = kept.filter((x) => x.t !== "say" || !said.has(String(x.text || "").trim()));
  const steps = kept.filter((x) => x.t === "step");
  const first = part.from === -Infinity;
  const fold = part.to === Infinity ? hookFold(sid, turn, hooked) : "";
  if (!steps.length) return fold ? `<div class="stx" data-key="${esc(`t:${turn.at_ms}:${part.from}`)}">${fold}</div>` : "";
  const items = kept.map((x, i) => [x, i]).filter(([x]) => x.at_ms >= part.from && x.at_ms < part.to);
  const key = `${sid}|${turn.at_ms}`;
  const open = turnFold.has(key) ? turnFold.get(key) : mode === "all" || (mode === "line" && (live || latest));
  const all = mode === "all" || turnAll.has(key);
  const count = `${steps.length}${turn.more ? "+" : ""} step${steps.length === 1 ? "" : "s"}`;
  const bk = `t:${turn.at_ms}:${part.from}`;
  const head = `<div class="stx-head" data-key="head"><button class="stx-fold" data-turn-fold="${esc(key)}" data-open="${open ? 1 : 0}">${live ? `<span class="dot-live"></span>` : `<span class="stx-tri">${open ? "▾" : "▸"}</span>`}<b>${live ? `Working · ${count}` : count}</b>${open ? "" : `<span class="stx-sum"> · ${esc(turnSummary(steps))}</span>`}</button>${open ? `<button class="stx-all" data-turn-all="${esc(key)}">${all ? "Show less" : "Show all"}</button>` : ""}</div>`;
  // Folded, only the steps go: what it said to you stays in the chat (a fold never hides its words).
  if (!open) {
    const says = items.filter(([x]) => x.t === "say").map(([x, i]) => `<div class="stx-say msg" data-key="${esc(itemKey(turn, x, i))}">${md(x.text)}</div>`).join("");
    return first || fold || says ? `<div class="stx" data-key="${esc(bk)}">${first ? head : ""}${says}${fold}</div>` : "";
  }
  if (!items.length) return first || fold ? `<div class="stx" data-key="${esc(bk)}">${first ? head : ""}${fold}</div>` : "";
  let body = "", run = [], runKey = "";
  // A run of steps is keyed by its first step: a paragraph between steps starts the next run.
  const flush = () => { if (run.length) body += `<div class="stx-steps" data-key="run:${esc(runKey)}">${run.join("")}</div>`; run = []; };
  // Over: under the step that started it. A teammate stays in the list above the box instead.
  const finished = new Map(helpersOf(sid).filter((h) => !helperRunning(h) && !h.teammate).map((h) => [h.step, h]));
  items.forEach(([x, i]) => {
    if (x.t !== "say") { if (!run.length) runKey = x.id; run.push(stepLine(sid, x, all)); const h = finished.get(x.id); if (h) run.push(bgRow(sid, h)); return; }
    flush();
    // New words unroll top to bottom, quicker for short ones.
    const ms = Math.min(REVEAL_MS, 250 + x.text.length * 2);
    body += `<div class="stx-say msg" data-key="${esc(itemKey(turn, x, i))}" style="--rv:${ms}ms">${md(x.text)}</div>`;
  });
  flush();
  if (!first) return `<div class="stx" data-key="${esc(bk)}">${body}${fold}</div>`;
  return `<div class="stx" data-key="${esc(bk)}">${head}${turn.more ? `<div class="stx-more dim" data-key="more">${turn.more} earlier steps not shown</div>` : ""}${body}${fold}</div>`;
}
/** A turn's "After a stop hook": folded, what the hook made it do and write (a checklist, the commands it
 *  ran for it), so it never reads as the answer, live or after. "" when no hook sent it back. */
function hookFold(sid, turn, hooked) {
  if (!hooked.length) return "";
  const key = `${sid}|${turn.at_ms}`, open = hookOpen.has(key);
  const busy = hooked.some((x) => x.t === "step" && !x.done);
  const body = open ? hooked.map((x, i) => x.t === "step" ? `<div class="stx-steps">${stepLine(sid, x, false)}</div>` : `<div class="stx-say msg" data-key="${esc(itemKey(turn, x, i))}">${md(x.text)}</div>`).join("") : "";
  return `<div class="follow stx-hook" data-key="hook"><button class="thread-toggle" data-hook-fold="${esc(key)}">${busy ? `<span class="dot-live"></span>` : ""}After a stop hook ${open ? "▴" : "▾"}</button>${open ? `<div class="follow-body">${body}</div>` : ""}</div>`;
}
/** A turn since your last message (`since`) has its "After a stop hook" in the chat, so the reply's own copy
 *  of it, from its card, isn't drawn as well. Since, not the last turn only: a background task reporting
 *  after the hook starts a turn of its own, and the hook's part stays in the one before. */
const hookInChat = (sid, since) => stepsMode() !== "hidden" && !!stepFeeds.get(sid)?.turns?.some((t) => t.hooked_ms && t.hooked_ms >= since);
/** The chat with each turn's steps put in by time: after your message, before the reply. `extra`:
 *  more lines to place by time (what you chose on a decision, when you chose it). */
function withSteps(sid, s, ex, rows, extra = []) {
  const f = sid && stepFeeds.get(sid);
  const first = ex[0]?.at_ms ?? 0;
  const turns = f?.turns || [];
  const lastTurn = turns.at(-1);
  // Its replies, already in the chat: a Stop hook that made it carry on leaves the answer mid-turn.
  const said = new Set(ex.filter((e) => e.role === "agent").map((e) => e.text.trim()));
  // A turn you wrote into while it ran is split at your messages, so the chat stays in time order.
  const blocks = turns
    .filter((t) => (t.items.at(-1)?.at_ms ?? t.at_ms) >= first || (!ex.length && t === lastTurn))
    .flatMap((t) => {
      const start = t.items[0]?.at_ms ?? t.at_ms, end = t.items.at(-1)?.at_ms ?? start;
      const next = turns[turns.indexOf(t) + 1]?.at_ms ?? Infinity;
      const replied = ex.some((e) => e.role === "agent" && e.at_ms >= start && e.at_ms < next);
      // Open: the newest turn, and while that one works, the one before it too: the turn you were just
      // reading doesn't fold the moment your next message starts another.
      const latest = t === lastTurn || (s?.state === "working" && t === turns.at(-2));
      const cuts = ex.map((e) => e.at_ms).filter((a) => a > start && a <= end);
      const bounds = [-Infinity, ...cuts, Infinity];
      return cuts.concat([null]).map((_, k) => ({
        at: k === 0 ? start : cuts[k - 1] + 0.5,
        html: stepsBlock(sid, t, t === lastTurn && s?.state === "working", latest, { from: bounds[k], to: bounds[k + 1] }, said, replied),
      }));
    })
    .concat(extra)
    .filter((b) => b.html)
    .sort((a, b) => a.at - b.at);
  if (!blocks.length) return rows;
  const out = [];
  let b = 0;
  ex.forEach((e, n) => { while (b < blocks.length && blocks[b].at < e.at_ms) out.push(blocks[b++].html); out.push(rows[n]); });
  while (b < blocks.length) out.push(blocks[b++].html);
  return out;
}
/** Agent replies left open: the newest few, so the one you were reading doesn't fold the moment the next lands. */
const OPEN_REPLIES = 3;
/** Agent: full-width prose. You: a compact tinted bubble on the right. Older agent turns fold to 3 lines.
 *  Another session: a striped bubble saying which one (sent by its agent, or forwarded by you). */
/** `paged`: the Active pane's chat, which loads earlier messages as you scroll up. */
/** "Not sent" for one of the agent's own screens, which Cue says to run in a terminal: a button that does, in Cue's
 *  own terminal in the session's folder (on this Mac only: one on a machine is run there). */
function runHereBtn(via, s) {
  const line = /run (.+?) in a terminal in /.exec(via)?.[1];
  if (!line || !s?.cwd || s.machine) return "";
  return ` <button class="cv-pill run-here" data-act="run-here" data-line="${esc(line)}" data-cwd="${esc(s.cwd)}">Run it in a terminal</button>`;
}

function chatHtml(it, s, harness, paged = false) {
  const ex = conversation(it, s);
  const lastAgent = ex.map((e) => e.role).lastIndexOf("agent");
  const agentAt = ex.flatMap((e, n) => (e.role === "agent" ? [n] : []));
  const recent = new Set(agentAt.slice(-OPEN_REPLIES));
  const project = s?.project || it?.project || "";
  const sid = s?.session_id || it?.session_id;
  // What you chose on a decision sits in the chat when you chose it (later steps come below it).
  const K = keyer();
  const chose = it && !isPending(it) && it.resolved_ms ? [{ at: it.resolved_ms, html: keyed(resultLine(it), "chose") }] : [];
  const ran = sid ? (ranLog.get(sid) || []).map((e, n) => ({ at: e.at, html: keyed(ranCard(sid, e, n), K(`ran:${e.at}`)) })) : [];
  const restarts = sid ? (state.restarted?.[sid] || []).map((r, n, all) => ({ at: r.at_ms, html: keyed(restartLine(sid, r, n === all.length - 1), K(`rs:${r.at_ms}`)) })) : [];
  // A tracked session you come back to: "Since you looked" where the new part starts (about.js), no call.
  const since = sid ? aboutSince(sid, ex) : null;
  if (since) restarts.push({ at: since.at, html: keyed(since.html, "since") });
  const team = s?.team || [];
  const reviews = reviewReplies(ex, (name) => [...team].reverse().find((m) => m.name === name)?.review || "");
  const rows = withSteps(sid, s, ex, ex.map((e, n) => keyed((() => {
    if (e.role === "peer") return peerBubble(e.from || "another agent", "", e.at_ms, e.text);
    // What Cue sent the lead for its crew: Cue's bubble on the right, never yours (team.js).
    const cue = e.role === "you" && cueSaid(e.text);
    if (cue) return cueSaidLine(cue, `${sid}:${e.at_ms}`, e.text, `${ago(e.at_ms)} ago`);
    // A crew member's review (the lead's reply to Cue's request): one line, its findings under the member's row.
    if (reviews.has(n) && sid) return reviewLine(sid, reviews.get(n), stripStatus(e.text));
    const fwd = e.role === "you" && FORWARDED.exec(e.text);
    if (fwd) return peerBubble(fwd[2], "forwarded by you", e.at_ms, fwd[1], fwd[3]);
    if (e.role === "you") {
      const how = lastSent && lastSent.sid === (s?.session_id || it?.session_id) && e.text.startsWith(lastSent.text) ? lastSent.via : "";
      // "queued via iTerm session: it reads this…" read as "via queued via…": say it plainly.
      const via = !how ? "" : /^queued/.test(how) ? " · queued, it reads this at its next step" : ` · via ${esc(how)}`;
      // Typed into its terminal, but it never became a message: say so where you'll see it.
      const unsent = e.unsent ? `<div class="cv-unsent">⚠ This didn't go through as a message. It may have run as a command, or still be in its box.${sid ? ` <button data-act="resend" data-sid="${esc(sid)}" data-resend="${esc(e.text)}">Send again</button>` : ""}</div>` : "";
      // Typed, and Cue hasn't yet seen it take it: a ring until it does (or until Cue can say it didn't).
      // Past half a minute it's no longer "any moment now": say what Cue knows, which is nothing yet.
      const wait = !e.pending ? "" : Date.now() - e.at_ms > 30_000 ? " · Cue hasn't seen it take this yet" : " · waiting for it to take this";
      // Sent now right after the one above, before it answered: the agent got both as one (sessions.rs note_with).
      const joined = e.joined ? " · went to the agent with your message above, as one" : "";
      return `<div class="cv-you ${e.unsent ? "unsent" : ""} ${e.pending ? "pending" : ""}">${yourBubble(e, `${sid}:${e.at_ms}`)}${thumbs(e.images)}<div class="cv-meta">You · ${ago(e.at_ms)} ago${via}${wait}${joined}</div>${unsent}</div>`;
    }
    const key = `${s?.session_id || it?.id}:${e.at_ms}`;
    const long = e.text.length > 280 || e.text.split("\n").length > 4;
    const folded = !recent.has(n) && long && !openMsgs.has(key);
    const follow = it?.followup && e.text === it.message && !hookInChat(sid, ex.findLast((y) => y.role === "you" && y.at_ms < e.at_ms)?.at_ms ?? 0) ? followHtml(it) : "";
    fwdMsgs.set(key, { text: addOn.plain(e.text), from: project, fromSid: sid });
    const said = addOn.reply(sid, stripStatus(e.text));
    return `<div class="cv-agent ${folded ? "folded" : ""} ${n === lastAgent ? "last" : ""}"><div class="cv-meta">${esc(agentName(harness))}${machTag(it?.machine || s?.machine)} · ${ago(e.at_ms)} ago${n === lastAgent ? toldMeta(s) : ""}</div>
      <div class="msg cv-text">${md(said.text, { runIn: s?.session_id })}</div>${said.after}${thumbs(e.images)}${localImages(e.text)}
      <div class="cv-acts">${sid ? listenBtn(key) : ""}${long && paged && sid ? `<button class="cv-pill" data-read="${esc(key)}">${BOOK_ICON}Read</button>` : ""}${!recent.has(n) && long ? `<button class="cv-more" data-msg="${esc(key)}">${folded ? "Show all" : "Fold"}</button>` : ""}${n === lastAgent ? actReplyHtml(s, e.at_ms) : ""}<button class="cv-fwd" data-fwd="${esc(key)}">↗ Send to…</button></div>${follow}</div>`;
  })(), K(`${e.role[0]}:${e.at_ms}`))), chose.concat(ran, restarts));
  for (const o of outbox) if (o.sid === (s?.session_id || it?.session_id) && !landed(o)) rows.push(`<div class="cv-you ${o.via ? "" : "sending"}" data-key="${esc(K(`out:${o.at}`))}"><div class="cv-you-text">${esc(o.text).replace(/\n/g, "<br>")}</div>${o.images.length ? `<div class="thumbs">${o.images.map((im) => `<span class="thumb"><img src="${esc(im.data)}" alt=""/></span>`).join("")}</div>` : ""}<div class="cv-meta" data-sid="${esc(o.sid)}">${/^kept in Cue/.test(o.via || "") ? "Kept in Cue · it goes when this turn ends" : /^not sent: /.test(o.via || "") ? `Not sent · ${esc(o.via.slice(10))}${runHereBtn(o.via, s)}` : o.via ? `Sent · via ${esc(o.via)}` : esc(sendingMeta(o.sid))}</div></div>`);
  // Sent while it worked: Cue keeps it until the turn ends. Yours to take back (Edit, Esc) or send now.
  // Reviews you asked for while this lead was busy: each goes, in order, once it's free (Skip takes one back).
  for (const q of (s && state.review_queue?.[s.session_id]) || []) rows.push(keyed(cueSaidLine({ html: `Cue will ask for <b>${esc(q.name)}</b>'s review` }, "", "", `waits for the lead to be free · <button class="q-now" data-act="unqueue-review" data-exec="${esc(q.exec)}">Skip</button>`), K(`rq:${q.exec}`)));
  // A kept message that couldn't be sent says so (and why), rather than promising it goes when the turn ends.
  const heldWhy = s?.held?.unsent ? (s.held.delivery || []).map((d) => d.what).filter((w) => w.startsWith("couldn't send: ")).pop() : "";
  if (s?.held) rows.push(`<div class="cv-you queued held" data-key="held">${yourBubble(s.held, `${s.session_id}:held`, qPhase(), true)}${thumbs(s.held.images)}<div class="cv-meta">${heldWhy ? `Couldn't send it: ${esc(heldWhy.slice(15))}` : "Kept in Cue · it goes when this turn ends"} · <button class="q-now" data-act="held-edit" data-sid="${esc(s.session_id)}" title="Back into the box to change it (Esc)">Edit</button> · <button class="q-now" data-act="held-now" data-sid="${esc(s.session_id)}">Send now <kbd>⌘↵</kbd></button></div></div>`);
  if (s?.queued) rows.push(`<div class="cv-you queued" data-key="queued">${yourBubble(s.queued, `${s.session_id}:queued`, qPhase(), true)}${thumbs(s.queued.images)}<div class="cv-meta">Queued · it reads this when it finishes the current step · <button class="q-now" data-act="send-now" data-sid="${esc(s.session_id)}">Send now <kbd>⌘↵</kbd></button></div></div>`);
  return rows.length ? (paged && sid ? keyed(olderRow(sid, harness), "older") : "") + rows.join("") : ran.length ? ran.map((r) => r.html).join("") : `<div class="dim cv-empty" data-key="empty">No messages yet in this session.</div>`;
}
/** In the chat where a restart (after an MCP change) ended: back, with which servers and how long it took,
 *  or why it didn't finish. The latest one also says how each server did, once Claude Code has said
 *  (logs/mcp.rs with_live: after its next message); until then, just their names. */
function restartLine(sid, r, latest) {
  const when = `<span class="grow"></span><span class="dim nowrap">${ago(r.at_ms)} ago</span>`;
  if (r.error) return `<div class="cv-result no"><span class="sent-check">✕</span><span>The restart didn't finish: ${esc(r.error)}</span>${when}</div>`;
  const n = r.servers?.length;
  const title = r.servers ? `Restarted with ${n} MCP server${n === 1 ? "" : "s"}` : "Restarted with the folder's MCP servers";
  if (latest) mcpLoad(sid);
  const d = latest ? mcpCache.get(sid)?.data : null;
  const known = d?.live_known ? (d.servers || []).filter((m) => (r.servers ? r.servers.includes(m.name) : m.selected ?? m.enabled)) : null;
  const list = known?.length ? known.map((m) => `<span class="rs-sv-it"><b>${esc(m.name)}</b>${mcpLiveWord(m, sid)}</span>`).join("")
    : r.servers?.length ? r.servers.map((s) => `<span class="rs-sv-it"><b>${esc(s)}</b></span>`).join("") : "";
  const later = latest && !known?.length ? `<div class="rs-sv">Whether each one connected shows after its next message</div>` : "";
  return `<div class="cv-result rs-line"><span class="sent-check">✓</span><span>${title}<span class="dim"> · ${r.secs}s</span></span>${when}${list ? `<div class="rs-sv">${list}</div>` : ""}${later}</div>`;
}
/** A turn's changed files, one per line, for the pill's tooltip. */
const changesTip = (ch) => ch.files.map((f) => `${f.path}  +${f.add} −${f.del}`).join("\n");
/** Under the chat: where the session is now, and while any of its background agents run, their tray, whatever
 *  state it's in (working on your message, your turn, waiting on them), so they never drop out of sight. */
function statusLine(it, s) {
  const tray = s?.session_id ? agentTray(s.session_id) : "";
  // Waiting on them: the tray says it all, unless it's compacting (or just did): that line shows above it.
  if (tray && s.state === "agent" && s.helpers && !s.compacting_ms && !s.compacted_ms) return tray;
  // A hand-off on its way, a note written ahead, a cache gone (insights.js): lines here, never a bar on the box.
  return (s?.session_id ? handoffStatus(s.session_id, s) : "") + whereNow(it, s) + tray;
}
/** A session's latest status list while it's on for the session: { status, key, sw } (`sw` as status.js takes it).
 *  The latest reply that has one: a reply without a list (a spoken answer, a short one with no tools, a note
 *  mid-turn) keeps the one before it in view. */
function latestStatus(it, s) {
  const sid = s?.session_id || it?.session_id, on = statusOn(sid);
  let last = null, status = { items: [], waiting: "" };
  if (on) for (const e of conversation(it, s).filter((e) => e.role === "agent").reverse()) {
    const p = parseStatus(e.text);
    if (p.items.length) { last = e; status = p; break; }
  }
  const key = status.items.length ? `${sid}:${last.at_ms}` : `${sid}:${on ? "on" : "off"}`;
  return { status, key, sw: { sid, on, working: s?.state === "working" } };
}
/** A session's ring (status.js), in the box's row ahead of the mic: the panel's count and its put away / bring back,
 *  or (no list) its switch. */
function statusChip(it, s) {
  const { status, key, sw } = latestStatus(it, s);
  return statusChipHtml(status, key, undefined, { ...sw, shown: !statusPanelView(sw.sid).away });
}
/** A Claude Code session's list above its chat, under its tabs (status_panel.js), while the status list is on; its
 *  switch says what the list has cost this session (statuslist.rs spent, the same estimate as Settings'). */
function statusPanel(it, s) {
  if (s?.harness !== "claude" || !statusShown()) return "";
  const { status, key, sw } = latestStatus(it, s);
  const own = state.status_spent?.sessions?.[sw.sid];
  const spent = own?.replies ? `${own.usd < 0.01 ? "" : "about "}${aboutMoney(own.usd)} so far` : "";
  const bg = backgroundOn(sw.sid), agents = agentsOf(sw.sid), live = agents.filter(agentLive).length;
  // The agents each step runs on, found by the step's words in the agent's description (status.js agentOfStep).
  const known = bg ? agents.map((a) => ({ text: [a.h?.desc, a.name].filter(Boolean), mark: agentMark(a), name: a.name })) : [];
  return statusPanelHtml(status, { ...sw, spent, key, background: { on: bg, n: agentCount(sw.sid).n, live: live > 0 }, agents: known }, statusPanelView(sw.sid));
}
/** "Tell me" on the turn that just ended: on its reply's meta line (no line of its own under the chat), the
 *  tada and how long it took, until the next turn starts. While it runs, the tada in place of the title's dot
 *  says Cue is watching; nothing else. */
function toldMeta(s) {
  if (!s?.told_ms || atWork(s)) return "";
  return ` · <span class="told-meta">${tadaIcon(11, true)}${s.told_ms >= 1000 ? `took ${tookText(s.told_ms)}` : "done"}</span>`;
}
/** How long a turn ran: "45s", "38m", "1h 52m" (as its notification says it). */
const tookText = (ms) => { const sec = Math.floor(ms / 1000); return sec < 60 ? `${sec}s` : sec < 3600 ? `${Math.floor(sec / 60)}m` : `${Math.floor(sec / 3600)}h ${Math.floor(sec % 3600 / 60)}m`; };
/** One line under the chat: where the session is now. */
function whereNow(it, s) {
  // Started in a folder Claude Code doesn't trust yet: it's asking in its terminal before it does anything.
  if (s?.trust_ms) return `<div class="cv-status stuck"><span class="lim-dot"></span><span><b>Claude Code is asking whether you trust ${esc(homeless(s.cwd) || "this folder")}.</b> It won't start until you answer.</span><button class="btn small primary" data-act="trust" data-sid="${esc(s.session_id)}">Trust folder</button></div>`;
  // Waiting for Enter on a screen before Claude Code starts ("Security notes … Press Enter to continue").
  if (s?.enter_ms) return `<div class="cv-status stuck"><span class="lim-dot"></span><span><b>Claude Code is waiting for Enter in its terminal before it starts.</b> Nothing runs until then.</span><button class="btn small primary" data-act="continue-start" data-sid="${esc(s.session_id)}">Continue</button></div>`;
  // Working, but Claude Code can't reach the API (or the proxy in front of it) and keeps trying: minutes of a
  // spinner in its terminal otherwise. What failed, which try; Stop ends the turn instead of waiting it out.
  if (s?.state === "working" && s.retry) {
    const r = s.retry, head = r.network ? "Can't reach the API." : "The API returned an error.";
    return `<div class="cv-status stuck"><span class="lim-dot"></span><span><b>${head}</b> ${esc(r.error)}. Claude Code is trying again${r.max ? ` (${r.attempt} of ${r.max})` : ""}; messages you send wait until it gets through or gives up.</span><button class="btn small deny" data-act="interrupt" data-sid="${esc(s.session_id)}">Stop</button></div>`;
  }
  // Working, but nothing new from it in a while: a command waiting for input in its terminal, or hung.
  if (s?.state === "working" && s.stuck_ms) return `<div class="cv-status stuck"><span class="lim-dot"></span><span><b>No new output for <span data-ago="${s.stuck_ms}">${ago(s.stuck_ms)}</span>.</b> ${esc((s.doing || "Thinking").replace(/…$/, ""))}. If it's waiting for input, it's in its terminal.</span></div>`;
  // Compact (⋯) typed /compact: it's summarizing, until Claude Code says it's done.
  if (s?.compacting_ms) return `<div class="cv-status"><span class="dot-live"></span>Compacting: summarizing the conversation to free up context<span class="dim"> · <span data-ago="${s.compacting_ms}">${ago(s.compacting_ms)}</span></span></div>`;
  // It finished compacting (until the next turn starts).
  if (s?.compacted_ms && s.state !== "working") return `<div class="cv-status"><span class="ok-dot"></span>Compacted: the conversation was summarized and its context freed<span class="dim"> · <span data-ago="${s.compacted_ms}">${ago(s.compacted_ms)}</span> ago</span></div>`;
  // What it's doing right now (its latest step), else the prompt it's on.
  if (s?.state === "working" && s.doing) return `<div class="cv-status"><span class="dot-live"></span><span class="cv-what">${esc(s.doing)}</span><span class="dim"> · <span data-ago="${s.doing_ms}">${ago(s.doing_ms)}</span></span>${tipFor(it, s, "working")}</div>`;
  // Nothing in its transcript yet (it's thinking; Claude Code writes a message once it's whole): a live
  // count since the turn started, so a long think doesn't look stuck.
  if (s?.state === "working") return `<div class="cv-status"><span class="dot-live"></span><span class="cv-what">Working${s.prompt ? ` on: ${esc(s.prompt.length > 120 ? s.prompt.slice(0, 120) + "…" : s.prompt)}` : ""}</span><span class="dim"> · <span data-ago="${s.since_ms}">${ago(s.since_ms)}</span></span>${tipFor(it, s, "working")}</div>`;
  if (s?.state === "waiting" || s?.state === "deciding") return `<div class="cv-status"><i class="sw z live"></i>Waiting on you · ${ago(s.since_ms)}${tipFor(it, s, "waiting")}</div>`;
  // A lead whose teammates wait on you, or that waits on teammates at work: named, not "1 background agent".
  const crew = s?.state === "agent" && s.session_id ? crewWaitLine(s.session_id) : "";
  if (crew) return `<div class="cv-status">${/^<span class="cv-what">Waiting on you/.test(crew) ? `<i class="sw z live"></i>` : ""}${crew}${tipFor(it, s, "working")}</div>`;
  if (s?.state === "agent") return `<div class="cv-status">${s.helpers ? `<span class="dot-live"></span><span class="cv-what">Waiting on ${esc(s.driven_by)}</span>` : `<span class="cv-what">Idle, waiting on ${esc(s.driven_by || "another agent")}</span>`}${tipFor(it, s, "working")}</div>`;
  if (s?.state === "limited" && s.limit) {
    const l = s.limit, what = s.prompt ? `“${esc(s.prompt.length > 80 ? s.prompt.slice(0, 80) + "…" : s.prompt)}”` : "Your last message";
    if (lifted(l)) return `<div class="cv-status limit"><span class="lim-dot back"></span><span><b>${esc(scopeName(l))} is back.</b> ${what} didn't run.</span><button class="btn small" data-usage="resend-one" data-sid="${esc(s.session_id)}">Resend</button></div>`;
    const when = l.resets_ms ? ` · resets ${clockAt(l.resets_ms)}${l.resets_ms - now() < 20 * 3600000 ? `, in ${untilText(l.resets_ms - now())}` : ""}` : "";
    const head = l.scope === "all" ? "Out of usage" : `${esc(l.scope)} used up`;
    const tail = l.scope === "all" ? "Cue will tell you when it resets." : "Switch models with /model to keep going.";
    return `<div class="cv-status limit"><span class="lim-dot"></span><span><b>${head}</b>${when}. ${what} didn't run. ${tail}</span></div>`;
  }
  // Cue quit (restarted, reinstalled) while a session it hosts was mid-turn: that turn ended there, not by you.
  if (s?.state === "stopped" && s.cut_by_restart) return `<div class="cv-status">Cue restarted mid-turn, so that turn ended · your next message starts it again</div>`;
  if (s?.state === "stopped") return `<div class="cv-status">Stopped by you · it's at its prompt</div>`;
  if (!s && it && !isPending(it)) return `<div class="cv-status">This session has ended.</div>`;
  return "";
}
/** After a decision: what you chose. Replies need no line, they're already in the chat. */
function resultLine(it) {
  if (!it || isPending(it) || !it.resolved_ms || it.kind === "waiting") return "";
  return `<div class="cv-result ${outcomeClass(it)}"><span class="sent-check">${outcomeClass(it) === "no" ? "✕" : "✓"}</span><span>${esc(outcomeText(it))}</span><span class="grow"></span><span class="dim nowrap">${ago(it.resolved_ms)} ago</span></div>`;
}
function nextBar() {
  const n = nextUp();
  if (!n) return "";
  // × only on a finished turn; a decision would just hide something the agent is still blocked on.
  const clear = n.kind === "waiting" ? `<button class="next-clear" data-act="dismiss" data-id="${esc(n.id)}" title="Take it off Waiting" aria-label="Take it off Waiting">×</button>` : "";
  return `<div class="next-bar" data-act="next" role="button" tabindex="0"><span class="next-label">Next</span>${badge(n.harness)}<span class="next-proj">${esc(n.project)}</span><span class="next-what">${esc(plain(summary(n)))}</span>${clear}<kbd>N</kbd></div>`;
}

/** The chat header's ⋯: what's used now and then. Compact (any agent's /compact), and Decide later / Back to Waiting. */
let moreFor = null, movingProj = false;
/** + New project… at the foot of Move to project: the session whose list has the name box open (draft
 *  `newproj:<sid>`), and what the box last said back (a name already taken). */
let newProjFor = null, newProjNote = "";
/** ⋯ → Move to project: the projects it can show under instead, its folder's first, then + New project…. */
function projMoveList(sid) {
  const own = folderProject(sid), now = sessionOf(sid)?.project ?? findItem(active?.id)?.project ?? own;
  const names = [...new Set([own, ...projectsKnown().map(([p]) => p)])].filter((p) => p && p !== now);
  const rows = names.map((p) => `<button data-move-proj="${esc(p)}" data-sid="${esc(sid)}">${projDot(p)}${esc(projLabel(p))}${p === own ? "<span>Its folder's own</span>" : ""}</button>`).join("");
  const add = newProjFor === sid
    ? `<div class="pm-new"><input class="pm-new-in" data-text="newproj:${esc(sid)}" maxlength="40" placeholder="Name the new project" spellcheck="false" value="${esc(draft(`newproj:${sid}`).text)}"/>${newProjNote ? `<div class="pm-note">${esc(newProjNote)}</div>` : ""}</div>`
    : `<button class="pm-add" data-new-proj="${esc(sid)}">+ New project…</button>`;
  return `<div class="more-menu proj-move"><div class="pm-title">Move to project</div><div class="pm-note">This session only</div>${rows}${add}</div>`;
}
/** Enter in + New project…'s box: this session goes to a project of that name. One that's already there (its
 *  name or the name you gave it) is said instead, so two never quietly become one. */
function submitNewProj(sid) {
  const name = draft(`newproj:${sid}`).text.trim();
  if (!name) return;
  const taken = takenProject(name, [folderProject(sid)]);
  if (taken) { newProjNote = `${projLabel(taken)} is already a project: pick it above`; return renderMain(); }
  draft(`newproj:${sid}`).text = "";
  newProjFor = null; newProjNote = ""; moreFor = null; movingProj = false;
  moveToProject(sid, name);
  // The window goes to the new project, the moved session still open in it.
  setProjFilter(name);
  renderMain();
}
function moreMenu(sid, s, closable = false) {
  const open = moreFor === sid;
  if (open && movingProj) return `<span class="more-wrap"><button class="btn more-btn" data-more="${esc(sid)}" aria-label="More" aria-expanded="true">⋯</button>${projMoveList(sid)}</span>`;
  if (open && mcpPicking) return `<span class="more-wrap"><button class="btn more-btn" data-more="${esc(sid)}" aria-label="More" aria-expanded="true">⋯</button>${mcpPicker(sid)}</span>`;
  const busy = s?.state === "working";
  const later = isParked(sid) ? `<button data-park="${esc(sid)}:0">Back to Waiting</button>` : `<button data-park="${esc(sid)}:1">Decide later<span>Move it to Need to decide while you think it over</span></button>`;
  const items = !open ? "" : `<div class="more-menu">
      <button data-cmd="compact" data-sid="${esc(sid)}" ${busy ? "disabled" : ""}>Compact<span>${busy ? "When this turn ends" : "Summarize the conversation to free up context"}</span></button>
      ${tellItem(sid, s)}
      ${handoffMenu(sid, s)}
      ${addOn.chatMenu(sid, s)}
      <button data-move-ask="${esc(sid)}">Move to project…<span>Show its folder's sessions under another project in Cue; the agent stays in its folder</span></button>
      ${mcpItem(sid)}
      ${restartItem(sid, s)}
      ${forkItem(sid, s)}
      ${moveOutItem(sid, s)}
      ${later}${parkMenuItem(sid, s, closable)}
      <button data-delete-ask="${esc(sid)}" ${busy ? "disabled" : ""}>Delete…<span>${busy ? "When this turn ends" : "Gone for good: from Cue, and its conversation, so it can't be resumed"}</span></button></div>`;
  return `<span class="more-wrap"><button class="btn more-btn" data-more="${esc(sid)}" aria-label="More" aria-expanded="${open}">⋯</button>${items}</span>`;
}
/** ⋯ → Move to terminal, for a session Cue runs (hosted, or in Cue's own terminal): resumed where you run agents
 *  yourself (ipc/move_out.rs). Only for moving away from Cue, so it's here, not a button, and says so. Not while it
 *  works or asks you something. */
function moveOutItem(sid, s) {
  if (!s || !(s.hosted || (inTmux(sid) && !s.machine))) return "";
  const why = s.state === "working" ? "When this turn ends" : s.state === "deciding" ? "Once you've answered what it asks" : "";
  const where = s.hosted && s.machine ? `in tmux on ${machName(s.machine)}` : `in a ${termName()} tab`;
  return `<button data-act="move-term" data-sid="${esc(sid)}" ${why ? "disabled" : ""}>Move to terminal<span>${why || `Only if you're moving away from Cue: the conversation goes on ${esc(where)}, and Cue stops running it`}</span></button>`;
}
/** ⋯ → Restart session: stopped and resumed with the same conversation, so it loads what it reads only at
 *  start (a skill added since, MCP servers, settings). Covered while it restarts, as an MCP restart is. */
function restartItem(sid, s) {
  if (!s || !["claude", "codex", "pi"].includes(s.harness) || !s.cwd) return "";
  const busy = s.state === "working";
  return `<button data-restart="${esc(sid)}" ${busy ? "disabled" : ""}>Restart session<span>${busy ? "When this turn ends" : "Same conversation, fresh start: picks up new skills, MCP servers and settings"}</span></button>`;
}
/** ⋯ → Fork, for a Claude Code session: its conversation goes on as a new session too (forkBtw). */
function forkItem(sid, s) {
  if (s?.harness !== "claude") return "";
  return `<button data-fork="${esc(sid)}" ${btwForking != null ? "disabled" : ""}>Fork<span>Its conversation goes on as a new session too, with -fork on its name; this one carries on as it is</span></button>`;
}
/** ⋯ → MCP servers…, for a Claude Code session on this Mac: how many it runs with. A hosted one's (here or on a
 *  machine) is Claude Code's own list (chat/mcp_live.js). */
function mcpItem(sid) {
  const s = sessionOf(sid);
  if (isHostedClaude(sid)) return mcplItem(sid);
  if (s?.harness !== "claude" || s?.machine) return "";
  mcpLoad(sid);
  const n = mcpOnCount(sid);
  return `<button data-mcp-ask="${esc(sid)}">${n == null ? "MCP servers…" : `MCP servers · ${n} on`}<span>Pick which it runs with; it restarts with them</span></button>`;
}
/** "claude-opus-5-5" → "Opus 5.5", "gpt-5.6-terra" → "GPT-5.6 Terra", "deepseek-v4-pro" → "DeepSeek V4 Pro". */
function prettyModel(id) {
  const m = String(id || "").replace(/\[.*\]$/, "").replace(/^.*\//, "");
  const claude = /^claude-([a-z]+)-(\d+)(?:-(\d+))?/.exec(m);
  if (claude) return `${claude[1][0].toUpperCase()}${claude[1].slice(1)} ${claude[2]}${claude[3] ? `.${claude[3]}` : ""}`;
  return m.split("-").map((w) => /^gpt$/i.test(w) ? "GPT" : /^deepseek$/i.test(w) ? "DeepSeek" : /^v\d/i.test(w) ? w.toUpperCase() : w[0] ? w[0].toUpperCase() + w.slice(1) : w)
    .join(" ").replace(/^GPT (\d)/, "GPT-$1");
}
/** How full the session's context is, in % (null until Cue knows its window: Claude Code's status line says it). */
const ctxPct = (m) => (m?.window ? Math.min(100, Math.round((m.context / m.window) * 100)) : null);
/** Just compacted and not at work again: its log's last count is from before, until a new reply brings a fresh one. */
const justCompacted = (s) => !!s?.compacted_ms && s.state !== "working";
/** The open session's model, how full its context is, how far it has grown, its cache and its cost so far (where
 *  the agent logs it), on the line under the chat's title; context as a small gauge. From its log. */
function metaInline(sid) {
  const m = sid && stepFeeds.get(sid)?.meta;
  if (!m || (!m.model && !m.context)) return "";
  const pct = ctxPct(m);
  const lvl = pct === null ? "" : pct >= 95 ? "full" : pct >= 80 ? "high" : "";
  const k = (n) => (n >= 1e6 ? `${(n / 1e6).toFixed(n % 1e6 ? 2 : 0)}M` : `${Math.round(n / 1000)}k`);
  const money = (c) => `$${c >= 100 ? Math.round(c) : c.toFixed(2)}`;
  const s = sessionOf(sid);
  const freed = justCompacted(s);
  // No share until Cue knows the window: the tokens alone, not a guess.
  const ctx = freed ? `<span class="mi-ctx">context freed</span>` : pct !== null ? `<span class="mi-ctx ${lvl}"><span class="ctx-bar"><i style="width:${pct}%"></i></span>${pct}% context</span>` : m.context ? `<span class="mi-ctx">${k(m.context)} context</span>` : "";
  // A fresh session due (insights.js handoffDue): how far it has grown, in amber.
  const grew = !freed && !state.skill_handoffs?.[sid] ? handoffGrew(sid) : 0;
  // Its cache's countdown (insights.js cacheMeta): the ring and the minutes, amber in its last 5; "cache gone" after.
  const cache = freed ? "" : cacheMeta(sid);
  // On a tight line they give way from the right, the cost first.
  const parts = [m.model ? `<b>${esc(prettyModel(m.model))}</b>` : "", ctx, grew ? `<span class="mi-grow">${Math.round(grew)}× its start</span>` : "", cache, m.cost != null ? money(m.cost) : ""].filter(Boolean);
  return `<span class="ap-meta">${parts.join(" · ")}</span>`;
}
/** The one thing to do now, on the chat's title row beside its state: Compact once its context is 80% full (not
 *  mid-turn; the same as ⋯ → Compact), then insights.js handoffChips (one chip, Fresh session ▾, whatever the state). */
function titleChips(sid) {
  const s = sessionOf(sid), pct = ctxPct(stepFeeds.get(sid)?.meta);
  const compact = pct !== null && pct >= 80 && s && s.state !== "working" && !s.compacting_ms && !justCompacted(s) ? `<button class="mi-compact" data-cmd="compact" data-sid="${esc(sid)}">${BULB}Compact</button>` : "";
  return compact + handoffChips(sid, s);
}
/** A quiet session open in Active: running, but started before Cue was connected, so it never
 *  reports to Cue. Read-only: what it's been doing, from its transcript, and how to make it a full one. */
let quietOpen = null;
function quietPane() {
  const q = (state.live || []).find((x) => x.session_id === quietOpen);
  if (!q) { quietOpen = null; return activePane(); }   // it ended (or reported in): back to the usual
  const f = stepFeeds.get(q.session_id);
  const busy = q.status === "busy";
  const turns = (f?.turns || []).slice(-8);
  const chat = turns.map((t, i) => quietTurn(q.session_id, t, busy && i === turns.length - 1, i === turns.length - 1, q.harness)).join("");
  return `<div class="active-pane ${busy ? "busy" : ""}">
    <div class="ap-head">${projBtn(q.project || projOf(q.cwd), "ap")}${badge(q.harness)}<span class="proj">${esc(q.name || baseName(q.cwd) || "session")}</span><span class="pill soft">${busy ? "working" : "quiet"}</span><span class="grow"></span>
      ${moveBtn(q.session_id, q.harness, busy ? "working" : "idle", "btn primary")}<button class="btn" data-sv="tab" data-sid="${esc(q.session_id)}">${openInLabel(null)}</button></div>
    ${subHead(q.cwd, "", q.session_id)}
    <div class="quiet-note">It started before Cue was connected, so Cue can show what it's doing but can't answer it yet. In its terminal, type <code>/hooks</code> once to pick up Cue's hooks (or restart it with <code>claude --resume</code>; the conversation carries on), and you can reply, allow and answer from Cue.</div>
    ${chatBox(`q|${q.session_id}`, chat || `<div class="dim cv-empty" data-key="empty">${f ? "Nothing in its transcript yet." : "Reading its transcript…"}</div>`)}</div>`;
}
/** One turn of a quiet session: your message, its steps, its reply (all from the transcript). */
function quietTurn(sid, t, live, latest, harness = "claude") {
  const you = t.prompt ? `<div class="cv-you" data-key="y:${t.at_ms}"><div class="cv-you-text">${linkify(esc(t.prompt)).replace(/\n/g, "<br>")}</div><div class="cv-meta">You · ${ago(t.at_ms)} ago</div></div>` : "";
  // Its reply: the words after its last step (the steps leave a finished turn's last words to the chat).
  const tail = [];
  if (!live) for (let k = t.items.length - 1; k >= 0 && t.items[k].t === "say"; k--) tail.unshift(t.items[k]);
  const said = tail.length ? addOn.reply(sid, stripStatus(tail.map((x) => x.text).join("\n\n"))) : null;
  const reply = said ? `<div class="cv-agent" data-key="a:${t.at_ms}"><div class="cv-meta">${esc(agentName(harness))} · ${ago(tail.at(-1).at_ms)} ago</div><div class="msg cv-text">${md(said.text)}</div>${said.after}</div>` : "";
  return you + stepsBlock(sid, t, live, latest) + reply;
}
