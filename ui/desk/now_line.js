// Cue UI desk — the line at the top of each Desk (#783): what Cue is doing with landed work, in that Desk's own landing
// steps (desk/hub/landing/now_line.rs sends it as `landing_now`): "Testing main at 1e01e83 · Rust tests · started
// 16:57", then "Building Cue", "Installing Cue", or "Failed: <test>" with what to know of it. Under it, the items
// that step carries, each opening its item. A Desk with no check and nothing after landing has none.
// styles/desk-c.css (.dkl). Loaded after desk/c.js (index.html, mini.html).

/** Desk `d`'s top line (html), or "" when it has none. */
function deskNowLineHtml(d) {
  const l = d?.landing_now;
  if (!l || !l.what) return "";
  const when = l.since ? { run: "started", wait: "since", fail: "ended", ok: "at" }[l.state] : "";
  const parts = [l.step ? `<span class="dkl-p${l.cmd ? " dkl-cmd" : ""}">${esc(l.step)}</span>` : "", when ? `<span class="dkl-p dkl-t">${when} ${esc(clockAt(l.since))}</span>` : ""].join("");
  const note = l.note ? `<span class="dkl-note">${esc(l.note)}</span>` : "";
  const also = l.also ? `<span class="dkl-note">also ${esc(l.also.charAt(0).toLowerCase() + l.also.slice(1))}</span>` : "";
  const items = (l.items || []).map((id) => {
    const it = deskItem(d.project, id);
    return `<button class="dkl-it" data-desk-item="${esc(id)}"><span class="dkl-id">#${esc(id)}</span>${it ? `<span class="dkl-tt">${esc(it.title)}</span>` : ""}</button>`;
  }).join("");
  return `<div class="dkl dkl-${esc(l.state)}" data-desk-now-line="${esc(l.state)}" role="status">`
    + `<div class="dkl-1"><i class="dkl-dot" aria-hidden="true"></i><b class="dkl-what">${esc(l.what)}</b>${parts}${note}${also}</div>`
    + (items ? `<div class="dkl-2">${items}</div>` : "")
    + `</div>`;
}
/** What item `i` waits on in its Desk's top line, when that line's step carries it and is under way: "after landing:
 *  make deploy", "its check: npm test" (html), else "". Its row says that instead of just finishing. */
function deskNowLineFor(i) {
  const l = deskOf(i?.project)?.landing_now;
  if (!l || l.state !== "run" || !(l.items || []).includes(i.id) || !l.step) return "";
  const step = l.cmd ? `<span class="dkl-cmd">${esc(l.step)}</span>` : esc(l.step);
  return /^After landing/.test(l.what) ? `On main · after landing: ${step}` : /^Checking/.test(l.what) ? `Checking before it lands: ${step}` : "";
}
