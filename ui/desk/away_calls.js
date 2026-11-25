// Cue UI desk — Calls while away (#863, the #835 mocks' option B2): when Away goes off, one "While you were away" card
// in the lead's chat, under an "Away off" line: when, what moved, steps held, spend, then the calls made while you
// were away, each closed with Keep, Undo ("What should happen instead?", Send back) or Got it, and Keep all. The
// calls you leave are kept when you next write to the lead (desk/hub/away/away_log.rs desk_calls_keep_left). The Away
// tab (desk/away.js deskAwayLog) has every time away, with a search box and filters (F1). Loaded after desk/away.js.

/** The call whose Undo box is open ("What should happen instead?"), by its id; null when none. */
let deskAwcUndoOpen = null;
/** A call's state: "kept", "undone" (sent back to whoever made it), or "open". */
const deskAwcState = (c) => (c.undo === "asked" ? "undone" : c.kept ? "kept" : "open");
/** Who made call `c`: its maker as the Desk's decisions say, else the lead. */
const deskAwcBy = (c, d) => (d.decisions || []).find((x) => x.id === c.id)?.by || deskLeadName(d) || "the lead";
/** `text` escaped, with each of `words` marked where it shows (a search's match). */
function deskAwcMark(text, words) {
  const t = String(text ?? "");
  const ws = (words || []).filter(Boolean);
  if (!ws.length) return esc(t);
  const re = new RegExp(`(${ws.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|")})`, "gi");
  return t.split(re).map((part, n) => (n % 2 ? `<mark class="dk-awf-hl">${esc(part)}</mark>` : esc(part))).join("");
}

/** One call, in the card or the Away tab: its time, what was decided, its item and why (and the question it stood in
 *  for), then how it's closed: Kept, Sent back, or Undo and Keep (Got it when it can't be undone). Its Undo box
 *  under it, open. `words`: a search's words, marked. */
function deskAwcRow(c, d, words = []) {
  const st = deskAwcState(c), by = deskAwcBy(c, d);
  const it = c.item_id ? deskItem(d.project, c.item_id) : null;
  const item = c.item_id ? `#${c.item_id}` : "";
  const done = it && (it.state === "done" || it.state === "dropped") ? (it.state === "done" ? "done since" : "dropped since") : "";
  const why = [item, c.why ? deskOneLine(c.why) : "", done].filter(Boolean).join(" · ");
  const asked = c.asked ? `<span class="dk-aw-asked">Instead of asking you: ${deskAwcMark(deskOneLine(c.asked), words)}</span>` : "";
  const acts = st === "kept" ? `<span class="dk-awc-u">Kept</span>`
    : st === "undone" ? `<span class="dk-awc-u">Sent back · ${esc(by)} has it</span>`
    : c.undo === "can" ? `<span class="dk-awc-acts"><button class="btn small dk-awc-q" data-desk-awc-undo="${esc(c.id)}" aria-expanded="${deskAwcUndoOpen === c.id}">Undo</button><button class="btn small" data-desk-awc-keep="${esc(c.id)}">Keep</button></span>`
    : `<span class="dk-awc-acts"><button class="btn small" data-desk-awc-keep="${esc(c.id)}">Got it</button></span>`;
  const row = `<div class="dk-awc-mark${st === "open" ? "" : " leave"}" data-desk-call="${esc(c.id)}"><span class="dk-aw-tm">${c.at ? esc(deskHm(deskMs(c.at))) : ""}</span>`
    + `<span class="dk-aw-mt"><b>${deskAwcMark(c.text, words)}</b>${why ? `<span class="dk-aw-why"> · ${deskAwcMark(why, words)}</span>` : ""}${asked}</span>${acts}</div>`;
  return row + (deskAwcUndoOpen === c.id && st === "open" ? deskAwcUndoBox(c, by) : "");
}
/** A call's Undo, open: "What should happen instead?" (optional), who it goes to, Cancel and Send back. */
function deskAwcUndoBox(c, by) {
  const key = `awundo:${c.id}`;
  return `<div class="dk-awc-ubox"><label class="dk-awc-lb" for="awu-${esc(c.id)}">What should happen instead?</label>`
    + `<textarea id="awu-${esc(c.id)}" class="dk-awc-ta" data-text="${esc(key)}" rows="2" spellcheck="true" placeholder="Optional">${esc(draft(key).text || "")}</textarea>`
    + `<div class="dk-awc-bt"><span class="grow">Goes to ${esc(by)} with its call</span><button class="btn small dk-awc-q" data-desk-awc-undo="">Cancel</button>`
    + `<button class="btn small primary" data-desk-awc-send="${esc(c.id)}">Send back</button></div></div>`;
}

