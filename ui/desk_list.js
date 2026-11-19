// Cue UI desk — the Items list: holders (a lead and its crew, a session, an add-on), the groups by state, an item's row, finished items loaded as you scroll.
// Split from desk.js, which keeps the Desk's state and item basics; loaded after it (index.html, mini.html).
// ---------- holders: a lead (its crew from its team), a session on its own, or the add-on's ----------

/** The lead's teammates still at it or waiting (team.js mateList), or none for a session that isn't one. */
const deskCrew = (sid) => {
  // One row per name: a member the lead started again under the same name is one member, its newest run (one at work
  // before one that isn't).
  const all = typeof mateList === "function" ? mateList(sid) : [], pick = new Map();
  const rank = (t) => [t.state === "working" ? 1 : 0, t.started_ms || 0];
  for (const t of all) {
    const o = pick.get(t.name), [a, b] = rank(t), [c, e] = o ? rank(o) : [-1, -1];
    if (!o || a > c || (a === c && b > e)) pick.set(t.name, t);
  }
  return all.filter((t) => pick.get(t.name) === t);
};
/** A crew member at work (or its lead reviewing it). */
const deskMateBusy = (t) => t.state === "working" || t.state === "reviewing";
/** A crew member that gets its own row on the Desk: at work, asking you, or with uncommitted files (`uncommitted`,
 *  from git: hub/team_tidy.rs). */
const deskMateLive = (sid, t) => t.state !== "committed" && t.state !== "aside" && (deskMateBusy(t) || mateNeeds(sid, t) || !!t.uncommitted);
/** The leads whose "N done" fold you opened on the Desk: kept across draws and restarts (this window's storage). */
const deskFoldOpen = (() => { try { return new Set(JSON.parse(localStorage.getItem("cue.deskFold") || "[]")); } catch { return new Set(); } })();
/** A crew member of lead `lead` by its holder id or name. */
const deskMate = (lead, key) => (sessionOf(lead)?.team || []).find((t) => t.agent_id === key || t.call === key || t.name === key) || null;
/** The group an item is listed under: a crew member's item under its lead. */
const deskGroupKey = (h) => (!h ? "" : h.kind === "crew" ? h.lead_sid || h.sid : h.sid);
/** A holder's name: what the hub named it, else the session's name. */
const deskHolderName = (sid, fallback = "") => fallback || (sessionOf(sid) ? nameOf(sid, sessionOf(sid).project) : sid);
/** Whether session `sid` leads a crew, or is given items as a lead. */
const deskIsLead = (sid, items) => deskCrew(sid).length > 0 || items.some((i) => i.holder?.kind === "lead" || i.holder?.kind === "crew");
/** The items queued behind item `i` (after it, and those after them), not done, in order: what it unblocks. */
function deskUnblocks(i, items) {
  const out = [];
  let front = [i.id];
  while (front.length) {
    const next = items.filter((x) => x.state !== "done" && x.after && front.includes(x.after) && x.id !== i.id && !out.includes(x.id)).map((x) => x.id);
    out.push(...next);
    front = next;
  }
  return out;
}
/** "unblocks #154, #157, #160 +2 more" under a waiting row, or "". */
function deskUnblocksHtml(i) {
  const ids = deskUnblocks(i, deskOf(i.project)?.items || []);
  if (!ids.length) return "";
  return `<div class="dk-ub">unblocks ${ids.slice(0, 3).map((x) => `#${esc(x)}`).join(", ")}${ids.length > 3 ? ` +${ids.length - 3} more` : ""}</div>`;
}
/** Markdown on the item page and in its thread, with each file or folder named in `code` as a file chip (#305): an
 *  icon, its folder quiet (a long one by its last two) and its name in ink, at the words' size, as Slack shows a file. */
const DESK_FILE_SVG = '<svg class="dk-fchip-i" width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" aria-hidden="true"><path d="M3 1.5h4l2.5 2.5v6.5H3z"/><path d="M7 1.5V4h2.5"/></svg>';
const DESK_DIR_SVG = '<svg class="dk-fchip-i" width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" aria-hidden="true"><path d="M1.5 3h3.2l1 1.2h4.8v5.3h-9z"/></svg>';
/** A `code` span that names a file or folder: a path with a slash (tests/ui, ~/x, ./a), or a file name with an
 *  extension (desk.js). Not a command (spaces) or a site. */
const deskIsPath = (t) => !/\s/.test(t) && t.length < 200 && !/^[a-z]+:\/\//i.test(t) && !/^[\w-]+(\.[\w-]+)+\//.test(t)
  && (/^(?:~|\.{1,2})?\/?[\w.@+-]+(?:\/[\w.@+-]+)+\/?$/.test(t) || /^(?:~|\.{1,2})\/[\w.@+\/-]*$/.test(t) || /^[\w@+-][\w.@+-]*\.[A-Za-z][\w]{0,7}$/.test(t)) && !/^\d+(\.\d+)+$/.test(t);
