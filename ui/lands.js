"use strict";
/** A project's settings (#381): how work lands (lands.rs: where a Desk item works, how it lands, its check, what
 *  follows, push, and how many at once), Ask me before and old working folders, on one screen. Opened from the
 *  Desk header's Settings (#417). Each change is saved the moment you make it, with a short "Saved"; Away is on the
 *  Desk's own switch, not here. A Desk's `lands` (desk_view) has the setting now,
 *  its parts with each choice's words and whether it can be picked, and why when the folder decides it. Kept in Cue's
 *  own settings, never in the project. The lead can propose a change too (a LANDS: line): asked of you, saved on yes. */
let lands = null;   // { desk, set: { where, how, check, after, push, at_once, check_steps, after_steps }, steps, ab, busy, old }
// Remove old working folders (#397, hub/desk_copies.rs): looked at when you press it, listed with their size, removed
// on a second press. `old`: null, { looking }, { list: { copies, kb } }, or { removing }.

/** A Desk's one settings line (#417, #447), under its counts at the top left: what it asks before and how its work
 *  lands, then one "Change" that opens this screen. Only on a Desk run by a lead. */
function deskSettingsLine(d) {
  if (!d?.lead || !(d.lands || d.ask_before?.project)) return "";
  const l = d.lands, ab = d.ask_before;
  const asks = ab?.project ? askBeforeWords(ab) : "";
  const words = [asks ? (asks === "nothing" ? "Asks nothing first" : `Asks before ${asks}`) : "", l?.summary ? `lands in ${l.summary}` : ""].filter(Boolean).join(" · ");
  const open = l ? `data-lands-change="${esc(d.id || d.project)}"` : `data-ab-change="${esc(ab.project)}"`;
  const said = words ? `<span class="dk-ab-words">${esc(words[0].toUpperCase() + words.slice(1))}</span><span class="dk-ab-sep"> · </span>` : "";
  return `<span class="dk-ab">${said}<button class="dk-ab-change" ${open}>Change</button></span>`;
}
/** Open Desk `desk`'s settings. */
function landsChange(desk) {
  const d = deskOf(desk), l = d?.lands;
  if (!l) return;
  const project = d.ask_before?.project || l.project;
  // Your own steps, one per line while you edit them.
  const steps = { check: (l.set.check_steps || []).join("\n"), after: (l.set.after_steps || []).join("\n") };
  const ab = project && askBeforeKinds().length ? { project, ticks: [...askBeforeNow(project)], more: false } : null;
  lands = { desk, set: { ...l.set }, steps, ab };
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
    ? `<ol class="ld-steps">${(l.steps || []).map((s) => `<li><b>${esc(s.label)}</b></li>`).join("")}</ol>`
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
    <div class="ld-hint">Their work is already in the app and nothing uses them. Listed before anything is removed.</div>`;
  else if (o.looking) body = `<button class="btn" disabled>Looking…</button>`;
  else if (o.removing) body = `<button class="btn" disabled>Removing…</button>`;
  else if (!n) body = `<div class="ld-hint">None: every other folder has work not in the app yet, or is in use.</div>`;
  else body = `<div class="ld-old-list">${l.copies.map((c) => `<div>${esc(c.path)} · ${landsKb(c.kb)}</div>`).join("")}</div>
    <div class="ld-old-b"><button class="btn armed" data-ld="old-yes">Remove ${n} (${landsKb(l.kb)})</button><button class="btn" data-ld="old-no">Keep them</button></div>
    <div class="ld-hint">Any that gets new work or comes into use first stays.</div>`;
  return `<div class="ld-old"><div class="ld-body">${body}</div></div>`;
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
/** Ask me before, its kinds on one line, each saved as you tick it (ask_before.js has the rows and their words). */
function landsAskBefore() {
  if (!lands.ab) return "";
  return `<div class="ld-sec" data-ld-sec="ask"><div class="set-group ld-h">Ask me before</div>
    <div class="ld-hint ld-sub">The lead and its sessions stop and ask you first.</div>${askBeforeRows(lands.ab)}</div>`;
}
function landsHtml() {
  if (!lands) return "";
  const d = deskOf(lands.desk), l = d?.lands;
  if (!l) { lands = null; return ""; }
  const name = typeof projLabel === "function" ? projLabel(l.project) : l.project;
  const parts = l.parts.map((p) => p.key === "how" ? landsAtOnce(l) + landsPart(p, l) : landsPart(p, l)).join("");
  // As Settings is (sheets.js settingsSheet): a sheet, its title and Esc on one line, groups under it; a click
  // outside or Esc closes it (#447).
  return `<div class="scrim ld-scrim" data-ld="scrim"><div class="sheet ld-dialog" role="dialog" aria-labelledby="ld-title">
    <div class="sheet-head"><h3 class="move-title" id="ld-title">Settings for ${esc(name)}</h3><span class="ld-saved" aria-live="polite">${lands.saved ? "Saved" : ""}</span><span class="grow"></span><kbd class="ld-esc" data-ld="close" role="button" aria-label="Close">Esc</kbd></div>
    <div class="sheet-body">
    <div class="ld-hint ld-intro">Kept in Cue, not in the project. Each change saves at once; you can also tell the lead, and it asks you first.</div>
    <div class="ld-sec" data-ld-sec="lands"><div class="set-group ld-h">How work lands</div>
    ${l.why ? `<div class="ld-why">${esc(l.why)}</div>` : ""}
    <div class="ld-rows">${parts}</div></div>
    ${landsAskBefore()}
    <div class="ld-sec" data-ld-sec="old"><div class="set-group ld-h">Old working folders</div><div class="ld-rows">${landsOld()}</div></div>
  </div></div></div>`;
}
/** "Saved" for a moment after a change goes through. */
function landsSaved(a) {
  a.saved = true;
  clearTimeout(a.savedT);
  a.savedT = setTimeout(() => { a.saved = false; if (lands === a) renderMain(); }, 1600);
  if (lands === a) renderMain();
}
/** How work lands, saved as it is on the screen now: at once for a pick, a moment after the last + or −, and steps
 *  when you leave their box. Your own steps go with a part only when that part is "Your own steps". */
function landsSaveSet(wait = 0) {
  const a = lands;
  if (!a) return;
  clearTimeout(a.setT);
  a.setT = setTimeout(() => {
    const set = { ...a.set };
    delete set.check_steps;
    delete set.after_steps;
    for (const k of ["check", "after"]) if (set[k] === "command") set[`${k}_steps`] = (a.steps[k] || "").split("\n").map((x) => x.trim()).filter(Boolean);
    invoke("desk_lands_set", { desk: a.desk, lands: set }).then(
      () => landsSaved(a),
      (e) => toast(`Couldn't save how work lands: ${e}`, 6000),
    );
  }, wait);
}
/** Ask me before, saved as ticked; a tick that couldn't be saved goes back. */
function landsSaveAsk(was) {
  const a = lands, ab = a?.ab;
  if (!ab) return;
  const kinds = askBeforeKinds().map((x) => x.key).filter((x) => ab.ticks.includes(x));
  invoke("ask_before_set", { project: ab.project, kinds }).then(
    () => landsSaved(a),
    (e) => { ab.ticks = was; toast(`Couldn't save Ask me before: ${e}`, 6000); renderMain(); },
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
  const pick = e.target.closest?.("[data-ld-pick]"), b = e.target.closest?.("[data-ld]:not([data-ld=scrim])") || (e.target.dataset?.ld === "scrim" ? e.target : null);
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
    // Your own steps with none written yet waits for them (saved when you leave the box).
    if (s[pick.dataset.ldPart] !== "command") landsSaveSet();
    return renderMain();
  }
  const act = b.dataset.ld;
  const range = deskOf(lands.desk)?.lands?.at_once || {};
  if (act === "less" || act === "more") {
    s.at_once = act === "less" ? Math.max(range.min ?? 1, s.at_once - 1) : Math.min(range.max ?? 9, s.at_once + 1);
    landsSaveSet(500);
  }
  else if (act === "old") return landsOldLook();
  else if (act === "old-yes") return landsOldRemove();
  else if (act === "old-no") lands.old = null;
  else if (act === "close" || act === "scrim") lands = null;
  renderMain();
}, true);
// Ask me before's rows and More on the screen (ask_before.js).
document.addEventListener("click", (e) => {
  if (!lands?.ab || !e.target.closest?.(".ld-dialog")) return;
  const was = [...lands.ab.ticks];
  if (askBeforeClick(lands.ab, e) && was.join() !== lands.ab.ticks.join()) landsSaveAsk(was);
}, true);
document.addEventListener("input", (e) => {
  const t = e.target.closest?.("[data-ld-steps]");
  if (t && lands) lands.steps[t.dataset.ldSteps] = t.value;
}, true);
document.addEventListener("change", (e) => {
  if (lands && e.target.closest?.("[data-ld-steps]")) landsSaveSet();
}, true);