/** The "While you were away" card for stretch `r` (desk/hub/away/away_log.rs log_view's), under its "Away off" line. */
function deskAwcCard(r, d) {
  const calls = r.calls || [], items = r.items || [], held = r.held || [];
  const facts = [
    items.length ? `<span><b>${items.length}</b> item${items.length === 1 ? "" : "s"} moved</span>` : "",
    held.length ? `<span><b>${held.length}</b> step${held.length === 1 ? "" : "s"} held for you</span>` : "",
    r.spent > 0 || r.budget != null ? `<span><b>${esc(deskMoney(r.spent))}</b>${r.budget != null ? ` of ${esc(deskMoney(r.budget))}` : ""}</span>` : "",
  ].filter(Boolean);
  const bys = [...new Set(calls.map((c) => deskAwcBy(c, d)))];
  const sec = calls.length ? `<div class="dk-awc-sec">${esc(`${bys.length === 1 ? `${bys[0]}’s calls` : "Calls"} · ${calls.length}`.toUpperCase())}</div>` : "";
  const left = calls.filter((c) => deskAwcState(c) === "open").length;
  const foot = `<div class="dk-awc-f"><button class="dk-awc-lnk" data-desk-awc-tab="${esc(r.id)}" data-project="${esc(d.project)}">Everything in the Away tab ›</button><span class="grow"></span>`
    + `${left ? `<span>${left} left</span><button class="btn small" data-desk-awc-keep-all="${esc(r.id)}" data-project="${esc(d.project)}">Keep all</button>` : ""}</div>`;
  return `<div class="dk-awc-wrap"><div class="dk-awc-cl"><i></i><span>Away off · ${esc(deskHm(r.end))}</span><i></i></div>`
    + `<div class="dk-awc"><div class="dk-awc-h">${DESK_AWAY_ICON}<b>While you were away</b><span>${esc(deskAwayRan(r.since, r.end))} · ${esc(deskAwayLong(r.end - r.since))}</span></div>`
    + `${facts.length ? `<div class="dk-awc-facts">${facts.join("")}</div>` : ""}${sec}${calls.map((c) => deskAwcRow(c, d)).join("")}${foot}</div></div>`;
}
/** The cards for the lead's chat (desk/page.js deskChat's extra rows): each time away in the last week (the newest
 *  three), at the time Away went off. */
function deskAwcRows(d) {
  const week = now() - 7 * 864e5;
  return (d.away_log || []).filter((r) => r.end >= week).slice(0, 3).map((r) => ({ at: r.end, html: keyed(deskAwcCard(r, d), `awc:${r.id}`) }));
}

/** Calls `ids` kept: closed here at once, then in Cue. */
async function deskAwcKeep(ids) {
  if (!ids.length) return;
  for (const d of state.desk?.projects || []) {
    for (const x of d.decisions || []) if (ids.includes(x.id)) x.kept = true;
    for (const r of d.away_log || []) for (const c of r.calls || []) if (ids.includes(c.id)) c.kept = true;
  }
  if (ids.includes(deskAwcUndoOpen)) deskAwcUndoOpen = null;
  renderMain();
  try { await invoke("desk_calls_keep", { ids }); } catch (e) { toast(`Couldn't keep it: ${e}`); }
}
/** Undo's Send back: whoever made call `id` is asked to go back on it, with what you wrote instead. */
async function deskAwcSend(id) {
  const key = `awundo:${id}`, answer = (draft(key).text || "").trim();
  deskAwcUndoOpen = null;
  try {
    toast((await invoke("desk_undo", { decisionId: id, answer: answer || null })) || "Sent back");
    draft(key).text = "";
    saveDrafts();
    for (const d of state.desk?.projects || []) {
      for (const x of d.decisions || []) if (x.id === id) x.undo = "asked";
      for (const r of d.away_log || []) for (const c of r.calls || []) if (c.id === id) c.undo = "asked";
    }
  } catch (e) { toast(String(e)); }
  renderMain();
}
/** The open calls of stretch `rid` on Desk `project`. */
const deskAwcLeft = (project, rid) => ((deskOf(project)?.away_log || []).find((r) => r.id === rid)?.calls || []).filter((c) => deskAwcState(c) === "open").map((c) => c.id);

