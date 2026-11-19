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
/** The Desks you opened last, newest first (#274): the one-click buttons beside the Desk's name. Kept in this window;
 *  a Desk goes first as its head is drawn (deskSessHead). */
let deskRecent = (() => { try { return JSON.parse(localStorage.getItem("cue.deskRecent") || "[]").filter((x) => typeof x === "string"); } catch { return []; } })();
function deskRecentPut(id) {
  deskRecent = [id, ...deskRecent.filter((x) => x !== id)].slice(0, 8);
  try { localStorage.setItem("cue.deskRecent", JSON.stringify(deskRecent)); } catch {}
}
/** Up to three of the Desks you opened last, other than `project`, while their names fit in about 30 letters
 *  together (always one, its name cut short if need be). Each opens its Desk. */
function deskRecentChips(project) {
  const ds = state.desk?.projects || [], out = [];
  let room = 30;
  for (const id of deskRecent) {
    const d = id !== project && ds.find((x) => x.project === id);
    if (!d) continue;
    const name = deskName(d);
    if (out.length && name.length > room) break;
    out.push(`<button class="dk-rc" data-desk-go="${esc(d.project)}">${esc(name)}</button>`);
    room -= name.length;
    if (out.length === 3 || room <= 0) break;
  }
  return out.length ? `<span class="dk-rcs">${out.join("")}</span>` : "";
}
const deskHasSession = (d, sid) => !!sid && (deskLead(d)?.sid === sid || (d.members || []).includes(sid));
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
/** A reply's "This turn in …" opened its session here (#306): the time of the reply to bring into view, once. */
let deskTurnAt = 0;

/** Seconds or ms, as the hub stamps them, in ms. */
const deskMs = (t) => (!t ? 0 : t < 1e12 ? t * 1000 : t);
/** Project `p`'s Desk ({ project, lead, items, decisions, members: its sessions' ids }), or an empty one. A dropped item is gone from it. */
const deskOf = (p) => {
  const d = (state.desk?.projects || []).find((x) => x.project === p);
  return d ? { ...d, items: d.items.filter((i) => i.state !== "dropped") } : { project: p, lead: null, items: [], decisions: [] };
};
// An item by its number, or by the tag it had before items were numbered (its alias), so old lines still find it.
const deskIdIs = (i, id) => i.id === id || (!!i.alias && i.alias === id);
// A big feature's piece done days ago is only in its feature's `pieces` (#282): found there too.
const deskItem = (p, id) => {
  const its = deskOf(p).items, old = typeof deskDonePages === "undefined" ? [] : deskDonePages.get(p)?.items || [];
  return its.find((i) => deskIdIs(i, id)) || [...its, ...old].flatMap((i) => i.pieces || []).find((i) => deskIdIs(i, id)) || null;
};
/** An item as people name it: its kind and number, "Bug #2", "Task #1"; "#2" while it has no kind yet. */
/** An item's kind as a small line icon in the one theme color (the "Type marks" pick): a checked box, a bug, a
 *  sparkle. The one helper for it: every place an item's kind shows (the item list, the left list, the item page,
 *  the lead's route and new-item cards, Ready to check) takes it from here, styled by .dk-kic. Takes an item or a
 *  kind; "" for none. */
