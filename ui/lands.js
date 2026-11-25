"use strict";
/** A project's settings (#381): how work lands (desk/lands.rs: where a Desk item works, how it lands, its check, what
 *  follows, push, and how many at once), Ask me before and old working folders, on one screen. Opened from the
 *  Desk header's Settings (#417). Changes wait on the screen until you press Save (#600), which applies them together;
 *  Cancel drops them, and leaving with some unsaved asks first. Away is on the Desk's own switch, not here. A Desk's `lands` (desk_view) has the setting now,
 *  its parts with each choice's words and whether it can be picked, and why when the folder decides it. Kept in Cue's
 *  own settings, never in the project. The lead can propose a change too (a LANDS: line): asked of you, saved on yes. */
let lands = null;   // { desk, set: { where, how, check, after, push, at_once, check_steps, after_steps }, steps, ab, was, leaving, busy, old }
// `was`: the settings as saved when the screen opened ({ set, steps, ticks }), to tell what you changed; `leaving`: you
// asked to leave with changes unsaved, and the screen asks first.
// Remove old working folders (#397, desk/hub/landing/copies.rs): looked at when you press it, listed with their size, removed
// on a second press. `old`: null, { looking }, { list: { copies, kb } }, or { removing }.

/** A Desk's one settings line (#417, #447), under its counts at the top left: what it asks before and how its work
 *  lands, then one "Change" that opens this screen. Only on a Desk run by a lead. */
function deskSettingsLine(d) {
  if (!d?.lead || !(d.lands || d.ask_before?.project)) return "";
  const l = d.lands, ab = d.ask_before;
  // Pushing to main is said once, by the push choice, when the Desk has one (#692).
  const push = l?.set?.push, skip = push ? ["main"] : [];
  const asks = ab?.project ? askBeforeWords(ab, skip) : "";
  const sends = LANDS_PUSH_WORDS[push] || "";
  const askWords = asks ? (asks === "nothing" ? (sends ? "" : "Asks nothing first") : `Asks before ${asks}`) : "";
  const words = [sends, askWords, l?.summary ? `lands in ${l.summary}` : ""].filter(Boolean).join(" · ");
  const open = l ? `data-lands-change="${esc(d.id || d.project)}"` : `data-ab-change="${esc(ab.project)}"`;
  const said = words ? `<span class="dk-ab-words">${esc(words[0].toUpperCase() + words.slice(1))}</span><span class="dk-ab-sep"> · </span>` : "";
  return `<span class="dk-ab">${said}<button class="dk-ab-change" ${open}>Change</button></span>`;
}
/** The push choice in a Desk's header line (#692). */
const LANDS_PUSH_WORDS = { after: "Sends to GitHub right away", ask: "Asks before sending to GitHub", never: "Doesn't send to GitHub" };
/** Open Desk `desk`'s settings, on tab `tab` (else the last one looked at). */
function landsChange(desk, tab) {
  const d = deskOf(desk), l = d?.lands;
  if (!l) return;
  const project = d.ask_before?.project || d.workspace_project || l.project;
  // Your own steps, one per line while you edit them.
  const steps = { check: (l.set.check_steps || []).join("\n"), after: (l.set.after_steps || []).join("\n") };
  // Pushing to main isn't an Ask me before row here: the push row says what happens (#692). Sessions never push main
  // themselves, so it stays asked; saving the screen keeps it so.
  const now = project && askBeforeKinds().length ? [...askBeforeNow(project)] : null;
  const ab = now ? { project, ticks: now.includes("main") ? now : [...now, "main"], more: false, stored: now } : null;
  // Where its new sessions start (#597): this Mac and the machines you ticked (#661, its new items spread over them),
  // each machine with the folder of the Desk's clone there and how many of its items may work there at once (#711).
  const ms = d.machines?.machines || [];
  const picked = (d.machines?.picked || [d.machines?.machine || ""]).filter((n) => !n || ms.some((m) => m.name === n));
  const mach = ms.length ? { list: picked.length ? picked : [""], folders: Object.fromEntries(ms.map((m) => [m.name, m.folder || ""])), at: Object.fromEntries(ms.map((m) => [m.name, m.at_once || 3])) } : null;
  const model = d.model || "default";
  lands = { desk, tab: tab || landsTabLast, edit: {}, set: { ...l.set }, steps, ab, mach, model, was: { set: { ...l.set }, steps: { ...steps }, ticks: ab ? [...ab.stored] : null, mach: mach ? { list: [...mach.list], folders: { ...mach.folders }, at: { ...mach.at } } : null, model } };
  renderMain();
}
/** How work lands as it's saved: your own steps go with a part only when that part is "Your own steps". */
function landsOut(set, steps) {
  const out = { ...set };
  delete out.check_steps;
  delete out.after_steps;
  for (const k of ["check", "after"]) if (out[k] === "command") out[`${k}_steps`] = (steps[k] || "").split("\n").map((x) => x.trim()).filter(Boolean);
  return out;
}
const landsSame = (x, y) => JSON.stringify(Object.keys({ ...x, ...y }).sort().map((k) => [k, x[k] ?? null])) === JSON.stringify(Object.keys({ ...x, ...y }).sort().map((k) => [k, y[k] ?? null]));
/** What you changed on the screen and haven't saved: how work lands, Ask me before. */
const landsSetChanged = (a) => !landsSame(landsOut(a.set, a.steps), landsOut(a.was.set, a.was.steps));
const landsAskChanged = (a) => !!a.ab && [...a.ab.ticks].sort().join() !== [...a.was.ticks].sort().join();
const landsModelChanged = (a) => (a.model || "default") !== (a.was.model || "default");
const landsMachChanged = (a) => !!a.mach && (a.mach.list.join("\n") !== a.was.mach.list.join("\n") || a.mach.list.some((n) => !!n && (a.mach.folders[n] || "").trim() !== (a.was.mach.folders[n] || "").trim()) || Object.keys(a.mach.at).some((n) => a.mach.at[n] !== a.was.mach.at[n]));
const landsChanged = (a) => !!a && (landsSetChanged(a) || landsAskChanged(a) || landsMachChanged(a) || landsModelChanged(a));
/** Where an item works changed while items are open: what happens to them, said before it's saved. An item already
 *  started keeps the folder it has (desk/hub/landing/trees.rs gives a copy only as an item starts working). */
