// Cue UI core: projects (a folder's repository), pins, the one-project filter, and the window's one Color.
// ---------- a project: the git repository a folder is in (every worktree of it too), else the folder ----------
/** A folder's project: its repository's name, as Cue read it ("cue" for any worktree of cue), else the folder's name. */
const projOf = (cwd) => state.projects?.[cwd]?.name || baseName(cwd);
/** Where + New starts for a project shown with this folder: its repository's main checkout, else the folder. */
const projRoot = (cwd) => state.projects?.[cwd]?.root || cwd;
// ---------- the window's Color (Settings → General → Color): one for all of Cue, every project alike ----------
/** The ten swatches, in the order offered: Moss (Cue's own look, saved as 0), then around the color wheel,
 *  graphite last: [saved as, hue, strength (0: grey, 1: as colorful as Moss)], from the colors picked
 *  (styles/base.css --pcpN, in saved order: Moss #7a7a66 #9b8eb4 #a8925e #b7a38f #6d8375 #5f8b8a #6b739c #a3828f
 *  #6e7076). Nothing between rose and taupe (red and amber mean "asks you" and "stuck"). The window takes
 *  the hue and strength, at Moss's own lightness (projTheme); Moss is Cue's own theme. Projects have no color
 *  of their own (#848): a project.colors or appearance.project left in settings.json is never read. */
const MOSS = 0;
const SWATCHES = [[MOSS], [4, 67, 0.37], [3, 87, 0.75], [1, 107, 0.3], [5, 157, 0.33], [6, 194, 0.48], [7, 276, 0.64], [2, 301, 0.57], [8, 353, 0.44], [9, 271, 0.1]];
const swatchOk = (c) => Number.isInteger(c) && c >= 0 && c < SWATCHES.length;
/** Your own Color, past the ten (Settings → General → Color): any hue and strength, saved as
 *  project.hue { h, k }. project.all keeps the nearest of the ten, for anything that knows only those. */
const OWN = 10;
const ownHue = () => {
  const v = state.settings?.project?.hue;
  if (hueLive && v?.h === hueLive.h && v?.k === hueLive.k) hueLive = null;   // Cue has it now
  const o = hueLive || v;
  return o && Number.isFinite(o.h) && Number.isFinite(o.k) ? o : null;
};
/** Your own Color just picked, until Cue's update has it: a snapshot meanwhile mustn't put the old one back. */
let hueLive = null;
/** Your own Color's chroma at full strength (k = 1): a picked color as vivid or more is k 1, grey is 0. */
const OWN_FULL = 0.12;
/** The window's Color: Moss until you pick another (one of the ten, or your own). */
const allIdx = () => { if (ownHue()) return OWN; const c = state.settings?.project?.all; return swatchOk(c) ? c : MOSS; };
/** The one of the ten nearest a hue and strength (Moss, which has no hue, never). */
function nearestSwatch(h, k) {
  const far = ([, sh, sk]) => Math.min(Math.abs(h - sh), 360 - Math.abs(h - sh)) / 180 + Math.abs(k - sk);
  return SWATCHES.slice(1).reduce((a, b) => far(b) < far(a) ? b : a)[0];
}
/** Your own Color, shown now (picking) or saved too (done): with it, the nearest of the ten. */
function setOwnHue(h, k, save = true) {
  const p = ((state.settings ||= {}).project ||= {});   // shown now; Cue's next update says the same
  p.hue = hueLive = { h, k };
  projTheme();
  retheme();
  if (!save) return;
  p.all = nearestSwatch(h, k);
  setSetting("project.hue", p.hue);
  setSetting("project.all", p.all);
}
/** Under the ten: your own Color (a rainbow until you pick one); pressed, the Mac's color picker. */
const ownRow = () => `<div class="pc-own"><button class="pc-sw own ${ownHue() ? "on" : ""}" style="--c:var(--pc${OWN})" data-pcown aria-label="Your own color"></button><button class="pc-own-l" data-pcown>Your own</button></div>`;
/** sRGB "#rrggbb" ⇄ OKLCH, for the color picker: what you pick gives the hue and strength, Cue keeps its own lightness. */
function hexToOwn(hex) {
  const lin = [1, 3, 5].map((i) => { const c = parseInt(hex.slice(i, i + 2), 16) / 255; return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4; });
  const [l, m, s] = [[0.4122214708, 0.5363325363, 0.0514459929], [0.2119034982, 0.6806995451, 0.1073969566], [0.0883024619, 0.2817188376, 0.6299787005]].map((r) => Math.cbrt(r[0] * lin[0] + r[1] * lin[1] + r[2] * lin[2]));
  const a = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s, b = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  return { h: Math.round(((Math.atan2(b, a) * 180) / Math.PI + 360) % 360), k: Math.round(Math.min(1, Math.hypot(a, b) / OWN_FULL) * 100) / 100 };
}
function ownToHex({ h, k }) {
  const c = OWN_FULL * k, a = c * Math.cos((h * Math.PI) / 180), b = c * Math.sin((h * Math.PI) / 180), L = 0.66;
  const [l, m, s] = [L + 0.3963377774 * a + 0.2158037573 * b, L - 0.1055613458 * a - 0.0638541728 * b, L - 0.0894841775 * a - 1.291485548 * b].map((v) => v ** 3);
  const rgb = [4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s, -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s, -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s];
  return "#" + rgb.map((v) => { const g = v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055; return Math.round(Math.min(1, Math.max(0, g)) * 255).toString(16).padStart(2, "0"); }).join("");
}
/** The Mac's color picker for your own Color: one input kept outside the window's redraws (a redraw would cut the
 *  picker off from it). The window takes the color as you pick; it's saved once you're done. */