// ---------- The Away tab's search and filters (#863, the #835 mocks' F1) ----------

/** By Desk: { chip: "" (All) | "calls" | "open" | "kept" | "undone" | "moved" | "held", item: an item's id or "",
 *  kind: "" | "product" | "minor", days: "" | "7" | "30" }. The search's words are its box's draft. Kept in this window. */
const deskAwfFilters = new Map();
const deskAwfKey = (project) => `dk-away-find:${project}`;
const DESK_AWF_CHIPS = [["", "All"], ["calls", "Calls"], ["open", "Still open"], ["kept", "Kept"], ["undone", "Undone"], ["moved", "Items moved"], ["held", "Held steps"]];
/** Desk `d`'s search and filters now: { f, words, on (any set) }. */
function deskAwfNow(d) {
  const f = deskAwfFilters.get(d.project) || {};
  const words = (draft(deskAwfKey(d.project)).text || "").trim().toLowerCase().split(/\s+/).filter(Boolean);
  return { f, words, on: !!(words.length || f.chip || f.item || f.kind || f.days) };
}
/** Stretch `r`'s rows that the search and filters keep (all of them when none are set), but for the chip when
 *  `anyChip`: { calls, items, held }. */
function deskAwfRows(r, d, { f, words }, anyChip = false) {
  const has = (txt) => words.every((w) => txt.toLowerCase().includes(w) || txt.toLowerCase().includes(w.replace(/^#/, "")));
  const chip = anyChip ? "" : f.chip || "";
  const callsOn = !chip || ["calls", "open", "kept", "undone"].includes(chip);
  const calls = !callsOn ? [] : (r.calls || []).filter((c) => {
    if (["open", "kept", "undone"].includes(chip) && deskAwcState(c) !== chip) return false;
    if (f.kind && (f.kind === "minor") !== !!c.minor) return false;
    if (f.item && c.item_id !== f.item && !String(c.text || "").includes(`#${f.item}`)) return false;
    return has([c.text, c.why, c.asked, c.item_id ? `#${c.item_id}` : "", deskAwcBy(c, d)].join(" "));
  });
  const items = (chip && chip !== "moved") || f.kind ? [] : (r.items || []).filter((i) => (!f.item || i.id === f.item)
    && has([`#${i.id}`, i.title, deskStateWord(i.from), deskStateWord(i.to)].join(" ")));
  const held = (chip && chip !== "held") || f.kind || f.item ? [] : (r.held || []).filter((h) => has([h.who, h.what, h.command].join(" ")));
  return { calls, items, held };
}
/** The stretches the time menu keeps. */
const deskAwfInTime = (log, f) => (f.days ? log.filter((r) => r.end >= now() - Number(f.days) * 864e5) : log);
/** The Away tab's head: the search box, the chips (with how many each has), then Item, Kind of call and time. */
function deskAwfBar(d, log) {
  const cur = deskAwfNow(d), { f } = cur, key = deskAwfKey(d.project);
  const kept = deskAwfInTime(log, f).map((r) => deskAwfRows(r, d, cur, true));
  const calls = kept.flatMap((x) => x.calls);
  const n = { calls: calls.length, open: calls.filter((c) => deskAwcState(c) === "open").length, kept: calls.filter((c) => deskAwcState(c) === "kept").length,
    undone: calls.filter((c) => deskAwcState(c) === "undone").length, moved: kept.reduce((s, x) => s + x.items.length, 0), held: kept.reduce((s, x) => s + x.held.length, 0) };
  const chips = DESK_AWF_CHIPS.map(([k, label]) => `<button class="dk-awf-chip${(f.chip || "") === k ? " on" : ""}" data-desk-awf-chip="${k}" data-project="${esc(d.project)}" aria-pressed="${(f.chip || "") === k}">${esc(label)}${k ? `<span>${n[k]}</span>` : ""}</button>`).join("");
  const opt = (v, label, on) => `<option value="${esc(v)}"${on ? " selected" : ""}>${esc(label)}</option>`;
  const ids = [...new Set(log.flatMap((r) => [...(r.calls || []).map((c) => c.item_id), ...(r.items || []).map((i) => i.id)]).filter(Boolean))]
    .sort((a, b) => (parseInt(b, 10) || 0) - (parseInt(a, 10) || 0) || (a < b ? 1 : -1));
  if (f.item && !ids.includes(f.item)) ids.unshift(f.item);
  const itemLabel = (id) => { const t = deskItem(d.project, id)?.title || log.flatMap((r) => r.items || []).find((i) => i.id === id)?.title || ""; return `#${id}${t ? ` ${t.length > 40 ? `${t.slice(0, 39)}…` : t}` : ""}`; };
  const sel = (k, label, opts) => `<select class="dk-mv-sel" data-dk-awf-f="${k}" data-project="${esc(d.project)}" aria-label="${esc(label)}">${opts}</select>`;
  const menus = sel("item", "Every item", opt("", "Every item", !f.item) + ids.map((id) => opt(id, itemLabel(id), f.item === id)).join(""))
    + sel("kind", "Every kind of call", opt("", "Every kind of call", !f.kind) + opt("product", "Product calls", f.kind === "product") + opt("minor", "Housekeeping calls", f.kind === "minor"))
    + sel("days", "Any time", opt("", "Any time", !f.days) + opt("7", "Last 7 days", f.days === "7") + opt("30", "Last 30 days", f.days === "30"));
  const clear = cur.on ? `<button class="btn small dk-awc-q" data-desk-awf-clear="${esc(d.project)}">Clear</button>` : "";
  return `<div class="dk-awf"><div class="dk-awf-row"><input class="dk-mv-q" type="search" data-text="${esc(key)}" placeholder="Search: an item number, any word, a session's name" spellcheck="false" aria-label="Search the Away tab" value="${esc(draft(key).text || "")}"/>${clear}</div>`
    + `<div class="dk-awf-chips">${chips}</div><div class="dk-awf-row">${menus}</div></div>`;
}
/** A stretch while searching or filtering: opened, only its rows that match, the words marked; null when none match. */
function deskAwfStretch(r, d, cur) {
  const { calls, items, held } = deskAwfRows(r, d, cur);
  const total = calls.length + items.length + held.length;
  if (!total) return null;
  const all = (r.calls || []).length + (r.items || []).length + (r.held || []).length;
  const w = cur.words;
  const facts = total === all ? [deskAwayLong(r.end - r.since), r.spent > 0 ? deskMoney(r.spent) : "", items.length ? `${items.length} moved` : "", calls.length ? deskCount(calls.length, "call") : "", held.length ? `${held.length} held` : ""].filter(Boolean).join(" · ")
    : `${total} of ${all} match`;
  const head = `<div class="dk-aw-fold dk-al-h"><span class="dk-aw-chev open" aria-hidden="true">›</span>${DESK_AWAY_ICON}<span class="dk-al-tx"><b>${esc(deskAwayRan(r.since, r.end))}</b> <span class="dk-aw-w">${esc(facts)}</span></span></div>`;
  const sec = (t, rows) => (rows ? `<div class="dk-al-s">${esc(t)}</div>${rows}` : "");
  const moved = items.map((i) => {
    const what = `<b class="dk-inum">${deskAwcMark(`#${i.id}`, w)}</b> <span class="dk-al-t">${deskAwcMark(i.title, w)}</span><span class="dk-aw-why"> · ${deskAwcMark(i.from ? `${deskStateWord(i.from)} → ${deskStateWord(i.to)}` : `added · ${deskStateWord(i.to)}`, w)}</span>`;
    return deskItem(d.project, i.id) ? `<button class="dk-aw-mark dk-al-row" data-desk-chk-item="${esc(i.id)}"><span class="dk-aw-mt">${what}</span></button>` : `<div class="dk-aw-mark dk-al-row"><span class="dk-aw-mt">${what}</span></div>`;
  }).join("");
  const answer = (h) => (h.answer === "allow" ? "Allowed" : h.answer === "no" ? "Refused" : "Not answered yet: under Waiting on you");
  const heldRows = held.map((h) => `<div class="dk-aw-mark dk-al-held"><span class="dk-aw-tm">${esc(deskHm(deskMs(h.at)))}</span><span class="dk-aw-mt"><b>${deskAwcMark(`${h.who || "A session"} wanted to ${h.what}`, w)}</b><span class="dk-aw-why"> · ${esc(answer(h))}</span><code class="dk-al-cmd">${deskAwcMark(h.command, w)}</code></span></div>`).join("");
  return { n: { calls: calls.length, items: items.length, held: held.length },
    html: `<div class="dk-al open dk-awf-hit" data-key="alf:${esc(r.id)}">${head}<div class="dk-aw-list">${sec("Calls", calls.map((c) => deskAwcRow(c, d, w)).join(""))}${sec("Items", moved)}${sec("Held for you", heldRows)}</div></div>` };
}
/** The Away tab's list while searching or filtering: "4 calls in 2 of 4 times away", then the stretches that match. */
function deskAwfList(d, log) {
  const cur = deskAwfNow(d), inTime = deskAwfInTime(log, cur.f);
  const hits = inTime.map((r) => deskAwfStretch(r, d, cur)).filter(Boolean);
  if (!hits.length) return `<div class="dk-empty">Nothing here fits that search.</div>`;
  const sum = (k) => hits.reduce((s, h) => s + h.n[k], 0);
  const what = [sum("calls") ? deskCount(sum("calls"), "call") : "", sum("items") ? `${sum("items")} moved` : "", sum("held") ? `${sum("held")} held` : ""].filter(Boolean).join(", ");
  return `<div class="dk-awf-res">${esc(what)} in ${hits.length} of ${deskCount(log.length, "time")} away</div>${hits.map((h) => h.html).join("")}`;
}
/** A filter picked (onInput): the list drawn again with it. */
function deskAwfSet(el) {
  const p = el.dataset.project, f = { ...(deskAwfFilters.get(p) || {}) };
  f[el.dataset.dkAwfF] = el.value;
  deskAwfFilters.set(p, f);
  renderMain();
}

deskClicks({
  deskAwcUndo: (el) => { const id = el.dataset.deskAwcUndo; deskAwcUndoOpen = id && deskAwcUndoOpen !== id ? id : null; renderMain(); },
  deskAwcKeep: (el) => deskAwcKeep([el.dataset.deskAwcKeep]),
  deskAwcKeepAll: (el) => deskAwcKeep(deskAwcLeft(el.dataset.project, el.dataset.deskAwcKeepAll)),
  deskAwcSend: (el) => deskAwcSend(el.dataset.deskAwcSend),
  // Everything in the Away tab: that tab, this time away open in it, no search or filter hiding it.
  deskAwcTab: (el) => {
    const p = el.dataset.project;
    if (view !== "desk" && DESK_CLICKS.deskGo) DESK_CLICKS.deskGo({ dataset: { deskGo: p } });
    deskAwfFilters.delete(p);
    draft(deskAwfKey(p)).text = "";
    deskAwayLogOpen.add(el.dataset.deskAwcTab);
    deskTab = "away";
    renderMain();
  },
  deskAwfChip: (el) => { const p = el.dataset.project; deskAwfFilters.set(p, { ...(deskAwfFilters.get(p) || {}), chip: el.dataset.deskAwfChip }); renderMain(); },
  deskAwfClear: (el) => { const p = el.dataset.deskAwfClear; deskAwfFilters.delete(p); draft(deskAwfKey(p)).text = ""; saveDrafts(); renderMain(); },
});
