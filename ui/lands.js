"use strict";
/** How work lands (lands.rs): per project, where a Desk item works, how it lands, its check, what follows, push, and
 *  how many at once. Shown in a Desk's header ("Lands: own copy, checked, one commit per item · Change"); Change opens
 *  the setting. A Desk's `lands` (desk_view) has the setting now, its parts with each choice's words and whether it can
 *  be picked, and why when the folder decides it. Kept in Cue's own settings, never in the project. */
let lands = null;   // { desk, set: { where, how, check, after, push, at_once }, busy }

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
    <div class="ld-rows">${parts}</div>
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
    return renderMain();
  }
  const act = b.dataset.ld;
  const range = deskOf(lands.desk)?.lands?.at_once || {};
  if (act === "less") s.at_once = Math.max(range.min ?? 1, s.at_once - 1);
  else if (act === "more") s.at_once = Math.min(range.max ?? 9, s.at_once + 1);
  else if (act === "cancel") lands = null;
  else if (act === "save") return landsSave();
  renderMain();
}, true);
