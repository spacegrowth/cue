// The Inbox brief's side column, its live and git parts: what's running now (the session, its agents, scripts and
// crew) and what its turn changed. Kept apart from ui/inbox.js, which loads first and calls these when it draws.

/** The menu a teammate's name opens (team.js), in the block its name is in; "" when it's another's. */
const inboxMateMenu = (sid, list) => (list.some((t) => mateKey(sid, t) === mateMenu) ? mateMenuOf(sid) : "");
/** What's at work in it right now, live (the steps feed, read every half second while it works; app.js followSteps):
 *  the session itself, on its step now; then its background agents (those that need you first) and the scripts it
 *  runs in the background, each with its newest step, as its chat's tray shows them (a click on one opens it in full,
 *  in its chat); then its crew at work, a line each as in its drawer (team.js mateOneRow: its name opens its menu, how
 *  long and what it's on, Stop or Diff), and its finished ones folded into "N done ›". A teammate that needs you is
 *  only in the main column (inboxCrewHtml). Only crew left, the block is "Its crew".
 *  { html, agents, scripts }: html "" when nothing is at work and it has no crew to show. */
function inboxLive(r) {
  const sid = r.sid, s = r.s, crew = typeof mateList === "function" ? mateList(sid) : [];
  const mates = new Set(crew.map((t) => t.agent_id));
  const agents = agentsOf(sid).filter((a) => !mates.has(a.id) && (a.state === "needs" || agentLive(a)));
  const scripts = helpersOf(sid).filter((h) => h.script && helperRunning(h)).sort((a, b) => a.started_ms - b.started_ms);
  const working = !!s && (s.state === "working" || s.state === "agent" || !!s.compacting_ms);
  const g = crew.length ? crewGroups(sid) : null;
  const team = g ? [...g.working.map((t) => mateOneRow(sid, t)), ...g.cue.map((t) => mateCueRow(sid, t)), ...g.lead.map((t) => mateOneRow(sid, t))] : [];
  const done = g ? mateDoneHtml(sid, g.done) : "";
  if (!working && !agents.length && !scripts.length && !team.length && !done) return { html: "", agents, scripts };
  const step = (inboxTurn(sid)?.items || []).filter((x) => x.t === "step" && !x.done).at(-1);
  const main = working ? `<div class="ib-live-main"><span class="dot-live"></span><span class="ib-live-who">${esc(agentName(s.harness || "claude"))}</span><span class="ib-live-what">${step ? inboxStepRun([step]).what : esc(inboxRunLines(r, true)[1] || "Thinking")}</span></div>` : "";
  const rows = [...agents.map((a) => (a.state === "needs" ? agentRow(a).replace(/data-helper="/g, 'data-inbox-helper="') : inboxLiveRow(sid, a.h, a.type))), ...scripts.map((h) => inboxLiveRow(sid, h, "script"))].join("");
  const n = (working ? 1 : 0) + agents.length + scripts.length + team.length;
  // Its name's menu, here when it's one of these; its steps open in its chat, as a click on a row above does.
  const waits = g ? mateWaits(sid) : [], menu = g ? inboxMateMenu(sid, (sessionOf(sid)?.team || []).filter((t) => !waits.includes(t))).replace(/data-helper="/g, 'data-inbox-helper="') : "";
  const crewHtml = team.length || done ? `<div class="ib-live-crew">${team.join("")}${done}</div>` : "";
  return { html: `<div class="ib-blk ib-live${crewHtml ? " ib-mates" : ""}"><div class="ib-k">${n ? `Running now · ${n}` : "Its crew"}</div>${main}${rows ? `<div class="bg-rows">${rows}</div>` : ""}${crewHtml}${menu}</div>`, agents, scripts };
}
/** An agent or script at work, in two lines for the narrow side: its mark, name, kind and how long (amber when it
 *  looks stuck), then its newest step. */
function inboxLiveRow(sid, h, type) {
  const span = `<span data-helper-span="${h.started_ms}">${helperSpan(h)}</span>`;
  const quiet = helperStuck(h) ? ` · ${helperQuiet(h)}` : h.quiet === "tool" ? ` · ${helperLongRun(h)}` : "";
  return `<button class="ib-lv ${helperStuck(h) ? "stuck" : ""}" data-inbox-helper="${esc(`${sid}|${h.id}`)}"><span class="ib-lv-top">${bgIcon(h)}<span class="ib-lv-name">${esc(bgName(h))}</span>${liveDot(h)}</span>`
    + `<span class="ib-lv-meta">${esc(type)} · ${span}${quiet}</span><span class="ib-lv-step">${esc(bgLatest(h))}</span></button>`;
}
/** What its turn has changed so far, read from git against where the turn began: Cue clears a session's changes as a
 *  turn starts and counts them as it ends, so a question mid-turn would show none. Read once per turn and state
 *  (on open); null when Cue has no start for its turn. */
function inboxTurnDiff(sid) {
  const s = extSessionInfo(sid), key = s?.turn_from?.commit ? JSON.stringify([s.turn_from.commit, s.state]) : "";
  const had = inboxTurnDiffs.get(sid);
  if (!key) return null;
  if (had?.key === key) return had;
  const rec = { key, files: had?.files || null, bulk: had?.bulk || [] };
  inboxTurnDiffs.set(sid, rec);
  diffSince(s, s.turn_from).then((g) => { rec.files = g?.files || []; rec.bulk = g?.bulk || []; }, () => { rec.files = rec.files || []; })
    .then(() => { if (inboxTurnDiffs.get(sid) === rec && view === "inbox") renderMain(); });
  return rec;
}
/** The biggest change in a file: its largest hunk, from just before its first changed line, a few lines long. */
function inboxPeek(f) {
  const hunks = [];
  for (const l of f.lines || []) { if (l[0] === "h" || !hunks.length) hunks.push([]); hunks.at(-1).push(l); }
  const size = (h) => h.filter(([k]) => k === "a" || k === "d").length, big = hunks.sort((a, b) => size(b) - size(a))[0] || [];
  const at = Math.max(0, big.findIndex(([k]) => k === "a" || k === "d") - 1), head = big[0]?.[0] === "h" ? [big[0]] : [];
  const body = big.slice(Math.max(at, head.length), Math.max(at, head.length) + INBOX_PEEK_LINES);
  return [...head, ...body].map(([k, l]) => `<div class="${k}">${esc(l)}</div>`).join("");
}
/** "Changed this turn": its files with +/−, a peek of the biggest change, and the rest on its Diff tab. */
function inboxChangedBlock(r) {
  const d = inboxTurnDiff(r.sid), sid = esc(r.sid);
  const files = d?.files ? [...d.files].sort((a, b) => b.add + b.del - (a.add + a.del)) : (r.s?.changes?.files || []).map((f) => ({ ...f, lines: [] }));
  if (d && !d.files) return `<div class="ib-blk"><div class="ib-k">Changed this turn</div><div class="ib-dim">Reading git…</div></div>`;
  // Mid-turn with no start to read from, it can't be known: left out.
  const mid = r.kind === "ask" || r.kind === "run";
  if (!files.length) return d || !mid ? `<div class="ib-blk"><div class="ib-k">Changed this turn</div><div class="ib-dim">${mid ? "Nothing yet." : "No files changed."}</div></div>` : "";
  const add = files.reduce((n, f) => n + (f.add || 0), 0), del = files.reduce((n, f) => n + (f.del || 0), 0);
  const row = (f) => `<div class="ib-file"><code>${esc(f.path)}</code><span>${f.isNew ? `<span class="ib-dim">new</span> ` : ""}<span class="ib-add">+${f.add || 0}</span> <span class="ib-del">−${f.del || 0}</span></span></div>`;
  const peek = files[0].lines?.length ? `<div class="ib-peek"><div class="ib-peek-h"><code>${esc(files[0].path)}</code></div><div class="dv-lines">${inboxPeek(files[0])}</div></div>` : "";
  const rest = files.length > INBOX_FILES_MAX ? `<button class="ib-link" data-inbox-diff="${sid}">and ${files.length - INBOX_FILES_MAX} more files ›</button>` : "";
  return `<div class="ib-blk"><div class="ib-k">Changed this turn · ${files.length} file${files.length === 1 ? "" : "s"} <span class="ib-add">+${add}</span> <span class="ib-del">−${del}</span></div>
    <div class="ib-files">${files.slice(0, INBOX_FILES_MAX).map(row).join("")}</div>${peek}${rest}</div>`;
}
