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
    // A long folder keeps its last two: …/src/desk/hub/core/desk.rs.
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
/** Where an item to check is, in one plain line (#436): mocks to look at ("Mocks · open the canvas", with its pick),
 *  on its way in ("Lands with the next install"), changes left on its branch ("2 changes not landed yet"), or in the app since its install, with what to try. { text, html,
 *  tried } (tried: the line already says what you'll see), or null when it isn't to check. */
function deskWhereLine(i) {
  if (i?.state !== "to_check") return null;
  const plainLine = (text, tried = false) => ({ text, html: esc(text), tried });
  // Changes left on its own branch, not on main (#448): it isn't closed till they land.
  const left = i.finish?.unlanded || 0;
  if (left) return plainLine(`${left} change${left === 1 ? "" : "s"} not landed yet · send it back so they land`);
  const canvas = deskCanvas(i);
  if (canvas || i.finish?.mocks) {
    const pick = deskPickLine(i), tail = pick ? ` · ${pick}` : "";
    if (!canvas) return plainLine(`Nothing to install${tail}`);
    return { text: `Mocks · open the canvas${tail}`, html: `Mocks · <a class="dk-dlink" href="#" data-href="${esc(canvas)}">open the canvas</a>${esc(tail)}`, tried: false };
  }
  if (deskInstallWait(i)) return plainLine("Lands with the next install");
  // Not on main yet: the same line as its page's box (#526), whose button says Commit to main.
  if (deskUncommitted(i)) return plainLine(`Not on main yet · ${deskCheckText(deskToCheckLine(i))} · Commit to main puts it there`, true);
  // In the app: what to do with it, the line its session wrote at DONE or Cue's own words (#526).
  const c = deskToCheckLine(i), cw = c.cue ? " · Cue's words" : "";
  return { text: `${deskInAppSince(i)} · ${deskCheckText(c)}${cw}`, html: `${esc(deskInAppSince(i))} · ${deskCheckHtml(c)}${cw ? `<span class="dk-cw">${esc(cw)}</span>` : ""}`, tried: true };
}
/** Where an item to check that's in the app is (#436): "In the app since 3:04 PM", from its install (or its window's
 *  files served in place); a project with nothing to install: "On main since …". */
function deskInAppSince(i) {
  const app = deskLatestCheck(i)?.cue !== false;
  const at = i.finish?.step === "checked" && i.finish.at ? i.finish.at : app ? state.desk?.installed_ms || 0 : 0;
  return `${app ? "In the app" : "On main"}${at ? ` since ${clockAt(at)}` : ""}`;
}
/** The words before an item's check line, by its kind (#526; "Nothing to try:" since #668). */
const DESK_CHECK_LABEL = { try: "Try:", nothing: "Nothing to try:", unsaid: "Nothing said what to try:" };
/** What to do with item `i` to check, in one line (#526): { kind, text, cue, steps }. Its session's line at DONE
 *  (`i.check`: "try", something to try, or "nothing", nothing to try and why); else Cue's own words (`cue`): its
 *  you'll-see to try, or, with none, that nothing said what to try (kind "unsaid"). `steps` (#668): its session's
 *  three steps, [where, do, expect], when it gave them; its one line is then the Do and where. */
