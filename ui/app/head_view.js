// Cue UI app — the header's area after the waiting count follows the view (#666). Loaded before app/render.js.
// Board and Read (and the views not in the switch): the projects, as before. Desk: a Desks ▾
// menu first, with every Desk (each with a pin), All Desks, New Desk… and Desk settings…, then the pinned Desks as chips in
// the order pinned (a dot when that Desk waits on you), never reordered by use; a chip opens its Desk, the one open brings its lead's chat back.
// An add-on's view: what its optional head() gives, else nothing.

/** The menu open in this area: "desks", or "" for none. */
let hvMenu = "";
/** The view that menu was opened on: another view drawn means it closed (headViewHtml). */
let hvMenuView = "";
/** The Desks menu's lit row (↑ ↓ move it, Enter opens it); what's typed in its find box is the "hv-desk-find" draft. */
let hvAt = 0;
/** How many Desk chips the header shows at most. */
const HV_DESK_CHIPS = 3;

/** The area for the view shown. */
function headViewHtml() {
  if (hvMenu && hvMenuView !== view) { hvMenu = ""; delete drafts["hv-desk-find"]; }
  if (view === "desk") return headDesks();
  const v = addOn.views.find((x) => x.id === view);
  if (v) { const h = typeof v.head === "function" ? v.head() : ""; return h ? `<span class="hv-area">${h}</span>` : ""; }
  return projChip();
}

// ---------- Desk ----------

/** What Desk `d` has going: how many wait on you (its lead's own ask among them) and how many are being worked on. */
function hvDeskGoing(d) {
  const waits = deskWaitingItems(d).length + (typeof deskLeadAsk === "function" && deskLead(d) && deskLeadAsk(d) ? 1 : 0);
  const working = d.items.filter((i) => (i.state === "working" && !i.stopped) || i.state === "finishing").length;
  return { waits, working };
}
/** In a few words: "1 waits on you", "7 working", or "nothing going". */
const hvGoingText = ({ waits, working }) => waits ? `${waits} wait${waits === 1 ? "s" : ""} on you` : working ? `${working} working` : "nothing going";
/** The Desk open on the Desk view ("" on the list of Desks). */
const hvDeskNow = () => { const p = typeof deskSelection === "function" ? deskSelection() : ""; return (state.desk?.projects || []).some((d) => d.project === p) ? p : ""; };
/** The Desks pinned to the header, in the order you pinned them (kept across launches); null until you first pin or
 *  unpin one, when the chips shown then become the pins, so nothing you saw goes away. */