function landsWhereNote() {
  const was = lands.was.set.where, now = lands.set.where;
  if (was === now) return "";
  const open = (deskOf(lands.desk)?.items || []).filter((i) => !["done", "dropped"].includes(i.state));
  if (!open.length) return "";
  const started = open.filter((i) => !["new", "queued"].includes(i.state)).length, waiting = open.length - started;
  const parts = [];
  if (started) parts.push(`${started} already started ${started === 1 ? "stays where it is and lands" : "stay where they are and land"} from there`);
  if (waiting) parts.push(`${waiting} not started yet ${now === "copy" ? (waiting === 1 ? "works in its own copy when it starts" : "work in their own copies when they start") : (waiting === 1 ? "works in the shared folder when it starts" : "work in the shared folder when they start")}`);
  return `<div class="ld-move" data-ld-move>${open.length} ${open.length === 1 ? "item is" : "items are"} open: ${parts.join("; ")}.</div>`;
}
/** The Desk whose settings show Ask me before for `project`, if any. */
const landsDeskFor = (project) => (state.desk?.projects || []).find((d) => d.lead && d.lands && (d.ask_before?.project || d.workspace_project || d.lands.project) === project);
/** Option A of #904: the screen's words, short as Settings' rows (lands.rs keeps the long ones for the lead). A part's
 *  name on its row, a choice's name where it's shorter, and one line of help for the choice picked. */
