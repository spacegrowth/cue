// Cue UI app — one listener per event on a root, handing the event to an ordered list of rules: the first rule that
// takes it ends the walk, as one long if/return chain would. A rule is a function (target, event) → true when it took
// the event. Order matters where one element sits inside another's area (a button inside a card).

/** A rule for the element nearest the target that matches `sel` (itself or an ancestor): `run(el, event)` takes
 *  it, unless `when(el, target)` says no, and then the walk goes on. */
const rule = (sel, run, when) => (t, e) => {
  const el = t.closest(sel);
  if (!el || (when && !when(el, t))) return false;
  run(el, e);
  return true;
};

/** `rules` walked in order for every `type` event on `root`. */
function delegate(root, type, rules) {
  root.addEventListener(type, (e) => { for (const r of rules) if (r(e.target, e)) return; });
}
