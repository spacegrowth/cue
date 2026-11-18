"use strict";
/** Ask me before (one_way.rs, hub/guard.rs): per project, the steps its Desk's lead and sessions stop and ask you
 *  first about. Asked once, as a project's Desk gets a lead (a Desk's `ask_before`: its words, whether it was asked,
 *  what the question ticks, any One-way setting carried over); changed later from where it's shown. The kinds and
 *  their words come from Settings' `project.oneway` (one_way.rs view): only those Cue checks are offered. */
let askBefore = null;              // { project, ticks: [keys], more, change, busy }
const askBeforeSkip = new Set();   // projects answered here, until Cue's next update says so

const askBeforeKinds = () => state.settings?.project?.oneway?.kinds || [];
/** What `project` has now: what you set, else the default. */
function askBeforeNow(project) {
  const ow = state.settings?.project?.oneway || {};
  return Array.isArray(ow.set?.[project]) ? ow.set[project] : ow.default || ["main"];
}
/** A Desk with a lead whose project hasn't been asked: the question, once. */
function askBeforeDue() {
  if (askBefore || !askBeforeKinds().length) return;
  const d = (state.desk?.projects || []).find((x) => x.lead && x.ask_before && !x.ask_before.asked && x.ask_before.project && !askBeforeSkip.has(x.ask_before.project));
  if (d) askBefore = { project: d.ask_before.project, ticks: [...(d.ask_before.ticks || [])], more: false, change: false, busy: false };
}
/** Change it for `project` (a Desk's header, or a lead's page on a project with no Desk). */
function askBeforeChange(project) {
  if (!project) return;
  askBefore = { project, ticks: [...askBeforeNow(project)], more: false, change: true, busy: false };
  renderMain();
}
/** The short words for each kind, in a Desk's header line. */
const AB_SHORT = { main: "push to main", push: "any push", release: "release", deploy: "deploy", data: "delete", outside: "outside files" };
/** A Desk's header line (its `ask_before`, from desk_view): "Asks before: push to main, release, delete · Change".
 *  Only on a Desk run by a lead; Desks on one project share it. */
function askBeforeLine(d) {
  const ab = d?.ask_before;
  if (!d?.lead || !ab?.project) return "";
  const words = (ab.kinds || []).map((k) => AB_SHORT[k.key] || k.label.toLowerCase()).join(", ") || "nothing";
  return `<span class="dk-ab">Asks before: ${esc(words)} · <button class="dk-ab-change" data-ab-change="${esc(ab.project)}">Change</button></span>`;
}
const AB_FIRST = ["main", "release", "data"];
const AB_CHECK = `<svg viewBox="0 0 12 12" width="11" height="11" aria-hidden="true"><path d="M2.5 6.3l2.3 2.3 4.7-5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>`;
function askBeforeRow(k) {
  const on = askBefore.ticks.includes(k.key);
  return `<button class="ab-row" role="checkbox" aria-checked="${on}" data-ab-kind="${esc(k.key)}"><span class="ab-box${on ? " on" : ""}">${on ? AB_CHECK : ""}</span>${esc(k.label)}</button>`;
}
function askBeforeHtml() {
  askBeforeDue();
  if (!askBefore) return "";
  const kinds = askBeforeKinds(), offered = kinds.filter((k) => k.offered);
  // The three first; another one already ticked (carried over) shows with them.
  const first = offered.filter((k) => AB_FIRST.includes(k.key) || askBefore.ticks.includes(k.key));
  const rest = offered.filter((k) => !first.includes(k));
  const unchecked = kinds.filter((k) => !k.offered);
  const moreLine = [...rest, ...unchecked].map((k) => k.label.toLowerCase()).join(", ");
  const more = !moreLine ? "" : !askBefore.more
    ? `<button class="ab-more" data-ab="more">More: ${esc(moreLine)}</button>`
    : `${rest.map(askBeforeRow).join("")}${unchecked.length ? `<div class="ab-note">Not offered, as Cue can't check them: ${unchecked.map((k) => `${esc(k.label.toLowerCase())} (${esc(k.why)})`).join("; ")}. Sessions on another machine are told, not stopped.</div>` : ""}`;
  const name = typeof projLabel === "function" ? projLabel(askBefore.project) : askBefore.project;
  return `<div class="move-scrim"><div class="move-dialog ab-dialog" role="dialog" aria-labelledby="ab-title">
    <div class="move-title" id="ab-title">Ask me before</div>
    <div class="end-text">The lead and its sessions stop and ask you first. In ${esc(name)}.</div>
    <div class="ab-rows">${first.map(askBeforeRow).join("")}${more}</div>
    <div class="ab-note">Cue reads each command's own words, so a push, tag or delete run inside a script isn't seen, nor a move that throws a file away (mv x /dev/null). A tag pushed by a name that isn't a version (v2-ui-fixes) counts as a plain push. Deleting a branch with git branch -D (or -d --force) counts as deleting.</div>
    <div class="row-btns"><span class="grow"></span><button class="btn" data-ab="${askBefore.change ? "cancel" : "later"}">${askBefore.change ? "Cancel" : "Not now"} <kbd>Esc</kbd></button><button class="btn primary" data-ab="save"${askBefore.busy ? " disabled" : ""}>Save</button></div>
  </div></div>`;
}
/** Your answer: `kinds` ticked, or null for Not now (asked, the setting kept). */
function askBeforeAnswer(kinds) {
  const a = askBefore;
  if (!a || a.busy) return;
  if (!a.change) askBeforeSkip.add(a.project);
  a.busy = true;
  renderMain();
  invoke("ask_before_set", { project: a.project, kinds }).then(
    () => { if (askBefore === a) askBefore = null; renderMain(); },
    (e) => { a.busy = false; toast(`Couldn't save Ask me before: ${e}`); renderMain(); },
  );
}
/** Esc: Not now for the question, Cancel for a change. */
function askBeforeEscape() {
  if (!askBefore) return false;
  if (askBefore.change) { askBefore = null; renderMain(); }
  else askBeforeAnswer(null);
  return true;
}
document.addEventListener("click", (e) => {
  const c = e.target.closest?.("[data-ab-change]");
  if (!c) return;
  e.stopPropagation();
  askBeforeChange(c.dataset.abChange);
}, true);
document.addEventListener("click", (e) => {
  const k = e.target.closest?.("[data-ab-kind]"), b = e.target.closest?.("[data-ab]");
  if (!askBefore || (!k && !b)) return;
  e.stopPropagation();
  if (k) {
    const key = k.dataset.abKind;
    askBefore.ticks = askBefore.ticks.includes(key) ? askBefore.ticks.filter((x) => x !== key) : [...askBefore.ticks, key];
    return renderMain();
  }
  const act = b.dataset.ab;
  if (act === "more") { askBefore.more = true; return renderMain(); }
  if (act === "cancel") { askBefore = null; return renderMain(); }
  if (act === "later") return askBeforeAnswer(null);
  if (act === "save") return askBeforeAnswer(askBeforeKinds().map((x) => x.key).filter((x) => askBefore.ticks.includes(x)));
}, true);