function deskToCheckLine(i) {
  const c = i?.check, text = c && (c.kind === "try" || c.kind === "nothing") ? plain(c.text) : "";
  const steps = c?.kind === "try" && c.where && c.do && c.expect ? [plain(c.where), plain(c.do), plain(c.expect)] : null;
  if (steps) return { kind: "try", text: `${steps[1]} · ${steps[0]}`, cue: false, steps };
  if (text) return { kind: c.kind, text, cue: false };
  const see = deskOneLine(i?.see);
  return see ? { kind: "try", text: see, cue: true } : { kind: "unsaid", text: "Send back to ask its session", cue: true };
}
/** That line as text ("Try: …") and as html, its label bold. */
const deskCheckText = (c) => `${DESK_CHECK_LABEL[c.kind]} ${c.text}`;
const deskCheckHtml = (c) => `<b class="dk-vb">${DESK_CHECK_LABEL[c.kind]}</b> ${esc(c.text)}`;
function deskVerb(i) {
  if (i.state === "to_check") return ["", deskWhereLine(i).text];
  if (i.state !== "needs_you") return null;
  const a = i.ask;
  if (a) return [a.kind === "permission" ? "Allow" : (a.choices || []).length > 1 ? "Pick" : "Answer", a.kind === "permission" ? deskAllowLine(i) : deskOneLine(a.text)];
  const pick = deskPickLine(i);
  if (pick) return ["Pick", pick];
  // "Answer" only for a question: a status line ("A re-run is going now") is shown as it is (#435).
  const words = deskOneLine(i.finish?.question || i.latest || deskQuestion(i));
  return [/\?\s*$/.test(words || "") ? "Answer" : "", words];
}
/** That line as HTML: "<b>Answer:</b> keep the old group?", or just the words when there's no verb; words that start with the verb ("Pick one for …") bold it. */
function deskVerbHtml(i) {
  const v = deskVerb(i);
  if (!v || !v[1]) return "";
  const [verb, words] = v;
  if (i.state === "to_check") return deskWhereLine(i).html;
  if (!verb) return esc(words);
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
/** "3 of 5 done" and a thin bar of it. */
const deskFeatBar = (f, cls = "") => `<span class="dk-fp${cls}"><span>${deskFeatDone(f)} of ${f.pieces.length} done</span><span class="dk-pbar"><i style="width:${Math.round((100 * deskFeatDone(f)) / f.pieces.length)}%"></i></span></span>`;

/** The list's groups by state, with optional reviews in their own group. An
 *  item is in one group only. Who holds an item is the small word under its title; the sessions are on the left. */
function deskGroups(project, taskApprovals) {
  const d = deskOf(project);
  const items = deskItemsWithRequests(d, taskApprovals).sort((a, b) => deskMs(a.created) - deskMs(b.created));
  // A step Cue holds for your OK first, oldest first (#749); then what asks you something before what's ready to
  // check; then what unblocks the most first, then the oldest (#182).
  const waiting = items.filter(deskWaits);
  const ub = new Map(waiting.map((i) => [i.id, deskUnblocks(i, items).length])), ha = new Map(waiting.map((i) => [i.id, deskHeldAt(i)]));
  const heldOrder = (a, b) => (!!ha.get(b.id) - !!ha.get(a.id)) || (ha.get(a.id) && ha.get(b.id) ? ha.get(a.id) - ha.get(b.id) : 0);
  waiting.sort((a, b) => heldOrder(a, b) || (a.state === "needs_you" ? 0 : 1) - (b.state === "needs_you" ? 0 : 1) || ub.get(b.id) - ub.get(a.id));
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
  // The list splits what's yours (#602): Waiting on you holds only questions (and picks); what finished and waits
  // for your Looks right or Send back is Ready to check. `waiting` and `review` stay whole for the counts.
  const asks = waiting.filter((i) => !deskNeedsVerification(i)), check = [...waiting.filter(deskNeedsVerification), ...review];
  return { d, waiting, review, asks, check, working: rest.filter(going), landing, install, resume, paused, queued, held, extra, done, all: items };
}

// ---------- the list ----------

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
/** Where item `i`'s holder runs when it isn't this Mac: the machine label beside its name (core/helpers.js machTag),
 *  "" on this Mac. The hub's `machine` (its display name) first, else its holder session's own. */
function deskMachTag(i) {
  if (i?.machine) return `<span class="mach"><span>${esc(i.machine.length > 10 ? `${i.machine.slice(0, 5)}…${i.machine.slice(-5)}` : i.machine)}</span></span>`;
  return i?.machine === undefined ? machTag(sessionOf(deskHolderSid(i))?.machine) : "";
}
/** Item `i` worked on another machine (`i.remote`), in one plain line: the machine not answering, what it waits for,
 *  when its work was last pushed (pushed at each of its holder's stops, #598), or that none of it was while the machine
 *  is quiet. "" when it's worked here or there's nothing to say. */
function deskRemoteLine(i) {
  const r = i?.remote;
  if (!r) return "";
  const mins = (ms) => { const m = Math.max(1, Math.round((now() - ms) / 60000)); return m < 60 ? `${m} min` : ago(ms); };
  return [r.quiet_since > 0 ? `machine not answering for ${mins(r.quiet_since)}` : "", r.wait ? firstLine(r.wait) : "", r.pushed_at > 0 ? `last pushed ${mins(r.pushed_at)} ago` : r.quiet_since > 0 && !r.wait ? "its work is on the machine, not pushed" : ""].filter(Boolean).join(" · ");
}
const deskHolderOf = (i) => (!i.holder ? "" : i.holder.kind === "crew" ? `${deskHolderName(i.holder.lead_sid)} → ${i.holder.name}` : i.crew ? `${i.holder.name} → ${i.crew}` : i.holder.name);
/** A permission's two buttons on the Desk (#426): No, then Allow, side by side on one line. */
function deskAllowNo(id) {
  const off = answering.has(id) ? " disabled" : "";
  return `<button class="btn small" data-desk-decision="deny" data-id="${esc(id)}"${off}>No</button><button class="btn small primary" data-desk-decision="allow" data-id="${esc(id)}"${off}>Allow</button>`;
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
  const last = pg.items.at(-1), was = deskDoneSeenNow(project);
  try {
    const got = await invoke("desk_done", { project, beforeMs: last ? deskMs(last.updated) : null, beforeId: last?.id ?? null, take: DESK_DONE_STEP * 2 });
    pg.items.push(...got);
    pg.end = got.length < DESK_DONE_STEP * 2;
  } catch (e) { console.error(e); pg.end = true; }
  pg.loading = false;
  // Fetched ahead of what shows: nothing to draw till its end comes into view (each draw of a long Done list is slow).
  // Nothing new showing, its end is watched again (#735): reached while this was loading, the watch had let go and
  // nothing would load more till some other draw; still in view, it loads the next 10 at once.
  if (deskSelection() !== project || view !== "desk") return;
  if (deskDoneSeenNow(project) === was) deskDoneWatch();
  else if (!deskDonePatch(project)) renderMain();
}
/** What a project's Done shows: how many rows, and whether its end ("Loading older…") is there. */
function deskDoneSeenNow(project) {
  const d = deskOf(project), pg = deskDonePages.get(project);
  if (!d) return "";
  const all = deskDoneList(d).length, n = deskDoneShown.get(project) || DESK_DONE_STEP;
  return `${Math.min(all, n)}|${all > n || !pg?.end}`;
}
/** The list's end came into view: 10 more finished items, and the next ones fetched ahead. */
function deskDoneMore(project) {
  const pg = deskDonePages.get(project);
  const all = deskDoneList(deskOf(project)).length, n = deskDoneShown.get(project) || DESK_DONE_STEP;
  if (all > n) { deskDoneShown.set(project, n + DESK_DONE_STEP); if (!deskDonePatch(project)) renderMain(); }
  if (!pg?.end && all < n + DESK_DONE_STEP * 2) deskDoneFetch(project);
}
/** What each project's Done last drew (#733): its rows' count, the last one's id, how a row is drawn, and the fewest
 *  it shows (B2's Done today shows all of today's). Each draw sets it. */
const deskDoneDrawn = new Map();
/** 10 more finished items, or the end gone, put in at the list's end in place (#733): the rest of the window isn't
 *  drawn again, so each step stays quick however long the list grows and however busy the Mac is. False when the
 *  list on screen isn't the one last drawn (a full draw then). */
function deskDonePatch(project) {
  const at = deskDoneDrawn.get(project), d = deskOf(project), pg = deskDonePages.get(project);
  const end = document.querySelector(`.dk-items [data-desk-done-more="${CSS.escape(project)}"]`);
  if (!at || !d || !end || !at.n || at.b2 !== !!end.closest(".dk-b2") || end.closest(".dk-gbody.shut")) return false;
  const all = deskDoneList(d), n = Math.min(all.length, Math.max(deskDoneShown.get(project) || DESK_DONE_STEP, at.least));
  if (n < at.n || all[at.n - 1]?.id !== at.last) return false;
  if (n > at.n) end.insertAdjacentHTML("beforebegin", all.slice(at.n, n).map(at.row).join(""));
  deskDoneDrawn.set(project, { ...at, n, last: all[n - 1].id });
  if (all.length <= n && pg?.end) end.remove();
  deskDoneWatch();
  return true;
}
let deskDoneSeen = null;
/** Watches the list's end (after each draw): in view, 10 more. */
function deskDoneWatch() {
  deskDoneSeen?.disconnect();
  const end = document.querySelector(".dk-items [data-desk-done-more]"), box = end?.closest(".dk-scroll");
  if (!end || end.closest(".dk-gbody.shut") || typeof IntersectionObserver === "undefined") return;
  const project = end.dataset.deskDoneMore;
  deskDoneSeen = new IntersectionObserver((es) => { if (es.some((e) => e.isIntersecting)) { deskDoneSeen.disconnect(); deskDoneMore(project); } }, { root: box, rootMargin: "0px 0px 240px 0px" });
  deskDoneSeen.observe(end);
}

/** The item list's groups you folded (#430), by name ("queued", "working", …), the same on every Desk; kept across
 *  restarts. Parked and Done start folded (#602): for those it keeps the ones you opened. */
const deskGroupsShut = (() => { try { return new Set(JSON.parse(localStorage.getItem("cue.deskGroupsShut") || "[]")); } catch { return new Set(); } })();
const deskGroupsOpened = (() => { try { return new Set(JSON.parse(localStorage.getItem("cue.deskGroupsOpened") || "[]")); } catch { return new Set(); } })();
const DESK_GROUPS_FOLDED = new Set(["held", "done"]);
const deskGroupShut = (key) => (DESK_GROUPS_FOLDED.has(key) ? !deskGroupsOpened.has(key) : deskGroupsShut.has(key));
const DESK_GCHEV = `<svg class="dk-gchev" width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3.5 2l3 3-3 3"/></svg>`;
/** A group's header clicked: it folds or opens in place with a short slide (the list isn't drawn again, so nothing
 *  jumps), and stays so. */
function deskGroupFold(key) {
  const shut = !deskGroupShut(key), kept = DESK_GROUPS_FOLDED.has(key) ? deskGroupsOpened : deskGroupsShut;
  if (shut === (kept === deskGroupsShut)) kept.add(key);
  else kept.delete(key);
  try { localStorage.setItem("cue.deskGroupsShut", JSON.stringify([...deskGroupsShut])); localStorage.setItem("cue.deskGroupsOpened", JSON.stringify([...deskGroupsOpened])); } catch { /* kept for this window only */ }
  const h = document.querySelector(`.dk-items [data-desk-group="${key}"]`), b = document.querySelector(`.dk-items [data-desk-gbody="${key}"]`);
  if (!h || !b) return renderMain();
  h.setAttribute("aria-expanded", String(!shut));
  b.classList.add("moving");
  b.classList.toggle("shut", shut);
  setTimeout(() => { b.classList.remove("moving"); if (!shut) deskDoneWatch(); }, 240);
}




// ---------- the Queued list in your order (#387) ----------

/** Waiting for a free working folder, with a session to start it (#401). */
const deskCopyWaits = (i) => i.state === "queued" && !!i.tree_wait && !!i.holder;
/** Why it waits, in plain words: sent back or not, and which items are using the working folders. */
function deskCopyWaitLine(i) {
  const by = (i.copy_users || []).map((x) => `#${x}`).join(", ");
  return `${i.back_waiting ? "Sent back · waiting" : "Waiting"} for a free working folder on This Mac${by ? ` (in use by ${by})` : ""}`;
}
/** A time as h:mm, local, 24-hour ("9:05", "14:20"). */
const deskQHm = (ms) => { const d = new Date(ms); return `${d.getHours()}:${String(d.getMinutes()).padStart(2, "0")}`; };
/** What a queued item with no session yet waits for, in words (#576): "next for cue-268 · goes to a fresh session at
 *  14:20" (the session it's marked for, and when it goes to a fresh one instead), "after #271 · then cue-268", "any
 *  free session". Plain text; "" for one that isn't queued or has a holder (an older one, said as before). */
function deskQueueLine(i) {
  if (i.state !== "queued" || i.holder) return "";
  const nf = i.next_for ? deskHolderOf({ holder: i.next_for }) : "";
  if (i.after) {
    const on = deskAfterName(i);
    return [on.startsWith("#") ? `after ${on}` : `Waits for ${on}`, nf && `then ${nf}`].filter(Boolean).join(" · ");
  }
  // Every place it may start has its number of the Desk's items at work (#711): it starts on whichever has room
  // first, picked then (#776), so the row names the places, not one machine.
  if (i.machine_wait) return `waits for room on ${i.machine_wait}`;
  if (nf) return `next for ${nf}${i.fresh_at ? ` · goes to a fresh session at ${deskQHm(i.fresh_at)}` : ""}`;
  return "any free session";
}
/** A row in the Queue; one that can be dragged has a small grip (two short lines) in its right padding (#424). */
const DESK_GRIP = `<svg class="dk-grip" width="12" height="10" viewBox="0 0 12 10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true"><path d="M1 1h10M1 5h10M1 9h10"/></svg>`;
const deskQRow = (row) => (i) => {
  const moves = deskQMoves(i);
  const h = row(i).replace('<div class="dk-it ', `<div data-desk-qg${moves ? ` data-desk-q="${esc(i.id)}"` : ""} class="dk-it `);
  return moves ? h.replace(/^(<div [^>]*>)/, `$1${DESK_GRIP}`) : h;
};
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
  const scroller = row.closest(".dk-scroll, .dkn-dl"), st0 = scroller?.scrollTop || 0;
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
    deskQDrop(row, rows, from, to);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up, { once: true });
}
/** A queued row's drag let go (deskQPress): `row`, at `from` among `rows`, dropped at `to`. It takes its place
 *  (desk_item_act "reorder"), or snaps back with why. */