const LD_TITLES = { where: "Each item works in", how: "Lands on main as", check: "Check first", after: "Then runs", push: "Send to GitHub", tests: "Tests", waits: "Items waiting on another start" };
const LD_CHOICE = { how: { squash: "One commit" } };
const LD_HELP = {
  where: { shared: "The project's own folder. The Desk commits only the item's own files.", copy: "Its own copy of the project, cut from main when it starts.", branch: "Its own branch in the project's folder, one item at a time." },
  how: { squash: "One commit, named after the item. One at a time.", merge: "A merge commit, with the item's own commits kept under it. One at a time.", pr: "A pull request from the item's own branch. It lands once that's merged.", owner: "Nothing is committed. A finished item shows its files and waits for you." },
  check: { none: "Nothing is run. An item lands as not checked.", command: "Your steps run on main plus the item. One fails, nothing lands." },
  after: { nothing: "An item is done once it's on main.", command: "Your steps, in the main folder, once it's on main." },
  push: { after: "Each item, as soon as it lands. Nothing there is ever overwritten.", ask: "You're asked each time. Nothing there is ever overwritten.", never: "Nothing is sent. You push it yourself." },
  tests: { all: "Sessions run every test before they say done.", changed: "Sessions run the tests for what they changed.", changed_full: "The tests for what changed as they work, all of them before done." },
  waits: { landed: "An item queued after another starts once that one is on main.", checked: "An item queued after another starts once you've said that one looks right." },
};
/** The tabs, as Settings has them; the last one looked at opens next time. */
const LD_TABS = [["where", "Where it works"], ["done", "When it's done"], ["ask", "Ask me first"], ["more", "More"]];
let landsTabLast = "where";
/** A part's choices: those that can be picked, and those not built yet (shown, saying so). One the folder rules out
 *  isn't shown: the Desk's `why` says why. */
function landsSeg(p) {
  const shown = p.choices.filter((c) => c.ok || c.why === "not yet");
  return `<div class="seg ld-seg" role="radiogroup" aria-label="${esc(LD_TITLES[p.key] || p.label)}">${shown.map((c) => {
    const on = lands.set[p.key] === c.key, label = LD_CHOICE[p.key]?.[c.key] || c.label;
    return c.ok
      ? `<button role="radio" aria-checked="${on}" class="${on ? "on" : ""}" data-ld-part="${esc(p.key)}" data-ld-pick="${esc(c.key)}">${esc(label)}</button>`
      : `<button role="radio" aria-checked="false" disabled>${esc(label)} <span class="ld-not">${esc(c.why)}</span></button>`;
  }).join("")}</div>`;
}
/** One row as Settings' rows are: its name and a line of help on the left, its choice on the right. */
const landsRow = (title, help, control, extra = "", attrs = "") => `<div class="ld-row${extra ? " stack" : ""}"${attrs}><div class="ld-line"><div class="ld-text"><div class="ld-label">${esc(title)}</div>${help ? `<div class="ld-hint">${help}</div>` : ""}</div><div class="ld-ct">${control}</div></div>${extra}</div>`;
/** Your steps' names, for their one line: "[Build] app:build" is Build, a step with no name its command. */
const landsStepNames = (text) => (text || "").split("\n").map((x) => x.trim()).filter(Boolean).map((x) => x.match(/^\[([^\]]+)\]/)?.[1] || x);
function landsPart(p) {
  const now = p.choices.find((c) => c.key === lands.set[p.key]);
  const help = esc(LD_HELP[p.key]?.[now?.key] || "");
  let extra = "";
  if ((p.key === "check" || p.key === "after") && lands.set[p.key] === "command") {
    const names = landsStepNames(lands.steps[p.key]);
    // Your steps show as one line until you edit them; none yet opens the box at once.
    // Once open it stays open while the screen is, so a redraw as you type keeps the box and your caret.
    if (!names.length) lands.edit[p.key] = true;
    extra = lands.edit[p.key]
      ? `<textarea class="ld-cmd" rows="${landsRows(lands.steps[p.key])}" spellcheck="false" aria-label="${esc(p.label)}: your steps, one per line" placeholder="One step per line, e.g. npm test" data-ld-steps="${p.key}">${esc(lands.steps[p.key])}</textarea>${now?.hint ? `<div class="ld-hint ld-steps-help">${esc(now.hint)}</div>` : ""}`
      : `<div class="ld-steps"><span class="ld-steps-t"><b>${names.length} ${names.length === 1 ? "step" : "steps"}:</b> ${esc(names.join(" · "))}</span><span class="grow"></span><button class="ld-lk" data-ld-edit="${p.key}">Edit steps</button></div>`;
  }
  return landsRow(LD_TITLES[p.key] || p.label, help, landsSeg(p), extra + (p.key === "where" ? landsWhereNote() : ""), ` data-ld-key="${esc(p.key)}"`);
}
/** A steps box as tall as its steps (three lines while empty, five at most), so the screen stays short (#447). */
const landsRows = (text) => (text || "").trim() ? Math.min(5, text.split("\n").length) : 3;
/** At once: the most of this Desk's items at work on This Mac together, however they work there (#798: in the
 *  shared folder and on a branch too; in their own copies it's also the most copies). With machines it's theirs too
 *  (landsMach). */