function deskMd(text) {
  return md(text).replace(/<code>((?:(?!<\/?code>)[\s\S])*)<\/code>/g, (whole, inner) => {
    const t = inner.replace(/<[^>]+>/g, "");
    if (!deskIsPath(t.replace(/&amp;/g, "&"))) return whole;
    const dir = /\/$/.test(t) || !/\.[A-Za-z][\w]{0,7}$/.test(t.split("/").pop());
    const bare = t.replace(/\/$/, ""), cut = bare.lastIndexOf("/");
    // A long folder keeps its last two: …/src/hub/desk.rs.
    const parts = cut >= 0 ? bare.slice(0, cut + 1).split("/") : [];
    const folder = parts.length > 3 ? `…/${parts.slice(-3).join("/")}` : parts.join("/");
    const shown = `${folder ? `<span class="dk-fchip-d">${folder}</span>` : ""}<span class="dk-fchip-n">${bare.slice(cut + 1)}</span>`;
    // A path md() linked keeps its link (a click opens it), now around the whole chip.
    const link = /<a [^>]*data-path="[^"]*"[^>]*>/.exec(inner)?.[0];
    const chip = `${dir ? DESK_DIR_SVG : DESK_FILE_SVG}${shown}`;
    return link ? `${link.replace(/^<a /, '<a class="dk-fchip" ')}${chip}</a>` : `<span class="dk-fchip">${chip}</span>`;
  });
}
/** A question, an ask or a "done when" in one plain line: its line that asks (ends on "?"), else its first. */
function deskOneLine(t) {
  const lines = String(t || "").split(/\n+/).map((l) => plain(l)).filter(Boolean);
  return lines.findLast((l) => /\?$/.test(l)) || lines[0] || "";
}
/** What a tool it waits to run wants to do, in plain words (#298): the line Claude wrote for it ("Find where approvals
 *  render"); else a single command as it is (the row keeps it to two lines), and a chain of them as the programs it
 *  runs and where ("run cat, rm and grep in l2-i"). The full command is in the opened item. */