function deskQDrop(row, rows, from, to) {
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
}
// The press that may become a drag selects no text (the window's text would light up as you drag, and be copied).
document.addEventListener("mousedown", (e) => { if (e.button === 0 && e.target.closest?.(".dk-it[data-desk-q]") && !e.target.closest("button, a, input, textarea")) e.preventDefault(); });
document.addEventListener("pointerdown", deskQPress);

// ---------- the list kept across a draw (#480) ----------

/** Every draw sets the window's HTML afresh, which threw the Items list (and Progress) away and drew each row new
 *  several times a second while items were at work: each row's pulse started over and the row under the pointer lost
 *  its hover till the mouse moved, so the whole list blinked. Now the list's scroll box is lifted out before the window
 *  is drawn and put back after, brought up to date by key: a row whose HTML is the same is left exactly as it is, one
 *  that changed is updated in place, one that changed group is moved there (the same element), a new one goes in, a
 *  gone one comes out. */
const DESK_KEPT_LISTS = '.dk-items > .dk-scroll[data-scroll-key], .dkn-card > .dk-scroll[data-scroll-key], .dk-prog > .dk-scroll[data-scroll-key]';
/** A child's key among its siblings: an item's row by its id (the same row anywhere in the list), a group's head and
 *  body by the group, a Progress row by its item; anything else by its class and place among those of its kind. */
