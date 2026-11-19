"use strict";
/** How work lands (lands.rs): per project, where a Desk item works, how it lands, its check, what follows, push, and
 *  how many at once. Shown in a Desk's header ("Lands: own copy, checked, one commit per item · Change"); Change opens
 *  the setting. A Desk's `lands` (desk_view) has the setting now, its parts with each choice's words and whether it can
 *  be picked, and why when the folder decides it. Kept in Cue's own settings, never in the project. */
let lands = null;   // { desk, set: { where, how, check, after, push, at_once }, busy, old }
// Remove old working folders (#397, hub/desk_copies.rs): looked at when you press it, listed with their size, removed
// on a second press. `old`: null, { looking }, { list: { copies, kb } }, or { removing }.

/** A Desk's header line. Only on a Desk run by a lead, like Ask me before. */
function landsLine(d) {
  const l = d?.lands;
  if (!d?.lead || !l?.summary) return "";
  const why = l.why ? `<span class="dk-ab dk-lands-why">${esc(l.why)}</span>` : "";
  return `<span class="dk-ab">Lands: ${esc(l.summary)} · <button class="dk-ab-change" data-lands-change="${esc(d.id || d.project)}">Change</button></span>${why}`;
}
function landsChange(desk) {
  const l = deskOf(desk)?.lands;
  if (!l) return;
  lands = { desk, set: { ...l.set }, busy: false };
  renderMain();
}
/** A part's choices: those that can be picked, and those not built yet (shown, saying so). One the folder rules out
 *  isn't shown: the Desk's `why` says why. */