function deskAllowLine(i) {
  const req = deskItemRequest(i), ti = req?.tool_input || {};
  if (req?.kind !== "permission") return deskOneLine(i.ask?.text);
  if (typeof ti.description === "string" && ti.description.trim()) return plain(ti.description);
  if (typeof ti.command !== "string") return plain(summary(req));
  // Split on && ; | outside quotes: blank the quoted words first so a quoted "a|b" stays one word.
  const bare = ti.command.replace(/'[^']*'|"(?:\\.|[^"\\])*"/g, (q) => "_".repeat(q.length));
  const parts = [];
  let at = 0;
  for (const m of bare.matchAll(/&&|\|\||;|\||\n/g)) { parts.push(ti.command.slice(at, m.index)); at = m.index + m[0].length; }
  parts.push(ti.command.slice(at));
  const steps = parts.map((p) => p.trim().replace(/^(?:[A-Za-z_][A-Za-z0-9_]*=\S*\s+)+/, "")).filter(Boolean);
  const cd = steps.find((p) => /^cd\s/.test(p)), work = steps.filter((p) => !/^cd\s/.test(p));
  if (work.length <= 1) return plain(work[0] || ti.command);
  const dir = cd?.replace(/^cd\s+/, "").replace(/["']/g, "").split("/").filter(Boolean).at(-1);
  const progs = [...new Set(work.map((p) => p.split(/\s+/)[0].split("/").at(-1)))];
  const list = progs.length > 4 ? `${progs.slice(0, 3).join(", ")} and ${progs.length - 3} more` : progs.length > 1 ? `${progs.slice(0, -1).join(", ")} and ${progs.at(-1)}` : progs[0];
  return `run ${list}${dir ? ` in ${dir}` : ""}`;
}
/** What an item waiting on you asks you to do, in one plain line (#182): [verb, words], or null when it doesn't wait
 *  on you. Allow a tool, Pick one of its choices (or of its mocks), Answer its question; Try it once it's in the app
 *  (what you'll see, #382: its "done when" is the session's). */
function deskVerb(i) {
  if (i.state === "to_check" && !deskInstallWait(i)) return deskPickLine(i) ? ["Pick", deskPickLine(i)] : ["Try it", deskOneLine(i.see) || i.title];
  if (i.state !== "needs_you") return null;
  const a = i.ask;
  if (a) return [a.kind === "permission" ? "Allow" : (a.choices || []).length > 1 ? "Pick" : "Answer", a.kind === "permission" ? deskAllowLine(i) : deskOneLine(a.text)];
  const pick = deskPickLine(i);
  if (pick) return ["Pick", pick];
  return ["Answer", deskOneLine(i.finish?.question || i.latest || deskQuestion(i))];
}
/** That line as HTML: "<b>Answer:</b> keep the old group?"; words that start with the verb ("Pick one for …") bold it. */
function deskVerbHtml(i) {
  const v = deskVerb(i);
  if (!v || !v[1]) return "";
  const [verb, words] = v;
  const lead = words.match(new RegExp(`^${verb}\\b`, "i"));
  return lead ? `<b class="dk-vb">${esc(words.slice(0, verb.length))}</b>${esc(words.slice(verb.length))}` : `<b class="dk-vb">${esc(verb)}:</b> ${esc(words)}`;
}
/** A big feature (#282): an item with pieces, each its own item, listed under it. */
const deskFeat = (i) => !!i.pieces?.length;
const deskFeatDone = (f) => f.pieces.filter((p) => p.state === "done").length;
/** Where a feature nobody holds stands, as its pieces have it: done once all are, in progress while any has been
 *  started and isn't done, else queued. One with a holder is its own item. */
const deskFeatState = (f) => (f.holder ? null : f.pieces.every((p) => p.state === "done") ? "done"
  : f.pieces.some((p) => !["done", "new", "queued", "paused", "needs_resume"].includes(p.state)) ? "work" : "q");
/** A feature's list open: it or one of its pieces is open. */
const deskFeatOpen = (f) => deskOpen?.kind === "item" && (deskIdIs(f, deskOpen.id) || f.pieces.some((p) => deskIdIs(p, deskOpen.id)));
/** "3 of 5 done" and a thin bar of it. */
const deskFeatBar = (f, cls = "") => `<span class="dk-fp${cls}"><span>${deskFeatDone(f)} of ${f.pieces.length} done</span><span class="dk-pbar"><i style="width:${Math.round((100 * deskFeatDone(f)) / f.pieces.length)}%"></i></span></span>`;

/** The list's groups by state, with optional reviews in their own group. An
 *  item is in one group only. Who holds an item is the small word under its title; the sessions are on the left. */
function deskGroups(project, taskApprovals) {
  const d = deskOf(project);
  const items = deskItemsWithRequests(d, taskApprovals).sort((a, b) => deskMs(a.created) - deskMs(b.created));
  // What asks you something before what's ready to check; then what unblocks the most first, then the oldest (#182).
  const waiting = items.filter(deskWaits);
  const ub = new Map(waiting.map((i) => [i.id, deskUnblocks(i, items).length]));
  waiting.sort((a, b) => (a.state === "needs_you" ? 0 : 1) - (b.state === "needs_you" ? 0 : 1) || ub.get(b.id) - ub.get(a.id));
  const review = items.filter((i) => deskReadyToReview(i) && !deskWaits(i));
  // A piece of a feature that's listed shows under it (#282), not in a group of its own; one waiting on you or ready
  // to review stays in those groups too, so nothing you're asked is hidden.
  const ids = new Set(items.map((i) => i.id)), under = (i) => !!i.part_of && ids.has(i.part_of);
  const done = items.filter((i) => deskDoneToday(i) && !under(i)).sort((a, b) => deskMs(b.done_at) - deskMs(a.done_at));
  const rest = items.filter((i) => !deskWaits(i) && !deskReadyToReview(i) && i.state !== "done" && !under(i));
  let extra = [];
  try { extra = addOn.deskHolders?.(project) || []; } catch (e) { console.error(e); }
  // Finished and on its way in, in two lanes of their own (#346): the landing line, in the order it lands, then what's
  // on main and waits for the install, in the order it got there. A stop the Desk lead is settling, and a finish
  // that stopped on a question, stay with the active work.
  const landing = rest.filter((i) => deskLane(i) === "landing").sort(deskLandOrder);
  const install = rest.filter((i) => deskLane(i) === "install").sort((a, b) => (a.finish?.at || deskMs(a.updated)) - (b.finish?.at || deskMs(b.updated)));
  const going = (i) => !deskLane(i) && (i.state === "working" || deskFinishing(i) || (deskFeat(i) && deskFeatState(i) === "work"));
  const resume = rest.filter((i) => i.state === "needs_resume");
  // Paused by the lead (#365): kept apart, so it doesn't read as stuck or next up.
  const paused = rest.filter((i) => i.state === "paused");
  // Parked (#385, #391): a quiet group of their own; nothing starts them till you press Start.
  const held = rest.filter((i) => i.state === "held");
  // In your order (#387): those you dragged into place first, then the rest oldest first.
  const queued = rest.filter((i) => i.state !== "needs_resume" && i.state !== "paused" && i.state !== "held" && !going(i) && !deskLane(i));
  deskQSort(project, queued);
  return { d, waiting, review, working: rest.filter(going), landing, install, resume, paused, queued, held, extra, done, all: items };
}

// ---------- the list ----------

/** An item's state as its row's tag: [words, look]. Only an ask and a stuck one have words; the rest say it by their
 *  group. Back to check: a dot before it while not opened since (landed, its title bold too), nothing once opened. */
function deskTag(i) {
  // An ask carries no tag: its question, boxed under the title, says it.
  // In a circle of waits (#250): none of it can move till a link is cut.
  if (deskCircleOf(i)) return ["circle", "ask"];
  if (i.state === "needs_you") return ["", "ask"];
  if (deskInstallWait(i)) return ["", ""];
  if (i.state === "to_check") return deskLanded(i) ? ["", "landed"] : ["", "check"];
  // Its new session wouldn't start (#172): says so, its page has why and Retry.
  if ((i.state === "new" || i.state === "queued") && !i.holder && i.start_failed) return ["couldn't start", "ask"];
  if ((i.state === "new" || i.state === "queued") && i.route_failed) return ["stuck", "ask"];
  // Parked (#391): a quiet tag, so the row says it on its own too.
  if (i.state === "held") return ["parked", ""];
  return ["", i.state === "done" ? "done" : ""];
}
/** Where an item stands, as its row's icon: waiting on you (an ask, or back to check), in progress, queued (also
 *  one waiting for its install), done. */
const deskStateOf = (i) => (deskFeat(i) && !i.holder ? deskFeatState(i) : i.state === "done" ? "done" : i.state === "held" ? "held" : i.state === "needs_resume" || i.state === "paused" || i.stopped ? "stop" : i.state === "working" && deskStalled(i) ? "idle" : i.state === "working" ? "work" : deskFinishing(i) || deskLane(i) ? "fin" : (i.state === "needs_you" || i.state === "to_check") && !deskInstallWait(i) ? "you" : "q");
const DESK_STATE_SVG = {
  you: '<circle cx="7" cy="7" r="5.5" stroke="var(--muted)" stroke-width="1.6"/><path d="M7 4.2v3.4" stroke="var(--muted)" stroke-width="1.6"/><circle cx="7" cy="9.9" r=".9" fill="var(--muted)"/>',
  work: '<circle cx="7" cy="7" r="5.5" stroke="var(--accent)" stroke-width="1.5"/><circle class="dk-blink" cx="7" cy="7" r="2.2" fill="var(--accent)"/>',
  q: '<circle cx="7" cy="7" r="5.5" stroke="var(--muted)" stroke-width="1.5"/><path d="M7 4.3V7l1.9 1.3" stroke="var(--muted)" stroke-width="1.5"/>',
  stop: '<circle cx="7" cy="7" r="5.5" stroke="var(--muted)" stroke-width="1.5"/><path d="M5.5 5v4m3-4v4" stroke="var(--muted)" stroke-width="1.5"/>',
  // Stalled (#285): at work, but its holder has done nothing for a while. Muted and still, no blink.
  idle: '<circle cx="7" cy="7" r="5.5" stroke="var(--muted)" stroke-width="1.5"/><circle cx="7" cy="7" r="2" stroke="var(--muted)" stroke-width="1.3"/>',
  // Finishing: amber and steady (no blink), as nothing waits on you; a ring with an arrow down into it: done, on its
  // way in (#346).
  fin: '<circle cx="7" cy="7" r="5.5" stroke="var(--amber)" stroke-width="1.5"/><path d="M7 4.2v5M4.9 7.3 7 9.4l2.1-2.1" stroke="var(--amber)" stroke-width="1.5"/>',
  done: '<path d="m2.8 7.4 2.8 2.8 5.6-6" stroke="var(--muted)" stroke-width="1.7"/>',
  // Parked (#385): a dashed ring, nothing inside: it isn't going anywhere till you start it.
  held: '<circle cx="7" cy="7" r="5.5" stroke="var(--muted)" stroke-width="1.4" stroke-dasharray="2.2 2"/>',
};
const deskStateIcon = (i) => `<svg class="dk-si" width="14" height="14" viewBox="0 0 14 14" fill="none" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${DESK_STATE_SVG[deskStateOf(i)]}</svg>`;
/** Who holds an item, in a few words: "web-lead → search-centre" for its crew member. */
const deskHolderOf = (i) => (!i.holder ? "" : i.holder.kind === "crew" ? `${deskHolderName(i.holder.lead_sid)} → ${i.holder.name}` : i.crew ? `${i.holder.name} → ${i.crew}` : i.holder.name);
/** One item in two rows: its state, #id and title (what you'll see when it's done under it, a quiet line, #382); under them who holds it (Give to… when no one does), its age (a
 *  done one's time it finished), and a quiet ✓ when it's ready to check (Looks right; it fills on hover); an ask's question below that. Lit
 *  when it's open, or its crew member is. */
function deskItemRow(i, lit, held = false, lane = null) {
  // A feature's piece under it (#282): a smaller row, no lane.
  const piece = !!lane?.piece;
  if (piece) lane = null;
  const on = lit || (deskOpen?.kind === "item" && deskOpen.id === i.id);
  const [tag, look] = deskTag(i);
  // Lit by its session (clicked on the left) and at work: "working · host-lead · 4m", how long its session has been at it.
  const s = held && i.state === "working" ? sessionOf(i.holder?.sid) : null;
  const at = s ? (s.state === "working" && s.since_ms) || deskMs(i.updated) : 0;
  // At work on it (#285): what its holder is doing now and how long since ("cue-281 · Editing desk.rs · 1m"), or
  // that it has done nothing for a while ("Stalled · last read model.rs · 14m ago"). The holder's step, not the age.
  const act = !s && i.state === "working" && i.act;
  const stalled = act && deskStalled(i);
  const doing = !act ? "" : stalled ? `<span class="dk-doing"><b class="dk-stw">Stalled</b> · ${esc(i.act.was)} · <span data-ago="${i.act.at_ms}">${ago(i.act.at_ms)}</span> ago</span>`
    : `<span class="dk-doing"${i.act.stalls_at > 0 ? ` data-stalls="${i.act.stalls_at}"` : ""}>${esc(i.act.now)} · <span data-ago="${i.act.at_ms}">${ago(i.act.at_ms)}</span></span>`;
  // In a lane (#346): its holder and its step, short; the lane's head has the rest.
  const feat = deskFeat(i), fopen = feat && deskFeatOpen(i);
  const who = i.state === "held" ? "" : feat ? [esc(deskHolderOf(i)), deskFeatBar(i)].filter(Boolean).join(" · ") : lane ? [esc(deskHolderOf(i)), `<span class="dk-fstep">${deskLaneStep(i, lane.ahead)}</span>`].filter(Boolean).join(" · ") : deskCopyWaits(i) ? esc(deskCopyWaitLine(i)) : i.state === "queued" && i.after ? [esc(deskHolderOf(i)), `after #${esc(i.after)}`].filter(Boolean).join(" · ") : i.waiting && i.state === "working" ? `waiting · ${esc(firstLine(i.waiting))}` : i.stopped && i.state === "working" ? `stopped · the Desk lead is on it` : s ? `working · ${esc(deskHolderOf(i))} · <span data-ago="${at}">${ago(at)}</span>` : !i.holder && i.state !== "done" ? deskGive(i, "row") : deskInstallWait(i) ? `waiting for install · ${esc(deskHolderOf(i))}` : deskFinishing(i) ? `<span class="dk-fstep">${deskFinStep(i)}</span>` : esc(deskHolderOf(i));
  // Finishing: no age, so its step has the room.
  const when = s || act || deskFinishing(i) || (feat && i.state !== "done") ? "" : i.state === "done" ? `${i.commit ? `<span class="dk-cm">committed ${esc(i.commit)}</span> · ` : ""}${clockAt(deskMs(i.done_at || i.updated))}` : ago(deskMs(i.created));
  const ok = i.state === "held" ? `<button class="btn small dk-hstart" data-desk-act="start_now" data-id="${esc(i.id)}">Start</button>` : lane ? ""
    // Waiting for a free working folder (#401): Start now goes without one.
    : deskCopyWaits(i) && !piece && !feat ? `<button class="btn small dk-hstart" data-desk-act="start_now" data-id="${esc(i.id)}">Start now</button>`
    // Park on a row that waits its turn (#391), as on its page.
    : (i.state === "new" || i.state === "queued") && !piece && !feat ? `<button class="btn small dk-hstart dk-park" data-desk-act="park" data-id="${esc(i.id)}">Park</button>` : deskFinishing(i) ? deskInstallNow(i, "btn small dk-rinst") : deskInstallWait(i) || deskUncommitted(i) ? "" : i.state === "to_check" && deskLooksSoon.has(i.id) ? deskLooksSoonHtml(i.id) : i.state === "to_check" ? `<button class="dk-ok" data-desk-act="looks_right" data-id="${esc(i.id)}" aria-label="Looks right"></button>`
    : i.state === "done" && !i.picked ? `<button class="dk-undo" data-desk-act="reopen" data-id="${esc(i.id)}">Undo</button>` : "";
  // What it waits on you for, in one plain line (Answer, Pick, Allow, Try it), and what it unblocks (#182).
  const request = i.state === "needs_you" && deskItemRequest(i);
  const [question] = request?.kind === "question" ? questions(request) : [];
  const isQuestion = i.state === "needs_you" && (i.ask?.kind === "question" || (!i.ask && !i.finish?.question && !deskPickLine(i)));
  const vb = isQuestion ? esc(deskOneLine(question?.question || deskQuestion(i))) : deskVerbHtml(i);
  const decision = request?.kind === "permission"
    ? `<button class="btn small primary" data-desk-decision="allow" data-id="${esc(request.id)}"${answering.has(request.id) ? " disabled" : ""}>Allow</button><button class="btn small" data-desk-decision="deny" data-id="${esc(request.id)}"${answering.has(request.id) ? " disabled" : ""}>No</button>`
    // A question (#383): Send its suggestions, or Choose… on the item's page, where its whole card is.
    : question ? qFoldBtns(request, `<button class="btn small" data-desk-chk-item="${esc(i.id)}">Choose…</button>`, "btn small", `data-desk-send-answers="${esc(request.id)}"`)
    : i.state === "needs_you" && i.ask?.kind !== "permission" ? `<button class="btn small" data-desk-chk-item="${esc(i.id)}">Answer</button>` : "";
  const ask = vb || decision ? `<div class="dk-ask-row">${vb ? `<div class="dk-h">${vb}</div>` : ""}${question ? qFoldLine(request) : ""}${decision ? `<div class="dk-row-decisions">${decision}</div>` : ""}</div>` : "";
  return `<div class="dk-it ${i.state === "needs_you" ? "ask" : ""} ${look === "landed" ? "landed" : ""} dk-${deskStateOf(i)} ${on ? "on" : ""}${stalled ? " stalled" : ""}${i.state === "to_check" && deskLooksSoon.has(i.id) ? " soon" : ""}${deskLeaveStyle(i.id)}${feat ? " dk-feat" : ""}${piece ? " pc" : ""}" role="button" tabindex="0" data-desk-item="${esc(i.id)}"${feat ? ` aria-expanded="${fopen}"` : ""}>
    ${deskStateIcon(i)}<div class="dk-i1">${on ? '<span class="dk-larrow" aria-hidden="true">‹</span>' : ""}<span class="dk-tag ${look}">${tag}</span>${lane?.n ? `<span class="dk-rn">${lane.n}</span>` : ""}<span class="dk-t">${esc(i.title)}</span>${feat ? `<span class="dk-chev${fopen ? " open" : ""}" aria-hidden="true">›</span>` : ""}</div>
    ${i.see && !piece && deskVerb(i)?.[0] !== "Try it" ? `<div class="dk-see">You'll see: ${esc(deskOneLine(i.see))}</div>` : ""}<div class="dk-i2"><span class="dk-kid">${deskKindIcon(i)}<span class="dk-id">${esc(deskNum(i))}</span></span><span class="dk-who${s ? " held" : ""}">${who}</span>${doing}<span class="dk-age">${when}</span>${ok}</div>${ask}${deskWaits(i) ? deskUnblocksHtml(i) : ""}</div>`;
}
/** An add-on's head: its name, what it is, › while it's open (#219). */
const deskHolderHead = (h, on) => `<div class="dk-holder ${on ? "on" : ""}" role="button" tabindex="0" ${h.addon ? `data-desk-addon="${esc(h.key)}"` : `data-desk-holder="${esc(h.sid)}"`}><span class="dk-nm">${esc(h.name)}</span><span class="dk-s">${esc(h.sub || "")}</span><span class="grow"></span>${on ? '<span class="dk-go">›</span>' : ""}</div>`;
/** The item a crew member is on (lit with it while its pane is open). */
const deskCrewItem = (project, lead, agent) => { const t = deskMate(lead, agent); return deskOf(project).items.find((i) => i.state !== "done" && ((i.holder?.kind === "crew" && (i.holder.sid === agent || (t && i.holder.name === t.name))) || (t && i.crew && i.holder?.sid === lead && i.crew.toLowerCase() === (t.name || "").toLowerCase()))) || null; };

/** Finished items the list loaded from Cue (desk_done) as you scrolled, a project each: { items, end (none older),
 *  loading }; and how many finished items each project's list shows (10, then 10 more each time its end comes into
 *  view). Today's come with the Desk's state; older ones only from here. */
const deskDonePages = new Map(), deskDoneShown = new Map();
const DESK_DONE_STEP = 10;
/** A project's finished items, newest first: today's as the Desk's state has them, then those loaded; one that's open
 *  again (in the state, not done) left out. */
function deskDoneList(d) {
  const byId = new Map();
  for (const i of deskDonePages.get(d.project)?.items || []) byId.set(i.id, i);
  for (const i of d.items) { if (i.state === "done") byId.set(i.id, i); else byId.delete(i.id); }
  // A feature's pieces are under it (#282).
  const listed = new Set([...byId.keys(), ...d.items.map((i) => i.id)]);
  return [...byId.values()].filter((i) => !i.part_of || !listed.has(i.part_of)).sort((a, b) => deskMs(b.updated) - deskMs(a.updated) || (a.id < b.id ? 1 : a.id > b.id ? -1 : 0));
}
/** The next finished items older than the list's last, a step ahead of what shows so the next ones are there as you
 *  reach them. */
async function deskDoneFetch(project) {
  const pg = deskDonePages.get(project) || { items: [], end: false, loading: false };
  deskDonePages.set(project, pg);
  if (pg.end || pg.loading) return;
  pg.loading = true;
  const last = pg.items.at(-1);
  try {
    const got = await invoke("desk_done", { project, beforeMs: last ? deskMs(last.updated) : null, beforeId: last?.id ?? null, take: DESK_DONE_STEP * 2 });
    pg.items.push(...got);
    pg.end = got.length < DESK_DONE_STEP * 2;
  } catch (e) { console.error(e); pg.end = true; }
  pg.loading = false;
  if (deskSelection() === project && view === "desk") renderMain();
}
/** The list's end came into view: 10 more finished items, and the next ones fetched ahead. */
function deskDoneMore(project) {
  const pg = deskDonePages.get(project);
  const all = deskDoneList(deskOf(project)).length, n = deskDoneShown.get(project) || DESK_DONE_STEP;
  if (all > n) { deskDoneShown.set(project, n + DESK_DONE_STEP); renderMain(); }
  if (!pg?.end && all < n + DESK_DONE_STEP * 2) deskDoneFetch(project);
}
let deskDoneSeen = null;
/** Watches the list's end (after each draw): in view, 10 more. */
function deskDoneWatch() {
  deskDoneSeen?.disconnect();
  const end = document.querySelector(".dk-items [data-desk-done-more]"), box = end?.closest(".dk-scroll");
  if (!end || typeof IntersectionObserver === "undefined") return;
  const project = end.dataset.deskDoneMore;
  deskDoneSeen = new IntersectionObserver((es) => { if (es.some((e) => e.isIntersecting)) { deskDoneSeen.disconnect(); deskDoneMore(project); } }, { root: box, rootMargin: "0px 0px 240px 0px" });
  deskDoneSeen.observe(end);
}

function deskList(project, taskApprovals) {
  const g = deskGroups(project, taskApprovals);
  const o = deskOpen;
  const crewLit = o?.kind === "crew" ? deskCrewItem(project, o.lead, o.agent)?.id : null;
  // A session clicked on the left lights nothing on the right (#170): it can hold several items.
  // Away (#373): the lead's calls since it went on, each a line under the item it changed; rows stay where they are.
  const calls = deskAwayCalls(g.d), byItem = new Map(), placed = new Set();
  for (const c of calls) if (c.item_id) byItem.set(c.item_id, [...(byItem.get(c.item_id) || []), c]);
  const marks = (i) => { const cs = byItem.get(i.id) || []; cs.forEach((c) => placed.add(c.id)); return cs.map(deskAwayMark).join(""); };
  // A feature's row, then its pieces under it while it or one of them is open (#282): in place, nothing moves.
  const row = (i) => deskItemRow(i, i.id === crewLit, false) + marks(i) + (deskFeat(i) && deskFeatOpen(i) ? `<div class="dk-pcs">${i.pieces.map((p) => deskItemRow(p, p.id === crewLit, false, { piece: true }) + marks(p)).join("")}</div>` : "");
  const parts = [];
  // Waiting on you stays at 0 (in red only while something waits), so you see there's nothing for you. While away, the
  // open asks from the Desk's sessions join it, and a call with no item shown here sits at its end (filled last).
  const aw = deskAwayWaiting(g.d, taskApprovals), nWait = g.waiting.length + aw.n;
  parts.push(`<div class="dk-gh you${nWait ? "" : " none"}">Waiting on you <span class="n">${nWait}</span></div>${g.waiting.map(row).join("")}${aw.html}`);
  const loose = parts.length;
  // Ready to review sits right under Waiting on you (#275): both are yours to look at.
  if (g.review.length) parts.push(`<div class="dk-gh">Ready to review <span class="n">${g.review.length}</span></div>${g.review.map(row).join("")}`);
  const nCalls = calls.length ? `<span class="dk-split dk-aw-n">${calls.length} call${calls.length === 1 ? "" : "s"} while away</span>` : "";
  if (g.working.length) parts.push(`<div class="dk-gh">In progress <span class="n">${g.working.length}</span>${deskSplitHtml(g.working)}${nCalls}</div>${g.working.map(row).join("")}`);
  // The landing line, numbered in the order it lands (one at a time), then what's on main for the next install, with
  // its time and Install now (#346).
  if (g.landing.length) parts.push(`<div class="dk-gh dk-lane" data-lane="landing">Landing <span class="n">${g.landing.length}</span><span class="dk-split">one at a time</span></div>${g.landing.map((i, k) => deskItemRow(i, i.id === crewLit, false, { n: k + 1, ahead: g.landing[k - 1] }) + marks(i)).join("")}`);
  if (g.install.length) parts.push(`<div class="dk-gh dk-lane" data-lane="install">Waiting to install <span class="n">${g.install.length}</span>${deskInstallHead(g.install)}</div>${g.install.map((i) => deskItemRow(i, i.id === crewLit, false, {}) + marks(i)).join("")}`);
  if (g.resume.length) parts.push(`<div class="dk-gh">Needs resume <span class="n">${g.resume.length}</span></div>${g.resume.map(row).join("")}`);
  if (g.paused.length) parts.push(`<div class="dk-gh">Paused <span class="n">${g.paused.length}</span></div>${g.paused.map(row).join("")}`);
  // Dragged up and down into your order (#387): the next free copy goes to the top one.
  if (g.queued.length) parts.push(`<div class="dk-gh">Queued <span class="n">${g.queued.length}</span>${g.queued.length > 1 ? '<span class="dk-qhint">drag to reorder</span>' : ""}</div>${g.queued.map((i) => row(i).replace('<div class="dk-it ', `<div data-desk-qg${deskQMoves(i) ? ` data-desk-q="${esc(i.id)}"` : ""} class="dk-it `)).join("")}`);
  if (g.held.length) parts.push(`<div class="dk-gh dk-heldh">Parked <span class="n">${g.held.length}</span><span class="dk-split">start them when you want</span></div><div class="dk-held">${g.held.map(row).join("")}</div>`);
  for (const h of g.extra) parts.push(deskHolderHead({ ...h, addon: true, key: h.key || h.name }, o?.kind === "addon" && o.key === (h.key || h.name)) + (h.items || []).map(row).join(""));
  // Finished: the 10 newest, then 10 more each time the list's end comes into view (deskDoneWatch). Its end holds
  // its place while the next ones load, so nothing jumps.
  const pg = deskDonePages.get(project);
  if (!pg) queueMicrotask(() => deskDoneFetch(project));
  const done = deskDoneList(g.d), shown = done.slice(0, deskDoneShown.get(project) || DESK_DONE_STEP);
  const more = done.length > shown.length || !pg?.end;
  // Dropped items' copies kept with their work, till you remove them (hub/desk_trees.rs).
  const kept = g.d?.kept || [];
  if (kept.length) parts.push(`<div class="dk-gh">Kept copies <span class="n">${kept.length}</span></div>${kept.map(deskKeptRow).join("")}`);
  // Copies whose work is already in the app, for you to remove (#376).
  const old = deskOldCopiesHtml(project, g.d?.old_copies);
  if (old) parts.push(old);
  if (shown.length) parts.push(`<div class="dk-gh">Done</div>${shown.map(row).join("")}`);
  if (more && (shown.length || pg?.loading || !pg)) parts.push(`<div class="dk-more" data-desk-done-more="${esc(project)}">${shown.length ? "Loading older…" : ""}</div>`);
  // A call whose item isn't in the list (none, or an older finished one): at the end of Waiting on you.
  const left = calls.filter((c) => !placed.has(c.id));
  if (left.length) parts.splice(loose, 0, left.map(deskAwayMark).join(""));
  const body = parts.join("") + (parts.length > 1 ? "" : `<div class="dk-empty">No items yet.</div>`);
  return `<div class="dk-card dk-items"><div class="dk-ch"><b>Items</b>${deskPausedTag(g.d)}${deskPauseButton(g.d)}${deskAwaySwitch(g.d)}</div>
    ${deskQuickAdd()}<div class="dk-scroll" data-scroll-key="desk-items:${esc(project)}">${body}</div></div>`;
}

// ---------- the Queued list in your order (#387) ----------

/** Waiting for a free working folder, with a session to start it (#401). */
const deskCopyWaits = (i) => i.state === "queued" && !!i.tree_wait && !!i.holder;
/** Why it waits, in plain words: sent back or not, and which items are using the working folders. */
function deskCopyWaitLine(i) {
  const by = (i.copy_users || []).map((x) => `#${x}`).join(", ");
  return `${i.back_waiting ? "Sent back · waiting" : "Waiting"} for a free working folder${by ? ` (in use by ${by})` : ""}`;
}
/** Whether a row in Queued can be dragged: one that hasn't started (running ones keep their place). */
const deskQMoves = (i) => i.state === "new" || i.state === "queued";
/** Why order `ids` (top first) can't be, in plain words: one above the item it waits on. "" when it can. */
function deskQWhyNot(ids, items) {
  const at = new Map(ids.map((id, k) => [id, k]));
  for (const id of ids) {
    const i = items.find((x) => x.id === id), on = i?.after;
    if (on && at.has(on) && at.get(on) > at.get(id)) {
      const o = items.find((x) => x.id === on);
      return `#${i.id} waits on #${on}${o ? ` (${deskShort(o.title)})` : ""}, so it can't go above it`;
    }
  }
  return "";
}
/** A drop Cue hasn't confirmed yet, per Desk: { ids, until }. A change that comes in before Cue has saved it would show
 *  the old order for a moment (the row jumping back); till it has, or a few seconds pass, the list keeps your order. */
const deskQDropped = new Map();
/** Queued items in your order: a drop not yet confirmed first, then those you placed, then the rest oldest first. */
function deskQSort(project, queued) {
  const p = deskQDropped.get(project);
  if (p && (Date.now() > p.until || p.ids.every((id, k) => queued.find((i) => i.id === id)?.queue_order === k + 1))) deskQDropped.delete(project);
  const mine = deskQDropped.get(project) ? new Map(deskQDropped.get(project).ids.map((id, k) => [id, k + 1])) : null;
  const at = (i) => mine?.get(i.id) ?? i.queue_order ?? Infinity;
  return queued.sort((a, b) => at(a) - at(b));
}
/** A press on a queued row: past a few pixels up or down it's a drag, the others making room as it passes, the list
 *  scrolling when you hold it at its top or bottom edge; let go, it takes its place (desk_item_act "reorder"), or
 *  snaps back with why. A press that doesn't move is a click. Pressing doesn't select the window's text. */
function deskQPress(e) {
  if (e.button !== 0) return;
  const row = e.target.closest?.(".dk-it[data-desk-q]");
  if (!row || e.target.closest("button, a, input, textarea")) return;
  const rows = [...row.parentElement.querySelectorAll(":scope > .dk-it[data-desk-qg]")];
  const scroller = row.closest(".dk-scroll"), st0 = scroller?.scrollTop || 0;
  const from = rows.indexOf(row), y0 = e.clientY, rects = rows.map((r) => r.getBoundingClientRect());
  const h = rects[from].height + 2;
  let on = false, to = from, y = y0, edge = 0, tick = 0;
  // How far it has gone: the pointer's move plus the list's scroll since the press (rects are as at the press).
  const place = () => {
    const dy = y - y0 + ((scroller?.scrollTop || 0) - st0);
    const mid = rects[from].top + rects[from].height / 2 + dy;
    to = rows.reduce((n, r, k) => (k === from ? n : k < from ? (mid < rects[k].top + rects[k].height / 2 ? Math.min(n, k) : n) : mid > rects[k].top + rects[k].height / 2 ? Math.max(n, k) : n), from);
    rows.forEach((r, k) => { if (k === from) return; r.style.transform = from < k && k <= to ? `translateY(${-h}px)` : to <= k && k < from ? `translateY(${h}px)` : ""; });
    row.style.transform = `translateY(${dy}px)`;
  };
  // Held near the list's top or bottom: it scrolls that way, faster nearer the edge.
  const roll = () => {
    tick = 0;
    if (!on || !edge || !scroller) return;
    const was = scroller.scrollTop;
    scroller.scrollTop += edge;
    if (scroller.scrollTop !== was) place();
    tick = requestAnimationFrame(roll);
  };
  const move = (ev) => {
    y = ev.clientY;
    if (!on && Math.abs(y - y0) < 5) return;
    if (!on) { on = true; getSelection().removeAllRanges(); row.classList.add("dk-drag"); document.body.classList.add("dk-dragging"); }
    const box = scroller?.getBoundingClientRect(), zone = 36;
    const by = (d) => Math.min(16, Math.ceil(d / 3));
    edge = !box ? 0 : y < box.top + zone ? -by(box.top + zone - y) : y > box.bottom - zone ? by(y - (box.bottom - zone)) : 0;
    if (edge && !tick) tick = requestAnimationFrame(roll);
    place();
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    if (tick) cancelAnimationFrame(tick);
    document.body.classList.remove("dk-dragging");
    if (!on) return;
    // The click that ends a drag doesn't open the item (none may come: then it's let go of soon after).
    const eat = (c) => { c.stopPropagation(); c.preventDefault(); };
    addEventListener("click", eat, { capture: true, once: true });
    setTimeout(() => removeEventListener("click", eat, { capture: true }), 400);
    const ids = rows.map((r) => r.dataset.deskQ || r.dataset.deskItem);
    ids.splice(to, 0, ids.splice(from, 1)[0]);
    const project = deskSelection(), items = deskOf(project)?.items || [];
    const fixed = rows.findIndex((r, k) => !r.dataset.deskQ && (k - from) * (k - to) <= 0 && k !== from);
    const why = to === from ? "" : fixed >= 0 ? "Items already started keep their place" : deskQWhyNot(ids, items);
    const back = () => { rows.forEach((r) => { r.style.transition = "transform .15s ease"; r.style.transform = ""; }); setTimeout(() => { rows.forEach((r) => { r.style.transition = ""; }); row.classList.remove("dk-drag"); }, 160); };
    if (to === from || why) { back(); if (why) toast(why, 4000); return; }
    // It stays where you let go (the rows keep their places till the list is drawn again, in your order), and Cue
    // saves it, or says why it won't.
    const moved = ids.filter((id) => rows.find((r) => (r.dataset.deskQ || r.dataset.deskItem) === id)?.dataset.deskQ);
    deskQDropped.set(project, { ids: moved, until: Date.now() + 5000 });
    invoke("desk_item_act", { id: row.dataset.deskQ, act: "reorder", text: moved.join(" ") })
      .catch((err) => { deskQDropped.delete(project); toast(String(err), 4000); renderMain(); });
    renderMain();
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up, { once: true });
}
// The press that may become a drag selects no text (the window's text would light up as you drag, and be copied).
document.addEventListener("mousedown", (e) => { if (e.button === 0 && e.target.closest?.(".dk-it[data-desk-q]") && !e.target.closest("button, a, input, textarea")) e.preventDefault(); });
document.addEventListener("pointerdown", deskQPress);