function deskKeepKey(el, n) {
  const d = el.dataset;
  if (d.deskItem != null && el.classList.contains("dk-it")) return `i:${d.deskItem}`;
  if (d.deskProgRow != null) return `p:${d.deskProgRow}`;
  if (d.deskGroup != null) return `g:${d.deskGroup}`;
  if (d.deskGbody != null) return `b:${d.deskGbody}`;
  const k = `${el.tagName}.${el.classList[0] || ""}`;
  n.set(k, (n.get(k) || 0) + 1);
  return `${k}#${n.get(k)}`;
}
const deskKeepKeys = (box) => { const n = new Map(); return [...box.children].map((c) => deskKeepKey(c, n)); };
/** What holds rows: brought up to date child by child, not as one. */
const DESK_KEEP_INSIDE = ".dk-gbody, .dk-gin, .dk-pcs, .dk-held";
/** Attributes as the fresh draw has them; a group sliding open or shut (deskGroupFold) keeps sliding. */
function deskKeepAttrs(a, b) {
  if (a.classList.contains("moving")) b.classList.add("moving");
  morphAttrs(a, b);
}
/** `live`'s children made to match `fresh`'s, by key. `pool`: every row of the list as it was, by key, so a row
 *  that changed group is the same element in its new place. `spare`: where a row waits that left its group before
 *  the group it goes to is reached. */