let ownInput = null;
function pickOwnColor(at) {
  if (!ownInput) {
    ownInput = Object.assign(document.createElement("input"), { type: "color" });
    ownInput.className = "own-input";
    ownInput.addEventListener("input", () => setOwnHue(...Object.values(hexToOwn(ownInput.value)), false));
    ownInput.addEventListener("change", () => { setOwnHue(...Object.values(hexToOwn(ownInput.value)), true); renderMain(); });
    document.body.append(ownInput);
  }
  const sw = SWATCHES.find(([i]) => i === allIdx());
  ownInput.value = ownToHex(ownHue() || { h: sw?.[1] ?? 122, k: sw?.[2] ?? 1 });
  // The picker opens at its input: put that just under the Your own pressed.
  const r = (at.closest(".pc-own") || at).getBoundingClientRect();
  Object.assign(ownInput.style, { left: `${r.left}px`, top: `${r.bottom}px` });
  try { ownInput.showPicker(); } catch { ownInput.click(); }
}
/** The ten swatches with your own under them: Settings → General → Color. */
const swatchPicker = () => {
  const cur = allIdx();
  return `<div class="pc-row">${SWATCHES.map(([i], n) => `<button class="pc-sw ${cur === i ? "on" : ""}" style="--c:var(--pc${i})" data-pcpick="${i}" aria-label="${i === MOSS ? "Moss" : `Swatch ${n + 1}`}"></button>`).join("")}</div>${ownRow()}`;
};
/** The window in your Color (styles/base.css .proj-theme), whichever project is picked at the top. Moss: Cue's own theme. */
const themeIdx = () => allIdx();
function projTheme() {
  const idx = themeIdx(), o = ownHue();
  const sw = idx === OWN ? [OWN, o.h, o.k] : idx !== MOSS && SWATCHES.find(([i]) => i === idx);
  const root = document.documentElement;
  // Your own Color's swatch (styles/base.css --pc10) in Settings.
  if (o) { root.style.setProperty("--oh", o.h); root.style.setProperty("--ok", o.k); }
  root.classList.toggle("proj-theme", !!sw);
  if (!sw) return;
  root.style.setProperty("--ph", sw[1]);
  root.style.setProperty("--pk", sw[2]);
}
/** A project as Cue shows it: the name you gave it, else its own (its repository's or folder's). */
const projLabel = (p) => (state.settings?.project?.names || {})[p] || p;
let pjRenaming = null;   // the project whose name is being edited in the projects list
function saveProjName(project) {
  const key = `pjname:${project}`, name = draft(key).text.trim().replace(/\s+/g, " ").slice(0, 40);
  pjRenaming = null;
  delete drafts[key];
  const desk = (state.desk?.projects || []).find((d) => d.project === project && d.independent);
  if (desk) {
    invoke("desk_rename", { id: desk.id, name }).then(() => { desk.name = name; renderMain(); }, (e) => { toast(String(e)); renderMain(); });
    renderMain();
    return;
  }
  const p = ((state.settings ||= {}).project ||= {});   // shown now; Cue's next update says the same
  const { [project]: _, ...rest } = p.names || {};
  p.names = name && name !== project ? { ...rest, [project]: name } : rest;
  setSetting("project.names", p.names);
  renderMain();
}
/** A session's folder's project, before you put it anywhere (its repository's or folder's): Cue's state
 *  says the project shown, `folder_project` the one it had. */
const folderProject = (sid) => { const r = sessionOf(sid) || state.items.find((i) => i.session_id === sid) || parkedOf(sid); return r ? state.projects?.[r.cwd]?.own ?? r.folder_project ?? r.project : ""; };
/** Put a folder in a project (+ New's Project): every session there, now and later,
 *  shows under it. Its own project again takes it out. The agent stays where it is. */