let hvPins = (() => { try { const v = JSON.parse(localStorage.getItem("cue.deskPins") || "null"); return Array.isArray(v) ? v.filter((x) => typeof x === "string") : null; } catch { return null; } })();
/** The Desks pinned, those still there: the first few Desks by name until you pin any. */
function hvDeskPins(all) {
  const by = new Map(all.map((d) => [d.project, d]));
  const pins = hvPins ?? [...all].sort((a, b) => deskName(a).localeCompare(deskName(b))).slice(0, HV_DESK_CHIPS).map((d) => d.project);
  return pins.filter((p) => by.has(p));
}
/** Pin Desk `p` to the header (at the end), or unpin it. */
function hvPinToggle(p) {
  const all = typeof deskSorted === "function" ? deskSorted() : [];
  const pins = hvDeskPins(all);
  hvPins = pins.includes(p) ? pins.filter((x) => x !== p) : [...pins, p];
  try { localStorage.setItem("cue.deskPins", JSON.stringify(hvPins)); } catch {}
}
/** The Desks as chips: the pinned ones, always in the order you pinned them; opening one never moves them. */
function hvDeskChips(all) {
  const by = new Map(all.map((d) => [d.project, d]));
  return hvDeskPins(all).map((p) => by.get(p));
}
/** The Desks the menu lists: those whose name holds what's typed in its find box. */
function hvDeskFound() {
  const q = (draft("hv-desk-find").text || "").trim().toLowerCase();
  return (typeof deskSorted === "function" ? deskSorted() : []).filter((d) => !q || deskName(d).toLowerCase().includes(q));
}
/** Open or close the header's menu `m`: the Desks menu opens on the Desk open, its find box empty and taking the keys. */
function hvMenuSet(m) {
  hvMenu = m;
  hvMenuView = view;
  delete drafts["hv-desk-find"];
  if (m === "desks") hvAt = Math.max(0, hvDeskFound().findIndex((d) => d.project === hvDeskNow()));
  renderMain();
  if (m === "desks") document.querySelector(".top .hv-find")?.focus();
}
function headDesks() {
  const all = typeof deskSorted === "function" ? deskSorted() : [];
  const now = hvDeskNow();
  const dot = (g) => `<i class="hv-dot${g.waits ? " ask" : ""}" aria-hidden="true"></i>`;
  const chip = (d) => {
    const g = hvDeskGoing(d), on = d.project === now;
    return `<button class="hchip hv-chip${on ? " hot" : ""}" data-hv-desk="${esc(d.project)}" aria-current="${on}"${g.waits ? ` aria-label="${esc(deskName(d))}, ${esc(hvGoingText(g))}"` : ""}>${dot(g)}<span class="hv-cn">${esc(deskName(d))}</span></button>`;
  };
  const open = hvMenu === "desks";
  const found = hvDeskFound(), at = Math.min(hvAt, found.length - 1);
  const pins = hvDeskPins(all);
  const pin = (d) => { const p = pins.includes(d.project); return `<button class="pj-pin hv-pin${p ? " on" : ""}" data-hv-pin="${esc(d.project)}" aria-label="${p ? "Unpin" : "Pin"} ${esc(deskName(d))}" aria-pressed="${p}">${PIN_ICON}</button>`; };
  const row = (d, k) => { const g = hvDeskGoing(d), on = d.project === now; return `<div class="hv-row${on ? " on" : ""}${k === at ? " at" : ""}" role="menuitem" tabindex="-1" data-hv-desk="${esc(d.project)}"><span class="hv-tick" aria-hidden="true">${on ? "✓" : ""}</span>${dot(g)}<span class="hv-rn"><span>${esc(deskName(d))}</span></span><span class="hv-rs">${esc(hvGoingText(g))}</span>${pin(d)}</div>`; };
  const find = `<input class="hv-find" data-text="hv-desk-find" placeholder="Find a Desk…" aria-label="Find a Desk" spellcheck="false" autocomplete="off" value="${esc(draft("hv-desk-find").text || "")}"/>`;
  const menu = open ? `<div class="hv-menu" role="menu">${find}${found.map(row).join("") || `<div class="hv-none">No Desk matches.</div>`}${all.length ? `<div class="hv-sep"></div>` : ""}<button class="hv-row" role="menuitem" data-hv-desk-home>All Desks</button><button class="hv-row" role="menuitem" data-hv-new-desk>New Desk…</button>${now && hvSettingsOpener() ? `<button class="hv-row" role="menuitem" data-hv-desk-set="${esc(now)}">Desk settings…</button>` : ""}${now && hvB2() ? `<button class="hv-row" role="menuitem" data-hv-desk-sessions>All sessions</button><button class="hv-row" role="menuitem" data-hv-desk-ren="${esc(now)}">Rename this Desk…</button>` : ""}</div>` : "";
  const car = `<svg class="hv-car" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 9l6 6 6-6"/></svg>`;
  // The Desks menu first, as the projects chip is on the Board: it reads the Desk open when that one isn't pinned.
  const loose = now && !pins.includes(now) ? all.find((d) => d.project === now) : null;
  const main = `<span class="hv-wrap"><button class="hchip hv-chip hv-open${loose ? " hot" : ""}" data-hv-menu="desks" aria-label="Desks${loose ? `, ${esc(deskName(loose))}` : ""}" aria-expanded="${open}">${loose ? `${dot(hvDeskGoing(loose))}<span class="hv-cn">${esc(deskName(loose))}</span>` : "Desks"}${car}</button>${menu}</span>`;
  return `${main}${hvDeskChips(all).map(chip).join("")}`;
}

/** Today's Desk page (B2, desk/b2.js), whose All sessions list holds the Desk's name to rename: reached from here too,
 *  so a Desk with no lead (no strip over its chat) still has them. */