/** A move that keeps the row's state where it can: a row in a box taken out already (a feature's pieces) just goes in. */
const deskKeepMove = (parent, node, before) => (node.isConnected ? moveTo(parent, node, before) : parent.insertBefore(node, before));
function deskKeepPatch(live, fresh, pool, spare) {
  const kids = [...fresh.children], keys = deskKeepKeys(fresh), want = new Set(keys);
  const old = new Map();
  deskKeepKeys(live).forEach((k, x) => { const c = live.children[x]; if (!old.has(k)) old.set(k, c); });
  for (const [k, c] of [...old]) if (!want.has(k)) { old.delete(k); if (k.startsWith("i:")) deskKeepMove(spare, c, null); else c.remove(); }
  // Placed in order after `prev`: worked out afresh each time, as a group further in can take a row from here (a
  // feature's piece that's also listed on its own).
  let prev = null;
  const at = () => (prev ? prev.nextElementSibling : live.firstElementChild);
  kids.forEach((f, x) => {
    const k = keys[x];
    let node = old.get(k);
    old.delete(k);
    if (node && node.parentNode !== live) node = null;
    if (!node && k.startsWith("i:")) node = pool.get(k)?.find((n) => !n._deskUsed);
    if (!node) { live.insertBefore(f, at()); f._html = f.outerHTML; f._deskUsed = true; prev = f; return; }
    node._deskUsed = true;
    if (node !== at()) deskKeepMove(live, node, at());
    prev = node;
    const h = f.outerHTML;
    if ((node._html ?? node.outerHTML) === h) return;
    node._html = h;
    if (node.matches(DESK_KEEP_INSIDE) && f.matches(DESK_KEEP_INSIDE)) { deskKeepAttrs(node, f); deskKeepPatch(node, f, pool, spare); }
    else morph(node, f);   // a row or a header: in place where its shape is the same (chat/patch.js)
  });
  // Left over: not in the fresh draw (a duplicate key). A row waits aside in case a later group takes it.
  for (let c = at(); c; ) { const next = c.nextElementSibling; if (c.matches(".dk-it[data-desk-item]")) deskKeepMove(spare, c, null); else c.remove(); c = next; }
}
/** Before the window's HTML is set: the lists lifted out, by their scroll key. */
function deskListsLift() {
  const kept = new Map();
  for (const el of document.querySelectorAll(DESK_KEPT_LISTS)) { moveTo(parkSpot(), el); kept.set(el.dataset.scrollKey, el); }
  return kept;
}
/** After: each list back in its fresh place, brought up to date; one not drawn any more is dropped. */
function deskListsSettle(kept) {
  if (!kept?.size) return;
  for (const fresh of document.querySelectorAll(DESK_KEPT_LISTS)) {
    const el = kept.get(fresh.dataset.scrollKey);
    if (!el) continue;
    kept.delete(fresh.dataset.scrollKey);
    const pool = new Map();
    for (const r of el.querySelectorAll(".dk-it[data-desk-item]")) { r._deskUsed = false; const k = `i:${r.dataset.deskItem}`; pool.set(k, [...(pool.get(k) || []), r]); }
    for (const c of el.querySelectorAll("*")) c._deskUsed = false;
    const spare = document.createElement("div");
    spare.hidden = true;
    document.body.append(spare);
    deskKeepAttrs(el, fresh);
    deskKeepPatch(el, fresh, pool, spare);
    fresh.replaceChildren();
    moveBack(fresh.parentNode, el, fresh);
    fresh.remove();
    spare.remove();
  }
  for (const el of kept.values()) el.remove();
}