function landsAtOnce(l) {
  if (lands.mach) return "";
  const n = lands.set.at_once, min = l.at_once?.min ?? 1, max = l.at_once?.max ?? 9;
  const more = `More than ${n === 1 ? "one waits" : `${n === 2 ? "two" : n === 3 ? "three" : n} wait`} ${lands.set.where === "copy" ? "for a free copy" : "in Up next and start by themselves as one finishes"}.`;
  return landsRow("At once", more, `<div class="ld-step">
    <button class="btn" data-ld="less" aria-label="Fewer"${n <= min ? " disabled" : ""}>−</button><b class="ld-n">${n}</b><button class="btn" data-ld="more" aria-label="More"${n >= max ? " disabled" : ""}>+</button></div>`, "", ` data-ld-sec="at-once"`);
}
/** The model a Desk's new sessions start on (#620). */
const landsModelRow = () => landsRow("New sessions start on", "The model, from Claude Code's list on this Mac.", landsModelPick(), "", ` data-ld-sec="model"`);
/** A size given in KB, as people say it. */
const landsKb = (kb) => (kb >= 1048576 ? `${(kb / 1048576).toFixed(1)} GB` : kb >= 1024 ? `${Math.round(kb / 1024)} MB` : `${kb || 0} KB`);
/** Remove old working folders: the folders of this project whose work is already on main and that nothing uses. */
function landsOld() {
  const o = lands.old, l = o?.list, n = l?.copies?.length || 0;
  const help = "Their work is already on main and nothing uses them. Listed before anything is removed.";
  let ct = "", extra = "";
  if (!o) ct = `<button class="btn" data-ld="old">Remove old working folders</button>`;
  else if (o.looking) ct = `<button class="btn" disabled>Looking…</button>`;
  else if (o.removing) ct = `<button class="btn" disabled>Removing…</button>`;
  else if (!n) extra = `<div class="ld-hint">None: every other folder has work not on main yet, or is in use.</div>`;
  else extra = `<div class="ld-old-list">${l.copies.map((c) => `<div>${esc(c.path)} · ${landsKb(c.kb)}</div>`).join("")}</div>
    <div class="ld-old-b"><button class="btn armed" data-ld="old-yes">Remove ${n} (${landsKb(l.kb)})</button><button class="btn" data-ld="old-no">Keep them</button></div>
    <div class="ld-hint">Any that gets new work or comes into use first stays.</div>`;
  return `<div class="ld-old" data-ld-sec="old">${landsRow("Old working folders", help, ct, extra)}</div>`;
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
/** The model a Desk's new sessions start on (#620): Claude Code's list on this Mac, Default its own pick. */
function landsModelPick() {
  const list = typeof modelsOf === "function" ? modelsOf("") : null;
  // Asked once per opening: a list that can't be read isn't asked for again on every draw.
  if (!list && !lands.modelsAsked && typeof readModels === "function") { lands.modelsAsked = true; readModels(false, ""); }
  const opts = [...(list || [{ value: "default", label: "Default" }])];
  if (!opts.some((m) => m.value === "default")) opts.unshift({ value: "default", label: "Default" });
  if (!opts.some((m) => m.value === lands.model)) opts.push({ value: lands.model, label: lands.model });
  return `<select class="nf-in nf-sel ld-model" data-ld-model aria-label="Model new sessions start on">${opts.map((m) => `<option value="${esc(m.value)}" ${m.value === lands.model ? "selected" : ""}>${esc(m.label || m.value)}</option>`).join("")}</select>`;
}
/** Where the Desk's new sessions start (#597): This Mac and your machines, as many as you tick (#661: its new items
 *  spread over them), and for each machine the folder of this repository's clone there and how many of the Desk's
 *  items may work there at once (#711). Only when Cue has a machine. */
function landsMach() {
  const m = lands.mach;
  if (!m) return "";
  const ms = deskOf(lands.desk)?.machines?.machines || [];
  const opt = (key, label) => { const on = m.list.includes(key); return `<button role="checkbox" aria-checked="${on}" class="${on ? "on" : ""}" data-ld-mach="${esc(key)}">${esc(label)}</button>`; };
  const seg = `<div class="seg ld-seg" role="group" aria-label="New sessions start on">${opt("", "This Mac")}${ms.map((x) => opt(x.name, x.label)).join("")}</div>`;
  const ons = ms.filter((x) => m.list.includes(x.name)), many = m.list.length > 1;
  const input = (x) => `<input class="ld-cmd ld-folder" type="text" spellcheck="false" aria-label="Folder of this repository's clone on ${esc(x.label)}" placeholder="Full path there, e.g. ~/code/project" data-ld-folder="${esc(x.name)}" value="${esc(m.folders[x.name] || "")}">`;
  const range = deskOf(lands.desk)?.machines?.at_once || {}, min = range.min ?? 1, max = range.max ?? 9;
  const n = (x) => { const k = m.at[x.name] || 3; return `<div class="ld-step ld-mach-n"><button class="btn" data-ld-mach-n="${esc(x.name)}" data-ld-by="-1" aria-label="Fewer at once on ${esc(x.label)}"${k <= min ? " disabled" : ""}>−</button><b class="ld-n">${k}</b><button class="btn" data-ld-mach-n="${esc(x.name)}" data-ld-by="1" aria-label="More at once on ${esc(x.label)}"${k >= max ? " disabled" : ""}>+</button><span class="ld-mach-at">at once</span></div>`; };
  const folders = ons.length > 1 ? ons.map((x) => `<div class="ld-folder-on"><span class="ld-folder-name">${esc(x.label)}</span>${input(x)}</div>`).join("") : ons.map(input).join("");
  // Every machine's number, ticked or not (#711): a per-item choice or the lead can start items on any of them.
  // This Mac's is the Desk's Items at once, however its items work there (#798).
  const l = deskOf(lands.desk)?.lands, k = lands.set.at_once, lo = l?.at_once?.min ?? 1, hi = l?.at_once?.max ?? 9;
  const mac = `<div class="ld-folder-on ld-mach-row"><span class="ld-mach-name">This Mac</span><div class="ld-step ld-mach-n"><button class="btn" data-ld="less" aria-label="Fewer at once on This Mac"${k <= lo ? " disabled" : ""}>−</button><b class="ld-n">${k}</b><button class="btn" data-ld="more" aria-label="More at once on This Mac"${k >= hi ? " disabled" : ""}>+</button><span class="ld-mach-at">at once</span></div></div>`;
  const at = landsRow("At once", `The most of this Desk's items working on each machine together${lands.set.where === "copy" ? " (on This Mac, each in a folder of its own until it lands)" : ""}. More ${many ? "go to another place you ticked that has room, or " : ""}wait until one there is done, then start by themselves.`, "", `<div class="ld-mach-list">${mac}${ms.map((x) => `<div class="ld-folder-on ld-mach-row"><span class="ld-mach-name">${esc(x.label)}</span>${n(x)}</div>`).join("")}</div>`, ` data-ld-sec="at-once"`);
  const where = ons.length ? `<div class="ld-hint">The folder of this repository's clone on ${ons.length > 1 ? "each machine" : esc(ons[0].label)}. If the project isn't there yet, Cue downloads it there first. Each new session starts there from the latest main, on a branch of its own, and its work lands here.</div>` : "";
  const spread = many ? `<div class="ld-hint">New items start on whichever of these has the fewest of this Desk's items at work. Each item says where it's working.</div>` : "";
  const folder = `${folders}${spread}${where || (many ? "" : `<div class="ld-hint">The lead asks you before a Desk's first item on another machine. You can also choose per item, on its page. Tick more than one to spread new items over them.</div>`)}`;
  // On Where it works, under where an item works (#711, #913).
  return `<div data-ld-sec="machine">${landsRow("Starts on", "Where this Desk's new sessions start.", "", `${seg}${folder}`)}${at}</div>`;
}
/** Ask me first: a switch per kind, as Settings' notifications are (ask_before.js has the kinds and their words).
 *  Pushing to main isn't one: Send to GitHub says what happens (#692). */
function landsAskBefore() {
  if (!lands.ab) return "";
  const kinds = askBeforeKinds().filter((k) => k.key !== "main"), offered = kinds.filter((k) => k.offered), unchecked = kinds.filter((k) => !k.offered);
  const row = (k) => { const on = lands.ab.ticks.includes(k.key); return landsRow(k.label, "", `<button class="toggle${on ? " on" : ""}" role="switch" aria-checked="${on}" aria-label="${esc(k.label)}" data-ab-kind="${esc(k.key)}"><span></span></button>`); };
  return `<div data-ld-sec="ask"><div class="ld-hint ld-tabnote">The lead and its sessions stop and ask you first.</div>${offered.map(row).join("")}
    ${unchecked.length ? `<div class="ld-hint ld-tabnote ld-unchecked">Not offered, as Cue can't check them: ${unchecked.map((k) => `${esc(k.label.toLowerCase())} (${esc(k.why)})`).join("; ")}. Sessions on another machine are told, not stopped.</div>` : ""}</div>`;
}
function landsHtml() {
  if (!lands) return "";
  const d = deskOf(lands.desk), l = d?.lands;
  if (!l) { lands = null; return ""; }
  // Named as the Desk is: its settings are its own (#460).
  const name = d.name || (typeof projLabel === "function" ? projLabel(l.project) : l.project);
  const part = (k) => { const p = l.parts.find((x) => x.key === k); return p ? landsPart(p) : ""; };
  const tabs = LD_TABS.filter(([k]) => k !== "ask" || lands.ab);
  const tab = tabs.some(([k]) => k === lands.tab) ? lands.tab : "where";
  const pane = {
    where: `${l.why ? `<div class="ld-why">${esc(l.why)}</div>` : ""}<div data-ld-sec="lands">${part("where")}</div>${landsMach()}${landsAtOnce(l)}${landsModelRow()}`,
    done: `<div data-ld-sec="lands">${part("check")}${part("how")}${part("after")}${part("push")}</div>`,
    ask: landsAskBefore(),
    more: `<div data-ld-sec="lands">${part("tests")}${part("waits")}</div>${landsOld()}`,
  }[tab];
  // As Settings is (sheets.js settingsSheet): a sheet, its title and Esc on one line, its tabs under it; a click
  // outside or Esc closes it (#447). Option A of #904 (#913).
  return `<div class="scrim ld-scrim" data-ld="scrim"><div class="sheet ld-dialog" role="dialog" aria-labelledby="ld-title">
    <div class="sheet-head"><h3 class="move-title" id="ld-title">Settings for ${esc(name)}</h3><span class="grow"></span><kbd class="ld-esc" data-ld="close" role="button" aria-label="Close">Esc</kbd></div>
    <div class="set-tabs seg ld-tabs" role="tablist">${tabs.map(([k, t]) => `<button role="tab" class="${tab === k ? "on" : ""}" aria-selected="${tab === k}" data-ld-tab="${k}">${t}</button>`).join("")}</div>
    <div class="ld-hint ld-intro">Kept in Cue, not in the project. Save keeps what you changed on every tab.</div>
    <div class="sheet-body" data-ld-pane="${tab}">${pane}</div>${landsFoot()}</div></div>`;
}
/** Save and Cancel under the screen; leaving with changes unsaved asks there first. */
function landsFoot() {
  const a = lands, changed = landsChanged(a);
  const save = `<button class="btn primary" data-ld="save"${a.busy || !changed ? " disabled" : ""}>${a.busy ? "Saving…" : "Save"}</button>`;
  if (a.leaving && changed) return `<div class="ld-foot ld-leaving" role="alertdialog" aria-label="Unsaved changes"><span class="ld-foot-t">Leave without saving your changes?</span><span class="grow"></span>
    <button class="btn" data-ld="stay">Keep editing</button><button class="btn deny" data-ld="discard">Discard</button>${save}</div>`;
  return `<div class="ld-foot"><span class="ld-foot-t" aria-live="polite">${changed ? "Unsaved changes" : ""}</span><span class="grow"></span>
    <button class="btn" data-ld="cancel">Cancel</button>${save}</div>`;
}
/** Leave the screen (Esc, its Esc, a click outside): at once with nothing changed, else it asks first. */
function landsLeave() {
  if (!lands) return;
  if (landsChanged(lands) && !lands.busy) lands.leaving = true;
  else if (!lands.busy) lands = null;
  renderMain();
}
/** What was just saved, per Desk: { lands, model, at }. Shown at once, and kept over any update Cue built before the
 *  save that comes in after it (an update and a command's answer come back on their own ways), until one has it. */
const landsHeld = new Map();
const LANDS_HELD_MS = 10000;
/** What was just saved on Desk `desk` (`part` "lands" or "model"), kept on it from now: the screen opened again
 *  shows it, not what was there before. */
function landsKeep(desk, part, value) {
  landsHeld.set(desk, { ...landsHeld.get(desk), [part]: value, at: Date.now() });
  landsHeldApply(state);
}
/** Each update (app.js setState): what was just saved stays on its Desk until the update has it too. */
function landsHeldApply(s) {
  for (const [desk, h] of landsHeld) {
    const d = (s.desk?.projects || []).find((x) => x.project === desk);
    if (!d || Date.now() - h.at > LANDS_HELD_MS) { landsHeld.delete(desk); continue; }
    if ("model" in h) { if ((d.model || "") === h.model) delete h.model; else d.model = h.model; }
    if ("lands" in h) { if (JSON.stringify(d.lands?.set) === JSON.stringify(h.lands.set)) delete h.lands; else d.lands = h.lands; }
    if (!("model" in h) && !("lands" in h)) landsHeld.delete(desk);
  }
}
/** Save: what you changed, applied together, then the screen closes. A part that couldn't be saved says so and
 *  stays on the screen, changed; one that was saved counts as saved. */
function landsSave() {
  const a = lands;
  if (!a || a.busy || !landsChanged(a)) return;
  const jobs = [];
  if (landsSetChanged(a)) {
    const set = landsOut(a.set, a.steps), steps = { ...a.steps };
    jobs.push(invoke("desk_lands_set", { desk: a.desk, lands: set }).then(
      (view) => { a.was.set = { ...a.set }; a.was.steps = steps; if (view?.set) landsKeep(a.desk, "lands", view); },
      (e) => { throw `how work lands: ${e}`; },
    ));
  }
  if (landsModelChanged(a)) {
    const model = a.model;
    jobs.push(invoke("desk_model_set", { desk: a.desk, model }).then(
      (saved) => { a.was.model = model; landsKeep(a.desk, "model", typeof saved === "string" ? saved : model === "default" ? "" : model); },
      (e) => { throw `the model: ${e}`; },
    ));
  }
  if (landsMachChanged(a)) {
    const mach = { list: [...a.mach.list], folders: { ...a.mach.folders }, at: { ...a.mach.at } };
    const machine_folders = Object.fromEntries(mach.list.filter(Boolean).map((n) => [n, (mach.folders[n] || "").trim()]));
    const machine_at_once = Object.fromEntries(Object.keys(mach.at).map((n) => [n, mach.at[n] || 3]));
    jobs.push(invoke("desk_lands_set", { desk: a.desk, lands: { machines: mach.list, machine_folders, machine_at_once } }).then(
      () => { a.was.mach = mach; },
      (e) => { throw `${e}`; },
    ));
  }
  if (landsAskChanged(a)) {
    const ticks = [...a.ab.ticks], kinds = askBeforeKinds().map((x) => x.key).filter((x) => ticks.includes(x));
    jobs.push(invoke("ask_before_set", { project: a.ab.project, kinds }).then(
      (kept) => {
        a.was.ticks = ticks;
        const ow = state.settings?.project?.oneway;
        if (ow && Array.isArray(kept)) ow.set = { ...ow.set, [a.ab.project]: kept };
      },
      (e) => { throw `Ask me before: ${e}`; },
    ));
  }
  a.busy = true;
  a.leaving = false;
  renderMain();
  Promise.allSettled(jobs).then((rs) => {
    a.busy = false;
    const bad = rs.filter((r) => r.status === "rejected").map((r) => r.reason);
    if (bad.length) toast(`Couldn't save ${bad.join("; ")}`, 6000);
    else if (lands === a) lands = null;
    renderMain();
  });
}
/** Esc: back to editing while it asks, else leave. */
function landsEscape() {
  if (!lands) return false;
  if (lands.leaving) { lands.leaving = false; renderMain(); }
  else landsLeave();
  return true;
}
document.addEventListener("click", (e) => {
  const c = e.target.closest?.("[data-lands-change]");
  if (!c) return;
  e.stopPropagation();
  landsChange(c.dataset.landsChange);
}, true);
document.addEventListener("click", (e) => {
  // How many at once on a machine (#711), in the range the Desk allows.
  const mn = e.target.closest?.("[data-ld-mach-n]");
  if (lands?.mach && mn) {
    e.stopPropagation();
    const r = deskOf(lands.desk)?.machines?.at_once || {}, k = mn.dataset.ldMachN;
    lands.mach.at[k] = Math.min(r.max ?? 9, Math.max(r.min ?? 1, (lands.mach.at[k] || 3) + Number(mn.dataset.ldBy)));
    lands.leaving = false;
    return renderMain();
  }
  const mp = e.target.closest?.("[data-ld-mach]");
  if (lands?.mach && mp) {
    e.stopPropagation();
    // Ticked or not (#661), kept in the screen's order; the last one ticked stays.
    const name = mp.dataset.ldMach, m = lands.mach, order = ["", ...(deskOf(lands.desk)?.machines?.machines || []).map((x) => x.name)];
    const list = m.list.includes(name) ? m.list.filter((n) => n !== name) : [...m.list, name];
    if (!list.length) return;
    m.list = order.filter((n) => list.includes(n));
    lands.leaving = false;
    return renderMain();
  }
  // A tab, kept for next time as Settings keeps its own; Edit steps opens that part's box.
  const tb = e.target.closest?.("[data-ld-tab]"), ed = e.target.closest?.("[data-ld-edit]");
  if (lands && (tb || ed)) {
    e.stopPropagation();
    if (tb) lands.tab = landsTabLast = tb.dataset.ldTab;
    else lands.edit[ed.dataset.ldEdit] = true;
    return renderMain();
  }
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
    // Merge keeps an item's own commits, so it needs a branch of its own: an own copy, or a branch.
    if (pick.dataset.ldPart === "how" && s.how === "merge" && s.where === "shared") s.where = "copy";
    if (pick.dataset.ldPart === "where" && s.where === "shared" && s.how === "merge") s.how = "squash";
    lands.leaving = false;
    return renderMain();
  }
  const act = b.dataset.ld;
  const range = deskOf(lands.desk)?.lands?.at_once || {};
  if (act === "less" || act === "more") {
    s.at_once = act === "less" ? Math.max(range.min ?? 1, s.at_once - 1) : Math.min(range.max ?? 9, s.at_once + 1);
    lands.leaving = false;
  }
  else if (act === "old") return landsOldLook();
  else if (act === "old-yes") return landsOldRemove();
  else if (act === "old-no") lands.old = null;
  else if (act === "close" || act === "scrim") return landsLeave();
  else if (act === "save") return landsSave();
  else if (act === "cancel" || act === "discard") { if (!lands.busy) lands = null; }
  else if (act === "stay") lands.leaving = false;
  renderMain();
}, true);
// Ask me before's rows and More on the screen (ask_before.js).
document.addEventListener("click", (e) => {
  if (!lands?.ab || !e.target.closest?.(".ld-dialog")) return;
  const was = lands.ab.ticks.join();
  if (askBeforeClick(lands.ab, e) && was !== lands.ab.ticks.join() && lands.leaving) { lands.leaving = false; renderMain(); }
}, true);
document.addEventListener("change", (e) => {
  const t = e.target.closest?.("[data-ld-model]");
  if (!t || !lands) return;
  lands.model = t.value || "default";
  lands.leaving = false;
  renderMain();
}, true);
document.addEventListener("input", (e) => {
  const f = e.target.closest?.("[data-ld-folder]");
  if (f && lands?.mach) {
    const was = landsChanged(lands);
    lands.mach.folders[f.dataset.ldFolder] = f.value;
    if (was !== landsChanged(lands)) { const foot = document.querySelector(".ld-dialog .ld-foot"); if (foot) foot.outerHTML = landsFoot(); }
    return;
  }
  const t = e.target.closest?.("[data-ld-steps]");
  if (!t || !lands) return;
  const was = landsChanged(lands);
  lands.steps[t.dataset.ldSteps] = t.value;
  // Grows with a new line as you type, without drawing the box again.
  t.rows = Math.max(t.rows, landsRows(t.value));
  // The foot says "Unsaved changes" and Save comes on; only it is drawn again, so the box keeps your caret.
  if (was !== landsChanged(lands)) { const f = document.querySelector(".ld-dialog .ld-foot"); if (f) f.outerHTML = landsFoot(); }
}, true);