const hvB2 = () => typeof deskB2On === "function" && deskB2On();
/** What opens a Desk's settings: the Desk page's own (deskSettingsOpen), else the drawer of the v2 page. */
const hvSettingsOpener = () => (typeof deskSettingsOpen === "function" ? deskSettingsOpen : typeof deskV2Open === "function" ? deskV2Open : null);

// ---------- clicks ----------

/** A click in the area (a rule for app/clicks.js's list): true when it took it. An open menu closes on a click
 *  anywhere else. */
function headViewClick(t) {
  // Closed after this click's own handling (a microtask): the element clicked stays the one on the page while the rule
  // that takes it acts, instead of a copy a redraw just took out.
  if (hvMenu && !t.closest(".hv-wrap")) { hvMenu = ""; delete drafts["hv-desk-find"]; queueMicrotask(renderMain); }
  // A Desk's pin in the menu: pinned to the header, or not; the menu stays open.
  const pinEl = t.closest("[data-hv-pin]");
  if (pinEl && pinEl.closest(".top")) { hvPinToggle(pinEl.dataset.hvPin); renderMain(); return true; }
  const el = t.closest("[data-hv-menu], [data-hv-desk], [data-hv-desk-set], [data-hv-desk-home], [data-hv-new-desk], [data-hv-desk-sessions], [data-hv-desk-ren]");
  if (!el || !el.closest(".top")) return false;
  const d = el.dataset;
  if (d.hvMenu) { hvMenuSet(hvMenu === d.hvMenu ? "" : d.hvMenu); return true; }
  hvMenu = "";
  delete drafts["hv-desk-find"];
  // All Desks: the page of every Desk (desk/home.js), as the Desk switcher's foot had it before B2.
  if ("hvDeskHome" in d) { DESK_CLICKS.deskHome?.(); renderMain(); return true; }
  if ("hvNewDesk" in d) { renderMain(); if (typeof deskNewOpen === "function") deskNewOpen(); return true; }
  if (d.hvDeskSet) { renderMain(); hvSettingsOpener()?.(d.hvDeskSet); return true; }
  // All sessions on the right, as the strip's All N sessions opens it; Rename: there, its name in a box.
  if ("hvDeskSessions" in d || d.hvDeskRen) {
    if (deskOpen?.kind !== "sessions") DESK_CLICKS.desk2bSessions?.();
    if (d.hvDeskRen) renameProject({ dataset: { pjren: d.hvDeskRen } });
    else renderMain();
    return true;
  }
  // The Desk open: its lead's chat back in front (whatever item or chat was open); another: that Desk opens.
  if (d.hvDesk === hvDeskNow()) DESK_CLICKS.deskLeadChat?.(el);
  else DESK_CLICKS.deskGo?.({ dataset: { deskGo: d.hvDesk } });
  renderMain();
  return true;
}
/** Esc with one of the area's menus open closes it, before anything else sees the key. */
function headViewKey(e) {
  if (!hvMenu || e.metaKey || e.ctrlKey || e.altKey) return false;
  // Its keys only while it's drawn: a menu another view left behind takes none (Enter must still send).
  if (hvMenuView !== view || !document.querySelector(".top .hv-menu")) { hvMenu = ""; delete drafts["hv-desk-find"]; return false; }
  if (e.key === "Escape") {
    const m = hvMenu;
    hvMenuSet("");
    document.querySelector(`.top [data-hv-menu="${m}"]`)?.focus();   // the keyboard back on the chip that opened it
    return true;
  }
  // The Desks menu: ↑ ↓ move the lit row, Enter opens its Desk, as its chip's click does.
  if (hvMenu !== "desks") return false;
  const n = hvDeskFound().length;
  if (e.key === "ArrowDown" || e.key === "ArrowUp") { if (n) hvAt = (Math.min(hvAt, n - 1) + (e.key === "ArrowDown" ? 1 : n - 1)) % n; renderMain(); return true; }
  if (e.key === "Enter") {
    const d = hvDeskFound()[Math.min(hvAt, n - 1)];
    const row = d && document.querySelector(`.top .hv-menu [data-hv-desk="${CSS.escape(d.project)}"]`);
    if (row) headViewClick(row);
    return true;
  }
  return false;
}
