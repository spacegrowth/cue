// The Inbox's part of the Desk (ui/desk.js): asks from a project's Dispatch, and asks from a session that holds a
// Desk item. Kept apart from ui/inbox.js, which loads first and calls these when it draws.

/** The Desk's projects (state.desk, from the hub); [] when there's none. */
const inboxDeskProjects = () => state.desk?.projects || [];
/** The Desk project entry whose Dispatch session `sid` is; null when it's none's (its name may be ""). */
const inboxDispatchOf = (sid) => inboxDeskProjects().find((p) => p.dispatch && p.dispatch === sid) || null;
/** A row from a Dispatch: its ask says so (`from_dispatch`), or its session is a project's Dispatch. */
const inboxFromDispatch = (r) => !!r.it?.from_dispatch || !!inboxDispatchOf(r.sid);
/** The project a Dispatch row is for; "" when none is named. */
const inboxDispatchProject = (r) => (r.it?.from_dispatch && r.it.project) || inboxDispatchOf(r.sid)?.project || r.project || "";
/** A Dispatch row's name: "<project> Dispatch". */
const inboxDispatchName = (r) => (inboxDispatchProject(r) ? `${inboxDispatchProject(r)} Dispatch` : "Dispatch");
/** The Desk item session `sid` holds (not done yet), or null. */
function inboxHeldItem(sid) {
  for (const p of inboxDeskProjects()) {
    const it = (p.items || []).find((x) => x.holder?.sid === sid && x.state !== "done");
    if (it) return { ...it, project: it.project || p.project };
  }
  return null;
}
/** A row's name: a Dispatch's dark D and "<project> Dispatch"; else the session's own. */
const inboxNameHtml = (r) => (inboxFromDispatch(r) ? `<span class="ib-dmark">D</span><span class="ib-name">${esc(inboxDispatchName(r))}</span>` : nameSpan(r.sid, r.project, "ib-name"));
/** "<PROJECT> · " before an item's title; "" for a project with no name. */
const inboxPjHtml = (project) => (project ? `<span class="ib-pj">${esc(project)}</span> · ` : "");
/** A third line on a row whose session holds a Desk item: "<PROJECT> · for <item title>". */
function inboxForHtml(r) {
  const it = inboxFromDispatch(r) ? null : inboxHeldItem(r.sid);
  return it ? `<span class="ib-for">${inboxPjHtml(it.project)}for <b>${esc(it.title)}</b></span>` : "";
}
/** A button that opens `project`'s Desk (on item `id`, when given). */
const inboxDeskBtn = (project, id, label) => `<button class="ib-act ib-desk" data-inbox-desk="${esc(project)}" data-item="${esc(id || "")}">${esc(label)}</button>`;
/** Over the brief of a session that holds a Desk item: what it's for, and Open in Desk (not for a project with no
 *  name: it has no Desk to open). */
function inboxForBar(r) {
  const it = inboxFromDispatch(r) ? null : inboxHeldItem(r.sid);
  return it ? `<div class="ib-forbar"><span class="ib-forw">${inboxPjHtml(it.project)}for <b>${esc(it.title)}</b></span><span class="grow"></span>${it.project ? inboxDeskBtn(it.project, it.id, "Open in Desk ›") : ""}</div>` : "";
}
/** The Desk on `project` (the header's project picked), item `itemId` open when given. Nothing but a note when this
 *  build has no Desk (ui/desk.js's deskOpenItem). */
function openInDesk(project, itemId = "") {
  if (typeof deskOpenItem !== "function") return toast("The Desk isn't in this build");
  if (project && projFilter !== project) setProjFilter(project);
  inboxSid = null;
  inboxSnoozeAt = "";
  view = "desk";
  if (itemId) deskOpenItem(itemId);
  renderMain();
}