function landsSeg(p) {
  const shown = p.choices.filter((c) => c.ok || c.why === "not yet");
  return `<div class="seg ld-seg" role="radiogroup" aria-label="${esc(p.label)}">${shown.map((c) => {
    const on = lands.set[p.key] === c.key;
    return c.ok
      ? `<button role="radio" aria-checked="${on}" class="${on ? "on" : ""}" data-ld-part="${esc(p.key)}" data-ld-pick="${esc(c.key)}">${esc(c.label)}</button>`
      : `<button role="radio" aria-checked="false" disabled>${esc(c.label)} <span class="ld-not">${esc(c.why)}</span></button>`;
  }).join("")}</div>`;
}
function landsPart(p, l) {
  const now = p.choices.find((c) => c.key === lands.set[p.key]);
  const steps = p.key === "check" && lands.set.check === "cue"
    ? `<ol class="ld-steps">${(l.steps || []).map((s) => `<li><b>${esc(s.label)}</b> <span>${esc(s.hint)}</span></li>`).join("")}</ol>` : "";
  return `<div class="ld-row"><div class="ld-label">${esc(p.label)}</div><div class="ld-body">${landsSeg(p)}${steps}${now?.hint ? `<div class="ld-hint">${esc(now.hint)}</div>` : ""}</div></div>`;
}
function landsAtOnce(l) {
  if (lands.set.where !== "copy") return "";
  const n = lands.set.at_once, min = l.at_once?.min ?? 1, max = l.at_once?.max ?? 9;
  return `<div class="ld-row"><div class="ld-label">Items at once</div><div class="ld-body"><div class="ld-step">
    <button class="btn" data-ld="less" aria-label="Fewer"${n <= min ? " disabled" : ""}>−</button><b class="ld-n">${n}</b><button class="btn" data-ld="more" aria-label="More"${n >= max ? " disabled" : ""}>+</button></div>
    <div class="ld-hint">More than ${n === 1 ? "one waits" : `${n === 2 ? "two" : n === 3 ? "three" : n} wait`} for a free copy.</div></div></div>`;
}
/** A size given in KB, as people say it. */
const landsKb = (kb) => (kb >= 1048576 ? `${(kb / 1048576).toFixed(1)} GB` : kb >= 1024 ? `${Math.round(kb / 1024)} MB` : `${kb || 0} KB`);
/** Remove old working folders: the folders of this project whose work is already in the app and that nothing uses. */
function landsOld() {
  const o = lands.old, l = o?.list, n = l?.copies?.length || 0;
  let body;
  if (!o) body = `<button class="btn" data-ld="old">Remove old working folders</button>
    <div class="ld-hint">Folders whose work is already in the app, and that nothing is using. They're listed first.</div>`;
  else if (o.looking) body = `<button class="btn" disabled>Looking…</button>`;
  else if (o.removing) body = `<button class="btn" disabled>Removing…</button>`;
  else if (!n) body = `<div class="ld-hint">None: every other folder has work not in the app yet, or is in use.</div>`;
  else body = `<div class="ld-old-list">${l.copies.map((c) => `<div>${esc(c.path)} · ${landsKb(c.kb)}</div>`).join("")}</div>
    <div class="ld-old-b"><button class="btn armed" data-ld="old-yes">Remove ${n} (${landsKb(l.kb)})</button><button class="btn" data-ld="old-no">Keep them</button></div>
    <div class="ld-hint">Any that gets new work or comes into use first stays.</div>`;
  return `<div class="ld-row ld-old"><div class="ld-label">Old working folders</div><div class="ld-body">${body}</div></div>`;
}
/** Remove old working folders pressed: looked at now. */
function landsOldLook() {
  const a = lands;
  a.old = { looking: true };
  renderMain();
  invoke("desk_old_copies_list", { desk: a.desk }).then(
    (list) => { if (lands === a) { a.old = { list }; renderMain(); } },
    (e) => { a.old = null; toast(`Couldn't look for old working folders: ${e}`, 5000); renderMain(); },
  );
}
/** Remove N pressed: each is looked at again, then removed. */
function landsOldRemove() {
  const a = lands;
  a.old = { removing: true };
  renderMain();
  invoke("desk_old_copies", { desk: a.desk, yes: true }).then(
    (r) => {
      const kept = r.kept?.length ? ` · ${r.kept.length} kept: ${r.kept[0].why}` : "";
      toast(`Removed ${r.removed} old working ${r.removed === 1 ? "folder" : "folders"} · ${landsKb(r.kb)} freed${kept}`, 8000);
      if (lands === a) a.old = null;
      renderMain();
    },
    (e) => { a.old = null; toast(`Couldn't remove the old working folders: ${e}`, 5000); renderMain(); },
  );
}
function landsHtml() {
  if (!lands) return "";
  const d = deskOf(lands.desk), l = d?.lands;
  if (!l) { lands = null; return ""; }
  const name = typeof projLabel === "function" ? projLabel(l.project) : l.project;
  const parts = l.parts.map((p) => p.key === "how" ? landsAtOnce(l) + landsPart(p, l) : landsPart(p, l)).join("");
  return `<div class="move-scrim"><div class="move-dialog ld-dialog" role="dialog" aria-labelledby="ld-title">
    <div class="move-title" id="ld-title">How work lands</div>
    <div class="end-text">How an item's work gets onto main in ${esc(name)}. Kept in Cue, not in the project.</div>
    ${l.why ? `<div class="ld-why">${esc(l.why)}</div>` : ""}
    <div class="ld-rows">${parts}${landsOld()}</div>
    <div class="row-btns"><span class="grow"></span><button class="btn" data-ld="cancel">Cancel <kbd>Esc</kbd></button><button class="btn primary" data-ld="save"${lands.busy ? " disabled" : ""}>Save</button></div>
  </div></div>`;
}
function landsSave() {
  const a = lands;
  if (!a || a.busy) return;
  a.busy = true;
  renderMain();
  invoke("desk_lands_set", { desk: a.desk, lands: a.set }).then(
    () => { if (lands === a) lands = null; renderMain(); },
    (e) => { a.busy = false; toast(`Couldn't save how work lands: ${e}`); renderMain(); },
  );
}
function landsEscape() {
  if (!lands) return false;
  lands = null;
  renderMain();
  return true;
}
document.addEventListener("click", (e) => {
  const c = e.target.closest?.("[data-lands-change]");
  if (!c) return;
  e.stopPropagation();
  landsChange(c.dataset.landsChange);
}, true);
document.addEventListener("click", (e) => {
  const pick = e.target.closest?.("[data-ld-pick]"), b = e.target.closest?.("[data-ld]");
  if (!lands || (!pick && !b)) return;
  e.stopPropagation();
  const s = lands.set;
  if (pick) {
    s[pick.dataset.ldPart] = pick.dataset.ldPick;
    // You commit works in the shared folder; an own copy is landed by the Desk.
    if (pick.dataset.ldPart === "how" && s.how === "owner") s.where = "shared";
    if (pick.dataset.ldPart === "where" && s.where !== "shared" && s.how === "owner") s.how = "squash";
    // A pull request is opened from an item's own copy, so it has none in the shared folder.
    if (pick.dataset.ldPart === "how" && s.how === "pr") s.where = "copy";
    if (pick.dataset.ldPart === "where" && s.where === "shared" && s.how === "pr") s.how = "squash";
    return renderMain();
  }
  const act = b.dataset.ld;
  const range = deskOf(lands.desk)?.lands?.at_once || {};
  if (act === "less") s.at_once = Math.max(range.min ?? 1, s.at_once - 1);
  else if (act === "more") s.at_once = Math.min(range.max ?? 9, s.at_once + 1);
  else if (act === "old") return landsOldLook();
  else if (act === "old-yes") return landsOldRemove();
  else if (act === "old-no") lands.old = null;
  else if (act === "cancel") lands = null;
  else if (act === "save") return landsSave();
  renderMain();
}, true);