const DESK_KIND_SVG = {
  task: '<rect x="4" y="4" width="16" height="16" rx="4"/><path d="m8.5 12.2 2.4 2.4 4.6-5"/>',
  bug: '<rect x="8" y="7" width="8" height="12" rx="4"/><path d="M12 11v8M8 12H4.5M19.5 12H16M8 16.5l-3 1.5M16 16.5l3 1.5M8.5 9 6 7M15.5 9 18 7M10 7a2 2 0 0 1 4 0"/>',
  feature: '<path d="M12 3.5 13.9 10.1 20.5 12 13.9 13.9 12 20.5 10.1 13.9 3.5 12 10.1 10.1Z"/>',
  tray: '<path d="M4 13l3-7h10l3 7v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1z"/><path d="M4 13h4l2 3h4l2-3h4"/>',
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
const deskChkUninstalled = (c) => !!c?.cue && c.finish?.step !== "checked" && !/^https?:\/\//.test(c.designs || "") && !((state.desk?.installed_ms || 0) > (c.edited || c.at));
const deskLatestCheck = (i) => i?.checks?.at(-1) || (state.desk?.projects || []).find((d) => d.project === i?.project)?.checks?.find((c) => c.id === i.id && c.last);
/** Item `i` is done but changes the app, and the installed Cue doesn't have it yet: nothing to try, so it isn't to check
 *  yet; it says "waiting for install" till Cue is installed. */
const deskInstallWait = (i) => i?.state === "to_check" && i.finish?.step !== "checked"
  && !deskFinCommit(i) && !/^https?:\/\//.test(i.designs || "") && deskChkUninstalled(deskLatestCheck(i));
/** The commit Cue made for item `i` when its session said DONE (hub: commit, build, install, then to check), or "". An
 *  item with one went through that, so it's in the installed Cue: never "waiting for install". */
const deskFinCommit = (i) => (i?.finish ? i.finish.commit || i.commit || "" : "");
/** Item `i` to check with its files not committed (its session is on another machine, or Undo put it back): not on
 *  main yet, so no Looks right. Its button commits them to main and closes it (hub/desk_commit.rs, Looks right's
 *  commit). One with no files changed has nothing to land: Looks right as ever. */
const deskUncommitted = (i) => i?.state === "to_check" && !i.finish && !i.commit && (i.files || []).length > 0;
/** Item `i`'s DONE on its way into the app: Cue commits, builds and installs it. Nothing for you to do yet. */
const deskFinishing = (i) => i?.state === "finishing";
/** Where a finishing item is, in a short line (HTML): what it's doing and since when ("Checking before it lands · since
 *  8:31pm"); in the next install, that it's on main and when it installs, counting down ("On main · installs at 9:05pm ·
 *  in 12m"), so a finished item never looks stuck while it waits (#339). */
function deskFinStep(i) {
  const f = i.finish || {};
  const since = f.at ? ` · since ${clockAt(f.at)}` : "";
  // In its own copy, waiting its turn to land (hub/desk_trees.rs): one item lands at a time.
  if (f.step === "line") return `In the landing line${f.wait ? ` · waits: ${esc(f.wait)}` : ""}${since}`;
  // Committed, and not on main yet: where it is ("on side, not main").
  if (f.step === "landing") return `Committed, not on main yet${f.wait ? ` · ${esc(f.wait)}` : ""}${since}`;
  if (f.step === "queued") {
    // Cue installs half an hour after the first item waits (hub/desk_finish.rs), once no session is mid-turn.
    const due = state.desk?.install_at || 0;
    if (!due) return "On main · waiting for the next install";
    return due > now() ? `On main · installs at ${clockAt(due)} · <span data-until="${due}">${untilIn(due)}</span>` : "On main · installs once no session is mid-turn";
  }
  const what = { checking: "Checking before it lands", check_failed: "Check failed", building: "On main · building Cue", installing: "On main · installing Cue", starting: "Installed · Cue is restarting" }[f.step] || "Committing";
  return `${what}${since}`;
}
/** How many copies of one repository Desk `d`'s items work in at once (its How work lands, lands.rs). */
const deskCopies = (d) => d?.lands?.set?.at_once || 3;
/** A dropped item's copy that had work in it (`k`: { id, title, path, branch, at }), kept on disk until you Remove it.
 *  Remove takes two clicks: it deletes the copy and its branch. */
function deskKeptRow(k) {
  const armed = deskDropArm === k.id;
  return `<div class="dk-it dk-kept" data-kept="${esc(k.id)}"><div class="dk-kept-t"><b>#${esc(k.id)}</b> ${esc(k.title)}<div class="dk-kept-s">${esc(k.path)} · ${esc(k.branch)} · dropped ${ago(k.at)} ago</div></div><button class="btn small${armed ? " armed" : ""}" data-desk-act="remove_kept" data-id="${esc(k.id)}">${armed ? "Remove it and its work" : "Remove"}</button></div>`;
}
/** Install now on a finishing item in the next install (step "queued"): everything waiting installs at once. */
const deskInstallNow = (i, cls) => (deskFinishing(i) && i.finish?.step === "queued" ? `<button class="${cls}" data-desk-act="install_now" data-id="${esc(i.id)}">Install now</button>` : "");
/** Which lane a finished item waits in (#346, option B of docs/desk-install-lane-plan.md): "landing" from its DONE till
 *  it's on main (committed, in the landing line, checked); "install" once it's on main and waits for the next install
 *  (or is in it); "" for everything else. One that stopped on a question (its check failed, a clash) isn't moving on
 *  its own: it stays with the work, In progress, or Waiting on you. */
const DESK_LANDING_STEPS = ["commit", "commit_wait", "line", "checking", "landing"], DESK_INSTALL_STEPS = ["queued", "building", "installing", "starting"];
function deskLane(i) {
  if (deskInstallWait(i)) return "install";
  if (!deskFinishing(i) || i.finish?.question) return "";
  const step = i.finish?.step || "commit";
  return DESK_INSTALL_STEPS.includes(step) ? "install" : DESK_LANDING_STEPS.includes(step) ? "landing" : "";
}
/** The landing line's order (hub/desk_trees.rs desk_land_line): the one being checked first; then those the lead moved
 *  up (LAND FIRST), the latest moved first; then the oldest DONE first. Committed and waiting to be on main from a
 *  shared folder (step "landing", no copy) after them. */
const deskLandRank = (i) => ({ checking: 0, landing: 2 })[i.finish?.step] ?? 1;
const deskLandOrder = (a, b) => deskLandRank(a) - deskLandRank(b) || (!a.finish?.first) - (!b.finish?.first)
  || (b.finish?.first || 0) - (a.finish?.first || 0) || (a.finish?.at || 0) - (b.finish?.at || 0);
/** A lane row's step, short, after its holder (HTML): "Checking · 6m", "Waits on #301", "Next to land", "On main ·
 *  c71e0b4". `ahead`: the item before it in the landing line. The item page keeps the long line (deskFinStep). */
function deskLaneStep(i, ahead) {
  const f = i.finish || {};
  const age = f.at ? ` · <span data-ago="${f.at}">${ago(f.at)}</span>` : "";
  if (deskInstallWait(i)) return "waiting for install";
  if (f.step === "checking") return `Checking${age}`;
  if (f.step === "line") return f.wait ? `Waits: ${esc(f.wait)}${age}` : ahead ? `Waits on #${esc(ahead.id)}${age}` : `Next to land${age}`;
  if (f.step === "landing") return `Not on main yet${f.wait ? ` · ${esc(f.wait)}` : ""}${age}`;
  if (f.step === "commit_wait") return `Waits to be committed${f.wait ? ` · ${esc(f.wait)}` : ""}${age}`;
  const c = (f.commit || i.commit || "").slice(0, 7);
  if (f.step === "queued") return `On main${c ? ` · ${esc(c)}` : ""}`;
  return { building: "Building Cue", installing: "Installing Cue", starting: "Cue is restarting" }[f.step] || `Committing${age}`;
}
/** The Waiting to install lane's head, right side: when the next install is, counting down ("9:05pm · in 12m"), and
 *  Install now while anything waits for it. Cue installs on its own half an hour after the first of them, once no
 *  session is mid-turn (hub/desk_finish.rs). The install is Cue's, one for every Desk. */
function deskInstallHead(items) {
  const waiting = items.find((i) => deskFinishing(i) && i.finish?.step === "queued");
  if (!waiting) return "";
  const due = state.desk?.install_at || 0;
  const when = !due ? "at the next install" : due > now() ? `${clockAt(due)} · <span data-until="${due}">${untilIn(due)}</span>` : "once no session is mid-turn";
  return `<span class="dk-next">${when}</span>${deskInstallNow(waiting, "dk-instnow")}`;
}
const deskItemRequest = (i) => i?.ask?.item_id && state.items.find((it) => it.id === i.ask.item_id && actionableRequest(it));
const deskReadyToReview = (i) => i.state === "to_check" && !deskInstallWait(i);
/** The managed finish flow released a Cue change with a user-facing check. Older review cards can still be optional. */
const deskNeedsVerification = (i) => deskReadyToReview(i) && !deskLooksSoon.has(i.id)
  && i.finish?.step === "checked" && !!deskLatestCheck(i)?.cue && !!(i.see || i.done_when)?.trim() && !i.designs;
const deskWaits = (i) => i.state === "needs_you" && (!i.ask?.item_id || !!deskItemRequest(i))
  || deskNeedsVerification(i) || (deskReadyToReview(i) && !deskLooksSoon.has(i.id) && !!deskPickLine(i));

/** Use the same current request on the list, the item page, and the Desk's counts. */
function deskItemsWithRequests(d, taskApprovals = deskTaskApprovals(d)) {
  return d.items.map((i) => {
    if (deskItemRequest(i)) return i;
    const request = taskApprovals.get(i.id);
    if (request) return { ...i, state: "needs_you", ask: { item_id: request.id, sid: request.session_id, kind: request.kind, text: summary(request) } };
    return i.state === "needs_you" && i.ask?.item_id ? { ...i, state: "working", ask: null } : i;
  });
}
const deskWaitingItems = (d, taskApprovals) => deskItemsWithRequests(d, taskApprovals).filter(deskWaits);
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
const deskMoving = (i) => (i.state === "working" && !i.stopped) || deskFinishing(i);
/** Its holder, at work on it, has done nothing for a while (#285): past the time the hub gave (`act.stalls_at`, 0 while
 *  its helpers are at it). Waiting on you, stopped or waiting on something isn't stalled: those carry no `act`. */
const deskStalled = (i) => !!i.act && i.act.stalls_at > 0 && now() >= i.act.stalls_at;
/** "6 moving · 3 stalled" beside In progress (#285), so a count of items at work can't hide that none is moving. Only
 *  the items whose holder is at work on them count; "" when there are none. */
function deskSplitHtml(items) {
  const at = items.filter((i) => i.state === "working" && i.act);
  if (!at.length) return "";
  const stalled = at.filter(deskStalled).length;
  return `<span class="dk-split"><span class="n">${at.length - stalled}</span> moving · <span class="n">${stalled}</span> stalled</span>`;
}
/** A title cut short for a button or a line ("Search stays centred"): its first few words. */
const deskShort = (t) => { const w = String(t || "").split(/\s+/); return w.length > 3 ? w.slice(0, 3).join(" ") : t; };
