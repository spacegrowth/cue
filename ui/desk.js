// Desk IDs are independent of workspace projects. Legacy IPC fields named `project` still carry the Desk ID.
let deskSelected = (() => { try { return localStorage.getItem("cue.deskSelected"); } catch { return null; } })();
let deskAddingSession = null;
function deskSelection() {
  if (deskSelected !== null) return deskSelected;
  if (!projFilter) return "";
  const matches = (state.desk?.projects || []).filter((d) => (d.workspace_project || d.project) === projFilter);
  return matches.find((d) => d.project === projFilter)?.project || (matches.length === 1 ? matches[0].project : matches.length ? "" : projFilter);
}
function deskSelect(id) {
  if (id !== deskSelected) deskAddingSession = null;
  deskSelected = id;
  try { if (id === null) localStorage.removeItem("cue.deskSelected"); else localStorage.setItem("cue.deskSelected", id); } catch {}
}
const deskWorkspace = (d) => d.workspace_project || d.project;
const deskName = (d) => d.name || projLabel(d.project);

// Cue UI desk — the Desk view: the Desk's sessions, its items (a bug, a feature, a task), who holds each (a lead and its crew, a
// session on its own, or the add-on's), what each waits on, the chat with the project's lead (desk_lead.js: who runs
// the Desk, Give to…, the quick-add line) and today's decisions. With All projects picked, every project's Desk as a
// card (what waits on you, what got done, what was decided).
// The data is Cue's `state.desk` (hub: items, the lead, holders by session, decisions); the commands are desk_send,
// desk_add, desk_give, desk_set_lead, desk_item_act and desk_undo.

/** What the chat card shows besides the lead's chat: null (the chat, or Decisions), or { kind: "item", id },
 *  { kind: "holder", sid } (a lead or a session), { kind: "crew", lead, agent } (a lead's crew member), or
 *  { kind: "addon", key } (a holder the add-on added), or { kind: "chat", sid } (a session's chat, from the sessions
 *  on the left); each with the `project` it was opened in (another project
 *  picked closes it). Its row in the list lights in place. */
let deskOpen = null;
/** The project whose lead row you clicked on the left (its item lit), till something else is shown. */
let deskLeadLit = null;
/** The chat card's tab while nothing is open: "lead" (its chat) or "decisions". */
let deskTab = "lead";
/** The item whose Drop was clicked once: it now reads "Drop it", and a second click drops it. */
let deskDropArm = null;
/** The item whose Send back is open: its own "Why send it back?" box in place of its buttons (deskBackHtml). */
let deskBackFor = null;
/** Where each box on the Desk sends, by its draft key: { project, item } (the lead, naming the item when there is
 *  one) or { sid, mate } (a session, naming its crew member). Kept beside the key, never read back out of it. */
const deskBoxes = new Map();
/** What you wrote to a project's lead that its chat doesn't show yet: project → [{ text, at, sent }] (`sent`: when
 *  the send returned, or 0). */
const deskSent = new Map();
/** The lead's chat was at its end before the draw (it stays there as new messages come). */
let deskChatEnd = true;

/** Seconds or ms, as the hub stamps them, in ms. */
const deskMs = (t) => (!t ? 0 : t < 1e12 ? t * 1000 : t);
/** Project `p`'s Desk ({ project, lead, items, decisions, members: its sessions' ids }), or an empty one. A dropped item is gone from it. */
const deskOf = (p) => {
  const d = (state.desk?.projects || []).find((x) => x.project === p);
  return d ? { ...d, items: d.items.filter((i) => i.state !== "dropped") } : { project: p, lead: null, items: [], decisions: [] };
};
// An item by its number, or by the tag it had before items were numbered (its alias), so old lines still find it.
const deskIdIs = (i, id) => i.id === id || (!!i.alias && i.alias === id);
const deskItem = (p, id) => deskOf(p).items.find((i) => deskIdIs(i, id)) || null;
/** An item as people name it: its kind and number, "Bug #2", "Task #1"; "#2" while it has no kind yet. */
/** An item's kind as a small line icon in the one theme color (the "Type marks" pick): a checked box, a bug, a
 *  sparkle. The one helper for it: every place an item's kind shows (the item list, the left list, the item page,
 *  the lead's route and new-item cards, Ready to check) takes it from here, styled by .dk-kic. Takes an item or a
 *  kind; "" for none. */
const DESK_KIND_SVG = {
  task: '<rect x="4" y="4" width="16" height="16" rx="4"/><path d="m8.5 12.2 2.4 2.4 4.6-5"/>',
  bug: '<rect x="8" y="7" width="8" height="12" rx="4"/><path d="M12 11v8M8 12H4.5M19.5 12H16M8 16.5l-3 1.5M16 16.5l3 1.5M8.5 9 6 7M15.5 9 18 7M10 7a2 2 0 0 1 4 0"/>',
  feature: '<path d="M12 3.5 13.9 10.1 20.5 12 13.9 13.9 12 20.5 10.1 13.9 3.5 12 10.1 10.1Z"/>',
  // What a chat is (deskLabel): the Desk's dispatcher (a signpost), a session, a crew member, a lead.
  desk: '<path d="M12 3v18M9 21h6"/><path d="M12 5h6l2.5 2.5L18 10h-6"/><path d="M12 12H6l-2.5 2.5L6 17h6"/>',
  session: '<path d="M5 5h14a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-8l-5 4v-4H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2z"/>',
  crew: '<circle cx="12" cy="8" r="4"/><path d="M4.5 20a7.5 7.5 0 0 1 15 0"/>',
  lead: '<circle cx="9" cy="8" r="3.5"/><path d="M2.5 20a6.5 6.5 0 0 1 13 0M16 4.5a3.5 3.5 0 0 1 0 7M18 14.5a6.5 6.5 0 0 1 3.5 5.5"/>',
};
const deskKindIcon = (i) => ((i = typeof i === "string" ? i : i?.kind), DESK_KIND_SVG[i] ? `<svg class="dk-kic" aria-hidden="true" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${DESK_KIND_SVG[i]}</svg>` : "");
const deskNum = (i) => `${["bug", "feature", "task"].includes(i?.kind) ? `${i.kind[0].toUpperCase()}${i.kind.slice(1)} ` : ""}#${i?.id ?? ""}`;
/** Since midnight, here. */
const deskToday = (t) => deskMs(t) >= new Date(now()).setHours(0, 0, 0, 0);
/** A DONE's (`d.checks`) Cue work not in the installed Cue yet: Cue was installed before its work last changed
 *  (`edited`). A design item's (one with a canvas) is its mocks, nothing to install. */
const deskChkUninstalled = (c) => !!c?.cue && !/^https?:\/\//.test(c.designs || "") && !((state.desk?.installed_ms || 0) > (c.edited || c.at));
/** Item `i` is done but changes the app, and the installed Cue doesn't have it yet: nothing to try, so it isn't to check
 *  yet; it says "waiting for install" till Cue is installed. */
const deskInstallWait = (i) => i?.state === "to_check" && !deskFinCommit(i) && deskChkUninstalled((state.desk?.projects || []).find((x) => x.project === i.project)?.checks?.find((c) => c.id === i.id && c.last));
/** The commit Cue made for item `i` when its session said DONE (hub: commit, build, install, then to check), or "". An
 *  item with one went through that, so it's in the installed Cue: never "waiting for install". */
const deskFinCommit = (i) => (i?.finish ? i.finish.commit || i.commit || "" : "");
/** Item `i`'s DONE on its way into the app: Cue commits, builds and installs it. Nothing for you to do yet. */
const deskFinishing = (i) => i?.state === "finishing";
/** Where a finishing item is, in a short line: "committing · waits: web-lead is editing ui/desk.js", "building…". */
function deskFinStep(i) {
  const f = i.finish || {};
  if (f.step === "commit_wait") return `committing${f.wait ? ` · waits: ${f.wait}` : ""}`;
  if (f.step === "queued") return `in the next install${state.desk?.install_at ? ` · ${clockAt(state.desk.install_at)}` : ""}`;
  return { building: "building…", installing: "installing…", starting: "starting…" }[f.step] || "committing…";
}
/** Install now on a finishing item in the next install (step "queued"): everything waiting installs at once. */
const deskInstallNow = (i, cls) => (deskFinishing(i) && i.finish?.step === "queued" ? `<button class="${cls}" data-desk-act="install_now" data-id="${esc(i.id)}">Install now</button>` : "");
/** The Desk's next install, in its Items header: "Next install 10:30 · 2 items" and Install now, while anything (on
 *  any Desk: there's one Cue) waits for it. Cue installs it on its own half an hour after the first of them. */
function deskNextInstall() {
  const waiting = (state.desk?.projects || []).flatMap((p) => p.items || []).filter((i) => deskFinishing(i) && i.finish?.step === "queued");
  if (!waiting.length) return "";
  const at = state.desk?.install_at ? ` ${clockAt(state.desk.install_at)}` : "";
  return `<span class="dk-next">Next install${at} · ${waiting.length} ${waiting.length === 1 ? "item" : "items"}</span>${deskInstallNow(waiting[0], "dk-instnow")}`;
}
const deskWaits = (i) => i.state === "needs_you" || (i.state === "to_check" && !deskInstallWait(i));
/** Items you've opened since they came back to check: id → its `updated` then (kept across restarts).
 *  SHORTCUT: kept in this window only, so another window or device still shows them as landed; move to a `seen`
 *  stamp on the item (hub/desk.rs, a desk_seen command) when the Desk is shown anywhere else. */
