// The Inbox's part of the Desk (ui/desk/desk.js): an ask from a project's lead about its Desk (tagged "<PROJECT> · Desk"),
// an ask from a session that holds a Desk item ("<PROJECT> · for <item>"), Open in Desk, and Add to Desk on a session's
// ask. Kept apart from ui/inbox/, which loads first and calls these when it draws.

/** The Desk's projects (state.desk, from the hub); [] when there's none. */
const inboxDeskProjects = () => state.desk?.projects || [];
/** A required Desk verification belongs to the item, even when its session has ended. */
const inboxDeskVerifications = () => inboxDeskProjects().flatMap((p) =>
  inProj(deskWorkspace(p)) ? deskWaitingItems(deskOf(p.project)).filter(deskNeedsVerification) : []);

function inboxVerificationRows(items) {
  return items.map((i) => `<button type="button" class="ib-row" data-inbox-desk="${esc(i.project)}" data-item="${esc(i.id)}"><span class="ib-dot">${deskKindIcon("tray")}</span><span class="ib-l1"><span class="ib-name">${esc(deskName(deskOf(i.project)))}</span><span class="ib-subj">${esc(deskNum(i))} · ${esc(i.title)}</span></span><span class="ib-l2">${esc(deskOneLine(i.see))}</span></button>`).join("");
}
/** A row asking about a project's Desk: its lead's ask says so (`desk`). Listed with the other asks. */
const inboxDeskAsk = (r) => !!r.it?.desk;
/** The project a Desk ask is about; "" when none is named. */
const inboxDeskAskProject = (r) => (r.it?.desk && (r.it.tool_input?.desk_id || r.it.project || r.project)) || "";
/** The Desk item session `sid` holds (not done yet), or null. */
function inboxHeldItem(sid) {
  for (const p of inboxDeskProjects()) {
    const it = (p.items || []).find((x) => x.holder?.sid === sid && x.state !== "done" && x.state !== "dropped");
    if (it) return { ...it, project: it.project || p.project };
  }
  return null;
}
/** "<PROJECT> · " before an item's title; "" for a project with no name. */
const inboxPjHtml = (project) => {
  const d = inboxDeskProjects().find((d) => d.project === project);
  return project ? `<span class="ib-pj">${esc(d?.name || project)}</span> · ` : "";
};
/** What a row is for on the Desk: "<PROJECT> · Desk" (its lead asks about the Desk), "<PROJECT> · for <item title>"
 *  (it holds an item), or "". */
function inboxForWords(r) {
  if (inboxDeskAsk(r)) return `${inboxPjHtml(inboxDeskAskProject(r))}Desk`;
  const it = inboxHeldItem(r.sid);
  return it ? `${inboxPjHtml(it.project)}for <b>${esc(it.title)}</b>` : "";
}
/** A third line on a row, what it's for on the Desk. */
function inboxForHtml(r) {
  const w = inboxForWords(r);
  return w ? `<span class="ib-for">${w}</span>` : "";
}
/** A button that opens `project`'s Desk (on item `id`, when given). */
const inboxDeskBtn = (project, id, label) => `<button class="ib-act ib-desk" data-inbox-desk="${esc(project)}" data-item="${esc(id || "")}">${esc(label)}</button>`;
/** Over the brief: what it's for on the Desk and Open in Desk (not for a project with no name: it has no Desk). */
function inboxForBar(r) {
  const w = inboxForWords(r);
  if (!w) return "";
  const it = inboxDeskAsk(r) ? null : inboxHeldItem(r.sid), project = it ? it.project : inboxDeskAskProject(r);
  return `<div class="ib-forbar"><span class="ib-forw">${w}</span><span class="grow"></span>${project ? inboxDeskBtn(project, it?.id || "", "Open in Desk ›") : ""}</div>`;
}
/** Add to Desk, in an ask's head: a session's question (not the lead's about the Desk, not one already holding an
 *  item) becomes an item on its project's Desk, held by that session. */
function inboxAddDeskBtn(r) {
  const it = r.it;
  if (!it || it.kind === "waiting" || !r.project || !r.s || inboxDeskAsk(r) || inboxHeldItem(r.sid)) return "";
  const desks = inboxDeskProjects().filter((d) => (d.workspace_project || d.project) === r.project);
  const targets = desks.length ? desks : [{ project: r.project }];
  return targets.map((d) => `<button class="ib-act ib-desk" data-inbox-add-desk="${esc(r.sid)}" data-desk-id="${esc(d.project)}">Add to ${targets.length > 1 ? esc(d.name || d.project) : "Desk"}</button>`).join("");
}
/** Add session `sid`'s ask to its project's Desk: the ask's subject as the title, held by the session (nothing sent:
 *  it's already its work). */
async function inboxAddToDesk(sid, deskId) {
  const r = inboxRowFor(sid);
  if (!r?.project) return;
  const title = (inboxLines(r)[0] || "").trim() || "Its question";
  try {
    await invoke("desk_add", { project: deskId || r.project, kind: "task", title, holder: sid });
    toast(`Added to ${inboxDeskProjects().find((d) => d.project === deskId)?.name || r.project}'s Desk`);
  } catch (e) { toast(`Not added: ${e}`); }
  renderMain();
}
/** The Desk on `project` (the header's project picked), item `itemId` open when given. Nothing but a note when this
 *  build has no Desk (ui/desk/desk.js's deskOpenItem). */
function openInDesk(project, itemId = "") {
  if (typeof deskOpenItem !== "function") return toast("The Desk isn't in this build");
  if (project) deskSelect(project);
  inboxSid = null;
  inboxSnoozeAt = "";
  view = "desk";
  if (itemId) deskOpenItem(itemId);
  renderMain();
}
