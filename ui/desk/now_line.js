// Cue UI desk — what Cue is doing with landed work, in a Desk's own landing steps (#783; desk/hub/landing/now_line.rs
// sends it as `landing_now`). No line over the columns says it (taken out: that space stays empty); an item's row
// says the step it waits on instead (desk/desk.js). Loaded after desk/c.js (index.html, mini.html).

/** What item `i` waits on in its Desk's landing steps, when the step under way carries it: "after landing:
 *  make deploy", "its check: npm test", "being tested · Rust tests" (html), else "". Its row says that instead of just finishing. */
function deskNowLineFor(i) {
  const l = deskOf(i?.project)?.landing_now;
  if (!l || l.state !== "run" || !(l.items || []).includes(i.id) || !l.step) return "";
  const step = l.cmd ? `<span class="dkl-cmd">${esc(l.step)}</span>` : esc(l.step);
  // Cue's full run of main carrying it: its step ("On main · being tested · Rust tests").
  if (/^Testing main/.test(l.what)) return `On main · being tested · ${step}`;
  return /^After landing/.test(l.what) ? `On main · after landing: ${step}` : /^Checking/.test(l.what) ? `Checking before it lands: ${step}` : "";
}