const deskSeen = (() => { try { return JSON.parse(localStorage.getItem("cue.deskSeen") || "{}"); } catch { return {}; } })();
/** Just landed: back to check and not opened since (it stands out till you open it). */
const deskLanded = (i) => i.state === "to_check" && !deskInstallWait(i) && deskSeen[i.id] !== i.updated;
/** Item `id` opened: it no longer stands out (only the items still to check are kept). */
function deskSeeItem(id) {
  const i = (state.desk?.projects || []).flatMap((d) => d.items).find((x) => x.id === id);
  if (!i || i.state !== "to_check" || deskSeen[id] === i.updated) return;
  deskSeen[id] = i.updated;
  const open = new Set((state.desk?.projects || []).flatMap((d) => d.items).filter((x) => x.state === "to_check").map((x) => x.id));
  for (const k of Object.keys(deskSeen)) if (!open.has(k)) delete deskSeen[k];
  try { localStorage.setItem("cue.deskSeen", JSON.stringify(deskSeen)); } catch {}
}
/** A thing an item's holder made, as a link's words: a page, a file by its name. */
function deskMadeLabel(m) {
  if (/^https?:\/\//.test(m)) {
    if (/claude\.ai\/(code\/)?artifact\//.test(m)) return "Open the page";
    try { return new URL(m).host.replace(/^www\./, ""); } catch { return "Open the link"; }
  }
  return m.split("/").pop();
}
/** Links to what an item's holder made (its pages and files, from the turn that said DONE), and its changes when it
 *  has files: in its pane. */
function deskMadeLinks(i) {
  const links = (i.made || []).map((m) => `<button class="dk-made" data-desk-made="${esc(m)}">${esc(deskMadeLabel(m))} ↗</button>`);
  return links.length ? `<div class="dk-mades">${links.join("")}</div>` : "";
}
/** A to-check or done item's Files changed, as Looks right commits them (or committed them): item id → { key, files }
 *  (files null while it's read). `key` is what it was read for, so a new check or commit reads it again; `page` the
 *  page it was read for. */
const deskDiffs = new Map();
const deskDiffKey = (i) => [i.state, i.commit || "", i.updated || "", (i.files || []).length, (i.commit_wait?.files || []).length].join("|");
function deskFilesChanged(i) {
  // Still being worked on: what's changed in its files so far (#149), once it's on some.
  const checking = i.state === "to_check" || i.state === "done";
  if (!checking && !(i.files || []).length) return "";
  // Read again each time its page is opened too: its files can change on disk with nothing else about it changing.
  const key = deskDiffKey(i), got = deskDiffs.get(i.id);
  if (got?.key !== key || got.page !== deskOpen) {
    deskDiffs.set(i.id, { key, page: deskOpen, files: got?.files || null });
    const keep = (files) => { if (deskDiffs.get(i.id)?.key !== key) return; deskDiffs.set(i.id, { ...deskDiffs.get(i.id), files }); if (deskOpen?.id === i.id) renderMain(); };
    invoke("desk_item_diff", { id: i.id }).then((r) => keep(diffParse(r?.diff || "").files), () => keep([]));
  }
  const files = deskDiffs.get(i.id).files;
  // While its first read is on the way: the same rows from the files it says it changed, counts to come. Without them
  // the page drew "Files" as a line, then swapped it for these rows a moment later: it flickered as it turned to check.
  // Being worked on with nothing changed in them yet (or not in git): the files it's on, as they are.
  if (!files || (!checking && !files.length)) return (i.commit_wait?.files || i.files || []).map((f) => `<div class="dk-fc wait"><span class="dk-fc-p">${esc(f)}</span></div>`).join("");
  return files.map((f, n) => `<button class="dk-fc" data-desk-diff-file="${esc(i.id)}" data-n="${n}"><span class="dk-fc-p">${esc(f.path)}</span>${f.isNew ? `<span class="dk-fc-new">new</span>` : ""}<span class="dv-n dv-add">+${f.add}</span><span class="dv-n dv-del">−${f.del}</span></button>`).join("");
}
/** Item `id`'s file `n`, over the Desk (diff_overlay.js): its Files changed on the left, that file's diff on the right.
 *  You stay on the Desk; Esc closes it. */
function deskShowFile(id, n) {
  const i = (state.desk?.projects || []).flatMap((d) => d.items).find((x) => x.id === id);
  if (!i) return;
  deskSeeItem(id);
  diffOverlay(`${deskNum(i)} ${i.title || ""}`.trim(), "Files changed", deskDiffs.get(id)?.files || [], n);
}
const deskDoneToday = (i) => i.state === "done" && deskToday(i.done_at || i.updated);
const deskMoving = (i) => i.state === "working" || i.state === "queued" || deskFinishing(i);
/** A title cut short for a button or a line ("Search stays centred"): its first few words. */
const deskShort = (t) => { const w = String(t || "").split(/\s+/); return w.length > 3 ? w.slice(0, 3).join(" ") : t; };

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
/** A question, an ask or a "done when" in one plain line: its line that asks (ends on "?"), else its first. */
function deskOneLine(t) {
  const lines = String(t || "").split(/\n+/).map((l) => plain(l)).filter(Boolean);
  return lines.findLast((l) => /\?$/.test(l)) || lines[0] || "";
}
/** What an item waiting on you asks you to do, in one plain line (#182): [verb, words], or null when it doesn't wait
 *  on you. Allow a tool, Pick one of its choices (or of its mocks), Answer its question; Try it once it's in the app
 *  (its "done when"). */
function deskVerb(i) {
  if (i.state === "to_check" && !deskInstallWait(i)) return deskPickLine(i) ? ["Pick", deskPickLine(i)] : ["Try it", deskDoneWhenPoints(i.done_when || "")[0] || i.title];
  if (i.state !== "needs_you") return null;
  const a = i.ask;
  if (a) return [a.kind === "permission" ? "Allow" : (a.choices || []).length > 1 ? "Pick" : "Answer", deskOneLine(a.text)];
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
/** The list's groups by state: Waiting on you, In progress, Queued (given out or not), the add-on's, and Done today. An
 *  item is in one group only. Who holds an item is the small word under its title; the sessions are on the left. */
function deskGroups(project) {
  const d = deskOf(project);
  const items = [...d.items].sort((a, b) => deskMs(a.created) - deskMs(b.created));
  // What asks you something before what's ready to check; then what unblocks the most first, then the oldest (#182).
  const ub = new Map(items.map((i) => [i.id, deskUnblocks(i, items).length]));
  const waiting = items.filter(deskWaits).sort((a, b) => (a.state === "needs_you" ? 0 : 1) - (b.state === "needs_you" ? 0 : 1) || ub.get(b.id) - ub.get(a.id));
  const done = items.filter(deskDoneToday).sort((a, b) => deskMs(b.done_at) - deskMs(a.done_at));
  const rest = items.filter((i) => !deskWaits(i) && i.state !== "done");
  let extra = [];
  try { extra = addOn.deskHolders?.(project) || []; } catch (e) { console.error(e); }
  // In progress: its holder is on it. One left working by a crash or quit (its holder idle) waits with the queued.
  // One whose holder stopped on it is in progress too: the Desk lead is settling it (#182).
  const onIt = (i) => i.state === "working" && (!deskHolderIdle(i.holder) || i.stopped);
  // Waiting for install: still on its way to you, with what's in progress.
  const going = (i) => onIt(i) || deskInstallWait(i) || deskFinishing(i);
  return { d, waiting, working: rest.filter(going), queued: rest.filter((i) => !going(i)), extra, done, all: items };
}

// ---------- the list ----------

/** An item's state as its row's tag: [words, look]. Only an ask and a stuck one have words; the rest say it by their
 *  group. Back to check: a dot before it while not opened since (landed, its title bold too), nothing once opened. */
function deskTag(i) {
  // An ask carries no tag: its question, boxed under the title, says it.
  if (i.state === "needs_you") return ["", "ask"];
  if (deskInstallWait(i)) return ["", ""];
  if (i.state === "to_check") return deskLanded(i) ? ["", "landed"] : ["", "check"];
  // Its new session wouldn't start (#172): says so, its page has why and Retry.
  if ((i.state === "new" || i.state === "queued") && !i.holder && i.start_failed) return ["couldn't start", "ask"];
  if ((i.state === "new" || i.state === "queued") && i.route_failed) return ["stuck", "ask"];
  return ["", i.state === "done" ? "done" : ""];
}
/** Where an item stands, as its row's icon: waiting on you (an ask, or back to check), in progress, queued (also
 *  one waiting for its install), done. */
const deskStateOf = (i) => (i.state === "done" ? "done" : i.state === "working" ? (i.stopped ? "q" : "work") : deskFinishing(i) ? "fin" : (i.state === "needs_you" || i.state === "to_check") && !deskInstallWait(i) ? "you" : "q");
const DESK_STATE_SVG = {
  you: '<circle cx="7" cy="7" r="5.5" stroke="var(--muted)" stroke-width="1.6"/><path d="M7 4.2v3.4" stroke="var(--muted)" stroke-width="1.6"/><circle cx="7" cy="9.9" r=".9" fill="var(--muted)"/>',
  work: '<circle cx="7" cy="7" r="5.5" stroke="var(--accent)" stroke-width="1.5"/><circle class="dk-blink" cx="7" cy="7" r="2.2" fill="var(--accent)"/>',
  q: '<circle cx="7" cy="7" r="5.5" stroke="var(--muted)" stroke-width="1.5"/><path d="M7 4.3V7l1.9 1.3" stroke="var(--muted)" stroke-width="1.5"/>',
  // Finishing: amber and steady (no blink), as nothing waits on you.
  fin: '<circle cx="7" cy="7" r="5.5" stroke="var(--amber)" stroke-width="1.5"/><circle cx="7" cy="7" r="2.2" fill="var(--amber)"/>',
  done: '<path d="m2.8 7.4 2.8 2.8 5.6-6" stroke="var(--muted)" stroke-width="1.7"/>',
};
const deskStateIcon = (i) => `<svg class="dk-si" width="14" height="14" viewBox="0 0 14 14" fill="none" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${DESK_STATE_SVG[deskStateOf(i)]}</svg>`;
/** Who holds an item, in a few words: "web-lead → search-centre" for its crew member. */
const deskHolderOf = (i) => (!i.holder ? "" : i.holder.kind === "crew" ? `${deskHolderName(i.holder.lead_sid)} → ${i.holder.name}` : i.crew ? `${i.holder.name} → ${i.crew}` : i.holder.name);
/** One item in two rows: its state, #id and title; under them who holds it (Give to… when no one does), its age (a
 *  done one's time it finished), and a quiet ✓ when it's ready to check (Looks right; it fills on hover); an ask's question below that. Lit
 *  when it's open, or its crew member is. */
function deskItemRow(i, lit, held = false) {
  const on = lit || (deskOpen?.kind === "item" && deskOpen.id === i.id);
  const [tag, look] = deskTag(i);
  // Lit by its session (clicked on the left) and at work: "working · host-lead · 4m", how long its session has been at it.
  const s = held && i.state === "working" ? sessionOf(i.holder?.sid) : null;
  const at = s ? (s.state === "working" && s.since_ms) || deskMs(i.updated) : 0;
  const who = i.stopped && i.state === "working" ? `stopped · the Desk lead is on it` : s ? `working · ${esc(deskHolderOf(i))} · <span data-ago="${at}">${ago(at)}</span>` : !i.holder && i.state !== "done" ? deskGive(i, "row") : deskInstallWait(i) ? `waiting for install · ${esc(deskHolderOf(i))}` : deskFinishing(i) ? `<span class="dk-fstep">${esc(deskFinStep(i))}</span>` : esc(deskHolderOf(i));
  // Finishing: no age, so its step has the room.
  const when = s || deskFinishing(i) ? "" : i.state === "done" ? `${i.commit ? `<span class="dk-cm">committed ${esc(i.commit)}</span> · ` : ""}${clockAt(deskMs(i.done_at || i.updated))}` : ago(deskMs(i.created));
  const ok = deskInstallWait(i) || deskFinishing(i) ? "" : i.state === "to_check" && deskLooksSoon.has(i.id) ? deskLooksSoonHtml(i.id) : i.state === "to_check" ? `<button class="dk-ok" data-desk-act="looks_right" data-id="${esc(i.id)}" aria-label="Looks right"></button>`
    : i.state === "done" ? `<button class="dk-undo" data-desk-act="reopen" data-id="${esc(i.id)}">Undo</button>` : "";
  // What it waits on you for, in one plain line (Answer, Pick, Allow, Try it), and what it unblocks (#182).
  const vb = deskVerbHtml(i);
  const ask = (vb ? `<div class="dk-h">${vb}</div>` : "") + (deskWaits(i) ? deskUnblocksHtml(i) : "");
  return `<div class="dk-it ${i.state === "needs_you" ? "ask" : ""} ${look === "landed" ? "landed" : ""} dk-${deskStateOf(i)} ${on ? "on" : ""}${i.state === "to_check" && deskLooksSoon.has(i.id) ? " soon" : ""}${deskLeaveStyle(i.id)}" role="button" tabindex="0" data-desk-item="${esc(i.id)}">
    ${deskStateIcon(i)}<div class="dk-i1"><span class="dk-tag ${look}">${tag}</span><span class="dk-t">${esc(i.title)}</span></div>
    <div class="dk-i2"><span class="dk-kid">${deskKindIcon(i)}<span class="dk-id">${esc(deskNum(i))}</span></span><span class="dk-who${s ? " held" : ""}">${who}</span><span class="dk-age">${when}</span>${ok}</div>${ask}</div>`;
}
/** An add-on's head: its name, what it is, › (it opens). */
const deskHolderHead = (h, on) => `<div class="dk-holder ${on ? "on" : ""}" role="button" tabindex="0" ${h.addon ? `data-desk-addon="${esc(h.key)}"` : `data-desk-holder="${esc(h.sid)}"`}><span class="dk-nm">${esc(h.name)}</span><span class="dk-s">${esc(h.sub || "")}</span><span class="grow"></span><span class="dk-go">›</span></div>`;
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
  return [...byId.values()].sort((a, b) => deskMs(b.updated) - deskMs(a.updated) || (a.id < b.id ? 1 : a.id > b.id ? -1 : 0));
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

function deskList(project) {
  const g = deskGroups(project);
  const o = deskOpen;
  const crewLit = o?.kind === "crew" ? deskCrewItem(project, o.lead, o.agent)?.id : null;
  // A session clicked on the left lights nothing on the right (#170): it can hold several items.
  const row = (i) => deskItemRow(i, i.id === crewLit, false);
  const parts = [];
  // Waiting on you stays at 0 (in red only while something waits), so you see there's nothing for you.
  parts.push(`<div class="dk-gh you${g.waiting.length ? "" : " none"}">Waiting on you <span class="n">${g.waiting.length}</span></div>${g.waiting.map(row).join("")}`);
  if (g.working.length) parts.push(`<div class="dk-gh">In progress <span class="n">${g.working.length}</span></div>${g.working.map(row).join("")}`);
  if (g.queued.length) parts.push(`<div class="dk-gh">Queued <span class="n">${g.queued.length}</span></div>${g.queued.map(row).join("")}`);
  for (const h of g.extra) parts.push(deskHolderHead({ ...h, addon: true, key: h.key || h.name }, o?.kind === "addon" && o.key === (h.key || h.name)) + (h.items || []).map(row).join(""));
  // Finished: the 10 newest, then 10 more each time the list's end comes into view (deskDoneWatch). Its end holds
  // its place while the next ones load, so nothing jumps.
  const pg = deskDonePages.get(project);
  if (!pg) queueMicrotask(() => deskDoneFetch(project));
  const done = deskDoneList(g.d), shown = done.slice(0, deskDoneShown.get(project) || DESK_DONE_STEP);
  const more = done.length > shown.length || !pg?.end;
  if (shown.length) parts.push(`<div class="dk-gh">Done</div>${shown.map(row).join("")}`);
  if (more && (shown.length || pg?.loading || !pg)) parts.push(`<div class="dk-more" data-desk-done-more="${esc(project)}">${shown.length ? "Loading older…" : ""}</div>`);
  const body = parts.join("") + (parts.length > 1 ? "" : `<div class="dk-empty">No items yet.</div>`);
  return `<div class="dk-card dk-items"><div class="dk-ch"><b>Items</b>${deskNextInstall()}</div>
    ${deskQuickAdd()}<div class="dk-scroll" data-scroll-key="desk-items:${esc(project)}">${body}</div></div>`;
}

// ---------- the sessions: the project's sessions, on the Desk and not ----------

/** A session's state, for its row: [dot, words]. Waiting on you when it asks you something. */
function deskSessState(s) {
  const asks = s.state === "deciding" || state.items.some((i) => i.session_id === s.session_id && i.status === "pending" && i.kind !== "waiting");
  return asks ? ["ask", "waiting on you"] : s.state === "working" ? ["", "working"] : ["idle", "idle"];
}
/** The item a session holds (an ask before what's at work), or null. */
const deskSessItem = (d, sid) => deskSessItems(d, sid)[0] || null;
/** Every open item session `sid` holds, the one it's on now first (working, then waiting on you, then the newest). */
const deskSessItems = (d, sid) => d.items.filter((i) => i.state !== "done" && i.holder && i.holder.kind !== "crew" && i.holder.sid === sid)
  .sort((a, b) => (b.state === "working") - (a.state === "working") || (b.state === "needs_you") - (a.state === "needs_you") || (b.updated ?? 0) - (a.updated ?? 0));
/** A session row's size and cache, as its card says them: "142k · cache 3m" (minutes its cache has left)
 *  or "142k · cold"; amber with under 5 minutes left. With `when` (an idle row), when its latest reply came first:
 *  "2h ago · 142k · cold". From its card's numbers (insights.js insCache), asked for here when no card brings them
 *  (deskInsAsk); "" without. */
function deskSessCache(sid, when = false) {
  deskInsAsk(sid);
  const c = typeof insCache !== "undefined" ? insCache.get(sid) : null;
  if (!c?.tokens) return "";
  const left = (c.until_ms || 0) - now();
  const cache = left > 0 ? ` · cache ${Math.ceil(left / 60000)}m` : c.until_ms ? " · cold" : "";
  const at = when && c.last_ms ? `<span data-ago="${c.last_ms}">${ago(c.last_ms)}</span> ago · ` : "";
  return `<div class="dk-s3 dk-size${left > 0 && left < HOT_CACHE_MS ? " hot" : ""}">${at}${esc(insTok(c.tokens))}${cache}</div>`;
}
/** The first line of a session's latest reply, from the same numbers ("" until they're in). */
const deskSessSaid = (sid) => (typeof insCache !== "undefined" && insCache.get(sid)?.said) || "";
/** Sessions on a Desk whose numbers no card brings (an idle one with its card gone, #185): asked for together,
 *  each again a minute after it was last asked, so its last reply, size and cache show and stay current. */
const deskInsAt = new Map();
let deskInsWant = null;
function deskInsAsk(sid) {
  if (!sid || typeof insCache === "undefined" || (state.items || []).some((i) => i.session_id === sid)) return;
  if (now() - (deskInsAt.get(sid) || 0) < 60000) return;
  deskInsAt.set(sid, now());
  if (deskInsWant) return deskInsWant.add(sid);
  deskInsWant = new Set([sid]);
  setTimeout(async () => {
    const sids = [...deskInsWant];
    deskInsWant = null;
    try {
      const r = (await invoke("insight_state", { sessionIds: sids })) || {};
      let changed = false;
      for (const id of sids) {
        const c = r[id], had = insCache.get(id);
        if (c && (!had || (c.last_ms || 0) >= (had.last_ms || 0)) && JSON.stringify(c) !== JSON.stringify(had)) { insCache.set(id, c); changed = true; }
      }
      if (changed) renderMain();
    } catch {}
  }, 0);
}
/** A working session's (or teammate's) current step and how long it's been at it, kept live (window.js ticks
 *  data-ago): "Editing ui/desk.js · 4m". "" when it isn't working or says no step. */
const deskStepLine = (working, step, at) => (working && step ? `<div class="dk-s3 dk-now">${esc(step)}${at ? ` · <span data-ago="${at}">${ago(at)}</span>` : ""}</div>` : "");
/** A session's name with ✎ beside it, shown on hover, that renames the session (as the Active pane's header does:
 *  data-desk-ren → renameSession, Enter keeps it, Esc or leaving the box doesn't); the box itself while you do. */
const DESK_PEN = `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 20h4L19 9l-4-4L4 16z"/><path d="M14 6l4 4"/></svg>`;
function deskSessName(sid, name, cls) {
  if (renaming === sid) return `<input class="rename-in dk-ren-in" data-text="rename:${esc(sid)}" maxlength="60" placeholder="Name this session" aria-label="Rename this session" spellcheck="false" value="${esc(draft(`rename:${sid}`).text)}"/>`;
  return `<${cls === "b" ? "b" : `span class="${cls}"`}>${esc(name)}</${cls === "b" ? "b" : "span"}>${sessionOf(sid) ? `<button class="dk-ren dk-sren" data-desk-ren="${esc(sid)}" aria-label="Rename this session">${DESK_PEN}</button>` : ""}`;
}
/** One row: what it is as its icon before the name (`mark`: desk, lead, session or crew, the icons the chat headers use,
 *  #202), its name, its state; then the item it's on (no line when it's on none, #179); then its size and cache. `cls`
 *  is dk-srow; the lead's card in the head strip is dk-lcard, with `pre` "Run by" before its icon. Rows run edge to
 *  edge, so their hover and their lime reach both sides of the column. */
function deskSessRow({ name, sid = "", mark = "", dot, word, item, line, on, attrs, crew, step = "", size = "", loose = false, past = false, cls = "dk-srow", pre = "" }) {
  const what = item ? `<span class="dk-w">${deskKindIcon(item)}<span class="dk-id">${esc(deskNum(item))}</span> ${esc(item.title)}</span>` : line ? `<span class="dk-w">${esc(line)}</span>` : "";
  const loosePill = loose ? `<span class="dk-pill loose">not committed</span>` : "";
  const ic = DESK_MARKS[mark] ? `<span class="dk-ric" role="img" data-desk-type="${mark}" aria-label="${DESK_MARKS[mark]}">${deskKindIcon(mark)}</span>` : "";
  return `<div class="${cls} ${crew ? "crew" : ""} ${past ? "past" : ""} ${on ? "on" : ""}" role="button" tabindex="0" ${attrs}>
    <div class="dk-s1">${pre ? `<span class="dk-lk">${pre}</span>` : ""}${ic}${sid ? deskSessName(sid, name, "dk-nm") : `<span class="dk-nm">${esc(name)}</span>`}<span class="dk-st"><i class="${dot}"></i><b>${word}</b></span></div>
    ${what || loosePill ? `<div class="dk-s2">${what}${loosePill}</div>` : ""}${step}${size}</div>`;
}
/** The lead and the sessions already on this Desk, with their crew. */
function deskSessions(project) {
  const d = deskOf(project), o = deskOpen, leadSid = deskLead(d)?.sid;
  const sessions = (state.sessions || []).filter((s) => s.project === deskWorkspace(d) && s.session_id);
  const onDesk = (sid) => sid === leadSid || (d.members || []).includes(sid);
  const lit = (sid) => (sid === leadSid && !o && deskPick !== project) || ((o?.kind === "chat" || o?.kind === "holder") && o.sid === sid);
  const row = (s, card = false) => {
    const sid = s.session_id, [dot, word] = deskSessState(s);
    const lead = sid === leadSid || deskIsLead(sid, d.items.filter((i) => deskGroupKey(i.holder) === sid));
    const attrs = sid === leadSid ? `data-desk-lead-chat="${esc(sid)}"` : `data-desk-chat-of="${esc(sid)}"`;
    const mate = (t, past) => {
      const key = t.agent_id || t.call, it = past ? null : deskCrewItem(project, sid, key);
      const [cd, cw] = past ? ["idle", t.state === "aside" ? "set aside" : t.state] : mateNeeds(sid, t) ? ["ask", "waiting on you"] : deskMateBusy(t) ? ["", t.state] : ["idle", t.state];
      return deskSessRow({ name: t.name, mark: "crew", dot: cd, word: cw, item: it, line: it ? "" : t.what, crew: true, past, loose: !past && !deskMateBusy(t) && !!t.uncommitted,
        step: past ? "" : deskStepLine(cd === "", t.step, t.step_ms || t.started_ms),
        on: o?.kind === "crew" && o.lead === sid && o.agent === key, attrs: `data-desk-crew="${esc(key)}" data-lead="${esc(sid)}"` });
    };
    // Rows only for crew still in play: at work, asking you, or with files git says aren't committed. The rest
    // (committed, set aside, or finished with nothing left to commit) fold into one "N done" line, opened in place and
    // kept the way you left it (deskFoldOpen).
    const team = sessionOf(sid)?.team || [];
    const rank = (t) => (deskMateBusy(t) ? 0 : mateNeeds(sid, t) ? 1 : 2);
    const live = team.filter((t) => deskMateLive(sid, t)).sort((a, b) => rank(a) - rank(b)), done = team.filter((t) => !deskMateLive(sid, t)), open = deskFoldOpen.has(sid);
    const fold = done.length ? `<button class="dk-sfold" data-desk-done-fold="${esc(sid)}" aria-expanded="${open}">${done.length} done · ${open ? "hide" : "show"}</button>${open ? done.map((t) => mate(t, true)).join("") : ""}` : "";
    // The Desk's own lead shows no crew (it routes; any teammates it had, even old ones, aren't the Desk's).
    const crew = sid === leadSid ? "" : live.map((t) => mate(t, false)).join("") + fold;
    if (card) return deskSessRow({ name: deskHolderName(sid), sid, cls: "dk-lcard", pre: "Run by", mark: "desk", dot, word, item: deskSessItem(d, sid), on: lit(sid), attrs, step: deskStepLine(dot === "", s.compacting_ms ? "Compacting…" : s.doing, s.compacting_ms || s.doing_ms), size: deskSessCache(sid) });
    // On no item: the first line of its latest reply, and when it came beside its size (#185).
    const item = deskSessItem(d, sid), idle = dot !== "" && !item;
    return deskSessRow({ name: deskHolderName(sid), sid, mark: sid === leadSid ? "desk" : lead ? "lead" : "session", dot, word, item, line: idle ? deskSessSaid(sid) : "", on: lit(sid), attrs, step: deskStepLine(dot === "", s.compacting_ms ? "Compacting…" : s.doing, s.compacting_ms || s.doing_ms), size: deskSessCache(sid, idle) }) + crew;
  };
  const leadS = sessions.find((s) => s.session_id === leadSid);
  const mine = sessions.filter((s) => s !== leadS && onDesk(s.session_id));
  // At work: the session itself, its background agents (state "agent" with helpers running), or any of its crew. A
  // session idle while its crew works is listed with the working ones, so their rows never sit under idle sessions.
  const busy = (s) => deskSessState(s)[1] === "working" || (s.state === "agent" && !!s.helpers)
    || (sessionOf(s.session_id)?.team || []).some(deskMateBusy);
  // Each group most recent first: the one whose state changed last on top (#184). A row moves only when its state
  // does, never on a click.
  const recent = (list) => list.sort((a, b) => (b.since_ms || 0) - (a.since_ms || 0));
  const atWork = recent(mine.filter(busy));
  const on = recent(mine.filter((s) => !busy(s)));
  const group = (title, list) => (list.length ? `<div class="dk-gh">${title} <span class="n">${list.length}</span></div>${list.map((s) => row(s)).join("")}` : "");
  // The strip's meta line counts this Desk's sessions, and the working ones as their rows show them (deskSessState).
  const runs = mine.filter((s) => deskSessState(s)[0] === "").length;
  const body = group("Working", atWork) + group("On the Desk", on) || '<div class="dk-empty">No other sessions on this Desk.</div>';
  const picking = deskAddingSession === project;
  const candidates = picking ? deskSessionsOf(project, d).filter((r) => !onDesk(r.sid)) : [];
  const picker = picking ? `<div class="dk-add-picker" role="group" aria-label="Sessions to add">${candidates.length
    ? candidates.map((r) => `<button class="btn small" data-desk-member="${esc(r.sid)}" data-project="${esc(project)}">${esc(r.name)}</button>`).join("")
    : '<div class="dim">No other sessions available.</div>'}</div>` : "";
  return `<div class="dk-card dk-sessions">${deskSessHead(project, mine.length, runs, leadS ? row(leadS, true) : "")}
    <div class="dk-scroll" data-scroll-key="desk-sessions:${esc(project)}">${body}</div>
    <div class="dk-sfoot"><button class="btn small" data-desk-add-session="${esc(project)}" aria-expanded="${picking}">Add session</button>${picker}<button class="btn small" data-desk-new-session="${esc(project)}">+ New session</button></div></div>`;
}
/** The head strip (pick B2), set in from the card's sides so it never meets the window's own colour: ‹ Desks back to
 *  Desk home, ⋯ to hand the Desk to another session, this Desk's name (✎ on hover renames it, as the projects list
 *  does), how many sessions and how many work; then the lead's card (`lead`), outlined while its chat shows. */
function deskSessHead(project, n, working, lead = "") {
  const name = pjRenaming === project
    ? `<input class="pj-name-in" data-text="pjname:${esc(project)}" maxlength="40" placeholder="${esc(project)}" aria-label="Rename this Desk" spellcheck="false" value="${esc(draft(`pjname:${project}`).text)}"/>`
    : `<b>${esc(deskName(deskOf(project)))}</b><button class="dk-ren" data-pjren="${esc(project)}" aria-label="Rename this Desk"><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 20h4L19 9l-4-4L4 16z"/><path d="M14 6l4 4"/></svg></button>`;
  const more = lead ? `<button class="dk-more-btn" data-desk-change aria-label="Hand this Desk to another session"><svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><circle cx="5" cy="12" r="1.8"/><circle cx="12" cy="12" r="1.8"/><circle cx="19" cy="12" r="1.8"/></svg></button>` : "";
  return `<div class="dk-strip"><div class="dk-sh"><div class="dk-stop"><button class="dk-home-btn" data-desk-home>‹ Desks</button><span class="grow"></span>${more}</div><div class="dk-shr">${name}</div><span class="dk-sub">${n} session${n === 1 ? "" : "s"}${working ? ` · ${working} working` : ""}</span></div>${lead}</div>`;
}
/** + New, its folder that of a session in the project (else what + New would pick). */
/** Session `sid` on `project`'s Desk (`on`) or off it: the sessions its lead routes to without asking you. */
async function deskMember(project, sid, on) {
  try {
    await invoke("desk_member", { project, sid, on });
    if (deskAddingSession === project) {
      deskAddingSession = null;
      renderMain();
    }
  }
  catch (e) { toast(String(e)); }
}
/** ⋯ → Add to Desk / Remove from Desk, for a session of a project (not the Desk's lead, which is always on it). */
function deskMemberItem(sid, s) {
  const workspace = s?.project;
  if (!workspace || !sid) return "";
  const matches = (state.desk?.projects || []).filter((d) => deskWorkspace(d) === workspace);
  const targets = matches.length ? matches : [deskOf(workspace)];
  return targets.filter((d) => deskLead(d)?.sid !== sid).map((d) => {
    const on = (d.members || []).includes(sid);
    const label = targets.length > 1 ? deskName(d) : "Desk";
    return `<button data-desk-member-set="${esc(sid)}" data-project="${esc(d.project)}" data-on="${on ? "0" : "1"}">${on ? "Remove from" : "Add to"} ${esc(label)}<span>${on ? "Its lead asks before giving it work again" : "Its lead can give it work without asking you"}</span></button>`;
  }).join("");
}
function deskNewSession(project) {
  const d = deskOf(project);
  const s = (state.sessions || []).find((x) => x.project === deskWorkspace(d) && x.cwd && !x.machine);
  if (typeof lvAct === "function") lvAct("new", { spot: true, cwd: d.folder || s?.cwd || "" });
}

// ---------- the chat card: the lead's chat, Decisions, or what's open ----------

/** Decisions or steps as one quiet table: who | what | why (muted) | when | Undo (a small icon, shown on hover). The
 *  same line said again in a row is one row marked "×N" (its latest time and Undo). `about`: more for the muted line
 *  (the item a lead's decision was on). */
function deskLog(list, about = null) {
  const rows = [];
  for (const c of list) {
    const last = rows[rows.length - 1];
    if (last && last.c.text === c.text && (last.c.why || "") === (c.why || "") && last.c.call === c.call) { last.n++; if (deskMs(c.at) >= deskMs(last.c.at)) last.c = c; }
    else rows.push({ c, n: 1 });
  }
  return `<div class="dk-log">${rows.map(({ c, n }) => {
    const on = deskLogOpen.has(c.id), what = about?.(c) || "";
    return `<div class="dk-lr ${c.call ? "dk-dec" : "dk-step"}${on ? " open" : ""}" data-desk-log-row="${esc(c.id)}"><span class="dk-lby">${esc(c.by || "")}</span>`
      + `<span class="dk-d">${esc(c.text)}${n > 1 ? ` <span class="dk-x">×${n}</span>` : ""}${what ? ` <span class="dk-la">· ${what}</span>` : ""}</span>`
      + `<span class="dk-w">${c.why ? esc(c.why) : ""}</span><span class="dk-age">${ago(deskMs(c.at))} ago</span><span class="dk-lu">${deskUndoLink(c)}</span></div>`;
  }).join("")}</div>`;
}
/** Rows of a Decisions or Steps table shown in full (a click on a row; long ones are cut at two lines). */
const deskLogOpen = new Set();
const DESK_UNDO_ICON = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/></svg>`;
/** Undo at a row's end: a small quiet icon shown while the pointer is on the row; "asked" once it was. */
const deskUndoLink = (c) => (c.undo === "can" ? `<button class="dk-undo-l" data-desk-undo="${esc(c.id)}" aria-label="Undo">${DESK_UNDO_ICON}</button>`
  : c.undo === "asked" ? `<span class="dk-u">Undo asked</span>` : "");
/** Your words about an item ("About #69 (its title): …", as deskSendBox sends them): a head over them with its kind's
 *  icon, its kind and number and its title (a click opens it), and your words under it. null for any other words. */
function deskAboutHead(e) {
  const m = /^About #([^\s(]+) \(/.exec(e?.text || "");
  if (!m) return null;
  const it = (state.desk?.projects || []).flatMap((d) => d.items).find((i) => deskIdIs(i, m[1]));
  // The title as sent; when the item is gone or renamed since, up to the first "): ".
  const rest = e.text.slice(m[0].length), title = it && rest.startsWith(`${it.title}): `) ? it.title : rest.split("): ")[0];
  if (rest.length === title.length) return null;
  const html = `<button class="dk-about" data-desk-chk-item="${esc(it?.id ?? m[1])}">${deskKindIcon(it)}<b>${esc(it ? deskNum(it) : `#${m[1]}`)}</b> · ${esc(title)}</button>`;
  return { html, text: rest.slice(title.length + 3) };
}
/** A chat the Desk shows: its earlier messages from Cue's log too (its thread keeps only the last few, all a restart
 *  starts with), loaded as it first shows, and Earlier messages above them, as in a session's chat (chat/thread.js). */
function deskChatRows(s, page) {
  const sid = s.session_id;
  if (!older.has(sid)) queueMicrotask(() => loadOlder(sid));
  const any = (s.thread || []).length || older.get(sid)?.msgs.length;
  return any ? chatHtml(null, s, s.harness || "claude", true, { lines: deskLineHtml, you: deskCueNote, youHead: deskAboutHead, noActs: true, ...page }) : "";
}
/** For ↑ ↓ in a Desk's box (chat/compose.js recallSent): the session it goes to ({ s, said }: your words as its chat
 *  shows them), the lead's for the lead's box; null for a box of no session's. */
function deskRecall(key) {
  const to = deskBoxes.get(key);
  if (!to) return null;
  if (to.sid) { const s = sessionOf(to.sid); return s ? { s } : null; }
  const d = deskOf(to.project), s = deskLead(d) && !deskLeadEnded(d) ? deskLeadSession(d) : null;
  return s ? { s } : null;
}
/** The session whose chat the Desk shows, when it's `sid`: its lead's (a parked one's kept messages too), or the one
 *  opened from the sessions on the left. Null otherwise. */
function deskShownChat(sid) {
  if (!deskSelection()) return null;
  if (deskOpen?.kind === "chat") return deskOpen.project === deskSelection() && deskOpen.sid === sid ? sessionOf(sid) : null;
  if (deskOpen || deskTab !== "lead" || deskPick === deskSelection()) return null;
  const d = deskOf(deskSelection());
  return deskLead(d)?.sid === sid && !deskLeadEnded(d) ? deskLeadSession(d) : null;
}
/** Read on a reply in a Desk chat (chat/reader.js): the reader in the chat's place, its bar, the reply set larger at
 *  the size picked, and its foot (Listen's player; Back to chat while `ask` waits). Null when no reply of `s`'s is
 *  open in it. Not a .dk-chat: the Desk's chat-end scroll (deskSettle) leaves it alone. */
function deskReader(s, ask = null) {
  const sid = s?.session_id, rv = sid && readerViewed(sid, ask, s);
  if (!rv) return null;
  readerView.drawn = { it: ask, s };   // what ‹ › and Copy step through (chat/reader.js readerOf)
  const p = readerPane(sid, ask, s, s.harness || "claude", rv);
  return `${p.bar}<div class="dk-rd" data-scroll-key="rd:${esc(rv.all[rv.at].key)}">${p.body}</div>${p.foot}`;
}
/** The lead's chat: its session's messages, drawn as any session's chat is (chat/render.js), your words without the
 *  Desk's summary; then what you sent that it doesn't show yet (given up a minute after the send returned). */
function deskChat(d) {
  const s = deskLeadSession(d);
  const thread = s?.thread || [];
  const pending = (deskSent.get(d.project) || []).filter((p) => !(p.sent && now() - p.sent > 60000)
    && !thread.some((e) => e.role === "you" && e.at_ms >= p.at - 60000 && e.text.includes(p.text)));
  deskSent.set(d.project, pending);
  // The lead's own Desk lines drawn as lines, not raw text.
  const rows = s ? deskChatRows(s, { extra: deskCheckRows(d), noSteps: true }) : "";
  const mine = pending.map((p) => { const hd = deskAboutHead(p); return `<div class="cv-you sending">${p.text ? `<div class="cv-you-text">${hd?.html || ""}${esc(hd ? hd.text : p.text).replace(/\n/g, "<br>")}</div>` : ""}${p.images?.length ? `<div class="thumbs">${p.images.map((im) => `<span class="thumb"><img src="${esc(im.data)}" alt=""/></span>`).join("")}</div>` : ""}<div class="cv-meta">Sending</div></div>`; }).join("");
  // Where the lead is now (working on what, its latest step, stuck, compacting), as under any session's chat.
  const where = s && sessionOf(s.session_id) ? whereNow(null, s) : "";
  const empty = !rows && !mine && !where ? `<div class="dk-empty">Tell ${esc(deskLeadName(d))} about a bug, a feature or a task. It adds them here, hands each to a session, and says why.</div>` : "";
  return `<div class="dk-chat" data-desk-chat="${esc(d.project)}">${empty}${rows}${mine}${where}</div>`;
}
/** A session's chat, opened from the sessions on the left: its name, Open its session, its messages and where it is
 *  now, as under any session's chat. */
/** What every chat on the Desk is (chat type mark, pick O): one solid label in the theme color, the kind's icon (the
 *  same icon the item list and About bubbles use) and its word, at the left of the header before the chat's name.
 *  `kind`: "desk" (the Desk's dispatcher), "session", "crew", "lead", "task", "bug" or "feature"; "" for none. `name`: the
 *  name's html. The same on every chat's head: the Dispatcher's, a session's, a holder's and a crew member's pane (#170).
 *  The left list keeps its own corner mark (Type marks pick 8, deskSessRow), not this one. */
const DESK_MARKS = { desk: "Dispatcher", session: "Session", crew: "Crew", lead: "Lead", task: "Task", bug: "Bug", feature: "Feature" };
/** Desk screens, one pattern (#200, pick B): every page in the middle card (an item, a session's chat, a crew member,
 *  a lead and its crew, the Dispatcher) is one header (back and the page's buttons, then its kind's icon, "Bug #197 · "
 *  quiet and its name, then one quiet line), the page's own column at the left (its chat or thread, the box under it)
 *  and a panel at the right: NOW (its state said once, a sentence, its buttons), then what else it is (DONE WHEN, WHO…).
 *  Hide panel folds the panel into one line under the header (its NOW: dot, word, counts, buttons), remembered by kind
 *  of page; a narrow card folds it on its own (desk.css). */
const DESK_PANEL_ICON = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M15 4v16"/></svg>`;
/** Kinds of page whose panel you hid ("item", "session", "crew", "lead", "dispatcher"), kept across restarts. */
const deskPanelHid = (() => { try { return new Set(JSON.parse(localStorage.getItem("cue.deskPanelHidden") || "[]")); } catch { return new Set(); } })();
function deskPanelToggle(page) {
  if (!deskPanelHid.delete(page)) deskPanelHid.add(page);
  try { localStorage.setItem("cue.deskPanelHidden", JSON.stringify([...deskPanelHid])); } catch { /* kept for this window only */ }
  renderMain();
}
/** A panel section: its small head, then `html` ("" leaves it out). */
const deskPsec = (head, html, cls = "") => (html ? `<section class="dk-psec${cls ? ` ${cls}` : ""}"><h4 class="dk-psh">${head}</h4>${html}</section>` : "");
/** The panel's NOW: the state's dot (`dot`: "ask", "work", "stop", "done") and word, once; one plain sentence; its
 *  buttons. `fold`: the counts said after the word only when the panel is folded into a line. */
const deskPnow = ({ dot, word, say = "", acts = "", fold = "" }) => `<section class="dk-psec dk-pnow"><h4 class="dk-psh">Now</h4><div class="dk-pnr"><i class="dk-pdot ${dot}" aria-hidden="true"></i><b class="dk-pword">${esc(word)}</b>${fold ? `<span class="dk-pfc"> · ${fold}</span>` : ""}</div>${say ? `<div class="dk-psay">${say}</div>` : ""}${acts ? `<div class="dk-pacts">${acts}</div>` : ""}</section>`;
/** A WHO line: a quiet label, then its value. */
const deskPwho = (label, html) => (html ? `<div class="dk-pw"><span class="dk-pwl">${label}</span><span class="dk-pwv">${html}</span></div>` : "");
/** Item `i`'s number as a link to its page ("#191"). */
const deskItemLink = (i) => `<button class="dk-sch-t" data-desk-chk-item="${esc(i.id)}">#${esc(i.id)}</button>`;
/** Session `sid`'s name, opening its chat when Cue has it; else just the name, bold. */
const deskChatLink = (sid, name, d) => (sessionOf(sid) ? `<button class="dk-pwa" ${deskChatAttrs(sid, d)}>${esc(name)}</button>` : `<b>${esc(name)}</b>`);
/** One page: `page` the kind of page (what Hide panel remembers), `kind` its icon and mark (DESK_MARKS), `mark` the
 *  quiet words before its name ("Bug #197 · "), `title` its name's html, `meta` its quiet line, `now` and `panel` the
 *  panel's sections, `main` its own column and `foot` the box under it. */
function deskPage({ page, kind, back, acts = "", mark, title, meta = "", now, panel = "", main, foot = "" }) {
  const hid = deskPanelHid.has(page);
  const tog = `<button class="btn small dk-ptog" data-desk-panel="${page}" aria-pressed="${!hid}">${DESK_PANEL_ICON}<span>${hid ? "Show panel" : "Hide panel"}</span></button>`;
  return `<div class="dk-pg${hid ? " hid" : ""}" data-desk-page="${page}"><div class="dk-ph"><div class="dk-ph1">${back}<span class="grow"></span>${acts}${tog}</div>`
    + `<h1 class="dk-ph2"><span class="dk-hk"${["desk", "session", "crew", "lead"].includes(kind) ? ` data-desk-mark="${kind}"` : ""}>${deskKindIcon(kind)}${esc(mark.replace(/ · $/, ""))}</span><span class="dk-hsep"> · </span><span class="dk-tname">${title}</span></h1>${meta ? `<div class="dk-ph3">${meta}</div>` : ""}</div>`
    + `<aside class="dk-panel">${now}${panel}</aside><div class="dk-pgm">${main}${foot}</div></div>`;
}
/** A session's dot and word for the panel, from deskSessState. */
const deskPstate = (s) => { const [dot, word] = s ? deskSessState(s) : ["idle", "idle"]; return [dot === "ask" ? "ask" : dot === "idle" ? "stop" : "work", word[0].toUpperCase() + word.slice(1)]; };
/** What session `sid` is on Desk `d`, as the left list's corner mark says it: "desk" (it runs the Desk: its
 *  Dispatcher, one name wherever it shows), "lead" (the Desk gave it a lead's work or it has a crew), else "session". */
const deskSessKind = (sid, d) => (sid === deskLead(d)?.sid ? "desk" : deskIsLead(sid, d.items.filter((i) => deskGroupKey(i.holder) === sid)) ? "lead" : "session");
/** The way back from a session's chat: to what opened it (an item's page, a holder's), else to the lead's chat. */
function deskChatBack(from, d) {
  const i = from?.kind === "item" && deskItem(d.project, from.id);
  const label = i ? deskNum(i) : from?.kind === "holder" ? deskHolderName(from.sid, d.items.find((x) => deskGroupKey(x.holder) === from.sid && x.state !== "done" && x.holder?.kind !== "crew")?.holder?.name) : deskLeadName(d) || "Back";
  return `<button class="dk-back" data-desk-chat-back>‹ ${esc(label)}</button>`;
}
function deskSessChat(sid, from, d, foot = "") {
  const s = sessionOf(sid);
  if (!s) return null;
  const rows = deskChatRows(s);
  const where = whereNow(null, s);
  const empty = !rows && !where ? `<div class="dk-empty">Nothing said yet.</div>` : "";
  const open = `<button class="btn small" data-desk-session="${esc(sid)}">Open its session</button>`;
  // One page (#200): back, Open its session; "Session · " (or Lead, Dispatcher: what the session is, never an item's
  // kind) and its name; quiet, the item it's on ("on #191 · its title", the number opening it). Its chat at the left,
  // its state, step and Stop in the panel, with every item it holds.
  const its = deskSessItems(d, sid), kind = deskSessKind(sid, d), on = its[0];
  const meta = on ? `on ${deskItemLink(on)} · ${esc(on.title)}${its.length > 1 ? ` · +${its.length - 1} more` : ""}` : s.since_ms ? `nothing on the Desk · since ${esc(clockAt(s.since_ms))}` : "nothing on the Desk";
  const [dot, word] = deskPstate(s);
  const say = dot === "ask" ? "It asks you something: answer under its chat." : dot === "work" ? (s.doing ? esc(s.doing) : "At work.") : "At its prompt: tell it what's next under its chat.";
  const stop = dot === "work" ? `<button class="btn small" data-act="interrupt" data-sid="${esc(sid)}">Stop</button>` : "";
  return deskPage({ page: kind === "desk" ? "dispatcher" : kind === "lead" ? "lead" : "session", kind, back: deskChatBack(from, d), acts: open,
    mark: `${DESK_MARKS[kind]} · `, title: deskSessName(sid, deskHolderName(sid), "dk-shn"), meta,
    now: deskPnow({ dot, word, say, acts: stop, fold: on ? `on #${esc(on.id)}` : "" }),
    panel: deskPsec("Who", deskPwho("On", its.length ? its.map(deskItemLink).join(", ") : "nothing yet")),
    main: `<div class="dk-chat" data-desk-chat="${esc(sid)}">${empty}${rows}${where}</div>`, foot });
}
/** The session whose steps the Desk follows live (chat/thread.js followSteps): its lead's, while its chat shows, or
 *  the session's whose chat is open. */
function deskFollowSid() {
  if (deskOpen?.kind === "chat" && deskOpen.project === deskSelection()) return sessionOf(deskOpen.sid) ? deskOpen.sid : null;
  if (!deskSelection() || deskOpen || deskTab !== "lead" || deskPick === deskSelection()) return null;
  const d = deskOf(deskSelection()), l = deskLead(d);
  return l && !deskLeadEnded(d) && sessionOf(l.sid) ? l.sid : null;
}
/** The Desk's judgment calls (DECIDED lines), the only decisions worth your look: routes, gives, sessions started,
 *  contexts kept and Cue's hand-off asks are steps, kept on their item (deskItemPane). */
const deskCalls = (d) => d.decisions.filter((c) => c.call);
function deskDecisions(d) {
  const calls = deskCalls(d);
  if (!calls.length) return `<div class="dk-empty">No judgment calls yet. Routes and hand-offs are on each item.</div>`;
  return `<div class="dk-pane" data-scroll-key="decisions:${esc(d.project)}"><div class="dk-sec">${deskLog(calls)}</div></div>`;
}
/** "Done when" as its points, one per line: its own lines (a "-", "*", "•" or "1." in front dropped), else its
 *  sentences and "; " parts. "e.g." / "i.e." / "etc." don't end one; a point's last "." goes. */
function deskDoneWhenPoints(t) {
  // Numbered in a line ("(1) … (2) …"): a point each.
  const lines = String(t || "").split(/\n+|\s*(?:^|\s)\(\d+\)\s+/).map((l) => l.replace(/^\s*(?:[-*•]|\d+[.)])\s+/, "").trim()).filter(Boolean);
  const out = [];
  for (const l of lines) {
    let cur = "";
    for (const part of l.split(/(?<=[.!?;]["”')]?)\s+/)) {
      cur = cur ? `${cur} ${part}` : part;
      if (!/\b(?:e\.g|i\.e|etc|vs)\.$|\b[A-Z]\.$/i.test(cur) || /[;!?]$/.test(cur)) { out.push(cur); cur = ""; }
    }
    if (cur) out.push(cur);
  }
  return out.map((p) => p.replace(/(?<!\.)[.;]$/, "").trim()).filter(Boolean);
}
/** "Done when" as a short checklist of what you'll see: a quiet ring, then the point, a line each. */
const deskDoneWhenHtml = (t, base = null) => {
  const pts = deskDoneWhenPoints(t);
  return pts.length ? `<ul class="dk-dwl">${pts.map((p) => `<li><i aria-hidden="true"></i><span>${linkify(esc(p), base)}</span></li>`).join("")}</ul>` : "";
};
/** Its designs' canvas (the last DESIGNS: link a session gave, hub/desk.rs designs_said): "Open the mocks ↗", a
 *  plain link in the accent colour (Lead chats pick 1 + D: no outlined button), in its page's header (#178). */
const deskDesignsLink = (i) => /^https?:\/\//.test(i.designs || "") ? `<div class="dk-dsg">${deskDesignsA(i.designs)}</div>` : "";
const deskDesignsA = (url) => `<a class="dk-dlink" href="#" data-href="${esc(url)}">Open the mocks ↗</a>`;
/** An item with mocks waiting on you to pick one: the line of its holder's newest words that asks you to, plain, for
 *  under its page's header ("Pick one for working items: D, E or F"). Its question first while it asks you, then its
 *  replies, newest first. "" when none asks for a pick. */
function deskPickLine(i) {
  if (!/^https?:\/\//.test(i.designs || "") || (i.state !== "needs_you" && i.state !== "to_check")) return "";
  const texts = [...(i.state === "needs_you" ? [deskQuestion(i)] : []), ...[...(i.said || [])].reverse().map((r) => r.text || "")];
  for (const t of texts) {
    const lines = plain(t).split(/\n+|(?<=[.?!])\s+/).map((x) => x.trim()).filter((x) => /\b(pick|choose)\b/i.test(x));
    const line = lines.find((x) => /^(pick|choose|which)\b/i.test(x)) || lines[0];
    if (line) return line.replace(/^(not done yet|still to do)\s*:\s*/i, "").replace(/^./, (c) => c.toUpperCase());
  }
  return "";
}
const deskSec = (title, html) => (html ? `<div class="dk-sec"><h3>${title}</h3>${html}</div>` : "");
/** Item `i`'s latest DONE (`d.checks`), quiet, under What to check on its page: "Done by web-lead · 12m ago · 2 files"
 *  (a click on who opens its chat, as its row does), then whether the installed Cue has it ("In the installed Cue ·
 *  installed Mon 3:04 PM" / "Not installed yet: install Cue to try it"), or for a design item, its mocks. */
function deskCheckInfo(i, d) {
  const c = i.state === "to_check" || i.state === "done" ? (d.checks || []).find((x) => x.id === i.id && x.last) : null;
  if (!c) return "";
  const by = c.by || "its session", ses = (state.sessions || []).find((s) => s.name === c.by && s.project === deskWorkspace(d));
  const who = ses ? `<button class="dk-chk-a" ${deskChatAttrs(ses.session_id, d)}>${esc(by)}</button>` : `<b>${esc(by)}</b>`;
  const n = (c.files || []).length, files = n ? ` · ${n === 1 ? "1 file" : `${n} files`}` : "";
  // In it when Cue was installed after the work last changed (`edited`), not after the DONE: you install, then say DONE.
  const mocks = /^https?:\/\//.test(c.designs || "") ? `<a class="dk-dlink" href="#" data-href="${esc(c.designs)}">Open the mocks ↗</a>` : "";
  const inst = mocks || !c.cue ? mocks : deskChkUninstalled(c) && !deskFinCommit(i) ? "Not installed yet: install Cue to try it" : `In the installed Cue · installed ${esc(deskChkWhen(state.desk?.installed_ms || 0))}`;
  return `<div class="dk-chk-info"><div class="dk-chk-w">Done by ${who} · ${ago(c.at)} ago${files}</div>${inst ? `<div class="dk-chk-in">${inst}</div>` : ""}</div>`;
}
const deskChkWhen = (ms) => new Date(ms).toLocaleString([], { weekday: "short", hour: "numeric", minute: "2-digit" });
const deskBack = (label, to = "") => `<button class="dk-back" data-desk-back="${esc(to)}">‹ ${esc(label)}</button>`;

/** What you answered an item's holder on its page, by item: { q, a, ask, at }. Shown there until its holder asks again
 *  goes back to work, or ten minutes pass (board.js respond calls deskAnswered). */
const deskAnswers = new Map();
/** Card `it` answered with `decision`: kept for the Desk item whose holder asked it, so its page says what you picked. */
function deskAnswered(it, decision) {
  const item = (state.desk?.projects || []).flatMap((p) => p.items || []).find((x) => x.ask?.item_id === it.id);
  if (!item) return;
  const picked = decision.answers ? Object.values(decision.answers).join("; ") : decision.message || "";
  const a = decision.behavior === "deny" ? (picked ? `Declined: ${picked}` : "Declined") : picked || (decision.behavior === "allow_always" ? "Always allowed" : "Allowed");
  deskAnswers.set(item.id, { q: item.ask.text, a, ask: it.id, at: now() });
}
/** What item `i`'s holder asks you: its ask's question, or, for a turn that ended on it, the last paragraph of its last
 *  words (all of it, not a line cut short; the hub's latest when its session isn't here). */
function deskQuestion(i) {
  if (i.ask) return i.ask.text || "";
  // Cue's own, while it finishes it ("The build failed"): not its holder's words.
  if (i.finish?.question) return i.finish.question;
  const last = [...(sessionOf(i.holder?.sid)?.thread || [])].reverse().find((e) => e.role === "agent")?.text || "";
  // Its calls made on its own (DECIDED) and its Desk lines (DONE, ITEM…) aren't what it asks you: they're dropped.
  const asked = last.split("\n").filter((l) => !DECIDED_LINE.test(l) && !DESK_SAY.test(l.trim())).join("\n").trim();
  return asked.split(/\n\s*\n/).pop().trim() || (last.trim() ? "" : i.latest || "");
}
/** What you last answered on item `i`'s page, while it's fresh and its holder isn't asking something new; else null. */
function deskAnswerOf(i) {
  const said = deskAnswers.get(i.id);
  if (!said || now() - said.at > 600000 || (i.ask && i.ask.item_id !== said.ask)) return null;
  if (i.state === "needs_you" && deskQuestion(i) !== said.q) return null;
  // Once its holder is at work again (your answer went through, or you sent it more), the old ask is gone.
  const s = i.holder?.kind === "crew" ? null : sessionOf(i.holder?.sid);
  if (i.state !== "needs_you" && s && ["working", "deciding", "agent"].includes(s.state)) return null;
  return said;
}
/** What holds an item up: the item it waits for, then this one; or what its holder asks you (only the question: it's
 *  answered at the page's foot, deskItemFoot), then what you answered. */
function deskChain(i) {
  const said = deskAnswerOf(i);
  if (said) return `<div class="dk-chain"><div class="dk-ln dk-q"><div><div class="dk-lt dk-md">${md(said.q)}</div><div class="dk-ls dk-picked"><span class="sent-check">✓</span> You answered: ${esc(said.a)}</div></div></div></div>`;
  if (i.state === "needs_you") {
    const q = deskQuestion(i);
    return q ? `<div class="dk-chain"><div class="dk-ln dk-q"><div class="dk-lt dk-md">${md(q)}</div></div></div>` : "";
  }
  if (i.state !== "queued" || !i.after) return "";
  const first = deskItem(i.project, i.after);
  if (!first) return "";
  const who = first.holder?.kind === "crew" ? `${first.holder.name} (${deskHolderName(first.holder.lead_sid)}'s crew)` : first.crew && first.holder ? `${first.holder.name} → ${first.crew}` : first.holder?.name || "";
  const files = deskFilesShort(first.files);
  const sub = [who, first.latest, files ? `has ${files} open` : ""].filter(Boolean).join(" · ");
  return `<div class="dk-chain"><div class="dk-ln"><i></i><div><div class="dk-lt">${esc(first.title)}</div><div class="dk-ls">${esc(sub)}</div></div></div>
    <div class="dk-ln"><i class="wait"></i><div><div class="dk-lt">This one</div><div class="dk-ls">starts when ${esc(deskShort(first.title))} is done${i.after_why ? `: ${esc(i.after_why)}` : ""}</div></div></div></div>`;
}
/** An item's files, short: a few names, the rest as "+N more" ("" for none). */
function deskFilesShort(files, sep = ", ") {
  const names = (files || []).map((f) => f.split("/").pop());
  const more = names.length > 3 ? ` +${names.length - 3} more` : "";
  return names.slice(0, 3).join(sep) + more;
}
/** Holder `h` not at work (as hub/desk.rs at_work): its session at its prompt, or for a crew member, that teammate. */
function deskHolderIdle(h) {
  if (!h) return false;
  // A teammate gone with the crash counts as not at work: its lead is sent it, for that teammate.
  if (h.kind === "crew") {
    const lead = sessionOf(h.lead_sid);
    return !!lead && !(lead.team || []).some((m) => (m.agent_id === h.sid || m.call === h.sid) && m.state === "working");
  }
  const s = sessionOf(h.sid);
  return !!s && !["working", "deciding", "agent"].includes(s.state);
}
/** An item's buttons, by where it is. */
function deskItemActs(i) {
  const b = (act, label, look = "") => `<button class="btn small ${look}" data-desk-act="${act}" data-id="${esc(i.id)}">${esc(label)}</button>`;
  const first = i.after && deskItem(i.project, i.after);
  if (i.state === "to_check" && deskBackFor === i.id) return deskBackHtml(i.id);
  // Waiting for install: nothing to try yet, so no Looks right (Send back stays: what it says can be wrong already).
  const acts = deskInstallWait(i) ? b("send_back", "Send back") : i.state === "to_check" ? (deskLooksSoon.has(i.id) ? "" : b("send_back", "Send back"))
    : (i.state === "new" || i.state === "queued") && !i.holder && i.start_failed ? b("start_now", "Retry")
    : i.state === "queued" ? b("start_now", "Start it now in a new session") + (first ? b("put_before", `Put it before ${deskShort(first.title)}`) : "")
    : i.state === "new" ? b("start_now", "Start it now in a new session")
    // Its holder's turn cut off (a crash, a quit) leaves it "working" with nobody on it: sent to the holder again.
    : i.state === "working" && deskHolderIdle(i.holder) ? b("start_now", "Restart")
    // Finishing stopped on a question (a build that failed, a word, a conflict): try it again once you've seen to it.
    : i.finish?.question ? b("retry_finish", "Try again")
    : deskFinishing(i) ? deskInstallNow(i, "btn small")
    : i.state === "done" ? `<div class="dk-chk-after">${deskCommitLine(i)}</div>` : "";
  const give = !i.holder && i.state !== "done" ? deskGive(i, "pane") : "";
  return acts || give ? `<div class="dk-acts">${give}${acts}</div>` : "";
}
/** Looks right on an item's page: in its title area, always in reach however far its page is scrolled (#163); its
 *  few seconds to take it back there too. None while it waits for an install or you're writing why to send it back. */
function deskLooksTop(i) {
  if (i.state !== "to_check" || deskInstallWait(i) || deskBackFor === i.id) return "";
  // Committed when its session said DONE: Looks right only closes it (its state line says which commit).
  return deskLooksSoon.has(i.id) ? deskLooksSoonHtml(i.id) : `<button class="btn small primary" data-desk-act="looks_right" data-id="${esc(i.id)}">Looks right</button>`;
}
const DESK_LOG_FOLD = 6;
/** Which items' build logs are open in full (Show all), by id. */
const deskFinLogOpen = new Set();
/** A failed build's or start's log tail on its item's page: mono, folded to a few lines, Show all opens it (and it
 *  scrolls). Its last lines show folded. */
function deskFinLog(i) {
  const log = i.state === "needs_you" ? (i.finish?.log || "").trimEnd() : "";
  if (!log) return "";
  // Folded: its last lines, where a build says what went wrong.
  const lines = log.split("\n"), open = deskFinLogOpen.has(i.id), long = lines.length > DESK_LOG_FOLD;
  return `<pre class="dk-flog${open ? " open" : ""}">${esc(open ? log : lines.slice(-DESK_LOG_FOLD).join("\n"))}</pre>${long ? `<button class="dk-flog-more" data-desk-act="log_all" data-id="${esc(i.id)}">${open ? "Show less" : "Show all"}</button>` : ""}`;
}
/** An item's decisions as sentences: what was decided (its age at the right), why under it, then who and Undo. The
 *  same decision said again in a row is one entry marked "×N"; a click shows a long one in full. */
function deskDecList(list) {
  const rows = [];
  for (const c of list) {
    const last = rows[rows.length - 1];
    if (last && last.c.text === c.text && (last.c.why || "") === (c.why || "")) { last.n++; if (deskMs(c.at) >= deskMs(last.c.at)) last.c = c; }
    else rows.push({ c, n: 1 });
  }
  return `<div class="dk-decs">${rows.map(({ c, n }) => `<div class="dk-lr dk-dec dk-dl${deskLogOpen.has(c.id) ? " open" : ""}" data-desk-log-row="${esc(c.id)}">`
    + `<div class="dk-dl1"><span class="dk-d">${esc(c.text)}${n > 1 ? ` <span class="dk-x">×${n}</span>` : ""}</span><span class="dk-age">${ago(deskMs(c.at))} ago</span></div>`
    + (c.why ? `<div class="dk-w">${esc(c.why)}</div>` : "")
    + `<div class="dk-dl3">${c.by ? `<span class="dk-lby">by <b>${esc(c.by)}</b></span>` : ""}<span class="grow"></span><span class="dk-lu">${deskUndoLink(c)}</span></div></div>`).join("")}</div>`;
}
/** What opens session `sid`'s chat here on the Desk, as its row in the sessions on the left does: the lead's chat
 *  for the Desk's lead, else that session's chat in the middle card. Never the board. */
const deskChatAttrs = (sid, d) => (sid === deskLead(d)?.sid ? `data-desk-lead-chat="${esc(sid)}"` : `data-desk-chat-of="${esc(sid)}"`);
/** Each item page's open tab ("thread" | "decisions" | "steps"), by item: Thread unless you picked another. */
const deskItemTabs = new Map();
/** What you wrote on an item's page that hasn't come back as one of its Steps yet, by item: [{ text, why, at }], so
 *  Thread has it the moment it's sent (deskSendBox, deskSendBack). Dropped once the step is in, or the send failed. */
const deskPending = new Map();
const deskPend = (id, text, why, pics = 0) => { const p = { text, why, pics, at: now() }; deskPending.set(id, [...(deskPending.get(id) || []), p]); return p; };
const deskUnpend = (id, p) => deskPending.set(id, (deskPending.get(id) || []).filter((x) => x !== p));
/** A step that hands the item to someone (hub/desk.rs routed_text, give): on its Thread as a Hand-off. */
const DESK_HANDOFF = /^(Routed|Gave|Started|Sent|Queued|Reopened) #/;
/** Item `i`'s steps and calls, oldest first: today's (newest first in `decisions`) after the older ones. */
const deskItemLog = (i, d) => [...(d.older || []), ...[...(d.decisions || [])].reverse()].filter((c) => c.item_id === i.id);
/** Item `i`'s Thread, oldest first: the words it was added with, then what you wrote about it (Sent back in red), the
 *  hand-offs, and what its holder said as each turn ended (hub/desk.rs replied); last, its holder at work now. */
function deskThread(i, d) {
  const mine = deskItemLog(i, d).filter((c) => !c.call);
  const you = (i.from || "you") === "you";
  // Its Done when isn't in this first message: it's under the thread, by where it stands (deskItemPane).
  const pics = thumbs(i.images);
  const rows = [{ at: deskMs(i.created), by: you ? "You" : deskFrom(i, d), you, html: `<div class="dk-md">${md(i.words || i.title)}</div>${pics ? `<div class="dk-tpics">${pics}</div>` : ""}` }];
  const backPics = deskBackPics(i, d, mine);
  for (const c of mine) {
    if (c.by === "You" && /^(to |sent back)/.test(c.why || "")) {
      const back = c.why.startsWith("sent back");
      rows.push({ at: deskMs(c.at), by: "You", you: true, back, tag: back ? "Sent back" : "", html: deskBackWords(c.text, backPics.get(c) || 0) });
    }
    else if (DESK_HANDOFF.test(c.text)) rows.push({ at: deskMs(c.at), by: c.by, tag: "Hand-off", html: `<div class="dk-md">${esc(c.text)}${c.why ? `: ${esc(c.why)}` : ""}</div>` });
  }
  for (const r of i.said || []) rows.push({ at: r.at, by: r.by, tag: r.done ? "Done" : "Reply", on: r.done, text: r.text, html: `<div class="dk-md">${md(r.text)}</div>` });
  // Sent from here, not back as a step yet.
  const steps = new Set(mine.filter((c) => c.by === "You").map((c) => c.text.trim()));
  for (const p of deskPending.get(i.id) || []) if (!steps.has(p.text.trim())) rows.push({ at: p.at, by: "You", you: true, back: p.why === "back", tag: p.why === "back" ? "Sent back" : "", html: deskBackWords(p.text, p.pics) });
  // At work on this one: its holder can hold several, and its work and latest words are only this one's while it's on
  // it (#176).
  const live = i.state === "working" && i.holder && i.now_on && !deskHolderIdle(i.holder);
  // Its latest words when no reply holds them yet (items from before replies were kept): after everything but what
  // you wrote since, so a message you send is still last (#160).
  const lat = (i.latest || "").trim(), lat1 = firstLine(lat);
  if (!live && lat && !(i.said || []).some((r) => r.text.includes(lat1))) {
    const fin = i.state === "to_check" || i.state === "done";
    const theirs = rows.filter((r) => !r.you).reduce((m, r) => Math.max(m, r.at), 0);
    rows.push({ at: Math.max(deskMs(i.updated), theirs), by: i.holder?.name || i.by || "It", tag: fin ? "Done" : "Reply", on: fin, text: lat, html: `<div class="dk-md">${md(lat)}</div>` });
  }
  rows.sort((a, b) => a.at - b.at);
  // Its holder at work: last, under what you wrote, as the answer to it comes (#165).
  if (live) rows.push({ at: 0, by: i.holder.name, live: true, html: `<div class="dk-md dk-quiet">${lat ? md(lat1) : "Working…"}</div><div class="dk-tbar"></div>` });
  return rows;
}
/** What you wrote on a Thread, in your words; for a send-back, how many pictures went with it, quiet under them. */
/** Item `i`'s send-backs (steps in `log`) and how many pictures went with each. The hub counts them on the DONE a
 *  send-back answers (hub/desk.rs send_back: `back_pics` on the last DONE then), so by time, not words: a DONE's
 *  count goes to the last send-back after it and before the next DONE (a later one on the same DONE overwrote it). */
function deskBackPics(i, d, log) {
  const out = new Map(), checks = (d.checks || []).filter((k) => k.id === i.id).sort((a, b) => a.at - b.at);
  const backs = log.filter((c) => c.by === "You" && (c.why || "").startsWith("sent back"));
  checks.forEach((k, n) => {
    const next = checks[n + 1]?.at ?? Infinity;
    const b = k.back_pics ? backs.filter((c) => deskMs(c.at) >= deskMs(k.at) && deskMs(c.at) < deskMs(next)).at(-1) : null;
    if (b) out.set(b, k.back_pics);
  });
  return out;
}
const deskBackWords = (text, pics = 0) => `<div class="dk-md">${md(text)}</div>${pics ? `<div class="dk-tpn">${pics === 1 ? "1 picture" : `${pics} pictures`}</div>` : ""}`;
/** An item page's suggested reply (`key` "<project>|<id>"): the words its newest message asks you to reply with
 *  (Reply "merge"), while nothing's at work on it and you haven't written since (#186). */
function deskItemSuggestion(key) {
  const [project, id] = [key.slice(0, key.lastIndexOf("|")), key.slice(key.lastIndexOf("|") + 1)];
  const d = deskOf(project), i = d && deskItem(project, id);
  const last = i && deskThread(i, d).at(-1);
  return last && !last.you && !last.live ? replyAsked(last.text) : "";
}
/** A Thread as a time line: each message's age, a dot (yours and the live one in the accent colour, a send-back's
 *  red), who and what kind, then the words. */
const deskThreadHtml = (rows) => `<div class="dk-thread">${rows.map((r) => `<div class="dk-tm${r.live ? " live" : ""}"><span class="dk-age">${r.live ? "now" : esc(ago(r.at))}</span><span class="dk-tdot${r.back ? " back" : r.you || r.live ? " on" : ""}"><i></i></span><div class="dk-tb"><div class="dk-tw"><b>${esc(r.by)}</b>${r.tag ? `<span class="dk-ttag${r.back ? " back" : r.on ? " on" : ""}">${esc(r.tag)}</span>` : ""}</div>${r.html}</div></div>`).join("")}</div>`;
function deskItemPane(i, d, foot = "") {
  const all = deskItemLog(i, d).reverse();
  const decs = all.filter((c) => c.call), steps = all.filter((c) => !c.call);
  // Its Files tab (#149): each file it changed with its counts, a click opening its diff; while it's worked on, what's
  // changed so far (the files it's on, till they've changed).
  const changed = deskFilesChanged(i), checking = i.state === "to_check" || i.state === "done";
  const read = deskDiffs.get(i.id)?.files, nFiles = (checking ? read?.length : read?.length || null) ?? (i.commit_wait?.files || i.files || []).length;
  const files = changed ? `<div class="dk-fcs">${changed}</div>` : "";
  const thread = deskThread(i, d), tab = deskItemTabs.get(i.id) || "thread";
  const h = i.holder, sid = h ? (h.kind === "crew" ? h.lead_sid : h.sid) : null;
  const armed = deskDropArm === i.id;
  const tabBtn = (k, label, n) => `<button class="dk-itab${tab === k ? " on" : ""}" role="tab" aria-selected="${tab === k}" data-desk-itab="${k}" data-id="${esc(i.id)}">${label}<span class="n">${n}</span></button>`;
  // Under the thread, by the newest message and the box: writing why to send it back, what held it up or what it
  // asks, a build log, what it made. Its state, Done when, buttons and who has it are in the panel (#200).
  const backing = i.state === "to_check" && deskBackFor === i.id;
  const at = `${backing ? deskBackHtml(i.id) : ""}${deskSec("Couldn't give it out", i.route_failed ? `<p>${esc(i.route_failed)}</p>` : "")}${deskSec("Couldn't start", !i.holder && i.start_failed ? `<p>Cue couldn't start a new session for it: ${esc(i.start_failed)}</p>` : "")}
    ${deskSec(i.state === "needs_you" ? "It asks you" : deskAnswerOf(i) ? "It asked you" : "What's holding it up", deskChain(i))}${deskSec("Build log", deskFinLog(i))}${deskSec("What it made", i.state === "to_check" || i.state === "done" ? deskMadeLinks(i) : "")}
    ${i.split_from ? `<p class="dk-split">Split from <a href="#" data-desk-item="${esc(i.split_from)}">#${esc(i.split_from)}</a></p>` : ""}`;
  const body = tab === "decisions" ? (decs.length ? deskDecList(decs) : `<p class="dk-note">No decisions on it yet.</p>`)
    : tab === "steps" ? (steps.length ? deskLog(steps) : `<p class="dk-note">No steps yet.</p>`)
    : tab === "files" ? (files ? deskSec(checking ? "Files changed" : deskDiffs.get(i.id)?.files?.length ? "Changed so far" : "Files it's on", files) : `<p class="dk-note">No files on it yet.</p>`)
    : `${deskThreadHtml(thread)}<div class="dk-inow">${at}</div>`;
  const main = `<div class="dk-pane dk-ipane" data-scroll-key="item:${esc(i.id)}"><div class="dk-ihead"><div class="dk-itabs" role="tablist">${tabBtn("thread", "Thread", thread.length)}${tabBtn("decisions", "Decisions", decs.length)}${tabBtn("steps", "Steps", steps.length)}${tabBtn("files", "Files", nFiles)}<span class="grow"></span>${i.state !== "done" ? `<button class="dk-drop${armed ? " armed" : ""}" data-desk-act="drop" data-id="${esc(i.id)}">${armed ? "Drop it" : "Drop this item"}</button>` : ""}</div></div>
    <div class="dk-ibody dk-itab-${tab}">${body}</div></div>`;
  // Done when as ticks: all ticked once it's to check or done (its session said DONE), none before; "n of m".
  // SHORTCUT: the ticks come from the item's state, so they're all or none: Cue has no status per point. Upgrade: its
  // holder reports each point met, and each tick follows that.
  const pts = deskDoneWhenPoints(i.done_when), ticked = checking ? pts.length : 0;
  const tk = (y) => `<i class="dk-tk${y ? " y" : ""}" aria-hidden="true">${y ? `<svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>` : ""}</i>`;
  const dw = pts.length ? `<div class="dk-idw"><ul class="dk-ptk">${pts.map((p) => `<li${checking ? ` class="y"` : ""}>${tk(checking)}<span>${linkify(esc(p), null)}</span></li>`).join("")}</ul></div>` : "";
  const done = deskPsec(`Done when${pts.length ? ` · ${ticked} of ${pts.length}` : ""}`, `${dw}${deskCheckInfo(i, d)}`, "dk-pdw");
  const who = h ? (h.kind === "crew" ? `<b>${esc(h.name)}</b>` : deskChatLink(h.sid, h.name, d)) : "";
  const leadOf = h?.kind === "crew" ? deskChatLink(h.lead_sid, deskHolderName(h.lead_sid), d) : "";
  const whoSec = deskPsec("Who", deskPwho("Held by", who ? `${who}${i.crew ? ` → <b>${esc(i.crew)}</b>` : ""}` : "nobody yet") + deskPwho("Lead", leadOf) + deskPwho("Files", nFiles ? `${nFiles} ${checking ? "changed" : "on it"}` : ""));
  return deskPage({ page: "item", kind: i.kind, back: deskBack(deskLeadName(d) || "Back"),
    acts: sid && sessionOf(sid) ? `<span class="dk-ilinks"><button class="btn small" ${deskChatAttrs(sid, d)}>Open ${esc(h.name)}</button></span>` : "",
    mark: `${deskNum(i)} · `, title: esc(i.title), meta: `<span class="dk-state">${deskItemMeta(i, d)}</span>`,
    now: deskPnow({ ...deskItemNow(i), acts: `${deskLooksTop(i)}${backing ? "" : deskItemActs(i)}${deskDesignsLink(i)}`, fold: pts.length ? `${ticked} of ${pts.length} done` : "" }),
    panel: done + whoSec, main, foot });
}
/** An item's quiet line under its title (#200): who has it, its commit, who added it and when; its state is the
 *  panel's, said once there. */
function deskItemMeta(i, d) {
  const who = i.holder ? `<b>${esc(i.holder.name)}</b>${i.crew ? ` → <b>${esc(i.crew)}</b>` : ""}` : "";
  const cm = i.state === "done" ? i.commit : i.state === "to_check" ? deskFinCommit(i) : "";
  return [who ? `${i.state === "queued" ? "next for" : i.state === "done" ? "by" : "with"} ${who}` : "", cm ? `committed <span class="dk-cm">${esc(cm)}</span>` : "",
    `added by ${esc(deskFrom(i, d))}`, `${ago(deskMs(i.created))} ago`].filter(Boolean).join(" · ");
}
/** An item's NOW (#200): its state's dot and word, and one plain sentence: what it waits on you for (#182; a pick on
 *  its mocks, #178) and what that unblocks, else where it is. */
function deskItemNow(i) {
  const pick = deskVerbHtml(i), ub = pick ? deskUnblocksHtml(i).replace("dk-ub", "dk-ub dk-iub") : "";
  const who = esc(i.holder?.name || "Its session"), first = i.after && deskItem(i.project, i.after);
  const idle = i.state === "working" && (i.stopped || deskHolderIdle(i.holder));
  const [dot, word, say] = i.state === "needs_you" ? ["ask", "Waiting on you", "It asks you something: answer under its thread."]
    : deskInstallWait(i) ? ["work", "Waiting for install", "Nothing to try till Cue is installed with it."]
    : i.state === "to_check" ? ["ask", "Ready to check", ""]
    : idle ? ["stop", "Stopped", i.stopped ? "The Desk lead is on it." : `${who} stopped on it: Restart sends it again.`]
    : i.state === "working" ? ["work", "Working", `${who} is on it.${i.now_on && i.latest ? ` ${esc(firstLine(i.latest))}` : ""}`]
    : deskFinishing(i) ? ["work", "Finishing", `<span class="dk-fstep">${esc(deskFinStep(i))}</span>`]
    : i.state === "queued" ? ["stop", "Next", first ? `Starts when ${esc(deskShort(first.title))} is done.` : i.holder ? `Next for ${who}.` : "Next up, not given to a session yet."]
    : i.state === "new" ? ["stop", "Not given out", "Not given to a session yet."]
    : i.state === "done" ? ["done", "Done", ""] : ["stop", i.state, ""];
  return { dot, word, say: pick ? `<p class="dk-pickl">${pick}</p>${ub}` : say };
}

/** A session's latest words in a line. */
const deskLatestOf = (s) => plain(firstLine([...(s?.thread || [])].reverse().find((e) => e.role === "agent")?.text || ""));
/** A crew member's dot: red when it asks you, filled at work, hollow otherwise. */
const deskMateDot = (lead, t) => (mateNeeds(lead, t) ? "ask" : t.state === "working" ? "" : "idle");
/** A lead (its crew, what's next, what it decided, its latest) or a session on its own. */
function deskHolderPane(sid, d, foot = "") {
  const s = sessionOf(sid);
  const mine = d.items.filter((i) => deskGroupKey(i.holder) === sid && i.state !== "done");
  const name = deskHolderName(sid, mine.find((i) => i.holder?.kind !== "crew")?.holder?.name);
  const crew = deskCrew(sid), lead = deskIsLead(sid, mine);
  const waits = mine.filter(deskWaits).length;
  const head = [`${mine.length} item${mine.length === 1 ? "" : "s"}${waits ? `, ${waits} waiting on you` : ""}`, lead ? (crew.length ? `crew of ${crew.length}` : "on its own") : "", s?.since_ms ? `since ${clockAt(s.since_ms)}` : ""].filter(Boolean).join(" · ");
  const crewRows = crew.map((t) => {
    const it = deskCrewItem(d.project, sid, t.agent_id || t.call);
    const files = (t.files || []).map((f) => f.split("/").pop()).join(", ");
    const line = [it ? it.title : t.what, mateNeeds(sid, t) ? "stopped to ask you" : t.step, files].filter(Boolean).join(" · ");
    return `<div class="dk-mb" role="button" tabindex="0" data-desk-crew="${esc(t.agent_id || t.call)}" data-lead="${esc(sid)}"><i class="${deskMateDot(sid, t)}"></i><div class="dk-lt">${esc(t.name)}</div><span class="dk-age">${esc(mateSpan(t))}</span><div class="dk-ls">${esc(line)}</div></div>`;
  }).join("");
  const next = mine.find((i) => i.state === "queued");
  const nextLine = next ? `<p class="dk-note">Next for this ${lead ? "lead" : "session"}: ${esc(next.title)}${next.after ? `, after ${esc(deskItem(d.project, next.after)?.title || "")}` : ""}.</p>` : "";
  const decs = d.decisions.filter((c) => c.call && (c.sid === sid || c.by === name));
  const decHtml = decs.length ? deskLog(decs, (c) => (c.item_id && deskItem(d.project, c.item_id) ? esc(deskItem(d.project, c.item_id).title) : "")) : "";
  const latest = deskLatestOf(s);
  // One page (#200): its crew, what's next, its calls and latest at the left; in the panel, what of it waits on you
  // (Open #190), else its state, and its crew and items counted.
  const kind = deskSessKind(sid, d), wait = mine.find(deskWaits);
  const [dot, word] = deskPstate(s);
  const now = wait ? deskPnow({ dot: "ask", word: "Waiting on you", say: `${deskKindIcon(wait)}<b>${esc(deskNum(wait))}</b> ${deskVerbHtml(wait) || esc(wait.title)}`, acts: `<button class="btn small primary" data-desk-chk-item="${esc(wait.id)}">Open #${esc(wait.id)}</button>`, fold: esc(head) })
    : deskPnow({ dot, word, say: latest ? esc(latest) : "", fold: esc(head) });
  const by = (f) => crew.filter(f).length;
  const counts = [[by((t) => mateNeeds(sid, t)), "ask you"], [by((t) => !mateNeeds(sid, t) && t.state === "working"), "working"], [by((t) => !mateNeeds(sid, t) && t.state !== "working" && t.state !== "done"), "stopped"], [by((t) => t.state === "done"), "done"]].filter(([n]) => n).map(([n, w]) => `${n} ${w}`).join(" · ");
  const who = deskPsec("Who", deskPwho("Crew", lead ? counts || "none at work" : "") + deskPwho("Items", `${mine.length} open${waits ? ` · ${waits} ${waits === 1 ? "waits" : "wait"} on you` : ""}`));
  const main = `<div class="dk-pane" data-scroll-key="holder:${esc(sid)}">
    ${lead ? deskSec("Crew", crewRows ? `<div class="dk-crew">${crewRows}</div>` : `<p class="dk-note">No crew at work.</p>`) : ""}${nextLine}
    ${deskSec(`Decided by ${esc(name)} · ${decs.length}`, decHtml)}
    ${deskSec(`Latest from ${esc(name)}`, latest ? `<p class="dk-quiet">${esc(latest)}</p>` : "")}</div>`;
  return deskPage({ page: kind === "lead" ? "lead" : kind === "desk" ? "dispatcher" : "session", kind, back: deskBack(deskLeadName(d) || "Back"),
    acts: s ? `<button class="btn small" ${deskChatAttrs(sid, d)}>Open its chat</button>` : "", mark: `${DESK_MARKS[kind]} · `, title: esc(name), meta: esc(head), now, panel: who, main, foot });
}
/** A crew member: where it is, its files, its latest, Open its session and Stop it. No box of its own: the box goes
 *  to its lead. */
function deskCrewPane(lead, agent, d, foot = "") {
  const t = deskMate(lead, agent);
  const leadName = deskHolderName(lead), skey = `crew:${esc(lead)}:${esc(agent)}`;
  if (!t) return `<div class="dk-pane" data-scroll-key="${skey}">${deskBack(leadName, lead)}<div class="dk-empty">It isn't in ${esc(leadName)}'s crew any more.</div></div>${foot}`;
  const it = deskCrewItem(d.project, lead, agent);
  const shown = (t.files || []), files = shown.length ? `<div class="dk-files">${shown.map(esc).join(" · ")}</div>` : "";
  const latest = [t.summary, it?.latest].filter(Boolean);
  const stop = t.state === "working" ? `<button class="btn small" data-mt-act="stop" data-mt-sid="${esc(lead)}" data-mt-agent="${esc(t.agent_id || t.call)}">Stop it</button>` : "";
  const asks = mateNeeds(lead, t);
  const [dot, word] = asks ? ["ask", "Waiting on you"] : t.state === "working" ? ["work", "Working"] : [t.state === "done" ? "done" : "stop", t.state ? t.state[0].toUpperCase() + t.state.slice(1) : "Stopped"];
  const say = asks ? "Stopped to ask you something." : t.step ? esc(t.step) : mateLine(t);
  const leadBtn = deskChatLink(lead, leadName, d);
  const who = deskPsec("Who", deskPwho("Lead", leadBtn) + deskPwho("On", it ? deskItemLink(it) : "") + deskPwho("Files", shown.length ? `${shown.length} changed` : ""));
  const main = `<div class="dk-pane" data-scroll-key="${skey}">
    ${deskSec("Files", files)}
    ${deskSec("Latest", latest.map((l) => `<div class="dk-md dk-quiet">${md(l)}</div>`).join(""))}
    <p class="dk-note">To steer it, tell ${esc(leadName)} below; the lead passes it on. Open its session to talk to it straight.</p></div>`;
  return deskPage({ page: "crew", kind: "crew", back: deskBack(leadName, lead), mark: "Crew · ", title: esc(t.name),
    meta: `in <b>${esc(leadName)}</b>'s crew${it?.title || t.what ? ` · on <b>${esc(it?.title || t.what)}</b>` : ""} · ${esc(mateSpan(t))}`,
    now: deskPnow({ dot, word, say, acts: `<button class="btn small" data-desk-crew-open="${esc(t.agent_id || t.call)}" data-lead="${esc(lead)}">Open its session</button>${stop}`, fold: `in ${esc(leadName)}'s crew` }),
    panel: who, main, foot });
}

/** The draft key of the box that sends to `to` ({ project, item } or { sid, mate }), noted with where it sends. */
function deskBoxKey(to) {
  const key = to.sid ? `desk-to:${JSON.stringify([to.sid, to.mate || "", ...(to.item ? [to.item] : [])])}` : `desk:${JSON.stringify([to.project, to.item || ""])}`;
  deskBoxes.set(key, to);
  return key;
}
/** An item page's box: a prefix that stays as you type, who gets it and which item ("To cue-131 · Task #131"), and
 *  the item's short title in the empty box ("Remove Actions…"). */
const deskToLine = (who, i) => ({ pre: `To ${who || "the lead"}${i ? ` · ${deskNum(i)}` : ""}`, ph: i ? `${deskShort(i.title)}…` : "" });
const deskPreHtml = (pre) => (pre ? `<span class="dk-pre" data-desk-pre="">${esc(pre)}</span>` : "");
/** The box at the foot: to the Desk's lead (about the open item, named, when one is), or to an open holder (about its
 *  crew member, when one is). None without a live lead; a holder whose session has ended has none either. */
function deskBox(d) {
  const o = deskOpen;
  const lead = deskLeadName(d);
  // No lead, or one that has ended: no box to it (a holder's own box stays).
  if ((!deskLead(d) || deskLeadEnded(d)) && (!o || o.kind === "item" || o.kind === "addon")) return "";
  let to = { project: d.project }, ph = `Tell ${lead}: a bug, a feature, or a question…`, pre = "";
  if (o?.kind === "item") {
    to = { project: d.project, item: o.id };
    // Who gets it, as deskSendBox sends it: ready to check, it goes back to its holder; else to the lead.
    const it = deskItem(d.project, o.id);
    ({ pre, ph } = deskToLine(it?.state === "to_check" && it.holder ? it.holder.name : lead, it));
  }
  else if (o?.kind === "holder" || o?.kind === "crew" || o?.kind === "chat") {
    const sid = o.kind === "crew" ? o.lead : o.sid;
    if (!sessionOf(sid)) return `<div class="dk-ended">Its session has ended.</div>`;
    if (o.kind !== "crew") { to = { sid }; ph = `Talk to ${deskHolderName(sid, d.items.find((i) => i.holder?.kind !== "crew" && i.holder?.sid === sid)?.holder.name)}…`; }
    else { const t = deskMate(o.lead, o.agent); to = { sid, mate: t?.name || "" }; ph = `Tell ${deskHolderName(sid)} about ${t?.name || "it"}…`; }
  }
  const key = deskBoxKey(to);
  // Pictures pasted, dropped or added, and the mic, as in a session's chat box (app/inputs.js, chat/compose.js); its
  // quick phrases over the text, and Claude's suggested reply in the empty box (Tab types it in) from the session it
  // talks to: the lead, or the open holder (not about an item, or a crew member: those words are Cue's, not its).
  const mic = dictating?.key === key;
  const sugSid = to.item ? `item:${d.project}|${to.item}` : to.mate ? "" : to.sid || deskLead(d)?.sid || "";
  const sug = suggestionFor(sugSid);
  // An item's page has no quick phrases: its box starts with who gets it, on its colored left end (#143).
  return `<div class="dk-box" data-drop="${esc(key)}">${o?.kind === "item" ? "" : quickRow(`data-quick="${esc(key)}"`)}${draftChips(key)}<div class="dk-box-row">${deskPreHtml(pre)}<div class="ta-wrap"><textarea rows="1" data-text="${esc(key)}"${sug ? ` class="sug" data-sug-sid="${esc(sugSid)}" aria-label="${esc(`Suggested: ${sug}. Tab types it in.`)}"` : ""} placeholder="${esc(sug ? " " : ph)}" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off">${esc(draft(key).text)}</textarea>${sugLineHtml(sug)}</div><button class="icon-btn mic ${mic ? "on" : ""}" data-mic="${esc(key)}" aria-label="${mic ? "Stop dictating" : "Dictate"}">${MIC_ICON}</button><button class="icon-btn" data-attach="${esc(key)}" aria-label="Add an image">${IMAGE_ICON}</button><button class="btn primary small" data-desk-send="${esc(key)}">Send</button></div></div>`;
}
/** An item page's foot: while its holder asks you, the answer goes there, so you stay on its page. Its ask's card
 *  (choices, Decline, Type answer…) as under a chat; a turn that ended on it, a box to its holder. Else the box. */
function deskItemFoot(i, d) {
  if (i?.state !== "needs_you" || !i.holder) return deskBox(d);
  const it = i.ask?.item_id && findItem(i.ask.item_id);
  if (it && it.kind !== "waiting") return `<div class="dk-ask">${askFoot(it, it.session_id, it.harness || "claude", deskBoxKey({ project: d.project, item: i.id }))}</div>`;
  const sid = i.ask?.sid || (i.holder.kind === "crew" ? i.holder.lead_sid : i.holder.sid);
  if (!sessionOf(sid)) return `<div class="dk-ended">Its session has ended.</div>`;
  const to = i.holder.kind === "crew" && !i.ask ? { sid, mate: i.holder.name, item: i.id } : { sid, item: i.id };
  const key = deskBoxKey(to), mic = dictating?.key === key, line = deskToLine(i.holder.name, i);
  return `<div class="dk-box" data-drop="${esc(key)}">${draftChips(key)}<div class="dk-box-row">${deskPreHtml(line.pre)}<textarea rows="1" data-text="${esc(key)}" placeholder="${esc(line.ph)}" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off">${esc(draft(key).text)}</textarea><button class="icon-btn mic ${mic ? "on" : ""}" data-mic="${esc(key)}" aria-label="${mic ? "Stop dictating" : "Dictate"}">${MIC_ICON}</button><button class="icon-btn" data-attach="${esc(key)}" aria-label="Add an image">${IMAGE_ICON}</button><button class="btn primary small" data-desk-send="${esc(key)}">Send</button></div></div>`;
}
function deskRight(project) {
  const d = deskOf(project);
  const o = deskOpen;
  let body;
  // Each page takes its foot, the box under its own column (#200); the add-on's pane has it under the whole card.
  const foot = () => (o?.kind === "item" ? deskItemFoot(deskItem(project, o.id), d) : deskBox(d));
  let tail = "";
  if (o?.kind === "item") {
    const i = deskItem(project, o.id);
    if (i) { delete o.fresh; body = deskItemPane(i, d, foot()); }
    // Just added: it opens once Cue's state has it (the chat shows till then); not there in a few seconds, it won't.
    else if (o.fresh && now() - o.fresh < 5000) return deskRightChat(d);
  }
  else if (o?.kind === "holder") body = deskHolderPane(o.sid, d, foot());
  // A reply open in Read: read-only, as in a session's chat, so no box under it (Reply brings it back).
  else if (o?.kind === "chat") { const rd = deskReader(sessionOf(o.sid)); body = rd || deskSessChat(o.sid, o.from, d, foot()); }
  else if (o?.kind === "crew") body = deskCrewPane(o.lead, o.agent, d, foot());
  else if (o?.kind === "addon") {
    const h = deskGroups(project).extra.find((x) => (x.key || x.name) === o.key);
    let html = "";
    try { html = (h && addOn.deskPane?.(h)) || ""; } catch (e) { console.error(e); }
    body = h ? `<div class="dk-pane" data-scroll-key="addon:${esc(o.key)}">${deskBack(deskLeadName(d) || "Back")}${html || `<h1>${esc(h.name)}</h1><div class="dk-state">${esc(h.sub || "")}</div>`}</div>` : null;
    if (body) tail = foot();
  }
  if (o && !body) deskOpen = null;
  if (deskOpen) return `<div class="dk-card dk-right">${body}${tail}</div>`;
  return deskRightChat(d);
}
/** The chat card with nothing open: who runs the Desk (no lead yet, or Change), else the lead's chat (or the line
 *  saying it ended), or Decisions. */
function deskRightChat(d) {
  if (!deskLead(d) || deskPick === d.project) return deskPicker(d);
  const tab = (t, label, n) => `<button class="dk-tab ${deskTab === t ? "on" : ""}" data-desk-tab="${t}" aria-pressed="${deskTab === t}">${label}${n ? `<span class="n">${n}</span>` : ""}</button>`;
  const ask = deskLeadEnded(d) ? null : deskLeadAsk(d);
  const rd = deskTab === "lead" && !deskLeadEnded(d) ? deskReader(deskLeadSession(d), ask) : null;
  if (rd) return `<div class="dk-card dk-right">${rd}</div>`;
  const ended = deskLeadEnded(d);
  const main = deskTab === "decisions" ? deskDecisions(d) : ended ? deskEndedLine(d) : deskChat(d);
  // One page (#200): the Desk's name for back (it's where you are), "Dispatcher · " and its lead's name; its chat and
  // Decisions at the left, the box (or the lead's ask) under them; in the panel its state, what waits on you and
  // the sessions at work for it.
  const open = d.items.filter((i) => i.state !== "done" && i.state !== "dropped").length;
  const working = deskWorkingFor(d);
  const meta = ["The Desk lead", `${open} ${open === 1 ? "item" : "items"}`, working.length ? `${working.length} ${working.length === 1 ? "session" : "sessions"} working for it` : ""].filter(Boolean).join(" · ");
  const waits = d.items.filter(deskWaits);
  const why = (i) => (i.state === "needs_you" ? (i.ask?.kind === "permission" ? "Asks to use a tool" : "Asks you something") : "Ready to check");
  const waitRows = waits.map((i) => `<button class="dk-prow" data-desk-chk-item="${esc(i.id)}"><i class="dk-pdot ask" aria-hidden="true"></i><span class="dk-prt">${deskKindIcon(i)}<b>${esc(deskNum(i))}</b></span><span class="dk-prs">${esc(why(i))} · ${ago(deskMs(i.updated))}</span></button>`).join("");
  const workRows = working.map(({ sid, name, on }) => `<button class="dk-prow" ${deskChatAttrs(sid, d)}><i class="dk-pdot work" aria-hidden="true"></i><span class="dk-prt"><b>${esc(name)}</b>${on ? ` <span class="dk-prs">on #${esc(on.id)}</span>` : ""}</span></button>`).join("");
  const panel = deskPsec(`Waits on you · ${waits.length}`, waitRows ? `<div class="dk-prows">${waitRows}</div>` : "") + deskPsec(`Working · ${working.length}`, workRows ? `<div class="dk-prows">${workRows}</div>` : "");
  const foot = ended ? "" : ask ? `<div class="dk-ask">${askFoot(ask, ask.session_id, ask.harness || "claude", deskBoxKey({ project: d.project }))}</div>` : deskBox(d);
  return `<div class="dk-card dk-right">${deskPage({ page: "dispatcher", kind: "desk", back: `<span class="dk-back dk-here">${esc(deskName(d))}</span>`, mark: "Dispatcher · ", title: esc(deskLeadName(d)), meta: esc(meta),
    now: deskLeadNow(d, waits, working), panel, main: `<div class="dk-tabs dk-ptabs">${tab("lead", esc(deskLeadName(d)), 0)}${tab("decisions", "Decisions", deskCalls(d).length)}</div>${main}`, foot })}</div>`;
}
/** The sessions at work for Desk `d` (not its lead): [{ sid, name, on }], `on` the item each is on. */
const deskWorkingFor = (d) => deskSessionsOf(d.project, d).filter((r) => r.sid !== deskLead(d)?.sid && sessionOf(r.sid)?.state === "working").map((r) => ({ ...r, on: deskSessItem(d, r.sid) }));
/** The Dispatcher's NOW (Lead chats pick 1 + D, as a panel since #200): its state's dot and word; a sentence: what it
 *  routes ("Routing #125"), what waits on you, how many items are with sessions; Check #131 opening the first that
 *  waits on you. Folded: "· 4 items · 6 sessions working". */
function deskLeadNow(d, waits, working) {
  const fold = `${waits.length} waiting · ${working.length} ${working.length === 1 ? "session" : "sessions"} working`;
  if (deskLeadEnded(d)) return deskPnow({ dot: "stop", word: "Ended", say: "Items stay here: resume it or pick another.", fold });
  const lead = deskLead(d), s = sessionOf(lead.sid);
  const [dot, word] = deskPstate(s);
  const fresh = d.items.filter((i) => i.state === "new" && !i.holder);
  const routing = word === "Working" && fresh.length === 1 ? fresh[0] : null;
  const held = d.items.filter((i) => i.holder && (deskMoving(i)) && deskGroupKey(i.holder) !== lead.sid).length;
  const num = (i) => `${deskKindIcon(i)}<b class="dk-inum">#${esc(i.id)}</b>`;
  const say = [routing ? `Routing ${num(routing)} to a session.` : "", waits.length === 1 ? `${num(waits[0])} waits on you.` : waits.length ? `${waits.length} items wait on you.` : "",
    held ? `${held} ${held === 1 ? "item is" : "items are"} with sessions.` : ""].filter(Boolean).join(" ") || "Nothing to route right now.";
  const check = waits.length ? `<button class="btn small primary dk-pgo" data-desk-chk-item="${esc(waits[0].id)}">Check #${esc(waits[0].id)}</button>` : "";
  return deskPnow({ dot, word, say, acts: check, fold });
}
/** The lead's own open ask (an ASK line's question, a permission, a prompt), not a crew member's (those are in its
 *  crew's Needs you): while it's open the lead takes no message, so its card (choices, Decline, Type answer…) stands
 *  in the box's place, as under a session's chat. Null when none. */
function deskLeadAsk(d) {
  const sid = deskLead(d)?.sid;
  return (sid && state.items.find((i) => i.session_id === sid && i.kind !== "waiting" && !mateOfAsk(i))) || null;
}

// ---------- All projects: every Desk (desk_home.js) ----------

/** The Desk view: the project picked (its items, and its lead beside them), or with All projects every Desk. */
function deskView() {
  // What was open belongs to the project it was opened in.
  if (deskOpen && deskOpen.project !== deskSelection()) { deskOpen = null; deskChatEnd = true; }
  if (deskPick && deskPick !== deskSelection()) deskPick = null;
  const given = deskGiveFor && deskItem(deskSelection(), deskGiveFor.id);
  if (deskGiveFor && (!given || given.holder)) deskGiveFor = null;
  if (deskDropArm && !deskItem(deskSelection(), deskDropArm)) deskDropArm = null;
  deskBoxes.clear();   // the boxes drawn now note theirs again
  if (!deskSelection()) return deskAll();
  deskLastSet(deskSelection());
  queueMicrotask(deskSettle);
  return `<div class="board dk-board">${deskSessions(deskSelection())}${deskRight(deskSelection())}${deskList(deskSelection())}</div>`;
}
/** How many messages each item's Thread showed when last drawn, and for which opening of its page, by item. The page
 *  opens at its newest message, as a chat does; then one more (it arrived, or you sent it) scrolls to show it (#145). */
const deskThreadSeen = new Map();
function deskThreadFollow() {
  const pane = document.querySelector(".dk-ipane"), th = pane?.querySelector(".dk-thread");
  if (!th) return;
  const rows = th.querySelectorAll(":scope > .dk-tm"), key = pane.dataset.scrollKey, was = deskThreadSeen.get(key);
  deskThreadSeen.set(key, { n: rows.length, open: deskOpen });
  // Just opened: at its end, the newest message and where it stands (what to check) in view.
  if (was?.open !== deskOpen) { pane.scrollTop = pane.scrollHeight; return; }
  if (rows.length <= was.n) return;
  // Down until the newest is in full view under the pinned header; one taller than the page shows from its top.
  const r = rows[rows.length - 1].getBoundingClientRect(), bottom = pane.getBoundingClientRect().bottom;
  const head = pane.querySelector(".dk-ihead")?.getBoundingClientRect().bottom ?? pane.getBoundingClientRect().top;
  const down = Math.min(r.bottom + 16 - bottom, r.top - head - 12);
  if (down > 0) pane.scrollTop += down;
}
/** After the draw: the lead's chat at its end, as a chat opens (unless you'd scrolled up in it); an item's Thread at
 *  a message that just came. */
/** What's open on the Desk when its box last had the chance to take the cursor ("desk|{open}"), null off the Desk. */
let deskBoxShown = null;
/** When a key was last pressed: a row opened with Enter after it keeps the keys (pointedAt: the last click, chat/patch.js). */
let deskKeyAt = 0;
document.addEventListener("keydown", () => { deskKeyAt = Date.now(); }, true);
/** A chat opened on the Desk (the Desk itself, the lead's chat, an item's page, a session's or a crew member's): its
 *  text box takes the cursor, so you can just type (#154), as a chat on the Board does. Not over your choice: not
 *  while you're writing in another box, a sheet is open, or you're moving through the list with the keys. */
function deskFocusOpened() {
  const k = view === "desk" ? `${deskSelection()}|${JSON.stringify(deskOpen)}` : null;
  if (k === deskBoxShown) return;
  deskBoxShown = k;
  const box = k && document.querySelector(".dk-right .dk-box textarea[data-text]"), a = document.activeElement;
  if (!box || sheet || a?.matches?.("input, textarea, select, [contenteditable]")) return;
  if (a?.closest?.(".dk-board") && a.matches('[role="button"]') && deskKeyAt > pointedAt) return;   // keys, not a click
  box.focus({ preventScroll: true });
  box.setSelectionRange(box.value.length, box.value.length);
}
function deskSettle() {
  deskDoneWatch();
  deskThreadFollow();
  deskFocusOpened();
  const c = document.querySelector(".dk-chat");
  // Earlier messages just went in above (chat/thread.js loadOlder): what you were reading stays put.
  if (c && holdChat != null) { c.scrollTop = c.scrollHeight - holdChat; holdChat = null; }
  else if (c && deskChatEnd) c.scrollTop = c.scrollHeight;
}

// ---------- acting ----------

/** Open `what` in the chat card (null: back to the lead's chat). */
function deskShow(what) {
  deskLeadLit = null;
  if (what?.kind === "item") deskSeeItem(what.id);
  deskOpen = what && { ...what, project: deskSelection() };
  deskDropArm = null;
  deskGiveFor = null;
  if (!what) deskChatEnd = true;
  renderMain();
}
/** Send what's in box `key`: to the lead (naming the open item), or to the lead (naming its crew member). Only what's
 *  sent is named: the box keeps what you typed if it doesn't go. */
async function deskSendBox(key) {
  const to = deskBoxes.get(key);
  const text = draft(key).text.trim();
  const images = draft(key).images.map(({ name, mime, data }) => ({ name, mime, data }));
  if (!to || (!text && !images.length)) return;
  if (to.sid) {
    // An answer from an item's page: the page says what you answered (deskChain).
    const it = to.item && deskItem(deskSelection(), to.item);
    if (it?.state === "needs_you" && sessionOf(to.sid)?.state !== "deciding" && !state.restarting?.[to.sid]) deskAnswers.set(it.id, { q: deskQuestion(it), a: text || "a picture", ask: it.ask?.item_id || "", at: now() });
    // From an item's page: it goes as about that item, so the session answers about it and its reply is that item's (#176).
    if (!to.mate && it && !/^[/!]/.test(text)) {
      if (state.restarting?.[to.sid]) return toast("It's restarting. Send once it's back.");
      if (sessionOf(to.sid)?.state === "deciding") return toast("It's asking you something first. Answer that, then send.");
      await deliver(to.sid, key, (typed, images) => {
        const said = typed ? `About #${it.id} (${it.title}): ${typed}` : `About #${it.id} (${it.title}): (a picture)`;
        insSent(to.sid, said, false);
        return invoke("send_to_session", { sessionId: to.sid, text: said, images, now: false });
      });
    }
    else if (!to.mate) await sendTo(to.sid, false, key);
    else if (state.restarting?.[to.sid]) return toast("It's restarting. Send once it's back.");
    else if (sessionOf(to.sid)?.state === "deciding") return toast("It's asking you something first. Answer that, then send.");
    else await deliver(to.sid, key, (typed, images) => {
      const said = `About ${to.mate}: ${typed}`;
      insSent(to.sid, said, false);
      return invoke("send_to_session", { sessionId: to.sid, text: said, images, now: false });
    });
    // Went (the box emptied): on the item's page, in its Steps.
    if (it && text && !draft(key).text.trim()) { deskPend(it.id, text, "holder"); deskWrote(it.id, "wrote_holder", text); renderMain(); }
    return;
  }
  const { project, item: id } = to;
  const it = id && deskItem(project, id);
  // About an item ready to check: it's sent back with your words, to its holder, as Send back does (off Waiting on you).
  if (it?.state === "to_check" && it.holder) {
    // Its page stays, as after anything you write about an item: the box empties and keeps the cursor.
    await deskSendBack(id, key, text, images);
    return renderMain();
  }
  const said = it ? `About #${it.id} (${it.title}): ${text}` : text;
  endDictation(key);
  delete drafts[key];
  saveDrafts();
  const list = deskSent.get(project) || [], mine = { text: said, images, at: now(), sent: 0 };
  list.push(mine);
  deskSent.set(project, list);
  // To the lead on its own: the chat shows it at once, Sending. From an item: its page stays, the box emptied and the
  // cursor still in it, so you can go on writing (it used to swap to the lead's chat once the message went, seconds
  // later, taking the box from under your typing); the lead's chat has it when you go back.
  if (!it) { deskTab = "lead"; deskChatEnd = true; }
  // About an item: on its Thread at once.
  const pend = it && text ? deskPend(it.id, text, "lead") : null;
  renderMain();
  try {
    await invoke("desk_send", { project, text: said, images });
    mine.sent = now();
    if (it) { toast(`Sent to ${deskLeadName(deskOf(project)) || "the lead"}`); if (text) deskWrote(it.id, "wrote_lead", text); }
  }
  catch (e) { if (pend) deskUnpend(it.id, pend); deskSent.set(project, (deskSent.get(project) || []).filter((p) => p !== mine)); Object.assign(draft(key), { text, images }); toast(`Not sent: ${e}`); renderMain(); }
}
/** What you wrote on item `id`'s page, sent (`act`: to its lead or its holder): kept in its Steps (hub/desk.rs act_on). */
const deskWrote = (id, act, text) => invoke("desk_item_act", { id, act, text }).catch((e) => console.error(e));
/** Item `id` sent back to its holder from box `key` with `text` and `images`: the box emptied once it went (kept when
 *  it didn't). Whether it went. */
async function deskSendBack(id, key, text, images) {
  endDictation(key);
  const pend = text.trim() ? deskPend(id, text.trim(), "back", (images || []).length) : null;
  if (pend) renderMain();
  try {
    const it = await invoke("desk_item_act", { id, act: "send_back", text, images });
    delete drafts[key];
    saveDrafts();
    toast(`Sent back${it?.holder?.name ? ` to ${it.holder.name}` : ""}`);
    return true;
  }
  catch (e) { if (pend) deskUnpend(id, pend); toast(`Not sent: ${e}`, 5000); return false; }
}
/** Item `id` of the project picked, open in the chat card (the Inbox's Open in Desk). */
function deskOpenItem(id) {
  deskSeeItem(id);
  deskOpen = { kind: "item", id, project: deskSelection(), fresh: now() };
  deskDropArm = deskGiveFor = null;
}
async function deskAct(id, act, el = null) {
  // Looks right commits the item's files at once; Undo on a done item only puts it back to check (desk_commit.js).
  if (act === "looks_right") return deskLooksRight(id);
  if (act === "looks_undo") return deskLooksUndo(id);
  if (act === "reopen") return deskReopen(id);
  if (act === "log_all") { if (!deskFinLogOpen.delete(id)) deskFinLogOpen.add(id); return renderMain(); }
  // Drop takes two clicks: the first arms it.
  if (act === "drop" && deskDropArm !== id) { deskDropArm = id; return renderMain(); }
  deskDropArm = null;
  // Send back opens the item's own box for why (deskBackHtml); its Send sends that, to the item's holder only.
  if (act === "send_back") { deskBackFor = deskBackFor === id ? null : id; renderMain(); return deskBackFocus(id); }
  if (act === "back_cancel") { endDictation(`back:${id}`); deskBackFor = null; return renderMain(); }
  if (act === "back_go") {
    const key = `back:${id}`, text = draft(key).text.trim();
    const images = draft(key).images.map(({ name, mime, data }) => ({ name, mime, data }));
    if (!text && !images.length) { toast("Say why it goes back first", 4000); return deskBackFocus(id); }
    if (await deskSendBack(id, key, text, images)) deskBackFor = null;
    return renderMain();
  }
  try {
    await invoke("desk_item_act", { id, act });
    if (act === "drop") deskOpen = null;
  }
  catch (e) { toast(String(e), 5000); }
  renderMain();
}
/** Item `id`'s Send back, open: "Why send it back?" and Cancel over a box of its own (never the lead's), as the Desk's
 *  boxes are: words, the mic, a picture, Send. Enter sends, ⇧Enter a new line, Esc closes it. */
function deskBackHtml(id) {
  const key = `back:${id}`, mic = dictating?.key === key;
  return `<div class="dk-sb"><div class="dk-sb-h"><span class="dk-sb-l">Why send it back?</span><button class="dk-sb-x" data-desk-act="back_cancel" data-id="${esc(id)}">Cancel</button></div><div class="dk-box dk-sb-box" data-drop="${esc(key)}">${draftChips(key)}<div class="dk-box-row"><textarea rows="1" data-text="${esc(key)}" aria-label="Why send it back?" placeholder="What isn't right yet" spellcheck="false" autocorrect="off" autocapitalize="off" autocomplete="off">${esc(draft(key).text)}</textarea><button class="icon-btn mic ${mic ? "on" : ""}" data-mic="${esc(key)}" aria-label="${mic ? "Stop dictating" : "Dictate"}">${MIC_ICON}</button><button class="icon-btn" data-attach="${esc(key)}" aria-label="Add an image">${IMAGE_ICON}</button><button class="btn primary small" data-desk-act="back_go" data-id="${esc(id)}">Send</button></div></div></div>`;
}
const deskBackFocus = (id) => document.querySelector(`textarea[data-text="${CSS.escape(`back:${id}`)}"]`)?.focus();
async function deskUndo(id) {
  try { toast((await invoke("desk_undo", { decisionId: id })) || "Asked to undo it"); }
  catch (e) { toast(String(e)); }
  renderMain();
}
/** A crew member's own session: its lead's crew page, on that member. */
function deskOpenCrew(lead, agent) {
  const t = deskMate(lead, agent);
  if (!t || !sessionOf(lead)) return;
  if (typeof addOn.openCrewPage !== "function") return openSession(lead);
  addOn.openCrewPage(lead, { from: "desk", back: "the Desk" });
  view = "desk";   // its lead becomes the active session, which picks the Board: the page shows on the Desk
  crewLeftSet(lead, { kind: "mate", key: mateKey(lead, t), tab: "", file: "" });
  renderMain();
}

/** The Desk's clicks, by attribute: each with its element. */
const DESK_CLICKS = {
  deskPre: (el) => el.closest(".dk-box-row")?.querySelector("textarea")?.focus(),
  deskItab: (el) => { deskItemTabs.set(el.dataset.id, el.dataset.deskItab); renderMain(); },
  deskPanel: (el) => deskPanelToggle(el.dataset.deskPanel),
  deskItem: (el) => deskShow(deskOpen?.kind === "item" && deskOpen.id === el.dataset.deskItem ? null : { kind: "item", id: el.dataset.deskItem }),
  deskHolder: (el) => deskShow({ kind: "holder", sid: el.dataset.deskHolder }),
  deskAddon: (el) => deskShow({ kind: "addon", key: el.dataset.deskAddon }),
  deskCrew: (el) => deskShow({ kind: "crew", lead: el.dataset.lead, agent: el.dataset.deskCrew }),
  deskBack: (el) => deskShow(el.dataset.deskBack ? { kind: "holder", sid: el.dataset.deskBack } : null),
  deskTab: (el) => { deskTab = el.dataset.deskTab; deskChatEnd = true; renderMain(); },
  ...DESK_LEAD_CLICKS,
  deskSend: (el) => deskSendBox(el.dataset.deskSend),
  deskAct: (el) => { deskGiveFor = null; deskAct(el.dataset.id, el.dataset.deskAct, el); },
  deskUndo: (el) => deskUndo(el.dataset.deskUndo),
  deskLogRow: (el) => { const id = el.dataset.deskLogRow; if (!deskLogOpen.delete(id)) deskLogOpen.add(id); renderMain(); },
  deskSession: (el) => openSession(el.dataset.deskSession),
  deskLeadChat: () => { deskPick = null; deskTab = "lead"; deskShow(null); deskLeadLit = deskSelection(); renderMain(); },
  deskChkItem: (el) => deskShow({ kind: "item", id: el.dataset.deskChkItem }),
  deskRen: (el) => renameSession({ dataset: { rename: el.dataset.deskRen } }),
  // Opened from an item's page or a holder's, it remembers that page so ‹ goes back to it (from the left list: the lead).
  deskChatOf: (el) => { deskChatEnd = true; deskShow({ kind: "chat", sid: el.dataset.deskChatOf, from: ["item", "holder"].includes(deskOpen?.kind) ? { ...deskOpen } : null }); },
  deskChatBack: () => deskShow(deskOpen?.from || null),
  deskNewSession: (el) => deskNewSession(el.dataset.deskNewSession),
  deskAddSession: (el) => {
    const project = el.dataset.deskAddSession;
    deskAddingSession = deskAddingSession === project ? null : project;
    renderMain();
  },
  deskMember: (el) => deskMember(el.dataset.project, el.dataset.deskMember, true),
  deskDoneFold: (el) => {
    const sid = el.dataset.deskDoneFold;
    if (deskFoldOpen.has(sid)) deskFoldOpen.delete(sid); else deskFoldOpen.add(sid);
    try { localStorage.setItem("cue.deskFold", JSON.stringify([...deskFoldOpen])); } catch { /* kept for this window only */ }
    renderMain();
  },
  deskMade: (el) => { deskSeeItem(el.closest("[data-desk-item]")?.dataset.deskItem || deskOpen?.id); invoke("open_link", { url: el.dataset.deskMade }).catch((err) => toast(`Couldn't open: ${err}`)); renderMain(); },
  deskDiffFile: (el) => deskShowFile(el.dataset.deskDiffFile, +el.dataset.n),
  deskCrewOpen: (el) => deskOpenCrew(el.dataset.lead, el.dataset.deskCrewOpen),
  deskHome: () => { deskSelect(""); deskOpen = null; deskPick = null; renderMain(); },
  deskProject: (el) => { deskSelect(el.dataset.deskProject); deskOpen = null; deskTab = "lead"; deskChatEnd = true; renderMain(); },
};
/** A click rule's attribute: deskItem → data-desk-item. */
const deskAttr = (k) => `data-${k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`;
const DESK_SEL = Object.keys(DESK_CLICKS).map((k) => `[${deskAttr(k)}]`).join(", ");
/** Element `el` (one with a Desk attribute, or null) clicked or pressed: an armed Drop disarms on anything else (as
 *  Remove does in the projects list); then `el` does its thing. True when it was the Desk's. */
function deskTake(el) {
  if (deskDropArm && !(el?.dataset.deskAct === "drop" && el.dataset.id === deskDropArm)) { deskDropArm = null; if (!el) renderMain(); }
  if (!el || !el.closest(".dk-board")) return false;
  DESK_CLICKS[Object.keys(DESK_CLICKS).find((k) => k in el.dataset)](el);
  return true;
}
/** A click in the Desk: the innermost element with one of its attributes. */
function deskClick(e) {
  if (view !== "desk" || e.button > 0) return;
  // In a session's name box (✎ on its row): the click stays in the box, it doesn't open the row.
  if (e.target.closest?.(".dk-board .rename-in")) { e.stopPropagation(); return; }
  if (deskTake(e.target.closest?.(DESK_SEL))) e.stopPropagation();
}
/** Keys on the Desk: ⌘N goes to the quick-add line, Enter sends a box (⇧Enter a new line), the quick-add line's keys
 *  (desk_lead.js), Esc closes a Give to… menu, then the picker (when changing), then the open pane. */
function deskKey(e) {
  if (view !== "desk" || e.isComposing || e.keyCode === 229) return false;
  const key = e.target?.dataset?.text || "";
  // A row (an item, a holder, a crew member, a project's Desk): Enter or Space opens it as a click does, and it keeps
  // the keyboard where it is.
  const row = e.target?.matches?.('.dk-board [role="button"]') && e.target.matches(DESK_SEL) ? e.target : null;
  if (row && (e.key === "Enter" || e.key === " ") && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
    const at = Object.keys(DESK_CLICKS).find((k) => k in row.dataset), sel = `[${deskAttr(at)}="${CSS.escape(row.dataset[at])}"]`;
    deskTake(row);
    document.querySelector(`.dk-board ${sel}`)?.focus();
    return true;
  }
  if (e.metaKey && !e.shiftKey && !e.altKey && !e.ctrlKey && e.key?.toLowerCase() === "n" && deskSelection() && !sheet && !(crewPageSid && document.querySelector(".crew-page"))) return deskQaFocus();
  if (deskQaKey(e, key)) return true;
  if (e.key === "Enter" && !e.shiftKey && deskBoxes.has(key)) { deskSendBox(key); return true; }
  if (e.key === "Enter" && !e.shiftKey && key.startsWith("back:")) { deskAct(key.slice(5), "back_go"); return true; }
  if (e.key === "Escape" && key.startsWith("back:")) { deskAct(key.slice(5), "back_cancel"); return true; }
  // Esc: only when nothing over the Desk wants it (the crew page, a sheet, search and the live list, the projects
  // list, a menu, a rename), as the Inbox's keys do.
  const over = sheet || lightbox || liveOpen || deleteAsk || renaming || pjRenaming || pjOpen || pjNewOpen || colorFor || newProjFor || btwFor || menuFor || redirectFor || (typeof moreFor !== "undefined" && moreFor) || (crewPageSid && document.querySelector(".crew-page"));
  if (e.key === "Escape" && !e.metaKey && !e.ctrlKey && !e.altKey && !over) {
    if (deskAddingSession && deskAddingSession === deskSelection()) {
      deskAddingSession = null;
      renderMain();
      document.querySelector("[data-desk-add-session]")?.focus();
      return true;
    }
    if (deskDropArm) { deskDropArm = null; renderMain(); return true; }
    if (deskGiveFor) { deskGiveFor = null; renderMain(); return true; }
    if (deskPick && deskLead(deskOf(deskSelection()))) { deskPick = null; renderMain(); return true; }
    if (deskOpen && !(e.target.matches?.("textarea, input") && e.target.value)) { deskShow(deskOpen.kind === "crew" ? { kind: "holder", sid: deskOpen.lead } : deskOpen.kind === "chat" ? deskOpen.from || null : null); return true; }
  }
  return false;
}

if (typeof document !== "undefined") {
  document.addEventListener("keydown", (e) => { if (deskKey(e)) { e.preventDefault(); e.stopImmediatePropagation(); } }, true);
  document.addEventListener("click", deskClick, true);
  // Leaving a session's name box (✎ on the Desk) drops the rename (Enter has kept it already). Checked a moment later:
  // a redraw puts the cursor back in the new box, and that isn't leaving it.
  document.addEventListener("focusout", (e) => {
    if (!e.target?.matches?.(".dk-board .rename-in")) return;
    const key = e.target.dataset.text;
    setTimeout(() => {
      if (!renaming || `rename:${renaming}` !== key || document.activeElement?.dataset?.text === key) return;
      renaming = null;
      delete drafts[key];
      renderMain();
    }, 0);
  }, true);
  // The lead's chat scrolled: whether it's at its end (new messages keep it there).
  document.addEventListener("scroll", (e) => { const c = e.target; if (c?.matches?.(".dk-chat")) deskChatEnd = c.scrollHeight - c.scrollTop - c.clientHeight < 24; }, true);
}