function putFolder(key, project, own) {
  const p = ((state.settings ||= {}).project ||= {});   // shown now; Cue's next update says the same
  const { [key]: _, ...rest } = p.folders || {};
  p.folders = project && project !== own ? { ...rest, [key]: project } : rest;
  setSetting("project.folders", p.folders);
}
function moveToProject(sid, project) {
  // This session alone (project.moves): its folder's other sessions stay where they are. A whole folder goes
  // in a project from + New's Project line (putFolder).
  const own = folderProject(sid);
  const p = ((state.settings ||= {}).project ||= {});   // shown now; Cue's next update says the same
  const { [sid]: _, ...rest } = p.moves || {};
  p.moves = project && project !== own ? { ...rest, [sid]: project } : rest;
  for (const r of [...state.sessions, ...state.items, ...state.history, ...(state.parked || [])]) {
    if (r.session_id !== sid) continue;
    r.folder_project ??= r.project;
    r.project = project || own;
  }
  setSetting("project.moves", p.moves);
  toast(project && project !== own ? `Moved to ${projLabel(project)}` : `Back in ${projLabel(own)}`);
}
/** A project in the list already called this (its name or the name you gave it), so a new one never quietly
 *  joins it. One that's gone from the list is free: naming it again brings it back, its history with it. */
function takenProject(name, also = []) {
  const low = name.toLowerCase();
  return [...also, ...projectsKnown().map(([p]) => p)].find((p) => p && (p.toLowerCase() === low || projLabel(p).toLowerCase() === low));
}
/** Projects you made in the projects list (+ New project…), kept while they're still empty. */
const madeList = () => (Array.isArray(state.settings?.project?.made) ? state.settings.project.made : []);
/** Projects you made (+ New project…, Move to project, a folder put in one), not a repository's or folder's own:
 *  only these can be removed, since an own one would come back with its next session. */
function madeProject(p) {
  const pr = state.settings?.project || {};
  if (![...madeList(), ...Object.values(pr.moves || {}), ...Object.values(pr.folders || {})].includes(p)) return false;
  return ![...state.sessions, ...(state.parked || [])].some((r) => folderProject(r.session_id) === p);
}
/** Remove a project you made: its sessions (and folders) go back to their own projects; the window, if it showed
 *  it, shows all of them again. */
function removeProject(p) {
  const pr = ((state.settings ||= {}).project ||= {});   // shown now; Cue's next update says the same
  const keep = (m) => Object.fromEntries(Object.entries(m || {}).filter(([, v]) => v !== p));
  pr.moves = keep(pr.moves); pr.folders = keep(pr.folders); pr.made = madeList().filter((m) => m !== p);
  for (const r of [...state.sessions, ...state.items, ...state.history, ...(state.parked || [])]) {
    if (r.project === p) r.project = state.projects?.[r.cwd]?.own ?? r.folder_project ?? r.project;
  }
  setSetting("project.moves", pr.moves);
  setSetting("project.folders", pr.folders);
  setSetting("project.made", pr.made);
  // Its pin and name go with it: a pin would keep showing a project that's gone.
  if (projPins().includes(p)) { pr.pins = projPins().filter((x) => x !== p); setSetting("project.pins", pr.pins); }
  if (pr.names && p in pr.names) { const { [p]: _, ...rest } = pr.names; pr.names = rest; setSetting("project.names", rest); }
  if (projFilter === p) setProjFilter("");
  toast(`Removed ${projLabel(p)}`);
}
/** The projects pinned beside All projects in the header (config.rs PINS_MAX), in the order pinned. */
const PINS_MAX = 3;
const projPins = () => (Array.isArray(state.settings?.project?.pins) ? state.settings.project.pins : []);
function togglePin(project) {
  const pins = projPins();
  const next = pins.includes(project) ? pins.filter((p) => p !== project) : [...pins, project];
  if (next.length > PINS_MAX) return toast(`${PINS_MAX} pinned at most: unpin one first`);
  ((state.settings ||= {}).project ||= {}).pins = next;   // shown now; Cue's next update says the same
  setSetting("project.pins", next);
}
const PIN_ICON = `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 17v5M9 3h6l-1 6 4 4v2H6v-2l4-4z"/></svg>`;
/** The window's Color, one of the ten: your own Color is put by (picked again from Your own, it starts from this one). */
function setWindowColor(idx) {
  const p = ((state.settings ||= {}).project ||= {});   // shown now; Cue's next update says the same
  hueLive = null;
  if (p.hue) { p.hue = null; setSetting("project.hue", null); }
  p.all = idx;
  setSetting("project.all", idx);
}
// ---------- one project at a time: the Projects chip in the header hides every other project's cards ----------
let projFilter = (() => { try { return localStorage.getItem("cue.projFilter") || ""; } catch { return ""; } })();
/** The window goes to project `p` ("" for All projects), remembered for the next launch. */
function setProjFilter(p) {
  if (typeof deskSelect === "function") deskSelect(null);
  projFilter = p;
  try { localStorage.setItem("cue.projFilter", p); } catch {}
}
let pjOpen = false;
/** + New project… at the foot of the projects list: its name box is open, and what it last said back. */
let pjNewOpen = false, pjNewNote = "";
/** The project whose × was clicked once: it now reads Remove, and a second click removes it. */
let pjDelArm = null;
const inProj = (project) => !projFilter || project === projFilter;
