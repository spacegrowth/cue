"use strict";
/** A project's settings (#381): how work lands (lands.rs: where a Desk item works, how it lands, its check, what
 *  follows, push, and how many at once), Ask me before, Away and old working folders, on one screen. Opened from a
 *  Desk header's Settings (#417); the Away switch still opens Away's own card at once. A Desk's `lands` (desk_view) has the setting now,
 *  its parts with each choice's words and whether it can be picked, and why when the folder decides it. Kept in Cue's
 *  own settings, never in the project. The lead can propose a change too (a LANDS: line): asked of you, saved on yes. */
let lands = null;   // { desk, set: { where, how, check, after, push, at_once, check_steps, after_steps }, steps, ab, busy, old }
// Remove old working folders (#397, hub/desk_copies.rs): looked at when you press it, listed with their size, removed
// on a second press. `old`: null, { looking }, { list: { copies, kb } }, or { removing }.

/** A Desk's one header line (#417): what it asks before and how its work lands, then Settings, which opens this
 *  screen ("Asks before release, delete · lands in own copy, checked · Settings"). Only on a Desk run by a lead. The
 *  words shorten with an ellipsis at narrow widths; Settings always shows. Why the folder decides a part is on the screen. */
function deskSettingsLine(d) {
  if (!d?.lead || !(d.lands || d.ask_before?.project)) return "";
  const open = d.lands ? `data-lands-change="${esc(d.id || d.project)}"` : `data-ab-change="${esc(d.ask_before.project)}"`;
  return `<span class="dk-ab"><button class="dk-ab-change" ${open}>Settings</button></span>`;
}
/** Open Desk `desk`'s settings. */
function landsChange(desk) {
  const d = deskOf(desk), l = d?.lands;
  if (!l) return;
  const project = d.ask_before?.project || l.project;
  // Your own steps, one per line while you edit them.
  const steps = { check: (l.set.check_steps || []).join("\n"), after: (l.set.after_steps || []).join("\n") };
  const ab = project && askBeforeKinds().length ? { project, ticks: [...askBeforeNow(project)], was: [...askBeforeNow(project)], more: false } : null;
  lands = { desk, set: { ...l.set }, steps, ab, busy: false };
  renderMain();
}
/** The Desk whose settings show Ask me before for `project`, if any. */
const landsDeskFor = (project) => (state.desk?.projects || []).find((d) => d.lead && d.lands && (d.ask_before?.project || d.lands.project) === project);
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
    ? `<ol class="ld-steps">${(l.steps || []).map((s) => `<li><b>${esc(s.label)}</b> <span>${esc(s.hint)}</span></li>`).join("")}</ol>`
    : (p.key === "check" || p.key === "after") && lands.set[p.key] === "command"
      ? `<textarea class="ld-cmd" rows="3" spellcheck="false" aria-label="${esc(p.label)}: your steps, one per line" placeholder="One step per line, e.g. npm test" data-ld-steps="${p.key}">${esc(lands.steps[p.key])}</textarea>` : "";
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
/** Ask me before, its ticks kept with the screen and saved with it (ask_before.js has the rows and their words). */
function landsAskBefore() {
  if (!lands.ab) return "";
  return `<div class="ld-sec" data-ld-sec="ask"><div class="ld-h">Ask me before</div><div class="ld-hint">The lead and its sessions stop and ask you first.</div>
    ${askBeforeRows(lands.ab)}</div>`;
}
/** Away: whether it's on, and its own dialog to turn it on or change it. */
function landsAway(d) {
  const a = deskAwayOf(d);
  return `<div class="ld-sec" data-ld-sec="away"><div class="ld-h">Away</div><div class="ld-away"><span class="grow">${a.on ? `On · ${esc(deskAwayStatus(a))}` : "Off: the lead waits for you on anything it can't settle."}</span>
    <button class="btn" data-ld="away">${a.on ? "Change…" : "Turn on…"}</button></div></div>`;
}
function landsHtml() {
  // Away's own dialog shows over the Desk; this screen comes back when it closes.
  if (!lands || (typeof deskAwayEdit !== "undefined" && deskAwayEdit)) return "";
  const d = deskOf(lands.desk), l = d?.lands;
  if (!l) { lands = null; return ""; }
  const name = typeof projLabel === "function" ? projLabel(l.project) : l.project;
  const parts = l.parts.map((p) => p.key === "how" ? landsAtOnce(l) + landsPart(p, l) : landsPart(p, l)).join("");
  return `<div class="move-scrim"><div class="move-dialog ld-dialog" role="dialog" aria-labelledby="ld-title">
    <div class="move-title" id="ld-title">Settings for ${esc(name)}</div>
    <div class="end-text">Kept in Cue, not in the project. You can also tell the lead what to change: it asks you first.</div>
    <div class="ld-sec" data-ld-sec="lands"><div class="ld-h">How work lands</div><div class="ld-hint">How an item's work gets onto main.</div>
    ${l.why ? `<div class="ld-why">${esc(l.why)}</div>` : ""}
    <div class="ld-rows">${parts}</div></div>
    ${landsAskBefore()}${landsAway(d)}
    <div class="ld-sec" data-ld-sec="old"><div class="ld-rows">${landsOld()}</div></div>
    <div class="row-btns"><span class="grow"></span><button class="btn" data-ld="cancel">Cancel <kbd>Esc</kbd></button><button class="btn primary" data-ld="save"${lands.busy ? " disabled" : ""}>Save</button></div>
  </div></div>`;
}
function landsSave() {
  const a = lands;
  if (!a || a.busy) return;
  a.busy = true;
  renderMain();
  // Your own steps go with a part only when that part is "Your own steps".
  const set = { ...a.set };
  delete set.check_steps;
  delete set.after_steps;
  for (const k of ["check", "after"]) if (set[k] === "command") set[`${k}_steps`] = (a.steps[k] || "").split("\n").map((x) => x.trim()).filter(Boolean);
  const ab = a.ab, kinds = ab && askBeforeKinds().map((x) => x.key).filter((x) => ab.ticks.includes(x));
  const abChanged = ab && [...kinds].sort().join() !== [...ab.was].sort().join();
  const failed = (e) => { a.busy = false; renderMain(); return e; };
  invoke("desk_lands_set", { desk: a.desk, lands: set }).then(
    () => {
      if (!abChanged) { if (lands === a) lands = null; return renderMain(); }
      // How work lands is saved now: what's left on the screen is Ask me before, as you ticked it.
      a.set = { ...set };
      invoke("ask_before_set", { project: ab.project, kinds }).then(
        () => { if (lands === a) lands = null; renderMain(); },
        (e) => toast(`How work lands is saved; Ask me before couldn't be saved: ${failed(e)}`, 6000),
      );
    },
    (e) => toast(`Nothing was saved. How work lands couldn't be saved: ${failed(e)}`, 6000),
  );
}
function landsEscape() {
  if (!lands || (typeof deskAwayEdit !== "undefined" && deskAwayEdit)) return false;
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
  else if (act === "away") return deskAwayOpen(deskOf(lands.desk)?.project || lands.desk);
  else if (act === "cancel") lands = null;
  else if (act === "save") return landsSave();
  renderMain();
}, true);
// Ask me before's rows and More on the screen (ask_before.js).
document.addEventListener("click", (e) => {
  if (lands?.ab && e.target.closest?.(".ld-dialog")) askBeforeClick(lands.ab, e);
}, true);
document.addEventListener("input", (e) => {
  const t = e.target.closest?.("[data-ld-steps]");
  if (t && lands) lands.steps[t.dataset.ldSteps] = t.value;
}, true);
